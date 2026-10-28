use crate::error::{FileKind, MdlError};
use crate::reader::{Reader, Result};

const MAGIC: &[u8; 4] = b"IDSV";
const VERSION: i32 = 4;
const HEADER_SIZE: usize = 64;
const MAX_LODS: usize = 8;
const FIXUP_SIZE: usize = 12;
pub(crate) const VERTEX_SIZE: usize = 48;
const TANGENT_SIZE: usize = 16;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BoneWeight {
    pub weights: [f32; 3],
    pub bones: [u8; 3],
    pub count: u8,
}

pub(crate) enum VertexMap {
    Identity(usize),
    Table(Vec<u32>),
}

impl VertexMap {
    pub(crate) fn len(&self) -> usize {
        match self {
            VertexMap::Identity(n) => *n,
            VertexMap::Table(t) => t.len(),
        }
    }

    #[inline]
    pub(crate) fn get(&self, i: usize) -> Option<usize> {
        match self {
            VertexMap::Identity(n) => (i < *n).then_some(i),
            VertexMap::Table(t) => t.get(i).map(|&v| v as usize),
        }
    }
}

pub(crate) struct Vvd<'a> {
    r: Reader<'a>,
    pub checksum: i32,
    lod_vertices: [usize; MAX_LODS],
    num_fixups: usize,
    fixup_start: usize,
    vertex_start: usize,
    tangent_start: Option<usize>,
}

impl<'a> Vvd<'a> {
    pub(crate) fn parse(data: &'a [u8]) -> Result<Self> {
        let r = Reader::new(data, FileKind::Vvd);
        let magic = r.bytes(0, 4, "header")?;
        if magic != MAGIC {
            return Err(MdlError::BadMagic {
                file: FileKind::Vvd,
                expected: "IDSV",
                found: String::from_utf8_lossy(magic).into_owned(),
            });
        }
        r.check(0, HEADER_SIZE, "vertexFileHeader_t")?;
        let version = r.i32(4)?;
        if version != VERSION {
            return Err(MdlError::UnsupportedVersion { file: FileKind::Vvd, version });
        }
        let checksum = r.i32(8)?;
        let num_lods = r.i32(12)?;
        if !(0..=MAX_LODS as i32).contains(&num_lods) {
            return Err(r.invalid("LOD count", i64::from(num_lods)));
        }
        let mut lod_vertices = [0usize; MAX_LODS];
        for (i, slot) in lod_vertices.iter_mut().enumerate() {
            let v = r.i32(16 + i * 4)?;
            *slot = usize::try_from(v).map_err(|_| r.invalid("LOD vertex count", i64::from(v)))?;
        }
        let total = lod_vertices[0];

        let vertex_start = r.rel(0, r.i32(56)?, "vertex data")?;
        let vertex_bytes = total.checked_mul(VERTEX_SIZE).ok_or_else(|| r.invalid("vertex count", total as i64))?;
        r.check(vertex_start, vertex_bytes, "vertex data")?;

        let tangent_rel = r.i32(60)?;
        let tangent_start =
            (tangent_rel > 0).then(|| r.rel(0, tangent_rel, "tangent data").ok()).flatten().filter(|&start| {
                total.checked_mul(TANGENT_SIZE).is_some_and(|size| r.check(start, size, "tangent data").is_ok())
            });

        let fixups = r.i32(48)?;
        let (fixup_start, num_fixups) = r.rel_table(0, r.i32(52)?, fixups, FIXUP_SIZE, "fixup table")?;

        Ok(Self { r, checksum, lod_vertices, num_fixups, fixup_start, vertex_start, tangent_start })
    }

    pub(crate) fn total_vertices(&self) -> usize {
        self.lod_vertices[0]
    }

    pub(crate) fn vertex_map(&self, root_lod: usize) -> Result<VertexMap> {
        let total = self.total_vertices();
        let wanted = self.lod_vertices.get(root_lod).copied().unwrap_or(0);
        if wanted > total {
            return Err(self.r.invalid("LOD vertex count", wanted as i64));
        }
        if self.num_fixups == 0 {
            return Ok(VertexMap::Identity(wanted));
        }
        let mut table: Vec<u32> = Vec::with_capacity(wanted);
        for i in 0..self.num_fixups {
            let f = self.fixup_start + i * FIXUP_SIZE;
            let lod = self.r.i32(f)?;
            if lod < root_lod as i32 {
                continue;
            }
            let src = self.r.i32(f + 4)?;
            let n = self.r.i32(f + 8)?;
            let (Ok(src), Ok(n)) = (usize::try_from(src), usize::try_from(n)) else {
                return Err(self.r.invalid("fixup range", i64::from(src.min(n))));
            };
            let end = src.checked_add(n).filter(|&e| e <= total);
            let fits = table.len().checked_add(n).is_some_and(|len| len <= wanted);
            let (Some(end), true) = (end, fits) else {
                return Err(self.r.invalid("fixup range", src as i64));
            };
            table.extend((src..end).map(|v| v as u32));
        }
        Ok(VertexMap::Table(table))
    }

