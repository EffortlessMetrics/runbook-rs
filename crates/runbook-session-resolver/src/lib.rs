use std::collections::HashMap;

use runbook_protocol::{AgentState, HooksMode};

/// Inputs required to resolve the currently relevant `AgentState`.
pub struct SessionResolutionInput<'a> {
    pub hooks_mode: HooksMode,
    pub session_states: &'a HashMap<String, AgentState>,
    pub last_ended_state: Option<AgentState>,
    pub selected_terminal_index: usize,
    pub terminal_tag_map: &'a HashMap<usize, String>,
    pub session_tag_map: &'a HashMap<String, String>,
}

/// Resolve the currently selected `session_id` from terminal/session tag maps.
pub fn resolve_selected_session_id<'a>(
    selected_terminal_index: usize,
    terminal_tag_map: &'a HashMap<usize, String>,
    session_tag_map: &'a HashMap<String, String>,
) -> Option<&'a str> {
    let tag = terminal_tag_map.get(&selected_terminal_index)?;
    let session_id = session_tag_map.get(tag)?;
    Some(session_id.as_str())
}

/// Resolve the current daemon-visible `AgentState` from session and capability context.
pub fn resolve_current_agent_state(input: SessionResolutionInput<'_>) -> AgentState {
    if input.hooks_mode == HooksMode::Absent {
        return AgentState::Unknown;
    }

    match input.session_states.len() {
        0 => input.last_ended_state.unwrap_or(AgentState::Unknown),
        1 => input
            .session_states
            .values()
            .copied()
            .next()
            .unwrap_or(AgentState::Unknown),
        _ => resolve_selected_session_id(
            input.selected_terminal_index,
            input.terminal_tag_map,
            input.session_tag_map,
        )
        .and_then(|session_id| input.session_states.get(session_id).copied())
        .unwrap_or(AgentState::Unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooks_absent_forces_unknown() {
        let session_states = HashMap::from([("s1".to_string(), AgentState::Running)]);
        let input = SessionResolutionInput {
            hooks_mode: HooksMode::Absent,
            session_states: &session_states,
            last_ended_state: Some(AgentState::Complete),
            selected_terminal_index: 0,
            terminal_tag_map: &HashMap::new(),
            session_tag_map: &HashMap::new(),
        };

        assert_eq!(resolve_current_agent_state(input), AgentState::Unknown);
    }

    #[test]
    fn no_live_sessions_uses_last_ended_state() {
        let input = SessionResolutionInput {
            hooks_mode: HooksMode::Active,
            session_states: &HashMap::new(),
            last_ended_state: Some(AgentState::Blocked),
            selected_terminal_index: 0,
            terminal_tag_map: &HashMap::new(),
            session_tag_map: &HashMap::new(),
        };

        assert_eq!(resolve_current_agent_state(input), AgentState::Blocked);
    }

    #[test]
    fn single_session_returns_its_state() {
        let session_states = HashMap::from([("s1".to_string(), AgentState::Running)]);
        let input = SessionResolutionInput {
            hooks_mode: HooksMode::Active,
            session_states: &session_states,
            last_ended_state: None,
            selected_terminal_index: 0,
            terminal_tag_map: &HashMap::new(),
            session_tag_map: &HashMap::new(),
        };

        assert_eq!(resolve_current_agent_state(input), AgentState::Running);
    }

    #[test]
    fn multi_session_uses_terminal_session_mapping() {
        let session_states = HashMap::from([
            ("s1".to_string(), AgentState::Running),
            ("s2".to_string(), AgentState::Complete),
        ]);
        let terminal_tag_map = HashMap::from([(1usize, "tag-b".to_string())]);
        let session_tag_map = HashMap::from([("tag-b".to_string(), "s2".to_string())]);

        let input = SessionResolutionInput {
            hooks_mode: HooksMode::Active,
            session_states: &session_states,
            last_ended_state: None,
            selected_terminal_index: 1,
            terminal_tag_map: &terminal_tag_map,
            session_tag_map: &session_tag_map,
        };

        assert_eq!(resolve_current_agent_state(input), AgentState::Complete);
    }

    #[test]
    fn multi_session_without_mapping_degrades_to_unknown() {
        let session_states = HashMap::from([
            ("s1".to_string(), AgentState::Running),
            ("s2".to_string(), AgentState::Complete),
        ]);

        let input = SessionResolutionInput {
            hooks_mode: HooksMode::Active,
            session_states: &session_states,
            last_ended_state: None,
            selected_terminal_index: 2,
            terminal_tag_map: &HashMap::new(),
            session_tag_map: &HashMap::new(),
        };

        assert_eq!(resolve_current_agent_state(input), AgentState::Unknown);
    }
}
