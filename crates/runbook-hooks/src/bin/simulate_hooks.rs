use std::{thread, time::Duration};

use anyhow::{Context, Result};
use clap::Parser;
use reqwest::blocking::Client;
use runbook_protocol::HookEvent;
use serde_json::{json, Value};

#[derive(Debug, Parser)]
#[command(
    name = "simulate-hooks",
    about = "Simulate a Claude Code hook lifecycle against runbookd"
)]
struct Args {
    /// Daemon base URL. Defaults to DAEMON_BASE_URL or http://127.0.0.1:29381.
    #[arg(
        long,
        env = "DAEMON_BASE_URL",
        default_value = "http://127.0.0.1:29381"
    )]
    daemon: String,

    /// Session id included in simulated hook events.
    #[arg(long, default_value = "sess-demo-001")]
    session_id: String,
}

struct Step {
    title: &'static str,
    hook: &'static str,
    matcher: Option<&'static str>,
    payload: Value,
    delay_after: Duration,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let client = Client::new();
    let url = format!("{}/hook", args.daemon.trim_end_matches('/'));

    println!("Simulating Claude Code lifecycle events…\n");

    for step in steps() {
        println!("--- {} ---", step.title);
        post(&client, &url, &args.session_id, &step)?;
        thread::sleep(step.delay_after);
    }

    println!("\n✓ Simulation complete");
    Ok(())
}

fn steps() -> Vec<Step> {
    vec![
        Step {
            title: "Session start",
            hook: "SessionStart",
            matcher: None,
            payload: json!({}),
            delay_after: Duration::from_millis(500),
        },
        Step {
            title: "Idle (waiting for prompt)",
            hook: "Notification",
            matcher: Some("idle_prompt"),
            payload: json!({}),
            delay_after: Duration::from_secs(1),
        },
        Step {
            title: "User submits prompt",
            hook: "UserPromptSubmit",
            matcher: None,
            payload: json!({ "prompt": "/runbook:prep-pr" }),
            delay_after: Duration::from_secs(1),
        },
        Step {
            title: "Permission prompt (agent needs approval)",
            hook: "Notification",
            matcher: Some("permission_prompt"),
            payload: json!({ "reason": "needs file write permission" }),
            delay_after: Duration::from_millis(1500),
        },
        Step {
            title: "Back to running",
            hook: "Notification",
            matcher: Some("idle_prompt"),
            payload: json!({}),
            delay_after: Duration::from_millis(500),
        },
        Step {
            title: "Continue prompt",
            hook: "UserPromptSubmit",
            matcher: None,
            payload: json!({ "prompt": "continue" }),
            delay_after: Duration::from_secs(1),
        },
        Step {
            title: "Task completed",
            hook: "TaskCompleted",
            matcher: None,
            payload: json!({ "result": "ok" }),
            delay_after: Duration::from_millis(500),
        },
        Step {
            title: "Stop",
            hook: "Stop",
            matcher: None,
            payload: json!({}),
            delay_after: Duration::from_millis(500),
        },
        Step {
            title: "Session end",
            hook: "SessionEnd",
            matcher: None,
            payload: json!({}),
            delay_after: Duration::ZERO,
        },
    ]
}

fn post(client: &Client, url: &str, session_id: &str, step: &Step) -> Result<()> {
    println!(
        "→ {}{}",
        step.hook,
        step.matcher.map(|m| format!("/{m}")).unwrap_or_default()
    );

    let event = HookEvent {
        hook: step.hook.to_string(),
        matcher: step.matcher.map(str::to_string),
        session_id: Some(session_id.to_string()),
        session_tag: None,
        payload: step.payload.clone(),
    };

    client
        .post(url)
        .json(&event)
        .send()
        .with_context(|| format!("failed to POST simulated {} hook to {url}", step.hook))?
        .error_for_status()
        .with_context(|| format!("daemon rejected simulated {} hook", step.hook))?;

    Ok(())
}
