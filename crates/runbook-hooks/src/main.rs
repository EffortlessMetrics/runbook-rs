use std::io::Read;

use clap::Parser;
use serde_json::Value;

/// Claude Code hook consumer.
///
/// Claude Code runs this binary with hook payload JSON on stdin.
/// We forward the event to runbookd over localhost and (optionally) emit hook output JSON to stdout
/// (e.g., to block a tool call).
#[derive(Debug, Parser)]
#[command(name = "runbook-hooks", about = "Runbook hook consumer for Claude Code")]
struct Args {
    /// Hook name, e.g. PreToolUse, UserPromptSubmit, Notification
    hook: String,

    /// Optional matcher (e.g. permission_prompt)
    matcher: Option<String>,

    /// Daemon base URL (runbookd)
    #[arg(long, default_value = "http://127.0.0.1:29381")]
    daemon: String,

    /// If set, deny obviously destructive Bash commands at PreToolUse.
    #[arg(long)]
    deny_destructive_bash: bool,
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

    // Forward event to daemon (best-effort).
    forward_to_daemon(&args, &payload);

    // Hook outputs (stdout) are only needed for specific hooks.
    if args.hook == "PreToolUse" && args.deny_destructive_bash {
        if let Some(cmd) = extract_bash_command(&payload) {
            if looks_destructive(&cmd) {
                // Claude Code docs: PreToolUse can block tool calls if stdout contains
                // {"decision":"block"} or by exiting non-zero.
                // We also include hookSpecificOutput.permissionDecision for forward-compat.
                //
                // IMPORTANT: This is intentionally conservative; tune with config.
                let out = serde_json::json!({
                    "decision": "block",
                    "reason": format!("Blocked destructive Bash command: {cmd}"),
                    "hookSpecificOutput": {
                        "permissionDecision": "deny",
                        "denyReason": format!("Blocked destructive Bash command: {cmd}"),
                    }
                });
                println!("{}", out);
                return Ok(());
            }
        }
    }

    if args.hook == "UserPromptSubmit" {
        // Optionally inject extra context. Keep it small and factual.
        // (If you don't want this, remove this block and keep context in the prompt template.)
        let branch = git_branch().unwrap_or_else(|| "(unknown)".to_string());
        let out = serde_json::json!({
            "hookSpecificOutput": {
                "additionalContext": format!("Runbook context: git_branch={branch}"),
            }
        });
        println!("{}", out);
    }

    Ok(())
}

fn forward_to_daemon(args: &Args, payload: &Value) {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(250))
        .build();

    let Ok(client) = client else { return };

    let ev = runbook_protocol::HookEvent {
        hook: args.hook.clone(),
        matcher: args.matcher.clone(),
        payload: payload.clone(),
    };

    let url = format!("{}/hook", args.daemon.trim_end_matches('/'));
    let _ = client.post(url).json(&ev).send();
}

fn extract_bash_command(payload: &Value) -> Option<String> {
    // Claude Code hook payload for PreToolUse includes tool_input.command for Bash.
    // We intentionally treat the payload as opaque JSON; this extractor is best-effort.
    payload
        .get("tool_input")
        .and_then(|v| v.get("command"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn looks_destructive(cmd: &str) -> bool {
    let c = cmd.to_lowercase();

    // Very conservative first-pass rules.
    // You should replace with a real parser + allowlist.
    c.contains("rm -rf")
        || c.contains(" rm -r ")
        || c.starts_with("rm ")
        || c.contains("mkfs")
        || c.contains("dd if=")
        || c.contains("shutdown")
        || c.contains("reboot")
        || c.contains("sudo ")
        || c.contains("git push")
        || c.contains("git reset --hard")
}

fn git_branch() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

