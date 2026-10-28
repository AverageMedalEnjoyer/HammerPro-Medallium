use egui::{Painter, Pos2, Rect, Shape, Stroke};

pub const CLIP_MARGIN: f32 = 8.0;

const COORD_LIMIT: f64 = 1e30;

pub fn view_clip(view_rect: Rect) -> Rect {
    view_rect.expand(CLIP_MARGIN)
}

fn coords(p: Pos2) -> Option<(f64, f64)> {
    let (x, y) = (f64::from(p.x), f64::from(p.y));
    if x.is_nan() || y.is_nan() {
        return None;
    }
    Some((x.clamp(-COORD_LIMIT, COORD_LIMIT), y.clamp(-COORD_LIMIT, COORD_LIMIT)))
}

fn usable(clip: Rect) -> bool {
    clip.min.x <= clip.max.x && clip.min.y <= clip.max.y
}

pub fn clip_segment(a: Pos2, b: Pos2, clip: Rect) -> Option<[Pos2; 2]> {
    if !usable(clip) {
        return None;
    }
    let (x0, y0) = coords(a)?;
    let (x1, y1) = coords(b)?;
    let (dx, dy) = (x1 - x0, y1 - y0);
    let (xmin, xmax) = (f64::from(clip.min.x), f64::from(clip.max.x));
    let (ymin, ymax) = (f64::from(clip.min.y), f64::from(clip.max.y));
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    let (mut side0, mut side1) = (None, None);
    for (side, (p, q)) in [(-dx, x0 - xmin), (dx, xmax - x0), (-dy, y0 - ymin), (dy, ymax - y0)].into_iter().enumerate()
    {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let r = q / p;
        if p < 0.0 {
            if r > t1 {
                return None;
            }
            if r > t0 {
                (t0, side0) = (r, Some(side));
            }
        } else {
            if r < t0 {
                return None;
            }
            if r < t1 {
                (t1, side1) = (r, Some(side));
            }
        }
    }
    if t0 > t1 {
        return None;
    }
    let at = |t: f64, side: Option<usize>, end: (f64, f64)| {
        let (x, y) = match side {
            None => end,
            Some(s @ (0 | 1)) => {
                let x = if s == 0 { xmin } else { xmax };
                (x, y0 + (x - x0) * (dy / dx))
            }
            Some(s) => {
                let y = if s == 2 { ymin } else { ymax };
                (x0 + (y - y0) * (dx / dy), y)
            }
        };
        let fallback = (x0 + dx * t, y0 + dy * t);
        let (x, y) = if x.is_finite() && y.is_finite() { (x, y) } else { fallback };
        Pos2::new(x.clamp(xmin, xmax) as f32, y.clamp(ymin, ymax) as f32)
    };
    Some([at(t0, side0, (x0, y0)), at(t1, side1, (x1, y1))])
}

pub fn clipped_segment(painter: &Painter, clip: Rect, a: Pos2, b: Pos2, stroke: Stroke) {
    if let Some(seg) = clip_segment(a, b, clip) {
        painter.line_segment(seg, stroke);
    }
}

pub fn dashed_segment(out: &mut Vec<Shape>, from: Pos2, to: Pos2, clip: Rect, stroke: Stroke, dash: f32, gap: f32) {
    let Some([c0, c1]) = clip_segment(from, to, clip) else { return };
    let (Some((fx, fy)), Some((tx, ty))) = (coords(from), coords(to)) else { return };
    let (dx, dy) = (tx - fx, ty - fy);
    let len = dx.hypot(dy);
    if !len.is_finite() || len <= 0.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let period = f64::from(dash.max(0.5) + gap.max(0.0));
    let skipped = (f64::from(c0.x) - fx) * ux + (f64::from(c0.y) - fy) * uy;
    let phase = skipped.rem_euclid(period);
    let phase = if phase.is_finite() { phase } else { 0.0 };
    let start = Pos2::new((f64::from(c0.x) - ux * phase) as f32, (f64::from(c0.y) - uy * phase) as f32);
    Shape::dashed_line_many(&[start, c1], stroke, dash.max(0.5), gap.max(0.0), out);
}

pub fn dashed_rect(out: &mut Vec<Shape>, rect: Rect, clip: Rect, stroke: Stroke, dash: f32, gap: f32) {
    if rect.any_nan() || !usable(rect) {
        return;
    }
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    for i in 0..4 {
        dashed_segment(out, corners[i], corners[(i + 1) % 4], clip, stroke, dash, gap);
    }
}

pub fn paint_dashed_rect(painter: &Painter, clip: Rect, rect: Rect, stroke: Stroke, dash: f32, gap: f32) {
    let mut shapes = Vec::new();
    dashed_rect(&mut shapes, rect, clip, stroke, dash, gap);
    painter.extend(shapes);
}

pub fn paint_rect_outline(painter: &Painter, clip: Rect, rect: Rect, stroke: Stroke) {
    if rect.any_nan() || !usable(rect) {
        return;
    }
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    for i in 0..4 {
        clipped_segment(painter, clip, corners[i], corners[(i + 1) % 4], stroke);
    }
}

pub fn visible_part(rect: Rect, clip: Rect) -> Option<Rect> {
    if rect.any_nan() || !usable(rect) || !usable(clip) {
        return None;
    }
    let v = rect.intersect(clip);
    usable(v).then_some(v)
}

#[cfg(test)]
mod tests {
    use egui::{Color32, vec2};

    use super::*;

    const CLIP: Rect = Rect { min: Pos2::new(0.0, 0.0), max: Pos2::new(800.0, 600.0) };

    fn stroke() -> Stroke {
        Stroke::new(1.0, Color32::WHITE)
    }

