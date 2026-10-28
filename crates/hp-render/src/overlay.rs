use glam::Vec3;
use rayon::prelude::*;

use crate::gpu::{ColorVertex, PointInstance, pack_rgba};
use crate::{Overlay, OverlayLine, OverlayTri};

pub(crate) const CHUNK_PRIMS: usize = 1024;
pub(crate) const UPLOAD_BLOCK: usize = 1 << 18;
const TASK_PRIMS: usize = 8192;

const VERTEX_BYTES: u64 = size_of::<ColorVertex>() as u64;
const POINT_BYTES: u64 = size_of::<PointInstance>() as u64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OverlayLayout {
    pub lines: (u32, u32),
    pub lines_top: (u32, u32),
    pub tris: (u32, u32),
    pub tris_top: (u32, u32),
    pub points_offset: u64,
    pub points: u32,
}

impl OverlayLayout {
    pub(crate) fn of(o: &Overlay) -> Option<(Self, u64)> {
        let n = |len: usize, per: u64| u64::try_from(len).ok()?.checked_mul(per);
        let lines = n(o.lines.len(), 2)?;
        let lines_top = n(o.lines_on_top.len(), 2)?;
        let tris = n(o.triangles.len(), 3)?;
        let tris_top = n(o.triangles_on_top.len(), 3)?;
        let vertices = lines.checked_add(lines_top)?.checked_add(tris)?.checked_add(tris_top)?;
        let v32 = |v: u64| u32::try_from(v).ok();
        v32(vertices)?;
        let points = u32::try_from(o.points.len()).ok()?;
        let layout = Self {
            lines: (0, v32(lines)?),
            lines_top: (v32(lines)?, v32(lines_top)?),
            tris: (v32(lines + lines_top)?, v32(tris)?),
            tris_top: (v32(lines + lines_top + tris)?, v32(tris_top)?),
            points_offset: vertices * VERTEX_BYTES,
            points,
        };
        let total = layout.points_offset.checked_add(u64::from(points).checked_mul(POINT_BYTES)?)?;
        Some((layout, total))
    }

    pub(crate) fn layer(&self, on_top: bool) -> ((u32, u32), (u32, u32)) {
        if on_top { (self.lines_top, self.tris_top) } else { (self.lines, self.tris) }
    }

    pub(crate) fn layer_is_empty(&self, on_top: bool) -> bool {
        let (lines, tris) = self.layer(on_top);
        lines.1 == 0 && tris.1 == 0 && (!on_top || self.points == 0)
    }
}

pub(crate) fn encode(o: &Overlay, scratch: &mut Vec<ColorVertex>, block: usize, flush: &mut dyn FnMut(&[u8])) {
    encode_section(&o.lines, scratch, block, flush);
    encode_section(&o.lines_on_top, scratch, block, flush);
    encode_section(&o.triangles, scratch, block, flush);
    encode_section(&o.triangles_on_top, scratch, block, flush);
    scratch.clear();
    if !o.points.is_empty() {
        let points: Vec<PointInstance> = o
            .points
            .iter()
            .map(|p| {
                let size = if p.size_px.is_finite() { p.size_px.clamp(1.0, 256.0) } else { 4.0 };
                PointInstance { pos: p.pos, size, color: pack_rgba(p.color) }
            })
            .collect();
        flush(bytemuck::cast_slice(&points));
    }
}

