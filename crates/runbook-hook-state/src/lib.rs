//! Pure mapping from Claude hook events to runbook agent-state transitions.

use runbook_protocol::AgentState;

/// The reducer action implied by a Claude hook event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookAction {
    /// Set the session state to this value.
    SetState(AgentState),
    /// Remove the session entirely.
    EndSession,
    /// No reducer action is needed for this hook/matcher pair.
    Noop,
}

/// Compute the reducer action for a hook event.
pub fn action_for_hook(hook: &str, matcher: Option<&str>) -> HookAction {
    match hook {
        "SessionStart" => HookAction::SetState(AgentState::Idle),
        "Notification" => match matcher {
            Some("idle_prompt") => HookAction::SetState(AgentState::Idle),
            Some("permission_prompt") => HookAction::SetState(AgentState::WaitingPermission),
            Some("elicitation_dialog") => HookAction::SetState(AgentState::WaitingInput),
            _ => HookAction::Noop,
        },
        "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PostToolUseFailure" => {
            HookAction::SetState(AgentState::Running)
        }
        "PermissionRequest" => HookAction::SetState(AgentState::WaitingPermission),
        "TaskCompleted" => HookAction::SetState(AgentState::Complete),
        "Stop" => HookAction::SetState(AgentState::Settled),
        "SessionEnd" => HookAction::EndSession,
        "RunbookPolicy" => match matcher {
            Some("blocked") => HookAction::SetState(AgentState::Blocked),
            _ => HookAction::Noop,
        },
        _ => HookAction::Noop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_matchers_map_to_expected_states() {
        assert_eq!(
            action_for_hook("Notification", Some("idle_prompt")),
            HookAction::SetState(AgentState::Idle)
        );
        assert_eq!(
            action_for_hook("Notification", Some("permission_prompt")),
            HookAction::SetState(AgentState::WaitingPermission)
        );
        assert_eq!(
            action_for_hook("Notification", Some("elicitation_dialog")),
            HookAction::SetState(AgentState::WaitingInput)
        );
        assert_eq!(
            action_for_hook("Notification", Some("other")),
            HookAction::Noop
        );
    }

    #[test]
    fn lifecycle_hooks_map_to_expected_states() {
        assert_eq!(
            action_for_hook("UserPromptSubmit", None),
            HookAction::SetState(AgentState::Running)
        );
        assert_eq!(
            action_for_hook("PermissionRequest", None),
            HookAction::SetState(AgentState::WaitingPermission)
        );
        assert_eq!(
            action_for_hook("TaskCompleted", None),
            HookAction::SetState(AgentState::Complete)
        );
        assert_eq!(
            action_for_hook("Stop", None),
            HookAction::SetState(AgentState::Settled)
        );
    }

    #[test]
    fn session_end_and_unknown_hooks() {
        assert_eq!(action_for_hook("SessionEnd", None), HookAction::EndSession);
        assert_eq!(action_for_hook("TotallyUnknown", None), HookAction::Noop);
    }

    #[test]
    fn runbook_policy_blocked_only() {
        assert_eq!(
            action_for_hook("RunbookPolicy", Some("blocked")),
            HookAction::SetState(AgentState::Blocked)
        );
        assert_eq!(
            action_for_hook("RunbookPolicy", Some("other")),
            HookAction::Noop
        );
    }
}
