use super::*;

fn above(x: f64, y: f64) -> ViewInfo {
    view3d(DVec3::new(x, y, 600.0), 0.0, -89.0)
}

fn press_at(p: Pos2) -> PointerFrame {
    PointerFrame { pos: Some(p), pressed: true, down: true, mods: Modifiers::NONE, ..PointerFrame::default() }
}

fn next_frame(state: &mut AppState, dt: f32) {
    state.frame.index += 1;
    state.frame.time += f64::from(dt);
    state.frame.dt = dt;
}

#[test]
fn a_brush_held_still_asks_for_frames_and_keeps_painting() {
    let (mut state, mut tool, face) = floor_with_disp(3);
    state.doc.clear_face_selection();
    let view = above(0.0, 0.0);
    let c = view.rect.center();
    assert!(!tool.needs_frames(), "idle");
    tool.handle_pointer(&mut state, &view, press_at(c));
    assert!(tool.needs_frames(), "a press that may become a stroke");
    assert!(!tool.is_painting(), "not yet");
    let mut peaks = Vec::new();
    for _ in 0..6 {
        next_frame(&mut state, 0.1);
        tool.handle_pointer(&mut state, &view, PointerFrame { pressed: false, ..press_at(c) });
        assert!(tool.needs_frames(), "the stroke keeps asking for frames");
        peaks.push(distances(&state, face).iter().copied().fold(0.0, f64::max));
    }
    assert!(tool.is_painting(), "holding still starts painting");
    for w in peaks.windows(2) {
        if let [a, b] = w
            && *a > 0.0
        {
            assert!(b > a, "every frame of a still brush paints more: {peaks:?}");
        }
    }
    assert!(peaks.last().is_some_and(|p| *p > 10.0), "{peaks:?}");
    tool.handle_pointer(&mut state, &view, PointerFrame { pressed: false, down: false, ..press_at(c) });
    assert!(!tool.is_painting());
    assert!(!tool.needs_frames(), "released");
    assert_eq!(state.doc.selected_faces(), Vec::<FaceRef>::new(), "a held press paints, it does not select");
    tool.interaction = Interaction::Select;
    tool.handle_pointer(&mut state, &view, press_at(c));
    assert!(!tool.needs_frames(), "select mode");
    tool.handle_pointer(&mut state, &view, PointerFrame { pressed: false, down: false, ..press_at(c) });
    tool.interaction = Interaction::Geometry;
    let top = view2d(ViewKind::Top, DVec2::ZERO, 0.5);
    let tc = top.rect.center();
    tool.handle_pointer(&mut state, &top, press_at(tc));
    assert!(!tool.needs_frames(), "2D views never paint");
    tool.handle_pointer(&mut state, &top, PointerFrame { pressed: false, down: false, ..press_at(tc) });
    state.doc.clear_face_selection();
    tool.interaction = Interaction::Sculpt;
    tool.handle_pointer(&mut state, &view, press_at(c));
    assert!(tool.needs_frames(), "sculpt press");
    next_frame(&mut state, 0.3);
    tool.handle_pointer(&mut state, &view, PointerFrame { pressed: false, ..press_at(c) });
    assert!(tool.is_painting() && tool.needs_frames(), "sculpt stroke");
    tool.handle_pointer(&mut state, &view, PointerFrame { pressed: false, down: false, ..press_at(c) });
    assert!(!tool.needs_frames());
}

fn two_tiles() -> (AppState, DisplacementTool, FaceRef, FaceRef) {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let a = block(&mut state, DVec3::new(-256.0, -128.0, -16.0), DVec3::new(0.0, 128.0, 0.0));
    let b = block(&mut state, DVec3::new(0.0, -128.0, -16.0), DVec3::new(256.0, 128.0, 0.0));
    let (fa, fb) = (side_facing(&state, a, DVec3::Z), side_facing(&state, b, DVec3::Z));
    make_disps(&mut state, &mut tool, &[fa, fb], 3);
    state.doc.select_faces(&[fa], SelectOp::Replace);
    (state, tool, fa, fb)
}

