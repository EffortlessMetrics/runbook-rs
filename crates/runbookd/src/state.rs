use std::collections::HashMap;
use std::time::Instant;

use runbook_protocol::AgentState;

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

    /// Currently selected session (if multi-session).
    pub active_session: Option<String>,

    // ----- Capability tracking -----
    /// True when at least one hook event has been received.
    pub hooks_connected: bool,

    /// True when VS Code extension is connected.
    pub vscode_connected: bool,

    /// True when Logi plugin is connected.
    pub logi_connected: bool,
}

impl DaemonState {
    pub fn new(initial_page: usize) -> Self {
        Self {
            armed: None,
            last_dispatched: None,
            page: initial_page,
            sessions: HashMap::new(),
            active_session: None,
            hooks_connected: false,
            vscode_connected: false,
            logi_connected: false,
        }
    }

    /// Returns the agent state for the currently active session,
    /// falling back to Unknown if no session is active or hooks are disconnected.
    pub fn current_agent_state(&self) -> AgentState {
        if !self.hooks_connected {
            return AgentState::Unknown;
        }
        self.active_session
            .as_ref()
            .and_then(|sid| self.sessions.get(sid))
            .map(|s| s.agent_state)
            .unwrap_or(AgentState::Unknown)
    }

    /// Ensure a session entry exists and return a mutable reference.
    pub fn ensure_session(&mut self, session_id: &str) -> &mut SessionState {
        self.sessions
            .entry(session_id.to_string())
            .or_insert_with(|| SessionState::new())
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
