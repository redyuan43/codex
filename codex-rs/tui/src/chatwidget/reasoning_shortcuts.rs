//! Keyboard shortcuts for stepping the active model's reasoning effort.
//!
//! The main chat surface treats `Alt+,` and `Alt+.` as small adjustments to the
//! current model configuration. This module keeps that behavior separate from
//! the larger `ChatWidget` key dispatcher while still reusing the same
//! model-selection and Plan-mode scope paths as the settings popups.
//!
//! The shortcut state machine is deliberately narrow: it only handles key
//! presses when no modal or popup owns input, it anchors unset reasoning to the
//! current model preset's default, and it walks only efforts advertised by the
//! active model. Unsupported efforts anchor to the model default, or the first
//! advertised effort when the default is absent, before stepping through the
//! advertised order.

use codex_protocol::config_types::ModeKind;
use codex_protocol::openai_models::ModelPreset;
use codex_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use crossterm::event::KeyEvent;

use super::ChatWidget;
use crate::app_event::AppEvent;
use crate::key_hint::KeyBindingListExt;

/// Direction requested by a reasoning-level shortcut.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReasoningShortcutDirection {
    Lower,
    Raise,
}

impl ReasoningShortcutDirection {
    fn bound_message(self, _effort: &ReasoningEffortConfig) -> String {
        match self {
            Self::Lower => "推理现在已经是最弱了。".to_string(),
            Self::Raise => "推理现在已经是最强了。".to_string(),
        }
    }

