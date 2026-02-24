//! Pure state reducer: `fn reduce(state, event) -> Vec<SideEffect>`.
//!
//! All state transitions happen here, making the daemon logic testable
//! without network or I/O.

use runbook_protocol::{
    AgentState, AdjustmentKind, DialpadButton, PageDirection, TerminalScrollUnit,
    TerminalTarget, VscodeCommand,
};

use crate::config::RunbookConfig;
use crate::state::DaemonState;

/// Events the reducer consumes.
#[derive(Debug)]
pub enum Event {
    KeypadPress { prompt_id: String },
    DialpadButton { button: DialpadButton },
    Adjustment { kind: AdjustmentKind, delta: i32 },
    PageNav { direction: PageDirection },
    HookEvent {
        hook: String,
        matcher: Option<String>,
        session_id: Option<String>,
    },
    ClientConnected { kind: ClientKindTag },
    ClientDisconnected { kind: ClientKindTag },
}

#[derive(Debug, Clone, Copy)]
pub enum ClientKindTag {
    Logi,
    Vscode,
}

/// Side effects emitted by the reducer (executed by the IO layer).
#[derive(Debug)]
pub enum SideEffect {
    /// Broadcast an updated render model to all connected devices.
    BroadcastRender,
    /// Send a VS Code command.
    SendVscodeCommand(VscodeCommand),
}

/// Apply an event to the daemon state, returning side effects to execute.
pub fn reduce(
    state: &mut DaemonState,
    config: &RunbookConfig,
    event: Event,
) -> Vec<SideEffect> {
    match event {
        Event::KeypadPress { prompt_id } => {
            // Arm the prompt (do NOT dispatch).
            if config.prompts.contains_key(&prompt_id) {
                state.armed = Some(prompt_id);
            }
            // Gates get dispatched immediately (they're navigation, not prompts).
            // The caller checks this before emitting the Event.
            vec![SideEffect::BroadcastRender]
        }

        Event::DialpadButton { button } => reduce_dialpad(state, config, button),

        Event::Adjustment { kind, delta } => reduce_adjustment(state, kind, delta),

        Event::PageNav { direction } => {
            let count = config.keypad.pages.len();
            if count == 0 {
                return vec![];
            }
            match direction {
                PageDirection::Next => state.page = (state.page + 1) % count,
                PageDirection::Prev => {
                    state.page = if state.page == 0 {
                        count - 1
                    } else {
                        state.page - 1
                    };
                }
            }
            // Clear armed prompt on page change (prompt_id may not exist on new page).
            state.armed = None;
            vec![SideEffect::BroadcastRender]
        }

        Event::HookEvent {
            hook,
            matcher,
            session_id,
        } => reduce_hook(state, hook, matcher, session_id),

        Event::ClientConnected { kind } => {
            match kind {
                ClientKindTag::Logi => state.logi_connected = true,
                ClientKindTag::Vscode => state.vscode_connected = true,
            }
            vec![SideEffect::BroadcastRender]
        }

        Event::ClientDisconnected { kind } => {
            match kind {
                ClientKindTag::Logi => state.logi_connected = false,
                ClientKindTag::Vscode => state.vscode_connected = false,
            }
            vec![SideEffect::BroadcastRender]
        }
    }
}