#[test]
fn a_brush_over_unselected_displacements_says_it_does_not_paint() {
    let (mut state, mut tool, fa, fb) = two_tiles();
    let over_b = above(128.0, 0.0);
    let c = over_b.rect.center();
    tool.handle_pointer(&mut state, &over_b, PointerFrame { pos: Some(c), ..PointerFrame::default() });
    assert!(tool.cursor_blocked(), "over the unselected tile");
    let hint = tool.status_hint(&state).expect("hint");
    assert_eq!(hint, hp_app::tf!("disp.hint_not_selected", hp_app::actions::key_label(egui::Key::Escape)));
    let mut out = hp_render::Overlay::default();
    tool.overlay(&state, &over_b, &mut out);
    let gray = crate::common::rgba(crate::disp::BLOCKED);
    assert!(out.lines_on_top.iter().filter(|l| l.color == gray).count() >= 48, "the ring is gray");
    let steps = undo_steps(&state);
    let toasts = state.toasts.live().len();
    tool.handle_pointer(&mut state, &over_b, press_at(c));
    for i in 1..=8 {
        next_frame(&mut state, 0.05);
        let at = c + vec2(i as f32 * 4.0, 0.0);
        tool.handle_pointer(&mut state, &over_b, PointerFrame { pos: Some(at), pressed: false, ..press_at(c) });
    }
    assert!(tool.is_painting());
    assert_eq!(state.toasts.live().len(), toasts + 1, "told once for the stroke");
    tool.handle_pointer(&mut state, &over_b, PointerFrame { pressed: false, down: false, ..press_at(c) });
    assert!(untouched(&state, fa) && untouched(&state, fb), "nothing painted");
    assert_eq!(undo_steps(&state), steps, "no empty undo step");
    assert_eq!(state.doc.selected_faces(), vec![fa], "the selection stays");
    let over_a = above(-128.0, 0.0);
    let ca = over_a.rect.center();
    tool.handle_pointer(&mut state, &over_a, PointerFrame { pos: Some(ca), ..PointerFrame::default() });
    assert!(!tool.cursor_blocked(), "over the selected tile");
    assert_ne!(tool.status_hint(&state), Some(hint.clone()));
    tool.handle_pointer(&mut state, &over_a, press_at(ca));
    for i in 1..=4 {
        next_frame(&mut state, 0.05);
        let at = ca + vec2(i as f32 * 4.0, 0.0);
        tool.handle_pointer(&mut state, &over_a, PointerFrame { pos: Some(at), pressed: false, ..press_at(ca) });
    }
    tool.handle_pointer(&mut state, &over_a, PointerFrame { pressed: false, down: false, ..press_at(ca) });
    assert!(!untouched(&state, fa), "the selected tile is painted");
    assert!(untouched(&state, fb));
    assert_eq!(undo_steps(&state), steps + 1);
    state.doc.clear_face_selection();
    tool.handle_pointer(&mut state, &over_b, PointerFrame { pos: Some(c), ..PointerFrame::default() });
    assert!(!tool.cursor_blocked(), "no selection: every displacement is painted");
    state.doc.select_faces(&[fa], SelectOp::Replace);
    tool.interaction = Interaction::Select;
    tool.handle_pointer(&mut state, &over_b, PointerFrame { pos: Some(c + vec2(1.0, 0.0)), ..PointerFrame::default() });
    assert!(!tool.cursor_blocked());
}

