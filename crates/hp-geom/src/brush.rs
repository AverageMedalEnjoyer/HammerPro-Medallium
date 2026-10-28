use std::cell::Cell;

use glam::DVec3;
use hp_vmf::{Side, Solid};
use rayon::prelude::*;

use crate::util::{clean_vec, dedupe_ring, polygon_area_vector, polygon_center};
use crate::{Aabb, CLIP_EPS, GeomError, MAX_COORD, NORMAL_EPS, Plane, WELD_EPS};

pub const BASE_POLYGON_HALF_SIZE: f64 = 1_048_576.0;

const UNBOUNDED_LIMIT: f64 = 262_144.0;

const MIN_FACE_AREA: f64 = 1e-6;

const MIN_SOLID_VOLUME: f64 = 1e-6;

const INVALID_LIMIT: f64 = 2.0 * MAX_COORD;

#[derive(Clone, Debug, PartialEq)]
pub struct FaceGeom {
    pub side_index: usize,
    pub plane: Plane,
    pub vertices: Vec<DVec3>,
}

impl FaceGeom {
    #[inline]
    pub fn normal(&self) -> DVec3 {
        self.plane.normal
    }

    #[inline]
    pub fn center(&self) -> DVec3 {
        polygon_center(&self.vertices)
    }

    #[inline]
    pub fn area(&self) -> f64 {
        polygon_area_vector(&self.vertices).length()
    }

