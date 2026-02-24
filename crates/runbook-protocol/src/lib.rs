//! Runbook protocol v1
//!
//! This crate intentionally keeps the on-the-wire JSON schema stable.
//!
//! Design goals:
//! - Versioned, explicit message types (`type` tag)
//! - JSON-first (easy for C#, TS, Rust)
//! - Backwards-compatible evolution (additive fields)

use serde::{Deserialize, Serialize};

/// Bump ONLY on breaking changes.
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Logi,
    Vscode,
    Hooks,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// No telemetry (non-Claude tools, or hooks not installed).
    Unknown,
    /// Claude Code is ready for the next prompt (idle_prompt).
    Idle,
    /// A prompt has been submitted (UserPromptSubmit) and Claude is working.
    Running,
    /// Claude is blocked on a permission prompt.
    WaitingPermission,
    /// Claude is blocked on an elicitation/clarification dialog.
    WaitingInput,
    /// Claude has completed a bounded task (TaskCompleted).
    Complete,
    /// Claude has stopped responding (Stop) but session still exists.
    Settled,
    /// Session ended (SessionEnd).
    Ended,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DialpadButton {
    CtrlC,
    Export,
    Esc,
    Enter,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdjustmentKind {
    Dial,
    Roller,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VscodeCommandKind {
    /// Send text to the target terminal.
    SendText,
    /// Focus/select a terminal session.
    FocusTerminal,
    /// Scroll terminal output.
    ScrollTerminal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TerminalScrollUnit {
    Lines,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientToDaemon {
    Hello(Hello),

    // --- Hardware input ---
    KeypadPress(KeypadPress),
    DialpadButtonPress(DialpadButtonPress),
    Adjustment(Adjustment),

    // --- Claude Code hook events (normalized) ---
    HookEvent(HookEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonToClient {
    Hello(HelloAck),

    /// UI model update (key labels, armed prompt, agent state).
    Render(RenderModel),

    /// Command to VS Code extension.
    VscodeCommand(VscodeCommand),

    /// Human-readable notification (debug / toast).
    Notice(Notice),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub client: ClientKind,
    pub protocol: u32,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloAck {
    pub protocol: u32,
    pub daemon_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeypadPress {
    /// 0..=8 (3x3 keypad)
    pub slot: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialpadButtonPress {
    pub button: DialpadButton,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Adjustment {
    pub kind: AdjustmentKind,
    /// Signed number of detents/steps.
    pub delta: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEvent {
    /// Claude Code hook name, e.g. "UserPromptSubmit", "Notification".
    pub hook: String,
    /// Optional matcher (e.g. notification matcher like "permission_prompt").
    pub matcher: Option<String>,
    /// Raw hook JSON payload (opaque to daemon v1; parse in future versions).
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notice {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderModel {
    pub agent_state: AgentState,
    pub armed: Option<ArmedPrompt>,
    pub keypad: KeypadRender,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmedPrompt {
    pub id: String,
    pub label: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeypadRender {
    /// What to show on each of the 9 LCD keys.
    pub slots: Vec<KeypadSlotRender>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeypadSlotRender {
    pub slot: u8,
    pub label: String,
    pub sublabel: Option<String>,
    pub armed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VscodeCommand {
    pub kind: VscodeCommandKind,
    pub target: TerminalTarget,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TerminalTarget {
    /// The daemon/extension's notion of the current Claude Code terminal.
    ActiveClaude,
    /// Whatever VS Code reports as active terminal.
    Active,
}

// Convenience constructors for common VS Code commands.
impl VscodeCommand {
    pub fn send_text(target: TerminalTarget, text: &str, add_newline: bool) -> Self {
        Self {
            kind: VscodeCommandKind::SendText,
            target,
            payload: serde_json::json!({
                "text": text,
                "add_newline": add_newline,
            }),
        }
    }

    pub fn focus_terminal(target: TerminalTarget, direction: i32) -> Self {
        Self {
            kind: VscodeCommandKind::FocusTerminal,
            target,
            payload: serde_json::json!({
                "direction": direction,
            }),
        }
    }

    pub fn scroll_terminal(target: TerminalTarget, delta: i32, unit: TerminalScrollUnit) -> Self {
        Self {
            kind: VscodeCommandKind::ScrollTerminal,
            target,
            payload: serde_json::json!({
                "delta": delta,
                "unit": unit,
            }),
        }
    }
}

