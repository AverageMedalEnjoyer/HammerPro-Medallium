use glam::{DVec2, DVec3};

use crate::{Aabb, Ray, SolidGeom};

pub fn ray_solid(ray: &Ray, geom: &SolidGeom) -> Option<(f64, usize)> {
    if geom.faces.is_empty() {
        return None;
    }
    geom.aabb.expand(1e-6).ray_intersect(ray)?;
    let mut t_near = f64::NEG_INFINITY;
    let mut t_far = f64::INFINITY;
    let mut near_face = None;
    for (i, f) in geom.faces.iter().enumerate() {
        let denom = f.plane.normal.dot(ray.dir);
        let dist = f.plane.distance(ray.origin);
        if denom.abs() < 1e-15 {
            if dist > 0.0 {
                return None;
            }
            continue;
        }
        let t = -dist / denom;
        if denom < 0.0 {
            if t > t_near {
                t_near = t;
                near_face = Some(i);
            }
        } else if t < t_far {
            t_far = t;
        }
        if t_near > t_far {
            return None;
        }
    }
    if t_near < 0.0 || !t_near.is_finite() {
        return None;
    }
    near_face.map(|f| (t_near, f))
}

pub fn ray_triangle(ray: &Ray, a: DVec3, b: DVec3, c: DVec3, cull_back_faces: bool) -> Option<f64> {
    const EPS: f64 = 1e-12;
    let e1 = b - a;
    let e2 = c - a;
    let p = ray.dir.cross(e2);
    let det = e1.dot(p);
    if cull_back_faces {
        if det < EPS {
            return None;
        }
    } else if det.abs() < EPS {
        return None;
    }
    let inv = 1.0 / det;
    let s = ray.origin - a;
    let u = s.dot(p) * inv;
    if !(-1e-9..=1.0 + 1e-9).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = ray.dir.dot(q) * inv;
    if v < -1e-9 || u + v > 1.0 + 1e-9 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t >= 0.0 && t.is_finite()).then_some(t)
}

#[inline]
pub fn ray_aabb(ray: &Ray, aabb: &Aabb) -> Option<f64> {
    aabb.ray_intersect(ray)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ViewAxis {
    Top,
    Front,
    Side,
}

impl ViewAxis {
    pub const ALL: [ViewAxis; 3] = [ViewAxis::Top, ViewAxis::Front, ViewAxis::Side];

    #[inline]
    pub fn axes_2d(self) -> (usize, usize) {
        match self {
            ViewAxis::Top => (0, 1),
            ViewAxis::Front => (1, 2),
            ViewAxis::Side => (0, 2),
        }
    }

    #[inline]
    pub fn depth_axis(self) -> usize {
        match self {
            ViewAxis::Top => 2,
            ViewAxis::Front => 0,
            ViewAxis::Side => 1,
        }
    }

    #[inline]
    pub fn to_2d(self, p: DVec3) -> DVec2 {
        let (h, v) = self.axes_2d();
        DVec2::new(p[h], p[v])
    }

    #[inline]
    pub fn from_2d(self, p: DVec2, depth: f64) -> DVec3 {
        let (h, v) = self.axes_2d();
        let mut out = DVec3::ZERO;
        out[h] = p.x;
        out[v] = p.y;
        out[self.depth_axis()] = depth;
        out
    }

    #[inline]
    pub fn depth(self, p: DVec3) -> f64 {
        p[self.depth_axis()]
    }

    #[inline]
    pub fn right(self) -> DVec3 {
        match self {
            ViewAxis::Top | ViewAxis::Side => DVec3::X,
            ViewAxis::Front => DVec3::Y,
        }
    }

    #[inline]
    pub fn up(self) -> DVec3 {
        match self {
            ViewAxis::Top => DVec3::Y,
            ViewAxis::Front | ViewAxis::Side => DVec3::Z,
        }
    }

    #[inline]
    pub fn forward(self) -> DVec3 {
        match self {
            ViewAxis::Top => DVec3::NEG_Z,
            ViewAxis::Front => DVec3::NEG_X,
            ViewAxis::Side => DVec3::Y,
        }
    }

    pub fn ray_through(self, p: DVec2) -> Ray {
        let f = self.forward();
        let origin = self.from_2d(p, 0.0) - f * (crate::MAX_COORD * 4.0);
        Ray::new(origin, f)
    }
}

pub fn point_segment_distance_2d(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 0.0 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + ab * t)
}

