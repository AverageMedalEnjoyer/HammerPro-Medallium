use egui::RichText;
use glam::DVec3;
use hp_app::{Action, AppState, Severity, t, tf, theme};

const LEAK_ENTITY_TOLERANCE: f64 = 4.0;

fn leak_entity_present(state: &AppState, start: DVec3) -> bool {
    state
        .doc
        .scene()
        .entities()
        .values()
        .any(|n| n.entity.origin().is_some_and(|o| o.distance(start) <= LEAK_ENTITY_TOLERANCE))
}

fn drop_fixed_leak(ctx: &egui::Context, state: &mut AppState) -> bool {
    let Some(start) = state.pointfile.as_ref().and_then(|pf| pf.points.last().copied()) else { return false };
    let path = state.pointfile.as_ref().map(|pf| pf.path.clone()).unwrap_or_default();
    let key = egui::Id::unique("hp_pointfile_leak_entity_seen").with((
        path,
        start.x.to_bits(),
        start.y.to_bits(),
        start.z.to_bits(),
    ));
    if leak_entity_present(state, start) {
        ctx.data_mut(|d| d.insert_temp(key, true));
        return false;
    }
    if !ctx.data(|d| d.get_temp::<bool>(key)).unwrap_or(false) {
        return false;
    }
    ctx.data_mut(|d| d.remove::<bool>(key));
    state.pointfile = None;
    state.notify(Severity::Info, t!("pointfile.leak_entity_gone"));
    true
}

pub(crate) fn leak_entity_index(n: usize) -> usize {
    n.saturating_sub(1)
}

pub(crate) fn point_number(n: usize, index: usize) -> usize {
    n.saturating_sub(index.min(leak_entity_index(n)))
}

pub(crate) fn next_index(index: usize) -> Option<usize> {
    index.checked_sub(1)
}

pub(crate) fn previous_index(n: usize, index: usize) -> Option<usize> {
    (index + 1 < n).then_some(index + 1)
}

pub(crate) fn path_camera(points: &[DVec3], cursor: usize) -> Option<(DVec3, DVec3)> {
    let p = *points.get(cursor)?;
    if let Some(next) = next_index(cursor).and_then(|i| points.get(i))
        && next.distance_squared(p) > 1e-9
    {
        return Some((p, *next));
    }
    if let Some(prev) = previous_index(points.len(), cursor).and_then(|i| points.get(i))
        && prev.distance_squared(p) > 1e-9
    {
        return Some((p, p + (p - *prev)));
    }
    Some((p, p + DVec3::X))
}

pub(crate) fn move_to_point(state: &mut AppState, cursor: usize) {
    let Some(pf) = &mut state.pointfile else { return };
    if pf.points.is_empty() {
        return;
    }
    pf.cursor = cursor.min(leak_entity_index(pf.points.len()));
    if let Some((pos, target)) = path_camera(&pf.points, pf.cursor) {
        state.cameras.cam3d.pos = pos;
        state.cameras.cam3d.look_at(target);
        state.cameras.center_2d(pos);
    }
}

pub fn go_to_leak_start(state: &mut AppState) {
    if let Some(n) = state.pointfile.as_ref().map(|p| p.points.len()) {
        move_to_point(state, leak_entity_index(n));
    }
}

fn end_label(n: usize, cursor: usize) -> Option<&'static str> {
    if n == 0 {
        None
    } else if cursor == leak_entity_index(n) {
        Some(t!("pointfile.at_entity"))
    } else if cursor == 0 {
        Some(t!("pointfile.at_void"))
    } else {
        None
    }
}

const NAVIGATOR_SIZE: egui::Vec2 = egui::vec2(360.0, 170.0);

