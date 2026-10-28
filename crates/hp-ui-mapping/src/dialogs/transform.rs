use egui::{RichText, Ui};
use glam::{DMat4, DQuat, DVec3, EulerRot};
use hp_app::actions::key_label;
use hp_app::{Action, AppState, t, tf, theme};

use crate::util::{enter_pressed, escape_pressed, icon_title, plural};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransformMode {
    Move,
    Rotate,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pivot {
    SelectionCenter,
    Origin,
    Custom,
}

pub(crate) fn rotation_quat(degrees: DVec3) -> DQuat {
    DQuat::from_euler(EulerRot::ZYX, degrees.z.to_radians(), degrees.y.to_radians(), degrees.x.to_radians())
}

pub(crate) fn transform_matrix(mode: TransformMode, v: DVec3, pivot: DVec3) -> Result<DMat4, &'static str> {
    if !v.is_finite() || !pivot.is_finite() {
        return Err("transform.err_invalid");
    }
    let around = |m: DMat4| DMat4::from_translation(pivot) * m * DMat4::from_translation(-pivot);
    match mode {
        TransformMode::Move => Ok(DMat4::from_translation(v)),
        TransformMode::Rotate => Ok(around(DMat4::from_quat(rotation_quat(v)))),
        TransformMode::Scale => {
            if v.x.abs() < 1e-6 || v.y.abs() < 1e-6 || v.z.abs() < 1e-6 {
                return Err("transform.err_zero_scale");
            }
            Ok(around(DMat4::from_scale(v)))
        }
    }
}

pub(crate) struct TransformDialog {
    pub(crate) mode: TransformMode,
    pub(crate) offset: DVec3,
    pub(crate) angles: DVec3,
    pub(crate) scale: DVec3,
    pub(crate) pivot: Pivot,
    pub(crate) error: Option<String>,
}

impl Default for TransformDialog {
    fn default() -> Self {
        Self {
            mode: TransformMode::Move,
            offset: DVec3::ZERO,
            angles: DVec3::ZERO,
            scale: DVec3::ONE,
            pivot: Pivot::SelectionCenter,
            error: None,
        }
    }
}

impl TransformDialog {
    fn values(&self) -> DVec3 {
        match self.mode {
            TransformMode::Move => self.offset,
            TransformMode::Rotate => self.angles,
            TransformMode::Scale => self.scale,
        }
    }

