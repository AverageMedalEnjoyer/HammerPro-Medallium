use std::path::PathBuf;

use egui::{RichText, Ui};
use hp_app::settings::CustomDirSetting;
use hp_app::{AppState, Severity, t, tf};
use hp_fs::{CustomLayout, CustomMode};

use crate::jobs::{PickRequest, PickTarget, Picker};

#[derive(Debug, Default)]
pub struct GameJobs {
    picker: Option<Picker>,
    pub editor: crate::gameconfig_ui::ConfigEditor,
    pub config_field: Option<crate::gameconfig_ui::ConfigField>,
    pub config_pick: Option<(crate::gameconfig_ui::ConfigField, PathBuf)>,
}

impl GameJobs {
    pub fn poll(&mut self, state: &mut AppState) {
        if let Some(picker) = &self.picker
            && let Some(paths) = picker.poll()
        {
            let target = picker.target;
            self.picker = None;
            match target {
                PickTarget::ConfigFolder | PickTarget::ConfigFile => {
                    if let (Some(field), Some(path)) = (self.config_field.take(), paths.into_iter().next()) {
                        self.config_pick = Some((field, path));
                    }
                }
                PickTarget::ConfigAddFile => {
                    for path in paths {
                        if hp_app::games::add_config_file(&path) {
                            state.log(Severity::Info, tf!("shell.gc.saved", path.display()));
                        }
                    }
                }
                _ => apply_pick(state, target, paths),
            }
        }
    }

    pub fn picking(&self) -> bool {
        self.picker.is_some()
    }

    pub fn pick(&mut self, ctx: &egui::Context, req: PickRequest) {
        if self.picker.is_none() {
            self.picker = Some(Picker::open(ctx, req));
        }
    }
}

pub fn apply_pick(state: &mut AppState, target: PickTarget, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let game = &mut state.settings.game;
    let first = paths.first().cloned();
    match target {
        PickTarget::MapDir => game.map_dir = first,
        PickTarget::FgdOverride => game.fgd_override = first,
        PickTarget::ExtraGameDir => {
            for p in paths {
                if !game.extra_game_dirs.contains(&p) {
                    game.extra_game_dirs.push(p);
                }
            }
        }
        PickTarget::ExtraFgds => {
            for p in paths {
                if !game.extra_fgds.contains(&p) {
                    game.extra_fgds.push(p);
                }
            }
        }
        PickTarget::ConfigFolder | PickTarget::ConfigFile | PickTarget::ConfigAddFile => {}
        PickTarget::CustomTextureDirs => {
            for p in paths {
                if !game.custom_texture_dirs.iter().any(|d| d.path == p) {
                    let prefix = hp_fs::suggest_prefix(&p);
                    game.custom_texture_dirs.push(CustomDirSetting { path: p, prefix, enabled: true });
                }
            }
        }
    }
    state.settings_dirty = true;
}

pub fn request(target: PickTarget, start_dir: Option<PathBuf>) -> PickRequest {
    let (title, filter) = match target {
        PickTarget::ExtraGameDir => (t!("shell.game.pick_extra_dir"), None),
        PickTarget::MapDir => (t!("shell.game.pick_map_dir"), None),
        PickTarget::ConfigFolder | PickTarget::ConfigFile | PickTarget::ConfigAddFile => (t!("shell.gc.pick_folder"), None),
        PickTarget::CustomTextureDirs => (t!("shell.game.pick_custom_dir"), None),
        PickTarget::FgdOverride | PickTarget::ExtraFgds => {
            (t!("shell.game.pick_fgd"), Some((t!("shell.game.fgd_filter").to_owned(), vec!["fgd"])))
        }
    };
    PickRequest { target, title: title.to_owned(), start_dir, filter }
}

pub fn optional_path_edit(ui: &mut Ui, value: &mut Option<PathBuf>, hint: &str, width: f32) -> bool {
    let mut text = value.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
    let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text(hint).desired_width(width));
    if r.changed() {
        let trimmed = text.trim();
        *value = if trimmed.is_empty() { None } else { Some(PathBuf::from(trimmed)) };
        return true;
    }
    false
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PathRowOutput {
    pub changed: bool,
    pub browse: bool,
}