fn encode_section<T: Prim + Sync>(
    prims: &[T],
    scratch: &mut Vec<ColorVertex>,
    block: usize,
    flush: &mut dyn FnMut(&[u8]),
) {
    let verts = T::VERTS as usize;
    let per_block = (block / verts).max(1);
    for group in prims.chunks(per_block) {
        scratch.clear();
        scratch.resize(group.len() * verts, ColorVertex::default());
        let fill = |(dst, src): (&mut [ColorVertex], &[T])| {
            for (out, prim) in dst.chunks_exact_mut(verts).zip(src) {
                let color = pack_rgba(prim.color());
                let mut k = 0;
                prim.corners(|pos| {
                    out[k] = ColorVertex { pos, color };
                    k += 1;
                });
            }
        };
        if group.len() > TASK_PRIMS {
            scratch.par_chunks_mut(TASK_PRIMS * verts).zip(group.par_chunks(TASK_PRIMS)).for_each(fill);
        } else {
            fill((&mut scratch[..], group));
        }
        flush(bytemuck::cast_slice(scratch));
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Chunk {
    pub first: u32,
    pub count: u32,
    pub min: Vec3,
    pub max: Vec3,
}

trait Prim {
    const VERTS: u32;
    fn corners(&self, f: impl FnMut([f32; 3]));
    fn color(&self) -> [f32; 4];
}

impl Prim for OverlayLine {
    const VERTS: u32 = 2;
    fn corners(&self, mut f: impl FnMut([f32; 3])) {
        f(self.a);
        f(self.b);
    }
    fn color(&self) -> [f32; 4] {
        self.color
    }
}

impl Prim for OverlayTri {
    const VERTS: u32 = 3;
    fn corners(&self, f: impl FnMut([f32; 3])) {
        self.p.into_iter().for_each(f);
    }
    fn color(&self) -> [f32; 4] {
        self.color
    }
}

fn section_chunks<T: Prim + Sync>(prims: &[T], first: u32) -> Vec<Chunk> {
    let bounds = |run: &[T]| {
        let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for prim in run {
            prim.corners(|p| {
                let v = Vec3::from_array(p);
                min = min.min(v);
                max = max.max(v);
            });
        }
        if min.is_finite() && max.is_finite() { (min, max) } else { (Vec3::splat(f32::MIN), Vec3::splat(f32::MAX)) }
    };
    let boxes: Vec<(Vec3, Vec3)> = if prims.len() > TASK_PRIMS {
        prims.par_chunks(CHUNK_PRIMS).map(bounds).collect()
    } else {
        prims.chunks(CHUNK_PRIMS).map(bounds).collect()
    };
    let mut start = first;
    prims
        .chunks(CHUNK_PRIMS)
        .zip(boxes)
        .map(|(run, (min, max))| {
            let count = run.len() as u32 * T::VERTS;
            start += count;
            Chunk { first: start - count, count, min, max }
        })
        .collect()
}

pub(crate) fn chunks(o: &Overlay, layout: &OverlayLayout) -> [Vec<Chunk>; 4] {
    [
        section_chunks(&o.lines, layout.lines.0),
        section_chunks(&o.lines_on_top, layout.lines_top.0),
        section_chunks(&o.triangles, layout.tris.0),
        section_chunks(&o.triangles_on_top, layout.tris_top.0),
    ]
}

pub(crate) fn visible_ranges(chunks: &[Chunk], visible: impl Fn(Vec3, Vec3) -> bool, out: &mut Vec<(u32, u32)>) {
    out.clear();
    for c in chunks {
        if c.count == 0 || !visible(c.min, c.max) {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.0 + last.1 == c.first => last.1 += c.count,
            _ => out.push((c.first, c.count)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OverlayPoint;

    fn line(x: f32) -> OverlayLine {
        OverlayLine { a: [x, 0.0, 0.0], b: [x + 1.0, 2.0, 3.0], color: [1.0; 4] }
    }

    fn tri(x: f32) -> OverlayTri {
        OverlayTri { p: [[x, 0.0, 0.0], [x, 1.0, 0.0], [x, 0.0, 1.0]], color: [0.5; 4] }
    }

    fn sample(n: usize) -> Overlay {
        let mut o = Overlay::default();
        o.lines.extend((0..n).map(|i| line(i as f32)));
        o.lines_on_top.extend((0..3).map(|i| line(-(i as f32))));
        o.triangles.extend((0..n + 7).map(|i| tri(i as f32 * 10.0)));
        o.triangles_on_top.push(tri(5.0));
        o.points.push(OverlayPoint { pos: [1.0, 2.0, 3.0], color: [1.0; 4], size_px: f32::NAN });
        o
    }

    #[test]
    fn layout_matches_the_encoding() {
        let o = sample(5);
        let (l, total) = OverlayLayout::of(&o).unwrap();
        assert_eq!(l.lines, (0, 10));
        assert_eq!(l.lines_top, (10, 6));
        assert_eq!(l.tris, (16, 36));
        assert_eq!(l.tris_top, (52, 3));
        assert_eq!(l.points_offset, 55 * 16);
        assert_eq!(l.points, 1);
        assert_eq!(total, 55 * 16 + 20);
        let mut scratch = Vec::new();
        let mut all = Vec::new();
        encode(&o, &mut scratch, usize::MAX, &mut |b| all.extend_from_slice(b));
        assert_eq!(all.len() as u64, total);
        let v: &[ColorVertex] = bytemuck::cast_slice(&all[..55 * 16]);
        assert_eq!(v[0].pos, [0.0, 0.0, 0.0]);
        assert_eq!(v[1].pos, [1.0, 2.0, 3.0]);
        assert_eq!(v[2].pos, [1.0, 0.0, 0.0]);
        assert_eq!(v[16].pos, [0.0, 0.0, 0.0]);
        assert_eq!(v[17].pos, [0.0, 1.0, 0.0]);
        assert_eq!(v[19].pos, [10.0, 0.0, 0.0]);
        assert_eq!(v[16].color, pack_rgba([0.5; 4]));
        let pt: &[PointInstance] = bytemuck::cast_slice(&all[55 * 16..]);
        assert_eq!(pt[0].size, 4.0, "non finite sizes are replaced");
        assert!(!l.layer_is_empty(false) && !l.layer_is_empty(true));
        assert!(OverlayLayout::of(&Overlay::default()).unwrap().0.layer_is_empty(true));
    }

    #[test]
    fn streamed_encoding_is_identical_and_aligned() {
        let o = sample(30_000);
        let mut scratch = Vec::new();
        let mut whole = Vec::new();
        encode(&o, &mut scratch, usize::MAX, &mut |b| whole.extend_from_slice(b));
        let mut streamed = Vec::new();
        let mut blocks = 0;
        let mut scratch = Vec::new();
        encode(&o, &mut scratch, 1000, &mut |b| {
            assert_eq!(b.len() % 4, 0, "queue writes need 4 byte multiples");
            assert!(b.len() <= 1000 * 16, "blocks are bounded");
            blocks += 1;
            streamed.extend_from_slice(b);
        });
        assert_eq!(whole, streamed);
        assert!(blocks > 100);
        assert!(scratch.capacity() <= 1000, "the scratch stays small: {}", scratch.capacity());
    }

    #[test]
    fn chunks_cover_every_primitive_with_bounds() {
        let o = sample(2500);
        let (l, _) = OverlayLayout::of(&o).unwrap();
        let c = chunks(&o, &l);
        assert_eq!(c[0].len(), 3);
        assert_eq!(c[0].iter().map(|c| c.count).sum::<u32>(), l.lines.1);
        assert_eq!(c[0][0].first, 0);
        assert_eq!(c[0][1].first, 2048);
        assert_eq!(c[0][0].min, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(c[0][0].max, Vec3::new(1024.0, 2.0, 3.0));
        assert_eq!(c[1].len(), 1);
        assert_eq!(c[1][0].first, l.lines_top.0);
        assert_eq!(c[2].iter().map(|c| c.count).sum::<u32>(), l.tris.1);
        assert_eq!(c[2][0].first, l.tris.0);
        assert_eq!(c[2][1].min.x, 10240.0);
        assert_eq!(c[3][0].first, l.tris_top.0);
    }

    #[test]
    fn non_finite_chunks_are_never_culled() {
        let mut o = Overlay::default();
        o.lines.push(OverlayLine { a: [f32::NAN; 3], b: [f32::INFINITY, 0.0, 0.0], color: [1.0; 4] });
        let (l, _) = OverlayLayout::of(&o).unwrap();
        let c = chunks(&o, &l);
        assert_eq!(c[0][0].min, Vec3::splat(f32::MIN));
        assert_eq!(c[0][0].max, Vec3::splat(f32::MAX));
    }

    #[test]
    #[ignore = "performance probe: cargo test -p hp-render --release --lib -- --ignored perf_probe"]
    fn perf_probe_encode() {
        let o = sample(1_200_000);
        let t = std::time::Instant::now();
        let mut scratch = Vec::new();
        let mut n = 0usize;
        encode(&o, &mut scratch, UPLOAD_BLOCK, &mut |b| n += b.len());
        eprintln!("encode {} MB: {:?}", n >> 20, t.elapsed());
        let t = std::time::Instant::now();
        let mut n = 0usize;
        encode(&o, &mut scratch, usize::MAX, &mut |b| n += b.len());
        eprintln!("encode {} MB: {:?}", n >> 20, t.elapsed());
        let t = std::time::Instant::now();
        let (l, _) = OverlayLayout::of(&o).unwrap();
        let c = chunks(&o, &l);
        eprintln!("chunks {}: {:?}", c[0].len() + c[2].len(), t.elapsed());
    }

    #[test]
    fn visible_ranges_merge_neighbours() {
        let ch = |first, x: f32| Chunk { first, count: 10, min: Vec3::splat(x), max: Vec3::splat(x + 1.0) };
        let list = [ch(0, 0.0), ch(10, 1.0), ch(20, 100.0), ch(30, 2.0), ch(40, 3.0)];
        let mut out = Vec::new();
        visible_ranges(&list, |min, _| min.x < 50.0, &mut out);
        assert_eq!(out, [(0, 20), (30, 20)]);
        visible_ranges(&list, |_, _| false, &mut out);
        assert!(out.is_empty());
    }
}
