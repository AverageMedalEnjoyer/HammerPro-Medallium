use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, vec2};
use hp_app::settings::{CtrlWheel, NavScheme};
use hp_app::testing::test_state;
use hp_app::{ToolFlow, ToolId};

use super::*;

#[derive(Default)]
struct Recorder {
    inputs: Vec<(usize, bool)>,
    clicks: Vec<usize>,
    middle_clicks: Vec<usize>,
}

impl Tool for Recorder {
    fn id(&self) -> ToolId {
        ToolId::Select
    }

    fn view_input(&mut self, _state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        self.inputs.push((input.view.index, input.navigating));
        if input.response.clicked() {
            self.clicks.push(input.view.index);
        }
        if input.response.middle_clicked() {
            self.middle_clicks.push(input.view.index);
        }
        ToolFlow::Handled
    }
}

struct Harness {
    ctx: egui::Context,
    vp: Viewports,
    state: AppState,
    tool: Recorder,
    time: f64,
    modifiers: Modifiers,
    copied: Vec<String>,
}

impl Harness {
    fn new(scheme: NavScheme) -> Self {
        register_translations();
        let mut state = test_state();
        state.settings.workflow.nav_scheme = scheme;
        let mut h = Self {
            ctx: egui::Context::default(),
            vp: Viewports::new(),
            state,
            tool: Recorder::default(),
            time: 0.0,
            modifiers: Modifiers::NONE,
            copied: Vec::new(),
        };
        h.frame(vec![]);
        h.frame(vec![]);
        h
    }

    fn frame(&mut self, mut events: Vec<Event>) {
        events.insert(0, Event::ModifiersChanged(self.modifiers));
        self.time += 1.0 / 60.0;
        self.state.frame.dt = 1.0 / 60.0;
        self.state.frame.time = self.time;
        self.state.frame.fps = 60.0;
        let input = RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))),
            time: Some(self.time),
            events,
            focused: true,
            ..Default::default()
        };
        let mut out = self.ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| self.vp.ui(ui, &mut self.state, &mut self.tool));
        });
        out.textures_delta.clear();
        for cmd in out.platform_output.commands {
            if let egui::OutputCommand::CopyText(text) = cmd {
                self.copied.push(text);
            }
        }
    }

    fn rect(&self, i: usize) -> Rect {
        match self.vp.view_rects()[i] {
            Some((_, r)) => r,
            None => panic!("view {i} is not visible"),
        }
    }

    fn move_to(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
    }

    fn button(&mut self, p: Pos2, button: PointerButton, pressed: bool) {
        self.frame(vec![
            Event::PointerMoved(p),
            Event::PointerButton { pos: p, button, pressed, modifiers: self.modifiers },
        ]);
    }

    fn key(&mut self, key: Key, physical: Option<Key>, pressed: bool) {
        self.frame(vec![Event::Key { key, physical_key: physical, pressed, repeat: false, modifiers: self.modifiers }]);
    }

    fn wheel(&mut self, p: Pos2, notches: f32) {
        self.frame(vec![
            Event::PointerMoved(p),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: vec2(0.0, notches),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
                modifiers: self.modifiers,
            },
        ]);
    }

    fn yaw(&self) -> f64 {
        self.state.cameras.cam3d.yaw_deg
    }
}

#[test]
fn hammer_space_left_drag_looks_in_3d() {
    let mut h = Harness::new(NavScheme::Hammer);
    let p = h.rect(0).center();
    h.move_to(p);
    h.key(Key::Space, None, true);
    let yaw = h.yaw();
    let pos = h.state.cameras.cam3d.pos;
    h.button(p, PointerButton::Primary, true);
    assert!(h.vp.captures_keyboard(), "WASD fly while looking");
    h.move_to(p + vec2(40.0, 0.0));
    h.move_to(p + vec2(80.0, 0.0));
    assert!(camera::wrap_degrees(h.yaw() - yaw) < -1.0, "moving right turns right");
    for _ in 0..10 {
        h.frame(vec![]);
    }
    assert!((h.state.cameras.cam3d.pos - pos).length() < 1e-9, "Space held for the gesture does not fly up");
    h.button(p + vec2(80.0, 0.0), PointerButton::Primary, false);
    h.key(Key::Space, None, false);
    assert!(h.tool.clicks.is_empty(), "the tool never saw the press");
    assert!(!h.vp.is_context_menu_open());
}

#[test]
fn hammerpro_space_left_click_still_reaches_the_tool_in_3d() {
    let mut h = Harness::new(NavScheme::HammerPro);
    let p = h.rect(0).center();
    h.move_to(p);
    h.key(Key::Space, None, true);
    let yaw = h.yaw();
    h.button(p, PointerButton::Primary, true);
    h.button(p, PointerButton::Primary, false);
    h.key(Key::Space, None, false);
    assert_eq!(h.yaw(), yaw);
    assert_eq!(h.tool.clicks, vec![0]);
}