    #[inline]
    fn vertex_bytes(&self, src: usize) -> Result<&'a [u8]> {
        if src >= self.total_vertices() {
            return Err(self.r.invalid("vertex index", src as i64));
        }
        self.r.bytes(self.vertex_start + src * VERTEX_SIZE, VERTEX_SIZE, "vertex")
    }

    #[inline]
    pub(crate) fn vertex(&self, src: usize) -> Result<([f32; 3], [f32; 3], [f32; 2])> {
        let b = self.vertex_bytes(src)?;
        let f = |i: usize| -> f32 {
            let o = 16 + i * 4;
            b.get(o..o + 4).map_or(0.0, |s| f32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        };
        Ok(([f(0), f(1), f(2)], [f(3), f(4), f(5)], [f(6), f(7)]))
    }

    pub(crate) fn bone_weight(&self, src: usize) -> Result<BoneWeight> {
        let b = self.vertex_bytes(src)?;
        let f = |i: usize| -> f32 {
            let o = i * 4;
            b.get(o..o + 4).map_or(0.0, |s| f32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        };
        let byte = |i: usize| b.get(i).copied().unwrap_or(0);
        Ok(BoneWeight { weights: [f(0), f(1), f(2)], bones: [byte(12), byte(13), byte(14)], count: byte(15) })
    }

    pub(crate) fn tangent(&self, src: usize) -> Result<Option<[f32; 4]>> {
        let Some(start) = self.tangent_start else {
            return Ok(None);
        };
        if src >= self.total_vertices() {
            return Err(self.r.invalid("vertex index", src as i64));
        }
        self.r.floats(start + src * TANGENT_SIZE).map(Some)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::mdl::tests::Buf;

    pub(crate) fn make_vvd(checksum: i32, lod_counts: &[i32], fixups: &[(i32, i32, i32)], n: usize) -> Vec<u8> {
        let mut b = Buf::default();
        b.at(0, MAGIC);
        b.i32(4, VERSION);
        b.i32(8, checksum);
        b.i32(12, lod_counts.len() as i32);
        for (i, c) in lod_counts.iter().enumerate() {
            b.i32(16 + i * 4, *c);
        }
        let fixup_start = HEADER_SIZE;
        b.i32(48, fixups.len() as i32);
        b.i32(52, fixup_start as i32);
        for (i, (lod, src, cnt)) in fixups.iter().enumerate() {
            b.i32(fixup_start + i * 12, *lod);
            b.i32(fixup_start + i * 12 + 4, *src);
            b.i32(fixup_start + i * 12 + 8, *cnt);
        }
        let vstart = fixup_start + fixups.len() * 12;
        b.i32(56, vstart as i32);
        for i in 0..n {
            let v = vstart + i * VERTEX_SIZE;
            b.f32s(v, &[1.0, 0.0, 0.0]);
            b.at(v + 12, &[0, 0, 0, 1]);
            b.f32s(v + 16, &[i as f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.5, 0.25]);
        }
        let tstart = vstart + n * VERTEX_SIZE;
        b.i32(60, tstart as i32);
        for i in 0..n {
            b.f32s(tstart + i * TANGENT_SIZE, &[1.0, 0.0, 0.0, -1.0]);
        }
        b.0
    }

    #[test]
    fn identity_and_fixups() {
        let data = make_vvd(7, &[4, 2], &[], 4);
        let vvd = Vvd::parse(&data).expect("vvd");
        assert_eq!(vvd.checksum, 7);
        assert!(vvd.tangent_start.is_some());
        assert_eq!(vvd.vertex_map(0).expect("map").len(), 4);
        assert_eq!(vvd.vertex_map(1).expect("map").len(), 2);
        let (p, n, uv) = vvd.vertex(3).expect("vertex");
        assert_eq!((p, n, uv), ([3.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.5, 0.25]));
        assert_eq!(vvd.bone_weight(0).expect("w").count, 1);
        assert_eq!(vvd.tangent(1).expect("t"), Some([1.0, 0.0, 0.0, -1.0]));
        assert!(vvd.vertex(4).is_err());

        let data = make_vvd(7, &[5, 3], &[(1, 0, 2), (0, 2, 2), (1, 4, 1)], 5);
        let vvd = Vvd::parse(&data).expect("vvd");
        let VertexMap::Table(t0) = vvd.vertex_map(0).expect("map") else { panic!("table expected") };
        assert_eq!(t0, vec![0, 1, 2, 3, 4]);
        let VertexMap::Table(t1) = vvd.vertex_map(1).expect("map") else { panic!("table expected") };
        assert_eq!(t1, vec![0, 1, 4]);
    }

    #[test]
    fn rejects_hostile_input() {
        assert!(Vvd::parse(b"IDSV").is_err());
        assert!(matches!(Vvd::parse(&[0u8; 64]), Err(MdlError::BadMagic { .. })));
        let mut data = make_vvd(1, &[4], &[], 4);
        data[16..20].copy_from_slice(&1_000_000i32.to_le_bytes());
        assert!(Vvd::parse(&data).is_err());
        let data = make_vvd(1, &[2], &[(0, 1, 5)], 2);
        let vvd = Vvd::parse(&data).expect("vvd");
        assert!(vvd.vertex_map(0).is_err());
        let data = make_vvd(1, &[2], &[(0, 0, 2), (0, 0, 2)], 2);
        let vvd = Vvd::parse(&data).expect("vvd");
        assert!(vvd.vertex_map(0).is_err());
    }
}