    pub(crate) fn apply(&mut self, state: &mut AppState) -> bool {
        let objs = state.doc.selected_objects();
        if objs.is_empty() {
            self.error = Some(t!("transform.no_selection").to_owned());
            return false;
        }
        let center = |state: &AppState| {
            let assets = &state.assets;
            state.doc.selection_center(&|e| assets.entity_bounds(e)).unwrap_or(DVec3::ZERO)
        };
        let pivot = match self.pivot {
            Pivot::Origin => DVec3::ZERO,
            Pivot::Custom => state.pivot.filter(|p| p.is_finite()).unwrap_or_else(|| center(state)),
            Pivot::SelectionCenter => center(state),
        };
        let v = self.values();
        let m = match transform_matrix(self.mode, v, pivot) {
            Ok(m) => m,
            Err(key) => {
                self.error = Some(t!(key).to_owned());
                return false;
            }
        };
        let result = match self.mode {
            TransformMode::Move => state.doc.translate(&objs, v),
            TransformMode::Rotate => state.doc.rotate(&objs, pivot, rotation_quat(v)),
            TransformMode::Scale => state.doc.transform(&objs, m),
        };
        match result {
            Ok(()) => {
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(tf!("transform.failed", e));
                false
            }
        }
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.ui.show_transform {
            self.error = None;
            return;
        }
        let mut open = true;
        let response = hp_app::chrome::Window::new(icon_title("⟲", t!("transform.title")))
            .id(egui::Id::unique("hp_transform_window"))
            .default_pos(crate::util::centered_pos(ctx, egui::vec2(360.0, 220.0)))
            .open(&mut open)
            .default_width(360.0)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| self.window_ui(ui, state));
        if let Some(r) = &response {
            if escape_pressed(ctx, &r.response) {
                open = false;
            } else if enter_pressed(ctx, &r.response) {
                self.apply(state);
            }
        }
        if !open {
            state.ui.show_transform = false;
        }
    }

    fn window_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal(|ui| {
            for (m, key) in [
                (TransformMode::Move, "transform.move"),
                (TransformMode::Rotate, "transform.rotate"),
                (TransformMode::Scale, "transform.scale"),
            ] {
                if ui.add(egui::Button::selectable(self.mode == m, t!(key))).clicked() {
                    self.mode = m;
                    self.error = None;
                }
            }
        });
        ui.add_space(4.0);
        let (label, speed) = match self.mode {
            TransformMode::Move => (t!("transform.offset"), 1.0),
            TransformMode::Rotate => (t!("transform.angles"), 1.0),
            TransformMode::Scale => (t!("transform.factors"), 0.01),
        };
        egui::Grid::new("hp_transform_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label(label);
            let v = match self.mode {
                TransformMode::Move => &mut self.offset,
                TransformMode::Rotate => &mut self.angles,
                TransformMode::Scale => &mut self.scale,
            };
            hp_app::widgets::vec3_edit(ui, v, speed);
            ui.end_row();
            if self.mode != TransformMode::Move {
                ui.label(t!("transform.pivot"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.pivot, Pivot::SelectionCenter, t!("transform.pivot_selection"));
                    ui.radio_value(&mut self.pivot, Pivot::Origin, t!("transform.pivot_origin"));
                    let custom = state.pivot.filter(|p| p.is_finite());
                    let tip = match custom {
                        Some(p) => tf!("transform.pivot_custom_tip", hp_app::widgets::fmt_vec3(p)),
                        None => t!("transform.pivot_custom_off").to_owned(),
                    };
                    ui.add_enabled_ui(custom.is_some() || self.pivot == Pivot::Custom, |ui| {
                        ui.radio_value(&mut self.pivot, Pivot::Custom, t!("transform.pivot_custom")).on_hover_text(tip);
                    });
                });
                ui.end_row();
            }
            ui.label(t!("transform.texture_lock"));
            ui.horizontal(|ui| {
                let lock = state.doc.options().texture_lock;
                let text = if lock { t!("common.enabled") } else { t!("common.disabled") };
                ui.label(RichText::new(text).color(if lock {
                    theme::success()
                } else {
                    ui.visuals().weak_text_color()
                }));
                if ui.small_button(t!("transform.toggle")).on_hover_text(t!("action.toggle_texture_lock")).clicked() {
                    state.request(Action::ToggleTextureLock);
                }
            });
            ui.end_row();
        });
        if ui.small_button(t!("common.reset")).clicked() {
            match self.mode {
                TransformMode::Move => self.offset = DVec3::ZERO,
                TransformMode::Rotate => self.angles = DVec3::ZERO,
                TransformMode::Scale => self.scale = DVec3::ONE,
            }
        }
        let n = state.doc.selection().objects.len();
        ui.add_space(4.0);
        ui.label(RichText::new(tf!(plural(n, "transform.selection_info_one", "transform.selection_info"), n)).weak());
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(theme::error()));
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t!("common.close")).on_hover_text(key_label(egui::Key::Escape)).clicked() {
                    state.ui.show_transform = false;
                }
                if ui
                    .add_enabled(n > 0, egui::Button::new(t!("common.apply")))
                    .on_hover_text(key_label(egui::Key::Enter))
                    .clicked()
                {
                    self.apply(state);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_editor::ObjRef;

    #[test]
    fn matrices() {
        let p = DVec3::new(10.0, 0.0, 0.0);
        let m = transform_matrix(TransformMode::Move, DVec3::new(1.0, 2.0, 3.0), p).expect("move");
        assert_eq!(m.transform_point3(DVec3::ZERO), DVec3::new(1.0, 2.0, 3.0));
        let r = transform_matrix(TransformMode::Rotate, DVec3::new(0.0, 0.0, 90.0), p).expect("rotate");
        assert!((r.transform_point3(DVec3::new(20.0, 0.0, 0.0)) - DVec3::new(10.0, 10.0, 0.0)).length() < 1e-9);
        assert!((r.transform_point3(p) - p).length() < 1e-9, "the pivot stays in place");
        let s = transform_matrix(TransformMode::Scale, DVec3::new(2.0, 1.0, 1.0), p).expect("scale");
        assert!((s.transform_point3(DVec3::new(20.0, 5.0, 0.0)) - DVec3::new(30.0, 5.0, 0.0)).length() < 1e-9);
        assert_eq!(
            transform_matrix(TransformMode::Scale, DVec3::new(0.0, 1.0, 1.0), p),
            Err("transform.err_zero_scale")
        );
        assert_eq!(transform_matrix(TransformMode::Move, DVec3::NAN, p), Err("transform.err_invalid"));
        let q = rotation_quat(DVec3::new(90.0, 0.0, 0.0));
        assert!((q * DVec3::Y - DVec3::Z).length() < 1e-9);
    }

    #[test]
    fn apply_to_selection() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut d = TransformDialog::default();
        assert!(!d.apply(&mut state), "nothing selected");
        assert!(d.error.is_some());

        state.doc.set_selection(&[ObjRef::Entity(5)]);
        d.offset = DVec3::new(16.0, 0.0, 0.0);
        assert!(d.apply(&mut state));
        assert_eq!(state.doc.scene().entity(5).and_then(|n| n.entity.origin()), Some(DVec3::new(16.0, 0.0, 96.0)));
        assert_eq!(state.doc.undo_label(), Some("Move"));

        state.doc.set_selection(&[ObjRef::Solid(3)]);
        let before = state.doc.scene().solid_bounds(3).expect("bounds");
        d.mode = TransformMode::Rotate;
        d.angles = DVec3::new(0.0, 0.0, 90.0);
        assert!(d.apply(&mut state));
        let after = state.doc.scene().solid_bounds(3).expect("bounds");
        assert!((after.size().x - before.size().y).abs() < 1e-6);
        assert!((after.center() - before.center()).length() < 1e-6);

        d.mode = TransformMode::Scale;
        d.pivot = Pivot::Origin;
        d.scale = DVec3::new(1.0, 1.0, 2.0);
        assert!(d.apply(&mut state));
        let scaled = state.doc.scene().solid_bounds(3).expect("bounds");
        assert!((scaled.size().z - after.size().z * 2.0).abs() < 1e-6);
        d.scale = DVec3::new(1.0, 0.0, 1.0);
        assert!(!d.apply(&mut state));

        state.doc.set_selection(&[ObjRef::Entity(5)]);
        state.pivot = Some(DVec3::new(100.0, 0.0, 96.0));
        d.mode = TransformMode::Rotate;
        d.pivot = Pivot::Custom;
        d.angles = DVec3::new(0.0, 0.0, 90.0);
        let before = state.doc.scene().entity(5).and_then(|n| n.entity.origin()).expect("origin");
        assert!(d.apply(&mut state));
        let after = state.doc.scene().entity(5).and_then(|n| n.entity.origin()).expect("origin");
        let p = DVec3::new(100.0, 0.0, 96.0);
        let rotated = p + glam::DQuat::from_rotation_z(90f64.to_radians()) * (before - p);
        assert!((after - rotated).length() < 1e-6, "{after:?}");
        state.pivot = None;
        assert!(d.apply(&mut state));
    }
}
