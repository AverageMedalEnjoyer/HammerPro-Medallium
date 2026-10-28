use std::cell::Cell;

use glam::DVec3;
use hp_app::i18n::TrTable;
use hp_app::view::{Camera3d, ViewInfo};
use hp_app::widgets::fmt_coord;
use hp_app::{AppState, tf};
use hp_editor::ObjRef;
use hp_render::{Overlay, OverlayLine, RenderStats};

use crate::layers::OverlayLayer;

pub const TR: TrTable = &[
    ("viewport.info_position", "Position {0}", "Position {0}"),
    ("viewport.info_angles", "Angles {0}", "Angles {0}"),
    (
        "viewport.info_render",
        "{0} draw calls, {1} triangles, {2} chunks",
        "{0} appels de dessin, {1} triangles, {2} blocs",
    ),
    (
        "viewport.info_copy_tip",
        "Click to copy setpos and setang for the game console (feet 64 units under the camera)",
        "Cliquez pour copier setpos et setang pour la console du jeu (pieds 64 unités sous la caméra)",
    ),
    ("viewport.info_copied", "Copied: {0}", "Copié : {0}"),
    ("viewport.bounds_capped", "Bounding boxes: the first {0} objects", "Boîtes englobantes : les {0} premiers objets"),
];

pub const EYE_HEIGHT: f64 = 64.0;

pub fn source_angles(cam: &Camera3d) -> DVec3 {
    let pitch = if cam.pitch_deg.is_finite() { -cam.pitch_deg } else { 0.0 };
    let yaw = if cam.yaw_deg.is_finite() { cam.yaw_deg } else { 0.0 };
    DVec3::new(pitch + 0.0, yaw, 0.0)
}

pub fn setpos_command(cam: &Camera3d) -> String {
    let n = |v: f64| {
        let v = if v.is_finite() { (v * 100.0).round() / 100.0 } else { 0.0 };
        let v = if v == 0.0 { 0.0 } else { v };
        let s = format!("{v:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    };
    let p = cam.pos;
    let a = source_angles(cam);
    format!("setpos {} {} {};setang {} {} {}", n(p.x), n(p.y), n(p.z - EYE_HEIGHT), n(a.x), n(a.y), n(a.z))
}

pub fn compact_count(n: u64) -> String {
    match n {
        0..=9_999 => n.to_string(),
        10_000..=999_999 => format!("{:.1}K", n as f64 / 1000.0),
        _ => format!("{:.1}M", n as f64 / 1_000_000.0),
    }
}

pub fn info_lines(fps: f32, cam: &Camera3d, stats: Option<&RenderStats>) -> Vec<String> {
    let a = source_angles(cam);
    let mut out = vec![
        tf!("status.fps", format!("{:.0}", if fps.is_finite() { fps } else { 0.0 })),
        tf!("viewport.info_position", hp_app::widgets::fmt_vec3(cam.pos.round())),
        tf!("viewport.info_angles", format!("{} {} {}", fmt_coord(a.x.round()), fmt_coord(a.y.round()), 0)),
    ];
    if let Some(s) = stats {
        out.push(tf!(
            "viewport.info_render",
            compact_count(u64::from(s.draw_calls)),
            compact_count(s.triangles),
            compact_count(u64::from(s.chunks))
        ));
    }
    out
}

pub const MAX_BOUNDS: usize = 20_000;
const REBUILD_INTERVAL: f64 = 0.4;
const BOUNDS_COLOR: [f32; 4] = [0.95, 0.85, 0.25, 0.85];

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(BoundsLayer::default())]
}

#[derive(Debug, Default)]
pub struct BoundsLayer {
    built: Option<(u64, u64)>,
    built_at: f64,
    version: u64,
    shown: bool,
    capped: Cell<usize>,
}

impl OverlayLayer for BoundsLayer {
    fn name(&self) -> &'static str {
        "workflow_bounds"
    }

    fn prepare(&mut self, state: &AppState) {
        self.shown = state.settings.workflow.show_bounds;
        if !self.shown {
            self.built = None;
            return;
        }
        let key = (state.doc.revision(), state.assets.generation());
        let now = state.frame.time;
        let due = self.built.is_none() || (now - self.built_at).abs() >= REBUILD_INTERVAL;
        if self.built != Some(key) && due {
            self.built = Some(key);
            self.built_at = now;
            self.version = self.version.wrapping_add(1);
        }
    }

    fn retained_version(&self, _state: &AppState, view: &ViewInfo) -> Option<u64> {
        (self.shown && !view.is_2d()).then_some(self.version)
    }

    fn retained(&self, state: &AppState, _view: &ViewInfo, out: &mut Overlay) {
        self.capped.set(add_bounds(state, out, MAX_BOUNDS));
    }

    fn paint(&self, _state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let capped = self.capped.get();
        if self.shown && !view.is_2d() && capped > 0 {
            crate::paint::text_pill(
                painter,
                view.rect.left_bottom() + egui::vec2(60.0, -8.0),
                egui::Align2::LEFT_BOTTOM,
                tf!("viewport.bounds_capped", MAX_BOUNDS),
                crate::paint::TEXT_WEAK,
            );
        }
    }
}

