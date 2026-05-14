//! Build the render model from daemon state + config.

use runbook_protocol::RenderModel;
use runbook_render::{
    build_render_model as build_render_model_core, GateDefinition, KeypadPage, KeypadSlot,
    PromptDefinition, RenderConfig, RenderState,
};

use crate::config::RunbookConfig;
use crate::state::DaemonState;

/// Build a `RenderModel` snapshot from the current state and config.
pub fn build_render_model(state: &DaemonState, config: &RunbookConfig) -> RenderModel {
    let render_config = RenderConfig {
        pages: config
            .keypad
            .pages
            .iter()
            .map(|page| KeypadPage {
                slots: page
                    .slots
                    .iter()
                    .map(|slot| KeypadSlot {
                        prompt_id: slot.prompt_id.clone(),
                        gate: slot.gate.clone(),
                    })
                    .collect(),
            })
            .collect(),
        prompts: config
            .prompts
            .iter()
            .map(|(prompt_id, prompt)| {
                (
                    prompt_id.clone(),
                    PromptDefinition {
                        label: prompt.label.clone(),
                        sublabel: prompt.sublabel.clone(),
                        style: config.arm_style_for(prompt_id),
                        claude_command: prompt.claude_command.clone(),
                        fallback_text: prompt.fallback_text.clone(),
                    },
                )
            })
            .collect(),
        gates: config
            .gates
            .iter()
            .map(|(gate_id, gate)| {
                (
                    gate_id.clone(),
                    GateDefinition {
                        label: gate.label.clone(),
                        sublabel: gate.sublabel.clone(),
                    },
                )
            })
            .collect(),
        is_claude_primary: config.is_claude_primary(),
    };

    let render_state = RenderState {
        page: state.page,
        armed: state.armed.clone(),
        agent_state: state.current_agent_state(),
        hooks_mode: state.hooks_mode,
    };

    build_render_model_core(&render_state, &render_config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::DaemonState;

    fn sample_config() -> RunbookConfig {
        let yaml = r#"
keypad:
  pages:
    - name: core
      slots:
        - prompt_id: prep_pr
        - {}
        - {}
        - {}
        - {}
        - {}
        - {}
        - {}
        - gate: pr
prompts:
  prep_pr:
    label: "PREP PR"
    sublabel: "receipts"
    claude_command: "/runbook:prep-pr"
gates:
  pr:
    label: "PR"
    sublabel: "jump"
    action: open_pr
"#;
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn render_model_shows_labels() {
        let config = sample_config();
        let state = DaemonState::new(0);
        let model = build_render_model(&state, &config);

        assert_eq!(model.keypad.slots.len(), 9);
        assert_eq!(model.keypad.slots[0].label, "PREP PR");
        assert_eq!(model.keypad.slots[0].sublabel.as_deref(), Some("receipts"));
        assert_eq!(model.keypad.slots[8].label, "PR");
    }

    #[test]
    fn render_model_shows_armed() {
        let config = sample_config();
        let mut state = DaemonState::new(0);
        state.armed = Some("prep_pr".to_string());

        let model = build_render_model(&state, &config);
        assert!(model.keypad.slots[0].armed);
        assert!(model.armed.is_some());
        assert_eq!(model.armed.as_ref().unwrap().prompt_id, "prep_pr");
    }

    #[test]
    fn render_model_page_metadata() {
        let config = sample_config();
        let state = DaemonState::new(0);
        let model = build_render_model(&state, &config);

        assert_eq!(model.page_index, 0);
        assert_eq!(model.page_count, 1);
        assert_eq!(model.hooks_mode, runbook_protocol::HooksMode::Absent);
    }
}
