use glam::{DVec3, Mat4, Vec3, Vec4};

use crate::camera::{Camera, aabb_inside, frustum_planes, mat_from_rows};
use crate::gpu::ViewUniform;
use crate::pipelines::{DEPTH_FORMAT, MeshPass, ShadowPipelines};
use crate::scene::ArenaKind;
use crate::{MaterialFlags, Renderer, classify};

pub(crate) const SHADOW_SIZE: u32 = 4096;
pub(crate) const SHADOW_DISTANCE: f32 = 8192.0;
const SIZE_STEP: f32 = 512.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShadowFit {
    pub view_proj: Mat4,
    pub texel_world: f32,
    pub depth_range: f32,
}

impl ShadowFit {
    pub fn params(&self) -> [f32; 4] {
        [1.0, 2.0 / self.depth_range.max(1.0), 1.0 / SHADOW_SIZE as f32, self.texel_world * 1.5]
    }
}

pub(crate) fn fit(cam: &Camera, dir: Vec3, scene_min: Vec3, scene_max: Vec3) -> Option<ShadowFit> {
    if !(scene_min.is_finite() && scene_max.is_finite() && scene_min.cmple(scene_max).all()) {
        return None;
    }
    let far = cam.far.min(SHADOW_DISTANCE);
    let aspect = cam.width / cam.height;
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for d in [cam.near, far] {
        let hh = d * cam.tan_half_fov;
        let hw = hh * aspect;
        let c = cam.eye + cam.forward * d;
        for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = c + cam.right * (hw * sx) + cam.up * (hh * sy);
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    let (rlo, rhi) = (lo.max(scene_min), hi.min(scene_max));
    if !rlo.cmple(rhi).all() {
        return None;
    }
    let f = dir.normalize_or(Vec3::NEG_Z).as_dvec3();
    let hint = if f.z.abs() < 0.99 { DVec3::Z } else { DVec3::Y };
    let r = f.cross(hint).normalize();
    let u = r.cross(f);
    let corners = |a: Vec3, b: Vec3| {
        (0..8).map(move |i| {
            DVec3::new(
                if i & 1 != 0 { f64::from(b.x) } else { f64::from(a.x) },
                if i & 2 != 0 { f64::from(b.y) } else { f64::from(a.y) },
                if i & 4 != 0 { f64::from(b.z) } else { f64::from(a.z) },
            )
        })
    };
    let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for p in corners(rlo, rhi) {
        x0 = x0.min(p.dot(r));
        x1 = x1.max(p.dot(r));
        y0 = y0.min(p.dot(u));
        y1 = y1.max(p.dot(u));
    }
    let (mut d0, mut d1) = (f64::MAX, f64::MIN);
    for p in corners(scene_min, scene_max) {
        d0 = d0.min(p.dot(f));
        d1 = d1.max(p.dot(f));
    }
    let step = f64::from(SIZE_STEP);
    let size = ((x1 - x0).max(y1 - y0) + 64.0).div_euclid(step) * step + step;
    let texel = size / f64::from(SHADOW_SIZE);
    let snap = |v: f64| (v / texel).floor() * texel;
    let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    let (ox, oy) = (snap(cx - size * 0.5), snap(cy - size * 0.5));
    let (d0, d1) = (d0 - 64.0, d1 + 64.0);
    let range = d1 - d0;
    let k = 2.0 / size;
    let rows = [
        [r.x * k, r.y * k, r.z * k, -ox * k - 1.0],
        [u.x * k, u.y * k, u.z * k, -oy * k - 1.0],
        [f.x / range, f.y / range, f.z / range, -d0 / range],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let view_proj = mat_from_rows(rows);
    view_proj.is_finite().then_some(ShadowFit { view_proj, texel_world: texel as f32, depth_range: range as f32 })
}

fn casts_no_shadow(desc: Option<&crate::MaterialDesc>) -> bool {
    desc.is_some_and(|d| d.flags.intersects(MaterialFlags::SKY | MaterialFlags::NODRAW_IN_3D))
}

pub(crate) struct ShadowGpu {
    pub view: wgpu::TextureView,
    uniform: wgpu::Buffer,
    bind_group: Option<(u64, wgpu::BindGroup)>,
    pipelines: ShadowPipelines,
    key: Option<([f32; 16], u64)>,
}

impl Renderer {
    pub(crate) fn prepare_shadow(&mut self, cam: &Camera) -> Option<(ShadowFit, bool)> {
        let sun = self.lights_cpu.first().filter(|_| self.dir_lights > 0)?;
        let dir = Vec3::new(sun.dir[0], sun.dir[1], sun.dir[2]);
        let (mut lo, mut hi) = self.instance_bounds.unwrap_or((Vec3::INFINITY, Vec3::NEG_INFINITY));
        for ch in self.scene.chunks.iter().flatten() {
            if !ch.is_edge() && ch.used_i > 0 {
                lo = lo.min(ch.aabb_min);
                hi = hi.max(ch.aabb_max);
            }
        }
        let fit = fit(cam, dir, lo, hi)?;
        if self.shadow.is_none() {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hp-render shadow map"),
                size: wgpu::Extent3d { width: SHADOW_SIZE, height: SHADOW_SIZE, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let uniform = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render shadow uniform"),
                size: size_of::<ViewUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.shadow = Some(ShadowGpu {
                view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
                uniform,
                bind_group: None,
                pipelines: ShadowPipelines::new(&self.device, &self.shared),
                key: None,
            });
            self.scene_gen += 1;
        }
        let key = (fit.view_proj.to_cols_array(), self.shadow_epoch);
        let scene_gen = self.scene_gen;
        let shadow = self.shadow.as_mut()?;
        let dirty = shadow.key != Some(key);
        if dirty {
            let u = ViewUniform {
                view_proj: fit.view_proj.to_cols_array_2d(),
                eye: [0.0; 4],
                viewport: [1.0; 4],
                opts: [crate::OPT_SHADOW_PASS, 0, 0, 0],
                ..Default::default()
            };
            self.queue.write_buffer(&shadow.uniform, 0, bytemuck::bytes_of(&u));
            shadow.key = Some(key);
        }
        if shadow.bind_group.as_ref().is_none_or(|(g, _)| *g != scene_gen) {
            let shadow = self.shadow.as_ref()?;
            let bg = self.shadow_pass_bind_group(&shadow.uniform, "hp-render shadow pass");
            self.shadow.as_mut()?.bind_group = Some((scene_gen, bg));
        }
        Some((fit, dirty))
    }

    pub(crate) fn shadow_pass_bind_group(&self, uniform: &wgpu::Buffer, label: &str) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.shared.view_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.states.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.sampler_clamp) },
                wgpu::BindGroupEntry { binding: 4, resource: self.lights_buf.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: self.dummy_storage.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: self.dummy_storage.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&self.shadow_dummy) },
                wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::Sampler(&self.shadow_sampler) },
                wgpu::BindGroupEntry { binding: 9, resource: self.dummy_storage.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 10, resource: self.dummy_storage.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 11, resource: self.light_styles.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 12, resource: wgpu::BindingResource::TextureView(self.cubes.view()) },
                wgpu::BindGroupEntry { binding: 13, resource: self.cubes.info().as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: wgpu::BindingResource::TextureView(&self.local_shadows.dummy),
                },
            ],
        })
    }

    pub(crate) fn record_shadow(&self, enc: &mut wgpu::CommandEncoder, fit: &ShadowFit) -> (u32, u64) {
        let Some(shadow) = self.shadow.as_ref() else { return (0, 0) };
        let Some((_, bind_group)) = shadow.bind_group.as_ref() else { return (0, 0) };
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("hp-render shadow pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &shadow.view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, bind_group, &[]);
        let planes: [Vec4; 6] = frustum_planes(&fit.view_proj);
        self.draw_shadow_casters(&mut pass, &shadow.pipelines, &planes)
    }

    pub(crate) fn draw_shadow_casters(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipelines: &ShadowPipelines,
        planes: &[Vec4; 6],
    ) -> (u32, u64) {
        let (mut draws, mut tris) = (0u32, 0u64);
        pass.set_vertex_buffer(0, self.arenas[ArenaKind::MeshVerts as usize].buffer.slice(..));
        pass.set_index_buffer(self.arenas[ArenaKind::Indices as usize].buffer.slice(..), wgpu::IndexFormat::Uint32);
        let mut current: Option<bool> = None;
        for (mi, m) in self.scene.materials.iter().enumerate() {
            if casts_no_shadow(m.desc.as_ref()) {
                continue;
            }
            let alpha = match classify(m.desc.as_ref()) {
                Some((MeshPass::Opaque, _)) => false,
                Some((MeshPass::AlphaTest, _)) => true,
                _ => continue,
            };
            let mut bound = false;
            for &c in &m.chunks {
                let Some(Some(ch)) = self.scene.chunks.get(c as usize) else { continue };
                if ch.used_i == 0 || ch.idx.len == 0 || !aabb_inside(planes, ch.aabb_min, ch.aabb_max) {
                    continue;
                }
                if current != Some(alpha) {
                    pass.set_pipeline(if alpha { &pipelines.chunk_alpha } else { &pipelines.chunk });
                    current = Some(alpha);
                }
                if alpha
                    && !bound
                    && let Some(g) = self.mat_gpu.get(mi)
                {
                    pass.set_bind_group(1, &g.bind_group, &[]);
                    bound = true;
                }
                pass.draw_indexed(ch.idx.offset..ch.idx.offset + ch.used_i, 0, 0..1);
                draws += 1;
                tris += u64::from(ch.used_i / 3);
            }
        }
        if self.instance_count > 0 {
            let mut current: Option<bool> = None;
            pass.set_vertex_buffer(1, self.instances.buffer.slice(..));
            for g in &self.instance_groups {
                let Some(model) = self.models.get(&g.key) else { continue };
                let mut bound = false;
                for part in &model.parts {
                    let desc = self.scene.materials.get(part.material as usize).and_then(|m| m.desc);
                    if casts_no_shadow(desc.as_ref()) {
                        continue;
                    }
                    let alpha = match classify(desc.as_ref()) {
                        Some((MeshPass::Opaque, _)) => false,
                        Some((MeshPass::AlphaTest, _)) => true,
                        _ => continue,
                    };
                    if alpha {
                        let Some(mat) = self.mat_gpu.get(part.material as usize) else { continue };
                        pass.set_bind_group(1, &mat.bind_group, &[]);
                    }
                    if current != Some(alpha) {
                        pass.set_pipeline(if alpha { &pipelines.model_alpha } else { &pipelines.model });
                        current = Some(alpha);
                    }
                    if !bound {
                        pass.set_vertex_buffer(0, model.vbuf.slice(..));
                        pass.set_vertex_buffer(2, model.tbuf.slice(..));
                        pass.set_index_buffer(model.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        bound = true;
                    }
                    pass.draw_indexed(part.first..part.first + part.count, 0, g.start..g.start + g.count);
                    draws += 1;
                    tris += u64::from(part.count / 3) * u64::from(g.count);
                }
            }
        }
        (draws, tris)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{RenderMode, ViewKind, ViewParams};

    fn cam(eye: [f32; 3], forward: [f32; 3]) -> Camera {
        Camera::new(&ViewParams {
            size: [800, 600],
            kind: ViewKind::Perspective { eye, forward, up: [0.0, 0.0, 1.0], fov_y_deg: 70.0, near: 4.0, far: 30000.0 },
            mode: RenderMode::Lit,
            grid: None,
            background: [0.0; 3],
            selection_color: [1.0; 4],
            show_sprites: false,
            show_models: false,
            msaa: false,
        })
    }

    #[test]
    fn fit_contains_visible_scene() {
        let c = cam([0.0, -2000.0, 500.0], [0.0, 1.0, -0.3]);
        let (smin, smax) = (Vec3::new(-5000.0, -5000.0, -100.0), Vec3::new(5000.0, 5000.0, 1000.0));
        let f = fit(&c, Vec3::new(0.3, 0.2, -1.0), smin, smax).expect("fit");
        let p = f.view_proj * Vec4::new(0.0, 0.0, 0.0, 1.0);
        assert!(p.x.abs() < 1.0 && p.y.abs() < 1.0 && (0.0..=1.0).contains(&p.z), "{p:?}");
        let q = f.view_proj * Vec4::new(-30.0, -20.0, 100.0, 1.0);
        assert!(q.z < p.z);
        for i in 0..8 {
            let v = Vec3::new(
                if i & 1 != 0 { smax.x } else { smin.x },
                if i & 2 != 0 { smax.y } else { smin.y },
                if i & 4 != 0 { smax.z } else { smin.z },
            );
            let z = (f.view_proj * v.extend(1.0)).z;
            assert!((0.0..=1.0).contains(&z), "corner {v:?} depth {z}");
        }
        assert!(f.texel_world > 0.0 && f.texel_world < 8.0);
    }

    #[test]
    fn fit_is_stable_and_handles_degenerate_input() {
        let (smin, smax) = (Vec3::splat(-1000.0), Vec3::splat(1000.0));
        let a = fit(&cam([0.0; 3], [1.0, 0.0, 0.0]), Vec3::NEG_Z, smin, smax).expect("fit");
        let b = fit(&cam([0.01, 0.0, 0.0], [1.0, 0.0, 0.0]), Vec3::NEG_Z, smin, smax).expect("fit");
        assert_eq!(a.view_proj, b.view_proj, "tiny camera moves keep the same texels");
        assert!(fit(&cam([0.0; 3], [1.0, 0.0, 0.0]), Vec3::new(0.0, 0.0, -1.0), smin, smax).is_some());
        assert!(fit(&cam([0.0; 3], [1.0, 0.0, 0.0]), Vec3::NEG_Z, Vec3::INFINITY, Vec3::NEG_INFINITY).is_none());
        let far_scene = (Vec3::splat(100_000.0), Vec3::splat(100_100.0));
        assert!(fit(&cam([0.0; 3], [1.0, 0.0, 0.0]), Vec3::NEG_Z, far_scene.0, far_scene.1).is_none());
        assert!(fit(&cam([0.0; 3], [1.0, 0.0, 0.0]), Vec3::NAN, smin, smax).is_some());
    }
}
