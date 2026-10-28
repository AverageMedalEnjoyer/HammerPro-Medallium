use egui::{Align2, vec2};
use glam::DVec3;
use hp_app::i18n::TrTable;
use hp_app::view::ViewInfo;
use hp_app::{AppState, t};
use hp_render::{Overlay, OverlayPoint};

use super::OverlayLayer;
use crate::overlay::thick_line_on_top;
use crate::paint;

pub const LEAK_COLOR: [f32; 4] = [1.0, 0.12, 0.08, 1.0];
pub const LEAK_START_COLOR: [f32; 4] = [1.0, 0.9, 0.1, 1.0];

pub(crate) const TR: TrTable = &[
    ("viewport.leak_start", "Leak start: entity", "Début de la fuite : entité"),
    ("viewport.leak_end", "Leak end: void", "Fin de la fuite : vide"),
];

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(PointfileLayer)]
}

pub struct PointfileLayer;

impl OverlayLayer for PointfileLayer {
    fn name(&self) -> &'static str {
        "pointfile"
    }

    fn add(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if let Some(pf) = &state.pointfile {
            add_pointfile(out, view, &pf.points);
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let Some(pf) = &state.pointfile else { return };
        for (p, key, color) in leak_labels(&pf.points) {
            if let Some(s) = view.world_to_screen(p)
                && view.rect.shrink(4.0).contains(s)
            {
                paint::text_pill(
                    painter,
                    s + vec2(9.0, -9.0),
                    Align2::LEFT_BOTTOM,
                    t!(key).to_owned(),
                    paint::overlay_color(color),
                );
            }
        }
    }
}

fn f(v: DVec3) -> [f32; 3] {
    v.as_vec3().to_array()
}

const GRADIENT_PIECES: usize = 4;

pub fn leak_gradient(t: f32) -> [f32; 4] {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let mix = |a: f32, b: f32| a + (b - a) * t;
    [
        mix(LEAK_COLOR[0], LEAK_START_COLOR[0]),
        mix(LEAK_COLOR[1], LEAK_START_COLOR[1]),
        mix(LEAK_COLOR[2], LEAK_START_COLOR[2]),
        1.0,
    ]
}

pub fn add_pointfile(out: &mut Overlay, view: &ViewInfo, points: &[DVec3]) {
    let segments = points.len().saturating_sub(1);
    let pieces = (segments * GRADIENT_PIECES).max(1) as f32;
    for (i, w) in points.windows(2).enumerate() {
        for k in 0..GRADIENT_PIECES {
            let (t0, t1) = (k as f64 / GRADIENT_PIECES as f64, (k + 1) as f64 / GRADIENT_PIECES as f64);
            let a = w[0].lerp(w[1], t0);
            let b = w[0].lerp(w[1], t1);
            let t = (i * GRADIENT_PIECES + k) as f32 / (pieces - 1.0).max(1.0);
            thick_line_on_top(out, view, a, b, leak_gradient(t));
        }
    }
    for (p, _, color) in leak_labels(points) {
        let size_px = if color == LEAK_START_COLOR { 11.0 } else { 7.0 };
        out.points.push(OverlayPoint { pos: f(p), color, size_px });
    }
}

pub fn leak_labels(points: &[DVec3]) -> Vec<(DVec3, &'static str, [f32; 4])> {
    let mut out = Vec::with_capacity(2);
    if let Some(&entity) = points.last()
        && entity.is_finite()
    {
        out.push((entity, "viewport.leak_start", LEAK_START_COLOR));
    }
    if points.len() > 1
        && let Some(&void) = points.first()
        && void.is_finite()
    {
        out.push((void, "viewport.leak_end", LEAK_COLOR));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, pos2, vec2};
    use glam::DVec2;
    use hp_app::view::{Camera2d, Camera3d, ViewKind};

    fn view(kind: ViewKind) -> ViewInfo {
        ViewInfo {
            index: 0,
            kind,
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(801.0, 601.0)),
            cam3d: Camera3d { pos: DVec3::new(0.0, 0.0, 128.0), yaw_deg: 0.0, pitch_deg: 0.0, ..Camera3d::default() },
            cam2d: Camera2d { center: DVec2::ZERO, zoom: 2.0 },
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    #[test]
    fn pointfile_overlay() {
        let mut o = Overlay::default();
        let pts = [DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0), DVec3::new(64.0, 64.0, 0.0)];
        add_pointfile(&mut o, &view(ViewKind::Top), &pts);
        assert_eq!(o.lines_on_top.len(), 2 * GRADIENT_PIECES * 5);
        assert_eq!(o.lines_on_top.first().map(|l| l.color), Some(LEAK_COLOR));
        assert_eq!(o.lines_on_top.last().map(|l| l.color), Some(leak_gradient(1.0)));
        assert_eq!(leak_gradient(1.0), LEAK_START_COLOR);
        assert_eq!(leak_gradient(f32::NAN), LEAK_COLOR);
        assert_eq!(o.points.len(), 2);
        assert_eq!(o.points[0].color, LEAK_START_COLOR);
        assert_eq!(o.points[0].pos, [64.0, 64.0, 0.0]);
        assert_eq!(o.points[1].pos, [0.0, 0.0, 0.0]);
        let labels = leak_labels(&pts);
        assert_eq!(labels[0].0, pts[2]);
        assert_eq!(labels[0].1, "viewport.leak_start");
        assert_eq!(labels[1].0, pts[0]);
        assert_eq!(labels[1].1, "viewport.leak_end");
        assert_eq!(leak_labels(&pts[..1]).len(), 1);
        assert!(leak_labels(&[DVec3::NAN]).is_empty());
        let dx = (o.lines_on_top[1].a[0] - o.lines_on_top[0].a[0]).abs();
        assert!((dx - 0.5).abs() < 1e-6);
        let mut empty = Overlay::default();
        add_pointfile(&mut empty, &view(ViewKind::Perspective), &[]);
        assert!(empty.is_empty());
        add_pointfile(&mut empty, &view(ViewKind::Perspective), &[DVec3::splat(f64::NAN)]);
        assert!(empty.is_empty());
    }

    #[test]
    fn layer_draws_the_loaded_pointfile_only() {
        let mut state = hp_app::testing::test_state();
        let mut o = Overlay::default();
        PointfileLayer.add(&state, &view(ViewKind::Top), &mut o);
        assert!(o.is_empty(), "no pointfile loaded");
        state.pointfile = Some(hp_app::state::Pointfile {
            path: "leak.lin".into(),
            points: vec![DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0)],
            cursor: 0,
        });
        PointfileLayer.add(&state, &view(ViewKind::Top), &mut o);
        assert_eq!(o.lines_on_top.len(), GRADIENT_PIECES * 5);
        assert_eq!(o.points.len(), 2);
    }
}
