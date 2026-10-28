use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2};
use glam::DVec3;
use hp_app::view::Camera3d;
use hp_app::{Action, AppState, ViewInfo, ViewKind};
use hp_editor::ObjRef;
use hp_geom::Aabb;
use hp_tools_select::SelectTool;
use hp_tools_select::gizmo::GizmoLayout;
use hp_ui_viewports::Viewports;
use hp_ui_viewports::params::{RenderConfig, view_info};

use crate::keyboard::{AppCapture, KeyRouter};

const WALL: ObjRef = ObjRef::Solid(3);
const CTRL: Modifiers = Modifiers { alt: false, ctrl: true, shift: false, mac_cmd: false, command: true };
const NEW_MATERIAL: &str = "DEV/DEV_MEASUREWALL01A";

struct App {
    ctx: egui::Context,
    router: KeyRouter,
    state: AppState,
    views: Viewports,
    tool: SelectTool,
    field: String,
    focus_field: bool,
    time: f64,
}

impl App {
    fn new() -> Self {
        let mut state = hp_app::testing::test_state();
        hp_ui_viewports::register_translations();
        hp_tools_select::register_translations();
        state.cameras.cam3d =
            Camera3d { pos: DVec3::new(0.0, -800.0, 64.0), yaw_deg: 90.0, pitch_deg: 0.0, ..Camera3d::default() };
        let mut app = Self {
            ctx: egui::Context::default(),
            router: KeyRouter::default(),
            state,
            views: Viewports::new(),
            tool: SelectTool::new(),
            field: String::new(),
            focus_field: false,
            time: 0.0,
        };
        app.field = app.wall_material();
        app.frame(vec![]);
        app.frame(vec![]);
        app
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        self.state.frame.dt = 1.0 / 60.0;
        let mut raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))),
            time: Some(self.time),
            events,
            focused: true,
            ..Default::default()
        };
        self.router.before_frame(&self.ctx, &mut raw, false, AppCapture::default());
        let Self { ctx, router, state, views, tool, field, focus_field, .. } = self;
        let mut out = ctx.clone().run_ui(raw, |ui| {
            egui::Panel::right("dock").default_size(300.0).show(ui, |ui| {
                let r = ui.text_edit_singleline(field);
                if std::mem::take(focus_field) {
                    r.request_focus();
                }
                if r.changed() {
                    let _ = state.doc.apply_material_to_objects(&[WALL], field);
                }
            });
            egui::CentralPanel::no_frame().show(ui, |ui| views.ui(ui, state, tool));
            let routed = router.after_ui(ui.ctx(), state, tool, AppCapture::default());
            for action in routed.actions {
                match action {
                    Action::Undo => {
                        state.doc.undo();
                    }
                    Action::Redo => {
                        state.doc.redo();
                    }
                    _ => {}
                }
            }
        });
        out.textures_delta.clear();
    }

    fn view(&self, kind: ViewKind) -> ViewInfo {
        let (index, rect) = self
            .views
            .view_rects()
            .iter()
            .enumerate()
            .find_map(|(i, v)| v.filter(|(k, _)| *k == kind).map(|(_, r)| (i, r)))
            .expect("view visible");
        view_info(index, kind, rect, &self.state.cameras, &RenderConfig::from_state(&self.state), 1.0)
    }

    fn press(pos: Pos2, pressed: bool, modifiers: Modifiers) -> Event {
        Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers }
    }

    fn click(&mut self, at: Pos2) {
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![Self::press(at, true, Modifiers::NONE)]);
        self.frame(vec![Self::press(at, false, Modifiers::NONE)]);
        self.time += 1.0;
        self.frame(vec![]);
    }

    fn drag(&mut self, a: Pos2, b: Pos2) {
        self.frame(vec![Event::PointerMoved(a)]);
        self.frame(vec![Self::press(a, true, Modifiers::NONE)]);
        for i in 1..=6 {
            self.frame(vec![Event::PointerMoved(a + (b - a) * (i as f32 / 6.0))]);
        }
        self.frame(vec![Self::press(b, false, Modifiers::NONE)]);
        self.frame(vec![]);
    }

    fn key(&mut self, key: Key, modifiers: Modifiers) {
        self.frame(vec![
            Event::ModifiersChanged(modifiers),
            Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers },
        ]);
        self.frame(vec![Event::Key { key, physical_key: Some(key), pressed: false, repeat: false, modifiers }]);
        self.frame(vec![Event::ModifiersChanged(Modifiers::NONE)]);
    }

    fn wall(&self) -> Aabb {
        self.state.doc.object_bounds(WALL, &|e| self.state.entity_bounds(e)).expect("wall bounds")
    }

    fn wall_material(&self) -> String {
        self.state.doc.scene().solid(3).expect("wall").solid.sides[0].material.clone()
    }

    fn labels(&self) -> Vec<String> {
        self.state.doc.undo_labels().into_iter().map(str::to_owned).collect()
    }
}

