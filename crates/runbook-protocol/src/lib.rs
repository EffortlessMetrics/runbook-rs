//! Runbook protocol v1
//!
//! This crate intentionally keeps the on-the-wire JSON schema stable.
//!
//! Design goals:
//! - Versioned, explicit message types (`type` tag)
//! - JSON-first (easy for C#, TS, Rust)
//! - Backwards-compatible evolution (additive fields)
//! - snake_case everywhere (enforced by serde)

use serde::{Deserialize, Serialize};

/// Bump ONLY on breaking changes.
pub const PROTOCOL_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Logi,
    Vscode,
    Hooks,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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
    /// A tool call was blocked by policy (PreToolUse deny).
    Blocked,
}

impl Default for AgentState {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DialpadButton {
    CtrlC,
    Export,
    Esc,
    Enter,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdjustmentKind {
    Dial,
    Roller,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PageDirection {
    Prev,
    Next,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VscodeCommandKind {
    /// Send text to the target terminal.
    SendText,
    /// Focus/select a terminal session.
    FocusTerminal,
    /// Scroll terminal output.
    ScrollTerminal,
    /// Open a URI in the default browser / editor.
    OpenUri,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TerminalScrollUnit {
    Lines,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TerminalTarget {
    /// The daemon/extension's notion of the current Claude Code terminal.
    ActiveClaude,
    /// Whatever VS Code reports as active terminal.
    Active,
    /// A terminal at a specific index in the terminal list.
    ByIndex(usize),
}

// ---------------------------------------------------------------------------
// Client → Daemon messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientToDaemon {
    Hello(Hello),

    // --- Hardware input ---
    KeypadPress(KeypadPress),
    DialpadButtonPress(DialpadButtonPress),
    Adjustment(Adjustment),
    PageNav(PageNav),

    // --- Claude Code hook events (normalized) ---
    HookEvent(HookEvent),
}

// ---------------------------------------------------------------------------
// Daemon → Client messages
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Payload structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub client: ClientKind,
    pub protocol: u32,
    pub version: String,
    /// Optional capability hints from the client (e.g. ["hooks", "terminals"]).
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloAck {
    pub protocol: u32,
    pub daemon_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeypadPress {
    /// Prompt ID from the current page slot (not a raw index).
    pub prompt_id: String,
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
pub struct PageNav {
    pub direction: PageDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEvent {
    /// Claude Code hook name, e.g. "UserPromptSubmit", "Notification".
    pub hook: String,
    /// Optional matcher (e.g. notification matcher like "permission_prompt").
    #[serde(default)]
    pub matcher: Option<String>,
    /// Session ID from Claude Code (extracted from hook input's `session_id`).
    #[serde(default)]
    pub session_id: Option<String>,
    /// Raw hook JSON payload (opaque to daemon v1; specific fields parsed as needed).
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notice {
    pub message: String,
}

// ---------------------------------------------------------------------------
// Render model (daemon → device)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderModel {
    pub agent_state: AgentState,
    pub armed: Option<ArmedPrompt>,
    pub keypad: KeypadRender,
    pub page_index: usize,
    pub page_count: usize,
    /// True when daemon has received at least one hook event (Claude Code is connected).
    pub hooks_connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmedPrompt {
    pub prompt_id: String,
    pub label: String,
    /// The command/text that will be dispatched.
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
    pub prompt_id: String,
    pub label: String,
    #[serde(default)]
    pub sublabel: Option<String>,
    pub armed: bool,
}

// ---------------------------------------------------------------------------
// VS Code commands
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VscodeCommand {
    pub kind: VscodeCommandKind,
    pub target: TerminalTarget,
    pub payload: serde_json::Value,
}

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

    pub fn open_uri(uri: &str) -> Self {
        Self {
            kind: VscodeCommandKind::OpenUri,
            target: TerminalTarget::Active,
            payload: serde_json::json!({
                "uri": uri,
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Hook decision output types (for runbook-hooks stdout)
// ---------------------------------------------------------------------------

/// Spec-compliant output for PreToolUse hooks.
///
/// Claude Code expects `hookSpecificOutput.hookEventName = "PreToolUse"` with
/// `permissionDecision` ∈ {"allow", "deny", "ask"}.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreToolUseDecisionOutput {
    #[serde(rename = "hookSpecificOutput")]
    pub hook_specific_output: PreToolUseHookOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreToolUseHookOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String,
    #[serde(rename = "permissionDecision")]
    pub permission_decision: String,
    #[serde(rename = "permissionDecisionReason")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_decision_reason: Option<String>,
    #[serde(rename = "additionalContext")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

impl PreToolUseDecisionOutput {
    pub fn deny(reason: &str) -> Self {
        Self {
            hook_specific_output: PreToolUseHookOutput {
                hook_event_name: "PreToolUse".to_string(),
                permission_decision: "deny".to_string(),
                permission_decision_reason: Some(reason.to_string()),
                additional_context: None,
            },
        }
    }

    pub fn allow(reason: Option<&str>) -> Self {
        Self {
            hook_specific_output: PreToolUseHookOutput {
                hook_event_name: "PreToolUse".to_string(),
                permission_decision: "allow".to_string(),
                permission_decision_reason: reason.map(|s| s.to_string()),
                additional_context: None,
            },
        }
    }
}

/// Spec-compliant output for UserPromptSubmit hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPromptSubmitOutput {
    #[serde(rename = "hookSpecificOutput")]
    pub hook_specific_output: UserPromptSubmitHookOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPromptSubmitHookOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String,
    #[serde(rename = "additionalContext")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

impl UserPromptSubmitOutput {
    pub fn with_context(context: &str) -> Self {
        Self {
            hook_specific_output: UserPromptSubmitHookOutput {
                hook_event_name: "UserPromptSubmit".to_string(),
                additional_context: Some(context.to_string()),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_client_to_daemon() {
        let messages: Vec<ClientToDaemon> = vec![
            ClientToDaemon::Hello(Hello {
                client: ClientKind::Logi,
                protocol: PROTOCOL_VERSION,
                version: "0.1.0".to_string(),
                capabilities: vec!["keypad".to_string()],
            }),
            ClientToDaemon::KeypadPress(KeypadPress {
                prompt_id: "prep_pr".to_string(),
            }),
            ClientToDaemon::DialpadButtonPress(DialpadButtonPress {
                button: DialpadButton::Enter,
            }),
            ClientToDaemon::Adjustment(Adjustment {
                kind: AdjustmentKind::Dial,
                delta: -3,
            }),
            ClientToDaemon::PageNav(PageNav {
                direction: PageDirection::Next,
            }),
            ClientToDaemon::HookEvent(HookEvent {
                hook: "UserPromptSubmit".to_string(),
                matcher: None,
                session_id: Some("sess-abc123".to_string()),
                payload: serde_json::json!({"prompt": "do stuff"}),
            }),
        ];

        for msg in &messages {
            let json = serde_json::to_string(msg).unwrap();
            let parsed: ClientToDaemon = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&parsed).unwrap();
            assert_eq!(json, json2, "round-trip failed for {json}");
        }
    }

    #[test]
    fn round_trip_daemon_to_client() {
        let messages: Vec<DaemonToClient> = vec![
            DaemonToClient::Hello(HelloAck {
                protocol: PROTOCOL_VERSION,
                daemon_version: "0.1.0".to_string(),
            }),
            DaemonToClient::Render(RenderModel {
                agent_state: AgentState::Idle,
                armed: Some(ArmedPrompt {
                    prompt_id: "prep_pr".to_string(),
                    label: "PREP PR".to_string(),
                    command: "/runbook:prep-pr".to_string(),
                }),
                keypad: KeypadRender {
                    slots: vec![KeypadSlotRender {
                        slot: 0,
                        prompt_id: "prep_pr".to_string(),
                        label: "PREP PR".to_string(),
                        sublabel: Some("receipts".to_string()),
                        armed: true,
                    }],
                },
                page_index: 0,
                page_count: 2,
                hooks_connected: true,
            }),
            DaemonToClient::Notice(Notice {
                message: "hello".to_string(),
            }),
        ];

        for msg in &messages {
            let json = serde_json::to_string(msg).unwrap();
            let parsed: DaemonToClient = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&parsed).unwrap();
            assert_eq!(json, json2, "round-trip failed for {json}");
        }
    }

    #[test]
    fn pre_tool_use_deny_output_matches_spec() {
        let out = PreToolUseDecisionOutput::deny("rm -rf is blocked by policy");
        let json = serde_json::to_string_pretty(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();

        let hso = &v["hookSpecificOutput"];
        assert_eq!(hso["hookEventName"], "PreToolUse");
        assert_eq!(hso["permissionDecision"], "deny");
        assert!(hso["permissionDecisionReason"].as_str().unwrap().contains("rm -rf"));
    }

    #[test]
    fn pre_tool_use_allow_output_matches_spec() {
        let out = PreToolUseDecisionOutput::allow(Some("safe command"));
        let json = serde_json::to_string(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "allow");
    }

    #[test]
    fn user_prompt_submit_output_matches_spec() {
        let out = UserPromptSubmitOutput::with_context("git_branch=main");
        let json = serde_json::to_string(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "UserPromptSubmit");
        assert_eq!(
            v["hookSpecificOutput"]["additionalContext"],
            "git_branch=main"
        );
    }

    #[test]
    fn agent_state_default_is_unknown() {
        assert_eq!(AgentState::default(), AgentState::Unknown);
    }

    #[test]
    fn terminal_target_by_index_serializes() {
        let target = TerminalTarget::ByIndex(3);
        let json = serde_json::to_string(&target).unwrap();
        let parsed: TerminalTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, target);
    }
}
