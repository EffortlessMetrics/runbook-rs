use std::{thread, time::Duration};

use clap::Parser;
use reqwest::blocking::Client;
use runbook_protocol::HookEvent;
use serde_json::{json, Value};

/// Simulate a Claude Code hook lifecycle against runbookd.
#[derive(Debug, Parser)]
#[command(
    name = "simulate-hooks",
    about = "Simulate Claude Code lifecycle events against runbookd"
)]
struct Args {
    /// Daemon base URL. Defaults to DAEMON_BASE_URL or localhost runbookd.
    #[arg(long)]
    daemon: Option<String>,

    /// Session ID to attach to each simulated hook event.
    #[arg(long, default_value = "sess-demo-001")]
    session_id: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let simulator = Simulator::new(args)?;

    println!("Simulating Claude Code lifecycle events…");
    println!();

    println!("--- Session start ---");
    simulator.post("SessionStart", None, json!({}))?;
    sleep_ms(500);

    println!("--- Idle (waiting for prompt) ---");
    simulator.post("Notification", Some("idle_prompt"), json!({}))?;
    sleep_ms(1_000);

    println!("--- User submits prompt ---");
    simulator.post(
        "UserPromptSubmit",
        None,
        json!({"prompt": "/runbook:prep-pr"}),
    )?;
    sleep_ms(1_000);

    println!("--- Permission prompt (agent needs approval) ---");
    simulator.post(
        "Notification",
        Some("permission_prompt"),
        json!({"reason": "needs file write permission"}),
    )?;
    sleep_ms(1_500);

    println!("--- Back to running ---");
    simulator.post("Notification", Some("idle_prompt"), json!({}))?;
    sleep_ms(500);
    simulator.post("UserPromptSubmit", None, json!({"prompt": "continue"}))?;
    sleep_ms(1_000);

    println!("--- Task completed ---");
    simulator.post("TaskCompleted", None, json!({"result": "ok"}))?;
    sleep_ms(500);

    println!("--- Stop ---");
    simulator.post("Stop", None, json!({}))?;
    sleep_ms(500);

    println!("--- Session end ---");
    simulator.post("SessionEnd", None, json!({}))?;

    println!();
    println!("✓ Simulation complete");

    Ok(())
}

struct Simulator {
    client: Client,
    daemon: String,
    session_id: String,
}

impl Simulator {
    fn new(args: Args) -> anyhow::Result<Self> {
        Ok(Self {
            client: Client::builder().build()?,
            daemon: args
                .daemon
                .or_else(|| std::env::var("DAEMON_BASE_URL").ok())
                .unwrap_or_else(|| "http://127.0.0.1:29381".to_string())
                .trim_end_matches('/')
                .to_string(),
            session_id: args.session_id,
        })
    }

    fn post(&self, hook: &str, matcher: Option<&str>, payload: Value) -> anyhow::Result<()> {
        match matcher {
            Some(matcher) => println!("→ {hook}/{matcher}"),
            None => println!("→ {hook}"),
        }

        let event = HookEvent {
            hook: hook.to_string(),
            matcher: matcher.map(str::to_string),
            session_id: Some(self.session_id.clone()),
            session_tag: None,
            payload,
        };

        self.client
            .post(format!("{}/hook", self.daemon))
            .json(&event)
            .send()?
            .error_for_status()?;

        Ok(())
    }
}

fn sleep_ms(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}
