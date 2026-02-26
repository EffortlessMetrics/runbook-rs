use std::io::Read;

use clap::Parser;
use serde_json::Value;

use runbook_protocol::{HookEvent, UserPromptSubmitOutput};

/// Claude Code hook consumer.
///
/// Claude Code runs this binary with hook payload JSON on stdin.
/// We forward the event to runbookd over localhost and (optionally) emit
/// hook-specific output JSON to stdout (e.g., to block a tool call).
#[derive(Debug, Parser)]
#[command(name = "runbook-hooks", about = "Runbook hook consumer for Claude Code")]
struct Args {
    /// Hook name, e.g. PreToolUse, UserPromptSubmit, Notification
    hook: String,

    /// Optional matcher (e.g. permission_prompt, Bash)
    matcher: Option<String>,

    /// Daemon base URL (runbookd)
    #[arg(long, default_value = "http://127.0.0.1:29381")]
    daemon: String,

    /// If set, deny destructive Bash commands at PreToolUse.
    /// In production, prefer policy.pre_tool_use.bash.deny in runbook.yaml.
    #[arg(long)]
    deny_destructive_bash: bool,

    /// Comma-separated list of additional deny patterns (supplements the built-in list).
    #[arg(long, value_delimiter = ',')]
    deny_patterns: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Read stdin JSON (Claude Code hook payload).
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    let payload: Value = if buf.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&buf)?
    };

    // Extract session_id from the hook input (Claude Code includes it in every payload).
    let session_id = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Extract session_tag from process environment (set by VS Code extension when
    // launching Claude terminals via "Start Claude Session").
    let session_tag = std::env::var("RUNBOOK_SESSION_TAG").ok();

    // Forward event to daemon (best-effort, fire-and-forget).
    forward_to_daemon(&args, &payload, session_id.as_deref(), session_tag.as_deref());

    // --- Hook-specific enforcement ---

    if args.hook == "PreToolUse" && args.deny_destructive_bash {
        if let Some(ref cmd) = extract_bash_command(&payload) {
            let deny_patterns = built_in_deny_patterns();
            let extra = &args.deny_patterns;

            if matches_any_pattern(cmd, &deny_patterns)
                || matches_any_pattern(cmd, extra)
            {
                // Notify the daemon that we blocked something (UI signal).
                notify_daemon_blocked(&args, session_id.as_deref(), session_tag.as_deref(), cmd);

                // Exit-code enforcement: exit 2 blocks the tool call.
                // This is more reliable than JSON stdout (upstream issues #10875, #18312).
                eprintln!("Blocked by Runbook policy: {cmd}");
                std::process::exit(2);
            }
        }
    }

    if args.hook == "UserPromptSubmit" {
        // Inject git branch as additional context.
        let branch = git_branch().unwrap_or_else(|| "(unknown)".to_string());
        let out = UserPromptSubmitOutput::with_context(&format!(
            "Runbook context: git_branch={branch}"
        ));
        println!("{}", serde_json::to_string(&out)?);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Daemon forwarding
// ---------------------------------------------------------------------------

fn forward_to_daemon(args: &Args, payload: &Value, session_id: Option<&str>, session_tag: Option<&str>) {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(250))
        .build();

    let Ok(client) = client else { return };

    let ev = HookEvent {
        hook: args.hook.clone(),
        matcher: args.matcher.clone(),
        session_id: session_id.map(|s| s.to_string()),
        session_tag: session_tag.map(|s| s.to_string()),
        payload: payload.clone(),
    };

    let url = format!("{}/hook", args.daemon.trim_end_matches('/'));
    let _ = client.post(url).json(&ev).send();
}

/// Notify the daemon that we blocked a tool call via our policy.
/// This is our own truth signal ("RunbookPolicy/blocked"), NOT a Claude lifecycle event.
fn notify_daemon_blocked(args: &Args, session_id: Option<&str>, session_tag: Option<&str>, command: &str) {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(250))
        .build();

    let Ok(client) = client else { return };

    let ev = HookEvent {
        hook: "RunbookPolicy".to_string(),
        matcher: Some("blocked".to_string()),
        session_id: session_id.map(|s| s.to_string()),
        session_tag: session_tag.map(|s| s.to_string()),
        payload: serde_json::json!({
            "runbook_policy": {
                "name": "deny_destructive_bash",
                "command": command,
            }
        }),
    };

    let url = format!("{}/hook", args.daemon.trim_end_matches('/'));
    let _ = client.post(url).json(&ev).send();
}

// ---------------------------------------------------------------------------
// Bash command analysis
// ---------------------------------------------------------------------------

fn extract_bash_command(payload: &Value) -> Option<String> {
    // Claude Code hook payload for PreToolUse includes tool_input.command for Bash.
    payload
        .get("tool_input")
        .and_then(|v| v.get("command"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn built_in_deny_patterns() -> Vec<String> {
    vec![
        "rm -rf".to_string(),
        "rm -r ".to_string(),
        "mkfs".to_string(),
        "dd if=".to_string(),
        "shutdown".to_string(),
        "reboot".to_string(),
        "git push --force".to_string(),
        "git push -f".to_string(),
        "git reset --hard".to_string(),
    ]
}

fn matches_any_pattern(cmd: &str, patterns: &[String]) -> bool {
    let lower = cmd.to_lowercase();
    patterns.iter().any(|p| lower.contains(&p.to_lowercase()))
}

// ---------------------------------------------------------------------------
// Git context
// ---------------------------------------------------------------------------

fn git_branch() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}