    #[inline]
    pub fn triangles(&self) -> impl Iterator<Item = [usize; 3]> + use<> {
        triangulate_fan(self.vertices.len())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SolidGeom {
    pub faces: Vec<FaceGeom>,
    pub aabb: Aabb,
    pub valid: bool,
}

impl Default for SolidGeom {
    fn default() -> Self {
        Self { faces: Vec::new(), aabb: Aabb::EMPTY, valid: false }
    }
}

impl SolidGeom {
    pub fn face_for_side(&self, side_index: usize) -> Option<&FaceGeom> {
        self.faces.iter().find(|f| f.side_index == side_index)
    }

    #[inline]
    pub fn center(&self) -> DVec3 {
        self.aabb.center()
    }

    pub fn face_vertex_count(&self) -> usize {
        self.faces.iter().map(|f| f.vertices.len()).sum()
    }

    pub fn vertices(&self) -> impl Iterator<Item = DVec3> + '_ {
        self.faces.iter().flat_map(|f| f.vertices.iter().copied())
    }

    pub fn volume(&self) -> f64 {
        faces_volume(&self.faces, self.aabb.center())
    }

    pub fn surface_area(&self) -> f64 {
        self.faces.iter().map(FaceGeom::area).sum()
    }

    pub fn edges(&self) -> Vec<[DVec3; 2]> {
        let mut out = Vec::with_capacity(self.face_vertex_count() / 2 + 4);
        for f in &self.faces {
            let n = f.vertices.len();
            for (i, &a) in f.vertices.iter().enumerate() {
                let Some(&b) = f.vertices.get(if i + 1 == n { 0 } else { i + 1 }) else {
                    continue;
                };
                if !self.valid || lex_less(a, b) {
                    out.push([a, b]);
                }
            }
        }
        out
    }
}

#[inline]
fn lex_less(a: DVec3, b: DVec3) -> bool {
    for k in 0..3 {
        let (x, y) = (a[k], b[k]);
        if x < y - WELD_EPS {
            return true;
        }
        if x > y + WELD_EPS {
            return false;
        }
    }
    false
}

#[inline]
pub fn triangulate_fan(vertex_count: usize) -> impl Iterator<Item = [usize; 3]> {
    (1..vertex_count.saturating_sub(1)).map(|i| [0, i, i + 1])
}

pub fn triangulate_fan_into(base: u32, vertex_count: usize, out: &mut Vec<u32>) {
    let n = u32::try_from(vertex_count).unwrap_or(u32::MAX);
    for i in 1..n.saturating_sub(1) {
        out.extend_from_slice(&[base, base.saturating_add(i), base.saturating_add(i + 1)]);
    }
}

pub fn base_polygon(plane: &Plane, half_size: f64) -> [DVec3; 4] {
    let n = plane.normal;
    let a = n.abs();
    let up = if a.z >= a.x && a.z >= a.y { DVec3::X } else { DVec3::Z };
    let u = (up - n * n.dot(up)).try_normalize().unwrap_or(DVec3::X);
    let v = n.cross(u);
    let o = n * plane.dist;
    let (u, v) = (u * half_size, v * half_size);
    [o - u - v, o + u - v, o + u + v, o - u + v]
}

enum ClipOutcome {
    Unchanged,
    Empty,
    Clipped,
}

fn clip_back(input: &[DVec3], plane: &Plane, eps: f64, out: &mut Vec<DVec3>, dists: &mut Vec<f64>) -> ClipOutcome {
    dists.clear();
    let mut front = 0usize;
    let mut back = 0usize;
    for &p in input {
        let d = plane.distance(p);
        if d > eps {
            front += 1;
        } else if d < -eps {
            back += 1;
        }
        dists.push(d);
    }
    if front == 0 {
        return ClipOutcome::Unchanged;
    }
    if back == 0 {
        return ClipOutcome::Empty;
    }
    out.clear();
    let n = input.len();
    for (i, (&p1, &d1)) in input.iter().zip(dists.iter()).enumerate() {
        if d1 <= eps {
            out.push(p1);
        }
        let j = if i + 1 == n { 0 } else { i + 1 };
        let (Some(&p2), Some(&d2)) = (input.get(j), dists.get(j)) else {
            continue;
        };
        if (d1 > eps && d2 < -eps) || (d1 < -eps && d2 > eps) {
            out.push(split_point(p1, p2, d1, d2, plane));
        }
    }
    ClipOutcome::Clipped
}

#[inline]
fn split_point(p1: DVec3, p2: DVec3, d1: f64, d2: f64, plane: &Plane) -> DVec3 {
    let t = d1 / (d1 - d2);
    let mut mid = p1 + (p2 - p1) * t;
    for k in 0..3 {
        let c = plane.normal[k];
        if c == 1.0 {
            mid[k] = plane.dist;
        } else if c == -1.0 {
            mid[k] = -plane.dist;
        }
    }
    mid
}

pub fn clip_polygon(poly: &[DVec3], plane: &Plane, eps: f64) -> (Vec<DVec3>, Vec<DVec3>) {
    let dists: Vec<f64> = poly.iter().map(|&p| plane.distance(p)).collect();
    let has_front = dists.iter().any(|&d| d > eps);
    let has_back = dists.iter().any(|&d| d < -eps);
    match (has_front, has_back) {
        (false, false) => (Vec::new(), Vec::new()),
        (true, false) => (poly.to_vec(), Vec::new()),
        (false, true) => (Vec::new(), poly.to_vec()),
        (true, true) => {
            let mut front = Vec::with_capacity(poly.len() + 1);
            let mut back = Vec::with_capacity(poly.len() + 1);
            let n = poly.len();
            for (i, (&p1, &d1)) in poly.iter().zip(dists.iter()).enumerate() {
                if d1 >= -eps {
                    front.push(p1);
                }
                if d1 <= eps {
                    back.push(p1);
                }
                let j = if i + 1 == n { 0 } else { i + 1 };
                let (Some(&p2), Some(&d2)) = (poly.get(j), dists.get(j)) else {
                    continue;
                };
                if (d1 > eps && d2 < -eps) || (d1 < -eps && d2 > eps) {
                    let m = split_point(p1, p2, d1, d2, plane);
                    front.push(m);
                    back.push(m);
                }
            }
            (front, back)
        }
    }
}

#[derive(Default)]
struct Scratch {
    planes: Vec<Option<Plane>>,
    cur: Vec<DVec3>,
    tmp: Vec<DVec3>,
    dists: Vec<f64>,
}

thread_local! {
    static SCRATCH: Cell<Scratch> = Cell::new(Scratch::default());
}

fn with_scratch<R>(f: impl FnOnce(&mut Scratch) -> R) -> R {
    let mut scratch = SCRATCH.try_with(Cell::take).unwrap_or_default();
    let r = f(&mut scratch);
    let _ = SCRATCH.try_with(|cell| cell.set(scratch));
    r
}

pub fn build_solid(sides: &[Side]) -> SolidGeom {
    build_checked(sides).0
}

fn build_checked(sides: &[Side]) -> (SolidGeom, Result<(), GeomError>) {
    with_scratch(|s| {
        let Scratch { planes, cur, tmp, dists } = s;
        planes.clear();
        planes.extend(sides.iter().map(Plane::from_side));
        let mut hint = Aabb::EMPTY;
        for (side, plane) in sides.iter().zip(planes.iter()) {
            if plane.is_some() {
                for p in side.plane {
                    if p.is_finite() {
                        hint.extend(p);
                    }
                }
            }
        }
        build_core(planes, cur, tmp, dists, hint)
    })
}

pub fn build_solid_from_planes(planes: &[Plane]) -> SolidGeom {
    with_scratch(|s| {
        let Scratch { planes: p, cur, tmp, dists } = s;
        p.clear();
        p.extend(planes.iter().map(|pl| {
            (pl.normal.is_finite() && pl.dist.is_finite() && pl.normal.length_squared() > 0.5).then_some(*pl)
        }));
        build_core(p, cur, tmp, dists, Aabb::EMPTY).0
    })
}

pub fn build_solids(solids: &[Solid]) -> Vec<SolidGeom> {
    solids.par_iter().map(|s| build_solid(&s.sides)).collect()
}

pub fn validate_solid(sides: &[Side]) -> Result<SolidGeom, GeomError> {
    if sides.iter().any(|s| Plane::from_side(s).is_none()) {
        return Err(GeomError::DegeneratePlane);
    }
    let (geom, check) = build_checked(sides);
    check?;
    Ok(geom)
}

fn build_core(
    planes: &[Option<Plane>],
    cur: &mut Vec<DVec3>,
    tmp: &mut Vec<DVec3>,
    dists: &mut Vec<f64>,
    hint: Aabb,
) -> (SolidGeom, Result<(), GeomError>) {
    let mut faces = Vec::with_capacity(planes.len());
    let mut planes_ok = true;
    'faces: for (i, plane) in planes.iter().enumerate() {
        let Some(plane) = plane else {
            planes_ok = false;
            continue;
        };
        if let Some(earlier) = planes.get(..i) {
            for e in earlier.iter().flatten() {
                if plane.approx_eq(e, NORMAL_EPS, CLIP_EPS) {
                    continue 'faces;
                }
            }
        }
        cur.clear();
        cur.extend_from_slice(&base_polygon(plane, BASE_POLYGON_HALF_SIZE));
        for (j, other) in planes.iter().enumerate() {
            if j == i {
                continue;
            }
            let Some(other) = other else { continue };
            match clip_back(cur, other, CLIP_EPS, tmp, dists) {
                ClipOutcome::Unchanged => {}
                ClipOutcome::Empty => {
                    cur.clear();
                    break;
                }
                ClipOutcome::Clipped => std::mem::swap(cur, tmp),
            }
        }
        if cur.len() < 3 {
            continue;
        }
        dedupe_ring(cur, WELD_EPS);
        if cur.len() < 3 {
            continue;
        }
        let vertices: Vec<DVec3> = cur.iter().map(|&v| clean_vec(v)).collect();
        if polygon_area_vector(&vertices).length() < MIN_FACE_AREA {
            continue;
        }
        faces.push(FaceGeom { side_index: i, plane: *plane, vertices });
    }
    let aabb = Aabb::from_points(faces.iter().flat_map(|f| f.vertices.iter().copied()));
    let check = if planes_ok { check_solid(&faces, &aabb) } else { Err(GeomError::DegeneratePlane) };
    if check.is_ok() {
        return (SolidGeom { faces, aabb, valid: true }, check);
    }
    let (faces, aabb) = bound_invalid_faces(faces, hint, cur, tmp, dists);
    (SolidGeom { faces, aabb, valid: false }, check)
}

