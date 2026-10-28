use glam::Vec3;

use crate::camera::Camera;
use crate::gpu::LightGpu;
use crate::types::{LightFalloff, LightKind, PreviewLight};

pub(crate) const MAX_LIGHTS: usize = 1024;
pub(crate) const MAX_DIRECTIONAL: usize = 8;
pub(crate) const TILE_PX: u32 = 64;
pub(crate) const SLICES: u32 = 16;
const MAX_PER_CLUSTER: u32 = 128;
const MAX_REFS: usize = 1 << 21;
const CLUSTER_FAR: f32 = 32768.0;

pub(crate) const KIND_POINT: f32 = 0.0;
pub(crate) const KIND_SPOT: f32 = 1.0;
pub(crate) const KIND_DIRECTIONAL: f32 = 2.0;

const DEFAULT_ATTEN: [f32; 4] = [0.0, 0.0, 1.0e-4, 1.0e9];

fn gpu_cone(f: Option<&LightFalloff>) -> [f32; 4] {
    match f {
        Some(f) => {
            let e = if f.exponent.is_finite() { f.exponent.clamp(0.0, 64.0) } else { 0.0 };
            [e, 1.0, 0.0, 0.0]
        }
        None => [0.0; 4],
    }
}

fn gpu_falloff(f: Option<&LightFalloff>) -> ([f32; 4], f32, [f32; 2]) {
    let Some(f) = f else { return (DEFAULT_ATTEN, 0.0, [0.0; 2]) };
    let term = |v: f32| if v.is_finite() { v.clamp(0.0, 1.0e9) } else { 0.0 };
    let (c, l, q) = (term(f.constant), term(f.linear), term(f.quadratic));
    if c + l + q <= 0.0 {
        return (DEFAULT_ATTEN, 0.0, [0.0; 2]);
    }
    let cap = if f.cap.is_finite() && f.cap >= 1.0 { f.cap.min(1.0e9) } else { 1.0e9 };
    let cutoff = if f.cutoff.is_finite() { f.cutoff.clamp(0.0, 1.0e9) } else { 0.0 };
    let fade =
        if f.fade.iter().all(|v| v.is_finite() && *v >= 0.0) && f.fade[1] > f.fade[0] { f.fade } else { [0.0; 2] };
    ([c, l, q, cap], cutoff, fade)
}

fn finite3(v: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    if v.iter().all(|c| c.is_finite()) { v } else { fallback }
}

