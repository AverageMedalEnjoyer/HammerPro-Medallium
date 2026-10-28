use egui::{Pos2, Rect, pos2};
use hp_app::view::{QuadrantView, ViewKind};

pub const GAP: f32 = 3.0;
pub const GRAB: f32 = 3.0;
pub const SPLIT_MIN: f32 = 0.1;
pub const SPLIT_MAX: f32 = 0.9;

pub fn clamp_split(v: f32) -> f32 {
    if v.is_finite() { v.clamp(SPLIT_MIN, SPLIT_MAX) } else { 0.5 }
}

pub fn split_from_pos(min: f32, size: f32, pos: f32) -> f32 {
    if size <= 1.0 || !pos.is_finite() {
        return 0.5;
    }
    clamp_split((pos - min) / size)
}

fn split_coord(min: f32, size: f32, split: f32) -> f32 {
    min + size * clamp_split(split)
}

pub fn quadrant_rects(area: Rect, split_x: f32, split_y: f32, gap: f32) -> [Rect; 4] {
    let x = split_coord(area.left(), area.width(), split_x);
    let y = split_coord(area.top(), area.height(), split_y);
    let h = gap.max(0.0) * 0.5;
    [
        Rect::from_min_max(area.min, pos2(x - h, y - h)),
        Rect::from_min_max(pos2(x + h, area.top()), pos2(area.right(), y - h)),
        Rect::from_min_max(pos2(area.left(), y + h), pos2(x - h, area.bottom())),
        Rect::from_min_max(pos2(x + h, y + h), area.max),
    ]
}

pub fn vertical_splitter_rect(area: Rect, split_x: f32) -> Rect {
    let x = split_coord(area.left(), area.width(), split_x);
    let h = GAP * 0.5 + GRAB;
    Rect::from_min_max(pos2(x - h, area.top()), pos2(x + h, area.bottom()))
}

pub fn horizontal_splitter_rect(area: Rect, split_y: f32) -> Rect {
    let y = split_coord(area.top(), area.height(), split_y);
    let h = GAP * 0.5 + GRAB;
    Rect::from_min_max(pos2(area.left(), y - h), pos2(area.right(), y + h))
}

pub fn cross_rect(area: Rect, split_x: f32, split_y: f32) -> Rect {
    let c = Pos2::new(split_coord(area.left(), area.width(), split_x), split_coord(area.top(), area.height(), split_y));
    Rect::from_center_size(c, egui::Vec2::splat((GAP * 0.5 + GRAB) * 2.0 + 4.0))
}

pub fn quadrant_kind(quadrants: &[u8; 4], index: usize) -> ViewKind {
    quadrants.get(index).map_or(ViewKind::Perspective, |&q| ViewKind::from_index(q))
}

pub fn quadrant_view(quadrants: &[u8; 4], index: usize) -> QuadrantView {
    quadrants.get(index).map_or(QuadrantView::View(ViewKind::Perspective), |&q| QuadrantView::from_index(q))
}

pub fn kind_index(kind: ViewKind) -> u8 {
    match kind {
        ViewKind::Top => 1,
        ViewKind::Front => 2,
        ViewKind::Side => 3,
        _ => 0,
    }
}

pub fn visible_views(area: Rect, split_x: f32, split_y: f32, maximized: Option<usize>) -> ([Option<Rect>; 4], bool) {
    match maximized {
        Some(i) if i < 4 => {
            let mut out = [None; 4];
            out[i] = Some(area);
            (out, true)
        }
        _ => {
            let r = quadrant_rects(area, split_x, split_y, GAP);
            ([Some(r[0]), Some(r[1]), Some(r[2]), Some(r[3])], false)
        }
    }
}

pub fn pixel_align(rect: Rect, ppp: f32, odd: bool) -> Rect {
    let ppp = if ppp.is_finite() && ppp > 0.0 { ppp } else { 1.0 };
    let fix = |v: f32| if v.is_finite() { v } else { 0.0 };
    let min_x = (fix(rect.min.x) * ppp).round();
    let min_y = (fix(rect.min.y) * ppp).round();
    let mut max_x = (fix(rect.max.x) * ppp).round().max(min_x + 1.0);
    let mut max_y = (fix(rect.max.y) * ppp).round().max(min_y + 1.0);
    if odd {
        if (max_x - min_x) as i64 % 2 == 0 {
            max_x -= 1.0;
        }
        if (max_y - min_y) as i64 % 2 == 0 {
            max_y -= 1.0;
        }
    }
    Rect::from_min_max(pos2(min_x / ppp, min_y / ppp), pos2(max_x / ppp, max_y / ppp))
}

