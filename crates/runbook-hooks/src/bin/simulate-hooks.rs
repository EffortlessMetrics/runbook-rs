use std::time::Duration;

use clap::Parser;
use runbook_protocol::HookEvent;
use serde_json::Value;

/// Simulate a Claude Code hook lifecycle against a running runbookd.
#[derive(Debug, Parser)]
#[command(
    name = "simulate-hooks",
    about = "Send a demo Claude Code hook lifecycle to runbookd"
)]
struct Args {
    /// Daemon base URL (runbookd).
    #[arg(
        long,
        env = "DAEMON_BASE_URL",
        default_value = "http://127.0.0.1:29381"
    )]
    daemon: String,

    /// Session ID to attach to simulated hook events.
    #[arg(long, default_value = "sess-demo-001")]
    session_id: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let simulator = HookSimulator::new(args)?;

    println!("Simulating Claude Code lifecycle events…");
    println!();

    println!("--- Session start ---");
    simulator.post("SessionStart", None, serde_json::json!({}))?;
    pause(500);

    println!("--- Idle (waiting for prompt) ---");
    simulator.post("Notification", Some("idle_prompt"), serde_json::json!({}))?;
    pause(1_000);

    println!("--- User submits prompt ---");
    simulator.post(
        "UserPromptSubmit",
        None,
        serde_json::json!({ "prompt": "/runbook:prep-pr" }),
    )?;
    pause(1_000);

    println!("--- Permission prompt (agent needs approval) ---");
    simulator.post(
        "Notification",
        Some("permission_prompt"),
        serde_json::json!({ "reason": "needs file write permission" }),
    )?;
    pause(1_500);

    println!("--- Back to running ---");
    simulator.post("Notification", Some("idle_prompt"), serde_json::json!({}))?;
    pause(500);
    simulator.post(
        "UserPromptSubmit",
        None,
        serde_json::json!({ "prompt": "continue" }),
    )?;
    pause(1_000);

    println!("--- Task completed ---");
    simulator.post("TaskCompleted", None, serde_json::json!({ "result": "ok" }))?;
    pause(500);

    println!("--- Stop ---");
    simulator.post("Stop", None, serde_json::json!({}))?;
    pause(500);

    println!("--- Session end ---");
    simulator.post("SessionEnd", None, serde_json::json!({}))?;

    println!();
    println!("✓ Simulation complete");

    Ok(())
}

struct HookSimulator {
    client: reqwest::blocking::Client,
    url: String,
    session_id: String,
}

impl HookSimulator {
    fn new(args: Args) -> anyhow::Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()?;
        let url = format!("{}/hook", args.daemon.trim_end_matches('/'));

        Ok(Self {
            client,
            url,
            session_id: args.session_id,
        })
    }

    fn post(&self, hook: &str, matcher: Option<&str>, payload: Value) -> anyhow::Result<()> {
        println!(
            "→ {hook}{}",
            matcher.map(|m| format!("/{m}")).unwrap_or_default()
        );

        let event = HookEvent {
            hook: hook.to_string(),
            matcher: matcher.map(str::to_string),
            session_id: Some(self.session_id.clone()),
            session_tag: None,
            payload,
        };

        self.client
            .post(&self.url)
            .json(&event)
            .send()?
            .error_for_status()?;

        Ok(())
    }
}

fn pause(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}