fn bound_invalid_faces(
    faces: Vec<FaceGeom>,
    hint: Aabb,
    cur: &mut Vec<DVec3>,
    tmp: &mut Vec<DVec3>,
    dists: &mut Vec<f64>,
) -> (Vec<FaceGeom>, Aabb) {
    let limit = Aabb::new(DVec3::splat(-INVALID_LIMIT), DVec3::splat(INVALID_LIMIT));
    let mut region = hint;
    for v in faces.iter().flat_map(|f| f.vertices.iter()) {
        if v.is_finite() && limit.contains_point(*v) {
            region.extend(*v);
        }
    }
    let region = region.intersection(&limit);
    if region.is_empty() {
        return (Vec::new(), Aabb::EMPTY);
    }
    let walls = [
        Plane { normal: DVec3::X, dist: region.max.x },
        Plane { normal: DVec3::NEG_X, dist: -region.min.x },
        Plane { normal: DVec3::Y, dist: region.max.y },
        Plane { normal: DVec3::NEG_Y, dist: -region.min.y },
        Plane { normal: DVec3::Z, dist: region.max.z },
        Plane { normal: DVec3::NEG_Z, dist: -region.min.z },
    ];
    let mut out = Vec::with_capacity(faces.len());
    for mut face in faces {
        cur.clear();
        cur.extend(face.vertices.iter().copied().filter(|v| v.is_finite()));
        for wall in &walls {
            match clip_back(cur, wall, CLIP_EPS, tmp, dists) {
                ClipOutcome::Unchanged => {}
                ClipOutcome::Empty => {
                    cur.clear();
                    break;
                }
                ClipOutcome::Clipped => std::mem::swap(cur, tmp),
            }
        }
        dedupe_ring(cur, WELD_EPS);
        if cur.len() < 3 {
            continue;
        }
        let vertices: Vec<DVec3> = cur.iter().map(|&v| clean_vec(v)).collect();
        if polygon_area_vector(&vertices).length() < MIN_FACE_AREA {
            continue;
        }
        face.vertices = vertices;
        out.push(face);
    }
    let aabb = Aabb::from_points(out.iter().flat_map(|f| f.vertices.iter().copied()));
    let aabb = if aabb.is_empty() { region } else { aabb };
    (out, aabb)
}

