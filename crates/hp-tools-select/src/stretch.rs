use egui::{Align2, Color32, CursorIcon, Painter, Pos2, Shape, Stroke, Vec2};
use glam::DVec3;
use hp_app::ViewInfo;
use hp_geom::Aabb;

use crate::gizmo::axis_color;
use crate::math::{axis_vec, units_per_point_at};

pub const GAP: f32 = 16.0;
pub const HEAD: f32 = 13.0;
pub const PICK: f32 = 8.0;
const PUSH_STEP: f32 = 4.0;
const PUSH_MAX: f32 = 160.0;
pub const BACK_SCALE: f32 = 0.75;
const BACK_ALPHA: f32 = 0.5;
const PROBE: f64 = 24.0;
const MIN_FORESHORTEN: f32 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceHandle {
    pub axis: usize,
    pub positive: bool,
    pub face: Pos2,
    pub dir: Vec2,
    pub knob: Pos2,
    pub front: bool,
}

impl FaceHandle {
    pub fn id(&self) -> (usize, bool) {
        (self.axis, self.positive)
    }

    fn scale(&self) -> f32 {
        if self.front { 1.0 } else { BACK_SCALE }
    }

    pub fn distance(&self, pos: Pos2) -> f32 {
        (self.knob.distance(pos) - HEAD * 0.5 * self.scale()).max(0.0)
    }

    pub fn with_gap(&self, gap: f32) -> Self {
        Self { knob: self.face + self.dir * gap, ..*self }
    }

    fn head_points(&self) -> [Pos2; 5] {
        let r = HEAD * 0.5 * self.scale();
        let side = Vec2::new(-self.dir.y, self.dir.x);
        [self.knob, self.knob + self.dir * r, self.knob - self.dir * r, self.knob + side * r, self.knob - side * r]
    }

    pub fn cursor(&self) -> CursorIcon {
        resize_cursor(self.dir)
    }

    pub fn label_anchor(&self) -> (Pos2, Align2) {
        let p = self.knob + self.dir * (HEAD * 0.5 * self.scale() + 6.0);
        let h = if self.dir.x >= 0.0 { egui::Align::Min } else { egui::Align::Max };
        let v = if self.dir.y >= 0.0 { egui::Align::Min } else { egui::Align::Max };
        (p, Align2([h, v]))
    }
}

pub fn face_center(b: &Aabb, axis: usize, positive: bool) -> DVec3 {
    let k = axis.min(2);
    let mut p = b.center();
    p[k] = if positive { b.max[k] } else { b.min[k] };
    p
}

pub fn layout(view: &ViewInfo, b: &Aabb, blocked: &dyn Fn(Pos2) -> bool) -> Vec<FaceHandle> {
    let mut out = Vec::with_capacity(6);
    if view.is_2d() || b.is_empty() || !b.min.is_finite() || !b.max.is_finite() {
        return out;
    }
    let cam = view.cam3d.pos;
    for axis in 0..3 {
        if b.max[axis] - b.min[axis] <= 1e-6 {
            continue;
        }
        for positive in [true, false] {
            let p = face_center(b, axis, positive);
            let n = axis_vec(axis) * if positive { 1.0 } else { -1.0 };
            let Some(face) = view.world_to_screen(p).filter(|s| s.is_finite() && view.rect.contains(*s)) else {
                continue;
            };
            let probe = units_per_point_at(view, p) * PROBE;
            let Some(tip) = view.world_to_screen(p + n * probe).filter(|s| s.is_finite()) else { continue };
            let v = tip - face;
            let len = v.length();
            if !len.is_finite() || len < PROBE as f32 * MIN_FORESHORTEN {
                continue;
            }
            let dir = v / len;
            let front = (cam - p).dot(n) > 0.0;
            let mut h = FaceHandle { axis, positive, face, dir, knob: face, front };
            let mut gap = GAP * h.scale();
            while h.with_gap(gap).head_points().iter().any(|p| blocked(*p)) {
                gap += PUSH_STEP;
                if gap > PUSH_MAX {
                    break;
                }
            }
            if gap <= PUSH_MAX {
                h.knob = face + dir * gap;
                out.push(h);
            }
        }
    }
    out
}