pub fn point_segment_distance(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 0.0 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + ab * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_solid;
    use crate::test_support::*;

    #[test]
    fn pick_box_faces() {
        let sides = hammer_box(DVec3::ZERO, DVec3::new(64.0, 64.0, 32.0));
        let g = build_solid(&sides);
        let r = Ray::new(DVec3::new(32.0, 32.0, 100.0), DVec3::NEG_Z);
        let (t, f) = ray_solid(&r, &g).expect("hit");
        assert!((t - 68.0).abs() < 1e-12);
        assert_eq!(g.faces[f].side_index, 0);
        let r = Ray::new(DVec3::new(-10.0, 10.0, 10.0), DVec3::new(1.0, 0.1, 0.0));
        let (t, f) = ray_solid(&r, &g).expect("hit");
        assert!((r.at(t).x).abs() < 1e-9);
        assert_eq!(g.faces[f].side_index, 5);
        assert!(ray_solid(&Ray::new(DVec3::new(100.0, 100.0, 100.0), DVec3::NEG_Z), &g).is_none());
        assert!(ray_solid(&Ray::new(DVec3::new(32.0, 32.0, 100.0), DVec3::Z), &g).is_none());
        assert!(ray_solid(&Ray::new(DVec3::splat(10.0), DVec3::X), &g).is_none());
        assert!(ray_solid(&Ray::new(DVec3::new(-1.0, 10.0, 40.0), DVec3::X), &g).is_none());
    }

    #[test]
    fn pick_matches_triangles() {
        let mut rng = Rng(99);
        let sides = hammer_box(DVec3::new(-30.0, -20.0, -10.0), DVec3::new(30.0, 25.0, 12.0));
        let g = build_solid(&sides);
        for _ in 0..500 {
            let origin = rng.point(200.0);
            let target = rng.point(25.0);
            let r = Ray::new(origin, target - origin);
            let convex = ray_solid(&r, &g).map(|(t, _)| t);
            let mut tri: Option<f64> = None;
            for f in &g.faces {
                for [a, b, c] in f.triangles() {
                    if let Some(t) = ray_triangle(&r, f.vertices[a], f.vertices[b], f.vertices[c], true) {
                        tri = Some(tri.map_or(t, |x: f64| x.min(t)));
                    }
                }
            }
            match (convex, tri) {
                (Some(a), Some(b)) => assert!((a - b).abs() < 1e-9),
                (None, None) => {}
                (a, b) => {
                    assert!(g.aabb.contains_point(origin), "{a:?} vs {b:?}");
                }
            }
        }
    }

    #[test]
    fn triangle_culling() {
        let (a, b, c) = (DVec3::ZERO, DVec3::X, DVec3::Y);
        let down = Ray::new(DVec3::new(0.2, 0.2, 1.0), DVec3::NEG_Z);
        assert_eq!(ray_triangle(&down, a, b, c, true), Some(1.0));
        let up = Ray::new(DVec3::new(0.2, 0.2, -1.0), DVec3::Z);
        assert_eq!(ray_triangle(&up, a, b, c, true), None);
        assert_eq!(ray_triangle(&up, a, b, c, false), Some(1.0));
        let miss = Ray::new(DVec3::new(0.8, 0.8, 1.0), DVec3::NEG_Z);
        assert_eq!(ray_triangle(&miss, a, b, c, false), None);
        assert_eq!(ray_aabb(&down, &Aabb::new(DVec3::ZERO, DVec3::ONE)), Some(0.0));
    }

    #[test]
    fn views() {
        let p = DVec3::new(1.0, 2.0, 3.0);
        assert_eq!(ViewAxis::Top.to_2d(p), DVec2::new(1.0, 2.0));
        assert_eq!(ViewAxis::Front.to_2d(p), DVec2::new(2.0, 3.0));
        assert_eq!(ViewAxis::Side.to_2d(p), DVec2::new(1.0, 3.0));
        for v in ViewAxis::ALL {
            assert_eq!(v.from_2d(v.to_2d(p), v.depth(p)), p);
            assert_eq!(v.right().cross(v.up()), -v.forward());
            assert_eq!(v.to_2d(v.right()), DVec2::X);
            assert_eq!(v.to_2d(v.up()), DVec2::Y);
            let r = v.ray_through(DVec2::new(5.0, 6.0));
            assert_eq!(v.to_2d(r.origin), DVec2::new(5.0, 6.0));
        }
        assert_eq!(point_segment_distance_2d(DVec2::new(5.0, 3.0), DVec2::ZERO, DVec2::new(10.0, 0.0)), 3.0);
        assert_eq!(point_segment_distance_2d(DVec2::new(-4.0, 3.0), DVec2::ZERO, DVec2::new(10.0, 0.0)), 5.0);
        assert_eq!(point_segment_distance_2d(DVec2::new(3.0, 4.0), DVec2::ZERO, DVec2::ZERO), 5.0);
        assert_eq!(point_segment_distance(DVec3::new(5.0, 0.0, 2.0), DVec3::ZERO, DVec3::X * 10.0), 2.0);
    }
}