    fn changed_message(self) -> String {
        match self {
            Self::Lower => "模型现在变弱了。".to_string(),
            Self::Raise => "模型现在有变强了。".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ModelShortcutDirection {
    Cheaper,
    Stronger,
}

impl ModelShortcutDirection {
    fn bound_message(self, _model: &str) -> String {
        match self {
            Self::Cheaper => "模型现在已经是最低级别了。".to_string(),
            Self::Stronger => "模型现在已经是最高级别了。".to_string(),
        }
    }

    fn changed_message(self) -> String {
        match self {
            Self::Cheaper => "模型现在级别变低了。".to_string(),
            Self::Stronger => "模型现在级别变高了。".to_string(),
        }
    }
}

impl ChatWidget {
    /// Handles main-surface reasoning shortcuts before general key dispatch.
    ///
    /// Returning `true` means the key was recognized as a reasoning shortcut and
    /// fully handled, even if handling only produced an informational message at
    /// a boundary. Returning `false` leaves the key available to the normal chat
    /// input flow, which is important while a popup or modal has focus.
    ///
    /// Callers should route recognized shortcuts through this method rather than
    /// directly mutating reasoning state. It applies normal-mode changes without
    /// persisting them. In Plan mode, shortcuts apply only to the active
    /// Plan-mode override and skip the global-vs-Plan scope prompt.
    pub(super) fn handle_reasoning_shortcut(&mut self, key_event: KeyEvent) -> bool {
        let direction = if self
            .chat_keymap
            .decrease_reasoning_effort
            .is_pressed(key_event)
        {
            ReasoningShortcutDirection::Lower
        } else if self
            .chat_keymap
            .increase_reasoning_effort
            .is_pressed(key_event)
        {
            ReasoningShortcutDirection::Raise
        } else {
            return false;
        };

        if !self.bottom_pane.no_modal_or_popup_active() {
            return false;
        }

        if !self.is_session_configured() {
            self.add_info_message(
                "Reasoning shortcuts are disabled until startup completes.".to_string(),
                /*hint*/ None,
            );
            return true;
        }

        let current_model = self.current_model().to_string();
        let Some(preset) = self.current_model_preset() else {
            self.add_info_message(
                format!("Reasoning shortcuts are unavailable for {current_model}."),
                /*hint*/ None,
            );
            return true;
        };

        let choices = reasoning_choices(&preset);
        let configured_effort = self
            .effective_reasoning_effort()
            .unwrap_or_else(|| preset.default_reasoning_effort.clone());
        let current_effort = if choices.contains(&configured_effort) {
            configured_effort
        } else if choices.contains(&preset.default_reasoning_effort) {
            preset.default_reasoning_effort
        } else {
            choices
                .first()
                .cloned()
                .unwrap_or(preset.default_reasoning_effort)
        };
        let Some(next_effort) =
            next_reasoning_effort(&choices, Some(current_effort.clone()), direction)
        else {
            self.add_info_message(direction.bound_message(&current_effort), /*hint*/ None);
            return true;
        };

        if self.collaboration_modes_enabled() && self.active_mode_kind() == ModeKind::Plan {
            self.app_event_tx
                .send(AppEvent::UpdatePlanModeReasoningEffort(Some(next_effort)));
        } else {
            self.apply_model_and_effort_without_persist(current_model, Some(next_effort));
        }

        true
    }

    pub(super) fn handle_reasoning_slash_command(&mut self, direction: ReasoningShortcutDirection) {
        if !self.is_session_configured() {
            self.add_info_message(
                "Reasoning shortcuts are disabled until startup completes.".to_string(),
                /*hint*/ None,
            );
            return;
        }

        let current_model = self.current_model().to_string();
        let Some(preset) = self.current_model_preset() else {
            self.add_info_message(
                format!("Reasoning shortcuts are unavailable for {current_model}."),
                /*hint*/ None,
            );
            return;
        };

        let choices = reasoning_choices(&preset);
        let current_effort = self
            .effective_reasoning_effort()
            .unwrap_or(preset.default_reasoning_effort);
        let Some(next_effort) =
            next_reasoning_effort(&choices, Some(current_effort.clone()), direction)
        else {
            self.add_info_message(direction.bound_message(&current_effort), /*hint*/ None);
            return;
        };

        self.apply_model_and_effort_for_all_modes_with_message(
            current_model,
            Some(next_effort),
            direction.changed_message(),
        );
    }

    pub(super) fn handle_model_slash_command(&mut self, direction: ModelShortcutDirection) {
        if !self.is_session_configured() {
            self.add_info_message(
                "Model shortcuts are disabled until startup completes.".to_string(),
                /*hint*/ None,
            );
            return;
        }

        let presets = match self.model_catalog.try_list_models() {
            Ok(models) => concrete_model_presets(models),
            Err(_) => {
                self.add_info_message(
                    "Models are being updated; please try again in a moment.".to_string(),
                    /*hint*/ None,
                );
                return;
            }
        };
        let current_model = self.current_model().to_string();
        let Some(current_idx) = presets
            .iter()
            .position(|preset| preset.model == current_model)
        else {
            self.add_info_message(
                format!("Model shortcuts are unavailable for {current_model}."),
                /*hint*/ None,
            );
            return;
        };
        let Some(next_idx) = next_model_index(&presets, current_idx, direction) else {
            self.add_info_message(direction.bound_message(&current_model), /*hint*/ None);
            return;
        };

        let next_preset = &presets[next_idx];
        let requested_effort = self.effective_reasoning_effort().or_else(|| {
            self.current_model_preset()
                .map(|preset| preset.default_reasoning_effort)
        });
        let next_effort = nearest_supported_effort(next_preset, requested_effort)
            .or(Some(next_preset.default_reasoning_effort.clone()));
        self.apply_model_and_effort_for_all_modes_with_message(
            next_preset.model.clone(),
            next_effort,
            direction.changed_message(),
        );
    }

    fn current_model_preset(&self) -> Option<ModelPreset> {
        let current_model = self.current_model();
        self.model_catalog
            .try_list_models()
            .ok()?
            .into_iter()
            .find(|preset| preset.model == current_model)
    }
}

fn concrete_model_presets(models: Vec<ModelPreset>) -> Vec<ModelPreset> {
    models
        .into_iter()
        .filter(|preset| preset.show_in_picker && !preset.model.starts_with("codex-auto-"))
        .collect()
}

fn next_model_index(
    presets: &[ModelPreset],
    current_idx: usize,
    direction: ModelShortcutDirection,
) -> Option<usize> {
    match direction {
        ModelShortcutDirection::Cheaper => current_idx
            .checked_add(1)
            .filter(|idx| *idx < presets.len()),
        ModelShortcutDirection::Stronger => current_idx.checked_sub(1),
    }
}

fn reasoning_choices(preset: &ModelPreset) -> Vec<ReasoningEffortConfig> {
    let mut choices: Vec<ReasoningEffortConfig> = preset
        .supported_reasoning_efforts
        .iter()
        .map(|option| option.effort.clone())
        .collect();
    if choices.is_empty() {
        choices.push(preset.default_reasoning_effort.clone());
    }
    choices
}

fn next_reasoning_effort(
    choices: &[ReasoningEffortConfig],
    current_effort: Option<ReasoningEffortConfig>,
    direction: ReasoningShortcutDirection,
) -> Option<ReasoningEffortConfig> {
    let current_effort = current_effort?;
    if let Some(current_index) = choices.iter().position(|choice| choice == &current_effort) {
        return match direction {
            ReasoningShortcutDirection::Lower => current_index
                .checked_sub(1)
                .and_then(|index| choices.get(index))
                .cloned(),
            ReasoningShortcutDirection::Raise => choices.get(current_index + 1).cloned(),
        };
    }

    None
}

fn nearest_supported_effort(
    preset: &ModelPreset,
    requested_effort: Option<ReasoningEffortConfig>,
) -> Option<ReasoningEffortConfig> {
    let requested_effort = requested_effort?;
    let choices = reasoning_choices(preset);
    let requested_rank = effort_rank(&requested_effort);
    choices
        .into_iter()
        .min_by_key(|choice| (effort_rank(choice) - requested_rank).abs())
}

fn effort_rank(effort: &ReasoningEffortConfig) -> i32 {
    match effort {
        ReasoningEffortConfig::None => 0,
        ReasoningEffortConfig::Minimal => 1,
        ReasoningEffortConfig::Low => 2,
        ReasoningEffortConfig::Medium => 3,
        ReasoningEffortConfig::High => 4,
        ReasoningEffortConfig::XHigh => 5,
        ReasoningEffortConfig::Custom(_) => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn next_reasoning_effort_raises_from_default_anchor() {
        let choices = vec![
            ReasoningEffortConfig::Low,
            ReasoningEffortConfig::Medium,
            ReasoningEffortConfig::High,
            ReasoningEffortConfig::XHigh,
        ];

        assert_eq!(
            next_reasoning_effort(
                &choices,
                Some(ReasoningEffortConfig::Medium),
                ReasoningShortcutDirection::Raise,
            ),
            Some(ReasoningEffortConfig::High)
        );
    }

    #[test]
    fn next_reasoning_effort_lowers_from_default_anchor() {
        let choices = vec![
            ReasoningEffortConfig::Low,
            ReasoningEffortConfig::Medium,
            ReasoningEffortConfig::High,
        ];

        assert_eq!(
            next_reasoning_effort(
                &choices,
                Some(ReasoningEffortConfig::Medium),
                ReasoningShortcutDirection::Lower,
            ),
            Some(ReasoningEffortConfig::Low)
        );
    }

    #[test]
    fn next_reasoning_effort_does_not_infer_position_for_unsupported_current() {
        let choices = vec![ReasoningEffortConfig::Low, ReasoningEffortConfig::High];

        assert_eq!(
            (
                next_reasoning_effort(
                    &choices,
                    Some(ReasoningEffortConfig::Medium),
                    ReasoningShortcutDirection::Raise,
                ),
                next_reasoning_effort(
                    &choices,
                    Some(ReasoningEffortConfig::Medium),
                    ReasoningShortcutDirection::Lower,
                ),
            ),
            (None, None)
        );
    }

    #[test]
    fn next_reasoning_effort_uses_advertised_order_for_custom_levels() {
        let custom_effort = ReasoningEffortConfig::Custom("max".to_string());
        let choices = vec![
            ReasoningEffortConfig::High,
            ReasoningEffortConfig::Low,
            custom_effort.clone(),
        ];

        assert_eq!(
            (
                next_reasoning_effort(
                    &choices,
                    Some(ReasoningEffortConfig::High),
                    ReasoningShortcutDirection::Raise,
                ),
                next_reasoning_effort(
                    &choices,
                    Some(custom_effort),
                    ReasoningShortcutDirection::Lower,
                ),
            ),
            (
                Some(ReasoningEffortConfig::Low),
                Some(ReasoningEffortConfig::Low),
            )
        );
    }

    #[test]
    fn next_reasoning_effort_clamps_at_bounds() {
        let choices = vec![
            ReasoningEffortConfig::Low,
            ReasoningEffortConfig::Medium,
            ReasoningEffortConfig::High,
        ];

        assert_eq!(
            next_reasoning_effort(
                &choices,
                Some(ReasoningEffortConfig::Low),
                ReasoningShortcutDirection::Lower,
            ),
            None
        );
        assert_eq!(
            next_reasoning_effort(
                &choices,
                Some(ReasoningEffortConfig::High),
                ReasoningShortcutDirection::Raise,
            ),
            None
        );
    }

    #[test]
    fn next_reasoning_effort_single_option_is_noop() {
        let choices = vec![ReasoningEffortConfig::High];

        assert_eq!(
            next_reasoning_effort(
                &choices,
                Some(ReasoningEffortConfig::High),
                ReasoningShortcutDirection::Raise,
            ),
            None
        );
        assert_eq!(
            next_reasoning_effort(
                &choices,
                Some(ReasoningEffortConfig::High),
                ReasoningShortcutDirection::Lower,
            ),
            None
        );
    }
}
