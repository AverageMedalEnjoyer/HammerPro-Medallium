use glam::{DQuat, DVec2, DVec3};
use hp_vmf::{Side, TexAxis};

use crate::util::{clean_unit, normalize_degrees};

pub const DEFAULT_TEXTURE_SCALE: f64 = 0.25;

const BASE_AXES: [[DVec3; 3]; 6] = [
    [DVec3::new(0.0, 0.0, 1.0), DVec3::new(1.0, 0.0, 0.0), DVec3::new(0.0, -1.0, 0.0)],
    [DVec3::new(0.0, 0.0, -1.0), DVec3::new(1.0, 0.0, 0.0), DVec3::new(0.0, -1.0, 0.0)],
    [DVec3::new(1.0, 0.0, 0.0), DVec3::new(0.0, 1.0, 0.0), DVec3::new(0.0, 0.0, -1.0)],
    [DVec3::new(-1.0, 0.0, 0.0), DVec3::new(0.0, 1.0, 0.0), DVec3::new(0.0, 0.0, -1.0)],
    [DVec3::new(0.0, 1.0, 0.0), DVec3::new(1.0, 0.0, 0.0), DVec3::new(0.0, 0.0, -1.0)],
    [DVec3::new(0.0, -1.0, 0.0), DVec3::new(1.0, 0.0, 0.0), DVec3::new(0.0, 0.0, -1.0)],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TextureAlign {
    #[default]
    World,
    Face,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Justify {
    Fit,
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

fn base_axis_index(normal: DVec3) -> usize {
    let mut best = 0.0;
    let mut best_index = 0;
    for (i, row) in BASE_AXES.iter().enumerate() {
        let d = normal.dot(row[0]);
        if d > best + 1e-9 {
            best = d;
            best_index = i;
        }
    }
    best_index
}

pub fn world_axes(normal: DVec3) -> (DVec3, DVec3) {
    let row = BASE_AXES.get(base_axis_index(normal)).unwrap_or(&BASE_AXES[0]);
    (row[1], row[2])
}

pub fn face_axes(normal: DVec3) -> (DVec3, DVec3) {
    let row = BASE_AXES.get(base_axis_index(normal)).unwrap_or(&BASE_AXES[0]);
    let Some(n) = normal.try_normalize() else {
        return (row[1], row[2]);
    };
    let q = DQuat::from_rotation_arc(row[0], n);
    (clean_unit(q * row[1]), clean_unit(q * row[2]))
}

#[inline]
pub fn effective_scale(scale: f64) -> f64 {
    if scale.is_finite() && scale.abs() > 1e-6 { scale } else { DEFAULT_TEXTURE_SCALE }
}

#[inline]
fn axis_coord(a: &TexAxis, p: DVec3) -> f64 {
    p.dot(a.axis) / effective_scale(a.scale) + a.shift
}

#[inline]
pub fn texel_uv(side: &Side, p: DVec3) -> DVec2 {
    DVec2::new(axis_coord(&side.uaxis, p), axis_coord(&side.vaxis, p))
}

pub fn texel_bounds(side: &Side, points: &[DVec3]) -> Option<(DVec2, DVec2)> {
    let mut it = points.iter().map(|&p| texel_uv(side, p));
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), uv| (lo.min(uv), hi.max(uv))))
}

pub fn axes_quality(side: &Side, normal: DVec3) -> f64 {
    let c = side.uaxis.axis.cross(side.vaxis.axis);
    match (c.try_normalize(), normal.try_normalize()) {
        (Some(c), Some(n)) => c.dot(n).abs(),
        _ => 0.0,
    }
}

pub fn texture_axes_valid(side: &Side, normal: DVec3) -> bool {
    side.uaxis.axis.is_finite() && side.vaxis.axis.is_finite() && axes_quality(side, normal) > 1e-3
}

