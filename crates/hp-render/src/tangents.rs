use glam::{Vec2, Vec3};

use crate::Vertex;

pub fn compute_tangents(vertices: &[Vertex], indices: &[u32]) -> Vec<[f32; 4]> {
    let mut tan = vec![Vec3::ZERO; vertices.len()];
    let mut bit = vec![Vec3::ZERO; vertices.len()];
    for t in indices.chunks_exact(3) {
        let Some(corners) = t.iter().map(|&i| vertices.get(i as usize)).collect::<Option<Vec<_>>>() else {
            continue;
        };
        let p = corners.iter().map(|v| Vec3::from(v.pos)).collect::<Vec<_>>();
        let uv = corners.iter().map(|v| Vec2::from(v.uv)).collect::<Vec<_>>();
        let (e1, e2) = (p[1] - p[0], p[2] - p[0]);
        let (d1, d2) = (uv[1] - uv[0], uv[2] - uv[0]);
        let det = d1.x * d2.y - d2.x * d1.y;
        if !det.is_finite() || det.abs() < 1e-12 {
            continue;
        }
        let r = 1.0 / det;
        let sdir = (e1 * d2.y - e2 * d1.y) * r;
        let tdir = (e2 * d1.x - e1 * d2.x) * r;
        if !sdir.is_finite() || !tdir.is_finite() {
            continue;
        }
        let (sdir, tdir) = (sdir.normalize_or_zero(), tdir.normalize_or_zero());
        for k in 0..3 {
            let a = (p[(k + 1) % 3] - p[k]).normalize_or_zero();
            let b = (p[(k + 2) % 3] - p[k]).normalize_or_zero();
            let angle = a.dot(b).clamp(-1.0, 1.0).acos();
            let w = if angle.is_finite() { angle } else { 0.0 };
            let i = t[k] as usize;
            tan[i] += sdir * w;
            bit[i] += tdir * w;
        }
    }
    vertices
        .iter()
        .zip(tan.iter().zip(&bit))
        .map(|(v, (&t, &b))| {
            let n = Vec3::from(v.normal).normalize_or(Vec3::Z);
            let mut s = (t - n * n.dot(t)).normalize_or_zero();
            if s == Vec3::ZERO {
                let helper = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
                s = (helper - n * n.dot(helper)).normalize_or(Vec3::X);
            }
            let w = if n.cross(s).dot(b) < 0.0 { -1.0 } else { 1.0 };
            [s.x, s.y, s.z, w]
        })
        .collect()
}

pub(crate) fn pack_tangent(t: [f32; 4]) -> u32 {
    let s = |v: f32| {
        let v = if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 };
        ((v * 127.0).round() as i8) as u8 as u32
    };
    let w = if t[3] < 0.0 { -1.0 } else { 1.0 };
    s(t[0]) | (s(t[1]) << 8) | (s(t[2]) << 16) | (s(w) << 24)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(pos: [f32; 3], uv: [f32; 2]) -> Vertex {
        Vertex { pos, normal: [0.0, 0.0, 1.0], uv, alpha: 1.0 }
    }

    #[test]
    fn tangents_follow_the_texture_axes() {
        let verts = [v([0.0, 0.0, 0.0], [0.0, 0.0]), v([10.0, 0.0, 0.0], [1.0, 0.0]), v([0.0, -10.0, 0.0], [0.0, 1.0])];
        let t = compute_tangents(&verts, &[0, 1, 2]);
        for tan in &t {
            assert!((tan[0] - 1.0).abs() < 1e-5 && tan[1].abs() < 1e-5 && tan[2].abs() < 1e-5, "{tan:?}");
            assert_eq!(tan[3], -1.0);
        }
        let mirrored =
            [v([0.0, 0.0, 0.0], [0.0, 0.0]), v([10.0, 0.0, 0.0], [1.0, 0.0]), v([0.0, 10.0, 0.0], [0.0, 1.0])];
        assert!(compute_tangents(&mirrored, &[0, 1, 2]).iter().all(|t| t[3] == 1.0));
    }

    #[test]
    fn tangents_are_orthogonal_to_smooth_normals() {
        let mut verts =
            [v([0.0, 0.0, 0.0], [0.0, 0.0]), v([10.0, 0.0, 0.0], [1.0, 0.0]), v([0.0, 10.0, 0.0], [0.0, 1.0])];
        let n = Vec3::new(0.3, 0.2, 1.0).normalize();
        for x in &mut verts {
            x.normal = n.to_array();
        }
        for t in compute_tangents(&verts, &[0, 1, 2]) {
            let t3 = Vec3::new(t[0], t[1], t[2]);
            assert!(t3.dot(n).abs() < 1e-5);
            assert!((t3.length() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn degenerate_input_still_gives_unit_tangents() {
        let verts = [v([0.0; 3], [0.0; 2]), v([0.0; 3], [0.0; 2]), v([f32::NAN, 0.0, 0.0], [0.0, 0.0])];
        let t = compute_tangents(&verts, &[0, 1, 2, 0, 1, 99]);
        assert_eq!(t.len(), 3);
        for tan in t {
            let l = Vec3::new(tan[0], tan[1], tan[2]).length();
            assert!((l - 1.0).abs() < 1e-5, "{tan:?}");
        }
        assert_eq!(pack_tangent([1.0, 0.0, 0.0, -1.0]) >> 24, 0x81);
        assert_eq!(pack_tangent([0.0, 0.0, 1.0, 1.0]) & 0xff, 0);
    }
}