fn check_solid(faces: &[FaceGeom], aabb: &Aabb) -> Result<(), GeomError> {
    if faces.len() < 4 {
        return Err(GeomError::TooFewFaces(faces.len()));
    }
    if !aabb.min.is_finite() || !aabb.max.is_finite() {
        return Err(GeomError::NotFinite);
    }
    if aabb.min.min_element() < -UNBOUNDED_LIMIT || aabb.max.max_element() > UNBOUNDED_LIMIT {
        return Err(GeomError::Unbounded);
    }
    if faces_volume(faces, aabb.center()) <= MIN_SOLID_VOLUME {
        return Err(GeomError::NoVolume);
    }
    Ok(())
}

pub(crate) fn faces_volume(faces: &[FaceGeom], reference: DVec3) -> f64 {
    let r = if reference.is_finite() { reference } else { DVec3::ZERO };
    let mut six_v = 0.0;
    for f in faces {
        let Some((&p0, rest)) = f.vertices.split_first() else { continue };
        let a = p0 - r;
        for w in rest.windows(2) {
            six_v += a.dot((w[0] - r).cross(w[1] - r));
        }
    }
    six_v / 6.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    #[test]
    fn sample_cube() {
        let sides = hammer_box(DVec3::new(-480.0, 352.0, 8.0), DVec3::new(-352.0, 480.0, 16.0));
        let top = Plane::from_side(&sides[0]).expect("top plane");
        assert_eq!(top.normal, DVec3::Z);
        let g = build_solid(&sides);
        assert!(g.valid);
        assert_eq!(g.faces.len(), 6);
        assert_eq!(g.aabb.min, DVec3::new(-480.0, 352.0, 8.0));
        assert_eq!(g.aabb.max, DVec3::new(-352.0, 480.0, 16.0));
        assert!((g.volume() - 128.0 * 128.0 * 8.0).abs() < 1e-6);
        for f in &g.faces {
            assert_eq!(f.vertices.len(), 4);
            let n = crate::polygon_normal(&f.vertices).expect("normal");
            assert!((n - f.plane.normal).length() < 1e-9, "face winding must be CCW from outside");
            for v in &f.vertices {
                assert_eq!(*v, v.round());
            }
            for v in &f.vertices {
                for o in &g.faces {
                    assert!(o.plane.distance(*v) < 1e-9);
                }
            }
        }
        assert_eq!(g.edges().len(), 12);
        assert!((g.surface_area() - 2.0 * (128.0 * 128.0 + 2.0 * 128.0 * 8.0)).abs() < 1e-6);
    }

    #[test]
    fn redundant_and_duplicate_planes() {
        let mut sides = hammer_box(DVec3::ZERO, DVec3::splat(64.0));
        sides.push(sides[0].clone());
        let mut far = sides[0].clone();
        for p in &mut far.plane {
            p.z += 100.0;
        }
        sides.push(far);
        let g = build_solid(&sides);
        assert!(g.valid);
        assert_eq!(g.faces.len(), 6);
        assert!((g.volume() - 64.0f64.powi(3)).abs() < 1e-6);
    }

    #[test]
    fn open_and_degenerate_solids() {
        let sides = hammer_box(DVec3::ZERO, DVec3::splat(64.0));
        let open: Vec<Side> = sides.iter().enumerate().filter(|(i, _)| *i != 1).map(|(_, s)| s.clone()).collect();
        let g = build_solid(&open);
        assert!(!g.valid);
        assert_eq!(validate_solid(&open).err(), Some(GeomError::Unbounded));
        let mut bad = sides.clone();
        bad[2].plane = [DVec3::ZERO, DVec3::ZERO, DVec3::X];
        assert!(!build_solid(&bad).valid);
        assert_eq!(validate_solid(&bad).err(), Some(GeomError::DegeneratePlane));
        assert!(!build_solid(&[]).valid);
        assert_eq!(validate_solid(&[]).err(), Some(GeomError::TooFewFaces(0)));
        let mut inverted = sides.clone();
        for p in &mut inverted[0].plane {
            p.z = -10.0;
        }
        assert!(!build_solid(&inverted).valid);
        let mut nan = sides;
        nan[3].plane[0].x = f64::NAN;
        assert!(!build_solid(&nan).valid);
    }

    #[test]
    fn invalid_solids_stay_bounded() {
        let sides = hammer_box(DVec3::new(1000.0, 2000.0, 0.0), DVec3::new(1128.0, 2128.0, 64.0));
        let real = Aabb::new(DVec3::new(1000.0, 2000.0, 0.0), DVec3::new(1128.0, 2128.0, 64.0));
        let within = |g: &SolidGeom| {
            !g.aabb.is_empty()
                && real.expand(0.01).contains_aabb(&g.aabb)
                && g.vertices().all(|v| real.expand(0.01).contains_point(v))
        };
        let three: Vec<Side> = sides[..3].to_vec();
        let open: Vec<Side> = sides.iter().enumerate().filter(|(i, _)| *i != 1).map(|(_, s)| s.clone()).collect();
        let mut degenerate = sides.clone();
        degenerate[3].plane = [DVec3::ZERO; 3];
        let mut nan = sides.clone();
        nan[4].plane[1].y = f64::NAN;
        let mut inf = sides.clone();
        inf[5].plane[0].x = f64::INFINITY;
        for (name, s) in [("three", &three), ("open", &open), ("degenerate", &degenerate), ("nan", &nan), ("inf", &inf)]
        {
            let g = build_solid(s);
            assert!(!g.valid, "{name}");
            assert!(within(&g), "{name}: {:?}", g.aabb);
            assert!(!g.faces.is_empty(), "{name}: something is left to draw");
        }
        assert_eq!(validate_solid(&open).err(), Some(GeomError::Unbounded));
        assert_eq!(validate_solid(&three).err(), Some(GeomError::TooFewFaces(3)));
        assert_eq!(validate_solid(&degenerate).err(), Some(GeomError::DegeneratePlane));
        let g = build_solid(&sides[..1]);
        assert!(!g.valid);
        assert!(g.aabb.size().max_element() <= 128.0 + 1e-6);
        let g = build_solid_from_planes(&[Plane::new(DVec3::Z, 0.0).expect("p")]);
        assert!(!g.valid);
        assert!(g.aabb.max.max_element() <= 2.0 * MAX_COORD + 1e-6);
    }

    #[test]
    fn off_grid_planes() {
        let mut planes = vec![
            Plane::new(DVec3::Z, 32.0).expect("p"),
            Plane::new(-DVec3::Z, 0.0).expect("p"),
            Plane::new(DVec3::X, 50.0).expect("p"),
            Plane::new(-DVec3::X, 0.0).expect("p"),
            Plane::new(DVec3::Y, 40.0).expect("p"),
            Plane::new(-DVec3::Y, 0.0).expect("p"),
        ];
        planes.push(Plane::new(DVec3::new(1.0, 1.0, 1.0), 70.123).expect("p"));
        let g = build_solid_from_planes(&planes);
        assert!(g.valid);
        assert_eq!(g.faces.len(), 7);
        for f in &g.faces {
            for v in &f.vertices {
                for o in &g.faces {
                    assert!(o.plane.distance(*v) < 1e-6);
                }
            }
        }
    }

    #[test]
    fn clip_polygon_split() {
        let sq = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(10.0, 10.0, 0.0),
            DVec3::new(0.0, 10.0, 0.0),
        ];
        let p = Plane::new(DVec3::X, 4.0).expect("p");
        let (f, b) = clip_polygon(&sq, &p, 0.01);
        assert_eq!(f.len(), 4);
        assert_eq!(b.len(), 4);
        assert!((polygon_area_vector(&f).length() - 60.0).abs() < 1e-9);
        assert!((polygon_area_vector(&b).length() - 40.0).abs() < 1e-9);
        let far = Plane::new(DVec3::X, 40.0).expect("p");
        let (f, b) = clip_polygon(&sq, &far, 0.01);
        assert!(f.is_empty());
        assert_eq!(b.len(), 4);
    }

    #[test]
    fn fan() {
        let t: Vec<_> = triangulate_fan(5).collect();
        assert_eq!(t, vec![[0, 1, 2], [0, 2, 3], [0, 3, 4]]);
        assert_eq!(triangulate_fan(2).count(), 0);
        let mut idx = Vec::new();
        triangulate_fan_into(10, 4, &mut idx);
        assert_eq!(idx, vec![10, 11, 12, 10, 12, 13]);
    }

    #[test]
    fn parallel_build_matches() {
        let solids: Vec<Solid> = (0..50)
            .map(|i| Solid {
                id: i,
                sides: hammer_box(DVec3::splat(i as f64), DVec3::splat(i as f64 + 16.0)),
                ..Default::default()
            })
            .collect();
        let par = build_solids(&solids);
        for (s, g) in solids.iter().zip(&par) {
            assert_eq!(*g, build_solid(&s.sides));
            assert!(g.valid);
        }
    }

    #[test]
    #[ignore]
    fn bench_build_100k_sides() {
        let mut rng = Rng(0x1234_5678_9abc_def1);
        let count = 100_000 / 6 + 1;
        let solids: Vec<Solid> = (0..count)
            .map(|_| {
                let min = DVec3::new(
                    rng.range(-16000.0, 16000.0),
                    rng.range(-16000.0, 16000.0),
                    rng.range(-16000.0, 16000.0),
                )
                .round();
                let size = DVec3::new(rng.range(1.0, 512.0), rng.range(1.0, 512.0), rng.range(1.0, 512.0)).round();
                Solid { sides: hammer_box(min, min + size), ..Default::default() }
            })
            .collect();
        let sides: usize = solids.iter().map(|s| s.sides.len()).sum();
        let mut single = std::time::Duration::MAX;
        let mut parallel = std::time::Duration::MAX;
        for _ in 0..5 {
            let t = std::time::Instant::now();
            let mut valid = 0usize;
            for s in &solids {
                valid += usize::from(build_solid(&s.sides).valid);
            }
            single = single.min(t.elapsed());
            assert_eq!(valid, solids.len());
            let t = std::time::Instant::now();
            let par = build_solids(&solids);
            parallel = parallel.min(t.elapsed());
            assert_eq!(par.len(), solids.len());
        }
        eprintln!(
            "build_solid: {} solids / {} sides: single thread {:.2} ms, rayon {:.2} ms",
            solids.len(),
            sides,
            single.as_secs_f64() * 1000.0,
            parallel.as_secs_f64() * 1000.0
        );
    }
}
