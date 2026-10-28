use glam::{DVec2, DVec3};
use hp_vmf::{Side, TexAxis};

use crate::texture::{axes_quality, effective_scale, face_axes, world_axes};
use crate::util::{FxHashMap, Welder, clean_point, clean_vec, fx_map, plane_points_from_polygon, weld_points};
use crate::{GeomError, NORMAL_EPS, PLANE_EPS, Plane, SolidGeom, WELD_EPS, build_solid};

const MIN_KEPT_AXES_QUALITY: f64 = 0.1;

#[derive(Clone, Debug, PartialEq)]
pub struct ConvexHull {
    pub points: Vec<DVec3>,
    pub faces: Vec<HullFace>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HullFace {
    pub plane: Plane,
    pub indices: Vec<usize>,
}

impl ConvexHull {
    pub fn face_plane_points(&self, face: &HullFace) -> [DVec3; 3] {
        let poly = self.face_polygon(face);
        if let Some(pts) = plane_points_from_polygon(&poly) {
            let pts = pts.map(|p| clean_point(face.plane.project(p)));
            if Plane::from_points(pts[0], pts[1], pts[2]).is_some_and(|p| p.approx_eq(&face.plane, 1e-6, 1e-3)) {
                return pts;
            }
        }
        face.plane.to_points()
    }

    pub fn face_polygon(&self, face: &HullFace) -> Vec<DVec3> {
        face.indices.iter().filter_map(|&i| self.points.get(i).copied()).collect()
    }

