use egui::{Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, StrokeKind};
use glam::DVec2;
use hp_app::ViewKind;
use hp_geom::{Aabb, SolidGeom};

use crate::common::{COLOR_PENDING, COLOR_SHAPE};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Projection {
    pub hulls: Vec<Vec<DVec2>>,
    pub edges: Vec<[DVec2; 2]>,
    pub footprint: (DVec2, DVec2),
    pub axes: (usize, usize),
}

const MAX_SOLIDS: usize = 1024;
const MAX_EDGES: usize = 8192;

pub(crate) fn view_along(up: usize) -> ViewKind {
    match up {
        0 => ViewKind::Front,
        1 => ViewKind::Side,
        _ => ViewKind::Top,
    }
}

pub(crate) fn project(solids: &[SolidGeom], bounds: &Aabb, up: usize) -> Projection {
    let kind = view_along(up);
    let (r, u, _) = kind.axes_2d().unwrap_or((0, 1, 2));
    let mut out = Projection {
        footprint: (kind.to_2d(bounds.min), kind.to_2d(bounds.max)),
        axes: (r, u),
        ..Projection::default()
    };
    if solids.len() > MAX_SOLIDS {
        let points: Vec<DVec2> = solids.iter().flat_map(|s| s.vertices()).map(|v| kind.to_2d(v)).collect();
        out.hulls.push(convex_hull(points));
        out.edges = outline_edges(&out.hulls);
        return out;
    }
    for solid in solids {
        let points: Vec<DVec2> = solid.vertices().map(|v| kind.to_2d(v)).collect();
        out.hulls.push(convex_hull(points));
        for [a, b] in solid.edges() {
            let (a, b) = (kind.to_2d(a), kind.to_2d(b));
            if (a - b).length_squared() > 1e-4 {
                out.edges.push([a, b]);
            }
        }
        if out.edges.len() > MAX_EDGES {
            break;
        }
    }
    if out.edges.len() > MAX_EDGES {
        for solid in &solids[out.hulls.len()..] {
            out.hulls.push(convex_hull(solid.vertices().map(|v| kind.to_2d(v)).collect()));
        }
        out.edges = outline_edges(&out.hulls);
    }
    out
}

fn outline_edges(hulls: &[Vec<DVec2>]) -> Vec<[DVec2; 2]> {
    hulls
        .iter()
        .filter(|h| h.len() >= 2)
        .flat_map(|h| (0..h.len()).map(move |i| [h[i], h[(i + 1) % h.len()]]))
        .collect()
}

