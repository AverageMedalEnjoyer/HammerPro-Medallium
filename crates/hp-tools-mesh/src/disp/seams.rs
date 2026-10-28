use std::collections::{HashMap, HashSet};

use glam::DVec3;
use hp_editor::{Document, FaceRef};
use hp_geom::{Aabb, DispMesh};
use hp_vmf::DispInfo;

use super::paint::nearby_disp_faces;

pub(crate) const SEAM_TOL: f64 = 0.25;
pub(crate) const NEIGHBOUR_MIN_DOT: f64 = 0.5;
pub(crate) const UNDO_SEW: &str = "Sew displacements";

pub(crate) fn border_indices(n: usize) -> [Vec<usize>; 4] {
    if n < 2 {
        return [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    }
    let last = n - 1;
    [
        (0..n).collect(),
        (0..n).map(|i| i * n + last).collect(),
        (0..n).rev().map(|j| last * n + j).collect(),
        (0..n).rev().map(|i| i * n).collect(),
    ]
}

pub(crate) fn base_corners(mesh: &DispMesh) -> Option<[DVec3; 4]> {
    let n = mesh.row_len;
    if n < 2 || mesh.base_positions.len() < n * n {
        return None;
    }
    let p = |k: usize| mesh.base_positions.get(k).copied();
    Some([p(0)?, p(n - 1)?, p(n * n - 1)?, p((n - 1) * n)?])
}

pub(crate) fn edge_overlap(a0: DVec3, a1: DVec3, b0: DVec3, b1: DVec3) -> Option<(f64, f64)> {
    let d = a1 - a0;
    let len = d.length();
    if len.is_nan() || len <= SEAM_TOL {
        return None;
    }
    let u = d / len;
    for p in [b0, b1] {
        let v = p - a0;
        if (v - u * v.dot(u)).length() > SEAM_TOL {
            return None;
        }
    }
    let (t0, t1) = ((b0 - a0).dot(u), (b1 - a0).dot(u));
    let lo = t0.min(t1).max(0.0);
    let hi = t0.max(t1).min(len);
    (hi - lo > SEAM_TOL).then_some((lo, hi))
}

pub(crate) fn shares_edge(a: &[DVec3; 4], b: &[DVec3; 4]) -> bool {
    (0..4).any(|i| (0..4).any(|j| edge_overlap(a[i], a[(i + 1) % 4], b[j], b[(j + 1) % 4]).is_some()))
}

pub(crate) fn flat_distance(p: DVec3, quad: &[DVec3; 4], normal: DVec3) -> f64 {
    let n = normal.try_normalize().unwrap_or(DVec3::Z);
    let q = p - n * (p - quad[0]).dot(n);
    let mut sign = 0.0f64;
    let mut inside = true;
    let mut best = f64::INFINITY;
    for i in 0..4 {
        let (a, b) = (quad[i], quad[(i + 1) % 4]);
        let ab = b - a;
        let side = ab.cross(q - a).dot(n);
        if side.abs() > 1e-9 {
            if sign == 0.0 {
                sign = side.signum();
            } else if side.signum() != sign {
                inside = false;
            }
        }
        let t = if ab.length_squared() > 0.0 { ((q - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
        best = best.min(q.distance(a + ab * t));
    }
    if inside { 0.0 } else { best }
}

fn mesh_of(doc: &Document, f: FaceRef) -> Option<&DispMesh> {
    doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)).map(|m| &**m)
}

pub(crate) fn brush_faces(
    doc: &Document,
    start: FaceRef,
    center: DVec3,
    radius: f64,
    spatial: bool,
    allowed: &[FaceRef],
) -> Vec<FaceRef> {
    let keep = |f: &FaceRef| allowed.is_empty() || allowed.contains(f);
    if spatial {
        return nearby_disp_faces(doc, center, radius).into_iter().filter(keep).collect();
    }
    let Some(start_mesh) = mesh_of(doc, start) else { return Vec::new() };
    let up = start_mesh.face_normal;
    let mut seen: HashSet<FaceRef> = HashSet::from([start]);
    let mut order = vec![start];
    let mut queue = vec![start];
    while let Some(f) = queue.pop() {
        let Some(corners) = mesh_of(doc, f).and_then(base_corners) else { continue };
        let bounds = Aabb::from_points(corners).expand(SEAM_TOL * 2.0);
        for id in doc.solids_in_box(&bounds) {
            if !doc.is_solid_visible(id) {
                continue;
            }
            let Some(node) = doc.scene().solid(id) else { continue };
            for (side, mesh) in node.disp.iter().enumerate() {
                let g = FaceRef::new(id, side);
                let Some(mesh) = mesh else { continue };
                if seen.contains(&g) || mesh.face_normal.dot(up) < NEIGHBOUR_MIN_DOT {
                    continue;
                }
                let Some(other) = base_corners(mesh) else { continue };
                if !shares_edge(&corners, &other) || flat_distance(center, &other, mesh.face_normal) > radius {
                    continue;
                }
                seen.insert(g);
                order.push(g);
                queue.push(g);
            }
        }
    }
    order.into_iter().filter(keep).collect()
}

pub(crate) fn nearest_disp_face(doc: &Document, p: DVec3) -> Option<FaceRef> {
    let mut best: Option<(f64, FaceRef)> = None;
    for f in nearby_disp_faces(doc, p, 0.0) {
        let Some(mesh) = mesh_of(doc, f) else { continue };
        let n = mesh.face_normal;
        let lift = mesh.aabb.size().length() + p.distance(mesh.aabb.center()) + 1.0;
        let ray = hp_geom::Ray::new(p - n * lift, n);
        let Some(t) = hp_geom::ray_disp(&ray, mesh) else { continue };
        let d = (t - lift).abs();
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, f));
        }
    }
    best.map(|(_, f)| f)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SewPlan {
    pub updates: Vec<(FaceRef, DispInfo)>,
    pub vertices: usize,
}