    fn inside(p: Pos2) -> bool {
        CLIP.expand(0.01).contains(p)
    }

    #[test]
    fn segments_are_clipped() {
        let [a, b] = clip_segment(Pos2::new(-100.0, 300.0), Pos2::new(900.0, 300.0), CLIP).expect("crosses the view");
        assert_eq!((a, b), (Pos2::new(0.0, 300.0), Pos2::new(800.0, 300.0)));
        let seg = clip_segment(Pos2::new(10.0, 20.0), Pos2::new(30.0, 40.0), CLIP);
        assert_eq!(seg, Some([Pos2::new(10.0, 20.0), Pos2::new(30.0, 40.0)]));
        let [a, b] = clip_segment(Pos2::new(-100.0, -100.0), Pos2::new(1000.0, 1000.0), CLIP).expect("diagonal");
        assert!(inside(a) && inside(b));
        assert!((a.x - a.y).abs() < 1e-3 && (b.x - b.y).abs() < 1e-3);
        assert_eq!(clip_segment(Pos2::new(-10.0, -5.0), Pos2::new(900.0, -5.0), CLIP), None);
        assert_eq!(clip_segment(Pos2::new(900.0, 0.0), Pos2::new(1000.0, 600.0), CLIP), None);
        let [a, b] = clip_segment(Pos2::new(-1e31, 5.0), Pos2::new(1e31, 5.0), CLIP).expect("huge");
        assert_eq!((a.y, b.y), (5.0, 5.0));
        assert_eq!((a.x, b.x), (0.0, 800.0));
        let [a, b] =
            clip_segment(Pos2::new(f32::NEG_INFINITY, 5.0), Pos2::new(f32::INFINITY, 5.0), CLIP).expect("infinite");
        assert!(inside(a) && inside(b));
        assert_eq!(clip_segment(Pos2::new(f32::NAN, 5.0), Pos2::new(10.0, 5.0), CLIP), None);
        assert_eq!(clip_segment(Pos2::ZERO, Pos2::new(10.0, 5.0), Rect::NOTHING), None);
    }

    #[test]
    fn dash_count_does_not_depend_on_size() {
        let mut small = Vec::new();
        dashed_rect(
            &mut small,
            Rect::from_min_size(Pos2::new(100.0, 100.0), vec2(80.0, 80.0)),
            CLIP,
            stroke(),
            5.0,
            3.0,
        );
        assert!(!small.is_empty());
        assert!(small.len() <= 4 * (80 / 8 + 2), "{}", small.len());
        let max_dashes = 4 * ((800 + 600) / 8 + 4);
        for size in [1e4f32, 1e7, 1e10, 1e20, 1e31, f32::INFINITY] {
            let rect = Rect::from_min_max(Pos2::new(-size, 50.0), Pos2::new(size, 500.0));
            let mut shapes = Vec::new();
            dashed_rect(&mut shapes, rect, CLIP, stroke(), 5.0, 3.0);
            assert!(!shapes.is_empty(), "the visible sides are drawn ({size})");
            assert!(shapes.len() <= max_dashes, "{size}: {} dashes", shapes.len());
            for s in &shapes {
                let r = s.visual_bounding_rect();
                assert!(r.is_finite() && CLIP.expand(9.0).contains_rect(r), "{size}: dash {r:?} leaves the view");
            }
        }
        let mut shapes = Vec::new();
        dashed_rect(
            &mut shapes,
            Rect::from_min_max(egui::pos2(-1e9, -1e9), egui::pos2(1e9, 1e9)),
            CLIP,
            stroke(),
            5.0,
            3.0,
        );
        assert!(shapes.is_empty());
        dashed_rect(
            &mut shapes,
            Rect::from_min_max(Pos2::new(f32::NAN, 0.0), egui::pos2(10.0, 10.0)),
            CLIP,
            stroke(),
            5.0,
            3.0,
        );
        assert!(shapes.is_empty());
    }

    #[test]
    fn dash_pattern_is_anchored_at_the_corner() {
        let rect = Rect::from_min_max(Pos2::new(-37.0, 100.0), Pos2::new(400.0, 300.0));
        let mut clipped = Vec::new();
        dashed_segment(&mut clipped, rect.left_top(), rect.right_top(), CLIP, stroke(), 5.0, 3.0);
        let mut whole = Vec::new();
        dashed_segment(&mut whole, rect.left_top(), rect.right_top(), CLIP.expand(100.0), stroke(), 5.0, 3.0);
        let starts = |v: &[Shape]| -> Vec<i32> {
            v.iter()
                .filter_map(|s| match s {
                    Shape::LineSegment { points, .. } => Some((points[0].x * 10.0).round() as i32),
                    _ => None,
                })
                .filter(|x| *x >= 10)
                .collect()
        };
        let a = starts(&clipped);
        let b = starts(&whole);
        assert!(!a.is_empty());
        assert_eq!(a, b);
    }

    #[test]
    fn visible_parts() {
        let big = Rect::from_min_max(egui::pos2(-1e30, -1e30), egui::pos2(1e30, 1e30));
        assert_eq!(visible_part(big, CLIP), Some(CLIP));
        assert_eq!(visible_part(Rect::NOTHING, CLIP), None);
        assert_eq!(visible_part(Rect::from_min_size(Pos2::new(900.0, 0.0), vec2(5.0, 5.0)), CLIP), None);
        let flat = Rect::from_min_max(Pos2::new(10.0, 50.0), Pos2::new(10.0, 80.0));
        assert_eq!(visible_part(flat, CLIP), Some(flat), "zero width rectangles stay visible");
    }
}
