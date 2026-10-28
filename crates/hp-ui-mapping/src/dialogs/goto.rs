use egui::{RichText, Ui};
use glam::DVec3;
use hp_app::actions::key_label;
use hp_app::{AppState, t, tf, theme};
use hp_editor::ObjRef;

use crate::util::{center_views_on, escape_pressed, focus_select_all, icon_title, select_and_center};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GoTarget {
    Id(u32),
    Pos(DVec3),
}

pub(crate) fn parse_goto(text: &str) -> Option<GoTarget> {
    let t = text.trim();
    let id_text = t.strip_prefix('#').or_else(|| crate::util::strip_prefix_ci(t, "id")).map(str::trim).unwrap_or(t);
    if let Ok(id) = id_text.parse::<u32>() {
        return Some(GoTarget::Id(id));
    }
    let cleaned: String =
        t.chars().map(|c| if matches!(c, '(' | ')' | '[' | ']' | ',' | ';') { ' ' } else { c }).collect();
    if let Ok(nums) = cleaned.split_whitespace().map(str::parse::<f64>).collect::<Result<Vec<f64>, _>>() {
        return match nums[..] {
            [x, y, z] if x.is_finite() && y.is_finite() && z.is_finite() => Some(GoTarget::Pos(DVec3::new(x, y, z))),
            _ => None,
        };
    }
    from_log_line(t)
}

