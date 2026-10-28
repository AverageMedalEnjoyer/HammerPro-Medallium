use std::collections::HashMap;
use std::sync::Arc;

use glam::DVec3;
use rayon::prelude::*;

use crate::{PLANE_EPS, Plane, SolidGeom, clip_polygon, polygon_area_vector};

const COVERED_AREA: f64 = 0.05;

#[derive(Clone, Debug)]
pub struct CoverageSolid {
    pub geom: Arc<SolidGeom>,
    pub candidates: Vec<bool>,
    pub covers: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct HiddenFace {
    pub solid: usize,
    pub face: usize,
    pub covered_by: Vec<usize>,
}

type PlaneKey = (i32, i32, i32, i64);

fn plane_key(p: &Plane) -> PlaneKey {
    let q = |x: f64| (x * 1000.0).round() as i32;
    (q(p.normal.x), q(p.normal.y), q(p.normal.z), (p.dist * 8.0).round() as i64)
}

fn subtract(piece: Vec<DVec3>, cover: &[DVec3], cover_normal: DVec3, out: &mut Vec<Vec<DVec3>>) {
    let mut inside = piece;
    let n = cover.len();
    for i in 0..n {
        let (a, b) = (cover[i], cover[(i + 1) % n]);
        let Some(edge) = Plane::from_normal_point((b - a).cross(cover_normal), a) else { continue };
        let (front, back) = clip_polygon(&inside, &edge, 1e-6);
        if front.len() >= 3 && polygon_area_vector(&front).length() > 1e-9 {
            out.push(front);
        }
        inside = back;
        if inside.len() < 3 {
            return;
        }
    }
}

fn uncovered_area(polygon: &[DVec3], covers: &[&[DVec3]], cover_normal: DVec3) -> f64 {
    let mut pieces = vec![polygon.to_vec()];
    for cover in covers {
        let mut next = Vec::with_capacity(pieces.len() + 4);
        for p in pieces {
            subtract(p, cover, cover_normal, &mut next);
        }
        pieces = next;
        if pieces.is_empty() {
            return 0.0;
        }
    }
    pieces.iter().map(|p| polygon_area_vector(p).length()).sum()
}

const CELL: f64 = 512.0;
const MAX_CELLS: i64 = 16;

fn cell_range(normal: DVec3, b: &crate::Aabb) -> ([i64; 2], [i64; 2]) {
    let k = normal.abs().max_position();
    let (u, v) = ((k + 1) % 3, (k + 2) % 3);
    let c = |x: f64| (x / CELL).floor().clamp(-1e9, 1e9) as i64;
    ([c(b.min[u]), c(b.min[v])], [c(b.max[u]), c(b.max[v])])
}

#[derive(Default)]
struct CoverIndex {
    entries: Vec<(usize, usize)>,
    cells: HashMap<(PlaneKey, i64, i64), Vec<u32>>,
    big: HashMap<PlaneKey, Vec<u32>>,
}

impl CoverIndex {
    fn build(solids: &[CoverageSolid]) -> Self {
        let mut index = CoverIndex::default();
        for (si, s) in solids.iter().enumerate() {
            if !s.covers || !s.geom.valid {
                continue;
            }
            for (fi, f) in s.geom.faces.iter().enumerate() {
                if f.vertices.len() < 3 {
                    continue;
                }
                let Ok(entry) = u32::try_from(index.entries.len()) else { return index };
                index.entries.push((si, fi));
                let key = plane_key(&f.plane.flip());
                let b = crate::Aabb::from_points(f.vertices.iter().copied());
                let (lo, hi) = cell_range(f.plane.normal, &b);
                if hi[0] - lo[0] >= MAX_CELLS || hi[1] - lo[1] >= MAX_CELLS {
                    index.big.entry(key).or_default().push(entry);
                    continue;
                }
                for cu in lo[0]..=hi[0] {
                    for cv in lo[1]..=hi[1] {
                        index.cells.entry((key, cu, cv)).or_default().push(entry);
                    }
                }
            }
        }
        index
    }