pub(crate) fn free_pos(screen: egui::Rect, size: egui::Vec2, avoid: &[egui::Rect]) -> egui::Pos2 {
    let (l, r) = (screen.left() + 80.0, screen.right() - size.x - 80.0);
    let (t, b) = (screen.top() + 120.0, screen.bottom() - size.y - 70.0);
    let mut candidates = vec![egui::pos2(l, b), egui::pos2(l, t), egui::pos2(r, b), egui::pos2(r, t)];
    if let Some(a) = avoid.first() {
        candidates.push(egui::pos2(a.left() - size.x - 12.0, a.top()));
        candidates.push(egui::pos2(a.right() + 12.0, a.top()));
        candidates.push(egui::pos2(a.left(), a.bottom() + 8.0));
        candidates.push(egui::pos2(a.left(), a.top() - size.y - 8.0));
    }
    let clamp = |p: egui::Pos2| {
        let max = (screen.max - size - egui::vec2(4.0, 4.0)).max(screen.min);
        p.clamp(screen.min + egui::vec2(4.0, 40.0), max.max(screen.min + egui::vec2(4.0, 40.0)))
    };
    let covered = |p: egui::Pos2| {
        let rect = egui::Rect::from_min_size(p, size);
        avoid.iter().filter(|a| a.intersects(rect)).map(|a| a.intersect(rect).area()).sum::<f32>()
    };
    let mut best = clamp(candidates[0]);
    let mut best_cover = covered(best);
    for c in candidates.into_iter().map(clamp) {
        let cover = covered(c);
        if cover < best_cover {
            best = c;
            best_cover = cover;
        }
    }
    best
}

fn compile_window_rect(ctx: &egui::Context) -> Option<egui::Rect> {
    let id = crate::compile::window_id();
    let layer = egui::LayerId::new(egui::Order::Middle, id);
    ctx.memory(|m| m.areas().is_visible(&layer).then(|| m.area_rect(id)).flatten())
}

pub(crate) fn window_id() -> egui::Id {
    egui::Id::unique("hp_pointfile_window")
}

