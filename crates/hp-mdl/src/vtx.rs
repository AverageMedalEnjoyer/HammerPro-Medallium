use crate::error::{FileKind, MdlError};
use crate::reader::{Reader, Result};

const VERSION: i32 = 7;
const HEADER_SIZE: usize = 36;
const BODYPART_SIZE: usize = 8;
const MODEL_SIZE: usize = 8;
const LOD_SIZE: usize = 12;
const MESH_SIZE: usize = 9;
pub(crate) const VERTEX_SIZE: usize = 9;
const REPLACEMENT_LIST_SIZE: usize = 8;
const REPLACEMENT_SIZE: usize = 6;

pub(crate) const STRIP_IS_TRILIST: u8 = 0x01;
pub(crate) const STRIP_IS_TRISTRIP: u8 = 0x02;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
    pub strip_group_size: usize,
    pub strip_size: usize,
    pub extended: bool,
}

impl Layout {
    pub(crate) const STANDARD: Layout = Layout { strip_group_size: 25, strip_size: 27, extended: false };
    pub(crate) const EXTENDED: Layout = Layout { strip_group_size: 33, strip_size: 35, extended: true };
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct StripGroup {
    pub verts_start: usize,
    pub num_verts: usize,
    pub indices_start: usize,
    pub num_indices: usize,
    pub strips_start: usize,
    pub num_strips: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Strip {
    pub index_offset: usize,
    pub num_indices: usize,
    pub flags: u8,
}

#[derive(Clone, Copy, Debug)]
struct StripRanges {
    index_offset: i32,
    num_indices: i32,
    vert_offset: i32,
    num_verts: i32,
}

pub(crate) struct Vtx<'a> {
    r: Reader<'a>,
    pub checksum: i32,
    pub num_lods: usize,
    replacement_offset: i32,
    body_parts_start: usize,
    pub num_body_parts: usize,
}

impl<'a> Vtx<'a> {
    pub(crate) fn parse(data: &'a [u8]) -> Result<Self> {
        let r = Reader::new(data, FileKind::Vtx);
        r.check(0, HEADER_SIZE, "FileHeader_t")?;
        let version = r.i32(0)?;
        if version != VERSION {
            return Err(MdlError::UnsupportedVersion { file: FileKind::Vtx, version });
        }
        let num_lods = r.i32(20)?;
        if !(0..=8).contains(&num_lods) {
            return Err(r.invalid("LOD count", i64::from(num_lods)));
        }
        let (body_parts_start, num_body_parts) =
            r.rel_table(0, r.i32(32)?, r.i32(28)?, BODYPART_SIZE, "body part table")?;
        Ok(Self {
            r,
            checksum: r.i32(16)?,
            num_lods: num_lods as usize,
            replacement_offset: r.i32(24)?,
            body_parts_start,
            num_body_parts,
        })
    }

    pub(crate) fn reader(&self) -> &Reader<'a> {
        &self.r
    }

    pub(crate) fn body_part(&self, bp: usize) -> Result<(usize, usize)> {
        if bp >= self.num_body_parts {
            return Err(self.r.invalid("body part index", bp as i64));
        }
        let b = self.body_parts_start + bp * BODYPART_SIZE;
        self.r.rel_table(b, self.r.i32(b + 4)?, self.r.i32(b)?, MODEL_SIZE, "model table")
    }

    pub(crate) fn model_lods(&self, model: usize) -> Result<(usize, usize)> {
        self.r.rel_table(model, self.r.i32(model + 4)?, self.r.i32(model)?, LOD_SIZE, "LOD table")
    }

    pub(crate) fn lod_switch_points(&self, count: usize) -> Vec<f32> {
        let read = || -> Result<Vec<f32>> {
            let (models_start, num_models) = self.body_part(0)?;
            if num_models == 0 {
                return Ok(Vec::new());
            }
            let (lods_start, num_lods) = self.model_lods(Self::model_at(models_start, 0))?;
            (0..count.min(num_lods)).map(|l| self.r.f32(Self::lod_at(lods_start, l) + 8)).collect()
        };
        read().unwrap_or_default().into_iter().map(|v| if v.is_finite() { v } else { 0.0 }).collect()
    }

    pub(crate) fn lod_meshes(&self, lod: usize) -> Result<(usize, usize)> {
        self.r.rel_table(lod, self.r.i32(lod + 4)?, self.r.i32(lod)?, MESH_SIZE, "mesh table")
    }

    pub(crate) fn model_at(start: usize, m: usize) -> usize {
        start + m * MODEL_SIZE
    }

    pub(crate) fn lod_at(start: usize, l: usize) -> usize {
        start + l * LOD_SIZE
    }

    pub(crate) fn mesh_at(start: usize, m: usize) -> usize {
        start + m * MESH_SIZE
    }

    pub(crate) fn mesh_strip_groups(&self, mesh: usize, layout: Layout) -> Result<(usize, usize)> {
        self.r.rel_table(mesh, self.r.i32(mesh + 4)?, self.r.i32(mesh)?, layout.strip_group_size, "strip group table")
    }

    pub(crate) fn strip_group(&self, start: usize, i: usize, layout: Layout) -> Result<StripGroup> {
        let g = start + i * layout.strip_group_size;
        let r = &self.r;
        let (verts_start, num_verts) = r.rel_table(g, r.i32(g + 4)?, r.i32(g)?, VERTEX_SIZE, "strip group vertices")?;
        let (indices_start, num_indices) = r.rel_table(g, r.i32(g + 12)?, r.i32(g + 8)?, 2, "strip group indices")?;
        let (strips_start, num_strips) =
            r.rel_table(g, r.i32(g + 20)?, r.i32(g + 16)?, layout.strip_size, "strip table")?;
        Ok(StripGroup { verts_start, num_verts, indices_start, num_indices, strips_start, num_strips })
    }