pub fn path_row(
    ui: &mut Ui,
    label: &str,
    label_width: Option<f32>,
    value: &mut Option<PathBuf>,
    hint: &str,
    browse_enabled: bool,
) -> (PathRowOutput, egui::Response) {
    let mut out = PathRowOutput::default();
    let row_height = ui.spacing().interact_size.y;
    let inner = ui.horizontal(|ui| {
        let label_response = match label_width {
            Some(w) => crate::common::fixed_label(ui, w, row_height, label),
            None => ui.label(label),
        };
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_height),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if value.is_some() && ui.small_button("✖").on_hover_text(t!("common.clear")).clicked() {
                    *value = None;
                    out.changed = true;
                }
                if ui.add_enabled(browse_enabled, egui::Button::new(t!("common.browse"))).clicked() {
                    out.browse = true;
                }
                let width = ui.available_width().max(120.0);
                out.changed |= optional_path_edit(ui, value, hint, width);
            },
        );
        label_response
    });
    (out, inner.inner)
}

pub fn mount_toggles(ui: &mut Ui, state: &mut AppState) -> bool {
    let g = &mut state.settings.game;
    let mut changed = false;
    changed |= ui
        .checkbox(&mut g.mount_addons, t!("shell.game.mount_addons"))
        .on_hover_text(t!("shell.game.mount_addons_tip"))
        .changed();
    changed |= ui
        .checkbox(&mut g.mount_workshop, t!("shell.game.mount_workshop"))
        .on_hover_text(t!("shell.game.mount_workshop_tip"))
        .changed();
    changed |= ui
        .checkbox(&mut g.mount_cfg, t!("shell.game.mount_cfg"))
        .on_hover_text(t!("shell.game.mount_cfg_tip"))
        .changed();
    if changed {
        state.settings_dirty = true;
    }
    changed
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrefixCell {
    Fixed { shown: String, mode: CustomMode },
    Edit { text: String, detected: bool },
}

pub fn prefix_cell(dir: &CustomDirSetting, layout: Option<&CustomLayout>) -> PrefixCell {
    match layout {
        Some(l) if matches!(l.mode, CustomMode::GameFolder | CustomMode::InsideMaterials) => {
            let shown = if l.prefix.is_empty() { "materials/".to_owned() } else { l.prefix.clone() };
            PrefixCell::Fixed { shown, mode: l.mode }
        }
        Some(l) if l.mode == CustomMode::Detected && is_auto_prefix(dir) => {
            PrefixCell::Edit { text: l.prefix.clone(), detected: true }
        }
        _ => PrefixCell::Edit { text: dir.prefix.clone(), detected: false },
    }
}

fn is_auto_prefix(dir: &CustomDirSetting) -> bool {
    hp_fs::is_auto_prefix(&hp_fs::CustomDir {
        path: dir.path.clone(),
        prefix: dir.prefix.clone(),
        enabled: dir.enabled,
    })
}

pub fn edited_prefix(original: &str, shown: &str, text: &str) -> String {
    if text == shown { original.to_owned() } else { text.to_owned() }
}

#[derive(Clone, Debug, Default)]
struct PrefixEdit {
    original: String,
    shown: String,
    text: String,
}

const PREFIX_WIDTH: f32 = 190.0;

fn prefix_cell_ui(ui: &mut Ui, dir: &mut CustomDirSetting, cell: &PrefixCell) -> bool {
    let size = egui::vec2(PREFIX_WIDTH, 20.0);
    let (text, detected) = match cell {
        PrefixCell::Fixed { shown, mode } => {
            let tip = match mode {
                CustomMode::GameFolder => t!("shell.game.prefix_mode_game"),
                _ => t!("shell.game.prefix_mode_inside"),
            };
            let label = egui::Label::new(RichText::new(shown).monospace().small()).truncate();
            ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(label).on_hover_text(tip);
            });
            return false;
        }
        PrefixCell::Edit { text, detected } => (text, *detected),
    };
    let mut changed = false;
    ui.allocate_ui_with_layout(size, egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if detected {
            ui.label(RichText::new(t!("shell.game.prefix_detected")).small().color(hp_app::theme::accent()))
                .on_hover_text(t!("shell.game.prefix_detected_tip"));
        }
        let id = ui.make_persistent_id(("hp_shell_custom_prefix", &dir.path));
        let focused = ui.memory(|m| m.has_focus(id));
        let mut edit = focused
            .then(|| ui.data(|d| d.get_temp::<PrefixEdit>(id)))
            .flatten()
            .unwrap_or_else(|| PrefixEdit { original: dir.prefix.clone(), shown: text.clone(), text: text.clone() });
        let hint = RichText::new(hp_fs::suggest_prefix(&dir.path)).monospace();
        let r = ui.add_sized(
            [ui.available_width().max(60.0), 20.0],
            egui::TextEdit::singleline(&mut edit.text).id(id).hint_text(hint),
        );
        if r.changed() {
            let prefix = edited_prefix(&edit.original, &edit.shown, &edit.text);
            if prefix != dir.prefix {
                dir.prefix = prefix;
                changed = true;
            }
        }
        if r.has_focus() {
            ui.data_mut(|d| d.insert_temp(id, edit));
        } else {
            ui.data_mut(|d| d.remove::<PrefixEdit>(id));
        }
    });
    changed
}

