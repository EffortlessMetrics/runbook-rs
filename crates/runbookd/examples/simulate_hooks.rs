use std::{env, thread, time::Duration};

use anyhow::{anyhow, Context};
use runbook_protocol::HookEvent;
use serde_json::Value;

const DEFAULT_DAEMON_BASE_URL: &str = "http://127.0.0.1:29381";
const DEFAULT_SESSION_ID: &str = "sess-demo-001";

#[derive(Debug)]
struct Args {
    daemon_base_url: String,
    session_id: String,
}

#[derive(Debug)]
struct SimulatedHook {
    heading: &'static str,
    hook: &'static str,
    matcher: Option<&'static str>,
    payload: Value,
    pause_after: Duration,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse()?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .context("failed to build HTTP client")?;

    println!("Simulating Claude Code lifecycle events…");
    println!();

    for event in simulated_lifecycle(&args.session_id) {
        println!("--- {} ---", event.heading);
        post_hook(&client, &args, &event)?;
        thread::sleep(event.pause_after);
    }

    println!();
    println!("✓ Simulation complete");

    Ok(())
}

impl Args {
    fn parse() -> anyhow::Result<Self> {
        let mut daemon_base_url =
            env::var("DAEMON_BASE_URL").unwrap_or_else(|_| DEFAULT_DAEMON_BASE_URL.to_string());
        let mut session_id = DEFAULT_SESSION_ID.to_string();

        let mut raw = env::args().skip(1);
        while let Some(arg) = raw.next() {
            match arg.as_str() {
                "--daemon" => {
                    daemon_base_url = raw
                        .next()
                        .ok_or_else(|| anyhow!("--daemon requires a base URL"))?;
                }
                "--session-id" => {
                    session_id = raw
                        .next()
                        .ok_or_else(|| anyhow!("--session-id requires a value"))?;
                }
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(anyhow!("unrecognized argument: {other}")),
            }
        }

        Ok(Self {
            daemon_base_url,
            session_id,
        })
    }
}

fn print_help() {
    println!(
        "Simulate Claude Code lifecycle hook events against runbookd.\n\n\
Usage: cargo run -p runbookd --example simulate_hooks -- [OPTIONS]\n\n\
Options:\n  --daemon <URL>       Daemon base URL (default: {DEFAULT_DAEMON_BASE_URL}, or DAEMON_BASE_URL)\n  --session-id <ID>    Simulated Claude session ID (default: {DEFAULT_SESSION_ID})\n  -h, --help           Print help"
    );
}

fn simulated_lifecycle(session_id: &str) -> Vec<SimulatedHook> {
    vec![
        SimulatedHook {
            heading: "Session start",
            hook: "SessionStart",
            matcher: None,
            payload: serde_json::json!({}),
            pause_after: Duration::from_millis(500),
        },
        SimulatedHook {
            heading: "Idle (waiting for prompt)",
            hook: "Notification",
            matcher: Some("idle_prompt"),
            payload: serde_json::json!({}),
            pause_after: Duration::from_secs(1),
        },
        SimulatedHook {
            heading: "User submits prompt",
            hook: "UserPromptSubmit",
            matcher: None,
            payload: serde_json::json!({"prompt": "/runbook:prep-pr"}),
            pause_after: Duration::from_secs(1),
        },
        SimulatedHook {
            heading: "Permission prompt (agent needs approval)",
            hook: "Notification",
            matcher: Some("permission_prompt"),
            payload: serde_json::json!({"reason": "needs file write permission"}),
            pause_after: Duration::from_millis(1500),
        },
        SimulatedHook {
            heading: "Back to running",
            hook: "Notification",
            matcher: Some("idle_prompt"),
            payload: serde_json::json!({}),
            pause_after: Duration::from_millis(500),
        },
        SimulatedHook {
            heading: "User continues prompt",
            hook: "UserPromptSubmit",
            matcher: None,
            payload: serde_json::json!({"prompt": "continue"}),
            pause_after: Duration::from_secs(1),
        },
        SimulatedHook {
            heading: "Task completed",
            hook: "TaskCompleted",
            matcher: None,
            payload: serde_json::json!({"result": "ok"}),
            pause_after: Duration::from_millis(500),
        },
        SimulatedHook {
            heading: "Stop",
            hook: "Stop",
            matcher: None,
            payload: serde_json::json!({}),
            pause_after: Duration::from_millis(500),
        },
        SimulatedHook {
            heading: "Session end",
            hook: "SessionEnd",
            matcher: None,
            payload: serde_json::json!({}),
            pause_after: Duration::ZERO,
        },
    ]
    .into_iter()
    .map(|mut event| {
        insert_session_id(&mut event.payload, session_id);
        event
    })
    .collect()
}

fn insert_session_id(payload: &mut Value, session_id: &str) {
    if let Value::Object(map) = payload {
        map.insert(
            "session_id".to_string(),
            Value::String(session_id.to_string()),
        );
    }
}

fn post_hook(
    client: &reqwest::blocking::Client,
    args: &Args,
    event: &SimulatedHook,
) -> anyhow::Result<()> {
    println!("→ {}{}", event.hook, matcher_suffix(event.matcher));

    let hook_event = HookEvent {
        hook: event.hook.to_string(),
        matcher: event.matcher.map(str::to_string),
        session_id: Some(args.session_id.clone()),
        session_tag: None,
        payload: event.payload.clone(),
    };

    let response = client
        .post(format!(
            "{}/hook",
            args.daemon_base_url.trim_end_matches('/')
        ))
        .json(&hook_event)
        .send()
        .with_context(|| format!("failed to post {} hook", event.hook))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "daemon rejected {} hook with status {}",
            event.hook,
            response.status()
        ));
    }

    Ok(())
}

fn matcher_suffix(matcher: Option<&str>) -> String {
    matcher.map(|m| format!("/{m}")).unwrap_or_default()
}