pub(crate) fn show(ctx: &egui::Context, state: &mut AppState) {
    if drop_fixed_leak(ctx, state) {
        return;
    }
    let shown_key = egui::Id::unique("hp_pointfile_window_shown");
    let Some(pf) = &state.pointfile else {
        ctx.data_mut(|d| d.insert_temp(shown_key, false));
        return;
    };
    let appearing = !ctx.data(|d| d.get_temp::<bool>(shown_key)).unwrap_or(false);
    ctx.data_mut(|d| d.insert_temp(shown_key, true));
    let n = pf.points.len();
    let cursor = pf.cursor.min(leak_entity_index(n));
    let current = pf.points.get(cursor).copied();
    let file = pf.path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    let mut target: Option<usize> = None;
    let mut unload = false;
    let screen = ctx.content_rect();
    let avoid: Vec<egui::Rect> = compile_window_rect(ctx).into_iter().collect();
    let window_id = window_id();
    hp_app::widgets::register_palette_window(ctx, window_id);
    let size = ctx.memory(|m| m.area_rect(window_id)).map_or(NAVIGATOR_SIZE, |r| r.size().max(NAVIGATOR_SIZE * 0.5));
    let title = tf!(crate::util::plural(n, "pointfile.title_one", "pointfile.title"), n);
    let mut window = hp_app::chrome::Window::new(RichText::new(title).color(theme::error()))
        .id(window_id)
        .default_pos(free_pos(screen, size, &avoid));
    if appearing {
        window = window.current_pos(free_pos(screen, size, &avoid));
    }
    let response = window.resizable(false).collapsible(true).show(ctx, |ui| {
        ui.label(RichText::new(file).weak().small())
            .on_hover_text(state.pointfile.as_ref().map(|p| p.path.display().to_string()).unwrap_or_default());
        if n == 0 {
            ui.label(t!("pointfile.empty"));
        } else {
            ui.horizontal(|ui| {
                ui.label(tf!("pointfile.position", point_number(n, cursor), n));
                if let Some(end) = end_label(n, cursor) {
                    ui.label(RichText::new(end).weak());
                }
            });
            if let Some(p) = current {
                ui.label(RichText::new(hp_app::widgets::fmt_vec3(p)).monospace());
            }
        }
        ui.horizontal(|ui| {
            let has = n > 0;
            let first = leak_entity_index(n);
            if ui
                .add_enabled(has && cursor != first, egui::Button::new("⏮"))
                .on_hover_text(t!("pointfile.first"))
                .clicked()
            {
                target = Some(first);
            }
            let previous = previous_index(n, cursor);
            if ui
                .add_enabled(previous.is_some(), egui::Button::new(format!("◀ {}", t!("pointfile.previous"))))
                .on_hover_text(t!("pointfile.previous_tip"))
                .clicked()
            {
                target = previous;
            }
            let next = next_index(cursor).filter(|_| has);
            if ui
                .add_enabled(next.is_some(), egui::Button::new(format!("{} ▶", t!("pointfile.next"))))
                .on_hover_text(t!("pointfile.next_tip"))
                .clicked()
            {
                target = next;
            }
            if ui.add_enabled(has, egui::Button::new("🎥")).on_hover_text(t!("pointfile.view_current")).clicked() {
                target = Some(cursor);
            }
        });
        ui.horizontal(|ui| {
            if ui.button(t!("pointfile.unload")).on_hover_text(t!("action.unload_pointfile")).clicked() {
                unload = true;
            }
        });
    });
    let _ = response;
    if let Some(i) = target {
        move_to_point(state, i);
    }
    if unload {
        state.request(Action::UnloadPointfile);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_app::state::Pointfile;

    fn leak() -> Vec<DVec3> {
        vec![DVec3::ZERO, DVec3::new(100.0, 0.0, 0.0), DVec3::new(100.0, 100.0, 0.0)]
    }

    #[test]
    fn point_one_is_the_leaking_entity() {
        assert_eq!(leak_entity_index(3), 2);
        assert_eq!(point_number(3, 2), 1);
        assert_eq!(point_number(3, 1), 2);
        assert_eq!(point_number(3, 0), 3);
        assert_eq!(point_number(3, 9), 1, "a stale cursor is clamped");
        assert_eq!(point_number(1, 0), 1);
        assert_eq!(point_number(0, 0), 0);
        assert_eq!(leak_entity_index(0), 0);
        assert_eq!(next_index(2), Some(1));
        assert_eq!(next_index(0), None);
        assert_eq!(previous_index(3, 0), Some(1));
        assert_eq!(previous_index(3, 2), None);
        assert_eq!(previous_index(0, 0), None);
    }

    #[test]
    fn end_labels_match_the_viewport() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert_eq!(end_label(3, 2), Some("Leak start: entity"));
        assert_eq!(end_label(3, 1), None);
        assert_eq!(end_label(3, 0), Some("Leak end: void"));
        assert_eq!(end_label(1, 0), Some("Leak start: entity"), "a single point is the entity");
        assert_eq!(end_label(0, 0), None);
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        let fr = end_label(3, 2);
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert_eq!(fr, Some("Début de la fuite : entité"));
    }

    #[test]
    fn camera_along_path() {
        let pts = leak();
        assert_eq!(path_camera(&pts, 2), Some((pts[2], pts[1])));
        assert_eq!(path_camera(&pts, 1), Some((pts[1], pts[0])));
        assert_eq!(path_camera(&pts, 0), Some((DVec3::ZERO, DVec3::new(-100.0, 0.0, 0.0))));
        assert_eq!(path_camera(&pts, 3), None);
        assert_eq!(path_camera(&[DVec3::ONE], 0), Some((DVec3::ONE, DVec3::ONE + DVec3::X)));
        let dup = [DVec3::ZERO, DVec3::ONE, DVec3::ONE];
        assert_eq!(path_camera(&dup, 1), Some((DVec3::ONE, DVec3::ZERO)));
        assert_eq!(path_camera(&[DVec3::ONE, DVec3::ONE, DVec3::ZERO], 1), Some((DVec3::ONE, DVec3::ONE * 2.0)));
    }

    #[test]
    fn navigator_avoids_the_compile_window() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 960.0));
        let size = NAVIGATOR_SIZE;
        let p = free_pos(screen, size, &[]);
        assert_eq!(p, egui::pos2(80.0, 960.0 - size.y - 70.0));
        let compile = egui::Rect::from_min_max(egui::pos2(60.0, 0.0), egui::pos2(900.0, 960.0));
        let p = free_pos(screen, size, &[compile]);
        assert!(!compile.intersects(egui::Rect::from_min_size(p, size)), "{p:?}");
        assert!(screen.contains_rect(egui::Rect::from_min_size(p, size)));
        let compile = egui::Rect::from_center_size(screen.center(), egui::vec2(880.0, 660.0));
        let p = free_pos(screen, size, &[compile]);
        let nav = egui::Rect::from_min_size(p, size);
        assert!(screen.contains_rect(nav), "{nav:?}");
        let covered = if compile.intersects(nav) { compile.intersect(nav).area() } else { 0.0 };
        assert!(covered < 0.05 * nav.area(), "{nav:?} covers {covered}");
    }

    #[test]
    fn navigation_moves_camera() {
        let mut state = hp_app::testing::test_state();
        move_to_point(&mut state, 1);
        state.pointfile = Some(Pointfile { path: "x.lin".into(), points: leak(), cursor: 0 });
        move_to_point(&mut state, 5);
        assert_eq!(state.pointfile.as_ref().map(|p| p.cursor), Some(2));
        assert_eq!(state.cameras.cam3d.pos, DVec3::new(100.0, 100.0, 0.0));
        let f = state.cameras.cam3d.forward();
        assert!((f - DVec3::NEG_Y).length() < 1e-6, "looks along the leak: {f:?}");
        move_to_point(&mut state, next_index(2).expect("next"));
        assert_eq!(state.cameras.cam3d.pos, DVec3::new(100.0, 0.0, 0.0));
        let f = state.cameras.cam3d.forward();
        assert!((f - DVec3::NEG_X).length() < 1e-6, "{f:?}");
    }

    #[test]
    fn leak_start_is_the_entity() {
        let mut state = hp_app::testing::test_state();
        go_to_leak_start(&mut state);
        assert!(state.pointfile.is_none());
        state.pointfile = Some(Pointfile { path: "x.lin".into(), points: leak(), cursor: 0 });
        go_to_leak_start(&mut state);
        assert_eq!(state.pointfile.as_ref().map(|p| p.cursor), Some(2));
        assert_eq!(state.cameras.cam3d.pos, leak()[2]);
        let f = state.cameras.cam3d.forward();
        assert!((f - DVec3::NEG_Y).length() < 1e-6, "{f:?}");
    }

    #[test]
    fn loaded_pointfile_starts_at_the_entity() {
        let dir = crate::test_scratch("pointfile_start");
        let lin = dir.join("gm_leak.lin");
        std::fs::write(&lin, "0 0 0\n64 0 0\n64 128 32\n").expect("write lin");
        let mut state = hp_app::testing::test_state();
        assert!(crate::compile::load_pointfile(&mut state, &lin));
        let pf = state.pointfile.as_ref().expect("loaded");
        assert_eq!(pf.points[0], DVec3::ZERO, "the .lin order is kept");
        assert_eq!(pf.cursor, 2);
        assert_eq!(point_number(pf.points.len(), pf.cursor), 1);
        assert_eq!(pf.points[pf.cursor], DVec3::new(64.0, 128.0, 32.0));
    }

    #[test]
    fn leak_path_goes_when_its_entity_goes() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.doc = hp_editor::Document::new_empty();
        let start = leak()[2];
        let id = state.doc.create_point_entity("info_player_start", start, &[]).expect("entity");
        state.pointfile = Some(Pointfile { path: "x.lin".into(), points: leak(), cursor: 0 });
        let ctx = egui::Context::default();
        let frame = |state: &mut AppState| {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| show(ui.ctx(), state));
            out.textures_delta.clear();
        };
        frame(&mut state);
        assert!(state.pointfile.is_some(), "the leaking entity is there: the path stays");
        state.doc.delete(&[hp_editor::ObjRef::Entity(id)]).expect("delete");
        frame(&mut state);
        assert!(state.pointfile.is_none(), "the leaking entity is gone: the path is unloaded");

        state.pointfile = Some(Pointfile { path: "y.lin".into(), points: leak(), cursor: 0 });
        frame(&mut state);
        frame(&mut state);
        assert!(state.pointfile.is_some());
    }
}