    pub(crate) fn strip(&self, g: &StripGroup, i: usize, layout: Layout) -> Result<Strip> {
        let s = g.strips_start + i * layout.strip_size;
        let r = &self.r;
        let num_indices = r.i32(s)?;
        let index_offset = r.i32(s + 4)?;
        let (Ok(num_indices), Ok(index_offset)) = (usize::try_from(num_indices), usize::try_from(index_offset)) else {
            return Err(r.invalid("strip index range", i64::from(num_indices.min(index_offset))));
        };
        if index_offset.checked_add(num_indices).is_none_or(|end| end > g.num_indices) {
            return Err(r.invalid("strip index range", index_offset as i64));
        }
        Ok(Strip { index_offset, num_indices, flags: r.u8(s + 18)? })
    }

    fn strip_ranges(&self, g: &StripGroup, i: usize, layout: Layout) -> Result<StripRanges> {
        let s = g.strips_start + i * layout.strip_size;
        Ok(StripRanges {
            num_indices: self.r.i32(s)?,
            index_offset: self.r.i32(s + 4)?,
            num_verts: self.r.i32(s + 8)?,
            vert_offset: self.r.i32(s + 12)?,
        })
    }

    pub(crate) fn strips_partition(&self, layout: Layout) -> bool {
        self.walk_strip_groups(layout, |vtx, group| {
            let (mut next_index, mut next_vert) = (0i64, 0i64);
            for i in 0..group.num_strips {
                let Ok(s) = vtx.strip_ranges(group, i, layout) else { return false };
                if i64::from(s.index_offset) != next_index || i64::from(s.vert_offset) != next_vert {
                    return false;
                }
                next_index += i64::from(s.num_indices);
                next_vert += i64::from(s.num_verts);
            }
            group.num_strips == 0 || (next_index == group.num_indices as i64 && next_vert == group.num_verts as i64)
        })
        .unwrap_or(false)
    }

    fn walk_strip_groups(&self, layout: Layout, mut f: impl FnMut(&Self, &StripGroup) -> bool) -> Result<bool> {
        for bp in 0..self.num_body_parts {
            let (models_start, num_models) = self.body_part(bp)?;
            for m in 0..num_models {
                let (lods_start, num_lods) = self.model_lods(Self::model_at(models_start, m))?;
                for l in 0..num_lods {
                    let (meshes_start, num_meshes) = self.lod_meshes(Self::lod_at(lods_start, l))?;
                    for e in 0..num_meshes {
                        let (groups_start, num_groups) =
                            self.mesh_strip_groups(Self::mesh_at(meshes_start, e), layout)?;
                        for g in 0..num_groups {
                            let group = self.strip_group(groups_start, g, layout)?;
                            if !f(self, &group) {
                                return Ok(false);
                            }
                        }
                    }
                }
            }
        }
        Ok(true)
    }

    pub(crate) fn index_bytes(&self, g: &StripGroup) -> Result<&'a [u8]> {
        self.r.bytes(g.indices_start, g.num_indices * 2, "strip group indices")
    }

    pub(crate) fn vertex_bytes(&self, g: &StripGroup) -> Result<&'a [u8]> {
        self.r.bytes(g.verts_start, g.num_verts * VERTEX_SIZE, "strip group vertices")
    }

    pub(crate) fn material_replacements(&self, lod: usize) -> Result<Vec<(usize, String)>> {
        if self.replacement_offset <= 0 || lod >= self.num_lods {
            return Ok(Vec::new());
        }
        let r = &self.r;
        let list_start = r.rel(0, self.replacement_offset, "material replacement lists")?;
        r.table(list_start, self.num_lods as i32, REPLACEMENT_LIST_SIZE, "material replacement lists")?;
        let list = list_start + lod * REPLACEMENT_LIST_SIZE;
        let (start, n) =
            r.rel_table(list, r.i32(list + 4)?, r.i32(list)?, REPLACEMENT_SIZE, "material replacements")?;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let e = start + i * REPLACEMENT_SIZE;
            let id = r.i16(e)?;
            let name_rel = r.i32(e + 2)?;
            let id = usize::try_from(id).map_err(|_| r.invalid("replacement material id", i64::from(id)))?;
            let name_at = r.rel(e, name_rel, "replacement material name")?;
            out.push((id, r.cstr(name_at, "replacement material name")?));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_headers() {
        assert!(Vtx::parse(&[0u8; 10]).is_err());
        let mut h = vec![0u8; HEADER_SIZE];
        h[0] = 6;
        assert!(matches!(Vtx::parse(&h), Err(MdlError::UnsupportedVersion { version: 6, .. })));
        h[0] = 7;
        h[28..32].copy_from_slice(&5i32.to_le_bytes());
        h[32..36].copy_from_slice(&36i32.to_le_bytes());
        assert!(matches!(Vtx::parse(&h), Err(MdlError::OutOfBounds { .. })));
        h[28..32].copy_from_slice(&0i32.to_le_bytes());
        let vtx = Vtx::parse(&h).expect("empty vtx");
        assert_eq!(vtx.num_body_parts, 0);
        assert!(vtx.body_part(0).is_err());
        assert!(vtx.material_replacements(0).expect("none").is_empty());
    }
}
