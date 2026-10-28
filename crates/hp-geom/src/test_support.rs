use glam::DVec3;
use hp_vmf::{Side, TexAxis};

pub fn hammer_box(min: DVec3, max: DVec3) -> Vec<Side> {
    let (x0, y0, z0) = (min.x, min.y, min.z);
    let (x1, y1, z1) = (max.x, max.y, max.z);
    let v = DVec3::new;
    let floor = (DVec3::X, DVec3::NEG_Y);
    let wall_y = (DVec3::X, DVec3::NEG_Z);
    let wall_x = (DVec3::Y, DVec3::NEG_Z);
    let faces = [
        ([v(x0, y1, z1), v(x1, y1, z1), v(x1, y0, z1)], floor),
        ([v(x0, y0, z0), v(x1, y0, z0), v(x1, y1, z0)], floor),
        ([v(x1, y1, z1), v(x0, y1, z1), v(x0, y1, z0)], wall_y),
        ([v(x1, y0, z0), v(x0, y0, z0), v(x0, y0, z1)], wall_y),
        ([v(x1, y1, z0), v(x1, y0, z0), v(x1, y0, z1)], wall_x),
        ([v(x0, y1, z1), v(x0, y0, z1), v(x0, y0, z0)], wall_x),
    ];
    faces
        .iter()
        .enumerate()
        .map(|(i, (plane, (u, w)))| Side {
            id: i as u32 + 1,
            plane: *plane,
            material: format!("TEST/FACE{i}"),
            uaxis: TexAxis { axis: *u, shift: 0.0, scale: 0.25 },
            vaxis: TexAxis { axis: *w, shift: 0.0, scale: 0.25 },
            ..Default::default()
        })
        .collect()
}

pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    pub fn point(&mut self, half: f64) -> DVec3 {
        DVec3::new(self.range(-half, half), self.range(-half, half), self.range(-half, half))
    }
}

pub fn id_counter(start: u32) -> impl FnMut() -> u32 {
    let mut next = start;
    move || {
        next += 1;
        next
    }
}