pub fn hit(handles: &[FaceHandle], pos: Pos2) -> Option<(usize, bool)> {
    if !pos.is_finite() {
        return None;
    }
    handles
        .iter()
        .filter_map(|h| {
            let d = h.distance(pos);
            (d <= PICK).then_some((!h.front, d, h.id()))
        })
        .min_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)))
        .map(|(_, _, id)| id)
}

pub fn resize_cursor(dir: Vec2) -> CursorIcon {
    let a = dir.y.atan2(dir.x).to_degrees().rem_euclid(180.0);
    if !(22.5..157.5).contains(&a) {
        CursorIcon::ResizeHorizontal
    } else if (67.5..112.5).contains(&a) {
        CursorIcon::ResizeVertical
    } else if a < 90.0 {
        CursorIcon::ResizeNwSe
    } else {
        CursorIcon::ResizeNeSw
    }
}

fn style(h: &FaceHandle, hot: bool) -> (Color32, Color32, f32) {
    let base = axis_color(h.axis);
    let mut color = if hot { base.lerp_to_gamma(Color32::WHITE, 0.45) } else { base };
    let mut dark = Color32::from_black_alpha(200);
    if !h.front && !hot {
        color = color.gamma_multiply(BACK_ALPHA);
        dark = dark.gamma_multiply(BACK_ALPHA);
    }
    (color, dark, h.scale() * if hot { 1.2 } else { 1.0 })
}

pub fn paint_stems(painter: &Painter, handles: &[FaceHandle], hot: Option<(usize, bool)>) {
    for front in [false, true] {
        for h in handles.iter().filter(|h| h.front == front) {
            paint_stem(painter, h, hot == Some(h.id()));
        }
    }
}

pub fn paint_heads(painter: &Painter, handles: &[FaceHandle], hot: Option<(usize, bool)>) {
    for front in [false, true] {
        for h in handles.iter().filter(|h| h.front == front) {
            paint_head(painter, h, hot == Some(h.id()));
        }
    }
}

pub fn paint(painter: &Painter, h: &FaceHandle, hot: bool) {
    paint_stem(painter, h, hot);
    paint_head(painter, h, hot);
}

fn paint_stem(painter: &Painter, h: &FaceHandle, hot: bool) {
    let (color, dark, s) = style(h, hot);
    let back = h.knob - h.dir * HEAD * s * 0.5;
    let side = Vec2::new(-h.dir.y, h.dir.x) * 4.5 * s;
    for (width, c) in [(3.5 * s, dark), (1.6 * s, color)] {
        painter.line_segment([h.face, back], Stroke::new(width, c));
        painter.line_segment([h.face - side, h.face + side], Stroke::new(width + 0.6, c));
    }
}

fn paint_head(painter: &Painter, h: &FaceHandle, hot: bool) {
    let (color, dark, s) = style(h, hot);
    let r = HEAD * 0.5 * s;
    if hot {
        painter.circle_filled(h.knob, r + 5.0, color.gamma_multiply(0.25));
    }
    painter.circle(h.knob, r, color, Stroke::new(1.2, dark));
    let side = Vec2::new(-h.dir.y, h.dir.x);
    let tip = h.knob + h.dir * r * 0.5;
    let base = h.knob - h.dir * r * 0.25;
    let alpha = if h.front || hot { 1.0 } else { BACK_ALPHA };
    let white = Stroke::new(1.6 * s, Color32::WHITE.gamma_multiply(alpha));
    painter.add(Shape::line(vec![base + side * r * 0.5, tip, base - side * r * 0.5], white));
}