struct Part<'a> {
    face: FaceRef,
    mesh: &'a DispMesh,
    disp: &'a DispInfo,
    pos: Vec<DVec3>,
    active: Vec<bool>,
    locked: bool,
}

impl Part<'_> {
    fn active(&self, k: usize) -> bool {
        self.active.get(k).copied().unwrap_or(false)
    }
}

pub(crate) fn plan_sew(doc: &Document, faces: &[FaceRef], locked: &[FaceRef]) -> SewPlan {
    sew_impl(doc, faces, None, locked)
}

pub(crate) fn plan_live_sew(
    doc: &Document,
    faces: &[FaceRef],
    before: &HashMap<FaceRef, Vec<DVec3>>,
    locked: &[FaceRef],
) -> SewPlan {
    sew_impl(doc, faces, Some(before), locked)
}

fn sew_impl(
    doc: &Document,
    faces: &[FaceRef],
    live: Option<&HashMap<FaceRef, Vec<DVec3>>>,
    locked: &[FaceRef],
) -> SewPlan {
    let mut parts: Vec<Part<'_>> = Vec::new();
    for f in faces {
        if parts.iter().any(|p| p.face == *f) {
            continue;
        }
        let Some(node) = doc.scene().solid(f.solid) else { continue };
        let (Some(mesh), Some(disp)) =
            (node.disp_mesh(f.side), node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()))
        else {
            continue;
        };
        let n = mesh.row_len;
        if n < 2 || mesh.positions.len() != n * n || mesh.base_positions.len() != n * n {
            continue;
        }
        let active = match live.and_then(|l| l.get(f)) {
            Some(old) if old.len() == n * n => {
                mesh.positions.iter().zip(old).map(|(p, q)| p.distance_squared(*q) > 1e-18).collect()
            }
            _ => Vec::new(),
        };
        let locked = locked.contains(f);
        parts.push(Part { face: *f, mesh, disp, pos: mesh.positions.clone(), active, locked });
    }
    if parts.len() < 2 || (live.is_some() && parts.iter().all(|p| !p.active.contains(&true))) {
        return SewPlan::default();
    }

    let cell = |p: DVec3| -> [i64; 3] {
        let c = (p / (SEAM_TOL * 4.0)).floor();
        [c.x as i64, c.y as i64, c.z as i64]
    };
    let mut grid: HashMap<[i64; 3], Vec<(usize, usize)>> = HashMap::new();
    for (pi, part) in parts.iter().enumerate() {
        let mut border: Vec<usize> = border_indices(part.mesh.row_len).into_iter().flatten().collect();
        border.sort_unstable();
        border.dedup();
        for k in border {
            if let Some(b) = part.mesh.base_positions.get(k) {
                grid.entry(cell(*b)).or_default().push((pi, k));
            }
        }
    }
    let mut averaged: Vec<(usize, usize, DVec3)> = Vec::new();
    for (pi, part) in parts.iter().enumerate() {
        if part.locked {
            continue;
        }
        for k in border_indices(part.mesh.row_len).into_iter().flatten() {
            let Some(&base) = part.mesh.base_positions.get(k) else { continue };
            let c = cell(base);
            let own = part.mesh.positions.get(k).copied().unwrap_or(base);
            let mut members = vec![(pi, own, part.active(k))];
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        let Some(list) = grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) else { continue };
                        for &(pj, kj) in list {
                            if members.iter().any(|m| m.0 == pj) {
                                continue;
                            }
                            let other = &parts[pj];
                            if other.mesh.base_positions.get(kj).is_some_and(|b| b.distance(base) <= SEAM_TOL) {
                                let p = other.mesh.positions.get(kj).copied().unwrap_or(base);
                                members.push((pj, p, other.active(kj)));
                            }
                        }
                    }
                }
            }
            if members.len() < 2 {
                continue;
            }
            let leaders: Vec<DVec3> = members.iter().filter(|m| live.is_none() || m.2).map(|m| m.1).collect();
            if leaders.is_empty() {
                continue;
            }
            if let Some(anchor) = members.iter().find(|m| parts[m.0].locked) {
                averaged.push((pi, k, anchor.1));
                continue;
            }
            let sum: DVec3 = leaders.iter().copied().sum();
            averaged.push((pi, k, sum / leaders.len() as f64));
        }
    }
    for (pi, k, p) in averaged {
        if let Some(slot) = parts.get_mut(pi).and_then(|part| part.pos.get_mut(k)) {
            *slot = p;
        }
    }
    let changed: Vec<Vec<bool>> = parts
        .iter()
        .map(|part| {
            (0..part.pos.len())
                .map(|k| {
                    live.is_none()
                        || part.active(k)
                        || part.pos.get(k).zip(part.mesh.positions.get(k)).is_some_and(|(a, b)| a.distance(*b) > 1e-9)
                })
                .collect()
        })
        .collect();
    let changed_at = |pi: usize, k: usize| changed.get(pi).and_then(|c| c.get(k)).copied().unwrap_or(false);

    let mut placed: Vec<(usize, usize, DVec3)> = Vec::new();
    for a in 0..parts.len() {
        for b in 0..parts.len() {
            if a == b || parts[b].locked {
                continue;
            }
            let (pa, pb) = (&parts[a], &parts[b]);
            let (Some(ca), Some(cb)) = (base_corners(pa.mesh), base_corners(pb.mesh)) else { continue };
            let (ba, bb) = (border_indices(pa.mesh.row_len), border_indices(pb.mesh.row_len));
            for i in 0..4 {
                for j in 0..4 {
                    let (a0, a1) = (ca[i], ca[(i + 1) % 4]);
                    let (b0, b1) = (cb[j], cb[(j + 1) % 4]);
                    let Some((lo, hi)) = edge_overlap(a0, a1, b0, b1) else { continue };
                    let len = a0.distance(a1);
                    let spacing_a = len / (pa.mesh.row_len - 1) as f64;
                    let spacing_b = b0.distance(b1) / (pb.mesh.row_len - 1) as f64;
                    let same = (spacing_a - spacing_b).abs() <= 1e-9;
                    if (!same && spacing_a < spacing_b) || (same && a > b) {
                        continue;
                    }
                    let u = (a1 - a0) / len;
                    let coarse: Vec<(f64, DVec3, bool)> = ba[i]
                        .iter()
                        .filter_map(|&k| {
                            Some(((pa.mesh.base_positions.get(k)? - a0).dot(u), *pa.pos.get(k)?, changed_at(a, k)))
                        })
                        .collect();
                    for &k in &bb[j] {
                        let Some(base) = pb.mesh.base_positions.get(k) else { continue };
                        let t = (*base - a0).dot(u);
                        if t < lo - SEAM_TOL || t > hi + SEAM_TOL {
                            continue;
                        }
                        if coarse.iter().any(|(tc, _, _)| (tc - t).abs() <= SEAM_TOL) {
                            continue;
                        }
                        let Some(w) = coarse.windows(2).find(|w| w[0].0 <= t && t <= w[1].0) else { continue };
                        if !(changed_at(b, k) || w[0].2 || w[1].2) {
                            continue;
                        }
                        let s = (t - w[0].0) / (w[1].0 - w[0].0).max(1e-12);
                        placed.push((b, k, w[0].1.lerp(w[1].1, s)));
                    }
                }
            }
        }
    }
    for (pi, k, p) in placed {
        if let Some(slot) = parts.get_mut(pi).and_then(|part| part.pos.get_mut(k)) {
            *slot = p;
        }
    }

    let mut plan = SewPlan::default();
    for part in parts {
        if part.locked {
            continue;
        }
        let mesh = part.mesh;
        let mut disp = part.disp.clone();
        let count = mesh.row_len * mesh.row_len;
        let fnorm = mesh.face_normal;
        disp.normals.resize(count, fnorm);
        disp.distances.resize(count, 0.0);
        disp.offsets.resize(count, DVec3::ZERO);
        disp.offset_normals.resize(count, fnorm);
        disp.alphas.resize(count, 0.0);
        let elevation = if disp.elevation.is_finite() { fnorm * disp.elevation } else { DVec3::ZERO };
        let mut moved = 0;
        for k in 0..count {
            let (Some(&old), Some(&new), Some(&base)) =
                (mesh.positions.get(k), part.pos.get(k), mesh.base_positions.get(k))
            else {
                continue;
            };
            if !new.is_finite() || old.distance(new) < 1e-6 {
                continue;
            }
            let offset = disp.offsets.get(k).copied().filter(|o| o.is_finite()).unwrap_or(DVec3::ZERO);
            let v = new - base - offset - elevation;
            let len = v.length();
            let (Some(normal), Some(dist)) = (disp.normals.get_mut(k), disp.distances.get_mut(k)) else { continue };
            if len > 1e-9 {
                *normal = v / len;
                *dist = len;
            } else {
                *normal = fnorm;
                *dist = 0.0;
            }
            moved += 1;
        }
        if moved > 0 {
            if let Some(poly) = doc.scene().solid(part.face.solid).and_then(|n| n.geom.face_for_side(part.face.side)) {
                hp_geom::update_disp_tags(&mut disp, &poly.vertices);
            }
            plan.vertices += moved;
            plan.updates.push((part.face, disp));
        }
    }
    plan
}

pub(crate) fn edge_neighbours(doc: &Document, face: FaceRef) -> Vec<FaceRef> {
    let Some(corners) = mesh_of(doc, face).and_then(base_corners) else { return Vec::new() };
    let bounds = Aabb::from_points(corners).expand(SEAM_TOL * 2.0);
    let mut out = Vec::new();
    for id in doc.solids_in_box(&bounds) {
        if !doc.is_solid_visible(id) {
            continue;
        }
        let Some(node) = doc.scene().solid(id) else { continue };
        for (side, mesh) in node.disp.iter().enumerate() {
            let g = FaceRef::new(id, side);
            if g == face {
                continue;
            }
            let Some(other) = mesh.as_deref().and_then(base_corners) else { continue };
            if shares_edge(&corners, &other) {
                out.push(g);
            }
        }
    }
    out
}
