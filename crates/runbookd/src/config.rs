use std::collections::HashMap;

use serde::Deserialize;

use runbook_protocol::DialMode;

/// Top-level config loaded from `runbook.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct RunbookConfig {
    /// Schema version (must be 1).
    #[serde(default = "default_version")]
    pub version: u32,

    #[serde(default)]
    pub daemon: DaemonConfig,

    #[serde(default)]
    pub tooling: ToolingConfig,

    #[serde(default)]
    pub dial: DialConfig,

    pub keypad: KeypadConfig,

    /// Named prompt templates, keyed by prompt_id.
    #[serde(default)]
    pub prompts: HashMap<String, PromptConfig>,

    /// Named jump gates, keyed by gate id.
    #[serde(default)]
    pub gates: HashMap<String, GateConfig>,

    #[serde(default)]
    pub policy: PolicyConfig,
}

fn default_version() -> u32 {
    1
}

// ---------------------------------------------------------------------------
// Daemon
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
}

fn default_listen() -> String {
    "127.0.0.1:29381".to_string()
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            listen: default_listen(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tooling
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ToolingConfig {
    /// Which tool integration is primary: "claude_code" or "other".
    #[serde(default = "default_primary")]
    pub primary: String,

    /// Label shown on device when running in degraded (non-hook) mode.
    #[serde(default = "default_degraded_label")]
    pub degraded_mode_label: String,
}

fn default_primary() -> String {
    "claude_code".to_string()
}

fn default_degraded_label() -> String {
    "KEYSTROKE MODE".to_string()
}

impl Default for ToolingConfig {
    fn default() -> Self {
        Self {
            primary: default_primary(),
            degraded_mode_label: default_degraded_label(),
        }
    }
}

// ---------------------------------------------------------------------------
// Dial
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct DialConfig {
    /// `os_scroll` (default) or `vscode_terminal_scroll`.
    #[serde(default)]
    pub mode: DialMode,
}

impl Default for DialConfig {
    fn default() -> Self {
        Self {
            mode: DialMode::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Keypad
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct KeypadConfig {
    pub pages: Vec<KeypadPageConfig>,

    #[serde(default)]
    pub initial_page: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeypadPageConfig {
    pub name: String,
    pub slots: Vec<KeypadSlotConfig>,
}

/// A slot on the keypad. Exactly one of `prompt_id` or `gate` should be set.
#[derive(Debug, Clone, Deserialize)]
pub struct KeypadSlotConfig {
    /// Which prompt this slot arms (references `prompts.<id>`).
    #[serde(default)]
    pub prompt_id: Option<String>,

    /// Which gate this slot opens (references `gates.<id>`).
    #[serde(default)]
    pub gate: Option<String>,
}

// ---------------------------------------------------------------------------
// Prompts
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct PromptConfig {
    /// What to show on the LCD key.
    pub label: String,

    /// Optional second line.
    #[serde(default)]
    pub sublabel: Option<String>,

    /// Claude Code slash command (used when tooling.primary == "claude_code").
    #[serde(default)]
    pub claude_command: Option<String>,

    /// Fallback text dispatched when hooks are not available.
    #[serde(default)]
    pub fallback_text: Option<String>,
}

impl PromptConfig {
    /// Returns the command to dispatch based on the tooling mode.
    pub fn effective_command(&self, is_claude: bool) -> Option<&str> {
        if is_claude {
            self.claude_command
                .as_deref()
                .or(self.fallback_text.as_deref())
        } else {
            self.fallback_text
                .as_deref()
                .or(self.claude_command.as_deref())
        }
    }
}

// ---------------------------------------------------------------------------
// Gates
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct GateConfig {
    pub label: String,

    #[serde(default)]
    pub sublabel: Option<String>,

    /// Action to invoke (e.g. "open_pr", "open_issue", "open_receipt").
    pub action: String,
}

// ---------------------------------------------------------------------------
// Policy
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PolicyConfig {
    #[serde(default)]
    pub pre_tool_use: PreToolUsePolicy,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PreToolUsePolicy {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub bash: BashPolicy,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BashPolicy {
    /// Patterns that will DENY a Bash tool call.
    #[serde(default)]
    pub deny: Vec<String>,

    /// Patterns that will unconditionally ALLOW a Bash tool call.
    #[serde(default)]
    pub allow: Vec<String>,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

impl RunbookConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.keypad.pages.is_empty() {
            anyhow::bail!("keypad.pages must have at least 1 page");
        }
        for (pi, p) in self.keypad.pages.iter().enumerate() {
            if p.slots.len() != 9 {
                anyhow::bail!(
                    "keypad.pages[{pi}] '{name}' must have exactly 9 slots (3x3 keypad). Got {n}.",
                    name = p.name,
                    n = p.slots.len()
                );
            }
            // Validate references.
            for (si, slot) in p.slots.iter().enumerate() {
                if let Some(ref pid) = slot.prompt_id {
                    if !self.prompts.contains_key(pid) {
                        anyhow::bail!(
                            "keypad.pages[{pi}].slots[{si}].prompt_id '{pid}' \
                             references unknown prompt"
                        );
                    }
                }
                if let Some(ref gid) = slot.gate {
                    if !self.gates.contains_key(gid) {
                        anyhow::bail!(
                            "keypad.pages[{pi}].slots[{si}].gate '{gid}' \
                             references unknown gate"
                        );
                    }
                }
                if slot.prompt_id.is_none() && slot.gate.is_none() {
                    // Empty slot is allowed (noop key).
                }
            }
        }
        Ok(())
    }

    /// Returns true when the primary tooling is Claude Code.
    pub fn is_claude_primary(&self) -> bool {
        self.tooling.primary == "claude_code"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_YAML: &str = r#"
version: 1

daemon:
  listen: "127.0.0.1:29381"

tooling:
  primary: claude_code
  degraded_mode_label: "KEYSTROKE MODE"

keypad:
  initial_page: 0
  pages:
    - name: core
      slots:
        - prompt_id: prep_pr
        - prompt_id: break_task
        - prompt_id: run_gates
        - prompt_id: write_receipt
        - {}
        - {}
        - gate: pr
        - gate: issue
        - gate: receipt

prompts:
  prep_pr:
    label: "PREP PR"
    sublabel: "receipts"
    claude_command: "/runbook:prep-pr"
    fallback_text: "Prep a PR. Include summary, risks, test plan."
  break_task:
    label: "BREAK TASK"
    sublabel: "plan"
    claude_command: "/runbook:break-task"
    fallback_text: "Break the task into steps and list acceptance criteria."
  run_gates:
    label: "RUN GATES"
    sublabel: "tests"
    claude_command: "/runbook:run-gates"
    fallback_text: "Run the quality gates."
  write_receipt:
    label: "RECEIPT"
    sublabel: "summary"
    claude_command: "/runbook:write-receipt"
    fallback_text: "Write a session receipt."

gates:
  pr:
    label: "PR"
    sublabel: "jump"
    action: open_pr
  issue:
    label: "ISSUE"
    sublabel: "jump"
    action: open_issue
  receipt:
    label: "RECEIPT"
    sublabel: "summary"
    action: open_receipt

policy:
  pre_tool_use:
    enabled: true
    bash:
      deny:
        - "rm -rf"
        - "git push --force"
        - "git reset --hard"
      allow:
        - "git status"
        - "rg "
        - "cargo test"
"#;

    #[test]
    fn parse_sample_config() {
        let cfg: RunbookConfig = serde_yaml::from_str(SAMPLE_YAML).unwrap();
        assert_eq!(cfg.version, 1);
        assert_eq!(cfg.keypad.pages.len(), 1);
        assert_eq!(cfg.keypad.pages[0].slots.len(), 9);
        assert_eq!(cfg.prompts.len(), 4);
        assert_eq!(cfg.gates.len(), 3);
        assert!(cfg.policy.pre_tool_use.enabled);
        assert_eq!(cfg.policy.pre_tool_use.bash.deny.len(), 3);
    }

    #[test]
    fn validate_sample_config() {
        let cfg: RunbookConfig = serde_yaml::from_str(SAMPLE_YAML).unwrap();
        cfg.validate().unwrap();
    }

    #[test]
    fn effective_command_claude_mode() {
        let cfg: RunbookConfig = serde_yaml::from_str(SAMPLE_YAML).unwrap();
        let prompt = &cfg.prompts["prep_pr"];
        assert_eq!(
            prompt.effective_command(true),
            Some("/runbook:prep-pr")
        );
    }

    #[test]
    fn effective_command_degraded_mode() {
        let cfg: RunbookConfig = serde_yaml::from_str(SAMPLE_YAML).unwrap();
        let prompt = &cfg.prompts["prep_pr"];
        assert_eq!(
            prompt.effective_command(false),
            Some("Prep a PR. Include summary, risks, test plan.")
        );
    }

    #[test]
    fn validate_bad_prompt_ref() {
        let yaml = r#"
keypad:
  pages:
    - name: test
      slots:
        - prompt_id: nonexistent
        - {}
        - {}
        - {}
        - {}
        - {}
        - {}
        - {}
        - {}
"#;
        let cfg: RunbookConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.validate().is_err());
    }
}
