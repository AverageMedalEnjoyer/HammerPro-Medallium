use egui::{Pos2, Rect};
use glam::DVec2;
use hp_editor::FaceRef;

use crate::uv_editor::{UvFrame, normalize_deg};
use crate::uv_layers::Affine2;

pub(crate) type Elem = (FaceRef, usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelOp {
    Replace,
    Add,
    Toggle,
}

impl SelOp {
    pub(crate) fn of(mods: egui::Modifiers) -> SelOp {
        if mods.command || mods.ctrl {
            SelOp::Toggle
        } else if mods.shift {
            SelOp::Add
        } else {
            SelOp::Replace
        }
    }
}

pub(crate) fn pick(sel: &mut Vec<Elem>, items: &[Elem], op: SelOp) {
    match op {
        SelOp::Replace => {
            sel.clear();
            for i in items {
                if !sel.contains(i) {
                    sel.push(*i);
                }
            }
        }
        SelOp::Add => {
            for i in items {
                sel.retain(|e| e != i);
                sel.push(*i);
            }
        }
        SelOp::Toggle => {
            for i in items {
                match sel.iter().position(|e| e == i) {
                    Some(k) => {
                        sel.remove(k);
                    }
                    None => sel.push(*i),
                }
            }
        }
    }
}

pub(crate) fn main_of(sel: &[Elem], face: FaceRef) -> Option<usize> {
    sel.iter().rev().find(|(f, _)| *f == face).map(|(_, i)| *i)
}

pub(crate) fn nearest_vertex(outlines: &[(FaceRef, &[Pos2])], p: Pos2, r: f32) -> Option<Elem> {
    let mut best: Option<(Elem, f32)> = None;
    for (f, pts) in outlines {
        for (i, q) in pts.iter().enumerate() {
            let d = q.distance(p);
            if d <= r && best.is_none_or(|(_, b)| d < b) {
                best = Some(((*f, i), d));
            }
        }
    }
    best.map(|(e, _)| e)
}

fn segment_distance(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() > 0.0 { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t).distance(p)
}

pub(crate) fn nearest_edge(outlines: &[(FaceRef, &[Pos2])], p: Pos2, r: f32) -> Option<Elem> {
    let mut best: Option<(Elem, f32)> = None;
    for (f, pts) in outlines {
        let n = pts.len();
        if n < 2 {
            continue;
        }
        for i in 0..n {
            let d = segment_distance(pts[i], pts[(i + 1) % n], p);
            if d <= r && best.is_none_or(|(_, b)| d < b) {
                best = Some(((*f, i), d));
            }
        }
    }
    best.map(|(e, _)| e)
}

pub(crate) fn vertices_in(outlines: &[(FaceRef, &[Pos2])], rect: Rect) -> Vec<Elem> {
    let mut out = Vec::new();
    for (f, pts) in outlines {
        for (i, q) in pts.iter().enumerate() {
            if rect.contains(*q) {
                out.push((*f, i));
            }
        }
    }
    out
}

pub(crate) fn edges_in(outlines: &[(FaceRef, &[Pos2])], rect: Rect) -> Vec<Elem> {
    let mut out = Vec::new();
    for (f, pts) in outlines {
        let n = pts.len();
        if n < 2 {
            continue;
        }
        for i in 0..n {
            if rect.contains(pts[i]) && rect.contains(pts[(i + 1) % n]) {
                out.push((*f, i));
            }
        }
    }
    out
}

pub(crate) fn moved_mask(n: usize, face: FaceRef, verts: &[Elem], edges: &[Elem]) -> Vec<bool> {
    let mut out = vec![false; n];
    for (f, i) in verts {
        if *f == face && *i < n {
            out[*i] = true;
        }
    }
    for (f, i) in edges {
        if *f == face && *i < n && n > 0 {
            out[*i] = true;
            out[(*i + 1) % n] = true;
        }
    }
    out
}

pub(crate) fn pins(uvs: &[DVec2], moved: &[bool]) -> Vec<usize> {
    let sel: Vec<DVec2> = uvs.iter().zip(moved).filter(|(_, m)| **m).map(|(u, _)| *u).collect();
    if sel.is_empty() || sel.len() == uvs.len() {
        return Vec::new();
    }
    let mid = sel.iter().fold(DVec2::ZERO, |a, u| a + *u) / sel.len() as f64;
    let mut kept: Vec<(usize, f64)> = uvs
        .iter()
        .enumerate()
        .filter(|(i, _)| !moved.get(*i).copied().unwrap_or(false))
        .map(|(i, u)| (i, (*u - mid).length_squared()))
        .collect();
    kept.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let want = if sel.len() == 1 { 1 } else { 2 };
    let mut out: Vec<usize> = Vec::new();
    for (i, _) in kept {
        if out.len() >= want {
            break;
        }
        if out.iter().all(|j| (uvs[*j] - uvs[i]).length_squared() > 1e-12) {
            out.push(i);
        }
    }
    out
}

fn sym_eigen(mut a: [[f64; 3]; 3], k: usize) -> ([f64; 3], [[f64; 3]; 3]) {
    let mut v = [[0.0; 3]; 3];
    for (i, row) in v.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for _ in 0..32 {
        let mut off = 0.0;
        for (i, row) in a.iter().enumerate().take(k) {
            off += row.iter().take(k).skip(i + 1).map(|x| x * x).sum::<f64>();
        }
        if off < 1e-30 {
            break;
        }
        for p in 0..k {
            for q in (p + 1)..k {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for row in a.iter_mut().take(k) {
                    let (arp, arq) = (row[p], row[q]);
                    row[p] = c * arp - s * arq;
                    row[q] = s * arp + c * arq;
                }
                let (old_p, old_q) = (a[p], a[q]);
                for (r, (apr, aqr)) in old_p.iter().zip(old_q.iter()).enumerate().take(k) {
                    a[p][r] = c * apr - s * aqr;
                    a[q][r] = s * apr + c * aqr;
                }
                for row in v.iter_mut().take(k) {
                    let (vp, vq) = (row[p], row[q]);
                    row[p] = c * vp - s * vq;
                    row[q] = s * vp + c * vq;
                }
            }
        }
    }
    ([a[0][0], a[1][1], a[2][2]], v)
}

fn lexi_lsq(tiers: &[Vec<([f64; 3], f64)>]) -> [f64; 3] {
    let mut x = [0.0; 3];
    let mut free: Vec<[f64; 3]> = vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for rows in tiers {
        let k = free.len();
        if k == 0 {
            break;
        }
        if rows.is_empty() {
            continue;
        }
        let mut g = [[0.0; 3]; 3];
        let mut h = [0.0; 3];
        for (a, b) in rows {
            let dot = |u: &[f64; 3], w: &[f64; 3]| u[0] * w[0] + u[1] * w[1] + u[2] * w[2];
            let m: Vec<f64> = free.iter().map(|n| dot(a, n)).collect();
            let r = b - dot(a, &x);
            for i in 0..k {
                h[i] += m[i] * r;
                for j in 0..k {
                    g[i][j] += m[i] * m[j];
                }
            }
        }
        let (vals, vecs) = sym_eigen(g, k);
        let top = vals.iter().take(k).fold(0.0f64, |m, v| m.max(v.abs()));
        if !(top.is_finite() && top > 1e-300) {
            continue;
        }
        let tol = top * 1e-10;
        let mut z = [0.0; 3];
        let mut next: Vec<[f64; 3]> = Vec::new();
        for e in 0..k {
            let col: Vec<f64> = (0..k).map(|i| vecs[i][e]).collect();
            if vals[e] > tol {
                let c = (0..k).map(|i| col[i] * h[i]).sum::<f64>() / vals[e];
                for i in 0..k {
                    z[i] += c * col[i];
                }
            } else {
                let mut n = [0.0; 3];
                for (i, base) in free.iter().enumerate() {
                    for d in 0..3 {
                        n[d] += col[i] * base[d];
                    }
                }
                next.push(n);
            }
        }
        for (i, base) in free.iter().enumerate() {
            for d in 0..3 {
                x[d] += z[i] * base[d];
            }
        }
        free = next;
    }
    x
}

pub(crate) fn vertex_affine(uvs: &[DVec2], moved: &[bool], d: DVec2) -> Option<Affine2> {
    if uvs.is_empty() || moved.len() != uvs.len() || !d.is_finite() {
        return None;
    }
    let count = moved.iter().filter(|m| **m).count();
    if count == 0 {
        return Some(Affine2::IDENTITY);
    }
    if count == uvs.len() {
        return Some(Affine2 { m: Affine2::IDENTITY.m, t: [d.x, d.y] });
    }
    let c = uvs.iter().fold(DVec2::ZERO, |a, u| a + *u) / uvs.len() as f64;
    let size = uvs.iter().map(|u| (*u - c).length()).fold(0.0f64, f64::max);
    if !(size.is_finite() && size > 1e-9) {
        return None;
    }
    let norm = |u: DVec2| (u - c) / size;
    let pinned = pins(uvs, moved);
    let mut tiers: [Vec<(DVec2, DVec2)>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for (i, u) in uvs.iter().enumerate() {
        let (tier, to) = if moved[i] {
            (0, *u + d)
        } else if pinned.contains(&i) {
            (1, *u)
        } else {
            (2, *u)
        };
        tiers[tier].push((norm(*u), norm(to)));
    }
    let solve = |axis: usize| {
        let mut rows: Vec<Vec<([f64; 3], f64)>> =
            tiers.iter().map(|t| t.iter().map(|(p, q)| ([p.x, p.y, 1.0], q[axis])).collect()).collect();
        let unit = |i: usize| {
            let mut r = [0.0; 3];
            r[i] = 1.0;
            r
        };
        rows.push((0..3).map(|i| (unit(i), if i == axis { 1.0 } else { 0.0 })).collect());
        lexi_lsq(&rows)
    };
    let (rx, ry) = (solve(0), solve(1));
    let m = [[rx[0], rx[1]], [ry[0], ry[1]]];
    let t =
        [c.x + size * rx[2] - (m[0][0] * c.x + m[0][1] * c.y), c.y + size * ry[2] - (m[1][0] * c.x + m[1][1] * c.y)];
    let out = Affine2 { m, t };
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    let finite = out.m.iter().flatten().chain(out.t.iter()).all(|v| v.is_finite());
    (finite && det.abs() > 1e-4 && det.abs() < 1e4).then_some(out)
}

pub(crate) fn edge_angle(a: DVec2, b: DVec2) -> Option<f64> {
    let d = b - a;
    (d.is_finite() && d.length_squared() > 1e-18).then(|| normalize_deg(d.y.atan2(d.x).to_degrees()))
}

impl UvFrame {
    pub(crate) fn transformed(&self, t: &Affine2) -> Option<UvFrame> {
        let m = t.m;
        let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
        if !(det.is_finite() && det.abs() > 1e-6) {
            return None;
        }
        let a = self.a * m[0][0] + self.b * m[0][1];
        let b = self.a * m[1][0] + self.b * m[1][1];
        let s = t.apply([self.shift.x, self.shift.y]);
        let turn = m[1][0].atan2(m[0][0]).to_degrees();
        let out = UvFrame { a, b, shift: DVec2::new(s[0], s[1]), rotation: normalize_deg(self.rotation - turn) };
        (out.a.is_finite() && out.b.is_finite() && out.shift.is_finite()).then_some(out)
    }
}

pub(crate) fn layer_map_moved(map: &Affine2, t: &Affine2, tex: DVec2) -> Option<Affine2> {
    let r = Affine2 {
        m: [[t.m[0][0], t.m[0][1] * tex.y / tex.x], [t.m[1][0] * tex.x / tex.y, t.m[1][1]]],
        t: [t.t[0] / tex.x, t.t[1] / tex.y],
    };
    Some(map.after(&r.inverse()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    const A: FaceRef = FaceRef { solid: 1, side: 0 };
    const B: FaceRef = FaceRef { solid: 1, side: 1 };

    fn apply(t: &Affine2, u: DVec2) -> DVec2 {
        let p = t.apply([u.x, u.y]);
        DVec2::new(p[0], p[1])
    }

    fn square() -> Vec<DVec2> {
        vec![DVec2::new(0.0, 0.0), DVec2::new(256.0, 0.0), DVec2::new(256.0, 128.0), DVec2::new(0.0, 128.0)]
    }

    const EPS: f64 = 1e-7;

    #[test]
    fn one_vertex_moves_and_the_opposite_one_stays() {
        let uvs = square();
        let d = DVec2::new(30.0, -12.0);
        let moved = [false, false, true, false];
        assert_eq!(pins(&uvs, &moved), vec![0], "the farthest vertex is the pivot");
        let t = vertex_affine(&uvs, &moved, d).expect("map");
        assert!((apply(&t, uvs[2]) - (uvs[2] + d)).length() < EPS, "the vertex follows the drag");
        assert!((apply(&t, uvs[0]) - uvs[0]).length() < EPS, "the pivot stays");
        let m1 = apply(&t, uvs[1]) - uvs[1];
        let m3 = apply(&t, uvs[3]) - uvs[3];
        assert!(m1.length() < d.length() && m3.length() < d.length(), "{m1:?} {m3:?}");
    }

    #[test]
    fn two_vertices_move_and_two_stay() {
        let uvs = square();
        for d in [DVec2::new(40.0, 0.0), DVec2::new(0.0, 25.0), DVec2::new(-17.0, 33.0)] {
            let moved = [false, true, true, false];
            let mut p = pins(&uvs, &moved);
            p.sort_unstable();
            assert_eq!(p, vec![0, 3]);
            let t = vertex_affine(&uvs, &moved, d).expect("map");
            for i in 0..4 {
                let want = if moved[i] { uvs[i] + d } else { uvs[i] };
                assert!((apply(&t, uvs[i]) - want).length() < EPS, "vertex {i}: {:?} {want:?}", apply(&t, uvs[i]));
            }
        }
        let moved = [true, false, true, false];
        let d = DVec2::new(10.0, 10.0);
        let t = vertex_affine(&uvs, &moved, d).expect("map");
        assert!((apply(&t, uvs[0]) - (uvs[0] + d)).length() < EPS && (apply(&t, uvs[2]) - (uvs[2] + d)).length() < EPS);
    }

    #[test]
    fn three_and_four_vertices() {
        let uvs = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(200.0, 0.0),
            DVec2::new(200.0, 100.0),
            DVec2::new(100.0, 100.0),
            DVec2::new(0.0, 100.0),
        ];
        let moved = [false, false, true, true, true];
        let d = DVec2::new(0.0, 40.0);
        let t = vertex_affine(&uvs, &moved, d).expect("map");
        for i in 0..5 {
            let want = if moved[i] { uvs[i] + d } else { uvs[i] };
            assert!((apply(&t, uvs[i]) - want).length() < EPS, "vertex {i}");
        }
        let uvs = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(300.0, 0.0),
            DVec2::new(300.0, 64.0),
            DVec2::new(200.0, 64.0),
            DVec2::new(100.0, 64.0),
            DVec2::new(0.0, 64.0),
        ];
        let moved = [false, false, true, true, true, true];
        let d = DVec2::new(16.0, 32.0);
        let t = vertex_affine(&uvs, &moved, d).expect("map");
        for i in 0..6 {
            let want = if moved[i] { uvs[i] + d } else { uvs[i] };
            assert!((apply(&t, uvs[i]) - want).length() < EPS, "vertex {i}");
        }
        let t = vertex_affine(&square(), &[true; 4], d).expect("map");
        assert_eq!(t.m, Affine2::IDENTITY.m);
        assert!((apply(&t, DVec2::ZERO) - d).length() < EPS);
        let t = vertex_affine(&square(), &[true, true, true, false], d).expect("map");
        for (i, u) in square().iter().enumerate().take(3) {
            assert!((apply(&t, *u) - (*u + d)).length() < EPS, "vertex {i}");
        }
        assert_eq!(vertex_affine(&square(), &[false; 4], d), Some(Affine2::IDENTITY));
        let tri = vec![DVec2::new(0.0, 0.0), DVec2::new(100.0, 0.0), DVec2::new(0.0, 100.0)];
        assert!(vertex_affine(&tri, &[false, true, false], DVec2::new(-100.0, 100.0)).is_none());
    }

    #[test]
    fn frames_follow_the_map() {
        let f = UvFrame {
            a: glam::DVec3::new(0.5, 0.1, 0.0),
            b: glam::DVec3::new(-0.1, 0.25, 0.3),
            shift: DVec2::new(12.0, -7.0),
            rotation: 10.0,
        };
        let pts =
            [glam::DVec3::new(10.0, 0.0, 0.0), glam::DVec3::new(-30.0, 64.0, 8.0), glam::DVec3::new(5.0, 5.0, 5.0)];
        let t = Affine2 { m: [[1.2, 0.3], [-0.2, 0.9]], t: [5.0, -3.0] };
        let g = f.transformed(&t).expect("frame");
        for p in pts {
            assert!((g.uv(p) - apply(&t, f.uv(p))).length() < 1e-9);
        }
        let (s, c) = 30f64.to_radians().sin_cos();
        let center = DVec2::new(40.0, 20.0);
        let m = [[c, -s], [s, c]];
        let rot = Affine2 {
            m,
            t: [
                center.x - (m[0][0] * center.x + m[0][1] * center.y),
                center.y - (m[1][0] * center.x + m[1][1] * center.y),
            ],
        };
        let a = f.transformed(&rot).expect("frame");
        let b = f.rotated(30.0, center);
        assert!((a.a - b.a).length() < 1e-12 && (a.b - b.b).length() < 1e-12 && (a.shift - b.shift).length() < 1e-9);
        assert!((a.rotation - b.rotation).abs() < 1e-9);
        assert!(f.transformed(&Affine2 { m: [[1.0, 2.0], [2.0, 4.0]], t: [0.0, 0.0] }).is_none());
        let tex = DVec2::new(256.0, 128.0);
        let map = Affine2 { m: [[2.0, 0.1], [0.0, 1.5]], t: [0.3, -0.2] };
        let w = layer_map_moved(&map, &t, tex).expect("map");
        let x = DVec2::new(70.0, 33.0);
        let moved = apply(&t, x);
        let before = map.apply([x.x / tex.x, x.y / tex.y]);
        let after = w.apply([moved.x / tex.x, moved.y / tex.y]);
        assert!((before[0] - after[0]).abs() < 1e-9 && (before[1] - after[1]).abs() < 1e-9);
    }

    #[test]
    fn picks_at_any_zoom() {
        for zoom in [0.05f32, 0.4, 1.0, 3.0, 12.0] {
            let pts: Vec<Pos2> =
                square().iter().map(|u| pos2(300.0 + u.x as f32 * zoom, 200.0 + u.y as f32 * zoom)).collect();
            let other: Vec<Pos2> = pts.iter().map(|p| *p + egui::vec2(0.0, 400.0 + 130.0 * zoom)).collect();
            let outlines = [(A, pts.as_slice()), (B, other.as_slice())];
            let off = if zoom < 0.2 { egui::vec2(0.5, -0.5) } else { egui::vec2(5.0, -4.0) };
            for (i, q) in pts.iter().enumerate() {
                assert_eq!(nearest_vertex(&outlines, *q + off, 9.0), Some((A, i)), "zoom {zoom}");
                assert_eq!(
                    nearest_vertex(&outlines, *q + egui::vec2(12.0, 0.0), 9.0).filter(|e| e.0 == A && e.1 == i),
                    None
                );
            }
            assert_eq!(nearest_vertex(&outlines, other[2] + egui::vec2(-3.0, 3.0), 9.0), Some((B, 2)));
            if zoom >= 0.4 {
                let mid = pts[1] + (pts[2] - pts[1]) * 0.5;
                assert_eq!(nearest_edge(&outlines, mid + egui::vec2(6.0, 0.0), 7.0), Some((A, 1)), "zoom {zoom}");
                assert_eq!(nearest_edge(&outlines, mid + egui::vec2(9.0, 0.0), 7.0), None);
            }
            let all = Rect::from_min_max(pts[0] - egui::vec2(1.0, 1.0), pts[2] + egui::vec2(1.0, 1.0));
            assert_eq!(vertices_in(&outlines, all).len(), 4);
            assert_eq!(edges_in(&outlines, all).len(), 4);
            let right = Rect::from_min_max(pts[1] - egui::vec2(1.0, 1.0), pts[2] + egui::vec2(1.0, 1.0));
            assert_eq!(edges_in(&outlines, right), vec![(A, 1)]);
        }
    }

    #[test]
    fn selection_ops() {
        let mut sel: Vec<Elem> = Vec::new();
        pick(&mut sel, &[(A, 0)], SelOp::Replace);
        pick(&mut sel, &[(A, 2)], SelOp::Add);
        assert_eq!(sel, vec![(A, 0), (A, 2)]);
        pick(&mut sel, &[(A, 0)], SelOp::Add);
        assert_eq!(sel, vec![(A, 2), (A, 0)], "added again: the main one");
        assert_eq!(main_of(&sel, A), Some(0));
        pick(&mut sel, &[(A, 2), (B, 1)], SelOp::Toggle);
        assert_eq!(sel, vec![(A, 0), (B, 1)]);
        pick(&mut sel, &[(B, 3)], SelOp::Replace);
        assert_eq!(sel, vec![(B, 3)]);
        pick(&mut sel, &[], SelOp::Replace);
        assert!(sel.is_empty());
        let m = SelOp::of(egui::Modifiers { ctrl: true, command: true, ..Default::default() });
        assert_eq!(m, SelOp::Toggle);
        assert_eq!(SelOp::of(egui::Modifiers::SHIFT), SelOp::Add);
        assert_eq!(SelOp::of(egui::Modifiers::NONE), SelOp::Replace);
        assert_eq!(moved_mask(4, A, &[(A, 1), (B, 0)], &[(A, 3)]), vec![true, true, false, true]);
        assert_eq!(edge_angle(DVec2::ZERO, DVec2::new(0.0, 5.0)), Some(90.0));
        assert_eq!(edge_angle(DVec2::ZERO, DVec2::ZERO), None);
    }
}
