use egui::{Modifiers, Pos2, Rect, vec2};
use glam::{DVec2, DVec3};
use hp_app::testing::test_state;
use hp_app::view::{Camera2d, Camera3d};
use hp_app::{AppState, Tool, ViewInfo, ViewKind};
use hp_editor::ObjRef;
use hp_geom::Aabb;

use crate::gizmo::{GizmoLayout, GizmoMode};
use crate::input::PointerEvent;
use crate::math::{Handle, scale_rect, stretch_box, stretch_span};
use crate::select::{STRETCH_LABEL, SelectTool};
use crate::stretch::{self, FaceHandle, face_center};

const WALL: ObjRef = ObjRef::Solid(3);
const LIGHT: ObjRef = ObjRef::Entity(5);
const TRIGGER: ObjRef = ObjRef::Entity(6);

fn state() -> AppState {
    let s = test_state();
    crate::register_translations();
    crate::sync_options(&s);
    s
}

fn view_from(pos: DVec3, target: DVec3) -> ViewInfo {
    let mut cam = Camera3d { pos, ..Camera3d::default() };
    cam.look_at(target);
    ViewInfo {
        index: 0,
        kind: ViewKind::Perspective,
        rect: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
        cam3d: cam,
        cam2d: Camera2d { center: DVec2::ZERO, zoom: 0.5 },
        pixels_per_point: 1.0,
        grid: 16.0,
    }
}

fn front_view() -> ViewInfo {
    view_from(DVec3::new(0.0, -800.0, 64.0), DVec3::new(0.0, 264.0, 64.0))
}

fn corner_view() -> ViewInfo {
    view_from(DVec3::new(-700.0, -600.0, 500.0), DVec3::new(0.0, 264.0, 64.0))
}

fn bounds(state: &AppState, obj: ObjRef) -> Aabb {
    state.doc.object_bounds(obj, &|e| state.entity_bounds(e)).expect("object bounds")
}

fn scr(view: &ViewInfo, p: DVec3) -> Pos2 {
    view.world_to_screen(p).expect("point in front of the camera")
}

fn handle(tool: &SelectTool, state: &AppState, view: &ViewInfo, axis: usize, positive: bool) -> FaceHandle {
    tool.stretch_handles(state, view)
        .into_iter()
        .find(|h| h.id() == (axis, positive))
        .unwrap_or_else(|| panic!("handle {axis} {positive}"))
}

fn drag(tool: &mut SelectTool, state: &mut AppState, view: &ViewInfo, from: Pos2, to: Pos2, mods: Modifiers) {
    tool.handle_event(state, view, PointerEvent::Hover(from), Modifiers::NONE);
    tool.handle_event(state, view, PointerEvent::DragStart { origin: from, pos: from }, mods);
    for i in 1..=4 {
        let t = i as f32 / 4.0;
        tool.handle_event(state, view, PointerEvent::Drag(from + (to - from) * t), mods);
    }
    tool.handle_event(state, view, PointerEvent::DragEnd(to), mods);
}