pub fn pixel_size(rect: Rect, ppp: f32) -> [u32; 2] {
    let ppp = if ppp.is_finite() && ppp > 0.0 { ppp } else { 1.0 };
    let w = (rect.width() * ppp).round();
    let h = (rect.height() * ppp).round();
    let clamp = |v: f32| if v.is_finite() { v.clamp(1.0, 16384.0) as u32 } else { 1 };
    [clamp(w), clamp(h)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), egui::vec2(1000.0, 600.0))
    }

    #[test]
    fn quadrants_tile_the_area() {
        let r = quadrant_rects(area(), 0.5, 0.5, 4.0);
        assert_eq!(r[0].min, area().min);
        assert_eq!(r[3].max, area().max);
        assert!((r[0].right() - 598.0).abs() < 1e-4);
        assert!((r[1].left() - 602.0).abs() < 1e-4);
        assert!((r[0].bottom() - 348.0).abs() < 1e-4);
        assert!((r[2].top() - 352.0).abs() < 1e-4);
        assert_eq!(r[1].top(), area().top());
        assert_eq!(r[2].left(), area().left());
        for i in 0..4 {
            for j in (i + 1)..4 {
                assert!(!r[i].intersects(r[j]) || r[i].intersect(r[j]).area() <= 0.0, "{i} {j}");
            }
        }
    }

    #[test]
    fn split_is_clamped() {
        assert_eq!(clamp_split(0.0), SPLIT_MIN);
        assert_eq!(clamp_split(1.0), SPLIT_MAX);
        assert_eq!(clamp_split(f32::NAN), 0.5);
        assert_eq!(split_from_pos(100.0, 1000.0, 350.0), 0.25);
        assert_eq!(split_from_pos(100.0, 1000.0, -5000.0), SPLIT_MIN);
        assert_eq!(split_from_pos(100.0, 1000.0, 5000.0), SPLIT_MAX);
        assert_eq!(split_from_pos(100.0, 0.0, 10.0), 0.5);
        let r = quadrant_rects(area(), -3.0, 7.0, 4.0);
        assert!((r[0].right() + 2.0 - (100.0 + 1000.0 * SPLIT_MIN)).abs() < 1e-3);
        assert!((r[0].bottom() + 2.0 - (50.0 + 600.0 * SPLIT_MAX)).abs() < 1e-3);
    }

    #[test]
    fn splitter_rects_contain_the_gap() {
        let a = area();
        let q = quadrant_rects(a, 0.3, 0.7, GAP);
        let v = vertical_splitter_rect(a, 0.3);
        assert!(v.left() <= q[0].right() && v.right() >= q[1].left());
        let h = horizontal_splitter_rect(a, 0.7);
        assert!(h.top() <= q[0].bottom() && h.bottom() >= q[2].top());
        let c = cross_rect(a, 0.3, 0.7);
        assert!(c.contains(pos2(100.0 + 300.0, 50.0 + 420.0)));
    }

    #[test]
    fn quadrant_switching() {
        let mut q = [0u8, 1, 2, 3];
        assert_eq!(quadrant_kind(&q, 0), ViewKind::Perspective);
        assert_eq!(quadrant_kind(&q, 3), ViewKind::Side);
        q[3] = kind_index(ViewKind::Top);
        assert_eq!(quadrant_kind(&q, 3), ViewKind::Top);
        for kind in ViewKind::ALL {
            assert_eq!(ViewKind::from_index(kind_index(kind)), kind);
        }
        q[1] = 200;
        assert_eq!(quadrant_kind(&q, 1), ViewKind::Perspective);
        assert_eq!(quadrant_kind(&q, 9), ViewKind::Perspective);
        assert_eq!(quadrant_view(&q, 9), QuadrantView::View(ViewKind::Perspective));
        q[2] = QuadrantView::Logical.index();
        assert_eq!(quadrant_view(&q, 2), QuadrantView::Logical);
        assert_eq!(quadrant_view(&q, 3), QuadrantView::View(ViewKind::Top));
        for view in QuadrantView::ALL {
            q[0] = view.index();
            assert_eq!(quadrant_view(&q, 0), view);
        }
    }

    #[test]
    fn maximized_layout() {
        let (views, maxed) = visible_views(area(), 0.5, 0.5, Some(2));
        assert!(maxed);
        assert_eq!(views.iter().filter(|v| v.is_some()).count(), 1);
        assert_eq!(views[2], Some(area()));
        let (views, maxed) = visible_views(area(), 0.5, 0.5, Some(7));
        assert!(!maxed);
        assert!(views.iter().all(Option::is_some));
    }

    #[test]
    fn pixel_alignment() {
        let r = Rect::from_min_max(pos2(10.3, 20.7), pos2(110.6, 70.2));
        for ppp in [1.0f32, 1.25, 1.5, 2.0] {
            let a = pixel_align(r, ppp, false);
            for v in [a.min.x, a.min.y, a.max.x, a.max.y] {
                assert!(((v * ppp) - (v * ppp).round()).abs() < 1e-3, "{v} at {ppp}");
            }
            let o = pixel_align(r, ppp, true);
            let s = pixel_size(o, ppp);
            assert_eq!(s[0] % 2, 1, "odd width at {ppp}: {s:?}");
            assert_eq!(s[1] % 2, 1, "odd height at {ppp}: {s:?}");
            assert!(o.width() <= a.width() + 1e-3);
            assert!((a.width() - o.width()) * ppp <= 1.0 + 1e-3);
        }
        let tiny = pixel_align(Rect::from_min_max(pos2(5.0, 5.0), pos2(5.0, 5.0)), 1.0, true);
        assert_eq!(pixel_size(tiny, 1.0), [1, 1]);
        let nan = pixel_align(Rect::from_min_max(pos2(f32::NAN, 0.0), pos2(4.0, 4.0)), f32::NAN, false);
        assert!(nan.min.x.is_finite());
    }
}
