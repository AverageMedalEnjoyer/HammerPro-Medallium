use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use glam::DVec3;

use crate::Plane;

pub(crate) const INT_SNAP_EPS: f64 = 1e-6;

#[inline]
pub(crate) fn clean_coord(x: f64) -> f64 {
    const MAGIC: f64 = 6_755_399_441_055_744.0;
    if x.is_nan() || x.abs() >= 1e15 {
        return x + 0.0;
    }
    let r = (x + MAGIC) - MAGIC;
    if (x - r).abs() < INT_SNAP_EPS { r + 0.0 } else { x + 0.0 }
}

#[inline]
pub(crate) fn clean_vec(v: DVec3) -> DVec3 {
    DVec3::new(clean_coord(v.x), clean_coord(v.y), clean_coord(v.z))
}

pub(crate) const DECIMAL_SNAP_EPS: f64 = 1e-9;

pub(crate) fn clean_decimal(x: f64) -> f64 {
    if !x.is_finite() || x.abs() >= 1e9 {
        return x + 0.0;
    }
    let r = (x * 1e6).round() / 1e6;
    if (x - r).abs() <= DECIMAL_SNAP_EPS { r + 0.0 } else { x + 0.0 }
}

pub(crate) fn clean_point(v: DVec3) -> DVec3 {
    let c = |x: f64| clean_decimal(clean_coord(x));
    DVec3::new(c(v.x), c(v.y), c(v.z))
}

#[inline]
pub(crate) fn clean_unit(v: DVec3) -> DVec3 {
    fn c(x: f64) -> f64 {
        const E: f64 = 1e-12;
        if x.abs() < E {
            0.0
        } else if (x - 1.0).abs() < E {
            1.0
        } else if (x + 1.0).abs() < E {
            -1.0
        } else {
            x
        }
    }
    DVec3::new(c(v.x), c(v.y), c(v.z))
}

#[inline]
pub fn snap(v: f64, grid: f64) -> f64 {
    if grid > 0.0 && grid.is_finite() && v.is_finite() { (v / grid).round() * grid + 0.0 } else { v }
}

#[inline]
pub fn snap_vec3(v: DVec3, grid: f64) -> DVec3 {
    DVec3::new(snap(v.x, grid), snap(v.y, grid), snap(v.z, grid))
}

pub fn polygon_area_vector(poly: &[DVec3]) -> DVec3 {
    let Some(&p0) = poly.first() else {
        return DVec3::ZERO;
    };
    let mut sum = DVec3::ZERO;
    for w in poly.windows(2).skip(1) {
        sum += (w[0] - p0).cross(w[1] - p0);
    }
    sum * 0.5
}

pub fn polygon_normal(poly: &[DVec3]) -> Option<DVec3> {
    let a = polygon_area_vector(poly);
    if a.length_squared() > 1e-24 { a.try_normalize() } else { None }
}

pub fn polygon_center(poly: &[DVec3]) -> DVec3 {
    if poly.is_empty() {
        return DVec3::ZERO;
    }
    poly.iter().copied().sum::<DVec3>() / poly.len() as f64
}

pub fn to_hammer_winding(poly: &[DVec3]) -> Vec<DVec3> {
    poly.iter().rev().copied().collect()
}

pub fn plane_points_from_polygon(poly: &[DVec3]) -> Option<[DVec3; 3]> {
    let (&p0, rest) = poly.split_first()?;
    if rest.len() < 2 {
        return None;
    }
    let mut k = 0usize;
    let mut best = -1.0f64;
    for (i, p) in rest.iter().enumerate() {
        let d = p.distance_squared(p0);
        if d > best {
            best = d;
            k = i;
        }
    }
    let pk = *rest.get(k)?;
    let mut j = usize::MAX;
    let mut best_area = -1.0f64;
    for (i, p) in rest.iter().enumerate() {
        if i == k {
            continue;
        }
        let a = (*p - p0).cross(pk - p0).length_squared();
        if a > best_area {
            best_area = a;
            j = i;
        }
    }
    let pj = *rest.get(j)?;
    let (first, second) = if j < k { (pj, pk) } else { (pk, pj) };
    let pts = [second, first, p0];
    Plane::from_points(pts[0], pts[1], pts[2])?;
    Some(pts)
}