#[test]
fn hammer_space_right_drag_pans_3d_without_menu() {
    let mut h = Harness::new(NavScheme::Hammer);
    let p = h.rect(0).center();
    h.move_to(p);
    h.key(Key::Space, None, true);
    let cam = h.state.cameras.cam3d;
    h.button(p, PointerButton::Secondary, true);
    h.move_to(p + vec2(30.0, 0.0));
    let moved = h.state.cameras.cam3d.pos - cam.pos;
    assert!(moved.dot(cam.right()) < -1.0, "the world follows the pointer: {moved:?}");
    assert_eq!(h.yaw(), cam.yaw_deg, "panning does not turn");
    h.button(p + vec2(30.0, 0.0), PointerButton::Secondary, false);
    h.key(Key::Space, None, false);
    h.frame(vec![]);
    assert!(!h.vp.is_context_menu_open());
    h.key(Key::Space, None, true);
    h.button(p, PointerButton::Secondary, true);
    h.button(p, PointerButton::Secondary, false);
    h.key(Key::Space, None, false);
    h.frame(vec![]);
    assert!(!h.vp.is_context_menu_open());
}

#[test]
fn hammer_both_buttons_move_and_strafe() {
    let mut h = Harness::new(NavScheme::Hammer);
    let p = h.rect(0).center();
    h.move_to(p);
    let cam = h.state.cameras.cam3d;
    h.button(p, PointerButton::Secondary, true);
    h.button(p, PointerButton::Primary, true);
    h.move_to(p + vec2(0.0, -20.0));
    let moved = h.state.cameras.cam3d.pos - cam.pos;
    assert!(moved.dot(cam.forward()) > 10.0, "pointer up moves forward: {moved:?}");
    assert_eq!(h.yaw(), cam.yaw_deg, "strafing does not turn");
    let pos = h.state.cameras.cam3d.pos;
    h.move_to(p + vec2(20.0, -20.0));
    assert!((h.state.cameras.cam3d.pos - pos).dot(cam.right()) > 10.0, "pointer right strafes right");
    h.button(p + vec2(20.0, -20.0), PointerButton::Primary, false);
    let pos = h.state.cameras.cam3d.pos;
    h.move_to(p + vec2(60.0, -20.0));
    h.button(p + vec2(60.0, -20.0), PointerButton::Secondary, false);
    h.frame(vec![]);
    assert!((h.state.cameras.cam3d.pos - pos).length() < 1e-9);
    assert!(!h.vp.is_context_menu_open());
    assert!(h.tool.clicks.is_empty());
    let mut h = Harness::new(NavScheme::HammerPro);
    let p = h.rect(0).center();
    h.move_to(p);
    let cam = h.state.cameras.cam3d;
    h.button(p, PointerButton::Secondary, true);
    h.button(p, PointerButton::Primary, true);
    h.move_to(p + vec2(0.0, -20.0));
    assert!((h.state.cameras.cam3d.pos - cam.pos).length() < 1e-9);
    h.button(p, PointerButton::Primary, false);
    h.button(p, PointerButton::Secondary, false);
}

#[test]
fn hammerpp_middle_drag_looks_and_middle_click_reaches_the_tool() {
    let mut h = Harness::new(NavScheme::HammerPlusPlus);
    let p = h.rect(0).center();
    h.move_to(p);
    let cam = h.state.cameras.cam3d;
    h.button(p, PointerButton::Middle, true);
    assert!(h.vp.captures_keyboard(), "flies with WASD like the right button");
    h.move_to(p + vec2(50.0, 0.0));
    assert!(camera::wrap_degrees(h.yaw() - cam.yaw_deg) < -1.0);
    assert!((h.state.cameras.cam3d.pos - cam.pos).length() < 1e-9, "no pan");
    h.button(p + vec2(50.0, 0.0), PointerButton::Middle, false);
    assert!(h.tool.middle_clicks.is_empty(), "a drag is not a click");
    h.button(p, PointerButton::Middle, true);
    h.button(p, PointerButton::Middle, false);
    h.frame(vec![]);
    assert_eq!(h.tool.middle_clicks, vec![0]);
    let q = h.rect(1).center();
    h.move_to(q);
    let center = h.state.cameras.top.center;
    h.button(q, PointerButton::Middle, true);
    h.move_to(q + vec2(30.0, 0.0));
    assert!(h.state.cameras.top.center.x < center.x);
    h.button(q + vec2(30.0, 0.0), PointerButton::Middle, false);
}

