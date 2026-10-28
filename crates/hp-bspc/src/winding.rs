use glam::DVec3;

use crate::planes::MapPlane;

pub const ON_EPSILON: f64 = 0.1;
pub const MAX_WORLD: f64 = 65536.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Winding {
    pub points: Vec<DVec3>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Front,
    Back,
    On,
    Cross,
}

impl Winding {
    pub fn new(points: Vec<DVec3>) -> Self {
        Self { points }
    }

    pub fn base_for_plane(normal: DVec3, dist: f64) -> Self {
        let a = normal.abs();
        let up = if a.z >= a.x && a.z >= a.y { DVec3::X } else { DVec3::Z };
        let up = (up - normal * up.dot(normal)).normalize();
        let org = normal * dist;
        let right = up.cross(normal);
        let (up, right) = (up * MAX_WORLD * 2.0, right * MAX_WORLD * 2.0);
        Self { points: vec![org - right + up, org + right + up, org + right - up, org - right - up] }
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn reversed(&self) -> Self {
        Self { points: self.points.iter().rev().copied().collect() }
    }

    pub fn area(&self) -> f64 {
        let mut total = DVec3::ZERO;
        for i in 2..self.points.len() {
            total += (self.points[i - 1] - self.points[0]).cross(self.points[i] - self.points[0]);
        }
        total.length() * 0.5
    }

    pub fn center(&self) -> DVec3 {
        if self.points.is_empty() {
            return DVec3::ZERO;
        }
        self.points.iter().copied().sum::<DVec3>() / self.points.len() as f64
    }

    pub fn bounds(&self) -> (DVec3, DVec3) {
        let mut lo = DVec3::splat(f64::MAX);
        let mut hi = DVec3::splat(f64::MIN);
        for p in &self.points {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        (lo, hi)
    }

    pub fn normal(&self) -> DVec3 {
        let mut total = DVec3::ZERO;
        for i in 2..self.points.len() {
            total += (self.points[i] - self.points[0]).cross(self.points[i - 1] - self.points[0]);
        }
        total.normalize_or_zero()
    }

    pub fn is_tiny(&self) -> bool {
        if self.points.len() < 3 {
            return true;
        }
        let mut edges = 0;
        for i in 0..self.points.len() {
            let j = (i + 1) % self.points.len();
            if (self.points[j] - self.points[i]).length() > 0.2 {
                edges += 1;
                if edges == 3 {
                    return false;
                }
            }
        }
        true
    }

    pub fn is_huge(&self) -> bool {
        self.points.iter().any(|p| p.abs().max_element() > MAX_WORLD - 1.0)
    }

    pub fn side_of(&self, plane: &MapPlane, eps: f64) -> Side {
        let (mut front, mut back) = (false, false);
        for p in &self.points {
            let d = plane.distance(*p);
            if d > eps {
                front = true;
            } else if d < -eps {
                back = true;
            }
        }
        match (front, back) {
            (true, true) => Side::Cross,
            (true, false) => Side::Front,
            (false, true) => Side::Back,
            (false, false) => Side::On,
        }
    }

    pub fn split(&self, plane: &MapPlane, eps: f64) -> (Option<Winding>, Option<Winding>) {
        let n = self.points.len();
        let dists: Vec<f64> = self.points.iter().map(|p| plane.distance(*p)).collect();
        let sides: Vec<i8> = dists
            .iter()
            .map(|d| {
                if *d > eps {
                    1
                } else if *d < -eps {
                    -1
                } else {
                    0
                }
            })
            .collect();
        let front = sides.iter().any(|s| *s > 0);
        let back = sides.iter().any(|s| *s < 0);
        if !front && !back {
            return if self.normal().dot(plane.normal) > 0.0 {
                (Some(self.clone()), None)
            } else {
                (None, Some(self.clone()))
            };
        }
        if !front {
            return (None, Some(self.clone()));
        }
        if !back {
            return (Some(self.clone()), None);
        }
        let mut f = Vec::with_capacity(n + 4);
        let mut b = Vec::with_capacity(n + 4);
        for i in 0..n {
            let p1 = self.points[i];
            match sides[i] {
                0 => {
                    f.push(p1);
                    b.push(p1);
                    continue;
                }
                1 => f.push(p1),
                _ => b.push(p1),
            }
            let j = (i + 1) % n;
            if sides[j] == 0 || sides[j] == sides[i] {
                continue;
            }
            let p2 = self.points[j];
            let t = dists[i] / (dists[i] - dists[j]);
            let mut mid = p1 + (p2 - p1) * t;
            for k in 0..3 {
                if plane.normal[k] == 1.0 {
                    mid[k] = plane.dist;
                } else if plane.normal[k] == -1.0 {
                    mid[k] = -plane.dist;
                }
            }
            f.push(mid);
            b.push(mid);
        }
        let f = Winding::new(f);
        let b = Winding::new(b);
        ((f.points.len() >= 3).then_some(f), (b.points.len() >= 3).then_some(b))
    }

    pub fn clip_back(&self, plane: &MapPlane, eps: f64, keep_on: bool) -> Option<Winding> {
        if keep_on && self.side_of(plane, eps) == Side::On {
            return Some(self.clone());
        }
        let (_, back) = self.split(plane, eps);
        back
    }

    pub fn remove_colinear(&mut self) {
        let mut out: Vec<DVec3> = Vec::with_capacity(self.points.len());
        let n = self.points.len();
        for i in 0..n {
            let prev = self.points[(i + n - 1) % n];
            let p = self.points[i];
            let next = self.points[(i + 1) % n];
            let a = (p - prev).normalize_or_zero();
            let b = (next - p).normalize_or_zero();
            if a == DVec3::ZERO || b == DVec3::ZERO {
                continue;
            }
            if a.dot(b) < 0.999_999 {
                out.push(p);
            }
        }
        self.points = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(n: DVec3, d: f64) -> MapPlane {
        MapPlane { normal: n, dist: d, kind: crate::planes::plane_type(n) }
    }

    #[test]
    fn base_winding_faces_its_plane_and_splits() {
        let w = Winding::base_for_plane(DVec3::Z, 10.0);
        assert!((w.normal() - DVec3::Z).length() < 1e-9);
        assert!(w.points.iter().all(|p| (p.z - 10.0).abs() < 1e-9));
        let (f, b) = w.split(&plane(DVec3::X, 0.0), ON_EPSILON);
        let (f, b) = (f.unwrap(), b.unwrap());
        assert!(f.points.iter().all(|p| p.x >= -1e-9));
        assert!(b.points.iter().all(|p| p.x <= 1e-9));
        assert!((f.area() + b.area() - w.area()).abs() < 1.0);
        let (f, b) = w.split(&plane(DVec3::Z, 10.0), ON_EPSILON);
        assert!(f.is_some() && b.is_none());
        let (f, b) = w.split(&plane(-DVec3::Z, -10.0), ON_EPSILON);
        assert!(f.is_none() && b.is_some());
    }
}