    pub fn volume(&self) -> f64 {
        let r = crate::util::polygon_center(&self.points);
        let mut six_v = 0.0;
        for f in &self.faces {
            let poly = self.face_polygon(f);
            let Some((&p0, rest)) = poly.split_first() else { continue };
            for w in rest.windows(2) {
                six_v += (p0 - r).dot((w[0] - r).cross(w[1] - r));
            }
        }
        six_v / 6.0
    }
}

struct Tri {
    v: [usize; 3],
    plane: Plane,
    outside: Vec<usize>,
    alive: bool,
}

#[inline]
fn tri_edges(v: [usize; 3]) -> [(usize, usize); 3] {
    [(v[0], v[1]), (v[1], v[2]), (v[2], v[0])]
}

fn plane_ccw(a: DVec3, b: DVec3, c: DVec3) -> Option<Plane> {
    let n = (b - a).cross(c - a);
    let len = n.length();
    if !len.is_finite() || len <= 1e-300 {
        return None;
    }
    let normal = n / len;
    Some(Plane { normal, dist: normal.dot(a) })
}

fn add_tri(
    tris: &mut Vec<Tri>,
    edges: &mut FxHashMap<(usize, usize), usize>,
    pts: &[DVec3],
    v: [usize; 3],
) -> Result<usize, GeomError> {
    let (Some(&a), Some(&b), Some(&c)) = (pts.get(v[0]), pts.get(v[1]), pts.get(v[2])) else {
        return Err(GeomError::HullFailed("vertex index out of range"));
    };
    let plane = plane_ccw(a, b, c).ok_or(GeomError::HullFailed("degenerate triangle"))?;
    let id = tris.len();
    for e in tri_edges(v) {
        if edges.insert(e, id).is_some() {
            return Err(GeomError::HullFailed("non manifold edge"));
        }
    }
    tris.push(Tri { v, plane, outside: Vec::new(), alive: true });
    Ok(id)
}

#[inline]
fn pt(pts: &[DVec3], i: usize) -> DVec3 {
    pts.get(i).copied().unwrap_or(DVec3::ZERO)
}

pub fn convex_hull(input: &[DVec3]) -> Result<ConvexHull, GeomError> {
    let pts = weld_points(input, WELD_EPS);
    if pts.len() < 4 {
        return Err(GeomError::TooFewPoints(pts.len()));
    }
    let scale = pts.iter().fold(1.0f64, |m, p| m.max(p.abs().max_element()));
    let eps = (scale * 1e-12).max(1e-7);

    let mut ext = [0usize; 6];
    for (i, p) in pts.iter().enumerate() {
        for axis in 0..3 {
            if p[axis] < pt(&pts, ext[2 * axis])[axis] {
                ext[2 * axis] = i;
            }
            if p[axis] > pt(&pts, ext[2 * axis + 1])[axis] {
                ext[2 * axis + 1] = i;
            }
        }
    }
    let (mut i0, mut i1, mut best) = (0usize, 0usize, -1.0f64);
    for &a in &ext {
        for &b in &ext {
            let d = pt(&pts, a).distance_squared(pt(&pts, b));
            if d > best {
                best = d;
                i0 = a;
                i1 = b;
            }
        }
    }
    if best.sqrt() <= eps {
        return Err(GeomError::DegenerateHull);
    }
    let p0 = pt(&pts, i0);
    let dir = (pt(&pts, i1) - p0).normalize_or_zero();
    let (mut i2, mut best) = (0usize, -1.0f64);
    for (i, &p) in pts.iter().enumerate() {
        let r = p - p0;
        let d = (r - dir * r.dot(dir)).length_squared();
        if d > best {
            best = d;
            i2 = i;
        }
    }
    if best.sqrt() <= eps {
        return Err(GeomError::DegenerateHull);
    }
    let n = (pt(&pts, i1) - p0).cross(pt(&pts, i2) - p0).normalize_or_zero();
    let (mut i3, mut best) = (0usize, -1.0f64);
    for (i, &p) in pts.iter().enumerate() {
        let d = (p - p0).dot(n).abs();
        if d > best {
            best = d;
            i3 = i;
        }
    }
    if best <= eps.max(1e-6) {
        return Err(GeomError::DegenerateHull);
    }

    let mut tris: Vec<Tri> = Vec::with_capacity(pts.len() * 2 + 8);
    let mut edges: FxHashMap<(usize, usize), usize> = fx_map(pts.len() * 6 + 16);
    let simplex = [i0, i1, i2, i3];
    for (face, opposite) in [([i0, i1, i2], i3), ([i0, i1, i3], i2), ([i0, i2, i3], i1), ([i1, i2, i3], i0)] {
        let mut v = face;
        let plane = plane_ccw(pt(&pts, v[0]), pt(&pts, v[1]), pt(&pts, v[2])).ok_or(GeomError::DegenerateHull)?;
        if plane.distance(pt(&pts, opposite)) > 0.0 {
            v.swap(1, 2);
        }
        add_tri(&mut tris, &mut edges, &pts, v)?;
    }
    for (i, &p) in pts.iter().enumerate() {
        if simplex.contains(&i) {
            continue;
        }
        let mut best: Option<(f64, usize)> = None;
        for (t, tri) in tris.iter().enumerate() {
            let d = tri.plane.distance(p);
            if d > eps && best.is_none_or(|(bd, _)| d > bd) {
                best = Some((d, t));
            }
        }
        if let Some((_, t)) = best
            && let Some(tri) = tris.get_mut(t)
        {
            tri.outside.push(i);
        }
    }

    let mut work: Vec<usize> = (0..tris.len()).collect();
    let mut mark: Vec<bool> = vec![false; tris.len()];
    let mut visible: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut horizon: Vec<(usize, usize)> = Vec::new();
    let mut orphans: Vec<usize> = Vec::new();
    let mut new_tris: Vec<usize> = Vec::new();
    let max_iterations = pts.len() * 4 + 64;
    let mut iterations = 0usize;
    while let Some(t) = work.pop() {
        let Some(tri) = tris.get(t) else { continue };
        if !tri.alive || tri.outside.is_empty() {
            continue;
        }
        iterations += 1;
        if iterations > max_iterations {
            return Err(GeomError::HullFailed("too many iterations"));
        }
        let plane = tri.plane;
        let mut eye = usize::MAX;
        let mut eye_d = f64::NEG_INFINITY;
        for &i in &tri.outside {
            let d = plane.distance(pt(&pts, i));
            if d > eye_d {
                eye_d = d;
                eye = i;
            }
        }
        let eye_p = pt(&pts, eye);

        visible.clear();
        stack.clear();
        visible.push(t);
        stack.push(t);
        if let Some(m) = mark.get_mut(t) {
            *m = true;
        }
        while let Some(f) = stack.pop() {
            let Some(v) = tris.get(f).map(|x| x.v) else { continue };
            for (a, b) in tri_edges(v) {
                let nb = *edges.get(&(b, a)).ok_or(GeomError::HullFailed("open mesh"))?;
                if mark.get(nb).copied().unwrap_or(true) {
                    continue;
                }
                let vis = tris.get(nb).is_some_and(|x| x.plane.distance(eye_p) > eps);
                if vis {
                    if let Some(m) = mark.get_mut(nb) {
                        *m = true;
                    }
                    visible.push(nb);
                    stack.push(nb);
                }
            }
        }
        horizon.clear();
        for &f in &visible {
            let Some(v) = tris.get(f).map(|x| x.v) else { continue };
            for (a, b) in tri_edges(v) {
                let nb = *edges.get(&(b, a)).ok_or(GeomError::HullFailed("open mesh"))?;
                if !mark.get(nb).copied().unwrap_or(false) {
                    horizon.push((a, b));
                }
            }
        }
        orphans.clear();
        for &f in &visible {
            if let Some(tri) = tris.get_mut(f) {
                orphans.append(&mut tri.outside);
                tri.alive = false;
                for e in tri_edges(tri.v) {
                    edges.remove(&e);
                }
            }
            if let Some(m) = mark.get_mut(f) {
                *m = false;
            }
        }
        new_tris.clear();
        for &(a, b) in &horizon {
            let id = add_tri(&mut tris, &mut edges, &pts, [a, b, eye])?;
            mark.push(false);
            new_tris.push(id);
        }
        for &p in &orphans {
            if p == eye {
                continue;
            }
            let q = pt(&pts, p);
            let mut best: Option<(f64, usize)> = None;
            for &nt in &new_tris {
                if let Some(tri) = tris.get(nt) {
                    let d = tri.plane.distance(q);
                    if d > eps && best.is_none_or(|(bd, _)| d > bd) {
                        best = Some((d, nt));
                    }
                }
            }
            if let Some((_, nt)) = best
                && let Some(tri) = tris.get_mut(nt)
            {
                tri.outside.push(p);
            }
        }
        for &nt in &new_tris {
            if tris.get(nt).is_some_and(|x| !x.outside.is_empty()) {
                work.push(nt);
            }
        }
    }

    merge_faces(&pts, &tris, &edges)
}

fn merge_faces(pts: &[DVec3], tris: &[Tri], edges: &FxHashMap<(usize, usize), usize>) -> Result<ConvexHull, GeomError> {
    let mut order: Vec<(f64, usize)> = tris
        .iter()
        .enumerate()
        .filter(|(_, t)| t.alive)
        .map(|(i, t)| {
            let [a, b, c] = t.v.map(|i| pt(pts, i));
            ((b - a).cross(c - a).length(), i)
        })
        .collect();
    order.sort_by(|x, y| y.0.total_cmp(&x.0));
    let mut cluster_of: Vec<bool> = vec![false; tris.len()];
    let mut remap: FxHashMap<usize, usize> = fx_map(pts.len());
    let mut out_points: Vec<DVec3> = Vec::new();
    let mut faces: Vec<HullFace> = Vec::new();
    let mut members: Vec<usize> = Vec::new();
    let mut queue: Vec<usize> = Vec::new();
    for &(_, seed) in &order {
        if cluster_of.get(seed).copied().unwrap_or(true) {
            continue;
        }
        let Some(seed_tri) = tris.get(seed) else { continue };
        let seed_plane = seed_tri.plane;
        members.clear();
        queue.clear();
        members.push(seed);
        queue.push(seed);
        if let Some(c) = cluster_of.get_mut(seed) {
            *c = true;
        }
        while let Some(t) = queue.pop() {
            let Some(v) = tris.get(t).map(|x| x.v) else { continue };
            for (a, b) in tri_edges(v) {
                let Some(&nb) = edges.get(&(b, a)) else { continue };
                if cluster_of.get(nb).copied().unwrap_or(true) {
                    continue;
                }
                let Some(ntri) = tris.get(nb) else { continue };
                let coplanar = ntri.plane.normal.dot(seed_plane.normal) > 0.9
                    && ntri.v.iter().all(|&i| seed_plane.distance(pt(pts, i)).abs() <= PLANE_EPS);
                if coplanar {
                    if let Some(c) = cluster_of.get_mut(nb) {
                        *c = true;
                    }
                    members.push(nb);
                    queue.push(nb);
                }
            }
        }
        let mut ids: Vec<usize> = members.iter().filter_map(|&t| tris.get(t)).flat_map(|t| t.v).collect();
        ids.sort_unstable();
        ids.dedup();
        let poly_ids = hull_2d(pts, &ids, seed_plane.normal);
        if poly_ids.len() < 3 {
            continue;
        }
        let poly: Vec<DVec3> = poly_ids.iter().map(|&i| pt(pts, i)).collect();
        let normal =
            crate::util::polygon_normal(&poly).filter(|n| n.dot(seed_plane.normal) > 0.9).unwrap_or(seed_plane.normal);
        let dist = ids.iter().map(|&i| normal.dot(pt(pts, i))).fold(f64::NEG_INFINITY, f64::max);
        let plane = Plane { normal, dist };
        let mut indices = Vec::with_capacity(poly_ids.len());
        for &i in &poly_ids {
            let next = out_points.len();
            let idx = *remap.entry(i).or_insert(next);
            if idx == next {
                out_points.push(pt(pts, i));
            }
            indices.push(idx);
        }
        faces.push(HullFace { plane, indices });
    }
    if faces.len() < 4 {
        return Err(GeomError::DegenerateHull);
    }
    Ok(ConvexHull { points: out_points, faces })
}

fn hull_2d(pts: &[DVec3], ids: &[usize], normal: DVec3) -> Vec<usize> {
    let u = normal.any_orthonormal_vector();
    let v = normal.cross(u);
    let mut proj: Vec<(DVec2, usize)> =
        ids.iter().map(|&i| (DVec2::new(pt(pts, i).dot(u), pt(pts, i).dot(v)), i)).collect();
    proj.sort_by(|a, b| a.0.x.total_cmp(&b.0.x).then(a.0.y.total_cmp(&b.0.y)));
    proj.dedup_by(|a, b| a.0.distance_squared(b.0) < 1e-18);
    if proj.len() < 3 {
        return Vec::new();
    }
    let extent = proj.iter().fold(0.0f64, |m, (p, _)| m.max(p.abs().max_element()));
    let tol = 1e-10 * extent * extent + 1e-14;
    let cross = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o);
    let mut hull: Vec<(DVec2, usize)> = Vec::with_capacity(proj.len() * 2);
    for &p in &proj {
        while hull.len() >= 2 {
            let (Some(&a), Some(&b)) = (hull.get(hull.len() - 2), hull.last()) else { break };
            if cross(a.0, b.0, p.0) <= tol {
                hull.pop();
            } else {
                break;
            }
        }
        hull.push(p);
    }
    let lower_len = hull.len() + 1;
    for &p in proj.iter().rev().skip(1) {
        while hull.len() >= lower_len {
            let (Some(&a), Some(&b)) = (hull.get(hull.len() - 2), hull.last()) else { break };
            if cross(a.0, b.0, p.0) <= tol {
                hull.pop();
            } else {
                break;
            }
        }
        hull.push(p);
    }
    hull.pop();
    hull.into_iter().map(|(_, i)| i).collect()
}