#[test]
fn the_brush_circle_follows_the_camera_under_a_still_mouse() {
    let (mut state, mut tool, _, _) = two_tiles();
    state.doc.clear_face_selection();
    let first = above(-128.0, 0.0);
    let c = first.rect.center();
    tool.handle_pointer(&mut state, &first, PointerFrame { pos: Some(c), ..PointerFrame::default() });
    let at = state.hover_world.expect("brush over the first tile");
    assert!((at.x + 128.0).abs() < 16.0, "{at}");
    next_frame(&mut state, 0.016);
    let second = above(128.0, 0.0);
    tool.handle_pointer(&mut state, &second, PointerFrame { pos: Some(c), ..PointerFrame::default() });
    let at = state.hover_world.expect("brush over the second tile");
    assert!((at.x - 128.0).abs() < 16.0, "the circle moved with the camera: {at}");
    let mut out = hp_render::Overlay::default();
    tool.overlay(&state, &second, &mut out);
    let xs: Vec<f64> = out.lines_on_top.iter().map(|l| f64::from(l.a[0])).collect();
    assert!(!xs.is_empty() && xs.iter().all(|x| *x > -1.0), "the ring is over the second tile: {xs:?}");
}

#[test]
fn project_without_a_target_brush_says_so() {
    let (mut state, mut tool, face) = floor_with_disp(4);
    state.doc.clear_face_selection();
    state.doc.clear_selection();
    tool.interaction = Interaction::Sculpt;
    tool.brush.sculpt.op = crate::disp::sculpt::SculptOp::Project;
    tool.brush.radius = 96.0;
    let view = above(0.0, 0.0);
    let c = view.rect.center();
    let steps = undo_steps(&state);
    let toasts = state.toasts.live().len();
    tool.handle_pointer(&mut state, &view, press_at(c));
    for i in 1..=6 {
        next_frame(&mut state, 0.05);
        let at = c + vec2(i as f32 * 3.0, 0.0);
        tool.handle_pointer(&mut state, &view, PointerFrame { pos: Some(at), pressed: false, ..press_at(c) });
    }
    assert!(tool.is_painting());
    assert_eq!(state.toasts.live().len(), toasts + 1, "told once for the stroke");
    let text = state.toasts.live().last().map(|t| t.text.clone()).unwrap_or_default();
    assert_eq!(text, hp_app::t!("disp.sculpt.project_none"));
    tool.handle_pointer(&mut state, &view, PointerFrame { pressed: false, down: false, ..press_at(c) });
    assert!(untouched(&state, face));
    assert_eq!(undo_steps(&state), steps, "no empty undo step");
    let slab = block(&mut state, DVec3::new(-128.0, -128.0, 64.0), DVec3::new(128.0, 128.0, 96.0));
    state.doc.select(&[ObjRef::Solid(slab)], SelectOp::Replace);
    let toasts = state.toasts.live().len();
    tool.handle_pointer(&mut state, &view, press_at(c));
    for _ in 0..10 {
        next_frame(&mut state, 0.05);
        tool.handle_pointer(&mut state, &view, PointerFrame { pressed: false, ..press_at(c) });
    }
    tool.handle_pointer(&mut state, &view, PointerFrame { pressed: false, down: false, ..press_at(c) });
    assert_eq!(state.toasts.live().len(), toasts, "nothing to say");
    let top = mesh(&state, face).positions.iter().map(|p| p.z).fold(f64::MIN, f64::max);
    assert!((top - 64.0).abs() < 1e-6, "the center reached the slab: {top}");
    assert_eq!(undo_steps(&state), steps + 2, "the slab and the stroke");
}