fn reduce_dialpad(
    state: &mut DaemonState,
    config: &RunbookConfig,
    button: DialpadButton,
) -> Vec<SideEffect> {
    match button {
        DialpadButton::Enter => {
            if let Some(prompt_id) = state.armed.take() {
                state.last_dispatched = Some(prompt_id.clone());
                // Resolve the prompt to a command.
                if let Some(prompt) = config.prompts.get(&prompt_id) {
                    let is_claude = config.is_claude_primary();
                    if let Some(cmd_text) = prompt.effective_command(is_claude) {
                        let cmd = VscodeCommand::send_text(
                            TerminalTarget::ActiveClaude,
                            cmd_text,
                            true,
                        );
                        return vec![
                            SideEffect::SendVscodeCommand(cmd),
                            SideEffect::BroadcastRender,
                        ];
                    }
                }
                vec![SideEffect::BroadcastRender]
            } else {
                // No prompt armed: send bare Enter (for /export confirmation, etc.)
                let cmd = VscodeCommand::send_text(
                    TerminalTarget::ActiveClaude,
                    "",
                    true,
                );
                vec![SideEffect::SendVscodeCommand(cmd)]
            }
        }

        DialpadButton::Esc => {
            if state.armed.is_some() {
                // Cancel arm (local only — do NOT send Esc to terminal).
                state.armed = None;
                vec![SideEffect::BroadcastRender]
            } else {
                // Send Esc to Claude terminal.
                let cmd = VscodeCommand::send_text(
                    TerminalTarget::ActiveClaude,
                    "\u{1b}",
                    false,
                );
                vec![SideEffect::SendVscodeCommand(cmd)]
            }
        }

        DialpadButton::CtrlC => {
            // Always forward Ctrl+C. Claude Code handles null-first-press gate.
            let cmd = VscodeCommand::send_text(
                TerminalTarget::ActiveClaude,
                "\u{0003}",
                false,
            );
            vec![SideEffect::SendVscodeCommand(cmd)]
        }

        DialpadButton::Export => {
            // Send /export without newline; user must confirm with Enter.
            let cmd = VscodeCommand::send_text(
                TerminalTarget::ActiveClaude,
                "/export",
                false,
            );
            vec![SideEffect::SendVscodeCommand(cmd)]
        }
    }
}

fn reduce_adjustment(
    state: &mut DaemonState,
    kind: AdjustmentKind,
    delta: i32,
) -> Vec<SideEffect> {
    match kind {
        AdjustmentKind::Dial => {
            // Scroll terminal output.
            let cmd = VscodeCommand::scroll_terminal(
                TerminalTarget::ActiveClaude,
                delta,
                TerminalScrollUnit::Lines,
            );
            vec![SideEffect::SendVscodeCommand(cmd)]
        }
        AdjustmentKind::Roller => {
            // Cycle terminals by direction.
            let _ = state; // state not mutated for roller
            let cmd = VscodeCommand::focus_terminal(
                TerminalTarget::Active,
                delta.signum(),
            );
            vec![SideEffect::SendVscodeCommand(cmd)]
        }
    }
}