pub fn unique_vertices(geom: &SolidGeom) -> (Vec<DVec3>, Vec<Vec<usize>>) {
    let mut welder = Welder::new(WELD_EPS, geom.face_vertex_count());
    let mut faces = Vec::with_capacity(geom.faces.len());
    for f in &geom.faces {
        let mut idx: Vec<usize> = Vec::with_capacity(f.vertices.len());
        for &v in &f.vertices {
            let i = welder.insert(v);
            if idx.last() != Some(&i) {
                idx.push(i);
            }
        }
        if idx.len() > 1 && idx.first() == idx.last() {
            idx.pop();
        }
        faces.push(idx);
    }
    (welder.points, faces)
}

pub fn split_edge(points: &mut Vec<DVec3>, a: usize, b: usize) -> Option<usize> {
    let (pa, pb) = (*points.get(a)?, *points.get(b)?);
    points.push(clean_vec((pa + pb) * 0.5));
    Some(points.len() - 1)
}

pub fn is_convex(vertices: &[DVec3], faces: &[Vec<usize>]) -> bool {
    if faces.len() < 4 {
        return false;
    }
    for f in faces {
        let poly: Vec<DVec3> = match f.iter().map(|&i| vertices.get(i).copied()).collect::<Option<Vec<_>>>() {
            Some(p) => p,
            None => return false,
        };
        let Some(pp) = plane_points_from_polygon(&poly) else { return false };
        let Some(plane) = Plane::from_points(pp[0], pp[1], pp[2]) else { return false };
        if poly.iter().any(|&p| plane.distance(p).abs() > PLANE_EPS) {
            return false;
        }
        if vertices.iter().any(|&p| plane.distance(p) > PLANE_EPS) {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy, Debug)]
pub struct FaceTopology<'a> {
    pub faces: &'a [Vec<usize>],
    pub templates: &'a [usize],
}

