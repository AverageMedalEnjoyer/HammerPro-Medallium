use crate::error::FileKind;
use crate::mdl::{Bone, SEQDESC_SIZE};
use crate::pose::{AnimClip, SkinMatrix, anim_timing, seq_anim, sequence_pose, skin_from_local};
use crate::reader::Reader;

const H_NUM_POSE_PARAMS: usize = 300;
const POSE_PARAM_SIZE: usize = 20;
const MAX_SOURCES: usize = 16;

struct Source {
    mdl: Vec<u8>,
    ani: Option<Vec<u8>>,
    bones: Vec<Bone>,
    map: Vec<Option<usize>>,
    sequences: Vec<String>,
}

pub struct AnimSet {
    bones: Vec<Bone>,
    sources: Vec<Source>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimRef {
    source: usize,
    sequence: usize,
    blend: usize,
    pub clip: AnimClip,
}

impl AnimSet {
    pub fn load(mdl: &[u8], read: impl Fn(&str) -> Option<Vec<u8>>) -> Option<Self> {
        let studio = crate::mdl::parse(mdl).ok()?;
        if studio.bones.is_empty() {
            return None;
        }
        let bones = studio.bones.clone();
        let mut sources = vec![Source {
            ani: crate::pose::animation_block_path(mdl).and_then(|p| read(&p)),
            map: (0..bones.len()).map(Some).collect(),
            sequences: studio.sequences.iter().map(|s| s.name.to_ascii_lowercase()).collect(),
            bones: studio.bones,
            mdl: mdl.to_vec(),
        }];
        for name in studio.include_models.iter().take(MAX_SOURCES) {
            let Some(path) = crate::deps::include_path(name) else { continue };
            let Some(data) = read(&path) else { continue };
            let Ok(inc) = crate::mdl::parse(&data) else { continue };
            let map =
                inc.bones.iter().map(|b| bones.iter().position(|t| t.name.eq_ignore_ascii_case(&b.name))).collect();
            sources.push(Source {
                ani: crate::pose::animation_block_path(&data).and_then(|p| read(&p)),
                sequences: inc.sequences.iter().map(|s| s.name.to_ascii_lowercase()).collect(),
                bones: inc.bones,
                map,
                mdl: data,
            });
        }
        Some(Self { bones, sources })
    }

    pub fn bones(&self) -> &[Bone] {
        &self.bones
    }

    pub fn has(&self, name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        self.sources.iter().any(|s| s.sequences.contains(&name))
    }

    pub fn sequence(&self, name: &str, params: &[(&str, f32)]) -> Option<AnimRef> {
        let name = name.to_ascii_lowercase();
        let (source, sequence) = self
            .sources
            .iter()
            .enumerate()
            .find_map(|(i, s)| s.sequences.iter().position(|n| *n == name).map(|q| (i, q)))?;
        let src = &self.sources[source];
        let r = Reader::new(&src.mdl, FileKind::Mdl);
        let blend = blend_cell(&r, sequence, params).unwrap_or(0);
        let (frames, fps, looping) = anim_timing(&r, sequence, blend)?;
        Some(AnimRef { source, sequence, blend, clip: AnimClip { sequence, frames, fps, looping } })
    }

    pub fn skin(&self, anim: &AnimRef, frame: usize, frac: f32) -> Option<Vec<SkinMatrix>> {
        let src = self.sources.get(anim.source)?;
        let r = Reader::new(&src.mdl, FileKind::Mdl);
        let ani = src.ani.as_deref().map(|a| Reader::new(a, FileKind::Mdl));
        let posed =
            sequence_pose(&r, ani.as_ref(), &src.bones, (anim.sequence, anim.blend), frame, frac.clamp(0.0, 1.0))?;
        let mut local: Vec<([f32; 3], [f32; 4])> = self.bones.iter().map(|b| (b.pos, b.quat)).collect();
        for (pose, target) in posed.into_iter().zip(&src.map) {
            if let Some(slot) = target.and_then(|t| local.get_mut(t)) {
                *slot = pose;
            }
        }
        let skin = skin_from_local(&self.bones, &local);
        skin.iter().flatten().flatten().all(|v| v.is_finite()).then_some(skin)
    }
}

fn pose_param(r: &Reader<'_>, i: i32) -> Option<String> {
    let count = r.i32(H_NUM_POSE_PARAMS).ok()?;
    if i < 0 || i >= count {
        return None;
    }
    let table = r.rel(0, r.i32(H_NUM_POSE_PARAMS + 4).ok()?, "pose parameters").ok()?;
    let at = table.checked_add(usize::try_from(i).ok()?.checked_mul(POSE_PARAM_SIZE)?)?;
    let name_at = r.rel(at, r.i32(at).ok()?, "pose parameter name").ok()?;
    r.cstr(name_at, "pose parameter name").ok()
}

fn blend_cell(r: &Reader<'_>, seq: usize, params: &[(&str, f32)]) -> Option<usize> {
    let (seq_at, _) = seq_anim(r, seq, 0)?;
    r.check(seq_at, SEQDESC_SIZE, "sequence").ok()?;
    let mut cell = [0usize; 2];
    for (d, slot) in cell.iter_mut().enumerate() {
        let size = usize::try_from(r.i32(seq_at + 68 + d * 4).ok()?).ok()?;
        if size <= 1 {
            continue;
        }
        let param = r.i32(seq_at + 76 + d * 4).ok()?;
        let start = r.f32(seq_at + 84 + d * 4).ok()?;
        let end = r.f32(seq_at + 92 + d * 4).ok()?;
        let name = pose_param(r, param).unwrap_or_default();
        let value = params.iter().find(|(n, _)| n.eq_ignore_ascii_case(&name)).map_or(0.0, |(_, v)| *v);
        let t = if (end - start).abs() > 1e-6 { ((value - start) / (end - start)).clamp(0.0, 1.0) } else { 0.0 };
        *slot = (t * (size - 1) as f32).round() as usize;
    }
    let width = usize::try_from(r.i32(seq_at + 68).ok()?).ok()?.max(1);
    Some(cell[1] * width + cell[0])
}
