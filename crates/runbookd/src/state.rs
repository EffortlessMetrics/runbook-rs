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

    /// Latched: the most recent state of the last session to end.
    /// Used so we can show "Ended" briefly after the last session goes away.
    pub last_ended_state: Option<AgentState>,
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
            last_ended_state: None,
        }
    }

    /// Returns the agent state to render.
    ///
    /// Rules:
    /// - **No hooks received** → `Unknown`
    /// - **0 live sessions** → `last_ended_state` (briefly shows `Ended`), then `Unknown`
    /// - **1 session** → that session's state
    /// - **>1 sessions** → `Unknown` (multi-session ambiguity)
    pub fn current_agent_state(&self) -> AgentState {
        if !self.hooks_connected {
            return AgentState::Unknown;
        }

        match self.sessions.len() {
            0 => {
                // All sessions ended. Show latched end state if available.
                self.last_ended_state.unwrap_or(AgentState::Unknown)
            }
            1 => {
                // Single session — show its state directly.
                self.sessions
                    .values()
                    .next()
                    .map(|s| s.agent_state)
                    .unwrap_or(AgentState::Unknown)
            }
            _ => {
                // Multiple live sessions — we can't truthfully map
                // terminal selection ↔ session_id yet, so degrade.
                AgentState::Unknown
            }
        }
    }

    /// Ensure a session entry exists and return a mutable reference.
    pub fn ensure_session(&mut self, session_id: &str) -> &mut SessionState {
        self.sessions
            .entry(session_id.to_string())
            .or_insert_with(|| SessionState::new())
    }

    /// Remove a session (on SessionEnd) and clean up related state.
    pub fn remove_session(&mut self, session_id: &str) {
        if let Some(session) = self.sessions.remove(session_id) {
            self.last_ended_state = Some(session.agent_state);
        }

        // If the ended session was the active one, clear selection.
        if self.active_session.as_deref() == Some(session_id) {
            self.active_session = None;

            // If exactly one session remains, auto-select it.
            if self.sessions.len() == 1 {
                self.active_session = self.sessions.keys().next().cloned();
            }
        }

        // Clear armed + last_dispatched — no valid target anymore.
        self.armed = None;
        self.last_dispatched = None;
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