#[test]
fn script_options_set_the_brush() {
    let mut state = state_with(&[FLOOR]);
    let mut tool = DisplacementTool::new();
    for (k, v) in [
        ("mode", "sculpt"),
        ("sculpt", "carve"),
        ("effect", "smooth"),
        ("axis", "z"),
        ("falloff", "linear"),
        ("radius", "200"),
        ("strength", "300"),
        ("spatial", "on"),
        ("autosew", "on"),
        ("line_of_sight", "on"),
    ] {
        assert!(tool.set_option(&mut state, k, v), "{k} {v}");
    }
    assert_eq!(tool.interaction, Interaction::Sculpt);
    assert_eq!(tool.brush.sculpt.op, crate::disp::sculpt::SculptOp::Carve);
    assert_eq!(tool.brush.geo_effect, GeoEffect::Smooth);
    assert_eq!(tool.brush.axis, PaintAxis::Z);
    assert_eq!(tool.brush.falloff, Falloff::Linear);
    assert_eq!((tool.brush.radius, tool.brush.strength), (200.0, 300.0));
    assert!(tool.brush.spatial && tool.brush.autosew && tool.brush.line_of_sight);
    for (k, v) in [("mode", "fly"), ("radius", "-3"), ("radius", "nan"), ("create", "7"), ("nonsense", "1")] {
        assert!(!tool.set_option(&mut state, k, v), "{k} {v}");
    }
    assert!(tool.set_option(&mut state, "create", "3"));
    assert_eq!(state.doc.disp_faces().len(), 1);
}

#[test]
fn every_brush_option_reads_back() {
    use hp_app::Tool as _;
    let mut state = state_with(&[FLOOR]);
    let mut tool = DisplacementTool::new();
    let changes = [
        ("mode", "alpha"),
        ("effect", "set"),
        ("axis", "view"),
        ("custom_axis", "0 1 0"),
        ("radius", "300"),
        ("strength", "900"),
        ("smooth", "0.25"),
        ("target_height", "-40"),
        ("falloff", "constant"),
        ("spatial", "on"),
        ("line_of_sight", "on"),
        ("autosew", "on"),
        ("selected_only", "off"),
        ("continuous", "off"),
        ("alpha_effect", "set"),
        ("alpha_value", "100"),
        ("alpha_strength", "50"),
        ("blend_channel", "3"),
        ("sculpt", "project"),
        ("push_mode", "adaptive"),
        ("sculpt_bounds", "additive"),
        ("falloff_position", "30"),
        ("falloff_final", "20"),
        ("push_distance", "12"),
        ("push_percent", "60"),
        ("carve_depth", "20"),
        ("carve_profile", "1 0.5 0.5 0.5 0"),
        ("project_range", "800"),
        ("scale_factor", "-2"),
        ("tag_kind", "remove"),
        ("tag_value", "off"),
        ("mask_grid", "on"),
        ("mask_select", "on"),
    ];
    let listed: Vec<&str> = tool.option_values().iter().map(|o| o.key).collect();
    assert_eq!(listed, changes.iter().map(|(k, _)| *k).collect::<Vec<_>>());
    for (k, v) in changes {
        assert!(tool.set_option(&mut state, k, v), "{k} {v}");
        let now = tool.option_values().into_iter().find(|o| o.key == k).map(|o| o.value).unwrap_or_default();
        assert_eq!(now, v, "{k}");
    }
    let brush = tool.brush.clone();
    for o in tool.option_values() {
        assert!(tool.set_option(&mut state, o.key, &o.value), "{} {}", o.key, o.value);
        assert!(!o.takes.is_empty());
    }
    assert_eq!(tool.brush, brush);
    assert_eq!(tool.brush.blend_channel, Some(2));
    assert_eq!(tool.brush.axis, PaintAxis::View);
    for (k, v) in [
        ("custom_axis", "0 0 0"),
        ("smooth", "2"),
        ("blend_channel", "5"),
        ("carve_profile", "1 2"),
        ("alpha_value", "300"),
        ("push_mode", "far"),
        ("scale_factor", "99"),
        ("tag_value", "maybe"),
        ("tag_kind", "lava"),
        ("falloff_final", "101"),
        ("sculpt_bounds", "far"),
    ] {
        assert!(!tool.set_option(&mut state, k, v), "{k} {v}");
    }
}