pub(crate) fn convex_hull(mut pts: Vec<DVec2>) -> Vec<DVec2> {
    pts.retain(|p| p.is_finite());
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup_by(|a, b| (*a - *b).length_squared() < 1e-8);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o);
    let mut hull: Vec<DVec2> = Vec::with_capacity(pts.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &DVec2>> =
            if pass == 0 { Box::new(pts.iter()) } else { Box::new(pts.iter().rev()) };
        for &p in iter {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 1e-9 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PreviewStyle {
    pub background: Color32,
    pub detailed: bool,
    pub rising: bool,
    pub mark_first: bool,
}

pub(crate) fn paint(painter: &egui::Painter, rect: Rect, proj: &Projection, error: Option<&str>, style: PreviewStyle) {
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let radius = if style.detailed { hp_app::theme::RADIUS } else { 2 };
    painter.rect_filled(rect, radius, style.background);
    let (lo, hi) = proj.footprint;
    let size = (hi - lo).max(DVec2::splat(1.0));
    let pad = if style.detailed { 22.0 } else { 3.0 };
    let avail = DVec2::new(f64::from(rect.width() - 2.0 * pad), f64::from(rect.height() - 2.0 * pad)).max(DVec2::ONE);
    let scale = (avail.x / size.x).min(avail.y / size.y);
    let mid = (lo + hi) * 0.5;
    let c = rect.center();
    let to_screen = |p: DVec2| Pos2::new(c.x + ((p.x - mid.x) * scale) as f32, c.y - ((p.y - mid.y) * scale) as f32);
    let to_world = |s: Pos2| DVec2::new(mid.x + f64::from(s.x - c.x) / scale, mid.y - f64::from(s.y - c.y) / scale);

    if style.detailed {
        paint_grid(&painter, rect, size.max_element(), style.background, to_screen, to_world);
    }
    let frame = Rect::from_two_pos(to_screen(lo), to_screen(hi));
    painter.rect_stroke(frame, 0, Stroke::new(1.0, COLOR_PENDING.gamma_multiply(0.55)), StrokeKind::Middle);

    let n = proj.hulls.len();
    for (i, hull) in proj.hulls.iter().enumerate() {
        if hull.len() < 3 {
            continue;
        }
        let alpha = if style.rising && n > 1 { 0.08 + 0.34 * i as f32 / (n - 1) as f32 } else { 0.16 };
        let fill = if style.mark_first && i == 0 && n > 1 {
            hp_app::theme::VIEW_ACCENT.gamma_multiply(0.45)
        } else {
            COLOR_SHAPE.gamma_multiply(alpha)
        };
        let pts: Vec<Pos2> = hull.iter().map(|p| to_screen(*p)).collect();
        painter.add(Shape::convex_polygon(pts, fill, Stroke::NONE));
    }
    let width = if style.detailed { 1.3 } else { 1.0 };
    let stroke = Stroke::new(width, COLOR_SHAPE.gamma_multiply(0.95));
    painter.extend(proj.edges.iter().map(|e| Shape::line_segment([to_screen(e[0]), to_screen(e[1])], stroke)));

    if style.detailed {
        let center = to_screen(mid);
        let mark = Stroke::new(1.0, COLOR_PENDING.gamma_multiply(0.7));
        painter.line_segment([center - egui::vec2(5.0, 0.0), center + egui::vec2(5.0, 0.0)], mark);
        painter.line_segment([center - egui::vec2(0.0, 5.0), center + egui::vec2(0.0, 5.0)], mark);
        let names = ["X", "Y", "Z"];
        let colors = [hp_app::theme::AXIS_X, hp_app::theme::AXIS_Y, hp_app::theme::AXIS_Z];
        let font = FontId::proportional(11.0);
        let (r, u) = proj.axes;
        let at = rect.right_bottom() - egui::vec2(6.0, 4.0);
        painter.text(at, Align2::RIGHT_BOTTOM, format!("+{}", names[r]), font.clone(), colors[r]);
        let at = rect.left_top() + egui::vec2(6.0, 4.0);
        painter.text(at, Align2::LEFT_TOP, format!("+{}", names[u]), font.clone(), colors[u]);
        if let Some(e) = error {
            painter.text(rect.center(), Align2::CENTER_CENTER, e, font, hp_app::theme::VIEW_ERROR);
        }
    } else if error.is_some() {
        let s = Stroke::new(1.5, hp_app::theme::VIEW_ERROR);
        let r = rect.shrink(6.0);
        painter.line_segment([r.left_top(), r.right_bottom()], s);
        painter.line_segment([r.right_top(), r.left_bottom()], s);
    }
}

fn paint_grid(
    painter: &egui::Painter,
    rect: Rect,
    extent: f64,
    background: Color32,
    to_screen: impl Fn(DVec2) -> Pos2,
    to_world: impl Fn(Pos2) -> DVec2,
) {
    let step = 2f64.powf((extent / 16.0).max(1.0).log2().ceil());
    let a = to_world(rect.left_bottom());
    let b = to_world(rect.right_top());
    let line = |bright: bool| {
        let c = if bright { 46 } else { 24 };
        Color32::from_rgb(
            background.r().saturating_add(c),
            background.g().saturating_add(c),
            background.b().saturating_add(c),
        )
    };
    let lines = |from: f64, to: f64| {
        let first = (from / step).ceil() as i64;
        let last = (to / step).floor() as i64;
        (first..=last).take(256).map(move |k| (k as f64 * step, k % 8 == 0))
    };
    for (x, bright) in lines(a.x, b.x) {
        let s = to_screen(DVec2::new(x, 0.0)).x;
        painter.line_segment([Pos2::new(s, rect.top()), Pos2::new(s, rect.bottom())], Stroke::new(1.0, line(bright)));
    }
    for (y, bright) in lines(a.y, b.y) {
        let s = to_screen(DVec2::new(0.0, y)).y;
        painter.line_segment([Pos2::new(rect.left(), s), Pos2::new(rect.right(), s)], Stroke::new(1.0, line(bright)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;
    use hp_geom::{Primitive, PrimitiveParams, build_solid, make_primitive};

    fn solids(params: &PrimitiveParams) -> Vec<SolidGeom> {
        let mut next = 0u32;
        let sides = make_primitive(params, &mut || {
            next += 1;
            next
        })
        .expect("primitive");
        sides.iter().map(|s| build_solid(s)).collect()
    }

    #[test]
    fn hull_of_a_square_with_inner_points() {
        let pts = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(1.0, 1.0),
            DVec2::new(2.0, 2.0),
            DVec2::new(0.0, 2.0),
            DVec2::new(0.0, 2.0),
            DVec2::new(1.0, 0.0),
        ];
        let hull = convex_hull(pts);
        assert_eq!(hull.len(), 4, "{hull:?}");
        let area: f64 = (0..hull.len()).map(|i| hull[i].perp_dot(hull[(i + 1) % hull.len()])).sum::<f64>() * 0.5;
        assert!((area - 4.0).abs() < 1e-9, "counter clockwise, area {area}");
    }

    #[test]
    fn arch_seen_from_above_shows_every_segment_with_its_wall() {
        let bounds = Aabb::new(DVec3::new(-128.0, -128.0, 0.0), DVec3::new(128.0, 128.0, 32.0));
        let params = PrimitiveParams {
            kind: Primitive::Arch,
            bounds,
            sides: 8,
            arch_wall_width: 32.0,
            arch_arc_degrees: 180.0,
            arch_start_angle: 0.0,
            arch_add_height: 16.0,
            ..PrimitiveParams::default()
        };
        let proj = project(&solids(&params), &bounds, 2);
        assert_eq!(proj.axes, (0, 1), "top view frame");
        assert_eq!(proj.footprint, (DVec2::new(-128.0, -128.0), DVec2::new(128.0, 128.0)));
        assert_eq!(proj.hulls.len(), 8, "one outline per segment");
        for hull in &proj.hulls {
            let r: Vec<f64> = hull.iter().map(|p| p.length()).collect();
            let (lo, hi) = r.iter().fold((f64::MAX, 0f64), |(a, b), &v| (a.min(v), b.max(v)));
            assert!((hi - 128.0).abs() < 1.5, "outer radius {hi}");
            assert!((lo - 96.0).abs() < 1.5, "inner radius {lo}");
        }
        let all: Vec<DVec2> = proj.hulls.iter().flatten().copied().collect();
        let left = all.iter().filter(|p| p.x < -1.0).count();
        let right = all.iter().filter(|p| p.x > 1.0).count();
        assert!(left == 0 || right == 0, "left {left} right {right}");
        assert!(!proj.edges.is_empty());
    }

    #[test]
    fn reoriented_primitives_use_the_frame_of_their_view() {
        let bounds = Aabb::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(64.0, 128.0, 256.0));
        let params = PrimitiveParams { kind: Primitive::Cylinder, bounds, up_axis: 0, ..PrimitiveParams::default() };
        let proj = project(&solids(&params), &bounds, 0);
        assert_eq!(proj.axes, (1, 2));
        assert_eq!(proj.footprint, (DVec2::new(0.0, 0.0), DVec2::new(128.0, 256.0)));
        assert_eq!(proj.hulls.len(), 1);
        assert_eq!(proj.hulls[0].len(), 8, "an octagon");
    }
}