fn reduce_hook(
    state: &mut DaemonState,
    hook: String,
    matcher: Option<String>,
    session_id: Option<String>,
) -> Vec<SideEffect> {
    state.hooks_connected = true;

    // Determine the session to update.
    let sid = session_id.unwrap_or_else(|| "_default".to_string());

    // Auto-select the session if none is active.
    if state.active_session.is_none() {
        state.active_session = Some(sid.clone());
    }

    let session = state.ensure_session(&sid);

    match hook.as_str() {
        "SessionStart" => {
            session.agent_state = AgentState::Idle;
        }
        "Notification" => match matcher.as_deref() {
            Some("idle_prompt") => session.agent_state = AgentState::Idle,
            Some("permission_prompt") => session.agent_state = AgentState::WaitingPermission,
            Some("elicitation_dialog") => session.agent_state = AgentState::WaitingInput,
            _ => {}
        },
        "UserPromptSubmit" => {
            session.agent_state = AgentState::Running;
        }
        "PreToolUse" => {
            // Tool about to execute — still running.
            session.agent_state = AgentState::Running;
        }
        "PermissionRequest" => {
            session.agent_state = AgentState::WaitingPermission;
        }
        "PostToolUse" | "PostToolUseFailure" => {
            // Tool finished — back to running (Claude will continue or stop).
            session.agent_state = AgentState::Running;
        }
        "TaskCompleted" => {
            session.agent_state = AgentState::Complete;
        }
        "Stop" => {
            session.agent_state = AgentState::Settled;
        }
        "SessionEnd" => {
            session.agent_state = AgentState::Ended;
        }
        _ => {}
    }

    vec![SideEffect::BroadcastRender]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RunbookConfig;
    use crate::state::DaemonState;

    fn sample_config() -> RunbookConfig {
        let yaml = r#"
keypad:
  initial_page: 0
  pages:
    - name: core
      slots:
        - prompt_id: prep_pr
        - prompt_id: break_task
        - {}
        - {}
        - {}
        - {}
        - {}
        - {}
        - {}
prompts:
  prep_pr:
    label: "PREP PR"
    claude_command: "/runbook:prep-pr"
    fallback_text: "Prep a PR."
  break_task:
    label: "BREAK TASK"
    claude_command: "/runbook:break-task"
    fallback_text: "Break task."
"#;
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn arm_and_dispatch() {
        let config = sample_config();
        let mut state = DaemonState::new(0);

        // Arm.
        let effects = reduce(
            &mut state,
            &config,
            Event::KeypadPress {
                prompt_id: "prep_pr".to_string(),
            },
        );
        assert!(state.armed.as_deref() == Some("prep_pr"));
        assert!(matches!(effects[0], SideEffect::BroadcastRender));

        // Dispatch.
        let effects = reduce(
            &mut state,
            &config,
            Event::DialpadButton {
                button: DialpadButton::Enter,
            },
        );
        assert!(state.armed.is_none());
        assert!(state.last_dispatched.as_deref() == Some("prep_pr"));
        assert!(effects.iter().any(|e| matches!(e, SideEffect::SendVscodeCommand(_))));
    }

    #[test]
    fn cancel_arm() {
        let config = sample_config();
        let mut state = DaemonState::new(0);

        // Arm then cancel.
        reduce(
            &mut state,
            &config,
            Event::KeypadPress {
                prompt_id: "prep_pr".to_string(),
            },
        );
        assert!(state.armed.is_some());

        let effects = reduce(
            &mut state,
            &config,
            Event::DialpadButton {
                button: DialpadButton::Esc,
            },
        );
        assert!(state.armed.is_none());
        // Should broadcast render, should NOT send Esc to terminal.
        assert!(effects.iter().all(|e| matches!(e, SideEffect::BroadcastRender)));
    }

    #[test]
    fn esc_when_not_armed_sends_to_terminal() {
        let config = sample_config();
        let mut state = DaemonState::new(0);

        let effects = reduce(
            &mut state,
            &config,
            Event::DialpadButton {
                button: DialpadButton::Esc,
            },
        );
        assert!(effects.iter().any(|e| matches!(e, SideEffect::SendVscodeCommand(_))));
    }

    #[test]
    fn enter_when_not_armed_sends_enter() {
        let config = sample_config();
        let mut state = DaemonState::new(0);

        let effects = reduce(
            &mut state,
            &config,
            Event::DialpadButton {
                button: DialpadButton::Enter,
            },
        );
        assert!(effects.iter().any(|e| matches!(e, SideEffect::SendVscodeCommand(_))));
    }

    #[test]
    fn page_nav_wraps() {
        let config = sample_config();
        let mut state = DaemonState::new(0);

        // Only 1 page; prev wraps to 0.
        reduce(
            &mut state,
            &config,
            Event::PageNav {
                direction: PageDirection::Prev,
            },
        );
        assert_eq!(state.page, 0);

        // Next wraps to 0.
        reduce(
            &mut state,
            &config,
            Event::PageNav {
                direction: PageDirection::Next,
            },
        );
        assert_eq!(state.page, 0);
    }

    #[test]
    fn hook_event_sets_session_state() {
        let config = sample_config();
        let mut state = DaemonState::new(0);
        assert_eq!(state.current_agent_state(), AgentState::Unknown);

        reduce(
            &mut state,
            &config,
            Event::HookEvent {
                hook: "Notification".to_string(),
                matcher: Some("idle_prompt".to_string()),
                session_id: Some("sess1".to_string()),
            },
        );
        assert!(state.hooks_connected);
        assert_eq!(state.current_agent_state(), AgentState::Idle);

        reduce(
            &mut state,
            &config,
            Event::HookEvent {
                hook: "UserPromptSubmit".to_string(),
                matcher: None,
                session_id: Some("sess1".to_string()),
            },
        );
        assert_eq!(state.current_agent_state(), AgentState::Running);
    }

    #[test]
    fn no_hooks_means_unknown() {
        let config = sample_config();
        let state = DaemonState::new(0);
        assert!(!state.hooks_connected);
        assert_eq!(state.current_agent_state(), AgentState::Unknown);
    }
}