#[test]
fn resizing_keeps_the_brush_circle_in_place() {
    for right in [false, true] {
        let (mut state, mut tool, _) = floor_with_disp(4);
        state.doc.clear_face_selection();
        let view = above(0.0, 0.0);
        let c = view.rect.center();
        tool.handle_pointer(&mut state, &view, PointerFrame { pos: Some(c), ..PointerFrame::default() });
        let start = state.hover_world.expect("brush");
        let press = if right {
            PointerFrame { pos: Some(c), right_pressed: true, right_down: true, ..PointerFrame::default() }
        } else {
            PointerFrame { mods: Modifiers { alt: true, ..Modifiers::NONE }, ..press_at(c) }
        };
        tool.handle_pointer(&mut state, &view, press);
        let ring_radius = |tool: &DisplacementTool, state: &AppState| {
            let mut out = hp_render::Overlay::default();
            tool.overlay(state, &view, &mut out);
            let color = crate::common::rgba(hp_app::theme::VIEW_ACCENT);
            let ring: Vec<DVec3> = out
                .lines_on_top
                .iter()
                .filter(|l| l.color == color)
                .map(|l| DVec3::new(f64::from(l.a[0]), f64::from(l.a[1]), 0.0))
                .collect();
            assert!(ring.len() >= 48, "right {right}: the circle shows during the drag");
            ring.iter().map(|p| p.distance(DVec3::new(start.x, start.y, 0.0))).fold(0.0, f64::max)
        };
        let mut last = ring_radius(&tool, &state);
        for i in 1..=5 {
            next_frame(&mut state, 0.016);
            let at = c + vec2(i as f32 * 30.0, 0.0);
            let drag = PointerFrame { pos: Some(at), pressed: false, right_pressed: false, ..press };
            tool.handle_pointer(&mut state, &view, drag);
            let now = ring_radius(&tool, &state);
            assert!(now > last, "right {right}: the circle grows with the drag ({last} then {now})");
            assert!((now - tool.brush.radius).abs() < 1.0, "right {right}: drawn at the new size");
            last = now;
        }
        assert!(tool.brush.radius > 64.0 * 1.8, "resized: {}", tool.brush.radius);
        let at = state.hover_world.expect("brush");
        assert!(at.distance(start) < 1e-6, "right {right}: the circle stayed at {start}, not {at}");
        assert!(!tool.is_painting(), "resizing does not paint");
    }
}

#[test]
fn the_brush_color_follows_the_sculpt_modifiers() {
    use hp_app::theme::{AXIS_Z, VIEW_ACCENT, VIEW_SUCCESS};
    let (mut state, mut tool, _) = floor_with_disp(4);
    state.doc.clear_face_selection();
    let view = above(0.0, 0.0);
    let c = view.rect.center();
    let ring_has = |tool: &DisplacementTool, state: &AppState, color: egui::Color32| {
        let mut out = hp_render::Overlay::default();
        tool.overlay(state, &view, &mut out);
        let want = crate::common::rgba(color);
        out.lines_on_top.iter().filter(|l| l.color == want).count() >= 48
    };
    let shift = Modifiers { shift: true, ..Modifiers::NONE };
    for (interaction, mods, color) in [
        (Interaction::Sculpt, Modifiers::NONE, VIEW_ACCENT),
        (Interaction::Sculpt, ctrl(), AXIS_Z),
        (Interaction::Sculpt, shift, VIEW_SUCCESS),
        (Interaction::Geometry, Modifiers::NONE, VIEW_ACCENT),
        (Interaction::Geometry, shift, AXIS_Z),
        (Interaction::Geometry, ctrl(), VIEW_ACCENT),
    ] {
        tool.interaction = interaction;
        next_frame(&mut state, 0.016);
        tool.handle_pointer(&mut state, &view, PointerFrame { pos: Some(c), mods, ..PointerFrame::default() });
        assert!(ring_has(&tool, &state, color), "{interaction:?} {mods:?}");
    }
}
