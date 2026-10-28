use glam::{DVec3, Vec3};

pub const NORMAL_EPSILON: f64 = 0.00001;
pub const DIST_EPSILON: f64 = 0.01;

pub const PLANE_X: i32 = 0;
pub const PLANE_Y: i32 = 1;
pub const PLANE_Z: i32 = 2;
pub const PLANE_ANYX: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapPlane {
    pub normal: DVec3,
    pub dist: f64,
    pub kind: i32,
}

impl MapPlane {
    #[inline]
    pub fn distance(&self, p: DVec3) -> f64 {
        self.normal.dot(p) - self.dist
    }
}

pub fn plane_type(n: DVec3) -> i32 {
    if n.x == 1.0 || n.x == -1.0 {
        return PLANE_X;
    }
    if n.y == 1.0 || n.y == -1.0 {
        return PLANE_Y;
    }
    if n.z == 1.0 || n.z == -1.0 {
        return PLANE_Z;
    }
    let a = n.abs();
    if a.x >= a.y && a.x >= a.z {
        PLANE_ANYX
    } else if a.y >= a.z {
        PLANE_ANYX + 1
    } else {
        PLANE_ANYX + 2
    }
}

pub fn snap_normal(n: DVec3) -> DVec3 {
    for i in 0..3 {
        if (n[i] - 1.0).abs() < NORMAL_EPSILON {
            let mut v = DVec3::ZERO;
            v[i] = 1.0;
            return v;
        }
        if (n[i] + 1.0).abs() < NORMAL_EPSILON {
            let mut v = DVec3::ZERO;
            v[i] = -1.0;
            return v;
        }
    }
    n
}

pub fn snap_dist(d: f64) -> f64 {
    let r = d.round();
    if (d - r).abs() < DIST_EPSILON { r } else { d }
}

pub fn snap_vector_f32(n: &mut Vec3) -> bool {
    for i in 0..3 {
        if f64::from((n[i] - 1.0).abs()) < NORMAL_EPSILON {
            *n = Vec3::ZERO;
            n[i] = 1.0;
            return true;
        }
        if f64::from((n[i] + 1.0).abs()) < NORMAL_EPSILON {
            *n = Vec3::ZERO;
            n[i] = -1.0;
            return true;
        }
    }
    false
}

fn round_int(d: f32) -> f32 {
    (d + 0.5).floor()
}

fn snap_dist_f32(d: f32) -> f32 {
    if (d - round_int(d)).abs() < 0.01 { round_int(d) } else { d }
}

pub fn normalize_f32(v: Vec3) -> (Vec3, f32) {
    let r = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    let ir = 1.0 / (r + 1e-10);
    (Vec3::new(v.x * ir, v.y * ir, v.z * ir), r)
}

const PLANE_HASHES: usize = 1024;

#[derive(Clone, Debug)]
pub struct PlaneSet {
    planes: Vec<MapPlane>,
    fplanes: Vec<(Vec3, f32)>,
    hash: Vec<Vec<u32>>,
}

impl Default for PlaneSet {
    fn default() -> Self {
        Self { planes: Vec::new(), fplanes: Vec::new(), hash: vec![Vec::new(); PLANE_HASHES] }
    }
}

impl PlaneSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.planes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.planes.is_empty()
    }

    #[inline]
    pub fn get(&self, i: usize) -> &MapPlane {
        &self.planes[i]
    }

    #[inline]
    pub fn getf(&self, i: usize) -> (Vec3, f32) {
        self.fplanes[i]
    }

    pub fn all(&self) -> &[MapPlane] {
        &self.planes
    }

    fn matches(p: (Vec3, f32), normal: Vec3, dist: f32) -> bool {
        let ne = NORMAL_EPSILON as f32;
        (p.0.x - normal.x).abs() < ne
            && (p.0.y - normal.y).abs() < ne
            && (p.0.z - normal.z).abs() < ne
            && (p.1 - dist).abs() < 0.01
    }

    fn bucket(dist: f32) -> usize {
        let i = dist.abs() as i64 / 8;
        (i as usize) & (PLANE_HASHES - 1)
    }

    pub fn find(&mut self, normal: DVec3, dist: f64) -> Option<usize> {
        if !dist.is_finite() || !normal.is_finite() {
            return None;
        }
        let len = normal.length();
        if len < 0.5 {
            return None;
        }
        let normal = if (len - 1.0).abs() > 1e-6 { normal / len } else { normal };
        Some(self.find_f32(normal.as_vec3(), dist as f32))
    }

    pub fn find_f32(&mut self, mut normal: Vec3, mut dist: f32) -> usize {
        snap_vector_f32(&mut normal);
        dist = snap_dist_f32(dist);
        let h = Self::bucket(dist);
        for k in [h + PLANE_HASHES - 1, h, h + 1] {
            let k = k & (PLANE_HASHES - 1);
            for &i in self.hash[k].iter().rev() {
                if Self::matches(self.fplanes[i as usize], normal, dist) {
                    return i as usize;
                }
            }
        }
        let kind = plane_type(normal.as_dvec3());
        let i = self.planes.len();
        let swap = kind < PLANE_ANYX && (normal.x < 0.0 || normal.y < 0.0 || normal.z < 0.0);
        let back = Vec3::ZERO - normal;
        let (first, second, result) =
            if swap { ((back, -dist), (normal, dist), i + 1) } else { ((normal, dist), (back, -dist), i) };
        for (n, d) in [first, second] {
            let idx = self.planes.len();
            self.planes.push(MapPlane { normal: n.as_dvec3(), dist: f64::from(d), kind });
            self.fplanes.push((n, d));
            self.hash[Self::bucket(d)].push(idx as u32);
        }
        result
    }

    pub fn from_points(&mut self, a: DVec3, b: DVec3, c: DVec3) -> Option<usize> {
        let (p0, p1, p2) = (a.as_vec3(), b.as_vec3(), c.as_vec3());
        let (mut normal, len) = normalize_f32((p0 - p1).cross(p2 - p1));
        if len == 0.0 || !len.is_finite() {
            return None;
        }
        let mut dist = p0.dot(normal);
        if snap_vector_f32(&mut normal) {
            let sum = p0 + p1 + p2;
            let oob = 1.0f32 / 3.0;
            let p3 = Vec3::new(sum.x * oob, sum.y * oob, sum.z * oob);
            dist = normal.dot(p3);
        }
        dist = snap_dist_f32(dist);
        Some(self.find_f32(normal, dist))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_are_shared_and_positive_first() {
        let mut s = PlaneSet::new();
        let a = s.find(DVec3::new(0.0, 0.0, -1.0), -64.0).unwrap();
        assert_eq!(a, 1, "the -Z plane is the odd one of its pair");
        assert_eq!(s.get(0).normal, DVec3::Z);
        assert_eq!(s.get(0).dist, 64.0);
        let b = s.find(DVec3::new(0.0, 0.0, 1.0), 64.000_001).unwrap();
        assert_eq!(b, 0);
        assert_eq!(a ^ 1, b);
        assert_eq!(s.len(), 2);
        let c = s.find(DVec3::new(1.0, 1.0, 0.0), 10.0).unwrap();
        assert_eq!(s.get(c).kind, PLANE_ANYX);
        assert!(s.find(DVec3::ZERO, 1.0).is_none());
    }
}
