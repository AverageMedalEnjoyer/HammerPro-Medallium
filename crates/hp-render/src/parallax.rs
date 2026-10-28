use glam::Vec3;

pub const PARALLAX_WGSL: &str = include_str!("shaders/parallax.wgsl");

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParallaxBox {
    pub rows: [[f32; 4]; 3],
    pub origin: [f32; 3],
}

impl ParallaxBox {
    pub fn axis_aligned(lo: Vec3, hi: Vec3, origin: Vec3) -> ParallaxBox {
        let size = (hi - lo).max(Vec3::splat(1e-3));
        let row = |i: usize| {
            let mut r = [0.0; 4];
            r[i] = 1.0 / size[i];
            r[3] = -lo[i] / size[i];
            r
        };
        ParallaxBox { rows: [row(0), row(1), row(2)], origin: origin.to_array() }
    }

    pub fn gpu(&self) -> [[f32; 4]; 4] {
        [self.rows[0], self.rows[1], self.rows[2], [self.origin[0], self.origin[1], self.origin[2], 1.0]]
    }
}

pub fn parallax_reflection(world: Vec3, r: Vec3, b: &ParallaxBox) -> Vec3 {
    let row = |i: usize| Vec3::new(b.rows[i][0], b.rows[i][1], b.rows[i][2]);
    let pos =
        Vec3::new(row(0).dot(world) + b.rows[0][3], row(1).dot(world) + b.rows[1][3], row(2).dot(world) + b.rows[2][3]);
    let ray = Vec3::new(row(0).dot(r), row(1).dot(r), row(2).dot(r));
    let first = (Vec3::ONE - pos) / ray;
    let second = -pos / ray;
    let far = first.max(second);
    let distance = far.x.min(far.y).min(far.z);
    if !distance.is_finite() || distance <= 0.0 {
        return r;
    }
    let hit = world + r * distance;
    hit - Vec3::from_array(b.origin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ray_meets_the_far_wall() {
        let b = ParallaxBox::axis_aligned(Vec3::ZERO, Vec3::new(256.0, 256.0, 128.0), Vec3::new(128.0, 128.0, 64.0));
        let world = Vec3::new(32.0, 128.0, 0.0);
        let r = Vec3::new(1.0, 0.0, 0.25);
        let d = parallax_reflection(world, r, &b);
        let hit = Vec3::new(128.0, 128.0, 64.0) + d;
        assert!((hit.x - 256.0).abs() < 1e-3, "{hit}");
        assert!((hit.z - 56.0).abs() < 1e-3, "{hit}");
        assert!(d.normalize().dot(r.normalize()) < 0.999);
        let d = parallax_reflection(Vec3::from_array(b.origin), Vec3::new(0.0, 1.0, 0.0), &b);
        assert!(d.normalize().abs_diff_eq(Vec3::Y, 1e-5), "{d}");
    }

    #[test]
    fn the_wgsl_has_the_same_function() {
        assert!(PARALLAX_WGSL.contains("fn parallax_reflection("));
        assert!(!PARALLAX_WGSL.contains('\u{2014}') && !PARALLAX_WGSL.contains('\u{2013}'));
        let b = ParallaxBox::axis_aligned(Vec3::ZERO, Vec3::ONE, Vec3::splat(0.5));
        assert_eq!(b.gpu()[3], [0.5, 0.5, 0.5, 1.0]);
        let module = wgpu::naga::front::wgsl::parse_str(PARALLAX_WGSL).expect("the WGSL parses");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("the WGSL is valid");
    }
}