#[allow(clippy::too_many_arguments)]
fn pull(
    tool: &mut SelectTool,
    state: &mut AppState,
    view: &ViewInfo,
    obj: ObjRef,
    axis: usize,
    positive: bool,
    offset: f64,
    mods: Modifiers,
) {
    let h = handle(tool, state, view, axis, positive);
    let b = bounds(state, obj);
    let face = face_center(&b, axis, positive);
    let mut target = face;
    target[axis] += offset;
    let to = h.knob + (scr(view, target) - scr(view, face));
    drag(tool, state, view, h.knob, to, mods);
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn stretch_span_anchors_snaps_and_keeps_a_minimum() {
    assert_eq!(stretch_span(0.0, 64.0, 1, 21.0, 16.0, true, false, 16.0), (0.0, 80.0));
    assert_eq!(stretch_span(0.0, 64.0, -1, -21.0, 16.0, true, false, 16.0), (-16.0, 64.0));
    assert_eq!(stretch_span(0.0, 64.0, 1, 21.5, 16.0, false, false, 1.0), (0.0, 85.5));
    assert_eq!(stretch_span(3.0, 61.0, 1, 1.0, 16.0, true, false, 16.0), (3.0, 64.0));
    assert_eq!(stretch_span(0.0, 64.0, 1, -500.0, 16.0, true, false, 16.0), (0.0, 16.0));
    assert_eq!(stretch_span(0.0, 64.0, -1, 500.0, 16.0, false, false, 1.0), (63.0, 64.0));
    assert_eq!(stretch_span(0.0, 64.0, 1, 16.0, 16.0, true, true, 16.0), (-16.0, 80.0));
    assert_eq!(stretch_span(0.0, 64.0, -1, 10.0, 16.0, true, true, 16.0), (16.0, 48.0));
    assert_eq!(stretch_span(0.0, 64.0, 1, -100.0, 16.0, true, true, 16.0), (24.0, 40.0));
    assert_eq!(stretch_span(0.0, 64.0, 0, 16.0, 16.0, true, false, 16.0), (0.0, 64.0));
    assert_eq!(stretch_span(0.0, 64.0, 1, f64::NAN, 16.0, true, false, 16.0), (0.0, 64.0));
}

#[test]
fn stretch_box_moves_one_face() {
    let (min, max) = (DVec3::new(0.0, 0.0, 0.0), DVec3::new(64.0, 32.0, 8.0));
    let (a, b) = stretch_box(min, max, 2, true, 37.0, 16.0, true, false);
    assert_eq!((a, b), (min, DVec3::new(64.0, 32.0, 48.0)));
    let (a, b) = stretch_box(min, max, 0, false, -16.0, 16.0, true, true);
    assert_eq!((a, b), (DVec3::new(-16.0, 0.0, 0.0), DVec3::new(80.0, 32.0, 8.0)));
    let (_, b) = stretch_box(min, max, 2, true, -100.0, 16.0, true, false);
    assert_eq!(b.z, 8.0);
    let (_, b) = stretch_box(min, max, 2, true, -100.0, 16.0, false, false);
    assert_eq!(b.z, 1.0);
    let (_, b) = stretch_box(min, max, 0, true, 1e9, 16.0, true, false);
    assert_eq!(b.x, hp_geom::MAX_COORD);
}

#[test]
fn scale_rect_sides_share_the_stretch() {
    let (min, max) = (DVec2::new(0.0, 0.0), DVec2::new(64.0, 32.0));
    let (lo, hi) = scale_rect(min, max, Handle::new(1, 0), DVec2::new(16.0, 0.0), 16.0, true, true);
    assert_eq!((lo, hi), (DVec2::new(-16.0, 0.0), DVec2::new(80.0, 32.0)));
    let (lo, hi) = scale_rect(min, max, Handle::new(1, 0), DVec2::new(16.0, 0.0), 16.0, true, false);
    let (a, b) = stretch_box(min.extend(0.0), max.extend(8.0), 0, true, 16.0, 16.0, true, false);
    assert_eq!((lo, hi), (a.truncate(), b.truncate()));
    let (lo, hi) = scale_rect(min, max, Handle::new(1, 1), DVec2::new(64.0, 5.0), 16.0, true, true);
    assert_eq!((lo, hi), (min, DVec2::new(128.0, 64.0)));
}

#[test]
fn handles_sit_on_the_faces_and_hide_when_unusable() {
    let mut state = state();
    let mut tool = SelectTool::new();
    tool.gizmo_visible = false;
    let v = front_view();
    assert!(tool.stretch_handles(&state, &v).is_empty(), "nothing selected");
    state.doc.set_selection(&[WALL]);
    let b = bounds(&state, WALL);
    let list = tool.stretch_handles(&state, &v);
    let mut ids: Vec<(usize, bool)> = list.iter().map(FaceHandle::id).collect();
    ids.sort();
    assert_eq!(ids, vec![(0, false), (0, true), (2, false), (2, true)]);
    let top = handle(&tool, &state, &v, 2, true);
    assert!(top.face.distance(scr(&v, face_center(&b, 2, true))) < 0.5);
    assert!(top.dir.y < -0.99, "the top arrow points up on screen: {:?}", top.dir);
    assert!(!top.front);
    let gap = stretch::GAP * stretch::BACK_SCALE;
    assert!((top.knob.distance(top.face) - gap).abs() < 0.01, "constant screen size");
    assert_eq!(stretch::hit(&list, top.knob), Some((2, true)));
    assert_eq!(stretch::hit(&list, top.knob + vec2(stretch::PICK + 7.0, 0.0)), None);
    assert_eq!(stretch::hit(&list, Pos2::new(f32::NAN, 0.0)), None);
    let far = view_from(DVec3::new(0.0, -1800.0, 64.0), DVec3::new(0.0, 264.0, 64.0));
    let top_far = handle(&tool, &state, &far, 2, true);
    assert!((top_far.knob.distance(top_far.face) - gap).abs() < 0.01);
    let c = corner_view();
    let list = tool.stretch_handles(&state, &c);
    assert_eq!(list.len(), 6);
    let front: Vec<(usize, bool)> = list.iter().filter(|h| h.front).map(FaceHandle::id).collect();
    assert_eq!(front.len(), 3);
    assert!(front.contains(&(0, false)) && front.contains(&(1, false)) && front.contains(&(2, true)), "{front:?}");
    let back = list.iter().find(|h| !h.front).expect("a back handle");
    let mut both = vec![*back];
    let mut fake_front = *back;
    fake_front.front = true;
    fake_front.axis = 2;
    both.push(fake_front);
    assert_eq!(stretch::hit(&both, back.knob), Some((2, back.positive)), "the front one wins");
    assert_eq!(stretch::resize_cursor(vec2(1.0, 0.0)), egui::CursorIcon::ResizeHorizontal);
    assert_eq!(stretch::resize_cursor(vec2(0.0, -1.0)), egui::CursorIcon::ResizeVertical);
    assert_eq!(stretch::resize_cursor(vec2(0.7, 0.7)), egui::CursorIcon::ResizeNwSe);
    assert_eq!(stretch::resize_cursor(vec2(0.7, -0.7)), egui::CursorIcon::ResizeNeSw);
    let mut top_view = front_view();
    top_view.kind = ViewKind::Top;
    assert!(tool.stretch_handles(&state, &top_view).is_empty());
}

#[test]
fn handles_slide_out_of_the_gizmo() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = front_view();
    state.doc.set_selection(&[WALL]);
    let b = bounds(&state, WALL);
    let top = handle(&tool, &state, &v, 2, true);
    let g = GizmoLayout::new(&v, b.center()).expect("gizmo");
    assert!(top.knob.distance(top.face) > stretch::GAP, "pushed out: {top:?}");
    for mode in GizmoMode::ALL {
        tool.set_gizmo_mode(mode);
        for h in tool.stretch_handles(&state, &v) {
            assert!(g.hit(&v, h.knob, mode).is_none(), "{mode:?} {h:?}");
        }
    }
    tool.set_gizmo_mode(GizmoMode::Move);
    let arrow = scr(&v, g.center + DVec3::Z * g.size * 0.6);
    tool.handle_event(&mut state, &v, PointerEvent::Hover(arrow), Modifiers::NONE);
    assert!(tool.hovered_gizmo().is_some());
    assert_eq!(tool.stretch_hit(&state, &v, arrow), None);
    assert_eq!(tool.stretch_hit(&state, &v, top.knob), Some((2, true)));
    pull(&mut tool, &mut state, &v, WALL, 2, true, 32.0, Modifiers::NONE);
    assert!(approx(bounds(&state, WALL).max.z, b.max.z + 32.0));
    tool.gizmo_visible = false;
    let top = handle(&tool, &state, &v, 2, true);
    assert!((top.knob.distance(top.face) - stretch::GAP * stretch::BACK_SCALE).abs() < 0.01);
}

