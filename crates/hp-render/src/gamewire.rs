use ahash::{HashMap, HashMapExt};

use crate::arena::{Grow, GrowBuffer};
use crate::camera::Camera;
use crate::scene::Scene;
use crate::types::MaterialFlags;

pub(crate) const WORLD_PINK: [u8; 3] = [231, 129, 129];
pub(crate) const WORLD_GREEN: [u8; 3] = [0, 255, 129];
pub(crate) const MODEL_CYAN: [u8; 3] = [0, 255, 255];

const NEVER: (f32, f32) = (1.0, 0.0);
pub(crate) const METRIC_MAX: f32 = 3.0e38;

pub(crate) fn lod_ranges(switch_points: &[f32]) -> Vec<(f32, f32)> {
    let n = switch_points.len();
    let point = |i: usize| switch_points.get(i).copied().filter(|v| v.is_finite()).unwrap_or(0.0);
    let mut lo = -1.0f32;
    (0..n)
        .map(|i| {
            if i > 0 {
                lo = lo.max(point(i));
            }
            let hi = if i + 1 < n { point(i + 1) } else { METRIC_MAX };
            if lo < hi { (lo, hi) } else { NEVER }
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn lod_metric(depth: f32, fov_y: f32, view_height: f32) -> f32 {
    let size = 2.0 * view_height / (2.0 * depth.max(1e-3) * (fov_y * 0.5).tan());
    if size != 0.0 { 100.0 / size } else { 0.0 }
}

pub(crate) fn outline_edges(indices: &[u32], base: u32, out: &mut Vec<u32>) {
    let mut count: HashMap<(u32, u32), (u32, u32, u32)> = HashMap::with_capacity(indices.len());
    for t in indices.chunks_exact(3) {
        if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
            continue;
        }
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let e = count.entry((a.min(b), a.max(b))).or_insert((a, b, 0));
            e.2 += 1;
        }
    }
    let mut edges: Vec<(u32, u32)> = count.into_values().filter(|e| e.2 == 1).map(|e| (e.0, e.1)).collect();
    edges.sort_unstable();
    for (a, b) in edges {
        out.push(a + base);
        out.push(b + base);
    }
}

pub(crate) fn wire_material(flags: MaterialFlags) -> bool {
    !flags.intersects(MaterialFlags::SKY | MaterialFlags::TOOL | MaterialFlags::NODRAW_IN_3D)
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ChunkStamp {
    generation: u64,
    vtx_offset: u32,
    rebuilt_epoch: u64,
    used_i: u32,
}

pub(crate) struct OutlineCache {
    membership_gen: u64,
    entries: HashMap<u32, (ChunkStamp, u32, u32)>,
    indices: Vec<u32>,
    dead: usize,
    uploaded: usize,
    pub buffer: GrowBuffer,
}

impl OutlineCache {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        Self {
            membership_gen: 0,
            entries: HashMap::new(),
            indices: Vec::new(),
            dead: 0,
            uploaded: 0,
            buffer: GrowBuffer::new(device, "hp-render game outlines", wgpu::BufferUsages::INDEX, 1 << 16),
        }
    }

    pub(crate) fn range(&self, chunk: u32) -> Option<(u32, u32)> {
        let (_, first, count) = self.entries.get(&chunk)?;
        (*count > 0 && (*first + *count) as usize <= self.uploaded).then_some((*first, *count))
    }

    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        cam: &Camera,
        max_buffer: u64,
    ) {
        if self.membership_gen != scene.membership_gen || self.dead > self.indices.len() / 2 + 4096 {
            self.membership_gen = scene.membership_gen;
            self.entries.clear();
            self.indices.clear();
            self.dead = 0;
            self.uploaded = 0;
        }
        let start = self.indices.len();
        for (mi, m) in scene.materials.iter().enumerate() {
            if m.chunks.is_empty() || !m.desc.as_ref().is_none_or(|d| wire_material(d.flags)) {
                continue;
            }
            for &c in &m.chunks {
                let Some(Some(ch)) = scene.chunks.get(c as usize) else { continue };
                if ch.used_i == 0 || ch.all_hidden || !cam.aabb_visible(ch.aabb_min, ch.aabb_max) {
                    continue;
                }
                let stamp = ChunkStamp {
                    generation: ch.generation,
                    vtx_offset: ch.vtx.offset,
                    rebuilt_epoch: ch.rebuilt_epoch,
                    used_i: ch.used_i,
                };
                if let Some((s, _, count)) = self.entries.get(&c) {
                    if *s == stamp {
                        continue;
                    }
                    self.dead += *count as usize;
                }
                let first = self.indices.len() as u32;
                for (id, member) in &ch.members {
                    let Some(batch) = scene
                        .objects
                        .get(id)
                        .and_then(|e| e.stored.as_ref())
                        .and_then(|s| s.batches.iter().find(|b| b.material == mi as u32))
                    else {
                        continue;
                    };
                    let n = (member.i_cnt as usize).min(batch.indices.len());
                    outline_edges(&batch.indices[..n], ch.vtx.offset + member.v_off, &mut self.indices);
                }
                let count = self.indices.len() as u32 - first;
                self.entries.insert(c, (stamp, first, count));
            }
        }
        if self.indices.len() == self.uploaded {
            return;
        }
        let bytes = (self.indices.len() * 4) as u64;
        match self.buffer.ensure(device, queue, bytes, max_buffer, false) {
            Grow::TooLarge => {
                self.entries.clear();
                self.indices.clear();
                self.uploaded = 0;
                self.dead = 0;
            }
            Grow::Grown => {
                queue.write_buffer(&self.buffer.buffer, 0, bytemuck::cast_slice(&self.indices));
                self.uploaded = self.indices.len();
            }
            Grow::Fits => {
                let from = start.min(self.uploaded);
                queue.write_buffer(&self.buffer.buffer, from as u64 * 4, bytemuck::cast_slice(&self.indices[from..]));
                self.uploaded = self.indices.len();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_ranges_follow_the_engine() {
        assert_eq!(lod_ranges(&[0.0]), [(-1.0, METRIC_MAX)]);
        assert_eq!(lod_ranges(&[0.0, 10.0, 25.0]), [(-1.0, 10.0), (10.0, 25.0), (25.0, METRIC_MAX)]);
        let r = lod_ranges(&[0.0, 30.0, 20.0, 40.0]);
        assert_eq!(r[0], (-1.0, 30.0));
        assert!(r[1].0 >= r[1].1, "LOD 1 can never be picked: {:?}", r[1]);
        assert_eq!(r[2], (30.0, 40.0));
        assert_eq!(r[3], (40.0, METRIC_MAX));
        let points = [0.0, 12.0, 8.0, 30.0, 31.0];
        let ranges = lod_ranges(&points);
        for metric in [0.0f32, 5.0, 8.0, 11.9, 12.0, 20.0, 30.0, 30.5, 31.0, 1000.0] {
            let engine = (0..points.len() - 1).find(|&i| points[i + 1] > metric).unwrap_or(points.len() - 1);
            let picked: Vec<usize> =
                ranges.iter().enumerate().filter(|(_, r)| r.0 <= metric && metric < r.1).map(|(i, _)| i).collect();
            assert_eq!(picked, [engine], "metric {metric}");
        }
        assert!(lod_ranges(&[]).is_empty());
    }

    #[test]
    fn lod_metric_matches_the_engine() {
        let m = lod_metric(100.0, 75f32.to_radians(), 720.0);
        assert!((m - 10.657).abs() < 0.01, "{m}");
        assert!((lod_metric(200.0, 75f32.to_radians(), 720.0) - 2.0 * m).abs() < 0.01);
    }

    #[test]
    fn outlines_drop_the_fan_diagonals() {
        let mut out = Vec::new();
        outline_edges(&[0, 1, 2, 0, 2, 3, 0, 3, 4], 100, &mut out);
        let mut edges: Vec<(u32, u32)> = out.chunks_exact(2).map(|e| (e[0].min(e[1]), e[0].max(e[1]))).collect();
        edges.sort_unstable();
        assert_eq!(edges, [(100, 101), (100, 104), (101, 102), (102, 103), (103, 104)]);
        out.clear();
        outline_edges(&[0, 1, 2, 0, 2, 3, 4, 5, 6], 0, &mut out);
        assert_eq!(out.len() / 2, 4 + 3);
        out.clear();
        outline_edges(&[0, 0, 0, 7, 7, 7], 0, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn wire_materials() {
        assert!(wire_material(MaterialFlags::empty()));
        assert!(wire_material(MaterialFlags::TRANSLUCENT | MaterialFlags::ALPHA_TEST));
        assert!(!wire_material(MaterialFlags::SKY));
        assert!(!wire_material(MaterialFlags::TOOL));
    }
}
