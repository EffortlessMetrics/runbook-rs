//! Hook-event state transitions extracted from `runbookd` reducer.

use runbook_protocol::AgentState;

/// Action a daemon should take after processing a hook event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookTransition {
    /// Update the session to this state.
    SetState(AgentState),
    /// Remove the session.
    EndSession,
    /// No state mutation for this hook.
    Noop,
}

/// Resolve a hook + optional matcher into a state transition.
pub fn transition_for_hook(hook: &str, matcher: Option<&str>) -> HookTransition {
    match hook {
        "SessionStart" => HookTransition::SetState(AgentState::Idle),
        "Notification" => match matcher {
            Some("idle_prompt") => HookTransition::SetState(AgentState::Idle),
            Some("permission_prompt") => HookTransition::SetState(AgentState::WaitingPermission),
            Some("elicitation_dialog") => HookTransition::SetState(AgentState::WaitingInput),
            _ => HookTransition::Noop,
        },
        "UserPromptSubmit" => HookTransition::SetState(AgentState::Running),
        "PreToolUse" => HookTransition::SetState(AgentState::Running),
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
    fn maps_known_hooks() {
        assert_eq!(
            transition_for_hook("PermissionRequest", None),
            HookTransition::SetState(AgentState::WaitingPermission)
        );
        assert_eq!(
            transition_for_hook("TaskCompleted", None),
            HookTransition::SetState(AgentState::Complete)
        );
    }

    #[test]
    fn maps_notification_matchers() {
        assert_eq!(
            transition_for_hook("Notification", Some("idle_prompt")),
            HookTransition::SetState(AgentState::Idle)
        );
        assert_eq!(
            transition_for_hook("Notification", Some("elicitation_dialog")),
            HookTransition::SetState(AgentState::WaitingInput)
        );
        assert_eq!(
            transition_for_hook("Notification", Some("unknown")),
            HookTransition::Noop
        );
    }

    #[test]
    fn maps_session_end_and_unknowns() {
        assert_eq!(
            transition_for_hook("SessionEnd", None),
            HookTransition::EndSession
        );
        assert_eq!(
            transition_for_hook("SomeFutureHook", None),
            HookTransition::Noop
        );
    }

    #[test]
    fn maps_runbook_policy_matcher() {
        assert_eq!(
            transition_for_hook("RunbookPolicy", Some("blocked")),
            HookTransition::SetState(AgentState::Blocked)
        );
        assert_eq!(
            transition_for_hook("RunbookPolicy", Some("other")),
            HookTransition::Noop
        );
    }
}