fn from_log_line(t: &str) -> Option<GoTarget> {
    let (_, loc) = hp_compile::parse_log_line(t);
    if let Some(b) = loc.as_ref().and_then(|l| l.brush_id) {
        return Some(GoTarget::Id(b));
    }
    if let Some(p) = loc.as_ref().and_then(|l| l.position) {
        return Some(GoTarget::Pos(DVec3::from_array(p)));
    }
    let mut rest = t;
    while let Some(open) = rest.find('(') {
        let after = &rest[open + 1..];
        let Some(close) = after.find(')') else { break };
        let inner: Vec<f64> = after[..close]
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        if let [x, y, z] = inner[..]
            && x.is_finite()
            && y.is_finite()
            && z.is_finite()
        {
            return Some(GoTarget::Pos(DVec3::new(x, y, z)));
        }
        rest = &after[close + 1..];
    }
    let nums: Vec<f64> = t
        .split(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
        .filter(|s| s.chars().any(|c| c.is_ascii_digit()))
        .filter_map(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .collect();
    match nums[..] {
        [.., x, y, z] if nums.len() >= 3 => Some(GoTarget::Pos(DVec3::new(x, y, z))),
        _ => None,
    }
}

pub(crate) fn go_to(state: &mut AppState, target: GoTarget) -> Result<(), String> {
    match target {
        GoTarget::Id(id) => {
            let obj = if state.doc.scene().solid(id).is_some() {
                ObjRef::Solid(id)
            } else if state.doc.scene().entity(id).is_some() {
                ObjRef::Entity(id)
            } else {
                return Err(tf!("goto.not_found", id));
            };
            if state.doc.is_visible(obj) {
                select_and_center(state, &[obj]);
            } else {
                if let Some(p) = object_center(state, obj) {
                    center_views_on(state, p);
                }
                state.notify(hp_app::Severity::Info, t!("goto.hidden"));
            }
            Ok(())
        }
        GoTarget::Pos(p) => {
            center_views_on(state, p);
            Ok(())
        }
    }
}

fn object_center(state: &AppState, obj: ObjRef) -> Option<DVec3> {
    let scene = state.doc.scene();
    match obj {
        ObjRef::Solid(id) => scene.solid_bounds(id).map(|b| b.center()),
        ObjRef::Entity(id) => {
            let n = scene.entity(id)?;
            state.assets.entity_bounds(&n.entity).map(|b| b.center()).or_else(|| n.entity.origin())
        }
    }
}

#[derive(Default)]
pub(crate) struct GotoDialog {
    pub(crate) text: String,
    pub(crate) error: Option<String>,
    was_open: bool,
}

impl GotoDialog {
    pub(crate) fn submit(&mut self, state: &mut AppState) -> bool {
        let Some(target) = parse_goto(&self.text) else {
            self.error = Some(t!("goto.invalid").to_owned());
            return false;
        };
        match go_to(state, target) {
            Ok(()) => {
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.ui.show_goto {
            self.was_open = false;
            return;
        }
        let first_frame = !self.was_open;
        if first_frame {
            self.was_open = true;
            self.error = None;
            let quick = std::mem::take(&mut state.ui.quick_input);
            if !quick.trim().is_empty() {
                self.text = quick;
            }
        }
        let mut open = true;
        let mut close = false;
        let response = hp_app::chrome::Window::new(icon_title("🎯", t!("goto.title")))
            .id(egui::Id::unique("hp_goto_window"))
            .default_pos(crate::util::centered_pos(ctx, egui::vec2(340.0, 140.0)))
            .open(&mut open)
            .default_width(340.0)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| close = self.window_ui(ui, state, first_frame));
        if let Some(r) = &response
            && escape_pressed(ctx, &r.response)
        {
            open = false;
        }
        if !open || close {
            state.ui.show_goto = false;
        }
    }

    fn window_ui(&mut self, ui: &mut Ui, state: &mut AppState, first_frame: bool) -> bool {
        ui.label(t!("goto.prompt"));
        let edit =
            ui.add(egui::TextEdit::singleline(&mut self.text).hint_text(t!("goto.hint")).desired_width(f32::INFINITY));
        if first_frame {
            focus_select_all(&edit, &self.text);
        }
        let mut close = false;
        let mut go = edit.lost_focus() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        if edit.changed() {
            self.error = None;
        }
        match (&self.error, parse_goto(&self.text)) {
            (Some(e), _) => {
                ui.label(RichText::new(e).color(theme::error()));
            }
            (None, Some(GoTarget::Id(id))) => {
                let what = if state.doc.scene().solid(id).is_some() {
                    tf!("goto.preview_solid", id)
                } else if let Some(n) = state.doc.scene().entity(id) {
                    tf!("goto.preview_entity", id, n.entity.classname)
                } else {
                    tf!("goto.not_found", id)
                };
                ui.label(RichText::new(what).weak());
            }
            (None, Some(GoTarget::Pos(p))) => {
                ui.label(RichText::new(tf!("goto.preview_position", hp_app::widgets::fmt_vec3(p))).weak());
            }
            (None, None) => {
                ui.label(RichText::new(" ").weak());
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t!("common.cancel")).on_hover_text(key_label(egui::Key::Escape)).clicked() {
                    close = true;
                }
                if ui
                    .add_enabled(!self.text.trim().is_empty(), egui::Button::new(t!("goto.go")))
                    .on_hover_text(key_label(egui::Key::Enter))
                    .clicked()
                {
                    go = true;
                }
            });
        });
        if go && self.submit(state) {
            close = true;
        }
        close
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing() {
        assert_eq!(parse_goto("1234"), Some(GoTarget::Id(1234)));
        assert_eq!(parse_goto("  #42 "), Some(GoTarget::Id(42)));
        assert_eq!(parse_goto("id 7"), Some(GoTarget::Id(7)));
        assert_eq!(parse_goto("ID7"), Some(GoTarget::Id(7)));
        assert_eq!(parse_goto("0 0 512"), Some(GoTarget::Pos(DVec3::new(0.0, 0.0, 512.0))));
        assert_eq!(parse_goto("(1.5, -2, 3e2)"), Some(GoTarget::Pos(DVec3::new(1.5, -2.0, 300.0))));
        assert_eq!(parse_goto("[1 2 3]"), Some(GoTarget::Pos(DVec3::new(1.0, 2.0, 3.0))));
        assert_eq!(parse_goto("1 2"), None);
        assert_eq!(parse_goto("abc"), None);
        assert_eq!(parse_goto(""), None);
        assert_eq!(parse_goto("1 2 inf"), None);
        assert_eq!(parse_goto("-5"), None);
        assert_eq!(parse_goto("Brush 1234: WARNING, microbrush"), Some(GoTarget::Id(1234)));
        assert_eq!(
            parse_goto("Entity info_player_start (-680.00 0.00 8.00) leaked!"),
            Some(GoTarget::Pos(DVec3::new(-680.0, 0.0, 8.0)))
        );
        assert_eq!(
            parse_goto("Displacement 4 has bad geometry near 10.00 20.00 30.00"),
            Some(GoTarget::Pos(DVec3::new(10.0, 20.0, 30.0)))
        );
        assert_eq!(
            parse_goto("something odd (oops) then (1 2 3) and (4 5 6)"),
            Some(GoTarget::Pos(DVec3::new(1.0, 2.0, 3.0)))
        );
        assert_eq!(parse_goto("x=1 y=2 z=3 w=4"), Some(GoTarget::Pos(DVec3::new(2.0, 3.0, 4.0))));
        assert_eq!(parse_goto("only 1 and 2"), None);
        assert_eq!(parse_goto("(1 2 3"), Some(GoTarget::Pos(DVec3::new(1.0, 2.0, 3.0))));
    }

    #[test]
    fn jumping() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        assert!(go_to(&mut state, GoTarget::Id(3)).is_ok());
        assert_eq!(state.doc.selected_objects(), vec![ObjRef::Solid(3)]);
        assert!(go_to(&mut state, GoTarget::Id(5)).is_ok());
        assert_eq!(state.doc.selected_objects(), vec![ObjRef::Entity(5)]);
        assert!(state.requested_actions.contains(&hp_app::Action::Center2dOnSelection));
        assert!(go_to(&mut state, GoTarget::Id(999)).is_err());
        assert!(go_to(&mut state, GoTarget::Pos(DVec3::new(32.0, 64.0, 0.0))).is_ok());
        assert_eq!(state.cameras.top.center, glam::DVec2::new(32.0, 64.0));

        state.doc.set_selection(&[]);
        state.doc.set_hidden(&[ObjRef::Solid(3)], true);
        assert!(go_to(&mut state, GoTarget::Id(3)).is_ok());
        assert!(state.doc.selected_objects().is_empty());
        let c = state.doc.scene().solid_bounds(3).expect("bounds").center();
        assert_eq!(state.cameras.top.center, glam::DVec2::new(c.x, c.y));

        let mut d = GotoDialog { text: "nonsense".into(), ..GotoDialog::default() };
        assert!(!d.submit(&mut state));
        assert!(d.error.is_some());
        d.text = "2".into();
        assert!(d.submit(&mut state));
        assert!(d.error.is_none());
    }

    #[test]
    fn reopened_field_replaces_its_text() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut d = GotoDialog { text: "516".into(), ..GotoDialog::default() };
        state.ui.show_goto = true;
        let ctx = egui::Context::default();
        let frame = |events: Vec<egui::Event>, state: &mut AppState, d: &mut GotoDialog| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| d.show(ui.ctx(), state));
            out.textures_delta.clear();
        };
        frame(Vec::new(), &mut state, &mut d);
        frame(vec![egui::Event::Text("1457".into())], &mut state, &mut d);
        assert_eq!(d.text, "1457");
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![enter], &mut state, &mut d);
        assert!(d.error.is_some());
        assert!(state.ui.show_goto, "an error keeps the dialog open");
    }
}