pub(crate) fn pack_lights(lights: &[PreviewLight]) -> (Vec<LightGpu>, u32) {
    let mut dir = Vec::new();
    let mut local = Vec::new();
    for l in lights.iter().take(MAX_LIGHTS) {
        let intensity = if l.intensity.is_finite() { l.intensity.max(0.0) } else { 0.0 };
        let c = finite3(l.color, [0.0; 3]);
        let color = [c[0].max(0.0) * intensity, c[1].max(0.0) * intensity, c[2].max(0.0) * intensity];
        let pos = finite3(l.pos, [0.0; 3]);
        let range = if l.range.is_finite() { l.range.clamp(1.0, 1.0e6) } else { 1.0 };
        let (atten, cutoff, fade) = gpu_falloff(l.falloff.as_ref());
        let extra = gpu_cone(l.falloff.as_ref());
        match l.kind {
            LightKind::Point => local.push(LightGpu {
                pos_range: [pos[0], pos[1], pos[2], range],
                color: [color[0], color[1], color[2], KIND_POINT],
                dir: [0.0, 0.0, -1.0, cutoff],
                cone: [-1.0, -1.0, fade[0], fade[1]],
                atten,
                extra,
            }),
            LightKind::Spot { dir: d, inner_deg, outer_deg } => {
                let d = Vec3::from(finite3(d, [0.0, 0.0, -1.0])).normalize_or(Vec3::NEG_Z);
                let outer = if outer_deg.is_finite() { outer_deg.clamp(0.1, 179.0) } else { 45.0 };
                let inner = if inner_deg.is_finite() { inner_deg.clamp(0.0, outer) } else { outer * 0.5 };
                let (ci, co) = (inner.to_radians().cos(), outer.to_radians().cos());
                let ci = ci.max(co + 1e-4);
                local.push(LightGpu {
                    pos_range: [pos[0], pos[1], pos[2], range],
                    color: [color[0], color[1], color[2], KIND_SPOT],
                    dir: [d.x, d.y, d.z, cutoff],
                    cone: [ci, co, fade[0], fade[1]],
                    atten,
                    extra,
                });
            }
            LightKind::Directional { dir: d } => {
                if dir.len() < MAX_DIRECTIONAL {
                    let d = Vec3::from(finite3(d, [0.0, 0.0, -1.0])).normalize_or(Vec3::NEG_Z);
                    dir.push(LightGpu {
                        pos_range: [0.0; 4],
                        color: [color[0], color[1], color[2], KIND_DIRECTIONAL],
                        dir: [d.x, d.y, d.z, 0.0],
                        cone: [0.0; 4],
                        atten: DEFAULT_ATTEN,
                        extra: [0.0; 4],
                    });
                }
            }
        }
    }
    let n_dir = dir.len() as u32;
    dir.extend(local);
    (dir, n_dir)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ClusterGrid {
    pub tiles_x: u32,
    pub tiles_y: u32,
    pub slices: u32,
    pub near: f32,
    pub far: f32,
    pub scale: f32,
}

impl ClusterGrid {
    pub fn new(cam: &Camera) -> Self {
        let tiles_x = (cam.width as u32).div_ceil(TILE_PX).max(1);
        let tiles_y = (cam.height as u32).div_ceil(TILE_PX).max(1);
        let near = cam.near.max(1e-3);
        let far = cam.far.min(CLUSTER_FAR).max(near * 2.0);
        let scale = SLICES as f32 / (far / near).ln();
        Self { tiles_x, tiles_y, slices: SLICES, near, far, scale }
    }

    pub fn count(&self) -> usize {
        (self.tiles_x * self.tiles_y * self.slices) as usize
    }

    fn slice(&self, depth: f32) -> u32 {
        if depth <= self.near {
            return 0;
        }
        ((depth / self.near).ln() * self.scale).max(0.0).min((self.slices - 1) as f32) as u32
    }
}

pub(crate) fn build_clusters(
    cam: &Camera,
    grid: &ClusterGrid,
    lights: &[LightGpu],
    first_local: u32,
) -> (Vec<[u32; 2]>, Vec<u32>) {
    let n = grid.count();
    let mut counts = vec![0u32; n];
    let aspect = cam.width / cam.height;
    let (tx, ty) = (cam.tan_half_fov * aspect, cam.tan_half_fov);
    let mut spans: Vec<(u32, [u32; 6])> = Vec::new();
    for (i, l) in lights.iter().enumerate().skip(first_local as usize) {
        let p = Vec3::new(l.pos_range[0], l.pos_range[1], l.pos_range[2]);
        let r = l.pos_range[3];
        let d = p - cam.eye;
        let (vx, vy, vz) = (d.dot(cam.right), d.dot(cam.up), d.dot(cam.forward));
        if vz + r < grid.near || vz - r > cam.far {
            continue;
        }
        let (s0, s1) = (grid.slice(vz - r), grid.slice(vz + r));
        let (x0, x1, y0, y1) = if vz - r <= grid.near {
            (0, grid.tiles_x - 1, 0, grid.tiles_y - 1)
        } else {
            let (zn, zf) = (vz - r, vz + r);
            let lo = |v: f32| if v < 0.0 { v / zn } else { v / zf };
            let hi = |v: f32| if v > 0.0 { v / zn } else { v / zf };
            let nx0 = lo(vx - r) / tx;
            let nx1 = hi(vx + r) / tx;
            let ny0 = lo(vy - r) / ty;
            let ny1 = hi(vy + r) / ty;
            if nx1 < -1.0 || nx0 > 1.0 || ny1 < -1.0 || ny0 > 1.0 {
                continue;
            }
            let to_px_x = |nx: f32| ((nx.clamp(-1.0, 1.0) + 1.0) * 0.5 * cam.width) as u32 / TILE_PX;
            let to_px_y = |ny: f32| ((1.0 - ny.clamp(-1.0, 1.0)) * 0.5 * cam.height) as u32 / TILE_PX;
            (
                to_px_x(nx0).min(grid.tiles_x - 1),
                to_px_x(nx1).min(grid.tiles_x - 1),
                to_px_y(ny1).min(grid.tiles_y - 1),
                to_px_y(ny0).min(grid.tiles_y - 1),
            )
        };
        spans.push((i as u32, [x0, x1, y0, y1, s0, s1]));
    }
    let idx = |x: u32, y: u32, s: u32| ((s * grid.tiles_y + y) * grid.tiles_x + x) as usize;
    let mut total = 0usize;
    for (_, [x0, x1, y0, y1, s0, s1]) in &spans {
        for s in *s0..=*s1 {
            for y in *y0..=*y1 {
                for x in *x0..=*x1 {
                    if let Some(c) = counts.get_mut(idx(x, y, s))
                        && *c < MAX_PER_CLUSTER
                        && total < MAX_REFS
                    {
                        *c += 1;
                        total += 1;
                    }
                }
            }
        }
    }
    let mut table = vec![[0u32; 2]; n];
    let mut offset = 0u32;
    for (t, c) in table.iter_mut().zip(&counts) {
        *t = [offset, 0];
        offset += c;
    }
    let mut list = vec![0u32; total];
    for (li, [x0, x1, y0, y1, s0, s1]) in &spans {
        for s in *s0..=*s1 {
            for y in *y0..=*y1 {
                for x in *x0..=*x1 {
                    let ci = idx(x, y, s);
                    let (Some(t), Some(&cap)) = (table.get_mut(ci), counts.get(ci)) else { continue };
                    if t[1] < cap
                        && let Some(slot) = list.get_mut((t[0] + t[1]) as usize)
                    {
                        *slot = *li;
                        t[1] += 1;
                    }
                }
            }
        }
    }
    (table, list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{RenderMode, ViewKind, ViewParams};

    fn cam() -> Camera {
        Camera::new(&ViewParams {
            size: [640, 480],
            kind: ViewKind::Perspective {
                eye: [0.0; 3],
                forward: [1.0, 0.0, 0.0],
                up: [0.0, 0.0, 1.0],
                fov_y_deg: 90.0,
                near: 1.0,
                far: 10000.0,
            },
            mode: RenderMode::Lit,
            grid: None,
            background: [0.0; 3],
            selection_color: [1.0; 4],
            show_sprites: false,
            show_models: false,
            msaa: false,
        })
    }

    fn point(pos: [f32; 3], range: f32) -> PreviewLight {
        PreviewLight { kind: LightKind::Point, pos, color: [1.0; 3], intensity: 1.0, range, falloff: None }
    }

    #[test]
    fn directional_first_and_capped() {
        let mut lights = vec![point([0.0; 3], 100.0)];
        for _ in 0..20 {
            lights.push(PreviewLight {
                kind: LightKind::Directional { dir: [0.0, 0.0, -1.0] },
                pos: [0.0; 3],
                color: [1.0; 3],
                intensity: 1.0,
                range: 0.0,
                falloff: None,
            });
        }
        let (packed, n_dir) = pack_lights(&lights);
        assert_eq!(n_dir as usize, MAX_DIRECTIONAL);
        assert_eq!(packed.len(), MAX_DIRECTIONAL + 1);
        assert_eq!(packed[MAX_DIRECTIONAL].color[3], KIND_POINT);
        let many: Vec<PreviewLight> = (0..5000).map(|_| point([0.0; 3], 1.0)).collect();
        assert_eq!(pack_lights(&many).0.len(), MAX_LIGHTS);
    }

    #[test]
    fn light_in_front_is_assigned_to_center_clusters() {
        let cam = cam();
        let grid = ClusterGrid::new(&cam);
        let (lights, n_dir) = pack_lights(&[point([500.0, 0.0, 0.0], 50.0), point([-500.0, 0.0, 0.0], 50.0)]);
        let (table, list) = build_clusters(&cam, &grid, &lights, n_dir);
        assert_eq!(table.len(), grid.count());
        assert!(!list.is_empty());
        assert!(list.iter().all(|&i| i == 0));
        let s = grid.slice(500.0);
        let center = ((s * grid.tiles_y + grid.tiles_y / 2) * grid.tiles_x + grid.tiles_x / 2) as usize;
        assert_eq!(table[center][1], 1);
        assert_eq!(table[(s * grid.tiles_y * grid.tiles_x) as usize][1], 0);
    }

    #[test]
    fn light_around_camera_covers_screen() {
        let cam = cam();
        let grid = ClusterGrid::new(&cam);
        let (lights, n_dir) = pack_lights(&[point([0.0; 3], 200.0)]);
        let (table, _) = build_clusters(&cam, &grid, &lights, n_dir);
        assert_eq!(table[0][1], 1);
        assert_eq!(table[(grid.tiles_x * grid.tiles_y - 1) as usize][1], 1);
    }

    #[test]
    fn hostile_values_do_not_panic() {
        let cam = cam();
        let grid = ClusterGrid::new(&cam);
        let lights = [PreviewLight {
            kind: LightKind::Spot { dir: [f32::NAN; 3], inner_deg: f32::NAN, outer_deg: -5.0 },
            pos: [f32::INFINITY, 0.0, 0.0],
            color: [f32::NAN; 3],
            intensity: f32::NAN,
            range: f32::NAN,
            falloff: None,
        }];
        let (packed, n_dir) = pack_lights(&lights);
        assert!(packed.iter().all(|l| l.pos_range.iter().all(|v| v.is_finite())));
        let _ = build_clusters(&cam, &grid, &packed, n_dir);
    }

    #[test]
    fn vrad_falloff_terms_reach_the_gpu() {
        let f = LightFalloff {
            constant: 0.0,
            linear: 1.0,
            quadratic: 0.5,
            cap: 4000.0,
            cutoff: 900.0,
            fade: [100.0, 600.0],
            exponent: 2.0,
        };
        let mut l = point([0.0; 3], 1000.0);
        l.falloff = Some(f);
        let (packed, _) = pack_lights(&[l]);
        assert_eq!(packed[0].atten, [0.0, 1.0, 0.5, 4000.0]);
        assert_eq!((packed[0].dir[3], packed[0].cone[2], packed[0].cone[3]), (900.0, 100.0, 600.0));
        assert_eq!(packed[0].extra, [2.0, 1.0, 0.0, 0.0]);
        let (packed, _) = pack_lights(&[point([0.0; 3], 10.0)]);
        assert_eq!(packed[0].atten, DEFAULT_ATTEN);
        let hostile = LightFalloff {
            constant: f32::NAN,
            linear: -1.0,
            quadratic: f32::INFINITY,
            cap: -5.0,
            cutoff: f32::NAN,
            fade: [5.0, 1.0],
            exponent: f32::NAN,
        };
        assert_eq!(gpu_falloff(Some(&hostile)), (DEFAULT_ATTEN, 0.0, [0.0; 2]));
        assert_eq!(gpu_cone(Some(&hostile)), [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(gpu_cone(None), [0.0; 4]);
        let zero = LightFalloff::default();
        assert_eq!(gpu_falloff(Some(&zero)).0, DEFAULT_ATTEN, "all zero terms would divide by zero");
        let capped = LightFalloff { quadratic: 1.0, cap: f32::NAN, ..zero };
        assert_eq!(gpu_falloff(Some(&capped)).0, [0.0, 0.0, 1.0, 1.0e9]);
    }
}