pub fn reset_axes(side: &mut Side, normal: DVec3, align: TextureAlign) {
    let (u, v) = match align {
        TextureAlign::World => world_axes(normal),
        TextureAlign::Face => face_axes(normal),
    };
    side.uaxis = TexAxis { axis: u, shift: 0.0, scale: effective_scale(side.uaxis.scale) };
    side.vaxis = TexAxis { axis: v, shift: 0.0, scale: effective_scale(side.vaxis.scale) };
    side.rotation = 0.0;
}

pub fn ensure_valid_axes(side: &mut Side, normal: DVec3) -> bool {
    if texture_axes_valid(side, normal) {
        return false;
    }
    let (u, v) = world_axes(normal);
    side.uaxis.axis = u;
    side.vaxis.axis = v;
    side.uaxis.scale = effective_scale(side.uaxis.scale);
    side.vaxis.scale = effective_scale(side.vaxis.scale);
    side.rotation = 0.0;
    true
}

pub fn rotate_texture(side: &mut Side, normal: DVec3, delta_degrees: f64) {
    if !delta_degrees.is_finite() {
        return;
    }
    let axis =
        side.uaxis.axis.cross(side.vaxis.axis).try_normalize().or_else(|| normal.try_normalize()).unwrap_or(DVec3::Z);
    let q = DQuat::from_axis_angle(axis, delta_degrees.to_radians());
    side.uaxis.axis = clean_unit(q * side.uaxis.axis);
    side.vaxis.axis = clean_unit(q * side.vaxis.axis);
    side.rotation = normalize_degrees(side.rotation + delta_degrees);
}

pub fn set_rotation(side: &mut Side, normal: DVec3, degrees: f64) {
    if !degrees.is_finite() {
        return;
    }
    let current = if side.rotation.is_finite() { side.rotation } else { 0.0 };
    rotate_texture(side, normal, degrees - current);
    side.rotation = normalize_degrees(degrees);
}

pub fn shift_texture(side: &mut Side, du: f64, dv: f64) {
    if du.is_finite() {
        side.uaxis.shift += du;
    }
    if dv.is_finite() {
        side.vaxis.shift += dv;
    }
}

pub fn scale_texture(side: &mut Side, fu: f64, fv: f64) {
    if fu.is_finite() && fu != 0.0 {
        side.uaxis.scale = effective_scale(side.uaxis.scale) * fu;
    }
    if fv.is_finite() && fv != 0.0 {
        side.vaxis.scale = effective_scale(side.vaxis.scale) * fv;
    }
}

pub fn set_texture_scale(side: &mut Side, su: f64, sv: f64) {
    side.uaxis.scale = effective_scale(su);
    side.vaxis.scale = effective_scale(sv);
}

fn wrap_shift(shift: f64, size: f64) -> f64 {
    if shift.is_finite() && size > 0.0 { shift.rem_euclid(size) + 0.0 } else { 0.0 }
}

pub fn justify(side: &mut Side, points: &[DVec3], tex_w: u32, tex_h: u32, mode: Justify) {
    if mode == Justify::Fit {
        fit_texture(side, points, tex_w, tex_h, 1.0, 1.0);
        return;
    }
    let w = f64::from(tex_w.max(1));
    let h = f64::from(tex_h.max(1));
    side.uaxis.scale = effective_scale(side.uaxis.scale);
    side.vaxis.scale = effective_scale(side.vaxis.scale);
    let mut probe = side.clone();
    probe.uaxis.shift = 0.0;
    probe.vaxis.shift = 0.0;
    let Some((lo, hi)) = texel_bounds(&probe, points) else {
        return;
    };
    match mode {
        Justify::Left => side.uaxis.shift = -lo.x,
        Justify::Right => side.uaxis.shift = w - hi.x,
        Justify::Top => side.vaxis.shift = -lo.y,
        Justify::Bottom => side.vaxis.shift = h - hi.y,
        Justify::Center => {
            side.uaxis.shift = w * 0.5 - (lo.x + hi.x) * 0.5;
            side.vaxis.shift = h * 0.5 - (lo.y + hi.y) * 0.5;
        }
        Justify::Fit => {}
    }
    side.uaxis.shift = wrap_shift(side.uaxis.shift, w);
    side.vaxis.shift = wrap_shift(side.vaxis.shift, h);
}

