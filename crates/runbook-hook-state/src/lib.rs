use runbook_protocol::AgentState;

/// Result of applying a hook event to a session's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookTransition {
    /// Set the session to a new state.
    Set(AgentState),
    /// End the session.
    EndSession,
    /// No state change.
    Noop,
}

/// Resolve a hook event into a state transition.
pub fn transition(hook: &str, matcher: Option<&str>) -> HookTransition {
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
        "SessionEnd" => HookTransition::EndSession,
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
    fn notification_matchers_map_correctly() {
        assert_eq!(
            transition("Notification", Some("idle_prompt")),
            HookTransition::Set(AgentState::Idle)
        );
        assert_eq!(
            transition("Notification", Some("permission_prompt")),
            HookTransition::Set(AgentState::WaitingPermission)
        );
        assert_eq!(
            transition("Notification", Some("elicitation_dialog")),
            HookTransition::Set(AgentState::WaitingInput)
        );
        assert_eq!(
            transition("Notification", Some("other")),
            HookTransition::Noop
        );
    }

    #[test]
    fn runbook_policy_only_sets_blocked_on_blocked_matcher() {
        assert_eq!(
            transition("RunbookPolicy", Some("blocked")),
            HookTransition::Set(AgentState::Blocked)
        );
        assert_eq!(
            transition("RunbookPolicy", Some("allowed")),
            HookTransition::Noop
        );
    }

    #[test]
    fn terminal_events_are_mapped() {
        assert_eq!(
            transition("TaskCompleted", None),
            HookTransition::Set(AgentState::Complete)
        );
        assert_eq!(
            transition("Stop", None),
            HookTransition::Set(AgentState::Settled)
        );
        assert_eq!(transition("SessionEnd", None), HookTransition::EndSession);
    }
}
