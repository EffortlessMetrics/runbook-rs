//! Pure hook event transition logic.

use runbook_protocol::AgentState;

/// Result of applying a hook event to a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookTransition {
    /// Session should remain alive and move into this state.
    SetState(AgentState),
    /// Session should be removed from active sessions.
    EndSession,
    /// Event had no state transition.
    Noop,
}

/// Compute the transition caused by a hook + optional matcher.
pub fn transition_for_hook(hook: &str, matcher: Option<&str>) -> HookTransition {
    match hook {
        "SessionStart" => HookTransition::SetState(AgentState::Idle),
        "Notification" => match matcher {
            Some("idle_prompt") => HookTransition::SetState(AgentState::Idle),
            Some("permission_prompt") => HookTransition::SetState(AgentState::WaitingPermission),
            Some("elicitation_dialog") => HookTransition::SetState(AgentState::WaitingInput),
            _ => HookTransition::Noop,
        },
        "UserPromptSubmit" | "PreToolUse" => HookTransition::SetState(AgentState::Running),
        "PermissionRequest" => HookTransition::SetState(AgentState::WaitingPermission),
        "PostToolUse" | "PostToolUseFailure" => HookTransition::SetState(AgentState::Running),
        "TaskCompleted" => HookTransition::SetState(AgentState::Complete),
        "Stop" => HookTransition::SetState(AgentState::Settled),
        "SessionEnd" => HookTransition::EndSession,
        "RunbookPolicy" => match matcher {
            Some("blocked") => HookTransition::SetState(AgentState::Blocked),
            _ => HookTransition::Noop,
        },
        _ => HookTransition::Noop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_notification_matchers() {
        assert_eq!(
            transition_for_hook("Notification", Some("idle_prompt")),
            HookTransition::SetState(AgentState::Idle)
        );
        assert_eq!(
            transition_for_hook("Notification", Some("permission_prompt")),
            HookTransition::SetState(AgentState::WaitingPermission)
        );
        assert_eq!(
            transition_for_hook("Notification", Some("elicitation_dialog")),
            HookTransition::SetState(AgentState::WaitingInput)
        );
    }

    #[test]
    fn maps_end_session() {
        assert_eq!(
            transition_for_hook("SessionEnd", None),
            HookTransition::EndSession
        );
    }

    #[test]
    fn ignores_unknown_events() {
        assert_eq!(
            transition_for_hook("SomethingElse", Some("x")),
            HookTransition::Noop
        );
    }
}
