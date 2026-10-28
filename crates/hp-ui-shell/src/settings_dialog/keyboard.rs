use egui::{Key, KeyboardShortcut, RichText, Ui};
use hp_app::actions::{Category, KeymapPreset, key_label, shortcut_label};
use hp_app::i18n::TrTable;
use hp_app::{Action, AppState, Severity, t, tf};

use super::SettingsDialog;
use crate::common::{CATEGORIES, category_label, heading, search_field, severity_color};
use crate::fuzzy::{Query, fold};
use crate::keys::{actions_bound_to, alias_text, conflicts, is_customized, shortcut_key, shortcut_search_text};

pub fn key_search_id() -> egui::Id {
    egui::Id::unique("hp_shell_settings_key_search")
}

const SHORTCUT_WIDTH: f32 = 170.0;

pub fn preset_label_key(preset: KeymapPreset) -> &'static str {
    match preset {
        KeymapPreset::HammerPro => "shell.keys.preset_hammerpro",
        KeymapPreset::Hammer => "shell.keys.preset_hammer",
        KeymapPreset::HammerPlusPlus => "shell.keys.preset_hammerpp",
    }
}

pub fn apply_keymap_preset(state: &mut AppState, preset: KeymapPreset) {
    state.settings.workflow.keymap_preset = preset;
    state.keys.set_preset(preset);
    state.settings.keybindings = state.keys.to_settings();
    state.settings_dirty = true;
}

fn pending_preset_id() -> egui::Id {
    egui::Id::unique("hp_shell_keys_pending_preset")
}

fn preset_row(ui: &mut Ui, state: &mut AppState) {
    let current = state.keys.preset();
    let mut pending: Option<KeymapPreset> = ui.data(|d| d.get_temp(pending_preset_id()));
    ui.horizontal(|ui| {
        ui.label(t!("shell.keys.preset"));
        egui::ComboBox::from_id_salt("hp_shell_keys_preset")
            .selected_text(t!(preset_label_key(current)))
            .width(160.0)
            .show_ui(ui, |ui| {
                for p in KeymapPreset::ALL {
                    if ui.selectable_label(p == current, t!(preset_label_key(p))).clicked() && p != current {
                        pending = Some(p);
                    }
                }
            })
            .response
            .on_hover_text(t!("shell.keys.preset_tip"));
    });
    if let Some(p) = pending {
        let mut keep = true;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(tf!("shell.keys.preset_confirm", t!(preset_label_key(p))));
            ui.horizontal(|ui| {
                if ui.button(t!("shell.keys.preset_apply")).clicked() {
                    apply_keymap_preset(state, p);
                    keep = false;
                }
                if ui.button(t!("common.cancel")).clicked() {
                    keep = false;
                }
            });
        });
        pending = keep.then_some(p);
    }
    ui.data_mut(|d| match pending {
        Some(p) => {
            d.insert_temp(pending_preset_id(), p);
        }
        None => {
            d.remove::<KeymapPreset>(pending_preset_id());
        }
    });
}