#[test]
fn point_entities_alone_have_no_handles_and_the_setting_hides_them() {
    let mut state = state();
    let tool = SelectTool::new();
    let v = corner_view();
    state.doc.set_selection(&[LIGHT]);
    assert!(tool.stretch_handles(&state, &v).is_empty(), "a light is only moved");
    state.doc.set_selection(&[TRIGGER]);
    assert!(!tool.stretch_handles(&state, &v).is_empty());
    state.doc.set_selection(&[WALL, LIGHT]);
    assert!(!tool.stretch_handles(&state, &v).is_empty());
    state.settings.selection.stretch_handles = false;
    assert!(tool.stretch_handles(&state, &v).is_empty());
}

#[test]
fn pulling_a_face_stretches_with_one_undo_step() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = front_view();
    state.doc.set_selection(&[WALL]);
    let b = bounds(&state, WALL);
    let steps = state.doc.undo_labels().len();
    let top = handle(&tool, &state, &v, 2, true);
    tool.handle_event(&mut state, &v, PointerEvent::Hover(top.knob), Modifiers::NONE);
    let hint = tool.status_hint(&state).unwrap_or_default();
    assert!(hint.contains("+Z"), "{hint}");
    assert_eq!(tool.cursor(&state, &v, top.knob), Some(egui::CursorIcon::ResizeVertical));
    pull(&mut tool, &mut state, &v, WALL, 2, true, 64.0, Modifiers::NONE);
    let nb = bounds(&state, WALL);
    assert!(approx(nb.max.z, b.max.z + 64.0) && approx(nb.min.z, b.min.z), "{nb:?}");
    assert!(approx(nb.size().x, b.size().x) && approx(nb.size().y, b.size().y), "{nb:?}");
    assert_eq!(state.doc.undo_label(), Some(STRETCH_LABEL));
    assert_eq!(state.doc.undo_labels().len(), steps + 1, "one undo step");
    assert!(hp_app::i18n::has_key(&format!("shell.undo.{}", STRETCH_LABEL.to_ascii_lowercase())));
    state.doc.undo();
    assert_eq!(bounds(&state, WALL), b);
    pull(&mut tool, &mut state, &v, WALL, 2, false, -32.0, Modifiers::NONE);
    let nb = bounds(&state, WALL);
    assert!(approx(nb.min.z, b.min.z - 32.0) && approx(nb.max.z, b.max.z), "{nb:?}");
}

