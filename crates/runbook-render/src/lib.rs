use std::collections::HashMap;

use runbook_protocol::{
    AgentState, ArmStyle, ArmedPrompt, HooksMode, KeypadRender, KeypadSlotRender, RenderModel,
};

#[derive(Debug, Clone)]
pub struct RenderConfig {
    pub pages: Vec<KeypadPage>,
    pub prompts: HashMap<String, PromptDefinition>,
    pub gates: HashMap<String, GateDefinition>,
    pub is_claude_primary: bool,
}

#[derive(Debug, Clone)]
pub struct KeypadPage {
    pub slots: Vec<KeypadSlot>,
}

#[derive(Debug, Clone)]
pub struct KeypadSlot {
    pub prompt_id: Option<String>,
    pub gate: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PromptDefinition {
    pub label: String,
    pub sublabel: Option<String>,
    pub style: ArmStyle,
    pub claude_command: Option<String>,
    pub fallback_text: Option<String>,
}

impl PromptDefinition {
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

#[derive(Debug, Clone)]
pub struct GateDefinition {
    pub label: String,
    pub sublabel: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RenderState {
    pub page: usize,
    pub armed: Option<String>,
    pub agent_state: AgentState,
    pub hooks_mode: HooksMode,
}

pub fn build_render_model(state: &RenderState, config: &RenderConfig) -> RenderModel {
    let page_count = config.pages.len();
    let page_index = state.page.min(page_count.saturating_sub(1));
    let page_cfg = &config.pages[page_index];

    let slots: Vec<KeypadSlotRender> = page_cfg
        .slots
        .iter()
        .enumerate()
        .map(|(i, slot)| {
            let (prompt_id, label, sublabel) = if let Some(ref pid) = slot.prompt_id {
                if let Some(p) = config.prompts.get(pid) {
                    (pid.clone(), p.label.clone(), p.sublabel.clone())
                } else {
                    (pid.clone(), "???".to_string(), None)
                }
            } else if let Some(ref gid) = slot.gate {
                if let Some(g) = config.gates.get(gid) {
                    (gid.clone(), g.label.clone(), g.sublabel.clone())
                } else {
                    (gid.clone(), "???".to_string(), None)
                }
            } else {
                ("_empty".to_string(), "—".to_string(), None)
            };

            KeypadSlotRender {
                slot: i as u8,
                prompt_id,
                label,
                sublabel,
                armed: state.armed.as_deref() == slot.prompt_id.as_deref(),
            }
        })
        .collect();

    let armed = state.armed.as_ref().and_then(|pid| {
        config.prompts.get(pid).map(|p| ArmedPrompt {
            prompt_id: pid.clone(),
            label: p.label.clone(),
            style: p.style,
            command: p
                .effective_command(config.is_claude_primary)
                .unwrap_or("")
                .to_string(),
        })
    });

    RenderModel {
        agent_state: state.agent_state,
        armed,
        keypad: KeypadRender { slots },
        page_index,
        page_count,
        hooks_mode: state.hooks_mode,
    }
}

#[cfg(test)]
mod tests {
    use runbook_protocol::AgentState;

    use super::*;

    fn sample_config() -> RenderConfig {
        RenderConfig {
            pages: vec![KeypadPage {
                slots: vec![
                    KeypadSlot {
                        prompt_id: Some("prep_pr".to_string()),
                        gate: None,
                    },
                    KeypadSlot {
                        prompt_id: None,
                        gate: None,
                    },
                    KeypadSlot {
                        prompt_id: None,
                        gate: None,
                    },
                    KeypadSlot {
                        prompt_id: None,
                        gate: None,
                    },
                    KeypadSlot {
                        prompt_id: None,
                        gate: None,
                    },
                    KeypadSlot {
                        prompt_id: None,
                        gate: None,
                    },
                    KeypadSlot {
                        prompt_id: None,
                        gate: None,
                    },
                    KeypadSlot {
                        prompt_id: None,
                        gate: None,
                    },
                    KeypadSlot {
                        prompt_id: None,
                        gate: Some("pr".to_string()),
                    },
                ],
            }],
            prompts: HashMap::from([(
                "prep_pr".to_string(),
                PromptDefinition {
                    label: "PREP PR".to_string(),
                    sublabel: Some("receipts".to_string()),
                    style: ArmStyle::Prefill,
                    claude_command: Some("/runbook:prep-pr".to_string()),
                    fallback_text: None,
                },
            )]),
            gates: HashMap::from([(
                "pr".to_string(),
                GateDefinition {
                    label: "PR".to_string(),
                    sublabel: Some("jump".to_string()),
                },
            )]),
            is_claude_primary: true,
        }
    }

    #[test]
    fn render_model_shows_labels() {
        let config = sample_config();
        let state = RenderState {
            page: 0,
            armed: None,
            agent_state: AgentState::Unknown,
            hooks_mode: HooksMode::Absent,
        };
        let model = build_render_model(&state, &config);

        assert_eq!(model.keypad.slots.len(), 9);
        assert_eq!(model.keypad.slots[0].label, "PREP PR");
        assert_eq!(model.keypad.slots[0].sublabel.as_deref(), Some("receipts"));
        assert_eq!(model.keypad.slots[8].label, "PR");
    }

    #[test]
    fn render_model_shows_armed() {
        let config = sample_config();
        let state = RenderState {
            page: 0,
            armed: Some("prep_pr".to_string()),
            agent_state: AgentState::Unknown,
            hooks_mode: HooksMode::Absent,
        };

        let model = build_render_model(&state, &config);
        assert!(model.keypad.slots[0].armed);
        assert!(model.armed.is_some());
        assert_eq!(model.armed.as_ref().unwrap().prompt_id, "prep_pr");
    }
}
