//! Hook event classification and agent-state transitions.

use runbook_protocol::AgentState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookTransition {
    SetAgentState(AgentState),
    RemoveSession,
    Noop,
}

/// Maps Claude hook event names into runbook session transitions.
pub fn classify_hook_event(hook: &str, matcher: Option<&str>) -> HookTransition {
    match hook {
        "SessionStart" => HookTransition::SetAgentState(AgentState::Idle),
        "Notification" => match matcher {
            Some("idle_prompt") => HookTransition::SetAgentState(AgentState::Idle),
            Some("permission_prompt") => {
                HookTransition::SetAgentState(AgentState::WaitingPermission)
            }
            Some("elicitation_dialog") => HookTransition::SetAgentState(AgentState::WaitingInput),
            _ => HookTransition::Noop,
        },
        "UserPromptSubmit" => HookTransition::SetAgentState(AgentState::Running),
        "PreToolUse" => HookTransition::SetAgentState(AgentState::Running),
        "PermissionRequest" => HookTransition::SetAgentState(AgentState::WaitingPermission),
        "PostToolUse" | "PostToolUseFailure" => HookTransition::SetAgentState(AgentState::Running),
        "TaskCompleted" => HookTransition::SetAgentState(AgentState::Complete),
        "Stop" => HookTransition::SetAgentState(AgentState::Settled),
        "SessionEnd" => HookTransition::RemoveSession,
        "RunbookPolicy" => match matcher {
            Some("blocked") => HookTransition::SetAgentState(AgentState::Blocked),
            _ => HookTransition::Noop,
        },
        _ => HookTransition::Noop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_notification_matchers() {
        assert_eq!(
            classify_hook_event("Notification", Some("idle_prompt")),
            HookTransition::SetAgentState(AgentState::Idle)
        );
        assert_eq!(
            classify_hook_event("Notification", Some("permission_prompt")),
            HookTransition::SetAgentState(AgentState::WaitingPermission)
        );
        assert_eq!(
            classify_hook_event("Notification", Some("elicitation_dialog")),
            HookTransition::SetAgentState(AgentState::WaitingInput)
        );
    }

    #[test]
    fn classifies_terminal_states() {
        assert_eq!(
            classify_hook_event("TaskCompleted", None),
            HookTransition::SetAgentState(AgentState::Complete)
        );
        assert_eq!(
            classify_hook_event("Stop", None),
            HookTransition::SetAgentState(AgentState::Settled)
        );
        assert_eq!(
            classify_hook_event("SessionEnd", None),
            HookTransition::RemoveSession
        );
    }

    #[test]
    fn classifies_policy_and_unknown() {
        assert_eq!(
            classify_hook_event("RunbookPolicy", Some("blocked")),
            HookTransition::SetAgentState(AgentState::Blocked)
        );
        assert_eq!(
            classify_hook_event("RunbookPolicy", Some("allow")),
            HookTransition::Noop
        );
        assert_eq!(classify_hook_event("Unknown", None), HookTransition::Noop);
    }
}
