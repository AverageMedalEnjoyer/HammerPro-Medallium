use std::collections::HashMap;

use crate::{NavArea, NavMesh, attr};

pub const STEP: f64 = 25.0;
pub const HALF_WIDTH: f64 = 16.0;
pub const STAND_HEIGHT: f64 = 72.0;
pub const CROUCH_HEIGHT: f64 = 36.0;
pub const STEP_HEIGHT: f64 = 18.0;
pub const JUMP_HEIGHT: f64 = 58.0;
pub const MAX_DROP: f64 = 200.0;
pub const MIN_NORMAL_Z: f64 = 0.7;
pub const MAX_SAMPLES: usize = 600_000;
const PLANAR_TOLERANCE: f64 = 9.0;
const SAME_HEIGHT: f64 = 9.0;
const INDEX_CELL: f64 = 256.0;

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normalize(a: V3) -> Option<V3> {
    let l = dot(a, a).sqrt();
    (l > 1e-9 && l.is_finite()).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

#[derive(Clone, Debug, Default)]
pub struct GenSolid {
    pub planes: Vec<(V3, f64)>,
    pub faces: Vec<Vec<V3>>,
    pub min: V3,
    pub max: V3,
}

impl GenSolid {
    pub fn from_faces(faces: Vec<Vec<V3>>) -> Option<GenSolid> {
        let faces: Vec<Vec<V3>> = faces.into_iter().filter(|f| f.len() >= 3).collect();
        let mut min = [f64::MAX; 3];
        let mut max = [f64::MIN; 3];
        let mut sum = [0.0; 3];
        let mut count = 0.0;
        for f in &faces {
            for p in f {
                for i in 0..3 {
                    min[i] = min[i].min(p[i]);
                    max[i] = max[i].max(p[i]);
                    sum[i] += p[i];
                }
                count += 1.0;
            }
        }
        if count == 0.0 {
            return None;
        }
        let center = [sum[0] / count, sum[1] / count, sum[2] / count];
        let mut planes = Vec::with_capacity(faces.len());
        for f in &faces {
            let mut n = normalize(cross(sub(f[1], f[0]), sub(f[2], f[0])))?;
            if dot(n, sub(center, f[0])) > 0.0 {
                n = [-n[0], -n[1], -n[2]];
            }
            planes.push((n, dot(n, f[0])));
        }
        (planes.len() >= 4 && min.iter().chain(&max).all(|v| v.is_finite())).then_some(GenSolid {
            planes,
            faces,
            min,
            max,
        })
    }

    pub fn oriented_box(center: V3, half: V3, axes: [V3; 3]) -> Option<GenSolid> {
        let corner = |sx: f64, sy: f64, sz: f64| -> V3 {
            let mut p = center;
            for i in 0..3 {
                p[i] += axes[0][i] * half[0] * sx + axes[1][i] * half[1] * sy + axes[2][i] * half[2] * sz;
            }
            p
        };
        let c = |i: usize| {
            corner(
                if i & 1 != 0 { 1.0 } else { -1.0 },
                if i & 2 != 0 { 1.0 } else { -1.0 },
                if i & 4 != 0 { 1.0 } else { -1.0 },
            )
        };
        let quads = [[0, 2, 6, 4], [1, 5, 7, 3], [0, 4, 5, 1], [2, 3, 7, 6], [0, 1, 3, 2], [4, 6, 7, 5]];
        GenSolid::from_faces(quads.iter().map(|q| q.iter().map(|&i| c(i)).collect()).collect())
    }
}

#[derive(Clone, Debug, Default)]
pub struct GenInput {
    pub solids: Vec<GenSolid>,
    pub surfaces: Vec<[V3; 3]>,
    pub seeds: Vec<V3>,
    pub bsp_size: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GenReport {
    pub samples: usize,
    pub capped: bool,
    pub bad_seeds: usize,
}

struct Index {
    cells: HashMap<(i32, i32), (Vec<u32>, Vec<u32>)>,
}

fn cell_of(v: f64) -> i32 {
    (v / INDEX_CELL).floor().clamp(-1e6, 1e6) as i32
}

impl Index {
    fn new(input: &GenInput) -> Index {
        let mut cells: HashMap<(i32, i32), (Vec<u32>, Vec<u32>)> = HashMap::new();
        for (i, s) in input.solids.iter().enumerate() {
            for cx in cell_of(s.min[0])..=cell_of(s.max[0]) {
                for cy in cell_of(s.min[1])..=cell_of(s.max[1]) {
                    cells.entry((cx, cy)).or_default().0.push(i as u32);
                }
            }
        }
        for (i, t) in input.surfaces.iter().enumerate() {
            let (lo, hi) = tri_bounds(t);
            for cx in cell_of(lo[0])..=cell_of(hi[0]) {
                for cy in cell_of(lo[1])..=cell_of(hi[1]) {
                    cells.entry((cx, cy)).or_default().1.push(i as u32);
                }
            }
        }
        Index { cells }
    }

    fn query(&self, lo: [f64; 2], hi: [f64; 2], mut f: impl FnMut(Option<u32>, Option<u32>)) {
        let (x0, x1, y0, y1) = (cell_of(lo[0]), cell_of(hi[0]), cell_of(lo[1]), cell_of(hi[1]));
        let single = x0 == x1 && y0 == y1;
        let mut seen_s = std::collections::HashSet::new();
        let mut seen_t = std::collections::HashSet::new();
        for cx in x0..=x1 {
            for cy in y0..=y1 {
                let Some((s, t)) = self.cells.get(&(cx, cy)) else { continue };
                for &i in s {
                    if single || seen_s.insert(i) {
                        f(Some(i), None);
                    }
                }
                for &i in t {
                    if single || seen_t.insert(i) {
                        f(None, Some(i));
                    }
                }
            }
        }
    }
}

fn tri_bounds(t: &[V3; 3]) -> (V3, V3) {
    let mut lo = t[0];
    let mut hi = t[0];
    for p in &t[1..] {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    (lo, hi)
}

fn plane_z(n: V3, d: f64, x: f64, y: f64) -> Option<f64> {
    (n[2].abs() > 1e-6).then(|| (d - n[0] * x - n[1] * y) / n[2])
}

fn inside_xy(poly: &[V3], x: f64, y: f64) -> bool {
    let mut sign = 0.0f64;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let c = (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]);
        if c.abs() > 1e-9 {
            if sign != 0.0 && c.signum() != sign {
                return false;
            }
            sign = c.signum();
        }
    }
    true
}

fn box_tri(lo: V3, hi: V3, t: &[V3; 3]) -> bool {
    let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
    let h = [(hi[0] - lo[0]) * 0.5, (hi[1] - lo[1]) * 0.5, (hi[2] - lo[2]) * 0.5];
    let v = [sub(t[0], c), sub(t[1], c), sub(t[2], c)];
    let e = [sub(v[1], v[0]), sub(v[2], v[1]), sub(v[0], v[2])];
    let axes_box = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let separated = |axis: V3| -> bool {
        if dot(axis, axis) < 1e-12 {
            return false;
        }
        let p: Vec<f64> = v.iter().map(|p| dot(*p, axis)).collect();
        let r = h[0] * axis[0].abs() + h[1] * axis[1].abs() + h[2] * axis[2].abs();
        let (mn, mx) = (p.iter().cloned().fold(f64::MAX, f64::min), p.iter().cloned().fold(f64::MIN, f64::max));
        mn > r || mx < -r
    };
    for a in axes_box {
        for ed in e {
            if separated(cross(a, ed)) {
                return false;
            }
        }
        if separated(a) {
            return false;
        }
    }
    !separated(cross(e[0], e[1]))
}

fn box_solid(lo: V3, hi: V3, s: &GenSolid) -> bool {
    for i in 0..3 {
        if hi[i] <= s.min[i] || lo[i] >= s.max[i] {
            return false;
        }
    }
    for (n, d) in &s.planes {
        let p = [
            if n[0] > 0.0 { lo[0] } else { hi[0] },
            if n[1] > 0.0 { lo[1] } else { hi[1] },
            if n[2] > 0.0 { lo[2] } else { hi[2] },
        ];
        if dot(*n, p) >= *d {
            return false;
        }
    }
    true
}

struct World<'a> {
    input: &'a GenInput,
    index: Index,
}

