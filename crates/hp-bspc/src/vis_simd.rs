#![allow(unsafe_code)]

use std::sync::OnceLock;

use super::{CENTER_SLACK, PRUNE_MARGIN, Plane};
use glam::DVec3;

pub(super) const LANES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Isa {
    Scalar,
    Sse,
    Avx2,
    Avx512,
}

pub fn isa() -> Isa {
    static ISA: OnceLock<Isa> = OnceLock::new();
    *ISA.get_or_init(|| {
        let forced = std::env::var("HP_VIS_SIMD").ok().map(|v| v.to_ascii_lowercase());
        let best = detect();
        match forced.as_deref() {
            Some("scalar") => Isa::Scalar,
            Some("sse") => Isa::Sse,
            Some("avx2") if best != Isa::Sse && best != Isa::Scalar => Isa::Avx2,
            Some("avx512") if best == Isa::Avx512 => Isa::Avx512,
            _ => best,
        }
    })
}

fn detect() -> Isa {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx512f") {
            return Isa::Avx512;
        }
        if std::arch::is_x86_feature_detected!("avx2") {
            return Isa::Avx2;
        }
    }
    Isa::Sse
}

#[derive(Clone, Copy, Default)]
pub(super) struct Spheres {
    pub x: [f64; LANES],
    pub y: [f64; LANES],
    pub z: [f64; LANES],
    pub r: [f64; LANES],
    pub len: usize,
}

impl Spheres {
    #[inline]
    pub fn push(&mut self, (o, r): (DVec3, f64)) {
        let k = self.len;
        self.x[k] = o.x;
        self.y[k] = o.y;
        self.z[k] = o.z;
        self.r[k] = r;
        self.len += 1;
    }
}

#[inline(always)]
#[allow(clippy::needless_range_loop)]
fn sphere_masks_body(planes: [&[Plane]; 2], s: &Spheres) -> (u32, u32) {
    let mut lim = [0.0f64; LANES];
    for k in 0..LANES {
        lim[k] = -(s.r[k] + PRUNE_MARGIN);
    }
    let near = CENTER_SLACK - PRUNE_MARGIN;
    let mut behind = [false; LANES];
    let mut points = [false; LANES];
    for pl in planes[0].iter().chain(planes[1]) {
        let (nx, ny, nz, d0) = (pl.n.x, pl.n.y, pl.n.z, pl.d);
        for k in 0..LANES {
            let d = nx * s.x[k] + ny * s.y[k] + nz * s.z[k] - d0;
            behind[k] |= d < lim[k];
            points[k] |= d < s.r[k] && d < near;
        }
    }
    let (mut b, mut p) = (0u32, 0u32);
    for k in 0..s.len.min(LANES) {
        b |= u32::from(behind[k]) << k;
        p |= u32::from(points[k]) << k;
    }
    (b, p)
}

fn sphere_masks_plain(planes: [&[Plane]; 2], s: &Spheres) -> (u32, u32) {
    sphere_masks_body(planes, s)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
fn sphere_masks_avx2(planes: [&[Plane]; 2], s: &Spheres) -> (u32, u32) {
    sphere_masks_body(planes, s)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
fn sphere_masks_avx512(planes: [&[Plane]; 2], s: &Spheres) -> (u32, u32) {
    sphere_masks_body(planes, s)
}

fn sphere_masks_scalar(planes: [&[Plane]; 2], s: &Spheres) -> (u32, u32) {
    let (mut b, mut p) = (0u32, 0u32);
    'spheres: for k in 0..s.len.min(LANES) {
        let lim = -(s.r[k] + PRUNE_MARGIN);
        for pl in planes[0].iter().chain(planes[1]) {
            let d = pl.n.x * s.x[k] + pl.n.y * s.y[k] + pl.n.z * s.z[k] - pl.d;
            if d < lim {
                b |= 1 << k;
                continue 'spheres;
            }
            if d < s.r[k] && d < CENTER_SLACK - PRUNE_MARGIN {
                p |= 1 << k;
            }
        }
    }
    (b, p)
}

#[inline]
pub(super) fn sphere_masks(isa: Isa, planes: [&[Plane]; 2], s: &Spheres) -> (u32, u32) {
    match isa {
        Isa::Scalar => sphere_masks_scalar(planes, s),
        Isa::Sse => sphere_masks_plain(planes, s),
        #[cfg(target_arch = "x86_64")]
        Isa::Avx2 => unsafe { sphere_masks_avx2(planes, s) },
        #[cfg(target_arch = "x86_64")]
        Isa::Avx512 => unsafe { sphere_masks_avx512(planes, s) },
        #[cfg(not(target_arch = "x86_64"))]
        _ => sphere_masks_plain(planes, s),
    }
}