#[test]
fn invert_mouse_x_turns_the_other_way() {
    let mut h = Harness::new(NavScheme::HammerPro);
    h.state.settings.workflow.invert_mouse_x = true;
    let p = h.rect(0).center();
    h.move_to(p);
    let yaw = h.yaw();
    h.button(p, PointerButton::Secondary, true);
    h.move_to(p + vec2(50.0, 0.0));
    assert!(camera::wrap_degrees(h.yaw() - yaw) > 1.0, "pointer right turns left when inverted");
    h.button(p + vec2(50.0, 0.0), PointerButton::Secondary, false);
}

#[test]
fn fly_response_zero_reaches_full_speed_at_once() {
    let mut h = Harness::new(NavScheme::HammerPro);
    h.state.settings.workflow.fly_response = 0.0;
    let p = h.rect(0).center();
    h.move_to(p);
    h.button(p, PointerButton::Secondary, true);
    let pos = h.state.cameras.cam3d.pos;
    h.key(Key::W, Some(Key::W), true);
    let speed = f64::from(h.state.settings.view.camera_speed);
    let moved = (h.state.cameras.cam3d.pos - pos).length();
    assert!((moved - speed / 60.0).abs() < 1e-3, "one frame at full speed: {moved}");
    h.key(Key::W, Some(Key::W), false);
    h.button(p, PointerButton::Secondary, false);
    h.frame(vec![]);
    assert!(!h.vp.is_navigating(), "stops at once too");
}

#[test]
fn arrows_scroll_a_2d_view_when_nothing_is_selected() {
    let mut h = Harness::new(NavScheme::HammerPro);
    let p = h.rect(1).center();
    h.move_to(p);
    let cam = h.state.cameras.top;
    h.key(Key::ArrowRight, Some(Key::ArrowRight), true);
    let step = f64::from(h.rect(1).width()) / cam.zoom / 8.0;
    assert!((h.state.cameras.top.center.x - cam.center.x - step).abs() < 1e-6);
    h.key(Key::ArrowRight, Some(Key::ArrowRight), false);
    h.key(Key::ArrowUp, Some(Key::ArrowUp), true);
    h.key(Key::ArrowUp, Some(Key::ArrowUp), false);
    assert!(h.state.cameras.top.center.y > cam.center.y);
    h.state.doc.select_all();
    let center = h.state.cameras.top.center;
    h.key(Key::ArrowLeft, Some(Key::ArrowLeft), true);
    h.key(Key::ArrowLeft, Some(Key::ArrowLeft), false);
    assert_eq!(h.state.cameras.top.center, center);
    h.state.doc.clear_selection();
    h.move_to(Pos2::new(1599.0, 899.0));
    h.key(Key::ArrowLeft, Some(Key::ArrowLeft), true);
    assert_eq!(h.state.cameras.top.center, center);
}

#[test]
fn hammer_arrows_look_around_in_3d() {
    let mut h = Harness::new(NavScheme::Hammer);
    let p = h.rect(0).center();
    h.move_to(p);
    let yaw = h.yaw();
    h.key(Key::ArrowLeft, Some(Key::ArrowLeft), true);
    for _ in 0..10 {
        h.frame(vec![]);
    }
    h.key(Key::ArrowLeft, Some(Key::ArrowLeft), false);
    assert!(camera::wrap_degrees(h.yaw() - yaw) > 5.0, "left arrow turns left");
    let mut h = Harness::new(NavScheme::HammerPro);
    let p = h.rect(0).center();
    h.move_to(p);
    let yaw = h.yaw();
    h.key(Key::ArrowLeft, Some(Key::ArrowLeft), true);
    h.frame(vec![]);
    assert_eq!(h.yaw(), yaw);
}

#[test]
fn digit_keys_are_2d_zoom_presets() {
    let mut h = Harness::new(NavScheme::HammerPro);
    let p = h.rect(2).center();
    h.move_to(p);
    let center = h.state.cameras.front.center;
    h.state.cameras.top.zoom = 0.5;
    h.key(Key::Num5, Some(Key::Num5), true);
    assert!((h.state.cameras.front.zoom - 1.0).abs() < 1e-9);
    assert!((h.state.cameras.front.center - center).length() < 1e-9, "the center stays");
    h.key(Key::Num5, Some(Key::Num5), false);
    h.key(Key::Quote, Some(Key::Num3), true);
    assert!((h.state.cameras.front.zoom - 0.25).abs() < 1e-9);
    h.key(Key::Quote, Some(Key::Num3), false);
    assert_eq!(h.state.cameras.top.zoom, 0.5, "only the view under the mouse zooms");
    h.state.settings.workflow.link_2d_zoom = true;
    h.key(Key::Num7, Some(Key::Num7), true);
    for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
        assert!((h.state.cameras.cam2d(kind).zoom - 4.0).abs() < 1e-9, "{kind:?}");
    }
    h.key(Key::Num7, Some(Key::Num7), false);
    h.key(Key::Minus, Some(Key::Num6), true);
    h.key(Key::Minus, Some(Key::Num6), false);
    assert!((h.state.cameras.front.zoom - 4.0).abs() < 1e-9, "no preset 6");
    h.modifiers = Modifiers::ALT;
    h.key(Key::Num1, Some(Key::Num1), true);
    h.modifiers = Modifiers::NONE;
    assert!((h.state.cameras.front.zoom - 4.0).abs() < 1e-9);
}