    fn query(&self, plane: &Plane, b: &crate::Aabb, out: &mut Vec<u32>) {
        out.clear();
        let key = plane_key(plane);
        if let Some(big) = self.big.get(&key) {
            out.extend_from_slice(big);
        }
        let (lo, hi) = cell_range(plane.normal, b);
        if (hi[0] - lo[0] + 1).saturating_mul(hi[1] - lo[1] + 1) > MAX_CELLS * MAX_CELLS * 4 {
            return;
        }
        for cu in lo[0]..=hi[0] {
            for cv in lo[1]..=hi[1] {
                if let Some(list) = self.cells.get(&(key, cu, cv)) {
                    out.extend_from_slice(list);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

fn contains(cover: &[DVec3], cover_normal: DVec3, polygon: &[DVec3]) -> bool {
    let n = cover.len();
    (0..n).all(|i| {
        let (a, b) = (cover[i], cover[(i + 1) % n]);
        let Some(edge) = (b - a).cross(cover_normal).try_normalize() else { return true };
        polygon.iter().all(|p| edge.dot(*p - a) <= 1e-6)
    })
}

pub fn hidden_faces(solids: &[CoverageSolid]) -> Vec<HiddenFace> {
    let index = CoverIndex::build(solids);
    if index.entries.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<HiddenFace> = solids
        .par_iter()
        .enumerate()
        .flat_map_iter(|(si, s)| {
            let index = &index;
            let mut found: Vec<u32> = Vec::new();
            let faces: &[crate::FaceGeom] = if s.geom.valid { &s.geom.faces } else { &[] };
            faces.iter().enumerate().filter_map(move |(fi, f)| {
                if !s.candidates.get(fi).copied().unwrap_or(false) || f.vertices.len() < 3 {
                    return None;
                }
                let bounds = crate::Aabb::from_points(f.vertices.iter().copied()).expand(PLANE_EPS);
                index.query(&f.plane, &bounds, &mut found);
                let (by, covers): (Vec<usize>, Vec<&[DVec3]>) = found
                    .iter()
                    .filter_map(|&e| index.entries.get(e as usize).copied())
                    .filter(|(ci, _)| *ci != si)
                    .filter_map(|(ci, cf)| Some((ci, solids.get(ci)?.geom.faces.get(cf)?)))
                    .filter(|(_, c)| c.plane.flip().approx_eq(&f.plane, 1e-6, PLANE_EPS))
                    .filter(|(_, c)| bounds.intersects(&crate::Aabb::from_points(c.vertices.iter().copied())))
                    .map(|(ci, c)| (ci, c.vertices.as_slice()))
                    .unzip();
                if covers.is_empty() {
                    return None;
                }
                let cover_normal = -f.plane.normal;
                let hidden = match covers.iter().position(|c| contains(c, cover_normal, &f.vertices)) {
                    Some(i) => Some(vec![by[i]]),
                    None => (uncovered_area(&f.vertices, &covers, cover_normal) < COVERED_AREA).then_some(by),
                };
                hidden.map(|mut covered_by| {
                    covered_by.sort_unstable();
                    covered_by.dedup();
                    HiddenFace { solid: si, face: fi, covered_by }
                })
            })
        })
        .collect();
    out.sort_unstable();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_solid;
    use crate::test_support::hammer_box;

    fn solid(min: DVec3, max: DVec3, covers: bool) -> CoverageSolid {
        let geom = Arc::new(build_solid(&hammer_box(min, max)));
        let n = geom.faces.len();
        CoverageSolid { geom, candidates: vec![true; n], covers }
    }

    fn normal(solids: &[CoverageSolid], h: &HiddenFace) -> DVec3 {
        solids[h.solid].geom.faces[h.face].plane.normal
    }

    #[test]
    fn a_face_flush_against_a_bigger_brush_is_hidden() {
        let solids = vec![
            solid(DVec3::new(-16.0, -128.0, 0.0), DVec3::new(0.0, 128.0, 128.0), true),
            solid(DVec3::new(0.0, -32.0, 0.0), DVec3::new(64.0, 32.0, 64.0), true),
        ];
        let hidden = hidden_faces(&solids);
        assert_eq!(hidden.len(), 1, "{hidden:?}");
        assert_eq!((hidden[0].solid, hidden[0].covered_by.as_slice()), (1, &[0][..]));
        assert_eq!(normal(&solids, &hidden[0]), DVec3::NEG_X);
    }

    #[test]
    fn partial_and_combined_coverage() {
        let solids = vec![
            solid(DVec3::new(0.0, 0.0, 0.0), DVec3::new(64.0, 64.0, 64.0), true),
            solid(DVec3::new(0.0, 0.0, 64.0), DVec3::new(32.0, 64.0, 96.0), true),
            solid(DVec3::new(32.0, 0.0, 64.0), DVec3::new(64.0, 64.0, 96.0), true),
        ];
        let hidden = hidden_faces(&solids);
        let top = hidden.iter().find(|h| h.solid == 0 && normal(&solids, h) == DVec3::Z).expect("top hidden");
        assert_eq!(top.covered_by, vec![1, 2]);
        let half = vec![
            solid(DVec3::new(0.0, 0.0, 0.0), DVec3::new(64.0, 64.0, 64.0), true),
            solid(DVec3::new(0.0, 0.0, 64.0), DVec3::new(32.0, 64.0, 96.0), true),
        ];
        let hidden = hidden_faces(&half);
        assert!(!hidden.iter().any(|h| h.solid == 0), "{hidden:?}");
        assert!(hidden.iter().any(|h| h.solid == 1 && normal(&half, h) == DVec3::NEG_Z));
    }

    #[test]
    #[ignore = "benchmark"]
    fn bench_hidden_faces() {
        let n = 300;
        let solids: Vec<CoverageSolid> = (0..n * n)
            .map(|i| {
                let (x, y) = (f64::from(i % n) * 64.0, f64::from(i / n) * 64.0);
                solid(DVec3::new(x, y, 0.0), DVec3::new(x + 64.0, y + 64.0, 64.0), true)
            })
            .collect();
        let t = std::time::Instant::now();
        let hidden = hidden_faces(&solids);
        eprintln!(
            "{} solids: {} hidden faces in {:.1} ms",
            solids.len(),
            hidden.len(),
            t.elapsed().as_secs_f64() * 1000.0
        );
        let n = n as usize;
        assert_eq!(hidden.len(), 4 * n * n - 4 * n);
    }

    #[test]
    fn non_covering_solids_and_candidates_are_respected() {
        let mut solids = vec![
            solid(DVec3::new(-16.0, -128.0, 0.0), DVec3::new(0.0, 128.0, 128.0), false),
            solid(DVec3::new(0.0, -32.0, 0.0), DVec3::new(64.0, 32.0, 64.0), true),
        ];
        assert!(hidden_faces(&solids).is_empty());
        solids[0].covers = true;
        solids[1].candidates = vec![false; 6];
        assert!(hidden_faces(&solids).is_empty());
        let apart = vec![
            solid(DVec3::new(-16.0, -128.0, 0.0), DVec3::new(0.0, 128.0, 128.0), true),
            solid(DVec3::new(1.0, -32.0, 0.0), DVec3::new(64.0, 32.0, 64.0), true),
        ];
        assert!(hidden_faces(&apart).is_empty());
        assert!(hidden_faces(&[]).is_empty());
    }
}
