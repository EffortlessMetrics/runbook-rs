//! Build the render model from daemon state + config.

use runbook_protocol::{ArmedPrompt, KeypadRender, KeypadSlotRender, RenderModel};

use crate::config::RunbookConfig;
use crate::state::DaemonState;

/// Build a `RenderModel` snapshot from the current state and config.
pub fn build_render_model(state: &DaemonState, config: &RunbookConfig) -> RenderModel {
    let page_count = config.keypad.pages.len();
    let page_index = state.page.min(page_count.saturating_sub(1));
    let slots_source = config
        .keypad
        .pages
        .get(page_index)
        .map_or(&[][..], |page| page.slots.as_slice());

    let slots: Vec<KeypadSlotRender> = slots_source
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
                slot: u8::try_from(i).unwrap_or(u8::MAX),
                prompt_id,
                label,
                sublabel,
                armed: state.armed.as_deref() == slot.prompt_id.as_deref(),
            }
        })
        .collect();

    let armed = state.armed.as_ref().and_then(|pid| {
        config.prompts.get(pid).map(|p| {
            let is_claude = config.is_claude_primary();
            ArmedPrompt {
                prompt_id: pid.clone(),
                label: p.label.clone(),
                style: config.arm_style_for(pid),
                command: p.effective_command(is_claude).unwrap_or("").to_string(),
            }
        })
    });

    RenderModel {
        agent_state: state.current_agent_state(),
        armed,
        keypad: KeypadRender { slots },
        page_index,
        page_count,
        hooks_mode: state.hooks_mode,
    }
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
        serde_yaml::from_str(yaml).unwrap_or_default()
    }

    #[test]
    fn render_model_shows_labels() {
        let config = sample_config();
        let state = DaemonState::new(0);
        let model = build_render_model(&state, &config);

        assert_eq!(model.keypad.slots.len(), 9);
        assert_eq!(
            model.keypad.slots.first().map(|slot| slot.label.as_str()),
            Some("PREP PR")
        );
        assert_eq!(
            model
                .keypad
                .slots
                .first()
                .and_then(|slot| slot.sublabel.as_deref()),
            Some("receipts")
        );
        assert_eq!(
            model.keypad.slots.get(8).map(|slot| slot.label.as_str()),
            Some("PR")
        );
    }

    #[test]
    fn render_model_shows_armed() {
        let config = sample_config();
        let mut state = DaemonState::new(0);
        state.armed = Some("prep_pr".to_string());

        let model = build_render_model(&state, &config);
        assert!(model.keypad.slots.first().is_some_and(|slot| slot.armed));
        assert!(model.armed.is_some());
        assert_eq!(
            model.armed.as_ref().map(|armed| armed.prompt_id.as_str()),
            Some("prep_pr")
        );
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