pub fn add_bounds(state: &AppState, out: &mut Overlay, max: usize) -> usize {
    let doc = &state.doc;
    let scene = doc.scene();
    let entity_bounds = |e: &hp_vmf::Entity| state.entity_bounds(e);
    let mut drawn = 0usize;
    let mut skipped = 0usize;
    let entities = scene.entities().keys().map(|id| ObjRef::Entity(*id));
    let solids =
        scene.solids().iter().filter(|(_, n)| n.owner == hp_editor::Owner::World).map(|(id, _)| ObjRef::Solid(*id));
    for obj in entities.chain(solids) {
        if !doc.is_visible(obj) {
            continue;
        }
        if drawn >= max {
            skipped += 1;
            continue;
        }
        let Some(b) = doc.object_bounds(obj, &entity_bounds) else { continue };
        if b.is_empty() || !b.min.is_finite() || !b.max.is_finite() {
            continue;
        }
        box_lines(out, b.min, b.max, BOUNDS_COLOR);
        drawn += 1;
    }
    skipped
}

fn box_lines(out: &mut Overlay, min: DVec3, max: DVec3, color: [f32; 4]) {
    let c = |i: usize| {
        [
            if i & 1 == 0 { min.x } else { max.x } as f32,
            if i & 2 == 0 { min.y } else { max.y } as f32,
            if i & 4 == 0 { min.z } else { max.z } as f32,
        ]
    };
    for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
        out.lines.push(OverlayLine { a: c(a), b: c(b), color });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty(), "{:#?}", hp_app::i18n::lint_table(TR));
        for (key, en, fr) in TR {
            assert!(key.starts_with("viewport."));
            assert!(!crate::TR.iter().any(|(k, _, _)| k == key), "{key} is in the viewport table");
            for table in crate::layers::TABLES {
                assert!(!table.iter().any(|(k, _, _)| k == key), "{key} is in a layer table");
            }
            for i in 0..4 {
                let p = format!("{{{i}}}");
                assert_eq!(en.contains(&p), fr.contains(&p), "{key}: placeholder {p} differs");
            }
        }
    }

    #[test]
    fn setpos_matches_the_game() {
        let cam =
            Camera3d { pos: DVec3::new(100.0, -200.5, 128.0), yaw_deg: 90.0, pitch_deg: 30.0, ..Camera3d::default() };
        assert_eq!(setpos_command(&cam), "setpos 100 -200.5 64;setang -30 90 0");
        let bad = Camera3d { pos: DVec3::new(f64::NAN, 0.0, 0.0), yaw_deg: f64::INFINITY, ..Camera3d::default() };
        assert!(setpos_command(&bad).starts_with("setpos 0 0 "));
        let level = Camera3d { pitch_deg: 0.0, ..cam };
        assert!(setpos_command(&level).ends_with("setang 0 90 0"), "{}", setpos_command(&level));
    }

    #[test]
    fn counts_and_info_lines() {
        assert_eq!(compact_count(950), "950");
        assert_eq!(compact_count(12_400), "12.4K");
        assert_eq!(compact_count(3_100_000), "3.1M");
        crate::register_translations();
        let cam = Camera3d::default();
        assert_eq!(info_lines(60.0, &cam, None).len(), 3);
        let stats = RenderStats { draw_calls: 12, triangles: 34_000, chunks: 5, ..RenderStats::default() };
        let lines = info_lines(f32::NAN, &cam, Some(&stats));
        assert_eq!(lines.len(), 4);
        assert!(lines[3].contains("34.0K"), "{lines:?}");
    }

    #[test]
    fn bounds_cover_visible_objects_and_respect_the_cap() {
        let mut state = hp_app::testing::test_state();
        let mut out = Overlay::default();
        assert_eq!(add_bounds(&state, &mut out, MAX_BOUNDS), 0);
        let objects = out.lines.len() / 12;
        assert!(objects > 0, "the test map has objects");
        out.clear();
        assert_eq!(add_bounds(&state, &mut out, 1), objects - 1);
        assert_eq!(out.lines.len(), 12);
        state.doc.select_all();
        assert!(state.doc.hide_selection() > 0);
        out.clear();
        assert_eq!(add_bounds(&state, &mut out, MAX_BOUNDS), 0);
        assert!(out.lines.is_empty());
    }
}