fn scr(view: &ViewInfo, p: DVec3) -> Pos2 {
    view.world_to_screen(p).expect("point on screen")
}

#[derive(Clone, Copy, Debug)]
enum How {
    Drag2d,
    Gizmo3d,
    Nudge,
}

fn edit_move_undo_redo(how: How) {
    let mut app = App::new();
    let top = app.view(ViewKind::Top);
    app.click(scr(&top, DVec3::new(100.0, 272.0, 0.0)));
    assert_eq!(app.state.doc.selected_objects(), vec![WALL], "{how:?}");
    let old_material = app.wall_material();

    app.focus_field = true;
    app.frame(vec![]);
    app.frame(vec![Event::Key {
        key: Key::A,
        physical_key: Some(Key::A),
        pressed: true,
        repeat: false,
        modifiers: CTRL,
    }]);
    app.frame(vec![Event::Text(NEW_MATERIAL.to_owned())]);
    app.time += 2.0;
    app.frame(vec![]);
    assert_eq!(app.wall_material(), NEW_MATERIAL, "{how:?}");
    let edits = app.labels().len();
    let before = app.wall();

    match how {
        How::Drag2d => {
            let from = scr(&top, before.center());
            app.drag(from, from + vec2(60.0, 0.0));
        }
        How::Gizmo3d => {
            let v3 = app.view(ViewKind::Perspective);
            let g = GizmoLayout::new(&v3, before.center()).expect("gizmo in front of the camera");
            let grab = g.center + DVec3::X * g.size * 0.6;
            app.drag(scr(&v3, grab), scr(&v3, grab + DVec3::new(100.0, 0.0, 0.0)));
        }
        How::Nudge => {
            app.click(scr(&top, before.center()));
            app.key(Key::ArrowRight, Modifiers::NONE);
        }
    }
    let moved = app.wall();
    assert!((moved.min - before.min).length() > 1.0, "{how:?}: the wall moved ({before:?} -> {moved:?})");
    assert_eq!(app.labels().len(), edits + 1, "{how:?}: the move is one step: {:?}", app.labels());

    app.key(Key::Z, CTRL);
    assert_eq!(app.wall(), before, "{how:?}: Ctrl+Z puts the wall back ({:?})", app.labels());
    assert_eq!(app.wall_material(), NEW_MATERIAL, "{how:?}: the edit before the move is kept");
    app.key(Key::Z, CTRL);
    assert_eq!(app.wall(), before, "{how:?}");
    assert_eq!(app.wall_material(), old_material, "{how:?}: the second Ctrl+Z undoes the edit");
    app.key(Key::Y, CTRL);
    assert_eq!(app.wall_material(), NEW_MATERIAL, "{how:?}");
    assert_eq!(app.wall(), before, "{how:?}");
    app.key(Key::Y, CTRL);
    assert_eq!(app.wall(), moved, "{how:?}");
    assert_eq!(app.wall_material(), NEW_MATERIAL, "{how:?}");
    assert!(app.ctx.memory(|m| m.focused()).is_none(), "{how:?}");
}

#[test]
fn ctrl_z_after_a_2d_drag_undoes_the_move() {
    edit_move_undo_redo(How::Drag2d);
}

#[test]
fn ctrl_z_after_a_3d_gizmo_drag_undoes_the_move() {
    edit_move_undo_redo(How::Gizmo3d);
}

#[test]
fn ctrl_z_after_a_nudge_undoes_the_nudge() {
    edit_move_undo_redo(How::Nudge);
}