impl SettingsDialog {
    pub(super) fn keyboard_page(&mut self, ui: &mut Ui, state: &mut AppState) {
        preset_row(ui, state);
        ui.add_space(4.0);
        let row_height = ui.spacing().interact_size.y + 2.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_height),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if ui.button(t!("shell.keys.reset_all")).on_hover_text(t!("shell.keys.reset_all_tip")).clicked() {
                    reset_all_bindings(state);
                    self.capture = None;
                }
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), row_height),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        let focus = std::mem::take(&mut self.focus_key_search);
                        search_field(ui, key_search_id(), &mut self.key_filter, t!("shell.keys.search"), focus);
                    },
                );
            },
        );
        let conflict_map = conflicts(&state.keys);
        if !conflict_map.is_empty() {
            ui.label(
                RichText::new(tf!("shell.keys.conflicts", conflict_map.len()))
                    .color(severity_color(ui.visuals(), Severity::Warning)),
            );
        }
        ui.add_space(4.0);
        let query = Query::new(&self.key_filter);
        for cat in CATEGORIES {
            let actions: Vec<Action> = Action::ALL
                .iter()
                .copied()
                .filter(|a| a.category() == cat)
                .filter(|&a| {
                    query.is_empty()
                        || query
                            .best_of([
                                fold(a.label()).as_str(),
                                fold(a.id()).as_str(),
                                fold(&shortcut_search_text(&state.keys, a)).as_str(),
                            ])
                            .is_some()
                })
                .collect();
            if actions.is_empty() {
                continue;
            }
            heading(ui, category_label(cat));
            let label_width = binding_label_width(ui.available_width());
            egui::Grid::new(("hp_shell_keys", cat_index(cat)))
                .num_columns(3)
                .striped(true)
                .spacing([12.0, 3.0])
                .min_col_width(80.0)
                .show(ui, |ui| {
                    for action in actions {
                        self.binding_row(ui, state, action, &conflict_map, label_width);
                    }
                });
        }
    }

    fn binding_row(
        &mut self,
        ui: &mut Ui,
        state: &mut AppState,
        action: Action,
        conflict_map: &std::collections::HashMap<String, Vec<Action>>,
        label_width: f32,
    ) {
        let customized = is_customized(&state.keys, action);
        let mut label = RichText::new(action.label());
        if customized {
            label = label.strong();
        }
        let aliases = alias_text(&state.keys, action);
        let label_response = if aliases.is_empty() {
            crate::common::fixed_label(ui, label_width, 20.0, label)
        } else {
            let mut job = egui::text::LayoutJob::default();
            label.append_to(&mut job, ui.style(), egui::FontSelection::Default, egui::Align::Center);
            RichText::new(format!("  {}", tf!("shell.keys.also", aliases)))
                .small()
                .color(ui.visuals().weak_text_color())
                .append_to(&mut job, ui.style(), egui::FontSelection::Default, egui::Align::Center);
            crate::common::fixed_label(ui, label_width, 20.0, job)
        };
        label_response.on_hover_text(action.id());
        let capturing = self.capture == Some(action);
        if capturing {
            let held = hp_app::actions::modifiers_label(ui.input(|i| i.modifiers));
            let text = if held.is_empty() { t!("shell.keys.press").to_owned() } else { format!("{held}...") };
            if !held.is_empty() {
                ui.ctx().request_repaint();
            }
            let b = egui::Button::new(RichText::new(text).color(hp_app::theme::on_accent()))
                .fill(hp_app::theme::accent())
                .min_size(egui::vec2(SHORTCUT_WIDTH, 20.0));
            let r = ui.add(b);
            r.request_focus();
            if r.clicked() {
                self.capture = None;
            }
        } else {
            let sc = state.keys.shortcut(action);
            let key = sc.map(|s| shortcut_key(&s)).unwrap_or_default();
            let text = sc.map(|s| shortcut_label(&s)).unwrap_or_default();
            let others: Vec<Action> = conflict_map
                .get(&key)
                .map(|v| v.iter().copied().filter(|a| *a != action).collect())
                .unwrap_or_default();
            let mut rich = RichText::new(if text.is_empty() { "-".to_owned() } else { text.clone() }).monospace();
            if !others.is_empty() {
                rich = rich.color(severity_color(ui.visuals(), Severity::Error));
            } else if text.is_empty() {
                rich = rich.color(ui.visuals().weak_text_color());
            }
            let b = egui::Button::new(rich).min_size(egui::vec2(SHORTCUT_WIDTH, 20.0));
            let mut r = ui.add(b).on_hover_text(tf!("shell.keys.change_tip", key_label(Key::Escape)));
            if !others.is_empty() {
                let names: Vec<&str> = others.iter().map(|a| a.label()).collect();
                r = r.on_hover_text(tf!("shell.keys.conflict_with", names.join(", ")));
            }
            if r.clicked() {
                self.capture = Some(action);
            }
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let has = state.keys.shortcut(action).is_some();
            if ui.add_enabled(has, egui::Button::new("✖").small()).on_hover_text(t!("shell.keys.clear")).clicked() {
                set_binding(state, action, None);
                if capturing {
                    self.capture = None;
                }
            }
            if ui
                .add_enabled(customized, egui::Button::new("⟲").small())
                .on_hover_text(t!("shell.keys.reset"))
                .clicked()
            {
                state.keys.reset(action);
                state.settings.keybindings = state.keys.to_settings();
                state.settings_dirty = true;
                if capturing {
                    self.capture = None;
                }
            }
        });
        ui.end_row();
    }
}

fn binding_label_width(page_width: f32) -> f32 {
    let others = SHORTCUT_WIDTH + 52.0 + 2.0 * 12.0 + 4.0;
    (page_width - others).clamp(140.0, 320.0)
}

fn cat_index(cat: Category) -> usize {
    CATEGORIES.iter().position(|c| *c == cat).unwrap_or(0)
}

pub fn set_binding(state: &mut AppState, action: Action, shortcut: Option<KeyboardShortcut>) {
    state.keys.set(action, shortcut);
    state.settings.keybindings = state.keys.to_settings();
    state.settings_dirty = true;
}

