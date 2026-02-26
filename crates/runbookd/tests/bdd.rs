//! BDD test harness for runbookd.
//!
//! Exercises the pure reducer + state model against Gherkin scenarios
//! covering the four acceptance criteria: safety, truthfulness,
//! multi-session honesty, and gating.

use cucumber::{given, then, when, World as _};
use runbookd::config::RunbookConfig;
use runbookd::reducer::{self, Event, SideEffect};
use runbookd::state::DaemonState;
use runbook_protocol::{AgentState, DialpadButton, HooksMode, TerminalInfo, TerminalsSnapshot};

// ---------------------------------------------------------------------------
// World — the BDD test state container
// ---------------------------------------------------------------------------

#[derive(Debug, cucumber::World)]
struct DaemonWorld {
    state: DaemonState,
    config: RunbookConfig,
    /// Collect side effects from each reduce() call so we can assert on them.
    effects: Vec<SideEffect>,
}

impl Default for DaemonWorld {
    fn default() -> Self {
        Self {
            state: DaemonState::new(0),
            config: sample_config(),
            effects: Vec::new(),
        }
    }
}

impl DaemonWorld {
    fn apply(&mut self, event: Event) {
        let effs = reducer::reduce(&mut self.state, &self.config, event);
        self.effects.extend(effs);
    }

    /// Check if any VscodeCommand side effect contains `text` with `newline`.
    fn has_send_text(&self, text: &str, newline: bool) -> bool {
        self.effects.iter().any(|e| match e {
            SideEffect::SendVscodeCommand(cmd) => {
                let payload_text = cmd.payload.get("text").and_then(|v| v.as_str());
                let payload_nl = cmd.payload.get("add_newline").and_then(|v| v.as_bool());
                payload_text == Some(text) && payload_nl == Some(newline)
            }
            _ => false,
        })
    }

    /// Check if any VscodeCommand sends a raw key sequence.
    fn has_send_sequence(&self, seq: &str) -> bool {
        self.effects.iter().any(|e| match e {
            SideEffect::SendVscodeCommand(cmd) => {
                let payload_text = cmd.payload.get("text").and_then(|v| v.as_str());
                payload_text == Some(seq)
            }
            _ => false,
        })
    }

    /// No VscodeCommand side effects at all.
    fn no_vscode_commands(&self) -> bool {
        !self.effects.iter().any(|e| matches!(e, SideEffect::SendVscodeCommand(_)))
    }
}

fn sample_config() -> RunbookConfig {
    let yaml = r#"
keypad:
  pages:
    - name: core
      slots:
        - prompt_id: prep_pr
        - prompt_id: break_task
        - prompt_id: scratch_note
        - {}
        - {}
        - {}
        - {}
        - {}
        - gate: pr
prompts:
  prep_pr:
    label: "PREP PR"
    sublabel: "receipts"
    claude_command: "/runbook:prep-pr"
    fallback_text: "Prep PR. Include summary, risks, test plan."
  break_task:
    label: "BREAK TASK"
    claude_command: "/runbook:break-task"
  scratch_note:
    label: "SCRATCH"
    arm_style: prefill
    fallback_text: "Draft a note"
gates:
  pr:
    label: "PR"
    sublabel: "jump"
    action: open_pr
"#;
    serde_yaml::from_str(yaml).unwrap()
}

// ===========================================================================
// Given steps
// ===========================================================================

#[given("a fresh daemon with prompts")]
async fn fresh_daemon(w: &mut DaemonWorld) {
    *w = DaemonWorld::default();
}

#[given(expr = "the operator has armed {string}")]
async fn operator_armed(w: &mut DaemonWorld, prompt_id: String) {
    w.effects.clear();
    w.apply(Event::KeypadPress { prompt_id });
    w.effects.clear(); // Clear arming effects; we only want to observe the next action.
}

// ===========================================================================
// When steps
// ===========================================================================

#[when(expr = "the operator presses keypad slot {string}")]
async fn press_keypad(w: &mut DaemonWorld, prompt_id: String) {
    w.effects.clear();
    w.apply(Event::KeypadPress { prompt_id });
}

#[when("the operator presses Enter")]
async fn press_enter(w: &mut DaemonWorld) {
    w.effects.clear();
    w.apply(Event::DialpadButton {
        button: DialpadButton::Enter,
    });
}

