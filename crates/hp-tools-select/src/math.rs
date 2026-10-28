use egui::{Pos2, Rect};
use glam::{DMat4, DQuat, DVec2, DVec3};
use hp_app::ViewKind;
use hp_geom::ViewAxis;

pub use hp_app::grid::AxisSnap;

pub const ROTATE_SNAP_DEG: f64 = 15.0;

pub fn view_axis(kind: ViewKind) -> Option<ViewAxis> {
    match kind {
        ViewKind::Top => Some(ViewAxis::Top),
        ViewKind::Front => Some(ViewAxis::Front),
        ViewKind::Side => Some(ViewAxis::Side),
        ViewKind::Perspective => None,
    }
}

pub fn axis_vec(k: usize) -> DVec3 {
    match k {
        0 => DVec3::X,
        1 => DVec3::Y,
        _ => DVec3::Z,
    }
}

pub fn snap(v: f64, grid: f64) -> f64 {
    hp_geom::snap(v, grid)
}

pub fn box_snap(start: f64, raw: f64, grid: f64, enabled: bool) -> f64 {
    if !enabled || grid <= 0.0 || !grid.is_finite() {
        return raw;
    }
    snap(start + raw, grid) - start
}

pub fn box_snap_2d(start: DVec2, raw: DVec2, grid: f64, enabled: bool) -> DVec2 {
    DVec2::new(box_snap(start.x, raw.x, grid, enabled), box_snap(start.y, raw.y, grid, enabled))
}

pub fn normalize_deg(deg: f64) -> f64 {
    if !deg.is_finite() {
        return 0.0;
    }
    let mut d = deg % 360.0;
    if d <= -180.0 {
        d += 360.0;
    } else if d > 180.0 {
        d -= 360.0;
    }
    d
}

pub fn snap_angle(deg: f64, step: f64, free: bool) -> f64 {
    if free || step <= 0.0 || !step.is_finite() {
        return deg;
    }
    (deg / step).round() * step + 0.0
}

pub fn angle_2d_deg(v: DVec2) -> f64 {
    v.y.atan2(v.x).to_degrees()
}

pub fn signed_angle_deg(a: DVec3, b: DVec3, axis: DVec3) -> f64 {
    let n = axis.normalize_or_zero();
    let a = a - n * a.dot(n);
    let b = b - n * b.dot(n);
    if a.length_squared() < 1e-18 || b.length_squared() < 1e-18 {
        return 0.0;
    }
    a.cross(b).dot(n).atan2(a.dot(b)).to_degrees()
}

pub fn rotation_about(pivot: DVec3, axis: DVec3, deg: f64) -> DMat4 {
    let Some(axis) = axis.try_normalize() else { return DMat4::IDENTITY };
    let q = DQuat::from_axis_angle(axis, deg.to_radians());
    DMat4::from_translation(pivot) * DMat4::from_quat(q) * DMat4::from_translation(-pivot)
}

pub fn scale_about(pivot: DVec3, axes: [DVec3; 3], factors: DVec3) -> DMat4 {
    let r = glam::DMat3::from_cols(axes[0], axes[1], axes[2]);
    let l = r * glam::DMat3::from_diagonal(factors) * r.transpose();
    DMat4::from_translation(pivot) * DMat4::from_mat3(l) * DMat4::from_translation(-pivot)
}