#[test]
fn stretch_snaps_unless_alt_and_ctrl_stretches_from_the_center() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = front_view();
    let grid = state.grid_size();
    state.doc.set_selection(&[WALL]);
    let b = bounds(&state, WALL);
    pull(&mut tool, &mut state, &v, WALL, 0, true, grid * 2.0 + grid * 0.3, Modifiers::NONE);
    let nb = bounds(&state, WALL);
    assert!(approx(nb.max.x, b.max.x + grid * 2.0), "{nb:?}");
    state.doc.undo();
    pull(&mut tool, &mut state, &v, WALL, 0, true, 21.0, Modifiers::ALT);
    let nb = bounds(&state, WALL);
    assert!((nb.max.x - (b.max.x + 21.0)).abs() < 0.5, "{nb:?}");
    assert!(approx(nb.min.x, b.min.x));
    state.doc.undo();
    pull(&mut tool, &mut state, &v, WALL, 0, true, grid * 2.0, Modifiers::CTRL);
    let nb = bounds(&state, WALL);
    assert!(approx(nb.max.x, b.max.x + grid * 2.0) && approx(nb.min.x, b.min.x - grid * 2.0), "{nb:?}");
    assert!(approx(nb.center().x, b.center().x));
    assert_eq!(state.doc.undo_label(), Some(STRETCH_LABEL));
    state.doc.undo();
    pull(&mut tool, &mut state, &v, WALL, 2, true, -1000.0, Modifiers::NONE);
    let nb = bounds(&state, WALL);
    assert!(approx(nb.size().z, grid.min(b.size().z)), "{nb:?}");
    assert!(approx(nb.min.z, b.min.z));
}

