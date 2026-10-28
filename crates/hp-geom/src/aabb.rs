use glam::DVec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: DVec3,
    pub dir: DVec3,
}

impl Ray {
    #[inline]
    pub fn new(origin: DVec3, dir: DVec3) -> Self {
        Self { origin, dir }
    }

    #[inline]
    pub fn at(&self, t: f64) -> DVec3 {
        self.origin + self.dir * t
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: DVec3,
    pub max: DVec3,
}

impl Default for Aabb {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Aabb {
    pub const EMPTY: Aabb = Aabb { min: DVec3::INFINITY, max: DVec3::NEG_INFINITY };

    #[inline]
    pub fn new(a: DVec3, b: DVec3) -> Self {
        Self { min: a.min(b), max: a.max(b) }
    }

    #[inline]
    pub fn empty() -> Self {
        Self::EMPTY
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        !(self.min.x <= self.max.x && self.min.y <= self.max.y && self.min.z <= self.max.z)
    }

    pub fn from_points(points: impl IntoIterator<Item = DVec3>) -> Self {
        let mut b = Self::EMPTY;
        for p in points {
            b.extend(p);
        }
        b
    }

    #[inline]
    pub fn extend(&mut self, p: DVec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    #[inline]
    pub fn union(&self, other: &Aabb) -> Aabb {
        Aabb { min: self.min.min(other.min), max: self.max.max(other.max) }
    }

    #[inline]
    pub fn intersection(&self, other: &Aabb) -> Aabb {
        Aabb { min: self.min.max(other.min), max: self.max.min(other.max) }
    }

    #[inline]
    pub fn center(&self) -> DVec3 {
        (self.min + self.max) * 0.5
    }

    #[inline]
    pub fn size(&self) -> DVec3 {
        if self.is_empty() { DVec3::ZERO } else { self.max - self.min }
    }

    #[inline]
    pub fn volume(&self) -> f64 {
        let s = self.size();
        s.x * s.y * s.z
    }

    #[inline]
    pub fn contains_point(&self, p: DVec3) -> bool {
        p.x >= self.min.x
            && p.y >= self.min.y
            && p.z >= self.min.z
            && p.x <= self.max.x
            && p.y <= self.max.y
            && p.z <= self.max.z
    }

    #[inline]
    pub fn contains_aabb(&self, other: &Aabb) -> bool {
        !other.is_empty() && self.contains_point(other.min) && self.contains_point(other.max)
    }

    #[inline]
    pub fn intersects(&self, other: &Aabb) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }

    #[inline]
    pub fn expand(&self, amount: f64) -> Aabb {
        Aabb { min: self.min - DVec3::splat(amount), max: self.max + DVec3::splat(amount) }
    }

    pub fn corners(&self) -> [DVec3; 8] {
        let (a, b) = (self.min, self.max);
        [
            DVec3::new(a.x, a.y, a.z),
            DVec3::new(b.x, a.y, a.z),
            DVec3::new(a.x, b.y, a.z),
            DVec3::new(b.x, b.y, a.z),
            DVec3::new(a.x, a.y, b.z),
            DVec3::new(b.x, a.y, b.z),
            DVec3::new(a.x, b.y, b.z),
            DVec3::new(b.x, b.y, b.z),
        ]
    }

    pub fn ray_intersect(&self, ray: &Ray) -> Option<f64> {
        if self.is_empty() {
            return None;
        }
        let mut tmin = 0.0f64;
        let mut tmax = f64::INFINITY;
        for k in 0..3 {
            let o = ray.origin[k];
            let d = ray.dir[k];
            let (lo, hi) = (self.min[k], self.max[k]);
            if d.abs() < 1e-300 {
                if o < lo || o > hi {
                    return None;
                }
                continue;
            }
            let inv = 1.0 / d;
            let mut t0 = (lo - o) * inv;
            let mut t1 = (hi - o) * inv;
            if t0 > t1 {
                std::mem::swap(&mut t0, &mut t1);
            }
            tmin = tmin.max(t0);
            tmax = tmax.min(t1);
            if tmin > tmax || tmax.is_nan() {
                return None;
            }
        }
        tmin.is_finite().then_some(tmin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        let mut b = Aabb::empty();
        assert!(b.is_empty());
        assert_eq!(b.size(), DVec3::ZERO);
        b.extend(DVec3::new(1.0, 2.0, 3.0));
        b.extend(DVec3::new(-1.0, 0.0, 5.0));
        assert!(!b.is_empty());
        assert_eq!(b.min, DVec3::new(-1.0, 0.0, 3.0));
        assert_eq!(b.max, DVec3::new(1.0, 2.0, 5.0));
        assert_eq!(b.center(), DVec3::new(0.0, 1.0, 4.0));
        assert!(b.contains_point(DVec3::new(0.0, 1.0, 4.0)));
        assert!(!b.contains_point(DVec3::new(0.0, 3.0, 4.0)));
        let c = Aabb::new(DVec3::splat(1.0), DVec3::splat(10.0));
        assert!(b.intersects(&c));
        assert!(!b.intersects(&c.expand(-1.5)));
        assert_eq!(b.union(&c).max, DVec3::splat(10.0));
        assert_eq!(Aabb::from_points([DVec3::ONE, DVec3::ZERO]).volume(), 1.0);
        assert!(c.contains_aabb(&Aabb::new(DVec3::splat(2.0), DVec3::splat(3.0))));
    }

    #[test]
    fn ray_hits() {
        let b = Aabb::new(DVec3::splat(-1.0), DVec3::splat(1.0));
        let r = Ray::new(DVec3::new(-5.0, 0.0, 0.0), DVec3::X);
        assert_eq!(b.ray_intersect(&r), Some(4.0));
        let inside = Ray::new(DVec3::ZERO, DVec3::Y);
        assert_eq!(b.ray_intersect(&inside), Some(0.0));
        let miss = Ray::new(DVec3::new(-5.0, 2.0, 0.0), DVec3::X);
        assert_eq!(b.ray_intersect(&miss), None);
        let behind = Ray::new(DVec3::new(5.0, 0.0, 0.0), DVec3::X);
        assert_eq!(b.ray_intersect(&behind), None);
        let parallel = Ray::new(DVec3::new(0.0, 5.0, 0.0), DVec3::X);
        assert_eq!(b.ray_intersect(&parallel), None);
        assert_eq!(Aabb::EMPTY.ray_intersect(&r), None);
    }
}
