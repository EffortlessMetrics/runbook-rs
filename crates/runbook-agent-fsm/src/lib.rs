//! Pure state machine for mapping hook events to [`AgentState`].

use runbook_protocol::AgentState;

/// Returns the next agent state for a hook event, if the event implies one.
///
/// Events like `SessionEnd` do not directly map to a state and return `None`.
pub fn next_agent_state(hook: &str, matcher: Option<&str>) -> Option<AgentState> {
    match hook {
        "SessionStart" => Some(AgentState::Idle),
        "Notification" => match matcher {
            Some("idle_prompt") => Some(AgentState::Idle),
            Some("permission_prompt") => Some(AgentState::WaitingPermission),
            Some("elicitation_dialog") => Some(AgentState::WaitingInput),
            _ => None,
        },
        "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PostToolUseFailure" => {
            Some(AgentState::Running)
        }
        "PermissionRequest" => Some(AgentState::WaitingPermission),
        "TaskCompleted" => Some(AgentState::Complete),
        "Stop" => Some(AgentState::Settled),
        "RunbookPolicy" => match matcher {
            Some("blocked") => Some(AgentState::Blocked),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_matchers_map_to_expected_states() {
        assert_eq!(
            next_agent_state("Notification", Some("idle_prompt")),
            Some(AgentState::Idle)
        );
        assert_eq!(
            next_agent_state("Notification", Some("permission_prompt")),
            Some(AgentState::WaitingPermission)
        );
        assert_eq!(
            next_agent_state("Notification", Some("elicitation_dialog")),
            Some(AgentState::WaitingInput)
        );
        assert_eq!(next_agent_state("Notification", Some("other")), None);
    }

    #[test]
    fn lifecycle_hooks_map_to_expected_states() {
        assert_eq!(
            next_agent_state("SessionStart", None),
            Some(AgentState::Idle)
        );
        assert_eq!(
            next_agent_state("UserPromptSubmit", None),
            Some(AgentState::Running)
        );
        assert_eq!(
            next_agent_state("PreToolUse", None),
            Some(AgentState::Running)
        );
        assert_eq!(
            next_agent_state("PostToolUse", None),
            Some(AgentState::Running)
        );
        assert_eq!(
            next_agent_state("PostToolUseFailure", None),
            Some(AgentState::Running)
        );
        assert_eq!(
            next_agent_state("PermissionRequest", None),
            Some(AgentState::WaitingPermission)
        );
        assert_eq!(
            next_agent_state("TaskCompleted", None),
            Some(AgentState::Complete)
        );
        assert_eq!(next_agent_state("Stop", None), Some(AgentState::Settled));
        assert_eq!(next_agent_state("SessionEnd", None), None);
    }

    #[test]
    fn runbook_policy_blocked_maps_to_blocked() {
        assert_eq!(
            next_agent_state("RunbookPolicy", Some("blocked")),
            Some(AgentState::Blocked)
        );
        assert_eq!(next_agent_state("RunbookPolicy", Some("allowed")), None);
    }
}