pub fn fit_texture(side: &mut Side, points: &[DVec3], tex_w: u32, tex_h: u32, repeat_u: f64, repeat_v: f64) {
    let w = f64::from(tex_w.max(1));
    let h = f64::from(tex_h.max(1));
    let ru = if repeat_u.is_finite() && repeat_u > 0.0 { repeat_u } else { 1.0 };
    let rv = if repeat_v.is_finite() && repeat_v > 0.0 { repeat_v } else { 1.0 };
    let mut probe = side.clone();
    probe.uaxis = TexAxis { axis: side.uaxis.axis, shift: 0.0, scale: 1.0 };
    probe.vaxis = TexAxis { axis: side.vaxis.axis, shift: 0.0, scale: 1.0 };
    let Some((lo, hi)) = texel_bounds(&probe, points) else {
        return;
    };
    let span = hi - lo;
    if span.x > 1e-6 {
        side.uaxis.scale = effective_scale(span.x / (w * ru));
    }
    if span.y > 1e-6 {
        side.vaxis.scale = effective_scale(span.y / (h * rv));
    }
    justify(side, points, tex_w, tex_h, Justify::Top);
    justify(side, points, tex_w, tex_h, Justify::Left);
}

pub fn align_to_view(side: &mut Side, right: DVec3, up: DVec3) {
    let (Some(r), Some(u)) = (right.try_normalize(), up.try_normalize()) else {
        return;
    };
    side.uaxis.axis = clean_unit(r);
    side.vaxis.axis = clean_unit(-u);
    side.rotation = 0.0;
}

pub fn uv_lock(old_pts: [DVec3; 3], new_pts: [DVec3; 3], side: &mut Side) -> bool {
    if !old_pts.iter().chain(&new_pts).all(|p| p.is_finite()) {
        return false;
    }
    let tiny = |a: DVec3, b: DVec3, c: DVec3| {
        let n = (b - a).cross(c - a).length();
        let scale = (b - a).length().max((c - a).length()).max(1e-9);
        !n.is_finite() || n <= scale * scale * 1e-9
    };
    let [o0, o1, o2] = old_pts;
    let [n0, n1, n2] = new_pts;
    if tiny(o0, o1, o2) || tiny(n0, n1, n2) {
        return false;
    }
    let uv = old_pts.map(|p| texel_uv(side, p));
    let e1 = n1 - n0;
    let e2 = n2 - n0;
    let n = e1.cross(e2);
    let n2_len = n.length_squared();
    let solve = |d1: f64, d2: f64| (e2.cross(n) * d1 + n.cross(e1) * d2) / n2_len;
    let a_u = solve(uv[1].x - uv[0].x, uv[2].x - uv[0].x);
    let a_v = solve(uv[1].y - uv[0].y, uv[2].y - uv[0].y);
    let (lu, lv) = (a_u.length(), a_v.length());
    if !(lu.is_finite() && lv.is_finite()) || lu < 1e-9 || lv < 1e-9 {
        return false;
    }
    if a_u.cross(a_v).length() <= lu * lv * 1e-6 {
        return false;
    }
    let axis = |a: DVec3, len: f64, old: &TexAxis, origin_uv: f64| -> TexAxis {
        let flip = if old.scale < 0.0 { -1.0 } else { 1.0 };
        let dir = a / len * flip;
        TexAxis {
            axis: clean_unit(dir),
            shift: crate::util::clean_decimal(origin_uv - a.dot(n0)),
            scale: crate::util::clean_decimal(flip / len),
        }
    };
    let u = axis(a_u, lu, &side.uaxis, uv[0].x);
    let v = axis(a_v, lv, &side.vaxis, uv[0].y);
    if !(u.axis.is_finite() && v.axis.is_finite() && u.shift.is_finite() && v.shift.is_finite()) {
        return false;
    }
    if u.scale.abs() <= 1e-6 || v.scale.abs() <= 1e-6 || !u.scale.is_finite() || !v.scale.is_finite() {
        return false;
    }
    side.uaxis = u;
    side.vaxis = v;
    true
}

