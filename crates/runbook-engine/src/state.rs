use std::collections::HashMap;
use std::time::Instant;

use runbook_protocol::{AgentState, HooksMode, TerminalInfo};

/// Central daemon state. Owned by the daemon task behind a Mutex.
#[derive(Debug)]
pub struct DaemonState {
    /// Armed prompt_id (set by keypad press, cleared by Esc or Enter dispatch).
    pub armed: Option<String>,

    /// Last dispatched prompt_id (for display / debug).
    pub last_dispatched: Option<String>,

    /// Active page index.
    pub page: usize,

    // ----- Per-session hook state -----
    /// Session states keyed by `session_id` (from Claude Code hooks).
    pub sessions: HashMap<String, SessionState>,

    /// Learned mapping: session_tag → session_id (populated from hook events).
    pub session_tag_map: HashMap<String, String>,

    // ----- Terminal tracking (from VS Code extension) -----
    /// Terminal list as last reported by VS Code.
    pub terminals: Vec<TerminalInfo>,

    /// Which terminal the roller has selected.
    pub selected_terminal_index: usize,

    /// Mapping: terminal_index → session_tag (from VS Code terminal env).
    pub terminal_tag_map: HashMap<usize, String>,

    // ----- Capability tracking -----
    /// Hook integration mode.
    pub hooks_mode: HooksMode,

    /// When the last hook event was received.
    pub last_hook_ts: Option<Instant>,

    /// True when VS Code extension is connected.
    pub vscode_connected: bool,

    /// True when Logi plugin is connected.
    pub logi_connected: bool,

    /// Latched: the most recent state of the last session to end.
    pub last_ended_state: Option<AgentState>,
}

impl DaemonState {
    pub fn new(initial_page: usize) -> Self {
        Self {
            armed: None,
            last_dispatched: None,
            page: initial_page,
            sessions: HashMap::new(),
            session_tag_map: HashMap::new(),
            terminals: Vec::new(),
            selected_terminal_index: 0,
            terminal_tag_map: HashMap::new(),
            hooks_mode: HooksMode::Absent,
            last_hook_ts: None,
            vscode_connected: false,
            logi_connected: false,
            last_ended_state: None,
        }
    }

    /// Returns the agent state to render.
    ///
    /// Rules:
    /// - **Hooks absent** → `Unknown`
    /// - **0 live sessions** → `last_ended_state`, then `Unknown`
    /// - **1 session** → that session's state
    /// - **>1 sessions** → try to resolve via terminal↔session correlation, else `Unknown`
    pub fn current_agent_state(&self) -> AgentState {
        if self.hooks_mode == HooksMode::Absent {
            return AgentState::Unknown;
        }

        match self.sessions.len() {
            0 => self.last_ended_state.unwrap_or(AgentState::Unknown),
            1 => self
                .sessions
                .values()
                .next()
                .map(|s| s.agent_state)
                .unwrap_or(AgentState::Unknown),
            _ => {
                // Multi-session: try to resolve via terminal selection.
                if let Some(session_id) = self.selected_session_id() {
                    self.sessions
                        .get(&session_id)
                        .map(|s| s.agent_state)
                        .unwrap_or(AgentState::Unknown)
                } else {
                    // Can't correlate terminal → session. Degrade.
                    AgentState::Unknown
                }
            }
        }
    }

    /// Attempt to resolve the currently selected terminal to a session_id.
    ///
    /// Path: selected_terminal_index → terminal_tag_map → session_tag → session_tag_map → session_id
    pub fn selected_session_id(&self) -> Option<String> {
        let tag = self.terminal_tag_map.get(&self.selected_terminal_index)?;
        let session_id = self.session_tag_map.get(tag)?;
        Some(session_id.clone())
    }

    /// Ensure a session entry exists and return a mutable reference.
    pub fn ensure_session(&mut self, session_id: &str) -> &mut SessionState {
        self.sessions
            .entry(session_id.to_string())
            .or_insert_with(SessionState::new)
    }

    /// Remove a session (on SessionEnd) and clean up related state.
    pub fn remove_session(&mut self, session_id: &str) {
        if let Some(session) = self.sessions.remove(session_id) {
            self.last_ended_state = Some(session.agent_state);
        }

        // Clean up session_tag_map entries pointing to this session.
        self.session_tag_map.retain(|_tag, sid| sid != session_id);

        // Clear armed + last_dispatched — no valid target anymore.
        self.armed = None;
        self.last_dispatched = None;
    }

    /// Learn the session_tag → session_id mapping from a hook event.
    pub fn learn_session_tag(&mut self, session_tag: &str, session_id: &str) {
        self.session_tag_map
            .insert(session_tag.to_string(), session_id.to_string());
    }
}

/// Per-session state derived from hook events.
#[derive(Debug, Clone)]
pub struct SessionState {
    pub agent_state: AgentState,
    pub last_tool: Option<String>,
    pub started_at: Instant,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            agent_state: AgentState::Unknown,
            last_tool: None,
            started_at: Instant::now(),
        }
    }
}