#[derive(Clone, Copy, Debug)]
pub struct VertexEdit<'a> {
    pub points: &'a [DVec3],
    pub base: Option<&'a [DVec3]>,
    pub topology: Option<FaceTopology<'a>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VertexEditOptions {
    pub uv_lock: bool,
}

pub fn rebuild_from_points(
    points: &[DVec3],
    template_sides: &[Side],
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Vec<Side>, GeomError> {
    let edit = VertexEdit { points, base: None, topology: None };
    rebuild_solid(&edit, template_sides, VertexEditOptions::default(), next_side_id)
}

fn same_vbsp_plane(a: &Plane, b: &Plane) -> bool {
    a.approx_eq(b, 1e-5, PLANE_EPS)
}

struct NewFace {
    plane: Plane,
    plane_points: [DVec3; 3],
    template: Option<usize>,
    rank: (f64, f64),
}

fn hull_faces(points: &[DVec3], templates: &[(usize, Plane)]) -> Result<Vec<NewFace>, GeomError> {
    let hull = convex_hull(points)?;
    let mut out = Vec::with_capacity(hull.faces.len());
    for face in &hull.faces {
        let n = face.plane.normal;
        let mut best: Option<(usize, f64, f64)> = None;
        for &(ti, tp) in templates {
            let dot = tp.normal.dot(n);
            let dd = (tp.dist - face.plane.dist).abs();
            let better = match best {
                None => true,
                Some((_, bd, bdd)) => dot > bd + 1e-9 || ((dot - bd).abs() <= 1e-9 && dd < bdd),
            };
            if better {
                best = Some((ti, dot, dd));
            }
        }
        out.push(NewFace {
            plane: face.plane,
            plane_points: hull.face_plane_points(face),
            template: best.map(|b| b.0),
            rank: best.map_or((f64::NEG_INFINITY, 0.0), |b| (b.1, -b.2)),
        });
    }
    Ok(out)
}

fn topology_faces(points: &[DVec3], topo: &FaceTopology<'_>) -> Result<Vec<NewFace>, GeomError> {
    if topo.faces.len() != topo.templates.len() {
        return Err(GeomError::InvalidInput("face topology and templates differ in length".to_owned()));
    }
    let mut welder = Welder::new(WELD_EPS, points.len());
    let canon: Vec<usize> = points.iter().map(|&p| welder.insert(p)).collect();
    let mut faces: Vec<(NewFace, f64)> = Vec::with_capacity(topo.faces.len());
    for (ring, &template) in topo.faces.iter().zip(topo.templates) {
        let mut ids: Vec<usize> = Vec::with_capacity(ring.len());
        for &i in ring {
            let c = *canon.get(i).ok_or_else(|| GeomError::InvalidInput("face vertex out of range".to_owned()))?;
            if ids.last() != Some(&c) {
                ids.push(c);
            }
        }
        while ids.len() > 1 && ids.first() == ids.last() {
            ids.pop();
        }
        if ids.len() < 3 {
            continue;
        }
        let poly: Vec<DVec3> = ids.iter().filter_map(|&i| welder.points.get(i).copied()).collect();
        let area_vec = crate::util::polygon_area_vector(&poly);
        let area = area_vec.length();
        if !area.is_finite() || area <= 1e-6 {
            continue;
        }
        let normal = area_vec / area;
        let dist = poly.iter().map(|p| normal.dot(*p)).sum::<f64>() / poly.len() as f64;
        let plane = Plane { normal, dist };
        if poly.iter().any(|p| plane.distance(*p).abs() > PLANE_EPS) {
            return Err(GeomError::InvalidInput("a face is not flat".to_owned()));
        }
        let plane_points = plane_points_from_polygon(&poly)
            .map(|pts| pts.map(clean_point))
            .filter(|pts| {
                Plane::from_points(pts[0], pts[1], pts[2]).is_some_and(|p| p.approx_eq(&plane, 1e-4, PLANE_EPS))
            })
            .unwrap_or_else(|| plane.to_points());
        let face = NewFace { plane, plane_points, template: Some(template), rank: (area, 0.0) };
        match faces.iter_mut().find(|(f, _)| same_vbsp_plane(&f.plane, &face.plane)) {
            Some((group, total)) => {
                *total += area;
                if area > group.rank.0 {
                    *group = face;
                }
            }
            None => faces.push((face, area)),
        }
    }
    Ok(faces
        .into_iter()
        .map(|(mut f, total)| {
            f.rank = (total, 0.0);
            f
        })
        .collect())
}

fn point_index(points: &[DVec3], base: &[DVec3], p: DVec3) -> Option<usize> {
    let eps2 = (WELD_EPS * 4.0) * (WELD_EPS * 4.0);
    let mut found: Option<usize> = None;
    for (i, q) in points.iter().enumerate() {
        if q.distance_squared(p) > eps2 {
            continue;
        }
        if base.get(i).is_some_and(|b| b.distance_squared(*q) <= 1e-18) {
            return Some(i);
        }
        found.get_or_insert(i);
    }
    found
}

fn uv_lock_triangle(pairs: &[(DVec3, DVec3)]) -> Option<([DVec3; 3], [DVec3; 3])> {
    const MAX_CANDIDATES: usize = 24;
    let step = pairs.len().div_ceil(MAX_CANDIDATES).max(1);
    let cand: Vec<(DVec3, DVec3)> = pairs.iter().step_by(step).copied().collect();
    let mut best: Option<(f64, [usize; 3])> = None;
    for i in 0..cand.len() {
        for j in i + 1..cand.len() {
            for k in j + 1..cand.len() {
                let (Some(a), Some(b), Some(c)) = (cand.get(i), cand.get(j), cand.get(k)) else { continue };
                let new_area = (b.0 - a.0).cross(c.0 - a.0).length();
                let old_area = (b.1 - a.1).cross(c.1 - a.1).length();
                let score = new_area.min(old_area);
                if score.is_finite() && best.is_none_or(|(s, _)| score > s) {
                    best = Some((score, [i, j, k]));
                }
            }
        }
    }
    let (score, [i, j, k]) = best?;
    if score <= 1e-6 {
        return None;
    }
    let (a, b, c) = (*cand.get(i)?, *cand.get(j)?, *cand.get(k)?);
    Some(([a.1, b.1, c.1], [a.0, b.0, c.0]))
}

fn kept_disp(
    t: &Side,
    same_plane: bool,
    poly: &[DVec3],
    old_poly: Option<&[DVec3]>,
    old_of: &dyn Fn(DVec3) -> Option<DVec3>,
) -> Option<Box<hp_vmf::DispInfo>> {
    let d = t.disp.as_ref()?;
    if poly.len() != 4 {
        return None;
    }
    let Some(old) = old_poly.filter(|o| o.len() == 4) else {
        return (same_plane && poly.iter().any(|v| v.distance(d.start_position) < PLANE_EPS)).then(|| d.clone());
    };
    let near = |a: DVec3, b: DVec3| a.distance_squared(b) <= (WELD_EPS * 4.0) * (WELD_EPS * 4.0);
    let map: Vec<Option<usize>> =
        poly.iter().map(|p| old_of(*p).and_then(|o| old.iter().position(|q| near(*q, o)))).collect();
    let [Some(k0), Some(k1), Some(k2), Some(k3)] = map[..] else { return None };
    if [k1, k2, k3].iter().enumerate().any(|(n, &k)| k != (k0 + n + 1) % 4) {
        return None;
    }
    let start = (0..4).min_by(|&a, &b| {
        let da = old.get(a).map_or(f64::INFINITY, |p| p.distance_squared(d.start_position));
        let db = old.get(b).map_or(f64::INFINITY, |p| p.distance_squared(d.start_position));
        da.total_cmp(&db)
    })?;
    let new_start = map.iter().position(|m| *m == Some(start)).and_then(|i| poly.get(i).copied())?;
    let unmoved = poly.iter().zip(&map).all(|(p, m)| m.and_then(|k| old.get(k)).is_some_and(|o| near(*o, *p)));
    if unmoved {
        return Some(d.clone());
    }
    let mut kept = d.clone();
    kept.start_position = new_start;
    crate::disp::update_disp_tags(&mut kept, poly);
    Some(kept)
}

pub fn rebuild_solid(
    edit: &VertexEdit<'_>,
    template_sides: &[Side],
    opts: VertexEditOptions,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Vec<Side>, GeomError> {
    let points = edit.points;
    if points.iter().any(|p| !p.is_finite()) {
        return Err(GeomError::NotFinite);
    }
    let base = edit.base.filter(|b| b.len() == points.len() && b.iter().all(|p| p.is_finite()));
    let templates: Vec<(usize, Plane)> =
        template_sides.iter().enumerate().filter_map(|(i, s)| Plane::from_side(s).map(|p| (i, p))).collect();
    let faces = match &edit.topology {
        Some(topo) => topology_faces(points, topo)?,
        None => hull_faces(points, &templates)?,
    };
    if faces.len() < 4 {
        return Err(GeomError::TooFewFaces(faces.len()));
    }
    let mut owner: FxHashMap<usize, (usize, (f64, f64))> = fx_map(16);
    for (fi, f) in faces.iter().enumerate() {
        let Some(ti) = f.template else { continue };
        let replace = match owner.get(&ti) {
            None => true,
            Some(&(_, (r0, r1))) => f.rank.0 > r0 + 1e-9 || ((f.rank.0 - r0).abs() <= 1e-9 && f.rank.1 > r1),
        };
        if replace {
            owner.insert(ti, (fi, f.rank));
        }
    }
    let needs_old = base.is_some() && (opts.uv_lock || template_sides.iter().any(|s| s.disp.is_some()));
    let old_geom = needs_old.then(|| build_solid(template_sides));
    let old_of = |p: DVec3| -> Option<DVec3> {
        let b = base?;
        point_index(points, b, p).and_then(|i| b.get(i).copied())
    };
    struct Pending {
        template: Option<usize>,
        keeps_id: bool,
        unchanged_plane: bool,
        same_plane: bool,
    }
    let mut sides: Vec<(Side, Pending)> = Vec::with_capacity(faces.len());
    for (fi, face) in faces.iter().enumerate() {
        let template = face.template.and_then(|ti| template_sides.get(ti).map(|s| (ti, s)));
        let entry = match template {
            Some((ti, t)) => {
                let keeps_id = owner.get(&ti).is_some_and(|&(f, _)| f == fi);
                let mut s = t.clone();
                s.id = if keeps_id { t.id } else { next_side_id() };
                let tp = Plane::from_side(t);
                let unchanged_plane = tp.is_some_and(|tp| tp.approx_eq(&face.plane, 1e-9, 1e-6));
                if !unchanged_plane {
                    s.plane = face.plane_points;
                }
                s.vertices_plus = None;
                s.disp = None;
                let same_plane = tp.is_some_and(|tp| tp.approx_eq(&face.plane, NORMAL_EPS * 100.0, PLANE_EPS));
                (s, Pending { template: Some(ti), keeps_id, unchanged_plane, same_plane })
            }
            None => {
                let (u, v) = world_axes(face.plane.normal);
                let s = Side {
                    id: next_side_id(),
                    plane: face.plane_points,
                    uaxis: TexAxis { axis: u, shift: 0.0, scale: crate::DEFAULT_TEXTURE_SCALE },
                    vaxis: TexAxis { axis: v, shift: 0.0, scale: crate::DEFAULT_TEXTURE_SCALE },
                    ..Default::default()
                };
                (s, Pending { template: None, keeps_id: false, unchanged_plane: false, same_plane: false })
            }
        };
        sides.push(entry);
    }
    let order = |s: &Side| template_sides.iter().position(|t| t.id == s.id).unwrap_or(usize::MAX);
    sides.sort_by_key(|(s, _)| order(s));
    let (mut sides, pending): (Vec<Side>, Vec<Pending>) = sides.into_iter().unzip();
    let geom = build_solid(&sides);
    if !geom.valid {
        crate::brush::validate_solid(&sides)?;
        return Err(GeomError::NoVolume);
    }
    for (i, (s, p)) in sides.iter_mut().zip(&pending).enumerate() {
        let Some(ti) = p.template else { continue };
        let Some(t) = template_sides.get(ti) else { continue };
        let Some(face) = geom.face_for_side(i) else { continue };
        let poly = face.vertices.as_slice();
        let normal = face.plane.normal;
        let old_poly = old_geom.as_ref().and_then(|g| g.face_for_side(ti)).map(|f| f.vertices.as_slice());
        if p.keeps_id {
            s.disp = kept_disp(t, p.same_plane, poly, old_poly, &old_of);
        }
        let mut locked = false;
        if opts.uv_lock && base.is_some() {
            let pairs: Vec<(DVec3, DVec3)> = poly.iter().filter_map(|q| old_of(*q).map(|o| (*q, o))).collect();
            let moved = !p.unchanged_plane || pairs.iter().any(|(n, o)| n.distance_squared(*o) > 1e-18);
            if moved && let Some((old3, new3)) = uv_lock_triangle(&pairs) {
                locked = crate::texture::uv_lock(old3, new3, s);
            }
        }
        if !locked
            && (axes_quality(s, normal) < MIN_KEPT_AXES_QUALITY
                || !s.uaxis.axis.is_finite()
                || !s.vaxis.axis.is_finite())
        {
            let (u, v) = face_axes(normal);
            s.uaxis.axis = u;
            s.vaxis.axis = v;
            s.uaxis.scale = effective_scale(s.uaxis.scale);
            s.vaxis.scale = effective_scale(s.vaxis.scale);
            s.rotation = 0.0;
        }
    }
    Ok(sides)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    fn check_hull(points: &[DVec3], hull: &ConvexHull) {
        for f in &hull.faces {
            assert!(f.indices.len() >= 3);
            let poly = hull.face_polygon(f);
            let n = crate::polygon_normal(&poly).expect("normal");
            assert!(n.dot(f.plane.normal) > 0.999, "face polygon must be CCW from outside");
            for &p in points {
                assert!(f.plane.distance(p) <= PLANE_EPS, "point outside hull: {}", f.plane.distance(p));
            }
        }
    }

    #[test]
    fn cube_hull_merges_quads() {
        let mut pts: Vec<DVec3> = crate::Aabb::new(DVec3::ZERO, DVec3::splat(64.0)).corners().to_vec();
        pts.push(DVec3::splat(32.0));
        pts.push(DVec3::new(32.0, 0.0, 0.0));
        pts.push(DVec3::new(32.0, 32.0, 64.0));
        let h = convex_hull(&pts).expect("hull");
        assert_eq!(h.faces.len(), 6);
        assert_eq!(h.points.len(), 8);
        for f in &h.faces {
            assert_eq!(f.indices.len(), 4);
        }
        assert!((h.volume() - 64.0f64.powi(3)).abs() < 1e-6);
        check_hull(&pts, &h);
    }

    #[test]
    fn random_clouds() {
        let mut rng = Rng(0xdead_beef);
        for round in 0..60 {
            let count = 4 + round * 7;
            let pts: Vec<DVec3> = (0..count).map(|_| rng.point(if round % 2 == 0 { 512.0 } else { 16000.0 })).collect();
            let h = convex_hull(&pts).expect("hull");
            check_hull(&pts, &h);
            let e: usize = h.faces.iter().map(|f| f.indices.len()).sum::<usize>() / 2;
            assert_eq!(h.points.len() + h.faces.len(), e + 2, "Euler failed in round {round}");
            let sphere: Vec<DVec3> = (0..count).map(|_| rng.point(1.0).normalize_or(DVec3::X) * 300.0).collect();
            let hs = convex_hull(&sphere).expect("hull");
            check_hull(&sphere, &hs);
        }
    }

    #[test]
    fn degenerate_inputs() {
        assert_eq!(convex_hull(&[DVec3::ZERO; 3]), Err(GeomError::TooFewPoints(1)));
        let line: Vec<DVec3> = (0..10).map(|i| DVec3::X * i as f64).collect();
        assert_eq!(convex_hull(&line), Err(GeomError::DegenerateHull));
        let plane: Vec<DVec3> = (0..10).map(|i| DVec3::new(i as f64, (i * i) as f64, 5.0)).collect();
        assert_eq!(convex_hull(&plane), Err(GeomError::DegenerateHull));
        let nan = [DVec3::NAN, DVec3::ZERO, DVec3::X, DVec3::Y];
        assert!(convex_hull(&nan).is_err());
    }

    #[test]
    fn rebuild_after_vertex_move() {
        let sides = hammer_box(DVec3::ZERO, DVec3::new(128.0, 64.0, 64.0));
        let g = build_solid(&sides);
        let (mut verts, faces) = unique_vertices(&g);
        assert_eq!(verts.len(), 8);
        assert!(faces.iter().all(|f| f.len() == 4));
        assert!(is_convex(&verts, &faces));
        let top = verts.iter().position(|v| *v == DVec3::new(128.0, 64.0, 64.0)).expect("corner");
        verts[top].z = 96.0;
        let mut next = id_counter(100);
        let new_sides = rebuild_from_points(&verts, &sides, &mut next).expect("rebuild");
        let ng = build_solid(&new_sides);
        assert!(ng.valid);
        assert_eq!(ng.faces.len(), 7);
        let mut ids: Vec<u32> = new_sides.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), new_sides.len());
        let bottom = new_sides.iter().find(|s| s.id == sides[1].id).expect("bottom");
        assert_eq!(bottom.material, sides[1].material);
        assert_eq!(bottom.uaxis, sides[1].uaxis);
        let same = rebuild_from_points(&g.vertices().collect::<Vec<_>>(), &sides, &mut next).expect("rebuild");
        assert_eq!(same.len(), 6);
        for s in &same {
            let orig = sides.iter().find(|o| o.id == s.id).expect("same id");
            let (a, b) = (Plane::from_side(s).expect("p"), Plane::from_side(orig).expect("p"));
            assert!(a.approx_eq(&b, 1e-9, 1e-9));
            assert_eq!(s.material, orig.material);
        }
        let mut inside = g.vertices().collect::<Vec<_>>();
        inside.push(DVec3::splat(10.0));
        assert_eq!(rebuild_from_points(&inside, &sides, &mut next).expect("rebuild").len(), 6);
    }

    #[test]
    fn convexity_and_split() {
        let sides = hammer_box(DVec3::ZERO, DVec3::splat(32.0));
        let g = build_solid(&sides);
        let (mut verts, faces) = unique_vertices(&g);
        assert!(is_convex(&verts, &faces));
        let mid = split_edge(&mut verts, faces[0][0], faces[0][1]).expect("split");
        assert_eq!(verts.len(), 9);
        assert_eq!(verts[mid], (verts[faces[0][0]] + verts[faces[0][1]]) * 0.5);
        assert!(split_edge(&mut verts, 0, 99).is_none());
        let mut dented = verts.clone();
        dented.truncate(8);
        dented[0] = dented[0] * 0.5 + DVec3::splat(8.0);
        assert!(!is_convex(&dented, &faces));
    }

    fn split_box() -> (Vec<Side>, Vec<DVec3>, Vec<Vec<usize>>, Vec<usize>) {
        let sides = hammer_box(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
        let g = build_solid(&sides);
        let (verts, mut faces) = unique_vertices(&g);
        let mut templates: Vec<usize> = g.faces.iter().map(|f| f.side_index).collect();
        let top = templates.iter().position(|&t| t == 0).expect("top face");
        let ring = faces[top].clone();
        let a = ring.iter().position(|&i| verts[i] == DVec3::new(0.0, 0.0, 64.0)).expect("corner a");
        let b = ring.iter().position(|&i| verts[i] == DVec3::new(128.0, 128.0, 64.0)).expect("corner b");
        let (lo, hi) = (a.min(b), a.max(b));
        let first: Vec<usize> = ring[lo..=hi].to_vec();
        let second: Vec<usize> = ring[hi..].iter().chain(&ring[..=lo]).copied().collect();
        faces[top] = first;
        faces.push(second);
        templates.push(0);
        (sides, verts, faces, templates)
    }

    #[test]
    fn split_faces_keep_their_planes_and_coplanar_halves_merge() {
        let (sides, mut verts, faces, templates) = split_box();
        let mut next = id_counter(100);
        let topo = FaceTopology { faces: &faces, templates: &templates };
        let edit = VertexEdit { points: &verts, base: None, topology: Some(topo) };
        let same = rebuild_solid(&edit, &sides, VertexEditOptions::default(), &mut next).expect("rebuild");
        assert_eq!(same.len(), 6, "no duplicate plane");
        assert!(same.iter().any(|s| s.id == sides[0].id && s.plane == sides[0].plane));
        let base = verts.clone();
        let k = verts.iter().position(|v| *v == DVec3::new(128.0, 0.0, 64.0)).expect("corner");
        verts[k].z = 32.0;
        let edit = VertexEdit { points: &verts, base: Some(&base), topology: Some(topo) };
        let split = rebuild_solid(&edit, &sides, VertexEditOptions::default(), &mut next).expect("rebuild");
        assert_eq!(split.len(), 7);
        let g = build_solid(&split);
        assert!(g.valid);
        let diagonal = [DVec3::new(0.0, 0.0, 64.0), DVec3::new(128.0, 128.0, 64.0)];
        let has_diagonal = g
            .edges()
            .iter()
            .any(|[a, b]| (*a == diagonal[0] && *b == diagonal[1]) || (*a == diagonal[1] && *b == diagonal[0]));
        assert!(has_diagonal, "the split edge is kept");
        assert_eq!(split.iter().filter(|s| s.material == sides[0].material).count(), 2);
        assert_eq!(split.iter().filter(|s| s.id == sides[0].id).count(), 1);
        let mut bent = verts.clone();
        let j = bent.iter().position(|v| *v == DVec3::new(0.0, 128.0, 0.0)).expect("corner");
        bent[j] += DVec3::new(0.0, 0.0, 7.0);
        let edit = VertexEdit { points: &bent, base: None, topology: Some(topo) };
        assert!(rebuild_solid(&edit, &sides, VertexEditOptions::default(), &mut next).is_err());
        let (mut dsides, dverts, dfaces, dtemplates) = split_box();
        let top = build_solid(&dsides).face_for_side(0).expect("top").vertices.clone();
        dsides[0].disp = Some(Box::new(crate::create_disp(&top, 2).expect("disp")));
        let lowered: Vec<DVec3> = dverts.iter().map(|p| if p.z < 1.0 { *p - DVec3::Z * 16.0 } else { *p }).collect();
        let dtopo = FaceTopology { faces: &dfaces, templates: &dtemplates };
        let edit = VertexEdit { points: &lowered, base: Some(&dverts), topology: Some(dtopo) };
        let out = rebuild_solid(&edit, &dsides, VertexEditOptions::default(), &mut next).expect("rebuild");
        assert_eq!(out.len(), 6);
        assert!(out[0].disp.is_some(), "kept on the merged top, at the same side index");
        let short = FaceTopology { faces: &faces, templates: &templates[..2] };
        let edit = VertexEdit { points: &verts, base: None, topology: Some(short) };
        assert!(rebuild_solid(&edit, &sides, VertexEditOptions::default(), &mut next).is_err());
        let bad_ring = vec![vec![0, 1, 99]; 6];
        let bad = FaceTopology { faces: &bad_ring, templates: &[0, 1, 2, 3, 4, 5] };
        let edit = VertexEdit { points: &verts, base: None, topology: Some(bad) };
        assert!(rebuild_solid(&edit, &sides, VertexEditOptions::default(), &mut next).is_err());
    }

    #[test]
    fn uv_lock_follows_moved_vertices() {
        let sides = hammer_box(DVec3::ZERO, DVec3::new(128.0, 64.0, 64.0));
        let g = build_solid(&sides);
        let (base, _) = unique_vertices(&g);
        let pts: Vec<DVec3> =
            base.iter().map(|p| if p.x > 64.0 { *p + DVec3::new(64.0, 0.0, 0.0) } else { *p }).collect();
        let mut next = id_counter(100);
        let edit = VertexEdit { points: &pts, base: Some(&base), topology: None };
        let locked = rebuild_solid(&edit, &sides, VertexEditOptions { uv_lock: true }, &mut next).expect("rebuild");
        let free = rebuild_solid(&edit, &sides, VertexEditOptions::default(), &mut next).expect("rebuild");
        let top = |s: &[Side]| s.iter().find(|x| x.id == sides[0].id).cloned().expect("top");
        assert_eq!(top(&free).uaxis, sides[0].uaxis);
        let t = top(&locked);
        for (old, new) in [(DVec3::new(128.0, 0.0, 64.0), DVec3::new(192.0, 0.0, 64.0)), (DVec3::ZERO, DVec3::ZERO)] {
            let want = crate::texel_uv(&sides[0], DVec3::new(old.x, old.y, 64.0));
            let got = crate::texel_uv(&t, DVec3::new(new.x, new.y, 64.0));
            assert!((want - got).length() < 1e-6, "{want} vs {got}");
        }
        assert!((t.uaxis.scale - 0.375).abs() < 1e-9, "stretched by 1.5: {}", t.uaxis.scale);
        let left = locked.iter().find(|x| x.id == sides[5].id).expect("-X wall");
        assert_eq!((left.uaxis, left.vaxis), (sides[5].uaxis, sides[5].vaxis));
    }

    #[test]
    fn displacements_follow_their_moved_quad() {
        let mut sides = hammer_box(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
        let g = build_solid(&sides);
        let top = g.face_for_side(0).expect("top").vertices.clone();
        let mut disp = crate::create_disp(&top, 2).expect("disp");
        disp.distances[12] = 16.0;
        sides[0].disp = Some(Box::new(disp.clone()));
        let (base, _) = unique_vertices(&g);
        let pts: Vec<DVec3> = base.iter().map(|p| if p.z > 32.0 { *p + DVec3::Z * 32.0 } else { *p }).collect();
        let mut next = id_counter(100);
        let edit = VertexEdit { points: &pts, base: Some(&base), topology: None };
        let out = rebuild_solid(&edit, &sides, VertexEditOptions::default(), &mut next).expect("rebuild");
        let kept = out.iter().find(|s| s.id == sides[0].id).and_then(|s| s.disp.as_ref()).expect("kept");
        assert_eq!(kept.start_position, disp.start_position + DVec3::Z * 32.0);
        assert_eq!(kept.distances, disp.distances);
        let out = rebuild_from_points(&pts, &sides, &mut next).expect("rebuild");
        assert!(out.iter().all(|s| s.disp.is_none()));
        let tilt: Vec<DVec3> =
            base.iter().map(|p| if p.z > 32.0 && p.x > 64.0 { *p + DVec3::Z * 32.0 } else { *p }).collect();
        let edit = VertexEdit { points: &tilt, base: Some(&base), topology: None };
        let out = rebuild_solid(&edit, &sides, VertexEditOptions::default(), &mut next).expect("rebuild");
        assert!(out.iter().any(|s| s.disp.is_some()));
        let corner: Vec<DVec3> =
            base.iter().map(|p| if *p == DVec3::new(128.0, 128.0, 64.0) { *p + DVec3::Z * 32.0 } else { *p }).collect();
        let edit = VertexEdit { points: &corner, base: Some(&base), topology: None };
        let out = rebuild_solid(&edit, &sides, VertexEditOptions::default(), &mut next).expect("rebuild");
        assert!(out.iter().all(|s| s.disp.is_none()), "the top is two triangles now");
    }
}