#[test]
fn ctrl_wheel_can_zoom_every_2d_view_and_zoom_can_be_linked() {
    let mut h = Harness::new(NavScheme::HammerPro);
    h.state.settings.workflow.ctrl_wheel = CtrlWheel::ZoomAll2d;
    let p = h.rect(1).center();
    h.move_to(p);
    let before: Vec<f64> =
        [ViewKind::Top, ViewKind::Front, ViewKind::Side].map(|k| h.state.cameras.cam2d(k).zoom).to_vec();
    h.modifiers = Modifiers::CTRL;
    h.wheel(p, 1.0);
    h.modifiers = Modifiers::NONE;
    for (i, kind) in [ViewKind::Top, ViewKind::Front, ViewKind::Side].into_iter().enumerate() {
        assert!((h.state.cameras.cam2d(kind).zoom - before[i] * camera::ZOOM_STEP).abs() < 1e-9, "{kind:?}");
    }
    assert!(!h.state.requested_actions.contains(&Action::GridSmaller));
    h.state.settings.workflow.link_2d_zoom = true;
    h.state.cameras.side.zoom = 0.5;
    h.wheel(p, 1.0);
    let top = h.state.cameras.top.zoom;
    assert!((h.state.cameras.side.zoom - top).abs() < 1e-9);
    assert!((h.state.cameras.front.zoom - top).abs() < 1e-9);
}

#[test]
fn view_info_shows_and_copies_setpos() {
    let mut h = Harness::new(NavScheme::HammerPro);
    h.state.settings.workflow.show_view_info = true;
    h.frame(vec![]);
    let info = h.vp.info_rects[0];
    assert!(info.is_positive(), "the 3D view shows its information");
    assert!(!h.vp.info_rects[1].is_positive(), "not in 2D views");
    let p = info.center();
    h.move_to(p);
    assert_ne!(h.state.ui.hovered_view, Some(0), "the pill belongs to itself, not to the view");
    h.button(p, PointerButton::Primary, true);
    h.button(p, PointerButton::Primary, false);
    assert!(h.tool.clicks.is_empty(), "the click does not reach the tool");
    let expected = workflow::setpos_command(&h.state.cameras.cam3d);
    assert_eq!(h.copied.last(), Some(&expected));
    assert!(expected.starts_with("setpos "));
    h.state.settings.workflow.show_view_info = false;
    h.frame(vec![]);
    assert!(!h.vp.info_rects[0].is_positive());
}

#[test]
fn bounds_layer_is_retained_and_3d_only() {
    let mut h = Harness::new(NavScheme::HammerPro);
    assert!(h.vp.layer_names().contains(&"workflow_bounds"));
    let mut layer = workflow::BoundsLayer::default();
    h.state.settings.workflow.show_bounds = true;
    layer.prepare(&h.state);
    let v3 = view_info(0, ViewKind::Perspective, h.rect(0), &h.state.cameras, &RenderConfig::from_state(&h.state), 1.0);
    let v2 = view_info(1, ViewKind::Top, h.rect(1), &h.state.cameras, &RenderConfig::from_state(&h.state), 1.0);
    let first = layer.retained_version(&h.state, &v3);
    assert!(first.is_some());
    assert_eq!(layer.retained_version(&h.state, &v2), None);
    h.state.frame.time += 10.0;
    layer.prepare(&h.state);
    assert_eq!(layer.retained_version(&h.state, &v3), first);
    h.state.doc.select_all();
    let _ = h.state.doc.hide_selection();
    h.state.frame.time += 10.0;
    layer.prepare(&h.state);
    assert_ne!(layer.retained_version(&h.state, &v3), first);
    h.state.settings.workflow.show_bounds = false;
    layer.prepare(&h.state);
    assert_eq!(layer.retained_version(&h.state, &v3), None);
    h.state.settings.workflow.show_bounds = true;
    h.frame(vec![]);
}