#[derive(Clone, Copy, Default)]
pub(crate) struct FxHasher {
    hash: u64,
}

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.add(u64::from(b));
        }
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }
    #[inline]
    fn write_i64(&mut self, i: i64) {
        self.add(i as u64);
    }
    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

pub(crate) type FxHashMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

pub(crate) fn fx_map<K, V>(capacity: usize) -> FxHashMap<K, V> {
    HashMap::with_capacity_and_hasher(capacity, BuildHasherDefault::default())
}

pub(crate) struct Welder {
    inv_cell: f64,
    eps2: f64,
    grid: Option<FxHashMap<[i64; 3], Vec<usize>>>,
    pub points: Vec<DVec3>,
}

const WELD_LINEAR_MAX: usize = 48;

#[inline]
fn floor_i64(v: f64) -> i64 {
    let i = v as i64;
    if (i as f64) > v { i.saturating_sub(1) } else { i }
}

impl Welder {
    pub fn new(eps: f64, capacity: usize) -> Self {
        let eps = if eps > 0.0 && eps.is_finite() { eps } else { 1e-9 };
        Self { inv_cell: 1.0 / eps, eps2: eps * eps, grid: None, points: Vec::with_capacity(capacity) }
    }

    #[inline]
    fn cell(&self, p: DVec3) -> [i64; 3] {
        [floor_i64(p.x * self.inv_cell), floor_i64(p.y * self.inv_cell), floor_i64(p.z * self.inv_cell)]
    }

    pub fn insert(&mut self, p: DVec3) -> usize {
        if self.grid.is_none() {
            if let Some(i) = self.points.iter().position(|q| q.distance_squared(p) < self.eps2) {
                return i;
            }
            if self.points.len() < WELD_LINEAR_MAX {
                self.points.push(p);
                return self.points.len() - 1;
            }
            let mut grid: FxHashMap<[i64; 3], Vec<usize>> = fx_map(self.points.capacity());
            for (i, &q) in self.points.iter().enumerate() {
                grid.entry(self.cell(q)).or_default().push(i);
            }
            self.grid = Some(grid);
        }
        let c = self.cell(p);
        if let Some(grid) = &self.grid {
            for dx in -1i64..=1 {
                for dy in -1i64..=1 {
                    for dz in -1i64..=1 {
                        let key = [c[0].saturating_add(dx), c[1].saturating_add(dy), c[2].saturating_add(dz)];
                        if let Some(list) = grid.get(&key) {
                            for &i in list {
                                if let Some(q) = self.points.get(i)
                                    && q.distance_squared(p) < self.eps2
                                {
                                    return i;
                                }
                            }
                        }
                    }
                }
            }
        }
        let idx = self.points.len();
        self.points.push(p);
        if let Some(grid) = &mut self.grid {
            grid.entry(c).or_default().push(idx);
        }
        idx
    }
}

pub(crate) fn weld_points(input: &[DVec3], eps: f64) -> Vec<DVec3> {
    let mut w = Welder::new(eps, input.len());
    for &p in input {
        if p.is_finite() {
            w.insert(p);
        }
    }
    w.points
}

pub(crate) fn dedupe_ring(poly: &mut Vec<DVec3>, eps: f64) {
    let eps2 = eps * eps;
    poly.dedup_by(|a, b| a.distance_squared(*b) < eps2);
    while poly.len() > 1 {
        match (poly.first(), poly.last()) {
            (Some(f), Some(l)) if f.distance_squared(*l) < eps2 => {
                poly.pop();
            }
            _ => break,
        }
    }
}