#[when("the operator presses Esc")]
async fn press_esc(w: &mut DaemonWorld) {
    w.effects.clear();
    w.apply(Event::DialpadButton {
        button: DialpadButton::Esc,
    });
}

#[when("the operator presses Ctrl+C")]
async fn press_ctrl_c(w: &mut DaemonWorld) {
    w.effects.clear();
    w.apply(Event::DialpadButton {
        button: DialpadButton::CtrlC,
    });
}

#[when("the operator presses Export")]
async fn press_export(w: &mut DaemonWorld) {
    w.effects.clear();
    w.apply(Event::DialpadButton {
        button: DialpadButton::Export,
    });
}

#[when(expr = "hook {string} arrives with matcher {string} for session {string}")]
async fn hook_with_matcher(w: &mut DaemonWorld, hook: String, matcher: String, session: String) {
    w.effects.clear();
    w.apply(Event::HookEvent {
        hook,
        matcher: Some(matcher),
        session_id: Some(session),
        session_tag: None,
    });
}

#[when(expr = "hook {string} arrives for session {string}")]
async fn hook_no_matcher(w: &mut DaemonWorld, hook: String, session: String) {
    w.effects.clear();
    w.apply(Event::HookEvent {
        hook,
        matcher: None,
        session_id: Some(session),
        session_tag: None,
    });
}

#[when(expr = "hook {string} arrives with matcher {string} for session {string} with tag {string}")]
async fn hook_with_tag(
    w: &mut DaemonWorld,
    hook: String,
    matcher: String,
    session: String,
    tag: String,
) {
    w.effects.clear();
    w.apply(Event::HookEvent {
        hook,
        matcher: Some(matcher),
        session_id: Some(session),
        session_tag: Some(tag),
    });
}

#[when(expr = "hook {string} arrives for session {string} with tag {string}")]
async fn hook_no_matcher_with_tag(
    w: &mut DaemonWorld,
    hook: String,
    session: String,
    tag: String,
) {
    w.effects.clear();
    w.apply(Event::HookEvent {
        hook,
        matcher: None,
        session_id: Some(session),
        session_tag: Some(tag),
    });
}

#[when(expr = "terminal {int} has tag {string}")]
async fn terminal_has_tag(w: &mut DaemonWorld, index: usize, tag: String) {
    // Inject terminal info into daemon state.
    w.state
        .terminal_tag_map
        .insert(index, tag.clone());

    // Ensure we have enough terminal entries.
    while w.state.terminals.len() <= index {
        w.state.terminals.push(TerminalInfo {
            index: w.state.terminals.len(),
            name: format!("terminal-{}", w.state.terminals.len()),
            session_tag: None,
        });
    }
    w.state.terminals[index].session_tag = Some(tag);
}

#[when(expr = "terminal {int} is selected")]
async fn terminal_selected(w: &mut DaemonWorld, index: usize) {
    w.state.selected_terminal_index = index;
}

// ===========================================================================
// Then steps
// ===========================================================================

#[then(expr = "the daemon is armed with {string}")]
async fn daemon_armed_with(w: &mut DaemonWorld, prompt_id: String) {
    assert_eq!(
        w.state.armed.as_deref(),
        Some(prompt_id.as_str()),
        "expected armed with {prompt_id}, got {:?}",
        w.state.armed
    );
}

#[then("the daemon is no longer armed")]
async fn daemon_not_armed(w: &mut DaemonWorld) {
    assert!(
        w.state.armed.is_none(),
        "expected not armed, got {:?}",
        w.state.armed
    );
}

#[then("no text was sent to the terminal")]
async fn no_text_sent(w: &mut DaemonWorld) {
    assert!(
        w.no_vscode_commands(),
        "expected no VS Code commands, got: {:?}",
        w.effects
    );
}

#[then(expr = "{string} is sent to the terminal with newline")]
async fn text_sent_with_newline(w: &mut DaemonWorld, text: String) {
    assert!(
        w.has_send_text(&text, true),
        "expected '{text}' with newline in effects: {:?}",
        w.effects
    );
}