#[test]
fn stretch_matches_the_2d_scale_and_respects_texture_lock() {
    for lock in [true, false] {
        let mut a = state();
        let mut b = state();
        a.doc.options_mut().texture_lock = lock;
        b.doc.options_mut().texture_lock = lock;
        a.doc.set_selection(&[WALL]);
        let from = bounds(&a, WALL);
        let mut tool = SelectTool::new();
        pull(&mut tool, &mut a, &front_view(), WALL, 2, true, 64.0, Modifiers::NONE);
        let to = bounds(&a, WALL);
        b.doc.scale_bounds(&[WALL], &from, &to).expect("scale");
        let (sa, sb) = (a.doc.scene().solid(3).expect("wall"), b.doc.scene().solid(3).expect("wall"));
        assert_eq!(sa.solid, sb.solid, "lock {lock}");
    }
    let side = |lock: bool| {
        let mut s = state();
        s.doc.options_mut().texture_lock = lock;
        s.doc.set_selection(&[WALL]);
        let mut tool = SelectTool::new();
        pull(&mut tool, &mut s, &front_view(), WALL, 2, true, 64.0, Modifiers::NONE);
        let solid = &s.doc.scene().solid(3).expect("wall").solid;
        solid.sides.iter().map(|f| (f.uaxis, f.vaxis)).collect::<Vec<_>>()
    };
    assert_ne!(side(true), side(false));
}

#[test]
fn shift_clones_and_clicks_on_handles_do_not_select() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = front_view();
    state.doc.set_selection(&[WALL]);
    let b = bounds(&state, WALL);
    let solids = state.doc.scene().solids().len();
    pull(&mut tool, &mut state, &v, WALL, 2, true, 64.0, Modifiers::SHIFT);
    assert_eq!(state.doc.scene().solids().len(), solids + 1, "a stretched copy");
    assert_eq!(bounds(&state, WALL), b, "the original stays");
    assert_eq!(state.doc.undo_label(), Some("Duplicate"));
    state.doc.undo();
    state.doc.set_selection(&[WALL]);
    let top = handle(&tool, &state, &v, 2, true);
    tool.handle_event(&mut state, &v, PointerEvent::Click(top.knob), Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
    tool.handle_event(&mut state, &v, PointerEvent::Click(Pos2::new(5.0, 5.0)), Modifiers::NONE);
    assert!(state.doc.selected_objects().is_empty());
    state.doc.set_selection(&[WALL]);
    let top = handle(&tool, &state, &v, 2, true);
    tool.handle_event(&mut state, &v, PointerEvent::DragStart { origin: top.knob, pos: top.knob }, Modifiers::NONE);
    tool.handle_event(&mut state, &v, PointerEvent::Drag(top.knob - vec2(0.0, 80.0)), Modifiers::NONE);
    assert!(tool.is_dragging());
    assert!(tool.cancel(&mut state));
    assert_eq!(bounds(&state, WALL), b);
}

#[test]
fn stretch_handles_are_a_tool_option_and_a_setting() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let value = |tool: &SelectTool, key: &str| {
        tool.option_values().into_iter().find(|o| o.key == key).map(|o| o.value).expect("option")
    };
    assert_eq!(value(&tool, "stretch_handles"), "on");
    assert!(tool.set_option(&mut state, "stretch_handles", "off"));
    assert!(!state.settings.selection.stretch_handles && state.settings_dirty);
    assert_eq!(value(&tool, "stretch_handles"), "off");
    assert!(!tool.set_option(&mut state, "stretch_handles", "maybe"));
    for (k, v) in
        [("handles", "skew"), ("box", "centers"), ("gizmo", "off"), ("gizmo_mode", "scale"), ("gizmo_local", "on")]
    {
        assert!(tool.set_option(&mut state, k, v), "{k}");
        assert_eq!(value(&tool, k), v, "{k}");
    }
    assert!(state.settings.selection.gizmo_local);
    assert!(!tool.set_option(&mut state, "handles", "bend"));
    assert!(!tool.set_option(&mut state, "nope", "on"));
    for o in tool.option_values() {
        assert!(tool.set_option(&mut state, o.key, &o.value), "{} {}", o.key, o.value);
    }
    assert!(tool.set_option(&mut state, "stretch_handles", "on"));
    crate::sync_options(&state);
    assert_eq!(value(&tool, "stretch_handles"), "on");
}