pub fn wrap_texture(src: &Side, src_normal: DVec3, dst: &mut Side, dst_normal: DVec3, edge_point: DVec3) {
    let (Some(sn), Some(dn)) = (src_normal.try_normalize(), dst_normal.try_normalize()) else {
        return;
    };
    let q = DQuat::from_rotation_arc(sn, dn);
    let fold = |a: &TexAxis| -> TexAxis {
        let s = effective_scale(a.scale);
        let axis = clean_unit(q * a.axis);
        let shift = a.shift + (edge_point.dot(a.axis) - edge_point.dot(axis)) / s;
        TexAxis { axis, shift, scale: s }
    };
    dst.material.clone_from(&src.material);
    dst.uaxis = fold(&src.uaxis);
    dst.vaxis = fold(&src.vaxis);
    dst.rotation = src.rotation;
    dst.lightmap_scale = src.lightmap_scale;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use crate::{Plane, build_solid};

    fn near_mod(x: f64, m: f64) -> bool {
        let r = x.rem_euclid(m);
        r < 1e-6 || m - r < 1e-6
    }

    fn side_with_axes(u: DVec3, v: DVec3) -> Side {
        Side {
            uaxis: TexAxis { axis: u, shift: 0.0, scale: 0.25 },
            vaxis: TexAxis { axis: v, shift: 0.0, scale: 0.25 },
            ..Default::default()
        }
    }

    #[test]
    fn world_axes_like_hammer() {
        assert_eq!(world_axes(DVec3::Z), (DVec3::X, DVec3::NEG_Y));
        assert_eq!(world_axes(DVec3::NEG_Z), (DVec3::X, DVec3::NEG_Y));
        assert_eq!(world_axes(DVec3::X), (DVec3::Y, DVec3::NEG_Z));
        assert_eq!(world_axes(DVec3::NEG_X), (DVec3::Y, DVec3::NEG_Z));
        assert_eq!(world_axes(DVec3::Y), (DVec3::X, DVec3::NEG_Z));
        assert_eq!(world_axes(DVec3::NEG_Y), (DVec3::X, DVec3::NEG_Z));
        let n = DVec3::new(1.0, 0.0, 1.0).normalize();
        assert_eq!(world_axes(n), (DVec3::X, DVec3::NEG_Y));
        let n = DVec3::new(1.0, 1.0, 0.0).normalize();
        assert_eq!(world_axes(n), (DVec3::Y, DVec3::NEG_Z));
        for s in hammer_box(DVec3::ZERO, DVec3::splat(32.0)) {
            let n = Plane::from_side(&s).expect("plane").normal;
            assert_eq!(world_axes(n), (s.uaxis.axis, s.vaxis.axis));
        }
    }

    #[test]
    fn face_axes_lie_in_plane() {
        for n in [
            DVec3::new(0.0, -1.0, 1.0),
            DVec3::new(0.3, 0.4, 0.8),
            DVec3::new(-0.9, 0.2, 0.1),
            DVec3::new(0.1, -0.8, -0.3),
        ] {
            let n = n.normalize();
            let (u, v) = face_axes(n);
            assert!(u.dot(n).abs() < 1e-12 && v.dot(n).abs() < 1e-12);
            assert!((u.length() - 1.0).abs() < 1e-12 && u.dot(v).abs() < 1e-12);
        }
        assert_eq!(face_axes(DVec3::X), world_axes(DVec3::X));
        assert_eq!(face_axes(DVec3::NEG_Z), world_axes(DVec3::NEG_Z));
    }

    #[test]
    fn texel_coordinates() {
        let mut s = side_with_axes(DVec3::X, DVec3::NEG_Y);
        s.uaxis.shift = 10.0;
        let uv = texel_uv(&s, DVec3::new(64.0, 32.0, 0.0));
        assert_eq!(uv, DVec2::new(266.0, -128.0));
        s.uaxis.scale = 0.0;
        assert_eq!(texel_uv(&s, DVec3::new(1.0, 0.0, 0.0)).x, 14.0);
    }

    #[test]
    fn rotation() {
        let mut s = side_with_axes(DVec3::X, DVec3::NEG_Y);
        set_rotation(&mut s, DVec3::Z, 90.0);
        assert_eq!(s.rotation, 90.0);
        assert!((s.uaxis.axis - DVec3::NEG_Y).length() < 1e-12, "{:?}", s.uaxis.axis);
        assert!((s.vaxis.axis - DVec3::NEG_X).length() < 1e-12, "{:?}", s.vaxis.axis);
        set_rotation(&mut s, DVec3::Z, 0.0);
        assert_eq!(s.rotation, 0.0);
        assert_eq!(s.uaxis.axis, DVec3::X);
        assert_eq!(s.vaxis.axis, DVec3::NEG_Y);
        set_rotation(&mut s, DVec3::Z, -45.0);
        assert_eq!(s.rotation, 315.0);
        assert!(texture_axes_valid(&s, DVec3::Z));
    }

    #[test]
    fn justification() {
        let sides = hammer_box(DVec3::new(10.0, 0.0, 0.0), DVec3::new(138.0, 64.0, 32.0));
        let g = build_solid(&sides);
        let top = g.face_for_side(0).expect("top");
        let mut s = sides[0].clone();
        justify(&mut s, &top.vertices, 256, 256, Justify::Left);
        let (lo, _) = texel_bounds(&s, &top.vertices).expect("bounds");
        assert!(near_mod(lo.x, 256.0));
        justify(&mut s, &top.vertices, 256, 256, Justify::Right);
        let (_, hi) = texel_bounds(&s, &top.vertices).expect("bounds");
        assert!(near_mod(hi.x, 256.0));
        justify(&mut s, &top.vertices, 256, 256, Justify::Top);
        let (lo, _) = texel_bounds(&s, &top.vertices).expect("bounds");
        assert!(near_mod(lo.y, 256.0));
        justify(&mut s, &top.vertices, 256, 256, Justify::Bottom);
        let (_, hi) = texel_bounds(&s, &top.vertices).expect("bounds");
        assert!(near_mod(hi.y, 256.0));
        justify(&mut s, &top.vertices, 256, 256, Justify::Center);
        let c = texel_uv(&s, top.center());
        assert!(near_mod(c.x - 128.0, 256.0));
        assert!(near_mod(c.y - 128.0, 256.0));

        justify(&mut s, &top.vertices, 256, 256, Justify::Fit);
        let (lo, hi) = texel_bounds(&s, &top.vertices).expect("bounds");
        assert!(((hi - lo) - DVec2::splat(256.0)).length() < 1e-9);
        assert!(near_mod(lo.x, 256.0) && near_mod(lo.y, 256.0));
        assert!((s.uaxis.scale - 0.5).abs() < 1e-12 && (s.vaxis.scale - 0.25).abs() < 1e-12);

        fit_texture(&mut s, &top.vertices, 256, 256, 2.0, 4.0);
        let (lo, hi) = texel_bounds(&s, &top.vertices).expect("bounds");
        assert!(((hi - lo) - DVec2::new(512.0, 1024.0)).length() < 1e-9);
    }

    #[test]
    fn shift_scale_and_view() {
        let mut s = side_with_axes(DVec3::X, DVec3::NEG_Y);
        shift_texture(&mut s, 4.0, -2.0);
        assert_eq!((s.uaxis.shift, s.vaxis.shift), (4.0, -2.0));
        scale_texture(&mut s, 2.0, 0.0);
        assert_eq!((s.uaxis.scale, s.vaxis.scale), (0.5, 0.25));
        set_texture_scale(&mut s, 0.0, 1.0);
        assert_eq!((s.uaxis.scale, s.vaxis.scale), (0.25, 1.0));
        align_to_view(&mut s, DVec3::Y, DVec3::Z);
        assert_eq!((s.uaxis.axis, s.vaxis.axis), (DVec3::Y, DVec3::NEG_Z));
        let mut bad = side_with_axes(DVec3::Z, DVec3::Z);
        assert!(!texture_axes_valid(&bad, DVec3::Z));
        assert!(ensure_valid_axes(&mut bad, DVec3::Z));
        assert_eq!((bad.uaxis.axis, bad.vaxis.axis), (DVec3::X, DVec3::NEG_Y));
        reset_axes(&mut bad, DVec3::new(0.0, 1.0, 1.0), TextureAlign::Face);
        assert!(texture_axes_valid(&bad, DVec3::new(0.0, 1.0, 1.0)));
    }

    #[test]
    fn uv_lock_glues_the_texture_to_moved_points() {
        let mut s = side_with_axes(DVec3::X, DVec3::NEG_Y);
        s.uaxis.shift = 12.0;
        s.vaxis.scale = -0.5;
        let old = [DVec3::new(0.0, 0.0, 64.0), DVec3::new(128.0, 0.0, 64.0), DVec3::new(0.0, 128.0, 64.0)];
        let new = [old[0], DVec3::new(160.0, 16.0, 96.0), old[2]];
        let want = old.map(|p| texel_uv(&s, p));
        assert!(uv_lock(old, new, &mut s));
        for (p, w) in new.iter().zip(want) {
            assert!((texel_uv(&s, *p) - w).length() < 1e-6, "{p}");
        }
        let n = (new[1] - new[0]).cross(new[2] - new[0]).normalize();
        assert!(s.uaxis.axis.dot(n).abs() < 1e-9 && s.vaxis.axis.dot(n).abs() < 1e-9);
        assert!(s.uaxis.scale > 0.0 && s.vaxis.scale < 0.0);
        assert!(texture_axes_valid(&s, n));
        let mut t = side_with_axes(DVec3::X, DVec3::NEG_Y);
        assert!(uv_lock(old, old, &mut t));
        assert_eq!((t.uaxis.axis, t.vaxis.axis), (DVec3::X, DVec3::NEG_Y));
        assert!((t.uaxis.scale - 0.25).abs() < 1e-12 && t.uaxis.shift.abs() < 1e-9);
        let before = t.clone();
        let line = [DVec3::ZERO, DVec3::X, DVec3::X * 2.0];
        assert!(!uv_lock(line, old, &mut t));
        assert!(!uv_lock(old, line, &mut t));
        assert!(!uv_lock([DVec3::NAN, DVec3::X, DVec3::Y], old, &mut t));
        assert_eq!(t, before);
    }

    #[test]
    fn wrap_across_edge() {
        let sides = hammer_box(DVec3::ZERO, DVec3::new(128.0, 64.0, 32.0));
        let mut top = sides[0].clone();
        top.uaxis.shift = 17.0;
        top.vaxis.shift = 5.0;
        set_rotation(&mut top, DVec3::Z, 30.0);
        let mut wall = sides[2].clone();
        let edge = DVec3::new(0.0, 64.0, 32.0);
        wrap_texture(&top, DVec3::Z, &mut wall, DVec3::Y, edge);
        assert_eq!(wall.material, top.material);
        for x in [0.0, 37.0, 128.0] {
            let p = DVec3::new(x, 64.0, 32.0);
            assert!((texel_uv(&top, p) - texel_uv(&wall, p)).length() < 1e-9);
        }
        let wn = Plane::from_side(&wall).expect("plane").normal;
        assert!(texture_axes_valid(&wall, wn));
    }
}