pub fn box_to_box(from_min: DVec3, from_max: DVec3, to_min: DVec3, to_max: DVec3) -> DMat4 {
    let fs = from_max - from_min;
    let ts = to_max - to_min;
    let mut s = DVec3::ONE;
    for k in 0..3 {
        if fs[k].abs() > 1e-9 {
            s[k] = ts[k] / fs[k];
        }
    }
    DMat4::from_translation(to_min) * DMat4::from_scale(s) * DMat4::from_translation(-from_min)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HandleMode {
    #[default]
    Scale,
    Rotate,
    Skew,
}

impl HandleMode {
    pub const ALL: [HandleMode; 3] = [HandleMode::Scale, HandleMode::Rotate, HandleMode::Skew];

    pub fn next(self) -> HandleMode {
        match self {
            HandleMode::Scale => HandleMode::Rotate,
            HandleMode::Rotate => HandleMode::Skew,
            HandleMode::Skew => HandleMode::Scale,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            HandleMode::Scale => "seltool.mode.scale",
            HandleMode::Rotate => "seltool.mode.rotate",
            HandleMode::Skew => "seltool.mode.skew",
        }
    }

    pub fn handles(self) -> &'static [Handle] {
        match self {
            HandleMode::Scale => &Handle::ALL,
            HandleMode::Rotate => &Handle::CORNERS,
            HandleMode::Skew => &Handle::EDGES,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Handle {
    pub x: i8,
    pub y: i8,
}

impl Handle {
    pub const fn new(x: i8, y: i8) -> Self {
        Self { x, y }
    }

    pub const CORNERS: [Handle; 4] = [Handle::new(-1, 1), Handle::new(1, 1), Handle::new(1, -1), Handle::new(-1, -1)];

    pub const EDGES: [Handle; 4] = [Handle::new(0, 1), Handle::new(1, 0), Handle::new(0, -1), Handle::new(-1, 0)];

    pub const ALL: [Handle; 8] = [
        Handle::new(-1, 1),
        Handle::new(0, 1),
        Handle::new(1, 1),
        Handle::new(1, 0),
        Handle::new(1, -1),
        Handle::new(0, -1),
        Handle::new(-1, -1),
        Handle::new(-1, 0),
    ];

    pub fn is_corner(self) -> bool {
        self.x != 0 && self.y != 0
    }

    pub fn cursor(self, mode: HandleMode) -> egui::CursorIcon {
        use egui::CursorIcon as C;
        match mode {
            HandleMode::Scale => match (self.x, self.y) {
                (0, _) => C::ResizeVertical,
                (_, 0) => C::ResizeHorizontal,
                (x, y) if x == y => C::ResizeNeSw,
                _ => C::ResizeNwSe,
            },
            HandleMode::Rotate => C::Grab,
            HandleMode::Skew => {
                if self.y != 0 {
                    C::ResizeHorizontal
                } else {
                    C::ResizeVertical
                }
            }
        }
    }
}

pub const HANDLE_SIZE: f32 = 7.0;
pub const HANDLE_GAP: f32 = 5.0;
pub const HANDLE_PICK_SLOP: f32 = 3.0;

pub fn handle_pos(rect: Rect, h: Handle) -> Pos2 {
    handle_pos_sized(rect, h, HANDLE_SIZE)
}

pub fn handle_pos_sized(rect: Rect, h: Handle, size: f32) -> Pos2 {
    let c = rect.center();
    let hx = rect.width() * 0.5 + HANDLE_GAP + size * 0.5;
    let hy = rect.height() * 0.5 + HANDLE_GAP + size * 0.5;
    Pos2::new(c.x + f32::from(h.x) * hx, c.y - f32::from(h.y) * hy)
}

pub fn hit_handle(rect: Rect, mode: HandleMode, pos: Pos2) -> Option<Handle> {
    hit_handle_sized(rect, mode, pos, HANDLE_SIZE)
}

pub fn hit_handle_sized(rect: Rect, mode: HandleMode, pos: Pos2, size: f32) -> Option<Handle> {
    if !rect.is_finite() || !pos.is_finite() || !size.is_finite() {
        return None;
    }
    let reach = size * 0.5 + HANDLE_PICK_SLOP;
    mode.handles()
        .iter()
        .copied()
        .filter_map(|h| {
            let d = handle_pos_sized(rect, h, size) - pos;
            (d.x.abs() <= reach && d.y.abs() <= reach).then_some((d.length_sq(), h))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, h)| h)
}

pub fn min_box_size(size: f64, grid: f64, snap_on: bool) -> f64 {
    if snap_on && grid > 0.0 && grid.is_finite() { grid.min(size).max(1.0) } else { 1.0 }
}

#[allow(clippy::too_many_arguments)]
pub fn stretch_span(
    lo: f64,
    hi: f64,
    side: i8,
    d: f64,
    grid: f64,
    snap_on: bool,
    symmetric: bool,
    min_size: f64,
) -> (f64, f64) {
    stretch_span_on(lo, hi, side, d, AxisSnap::step(grid), snap_on, symmetric, min_size)
}

#[allow(clippy::too_many_arguments)]
pub fn stretch_span_on(
    lo: f64,
    hi: f64,
    side: i8,
    d: f64,
    grid: AxisSnap,
    snap_on: bool,
    symmetric: bool,
    min_size: f64,
) -> (f64, f64) {
    if side == 0 || !d.is_finite() {
        return (lo, hi);
    }
    let moved = if side > 0 { hi + d } else { lo + d };
    let moved = if snap_on { grid.snap(moved) } else { moved };
    if symmetric {
        let c = (lo + hi) * 0.5;
        let half = if side > 0 { moved - c } else { c - moved };
        let half = half.max(min_size * 0.5);
        return (c - half, c + half);
    }
    if side > 0 { (lo, moved.max(lo + min_size)) } else { (moved.min(hi - min_size), hi) }
}

#[allow(clippy::too_many_arguments)]
pub fn stretch_box(
    min: DVec3,
    max: DVec3,
    axis: usize,
    positive: bool,
    d: f64,
    grid: f64,
    snap_on: bool,
    symmetric: bool,
) -> (DVec3, DVec3) {
    stretch_box_on(min, max, axis, positive, d, AxisSnap::step(grid), snap_on, symmetric)
}

#[allow(clippy::too_many_arguments)]
pub fn stretch_box_on(
    min: DVec3,
    max: DVec3,
    axis: usize,
    positive: bool,
    d: f64,
    grid: AxisSnap,
    snap_on: bool,
    symmetric: bool,
) -> (DVec3, DVec3) {
    let k = axis.min(2);
    let min_size = min_box_size(max[k] - min[k], grid.step, snap_on);
    let side = if positive { 1 } else { -1 };
    let (lo, hi) = stretch_span_on(min[k], max[k], side, d, grid, snap_on, symmetric, min_size);
    let (mut a, mut b) = (min, max);
    a[k] = lo.clamp(-hp_geom::MAX_COORD, hp_geom::MAX_COORD);
    b[k] = hi.clamp(-hp_geom::MAX_COORD, hp_geom::MAX_COORD);
    if b[k] - a[k] < min_size {
        return (min, max);
    }
    (a, b)
}

pub fn scale_rect(
    min: DVec2,
    max: DVec2,
    handle: Handle,
    raw: DVec2,
    grid: f64,
    snap_on: bool,
    ctrl: bool,
) -> (DVec2, DVec2) {
    scale_rect_on(min, max, handle, raw, [AxisSnap::step(grid); 2], snap_on, ctrl)
}

pub fn scale_rect_on(
    min: DVec2,
    max: DVec2,
    handle: Handle,
    raw: DVec2,
    grid: [AxisSnap; 2],
    snap_on: bool,
    ctrl: bool,
) -> (DVec2, DVec2) {
    let size = max - min;
    let keep_aspect = ctrl && handle.is_corner();
    if keep_aspect && size.x > 1e-9 && size.y > 1e-9 {
        let min_size = min_box_size(size.x.max(size.y), grid[0].step.min(grid[1].step), snap_on);
        let hx = f64::from(handle.x);
        let hy = f64::from(handle.y);
        let sx = (size.x + hx * raw.x) / size.x;
        let sy = (size.y + hy * raw.y) / size.y;
        let mut s = if (sx - 1.0).abs() >= (sy - 1.0).abs() { sx } else { sy };
        if snap_on {
            if (sx - 1.0).abs() >= (sy - 1.0).abs() {
                if grid[0].is_on() {
                    s = snap(size.x * s, grid[0].step).max(min_size) / size.x;
                }
            } else if grid[1].is_on() {
                s = snap(size.y * s, grid[1].step).max(min_size) / size.y;
            }
        }
        let s = s.max(min_size / size.x.min(size.y)).max(1e-3);
        let new = size * s;
        let (mut lo, mut hi) = (min, max);
        if handle.x > 0 {
            hi.x = min.x + new.x;
        } else {
            lo.x = max.x - new.x;
        }
        if handle.y > 0 {
            hi.y = min.y + new.y;
        } else {
            lo.y = max.y - new.y;
        }
        return (lo, hi);
    }
    let symmetric = ctrl && !handle.is_corner();
    let (mut lo, mut hi) = (min, max);
    for k in 0..2 {
        let side = if k == 0 { handle.x } else { handle.y };
        let min_size = min_box_size(size[k], grid[k].step, snap_on);
        (lo[k], hi[k]) = stretch_span_on(min[k], max[k], side, raw[k], grid[k], snap_on, symmetric, min_size);
    }
    (lo, hi)
}

pub fn skew_matrix(r: usize, u: usize, handle: Handle, min: DVec2, max: DVec2, amount: f64) -> Option<DMat4> {
    if handle.is_corner() || (handle.x == 0 && handle.y == 0) || r > 2 || u > 2 || r == u || !amount.is_finite() {
        return None;
    }
    let (along, across, moving, fixed) = if handle.y != 0 {
        let (m, f) = if handle.y > 0 { (max.y, min.y) } else { (min.y, max.y) };
        (r, u, m, f)
    } else {
        let (m, f) = if handle.x > 0 { (max.x, min.x) } else { (min.x, max.x) };
        (u, r, m, f)
    };
    let span = moving - fixed;
    if span.abs() < 1e-9 {
        return None;
    }
    let s = amount / span;
    let mut m = DMat4::IDENTITY;
    m.col_mut(across)[along] = s;
    m.w_axis[along] = -s * fixed;
    Some(m)
}

pub fn closest_param_on_line(ray_o: DVec3, ray_d: DVec3, line_o: DVec3, line_d: DVec3) -> Option<f64> {
    let a = ray_d.dot(ray_d);
    let b = ray_d.dot(line_d);
    let c = line_d.dot(line_d);
    let w = ray_o - line_o;
    let d = ray_d.dot(w);
    let e = line_d.dot(w);
    let denom = a * c - b * b;
    if a < 1e-18 || c < 1e-18 || denom.abs() < 1e-6 * a * c {
        return None;
    }
    let t = (a * e - b * d) / denom;
    t.is_finite().then_some(t)
}

pub fn ray_plane(ray_o: DVec3, ray_d: DVec3, p0: DVec3, n: DVec3) -> Option<DVec3> {
    let denom = ray_d.dot(n);
    if denom.abs() < 1e-9 {
        return None;
    }
    let t = (p0 - ray_o).dot(n) / denom;
    (t > 0.0 && t.is_finite()).then(|| ray_o + ray_d * t)
}

pub fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 < 1e-9 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

pub fn point_in_convex(poly: &[Pos2], p: Pos2) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut sign = 0.0f32;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let c = (b - a).x * (p - a).y - (b - a).y * (p - a).x;
        if c.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if c.signum() != sign {
            return false;
        }
    }
    sign != 0.0
}

pub fn dominant_horizontal_axis(v: DVec3) -> DVec3 {
    if v.x.abs() < 1e-9 && v.y.abs() < 1e-9 {
        return DVec3::X;
    }
    if v.x.abs() >= v.y.abs() { DVec3::X * v.x.signum() } else { DVec3::Y * v.y.signum() }
}

pub fn units_per_point_at(view: &hp_app::ViewInfo, p: DVec3) -> f64 {
    if view.is_2d() {
        return view.units_per_point();
    }
    let cam = &view.cam3d;
    let depth = (p - cam.pos).dot(cam.forward()).max(cam.near.max(0.1));
    let h = f64::from(view.rect.height().max(1.0));
    2.0 * depth * (cam.fov_y_deg.to_radians() * 0.5).tan() / h
}

pub fn fmt_len(v: f64) -> String {
    hp_app::widgets::fmt_coord(v)
}

pub fn fmt_deg(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 { format!("{}", v.round() as i64) } else { format!("{v:.1}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn box_snapping_moves_the_corner_onto_the_grid() {
        assert!(approx(box_snap(0.0, 5.0, 16.0, true), 0.0));
        assert!(approx(box_snap(0.0, 9.0, 16.0, true), 16.0));
        assert!(approx(box_snap(0.0, -9.0, 16.0, true), -16.0));
        assert!(approx(box_snap(3.0, 10.0, 16.0, true), 13.0));
        assert!(approx(box_snap(3.0, 0.0, 16.0, true), -3.0));
        assert!(approx(box_snap(3.0, 10.0, 16.0, false), 10.0));
        assert!(approx(box_snap(3.0, 10.0, 0.0, true), 10.0));
        let d = box_snap_2d(DVec2::new(-256.0, 256.0), DVec2::new(37.0, -40.0), 32.0, true);
        assert_eq!(d, DVec2::new(32.0, -32.0));
    }

    #[test]
    fn angle_snapping() {
        assert!(approx(snap_angle(7.0, 15.0, false), 0.0));
        assert!(approx(snap_angle(8.0, 15.0, false), 15.0));
        assert!(approx(snap_angle(-23.0, 15.0, false), -30.0));
        assert!(approx(snap_angle(-23.0, 15.0, true), -23.0));
        assert!(approx(snap_angle(89.0, 15.0, false), 90.0));
        assert!(approx(normalize_deg(190.0), -170.0));
        assert!(approx(normalize_deg(-180.0), 180.0));
        assert!(approx(normalize_deg(540.0), 180.0));
        assert!(approx(normalize_deg(f64::NAN), 0.0));
        assert!(approx(angle_2d_deg(DVec2::new(0.0, 1.0)), 90.0));
    }

    #[test]
    fn signed_angles_follow_the_right_hand_rule() {
        assert!(approx(signed_angle_deg(DVec3::X, DVec3::Y, DVec3::Z), 90.0));
        assert!(approx(signed_angle_deg(DVec3::Y, DVec3::X, DVec3::Z), -90.0));
        assert!(approx(signed_angle_deg(DVec3::X, DVec3::Y, DVec3::NEG_Z), -90.0));
        assert!(approx(signed_angle_deg(DVec3::new(1.0, 0.0, 5.0), DVec3::new(0.0, 1.0, -3.0), DVec3::Z), 90.0));
        assert!(approx(signed_angle_deg(DVec3::ZERO, DVec3::Y, DVec3::Z), 0.0));
        let m = rotation_about(DVec3::new(10.0, 0.0, 0.0), DVec3::Z, 90.0);
        assert!((m.transform_point3(DVec3::new(20.0, 0.0, 0.0)) - DVec3::new(10.0, 10.0, 0.0)).length() < 1e-9);
        assert_eq!(rotation_about(DVec3::ZERO, DVec3::ZERO, 45.0), DMat4::IDENTITY);
    }

    #[test]
    fn handle_hit_testing() {
        let rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(200.0, 160.0));
        for mode in HandleMode::ALL {
            for &h in mode.handles() {
                let p = handle_pos(rect, h);
                assert_eq!(hit_handle(rect, mode, p), Some(h), "{mode:?} {h:?}");
                assert_eq!(hit_handle(rect, mode, p + egui::vec2(2.0, -2.0)), Some(h));
            }
        }
        assert_eq!(hit_handle(rect, HandleMode::Scale, rect.center()), None);
        assert_eq!(hit_handle(rect, HandleMode::Scale, Pos2::new(150.0, 130.0)), None);
        let tl = handle_pos(rect, Handle::new(-1, 1));
        assert!(tl.x < rect.left() && tl.y < rect.top());
        let top = handle_pos(rect, Handle::new(0, 1));
        assert_eq!(hit_handle(rect, HandleMode::Rotate, top), None);
        assert_eq!(hit_handle(rect, HandleMode::Skew, tl), None);
        assert_eq!(hit_handle(rect, HandleMode::Scale, Pos2::new(500.0, 500.0)), None);
        assert_eq!(hit_handle(Rect::NOTHING, HandleMode::Scale, Pos2::ZERO), None);
        assert_eq!(HandleMode::Scale.next().next().next(), HandleMode::Scale);
        assert_eq!(Handle::new(1, 1).cursor(HandleMode::Scale), egui::CursorIcon::ResizeNeSw);
        assert_eq!(Handle::new(-1, 1).cursor(HandleMode::Scale), egui::CursorIcon::ResizeNwSe);
        assert_eq!(Handle::new(0, 1).cursor(HandleMode::Skew), egui::CursorIcon::ResizeHorizontal);
    }

    #[test]
    fn scale_rect_snaps_and_never_collapses() {
        let (min, max) = (DVec2::new(0.0, 0.0), DVec2::new(64.0, 32.0));
        let (lo, hi) = scale_rect(min, max, Handle::new(1, 0), DVec2::new(21.0, 50.0), 16.0, true, false);
        assert_eq!((lo, hi), (min, DVec2::new(80.0, 32.0)));
        let (lo, hi) = scale_rect(min, max, Handle::new(-1, -1), DVec2::new(-10.0, 10.0), 16.0, true, false);
        assert_eq!((lo, hi), (DVec2::new(-16.0, 16.0), max));
        let (lo, hi) = scale_rect(min, max, Handle::new(1, 0), DVec2::new(-500.0, 0.0), 16.0, true, false);
        assert_eq!((lo, hi), (min, DVec2::new(16.0, 32.0)));
        let (lo, hi) = scale_rect(min, max, Handle::new(0, 1), DVec2::new(0.0, 3.5), 16.0, false, false);
        assert_eq!((lo, hi), (min, DVec2::new(64.0, 35.5)));
        let (_, hi) = scale_rect(min, max, Handle::new(0, 1), DVec2::new(0.0, -100.0), 16.0, false, false);
        assert_eq!(hi.y, 1.0);
        let (lo, hi) = scale_rect(min, max, Handle::new(1, 1), DVec2::new(64.0, 5.0), 16.0, true, true);
        assert_eq!((lo, hi), (min, DVec2::new(128.0, 64.0)));
    }

    #[test]
    fn skew_matrices() {
        let (min, max) = (DVec2::new(0.0, 0.0), DVec2::new(64.0, 64.0));
        let m = skew_matrix(0, 1, Handle::new(0, 1), min, max, 32.0).expect("matrix");
        assert!((m.transform_point3(DVec3::new(0.0, 64.0, 5.0)) - DVec3::new(32.0, 64.0, 5.0)).length() < 1e-9);
        assert!((m.transform_point3(DVec3::new(10.0, 0.0, 0.0)) - DVec3::new(10.0, 0.0, 0.0)).length() < 1e-9);
        assert!((m.transform_point3(DVec3::new(0.0, 32.0, 0.0)) - DVec3::new(16.0, 32.0, 0.0)).length() < 1e-9);
        let m = skew_matrix(0, 1, Handle::new(0, -1), min, max, 32.0).expect("matrix");
        assert!((m.transform_point3(DVec3::new(0.0, 0.0, 0.0)) - DVec3::new(32.0, 0.0, 0.0)).length() < 1e-9);
        assert!((m.transform_point3(DVec3::new(0.0, 64.0, 0.0)) - DVec3::new(0.0, 64.0, 0.0)).length() < 1e-9);
        let m =
            skew_matrix(1, 2, Handle::new(1, 0), DVec2::new(-32.0, 0.0), DVec2::new(32.0, 16.0), 16.0).expect("matrix");
        assert!((m.transform_point3(DVec3::new(7.0, 32.0, 0.0)) - DVec3::new(7.0, 32.0, 16.0)).length() < 1e-9);
        assert!((m.transform_point3(DVec3::new(7.0, -32.0, 0.0)) - DVec3::new(7.0, -32.0, 0.0)).length() < 1e-9);
        assert!(skew_matrix(0, 1, Handle::new(1, 1), min, max, 1.0).is_none());
        assert!(skew_matrix(0, 1, Handle::new(0, 1), min, DVec2::new(64.0, 0.0), 1.0).is_none());
        assert!(skew_matrix(0, 0, Handle::new(0, 1), min, max, 1.0).is_none());
    }

    #[test]
    fn gizmo_axis_drag_math() {
        let ray_o = DVec3::new(0.0, -100.0, 0.0);
        let t0 = closest_param_on_line(ray_o, DVec3::Y, DVec3::ZERO, DVec3::X).expect("t0");
        assert!(approx(t0, 0.0));
        let dir = (DVec3::new(40.0, 0.0, 0.0) - ray_o).normalize();
        let t1 = closest_param_on_line(ray_o, dir, DVec3::ZERO, DVec3::X).expect("t1");
        assert!((t1 - 40.0).abs() < 1e-6, "{t1}");
        let t2 = closest_param_on_line(DVec3::new(25.0, -100.0, 10.0), DVec3::Y, DVec3::ZERO, DVec3::X).expect("t2");
        assert!((t2 - 25.0).abs() < 1e-6, "{t2}");
        assert!(closest_param_on_line(ray_o, DVec3::X, DVec3::ZERO, DVec3::X).is_none());
        let hit = ray_plane(DVec3::new(5.0, 5.0, 100.0), DVec3::NEG_Z, DVec3::ZERO, DVec3::Z).expect("hit");
        assert!((hit - DVec3::new(5.0, 5.0, 0.0)).length() < 1e-9);
        assert!(ray_plane(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, DVec3::ZERO, DVec3::Z).is_none());
        assert!(ray_plane(DVec3::ZERO, DVec3::X, DVec3::ZERO, DVec3::Z).is_none());
    }

    #[test]
    fn screen_helpers() {
        let a = Pos2::new(0.0, 0.0);
        let b = Pos2::new(10.0, 0.0);
        assert!((dist_to_segment(Pos2::new(5.0, 3.0), a, b) - 3.0).abs() < 1e-6);
        assert!((dist_to_segment(Pos2::new(-4.0, 3.0), a, b) - 5.0).abs() < 1e-6);
        let quad = [Pos2::new(0.0, 0.0), Pos2::new(10.0, 0.0), Pos2::new(10.0, 10.0), Pos2::new(0.0, 10.0)];
        assert!(point_in_convex(&quad, Pos2::new(5.0, 5.0)));
        assert!(!point_in_convex(&quad, Pos2::new(15.0, 5.0)));
        let rev: Vec<Pos2> = quad.iter().rev().copied().collect();
        assert!(point_in_convex(&rev, Pos2::new(5.0, 5.0)));
        assert!(!point_in_convex(&quad[..2], Pos2::new(5.0, 0.0)));
        assert_eq!(dominant_horizontal_axis(DVec3::new(-0.8, 0.3, 0.5)), DVec3::NEG_X);
        assert_eq!(dominant_horizontal_axis(DVec3::new(0.1, 0.3, -0.9)), DVec3::Y);
        assert_eq!(dominant_horizontal_axis(DVec3::Z), DVec3::X);
        assert_eq!(fmt_len(64.0), "64");
        assert_eq!(fmt_deg(45.0), "45");
        assert_eq!(fmt_deg(12.34), "12.3");
    }

    #[test]
    fn scaling_about_a_pivot_along_local_axes() {
        let m = scale_about(DVec3::new(10.0, 0.0, 0.0), [DVec3::X, DVec3::Y, DVec3::Z], DVec3::new(2.0, 1.0, 1.0));
        assert!((m.transform_point3(DVec3::new(20.0, 5.0, 0.0)) - DVec3::new(30.0, 5.0, 0.0)).length() < 1e-9);
        let m = scale_about(DVec3::ZERO, [DVec3::Y, DVec3::NEG_X, DVec3::Z], DVec3::new(3.0, 1.0, 1.0));
        assert!((m.transform_point3(DVec3::new(1.0, 1.0, 1.0)) - DVec3::new(1.0, 3.0, 1.0)).length() < 1e-9);
    }

    #[test]
    fn box_mapping_keeps_flat_axes() {
        let m = box_to_box(DVec3::ZERO, DVec3::new(64.0, 64.0, 0.0), DVec3::ONE, DVec3::new(129.0, 33.0, 1.0));
        assert!((m.transform_point3(DVec3::new(64.0, 64.0, 0.0)) - DVec3::new(129.0, 33.0, 1.0)).length() < 1e-9);
        assert!((m.transform_point3(DVec3::ZERO) - DVec3::ONE).length() < 1e-9);
    }
}