pub fn custom_dirs_list(ui: &mut Ui, state: &mut AppState, jobs: &mut GameJobs) {
    let ctx = ui.ctx().clone();
    let mut remove = None;
    let mut changed = false;
    let fs = state.assets.fs();
    let dirs = &mut state.settings.game.custom_texture_dirs;
    if dirs.is_empty() {
        ui.label(RichText::new(t!("shell.game.custom_empty")).small().color(ui.visuals().weak_text_color()));
    } else {
        let cells: Vec<PrefixCell> =
            dirs.iter().map(|d| prefix_cell(d, fs.and_then(|fs| fs.custom_layout_of_path(&d.path)).as_ref())).collect();
        egui::Grid::new("hp_shell_custom_dirs").num_columns(4).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(RichText::new(t!("common.enabled")).small().strong());
            ui.label(RichText::new(t!("common.path")).small().strong());
            ui.label(RichText::new(t!("shell.game.prefix")).small().strong())
                .on_hover_text(t!("shell.game.prefix_tip"));
            ui.label("");
            ui.end_row();
            for (i, (dir, cell)) in dirs.iter_mut().zip(&cells).enumerate() {
                changed |= ui.checkbox(&mut dir.enabled, "").changed();
                let text = dir.path.display().to_string();
                crate::common::fixed_label(ui, 260.0, 18.0, RichText::new(&text).monospace().small())
                    .on_hover_text(&text);
                changed |= prefix_cell_ui(ui, dir, cell);
                if ui.small_button("✖").on_hover_text(t!("common.remove")).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    }
    if let Some(i) = remove {
        dirs.remove(i);
        changed = true;
    }
    if changed {
        state.settings_dirty = true;
    }
    ui.horizontal(|ui| {
        if ui.add_enabled(!jobs.picking(), egui::Button::new(format!("➕ {}", t!("shell.game.add_custom")))).clicked()
        {
            jobs.pick(&ctx, request(PickTarget::CustomTextureDirs, None));
        }
        ui.label(RichText::new(t!("shell.game.custom_tip")).small().color(ui.visuals().weak_text_color()));
    });
}

pub fn path_list(ui: &mut Ui, id: &str, paths: &mut Vec<PathBuf>, empty_text: &str) -> (bool, bool) {
    let mut remove = None;
    if paths.is_empty() {
        ui.label(RichText::new(empty_text).small().color(ui.visuals().weak_text_color()));
    } else {
        egui::Grid::new(id).num_columns(2).striped(true).spacing([8.0, 2.0]).show(ui, |ui| {
            for (i, p) in paths.iter().enumerate() {
                let text = p.display().to_string();
                crate::common::fixed_label(ui, 420.0, 18.0, RichText::new(&text).monospace().small())
                    .on_hover_text(&text);
                if ui.small_button("✖").on_hover_text(t!("common.remove")).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    }
    let removed = remove.is_some();
    if let Some(i) = remove {
        paths.remove(i);
    }
    let add = ui.button(format!("➕ {}", t!("common.add"))).clicked();
    (removed, add)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_update_settings() {
        let mut state = hp_app::testing::test_state();
        apply_pick(&mut state, PickTarget::MapDir, vec![PathBuf::from("D:/Games/maps")]);
        assert_eq!(state.settings.game.map_dir, Some(PathBuf::from("D:/Games/maps")));
        assert!(state.settings_dirty);

        apply_pick(
            &mut state,
            PickTarget::CustomTextureDirs,
            vec![PathBuf::from("C:/Art/My Bricks"), PathBuf::from("C:/Art/My Bricks")],
        );
        assert_eq!(state.settings.game.custom_texture_dirs.len(), 1);
        assert_eq!(state.settings.game.custom_texture_dirs[0].prefix, "custom/my_bricks");
        assert!(state.settings.game.custom_texture_dirs[0].enabled);

        apply_pick(
            &mut state,
            PickTarget::ExtraFgds,
            vec![PathBuf::from("a.fgd"), PathBuf::from("b.fgd"), PathBuf::from("a.fgd")],
        );
        assert_eq!(state.settings.game.extra_fgds.len(), 2);
        state.settings_dirty = false;
        apply_pick(&mut state, PickTarget::MapDir, Vec::new());
        assert!(!state.settings_dirty);
        assert!(state.settings.game.map_dir.is_none());
    }

    #[test]
    fn prefix_cell_shows_the_effective_layout() {
        let dir =
            |prefix: &str| CustomDirSetting { path: "C:/Art/My Pack".into(), prefix: prefix.into(), enabled: true };
        let layout = |mode, prefix: &str| CustomLayout { mode, prefix: prefix.into() };
        let detected = layout(CustomMode::Detected, "mypack");
        let edit = |text: &str, detected| PrefixCell::Edit { text: text.into(), detected };
        assert_eq!(prefix_cell(&dir("custom/my_pack"), Some(&detected)), edit("mypack", true));
        assert_eq!(prefix_cell(&dir(""), Some(&detected)), edit("mypack", true));
        assert_eq!(prefix_cell(&dir("art/bricks"), Some(&detected)), edit("art/bricks", false));
        assert_eq!(prefix_cell(&dir("custom/my_pack"), None), edit("custom/my_pack", false));
        let prefixed = layout(CustomMode::Prefixed, "custom/my_pack");
        assert_eq!(prefix_cell(&dir("custom/my_pack"), Some(&prefixed)), edit("custom/my_pack", false));
        let game = layout(CustomMode::GameFolder, "");
        assert_eq!(
            prefix_cell(&dir("custom/my_pack"), Some(&game)),
            PrefixCell::Fixed { shown: "materials/".into(), mode: CustomMode::GameFolder }
        );
        let inside = layout(CustomMode::InsideMaterials, "custom/bricks");
        assert_eq!(
            prefix_cell(&dir("anything"), Some(&inside)),
            PrefixCell::Fixed { shown: "custom/bricks".into(), mode: CustomMode::InsideMaterials }
        );
    }

    #[test]
    fn editing_back_to_the_shown_prefix_keeps_it_automatic() {
        assert_eq!(edited_prefix("custom/my_pack", "mypack", "mypack2"), "mypack2");
        assert_eq!(edited_prefix("custom/my_pack", "mypack", "mypack"), "custom/my_pack");
        assert_eq!(edited_prefix("custom/my_pack", "mypack", ""), "");
        assert_eq!(edited_prefix("art", "art", "art/x"), "art/x");
    }

    #[test]
    fn requests_have_titles_and_filters() {
        crate::register_translations();
        let r = request(PickTarget::ExtraFgds, None);
        assert!(!r.title.is_empty());
        assert!(r.filter.is_some_and(|(_, e)| e == ["fgd"]));
        assert!(request(PickTarget::MapDir, None).filter.is_none());
    }
}
