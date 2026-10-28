use egui::{DragValue, Id, RichText, Ui};
use hp_app::{AppState, Severity, t, tf};
use hp_editor::max_distance::{DistanceKind, MAX_DISTANCE, MaxDistanceReport, edits, rule_for};
use hp_fgd::ResolvedFgd;

use crate::view::SelectionSummary;

pub const UNDO_LABEL: &str = "Set maximum distance";
const DEFAULT_DISTANCE: f64 = 2048.0;
const MAX_NAMED_CLASSES: usize = 6;

pub fn fgd_has_key(fgd: Option<&ResolvedFgd>, class: &str, key: &str) -> Option<bool> {
    fgd.and_then(|f| f.class(class)).map(|c| c.property(key).is_some())
}

pub fn set_max_distance(
    state: &mut AppState,
    ids: &[hp_editor::Id],
    d: f64,
    label: &str,
) -> hp_editor::EditResult<MaxDistanceReport> {
    let assets = &state.assets;
    state.doc.set_max_distance(ids, d, label, &|c, k| fgd_has_key(assets.fgd(), c, k))
}

pub fn report_text(r: &MaxDistanceReport) -> String {
    let mut text = tf!("entities.maxdist_done", r.changed.len(), r.unchanged.len());
    if !r.skipped.is_empty() {
        let mut names = r.skipped_classes.iter().take(MAX_NAMED_CLASSES).cloned().collect::<Vec<_>>().join(", ");
        if r.skipped_classes.len() > MAX_NAMED_CLASSES {
            names.push_str(&tf!("entities.and_more", r.skipped_classes.len() - MAX_NAMED_CLASSES));
        }
        text.push(' ');
        text.push_str(&tf!("entities.maxdist_skipped", r.skipped.len(), names));
    }
    text
}

pub(crate) fn key_note(class: &str, key: &str) -> Option<String> {
    Some(
        match hp_editor::max_distance::is_distance_key(class, key)? {
            DistanceKind::LightCutoff => t!("entities.maxdist_note_light"),
            DistanceKind::LightLength => t!("entities.maxdist_note_length"),
            DistanceKind::ViewFade if class.trim().eq_ignore_ascii_case("env_detail_controller") => {
                t!("entities.maxdist_note_detail")
            }
            DistanceKind::ViewFade => t!("entities.maxdist_note_fade"),
            DistanceKind::GlowFade => t!("entities.maxdist_note_glow"),
            DistanceKind::BrushFade => t!("entities.maxdist_note_lod"),
        }
        .to_owned(),
    )
}

pub(crate) struct MaxDistanceWindow {
    open: bool,
    distance: f64,
}

impl Default for MaxDistanceWindow {
    fn default() -> Self {
        Self { open: false, distance: DEFAULT_DISTANCE }
    }
}

fn preview(state: &AppState, fgd: Option<&ResolvedFgd>, ids: &[hp_editor::Id], d: f64) -> (usize, usize, usize) {
    let scene = state.doc.scene();
    let mut out = (0, 0, 0);
    for id in ids {
        let Some(node) = scene.entity(*id) else { continue };
        let e = &node.entity;
        match rule_for(&e.classname, &|k| fgd_has_key(fgd, &e.classname, k)) {
            None => out.2 += 1,
            Some(rule) if edits(e, &rule, d).is_empty() => out.1 += 1,
            Some(_) => out.0 += 1,
        }
    }
    out
}

impl MaxDistanceWindow {
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState, fgd: Option<&ResolvedFgd>, focus_before: bool) {
        if std::mem::take(&mut state.runtime.entities.set_max_distance) {
            self.open = true;
        }
        if !self.open {
            return;
        }
        let fgd = fgd.or_else(|| state.assets.fgd());
        let ids = SelectionSummary::of(&state.doc).entities;
        let mut open = true;
        let mut apply = false;
        let resp = hp_app::chrome::Window::new(t!("entities.maxdist_title"))
            .id(Id::unique("hp_ui_entities.max_distance_window"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(360.0)
            .show(ctx, |ui| apply = self.body(ui, state, fgd, &ids));
        if crate::dialogs::escape_closes(ctx, resp.as_ref().map(|r| r.response.layer_id), focus_before) {
            open = false;
        }
        if apply {
            let d = self.distance;
            let fgd_has = |c: &str, k: &str| fgd_has_key(fgd, c, k);
            match state.doc.set_max_distance(&ids, d, UNDO_LABEL, &fgd_has) {
                Ok(r) => {
                    state.notify(if r.changed.is_empty() { Severity::Info } else { Severity::Success }, report_text(&r))
                }
                Err(e) => state.notify(Severity::Error, tf!("entities.edit_failed", e)),
            }
        }
        self.open = open;
    }

    fn body(&mut self, ui: &mut Ui, state: &AppState, fgd: Option<&ResolvedFgd>, ids: &[hp_editor::Id]) -> bool {
        ui.set_max_width(360.0);
        ui.label(t!("entities.maxdist_help"));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(t!("entities.maxdist_distance"));
            ui.add(
                DragValue::new(&mut self.distance).range(0.0..=MAX_DISTANCE).speed(8.0).max_decimals(0).suffix(" u"),
            );
            if ui.small_button("0").on_hover_text(t!("entities.maxdist_unlimited")).clicked() {
                self.distance = 0.0;
            }
        });
        if self.distance <= 0.0 {
            ui.label(RichText::new(t!("entities.maxdist_unlimited")).weak());
        }
        ui.separator();
        let (change, same, skip) = preview(state, fgd, ids, self.distance);
        if ids.is_empty() {
            ui.label(RichText::new(t!("entities.maxdist_no_selection")).weak());
        } else {
            ui.label(tf!("entities.maxdist_preview", change, same, skip));
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| ui.add_enabled(change > 0, egui::Button::new(t!("entities.maxdist_apply"))).clicked()).inner
    }
}
