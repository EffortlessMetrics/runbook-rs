use runbook_protocol::AgentState;

/// Outcome of evaluating a hook event for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookTransition {
    /// Set the session to a specific agent state.
    Set(AgentState),
    /// Remove the session from daemon state.
    RemoveSession,
    /// No state change for this hook.
    Noop,
}

/// Maps hook + matcher values to a session state transition.
pub fn transition_for_hook(hook: &str, matcher: Option<&str>) -> HookTransition {
    match hook {
        "SessionStart" => HookTransition::Set(AgentState::Idle),
        "Notification" => match matcher {
            Some("idle_prompt") => HookTransition::Set(AgentState::Idle),
            Some("permission_prompt") => HookTransition::Set(AgentState::WaitingPermission),
            Some("elicitation_dialog") => HookTransition::Set(AgentState::WaitingInput),
            _ => HookTransition::Noop,
        },
        "UserPromptSubmit" => HookTransition::Set(AgentState::Running),
        "PreToolUse" => HookTransition::Set(AgentState::Running),
        "PermissionRequest" => HookTransition::Set(AgentState::WaitingPermission),
        "PostToolUse" | "PostToolUseFailure" => HookTransition::Set(AgentState::Running),
        "TaskCompleted" => HookTransition::Set(AgentState::Complete),
        "Stop" => HookTransition::Set(AgentState::Settled),
        "SessionEnd" => HookTransition::RemoveSession,
        "RunbookPolicy" => match matcher {
            Some("blocked") => HookTransition::Set(AgentState::Blocked),
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
            HookTransition::Set(AgentState::Idle)
        );
        assert_eq!(
            transition_for_hook("Notification", Some("permission_prompt")),
            HookTransition::Set(AgentState::WaitingPermission)
        );
        assert_eq!(
            transition_for_hook("Notification", Some("elicitation_dialog")),
            HookTransition::Set(AgentState::WaitingInput)
        );
    }

    #[test]
    fn maps_session_end() {
        assert_eq!(
            transition_for_hook("SessionEnd", None),
            HookTransition::RemoveSession
        );
    }

    #[test]
    fn maps_unknown_to_noop() {
        assert_eq!(
            transition_for_hook("AnythingElse", None),
            HookTransition::Noop
        );
    }
}
