pub(crate) const FLAG_CAST_TEXTURE_SHADOWS: i32 = 0x40000;
const VTX_VERSION: i32 = 7;
const STRIP_IS_TRILIST: u8 = 1;
const VERTEX_SIZE: usize = 48;

fn i32_at(d: &[u8], o: usize) -> Option<i32> {
    Some(i32::from_le_bytes(d.get(o..o.checked_add(4)?)?.try_into().ok()?))
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o.checked_add(2)?)?.try_into().ok()?))
}

fn f32_at(d: &[u8], o: usize) -> Option<f32> {
    i32_at(d, o).map(|v| f32::from_bits(v as u32))
}

fn vec_at(d: &[u8], o: usize) -> Option<[f32; 3]> {
    Some([f32_at(d, o)?, f32_at(d, o + 4)?, f32_at(d, o + 8)?])
}

fn at(base: usize, rel: i32) -> Option<usize> {
    usize::try_from(base as i64 + i64::from(rel)).ok()
}

fn count(v: i32, max: usize) -> Option<usize> {
    usize::try_from(v).ok().filter(|n| *n <= max)
}

fn cstr(d: &[u8], o: usize) -> Option<String> {
    let s = d.get(o..)?;
    let end = s.iter().position(|b| *b == 0).unwrap_or(s.len());
    Some(String::from_utf8_lossy(&s[..end]).into_owned())
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Mesh {
    pub material: i32,
    pub vertex_offset: i32,
    pub vertices: Vec<Vertex>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SubModel {
    pub num_vertices: i32,
    pub meshes: Vec<Mesh>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Studio {
    pub checksum: i32,
    pub flags: i32,
    pub name: String,
    pub hull_min: [f32; 3],
    pub hull_max: [f32; 3],
    pub textures: Vec<String>,
    pub cdtextures: Vec<String>,
    pub models: Vec<SubModel>,
    pub model_part: Vec<usize>,
}

pub(crate) fn studio_header(mdl: &[u8]) -> Option<Studio> {
    let id = mdl.get(0..4)?;
    if id != b"IDST" && id != b"IDAG" {
        return None;
    }
    let mut s = Studio {
        checksum: i32_at(mdl, 8)?,
        name: cstr(mdl.get(12..76)?, 0)?,
        hull_min: vec_at(mdl, 104)?,
        hull_max: vec_at(mdl, 116)?,
        flags: i32_at(mdl, 152)?,
        ..Studio::default()
    };
    if let Some(h2) = i32_at(mdl, 400).filter(|v| *v > 0).and_then(|v| usize::try_from(v).ok())
        && let Some(rel) = i32_at(mdl, h2 + 20).filter(|v| *v != 0)
        && let Some(name) = at(h2, rel).and_then(|o| cstr(mdl, o))
        && !name.is_empty()
    {
        s.name = name;
    }
    let ntex = count(i32_at(mdl, 204)?, 1 << 16)?;
    let tex = usize::try_from(i32_at(mdl, 208)?).ok()?;
    for i in 0..ntex {
        let t = tex + i * 64;
        s.textures.push(at(t, i32_at(mdl, t)?).and_then(|o| cstr(mdl, o)).unwrap_or_default());
    }
    let ncd = count(i32_at(mdl, 212)?, 1 << 16)?;
    let cd = usize::try_from(i32_at(mdl, 216)?).ok()?;
    for i in 0..ncd {
        s.cdtextures.push(at(0, i32_at(mdl, cd + i * 4)?).and_then(|o| cstr(mdl, o)).unwrap_or_default());
    }
    let nparts = count(i32_at(mdl, 232)?, 1 << 16)?;
    let parts = usize::try_from(i32_at(mdl, 236)?).ok()?;
    for p in 0..nparts {
        let bp = parts + p * 16;
        let nmodels = count(i32_at(mdl, bp + 4)?, 1 << 16)?;
        let first = at(bp, i32_at(mdl, bp + 12)?)?;
        for m in 0..nmodels {
            let mo = first + m * 148;
            let nmeshes = count(i32_at(mdl, mo + 72)?, 1 << 16)?;
            let meshes_at = at(mo, i32_at(mdl, mo + 76)?)?;
            let mut model = SubModel { num_vertices: i32_at(mdl, mo + 80)?, meshes: Vec::new() };
            for k in 0..nmeshes {
                let me = meshes_at + k * 116;
                model.meshes.push(Mesh {
                    material: i32_at(mdl, me)?,
                    vertex_offset: i32_at(mdl, me + 12)?,
                    vertices: vec![Vertex::default(); count(i32_at(mdl, me + 8)?, 1 << 20)?],
                });
            }
            s.models.push(model);
            s.model_part.push(p);
        }
    }
    Some(s)
}

fn model_vertex_bases(mdl: &[u8]) -> Option<Vec<i64>> {
    let nparts = count(i32_at(mdl, 232)?, 1 << 16)?;
    let parts = usize::try_from(i32_at(mdl, 236)?).ok()?;
    let mut out = Vec::new();
    for p in 0..nparts {
        let bp = parts + p * 16;
        let nmodels = count(i32_at(mdl, bp + 4)?, 1 << 16)?;
        let first = at(bp, i32_at(mdl, bp + 12)?)?;
        for m in 0..nmodels {
            out.push(i64::from(i32_at(mdl, first + m * 148 + 84)?) / VERTEX_SIZE as i64);
        }
    }
    Some(out)
}

pub(crate) fn vvd_vertices(vvd: &[u8], checksum: i32) -> Option<Vec<Vertex>> {
    if vvd.get(0..4)? != b"IDSV" || i32_at(vvd, 4)? != 4 || i32_at(vvd, 8)? != checksum {
        return None;
    }
    let read = |i: usize| -> Option<Vertex> {
        let o = usize::try_from(i32_at(vvd, 56)?).ok()?.checked_add(i.checked_mul(VERTEX_SIZE)?)?;
        let f = |k: usize| f32_at(vvd, o + k);
        Some(Vertex { pos: vec_at(vvd, o + 16)?, normal: vec_at(vvd, o + 28)?, uv: [f(40)?, f(44)?] })
    };
    let fixups = count(i32_at(vvd, 48)?, 1 << 20)?;
    let mut out = Vec::new();
    if fixups == 0 {
        let n = count(i32_at(vvd, 16)?, 1 << 22)?;
        for i in 0..n {
            out.push(read(i)?);
        }
        return Some(out);
    }
    let table = usize::try_from(i32_at(vvd, 52)?).ok()?;
    for k in 0..fixups {
        let f = table + k * 12;
        let (_lod, src, n) =
            (i32_at(vvd, f)?, count(i32_at(vvd, f + 4)?, 1 << 22)?, count(i32_at(vvd, f + 8)?, 1 << 22)?);
        for i in src..src + n {
            out.push(read(i)?);
        }
    }
    Some(out)
}

pub(crate) fn load_vertices(s: &mut Studio, mdl: &[u8], verts: &[Vertex]) -> bool {
    let Some(bases) = model_vertex_bases(mdl) else { return false };
    let mut ok = true;
    for (model, base) in s.models.iter_mut().zip(bases) {
        for mesh in &mut model.meshes {
            for (i, v) in mesh.vertices.iter_mut().enumerate() {
                match usize::try_from(base + i64::from(mesh.vertex_offset) + i as i64).ok().and_then(|g| verts.get(g)) {
                    Some(x) => *v = *x,
                    None => ok = false,
                }
            }
        }
    }
    ok
}

#[derive(Clone, Debug, Default)]
pub(crate) struct StripGroup {
    pub orig: Vec<u16>,
    pub tris: Vec<[u16; 3]>,
    pub has_strips: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Vtx {
    pub num_lods: usize,
    pub models: Vec<Vec<Vec<Vec<StripGroup>>>>,
}

fn strip_group(d: &[u8], g: usize) -> Option<StripGroup> {
    let nverts = count(i32_at(d, g)?, 1 << 20)?;
    let verts = at(g, i32_at(d, g + 4)?)?;
    let nidx = count(i32_at(d, g + 8)?, 1 << 22)?;
    let idx = at(g, i32_at(d, g + 12)?)?;
    let nstrips = count(i32_at(d, g + 16)?, 1 << 20)?;
    let strips = at(g, i32_at(d, g + 20)?)?;
    let mut out = StripGroup::default();
    for v in 0..nverts {
        out.orig.push(u16_at(d, verts + v * 9 + 4)?);
    }
    let index = |i: usize| -> Option<u16> {
        if i >= nidx {
            return None;
        }
        u16_at(d, idx + i * 2)
    };
    for s in 0..nstrips {
        let st = strips + s * 27;
        let n = count(i32_at(d, st)?, 1 << 22)?;
        let first = count(i32_at(d, st + 4)?, 1 << 22)?;
        if *d.get(st + 18)? & STRIP_IS_TRILIST == 0 {
            out.has_strips = true;
            break;
        }
        let mut i = 0;
        while i < n {
            let k = first + i;
            out.tris.push([index(k)?, index(k + 1)?, index(k + 2)?]);
            i += 3;
        }
    }
    Some(out)
}

pub(crate) fn vtx(d: &[u8], checksum: i32, s: &Studio) -> Option<Vtx> {
    if i32_at(d, 0)? != VTX_VERSION || i32_at(d, 16)? != checksum {
        return None;
    }
    let num_lods = count(i32_at(d, 20)?, 8)?;
    let nparts = count(i32_at(d, 28)?, 1 << 16)?;
    let parts = usize::try_from(i32_at(d, 32)?).ok()?;
    let mut out = Vtx { num_lods, models: Vec::new() };
    for (mi, model) in s.models.iter().enumerate() {
        let part = s.model_part[mi];
        let local = s.model_part[..mi].iter().filter(|p| **p == part).count();
        let mut lods = Vec::new();
        let vm = (part < nparts)
            .then(|| {
                let bp = parts + part * 8;
                let nm = count(i32_at(d, bp)?, 1 << 16)?;
                (local < nm).then_some(())?;
                Some(at(bp, i32_at(d, bp + 4)?)? + local * 8)
            })
            .flatten();
        for lod in 0..num_lods {
            let mut meshes = Vec::new();
            let lo = vm.and_then(|vm| Some(at(vm, i32_at(d, vm + 4)?)? + lod * 12));
            for mesh in 0..model.meshes.len() {
                let groups = lo
                    .and_then(|lo| {
                        let nmesh = count(i32_at(d, lo)?, 1 << 16)?;
                        (mesh < nmesh).then_some(())?;
                        let me = at(lo, i32_at(d, lo + 4)?)? + mesh * 9;
                        let ng = count(i32_at(d, me)?, 1 << 16)?;
                        let first = at(me, i32_at(d, me + 4)?)?;
                        (0..ng).map(|g| strip_group(d, first + g * 25)).collect::<Option<Vec<_>>>()
                    })
                    .unwrap_or_default();
                meshes.push(groups);
            }
            lods.push(meshes);
        }
        out.models.push(lods);
    }
    Some(out)
}

pub(crate) fn phy_first_solid(phy: &[u8]) -> Option<&[u8]> {
    if i32_at(phy, 0)? != 16 || i32_at(phy, 8)? <= 0 {
        return None;
    }
    let size = count(i32_at(phy, 16)?, phy.len())?;
    phy.get(20..20 + size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_files_give_none() {
        assert!(studio_header(b"IDST").is_none());
        assert!(vvd_vertices(b"IDSV\x04\0\0\0", 0).is_none());
        assert!(phy_first_solid(&[16, 0, 0, 0]).is_none());
        let mut phy = vec![16, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 1, 2, 3];
        assert_eq!(phy_first_solid(&phy), Some(&[1u8, 2, 3][..]));
        phy[16] = 9;
        assert!(phy_first_solid(&phy).is_none());
    }
}