pub fn apply_capture(state: &mut AppState, action: Action, sc: KeyboardShortcut) {
    let triggered = actions_bound_to(&state.keys, &sc);
    state.requested_actions.retain(|a| !triggered.contains(a));
    if let Some(tool) = triggered.iter().find_map(|a| a.tool())
        && state.requested_tool == Some(tool)
    {
        state.requested_tool = None;
    }
    set_binding(state, action, Some(sc));
}

pub fn reset_all_bindings(state: &mut AppState) {
    state.keys.reset_all();
    state.settings.keybindings.clear();
    state.settings_dirty = true;
}

pub(crate) const TR: TrTable = &[
    ("shell.keys.search", "Search actions or shortcuts", "Rechercher une action ou un raccourci"),
    ("shell.keys.reset_all", "Reset all shortcuts", "Réinitialiser tous les raccourcis"),
    ("shell.keys.reset_all_tip", "Restore every default shortcut", "Restaurer tous les raccourcis par défaut"),
    (
        "shell.keys.conflicts",
        "Shortcuts used by several actions: {0}",
        "Raccourcis utilisés par plusieurs actions : {0}",
    ),
    ("shell.keys.press", "Press a key combination...", "Appuyez sur une combinaison..."),
    (
        "shell.keys.change_tip",
        "Click, then press the new shortcut ({0} cancels)",
        "Cliquez puis appuyez sur le nouveau raccourci ({0} pour annuler)",
    ),
    ("shell.keys.conflict_with", "Also used by: {0}", "Aussi utilisé par : {0}"),
    ("shell.keys.clear", "Remove the shortcut", "Supprimer le raccourci"),
    ("shell.keys.reset", "Restore the default shortcut", "Restaurer le raccourci par défaut"),
    ("shell.keys.preset", "Shortcut set", "Jeu de raccourcis"),
    (
        "shell.keys.preset_tip",
        "Default shortcuts of Hammer Pro, Hammer or Hammer++. The shortcuts you changed stay.",
        "Raccourcis par défaut de Hammer Pro, de Hammer ou de Hammer++. Les raccourcis que vous avez modifiés sont conservés.",
    ),
    ("shell.keys.preset_hammerpro", "Hammer Pro", "Hammer Pro"),
    ("shell.keys.preset_hammer", "Hammer", "Hammer"),
    ("shell.keys.preset_hammerpp", "Hammer++", "Hammer++"),
    (
        "shell.keys.preset_confirm",
        "Use the {0} shortcuts? The shortcuts you changed stay.",
        "Utiliser les raccourcis de {0} ? Les raccourcis que vous avez modifiés sont conservés.",
    ),
    ("shell.keys.preset_apply", "Use these shortcuts", "Utiliser ces raccourcis"),
    ("shell.keys.also", "also {0}", "aussi {0}"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_apply_and_keep_user_shortcuts() {
        let mut state = hp_app::testing::test_state();
        set_binding(&mut state, Action::Undo, Some(KeyboardShortcut::new(egui::Modifiers::ALT, Key::Z)));
        apply_keymap_preset(&mut state, KeymapPreset::Hammer);
        assert_eq!(state.settings.workflow.keymap_preset, KeymapPreset::Hammer);
        assert_eq!(state.keys.preset(), KeymapPreset::Hammer);
        assert_eq!(state.keys.shortcut(Action::ToggleGrid).map(|s| shortcut_key(&s)).as_deref(), Some("Shift+R"));
        assert_eq!(state.settings.keybindings.get("undo").map(String::as_str), Some("Alt+Z"));
        assert!(state.settings_dirty);
        reset_all_bindings(&mut state);
        assert_eq!(state.keys.preset(), KeymapPreset::Hammer);
        assert_eq!(state.keys.shortcut(Action::Undo), Action::Undo.default_shortcut());
        assert_eq!(state.keys.shortcut(Action::ToggleGrid).map(|s| shortcut_key(&s)).as_deref(), Some("Shift+R"));
        for p in KeymapPreset::ALL {
            assert!(hp_app::i18n::has_key(preset_label_key(p)) || TR.iter().any(|(k, _, _)| *k == preset_label_key(p)));
        }
    }

    #[test]
    fn keyboard_page_renders_with_a_pending_preset() {
        let mut state = hp_app::testing::test_state();
        crate::register_translations();
        let mut dialog = SettingsDialog::default();
        hp_app::testing::run_ui(&mut state, |ui, state| {
            ui.data_mut(|d| d.insert_temp(pending_preset_id(), KeymapPreset::HammerPlusPlus));
            dialog.keyboard_page(ui, state);
        });
        assert_eq!(state.keys.preset(), KeymapPreset::HammerPro);
    }
}