#[then(expr = "{string} is sent to the terminal without newline")]
async fn text_sent_without_newline(w: &mut DaemonWorld, text: String) {
    assert!(
        w.has_send_text(&text, false),
        "expected '{text}' without newline in effects: {:?}",
        w.effects
    );
}

#[then("a literal Enter is sent to the terminal")]
async fn literal_enter_sent(w: &mut DaemonWorld) {
    // Un-armed Enter sends empty text with newline=true, which is the VS Code
    // Terminal.sendText("", true) equivalent of pressing Enter.
    assert!(
        w.has_send_text("", true),
        "expected literal Enter (empty text + newline) in effects: {:?}",
        w.effects
    );
}

#[then("no prompt text was sent")]
async fn no_prompt_sent(w: &mut DaemonWorld) {
    let has_prompt = w.effects.iter().any(|e| match e {
        SideEffect::SendVscodeCommand(cmd) => {
            let text = cmd.payload.get("text").and_then(|v| v.as_str()).unwrap_or("");
            text.starts_with("/runbook:")
        }
        _ => false,
    });
    assert!(
        !has_prompt,
        "expected no prompt command, got: {:?}",
        w.effects
    );
}

#[then("no escape was sent to the terminal")]
async fn no_esc_sent(w: &mut DaemonWorld) {
    assert!(
        !w.has_send_sequence("\x1b"),
        "expected no Esc sent, but found Esc in effects: {:?}",
        w.effects
    );
}

#[then("a literal Esc is sent to the terminal")]
async fn literal_esc_sent(w: &mut DaemonWorld) {
    assert!(
        w.has_send_sequence("\x1b"),
        "expected literal Esc (\\x1b) in effects: {:?}",
        w.effects
    );
}

#[then("a literal Ctrl+C is sent to the terminal")]
async fn literal_ctrl_c_sent(w: &mut DaemonWorld) {
    assert!(
        w.has_send_sequence("\x03"),
        "expected literal Ctrl+C (\\x03) in effects: {:?}",
        w.effects
    );
}

#[then(expr = "the agent state is {string}")]
async fn agent_state_is(w: &mut DaemonWorld, expected: String) {
    let actual = w.state.current_agent_state();
    let actual_str = serde_json::to_value(&actual)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        actual_str, expected,
        "expected agent state '{expected}', got '{actual_str}'"
    );
}

#[then(expr = "hooks mode is {string}")]
async fn hooks_mode_is(w: &mut DaemonWorld, expected: String) {
    let actual = &w.state.hooks_mode;
    let actual_str = serde_json::to_value(actual)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        actual_str, expected,
        "expected hooks mode '{expected}', got '{actual_str}'"
    );
}

#[then("no sessions remain")]
async fn no_sessions(w: &mut DaemonWorld) {
    assert!(
        w.state.sessions.is_empty(),
        "expected 0 sessions, got {}",
        w.state.sessions.len()
    );
}

#[then(expr = "the last ended state is {string}")]
async fn last_ended_state(w: &mut DaemonWorld, expected: String) {
    let actual = w.state.last_ended_state.expect("no last_ended_state");
    let actual_str = serde_json::to_value(&actual)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        actual_str, expected,
        "expected last ended state '{expected}', got '{actual_str}'"
    );
}

#[then(expr = "there are {int} sessions")]
async fn n_sessions(w: &mut DaemonWorld, n: usize) {
    assert_eq!(
        w.state.sessions.len(),
        n,
        "expected {n} sessions, got {}",
        w.state.sessions.len()
    );
}

#[then(expr = "there is {int} session")]
async fn one_session(w: &mut DaemonWorld, n: usize) {
    assert_eq!(
        w.state.sessions.len(),
        n,
        "expected {n} session(s), got {}",
        w.state.sessions.len()
    );
}

#[then(expr = "session tag {string} maps to session {string}")]
async fn session_tag_maps_to(w: &mut DaemonWorld, tag: String, session: String) {
    let mapped = w.state.session_tag_map.get(&tag);
    assert_eq!(
        mapped,
        Some(&session),
        "expected tag '{tag}' → session '{session}', got {:?}",
        mapped
    );
}

// ===========================================================================
// Entry point
// ===========================================================================

#[tokio::main]
async fn main() {
    DaemonWorld::run("tests/features").await;
}