impl World<'_> {
    fn clear(&self, lo: V3, hi: V3) -> bool {
        let lo = [lo[0] + 0.5, lo[1] + 0.5, lo[2] + 0.5];
        let hi = [hi[0] - 0.5, hi[1] - 0.5, hi[2] - 0.5];
        let mut blocked = false;
        self.index.query([lo[0], lo[1]], [hi[0], hi[1]], |s, t| {
            if blocked {
                return;
            }
            if let Some(s) = s {
                blocked = box_solid(lo, hi, &self.input.solids[s as usize]);
            } else if let Some(t) = t {
                let tri = &self.input.surfaces[t as usize];
                let (tl, th) = tri_bounds(tri);
                if (0..3).all(|i| th[i] >= lo[i] && tl[i] <= hi[i]) {
                    blocked = box_tri(lo, hi, tri);
                }
            }
        });
        !blocked
    }

    fn ground(&self, x: f64, y: f64, top: f64, bottom: f64) -> Option<(f64, f64)> {
        let mut best: Option<(f64, f64)> = None;
        let mut consider = |z: f64, nz: f64| {
            if z <= top && z >= bottom && best.is_none_or(|b| z > b.0) {
                best = Some((z, nz));
            }
        };
        self.index.query([x, y], [x, y], |s, t| {
            if let Some(s) = s {
                let s = &self.input.solids[s as usize];
                if x < s.min[0] || x > s.max[0] || y < s.min[1] || y > s.max[1] {
                    return;
                }
                for (f, (n, d)) in s.faces.iter().zip(&s.planes) {
                    if n[2] > 0.05
                        && inside_xy(f, x, y)
                        && let Some(z) = plane_z(*n, *d, x, y)
                    {
                        consider(z, n[2]);
                    }
                }
            } else if let Some(t) = t {
                let tri = &self.input.surfaces[t as usize];
                let Some(n) = normalize(cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]))) else { return };
                let n = if n[2] < 0.0 { [-n[0], -n[1], -n[2]] } else { n };
                if n[2] > 0.05
                    && inside_xy(tri, x, y)
                    && let Some(z) = plane_z(n, dot(n, tri[0]), x, y)
                {
                    consider(z, n[2]);
                }
            }
        });
        best
    }

    fn posture(&self, x: f64, y: f64, z: f64) -> Option<bool> {
        let lo = [x - HALF_WIDTH, y - HALF_WIDTH, z + STEP_HEIGHT];
        if self.clear(lo, [x + HALF_WIDTH, y + HALF_WIDTH, z + STAND_HEIGHT]) {
            Some(false)
        } else if self.clear(lo, [x + HALF_WIDTH, y + HALF_WIDTH, z + CROUCH_HEIGHT]) {
            Some(true)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
struct Sample {
    ix: i32,
    iy: i32,
    z: f64,
    crouch: bool,
    to: [Option<u32>; 4],
}

const DIRS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

fn opposite(d: usize) -> usize {
    (d + 2) % 4
}

fn sample(world: &World<'_>, report: &mut GenReport) -> Vec<Sample> {
    let mut samples: Vec<Sample> = Vec::new();
    let mut at: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    let mut queue: std::collections::VecDeque<u32> = std::collections::VecDeque::new();
    let find = |samples: &[Sample], at: &HashMap<(i32, i32), Vec<u32>>, ix: i32, iy: i32, z: f64| -> Option<u32> {
        at.get(&(ix, iy))?.iter().copied().find(|&i| (samples[i as usize].z - z).abs() < SAME_HEIGHT)
    };
    for seed in &world.input.seeds {
        let (ix, iy) = ((seed[0] / STEP).round() as i32, (seed[1] / STEP).round() as i32);
        let (x, y) = (f64::from(ix) * STEP, f64::from(iy) * STEP);
        let Some((z, nz)) = world.ground(x, y, seed[2] + STEP_HEIGHT, seed[2] - 256.0) else {
            report.bad_seeds += 1;
            continue;
        };
        let Some(crouch) = (nz >= MIN_NORMAL_Z).then(|| world.posture(x, y, z)).flatten() else {
            report.bad_seeds += 1;
            continue;
        };
        if find(&samples, &at, ix, iy, z).is_some() {
            continue;
        }
        let id = samples.len() as u32;
        samples.push(Sample { ix, iy, z, crouch, to: [None; 4] });
        at.entry((ix, iy)).or_default().push(id);
        queue.push_back(id);
    }
    while let Some(id) = queue.pop_front() {
        let s = samples[id as usize].clone();
        for (d, (dx, dy)) in DIRS.iter().enumerate() {
            let (nx, ny) = (s.ix + dx, s.iy + dy);
            let (x, y) = (f64::from(nx) * STEP, f64::from(ny) * STEP);
            let Some((z, nz)) = world.ground(x, y, s.z + JUMP_HEIGHT + 1.0, s.z - MAX_DROP) else { continue };
            if nz < MIN_NORMAL_Z {
                continue;
            }
            let (sx, sy) = (f64::from(s.ix) * STEP, f64::from(s.iy) * STEP);
            let base = s.z.max(z);
            let lo = [sx.min(x) - HALF_WIDTH, sy.min(y) - HALF_WIDTH, base + STEP_HEIGHT];
            let hi = [sx.max(x) + HALF_WIDTH, sy.max(y) + HALF_WIDTH, base + CROUCH_HEIGHT];
            if !world.clear(lo, hi) {
                continue;
            }
            let target = match find(&samples, &at, nx, ny, z) {
                Some(t) => t,
                None => {
                    let Some(crouch) = world.posture(x, y, z) else { continue };
                    if samples.len() >= MAX_SAMPLES {
                        report.capped = true;
                        continue;
                    }
                    let t = samples.len() as u32;
                    samples.push(Sample { ix: nx, iy: ny, z, crouch, to: [None; 4] });
                    at.entry((nx, ny)).or_default().push(t);
                    queue.push_back(t);
                    t
                }
            };
            if z - s.z <= JUMP_HEIGHT {
                samples[id as usize].to[d] = Some(target);
            }
            if s.z - z <= JUMP_HEIGHT {
                samples[target as usize].to[opposite(d)] = Some(id);
            }
        }
    }
    report.samples = samples.len();
    samples
}

fn build_areas(samples: &[Sample]) -> Vec<NavArea> {
    let n = samples.len();
    let mut area_of: Vec<Option<u32>> = vec![None; n];
    let linked = |a: u32, d: usize| -> Option<u32> {
        let b = samples[a as usize].to[d]?;
        (samples[b as usize].to[opposite(d)] == Some(a)).then_some(b)
    };
    let mut order: Vec<u32> = (0..n as u32).collect();
    order.sort_by_key(|&i| (samples[i as usize].iy, samples[i as usize].ix));
    let mut rects: Vec<Vec<Vec<u32>>> = Vec::new();
    for &start in &order {
        if area_of[start as usize].is_some() {
            continue;
        }
        let crouch = samples[start as usize].crouch;
        let free =
            |i: u32, area_of: &[Option<u32>]| area_of[i as usize].is_none() && samples[i as usize].crouch == crouch;
        let mut row = vec![start];
        while let Some(next) = linked(*row.last().unwrap_or(&start), 1) {
            if !free(next, &area_of) || row.len() >= 64 {
                break;
            }
            row.push(next);
            if !planar(samples, &[row.clone()]) {
                row.pop();
                break;
            }
        }
        let mut rows = vec![row];
        loop {
            if rows.len() >= 64 {
                break;
            }
            let last = &rows[rows.len() - 1];
            let next: Option<Vec<u32>> = last.iter().map(|&i| linked(i, 2).filter(|&j| free(j, &area_of))).collect();
            let Some(next) = next else { break };
            if next.windows(2).any(|w| linked(w[0], 1) != Some(w[1])) {
                break;
            }
            let mut candidate = rows.clone();
            candidate.push(next);
            if !planar(samples, &candidate) {
                break;
            }
            rows = candidate;
        }
        let id = rects.len() as u32;
        for r in &rows {
            for &i in r {
                area_of[i as usize] = Some(id);
            }
        }
        rects.push(rows);
    }
    let half = STEP * 0.5;
    let mut areas: Vec<NavArea> = rects
        .iter()
        .enumerate()
        .map(|(k, rows)| {
            let first = &rows[0];
            let last = &rows[rows.len() - 1];
            let nw = &samples[first[0] as usize];
            let ne = &samples[first[first.len() - 1] as usize];
            let sw = &samples[last[0] as usize];
            let se = &samples[last[last.len() - 1] as usize];
            let x0 = f64::from(nw.ix) * STEP - half;
            let y0 = f64::from(nw.iy) * STEP - half;
            let x1 = f64::from(se.ix) * STEP + half;
            let y1 = f64::from(se.iy) * STEP + half;
            NavArea {
                id: k as u32 + 1,
                flags: if nw.crouch { attr::CROUCH } else { 0 },
                nw: [x0 as f32, y0 as f32, nw.z as f32],
                se: [x1 as f32, y1 as f32, se.z as f32],
                ne_z: ne.z as f32,
                sw_z: sw.z as f32,
                light: Some([1.0; 4]),
                ..NavArea::default()
            }
        })
        .collect();
    for (i, s) in samples.iter().enumerate() {
        let Some(a) = area_of[i] else { continue };
        for (d, to) in s.to.iter().enumerate() {
            let Some(t) = to else { continue };
            let Some(b) = area_of[*t as usize] else { continue };
            if a != b {
                let list = &mut areas[a as usize].connections[d];
                if !list.contains(&(b + 1)) {
                    list.push(b + 1);
                }
            }
        }
    }
    areas
}

fn planar(samples: &[Sample], rows: &[Vec<u32>]) -> bool {
    let h = rows.len();
    let w = rows[0].len();
    let z = |r: usize, c: usize| samples[rows[r][c] as usize].z;
    let (nw, ne, sw, se) = (z(0, 0), z(0, w - 1), z(h - 1, 0), z(h - 1, w - 1));
    for r in 0..h {
        for c in 0..w {
            let u = if w > 1 { c as f64 / (w - 1) as f64 } else { 0.0 };
            let v = if h > 1 { r as f64 / (h - 1) as f64 } else { 0.0 };
            let expect = nw * (1.0 - u) * (1.0 - v) + ne * u * (1.0 - v) + sw * (1.0 - u) * v + se * u * v;
            if (z(r, c) - expect).abs() > PLANAR_TOLERANCE {
                return false;
            }
        }
    }
    true
}

pub fn generate(input: &GenInput) -> (NavMesh, GenReport) {
    let mut report = GenReport::default();
    let world = World { input, index: Index::new(input) };
    let samples = sample(&world, &mut report);
    let areas = build_areas(&samples);
    let mesh = NavMesh {
        version: crate::write::WRITE_VERSION,
        subversion: 0,
        bsp_size: Some(input.bsp_size),
        analyzed: false,
        places: Vec::new(),
        has_unnamed_areas: true,
        areas,
        ..NavMesh::default()
    };
    (mesh, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(lo: V3, hi: V3) -> GenSolid {
        let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
        let h = [(hi[0] - lo[0]) * 0.5, (hi[1] - lo[1]) * 0.5, (hi[2] - lo[2]) * 0.5];
        GenSolid::oriented_box(c, h, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]).expect("box")
    }

    fn room() -> Vec<GenSolid> {
        vec![
            block([-272.0, -272.0, -16.0], [272.0, 272.0, 0.0]),
            block([-272.0, -272.0, 256.0], [272.0, 272.0, 272.0]),
            block([-272.0, -272.0, 0.0], [-256.0, 272.0, 256.0]),
            block([256.0, -272.0, 0.0], [272.0, 272.0, 256.0]),
            block([-256.0, -272.0, 0.0], [256.0, -256.0, 256.0]),
            block([-256.0, 256.0, 0.0], [256.0, 272.0, 256.0]),
        ]
    }

    #[test]
    fn a_room_becomes_walkable_areas() {
        let input = GenInput { solids: room(), seeds: vec![[0.0, 0.0, 1.0]], bsp_size: 1234, ..Default::default() };
        let (mesh, r) = generate(&input);
        assert_eq!(r.bad_seeds, 0);
        assert!(!r.capped);
        assert!(r.samples > 200, "{r:?}");
        assert!(!mesh.areas.is_empty());
        for a in &mesh.areas {
            assert!(a.nw[2].abs() < 0.01 && a.se[2].abs() < 0.01, "{a:?}");
            assert!(a.nw[0] >= -256.0 && a.se[0] <= 256.0 && a.nw[1] >= -256.0 && a.se[1] <= 256.0, "{a:?}");
        }
        assert!(mesh.areas.len() <= 16, "{} areas", mesh.areas.len());
        assert_eq!(mesh.stats().isolated_areas, 0);
        let bytes = crate::write::write(&mesh);
        let back = crate::parse(&bytes).expect("parse");
        assert_eq!(back.areas.len(), mesh.areas.len());
        assert_eq!(back.bsp_size, Some(1234));
        assert_eq!(back.trailing_bytes, 0);
        assert_eq!(
            back.areas,
            mesh.areas.iter().map(|a| NavArea { encounter_paths: 0, ..a.clone() }).collect::<Vec<_>>()
        );
    }

    #[test]
    fn walls_split_and_low_ceilings_crouch() {
        let mut solids = room();
        solids.push(block([-8.0, -256.0, 40.0], [8.0, 256.0, 256.0]));
        let input = GenInput { solids, seeds: vec![[-128.0, 0.0, 1.0]], ..Default::default() };
        let (mesh, _) = generate(&input);
        assert!(mesh.areas.iter().any(|a| a.flags & attr::CROUCH != 0), "a crouch area under the wall");
        assert!(mesh.areas.iter().any(|a| a.se[0] > 100.0), "the other side is reached by crouching");
        let mut solids = room();
        solids.push(block([-8.0, -256.0, 0.0], [8.0, 256.0, 256.0]));
        let (mesh, _) = generate(&GenInput { solids, seeds: vec![[-128.0, 0.0, 1.0]], ..Default::default() });
        assert!(mesh.areas.iter().all(|a| a.se[0] <= 8.0), "nothing beyond a closed wall");
    }

    #[test]
    fn steps_connect_and_cliffs_drop_one_way() {
        let mut solids = room();
        solids.push(block([0.0, -256.0, 0.0], [256.0, 0.0, 16.0]));
        solids.push(block([0.0, 0.0, 0.0], [256.0, 256.0, 128.0]));
        let (mesh, _) = generate(&GenInput { solids, seeds: vec![[-128.0, 0.0, 1.0]], ..Default::default() });
        assert!(mesh.areas.iter().any(|a| (a.nw[2] - 16.0).abs() < 0.5), "the step is walkable");
        assert!(mesh.areas.iter().all(|a| (a.nw[2] - 128.0).abs() > 0.5), "the high block is not reached");
    }

    #[test]
    fn seeds_in_the_void_and_empty_input() {
        let (mesh, r) = generate(&GenInput { seeds: vec![[0.0, 0.0, 0.0]], ..Default::default() });
        assert!(mesh.areas.is_empty());
        assert_eq!(r.bad_seeds, 1);
        let (mesh, _) = generate(&GenInput::default());
        assert!(mesh.areas.is_empty());
    }
}