pub(crate) fn normalize_degrees(d: f64) -> f64 {
    if !d.is_finite() {
        return 0.0;
    }
    let r = d.rem_euclid(360.0);
    let r = clean_coord(r);
    if r >= 360.0 { 0.0 } else { r }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_basics() {
        assert_eq!(snap(13.0, 8.0), 16.0);
        assert_eq!(snap(-3.0, 8.0), 0.0);
        assert!(snap(-3.0, 8.0).is_sign_positive());
        assert_eq!(snap(5.0, 0.0), 5.0);
        assert_eq!(snap_vec3(DVec3::new(1.2, 2.6, -7.9), 1.0), DVec3::new(1.0, 3.0, -8.0));
    }

    #[test]
    fn clean_helpers() {
        assert_eq!(clean_coord(63.999_999_999_999_99), 64.0);
        assert_eq!(clean_coord(-0.0).to_bits(), 0.0f64.to_bits());
        assert_eq!(clean_coord(0.5), 0.5);
        assert_eq!(clean_unit(DVec3::new(1e-17, -1.0 + 1e-16, 0.5)), DVec3::new(0.0, -1.0, 0.5));
        assert_eq!(normalize_degrees(-90.0), 270.0);
        assert_eq!(normalize_degrees(720.0), 0.0);
    }

    #[test]
    fn clean_decimal_only_removes_noise() {
        assert_eq!(clean_decimal(255.999_999_999_999_94), 256.0);
        assert_eq!(clean_decimal(-256.000_000_000_000_06), -256.0);
        assert_eq!(clean_decimal(0.250_000_000_000_000_06), 0.25);
        assert_eq!(clean_decimal(0.1 + 0.2), 0.3);
        assert_eq!(clean_decimal(-0.0).to_bits(), 0.0f64.to_bits());
        for x in [-806.196_358_680_725_1, -60.423_133_134_841_92, 0.123_456_789_123, 1e-7, 123.456_789_5] {
            assert_eq!(clean_decimal(x), x);
        }
        assert!(clean_decimal(f64::NAN).is_nan());
        assert_eq!(clean_decimal(f64::INFINITY), f64::INFINITY);
        assert_eq!(clean_decimal(3e12 + 0.5), 3e12 + 0.5);
        assert_eq!(clean_point(DVec3::new(63.999_999_9, 96.499_999_999_999_99, -0.0)), DVec3::new(64.0, 96.5, 0.0));
    }

    #[test]
    fn plane_points_orientation() {
        let sq = [
            DVec3::new(0.0, 0.0, 5.0),
            DVec3::new(10.0, 0.0, 5.0),
            DVec3::new(10.0, 10.0, 5.0),
            DVec3::new(0.0, 10.0, 5.0),
        ];
        let pts = plane_points_from_polygon(&sq).expect("plane points");
        let p = Plane::from_points(pts[0], pts[1], pts[2]).expect("plane");
        assert!((p.normal - DVec3::Z).length() < 1e-12);
        assert!((p.dist - 5.0).abs() < 1e-12);
        assert!(plane_points_from_polygon(&sq[..2]).is_none());
        let line = [DVec3::ZERO, DVec3::X, DVec3::X * 2.0];
        assert!(plane_points_from_polygon(&line).is_none());
    }

    #[test]
    fn welder_merges() {
        let pts = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(0.0004, 0.0, 0.0),
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(f64::NAN, 0.0, 0.0),
            DVec3::new(1.0, 0.0, 0.0002),
        ];
        assert_eq!(weld_points(&pts, 0.001).len(), 2);
    }

    #[test]
    fn dedupe_ring_closing() {
        let mut p = vec![DVec3::ZERO, DVec3::ZERO, DVec3::X, DVec3::Y, DVec3::new(0.0, 0.0, 1e-5)];
        dedupe_ring(&mut p, 0.001);
        assert_eq!(p.len(), 3);
    }
}
