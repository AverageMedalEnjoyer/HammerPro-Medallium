use super::Part;
use super::buf::Buf;

pub(crate) const VERTEX_SIZE: usize = 48;
pub(crate) const TANGENT_SIZE: usize = 16;
const VVD_HEADER: usize = 64;

#[derive(Clone, Debug, Default)]
pub(crate) struct Layout {
    pub part_first: Vec<usize>,
    pub total: usize,
}

pub(crate) fn vvd_vertices(parts: &[Part]) -> (Vec<u8>, Layout) {
    let mut layout = Layout::default();
    let mut verts = Buf::default();
    let mut tangents = Buf::default();
    for part in parts {
        layout.part_first.push(layout.total);
        for v in part.meshes.iter().flat_map(|m| m.verts.iter()) {
            verts.f32(1.0);
            verts.f32(0.0);
            verts.f32(0.0);
            verts.b.extend_from_slice(&[0, 0, 0, 1]);
            verts.vec3(v.pos);
            verts.vec3(v.normal);
            verts.f32(v.uv.x as f32);
            verts.f32(v.uv.y as f32);
            for c in v.tangent {
                tangents.f32(c as f32);
            }
            layout.total += 1;
        }
    }
    verts.b.extend_from_slice(&tangents.b);
    (verts.b, layout)
}

pub(crate) fn vvd(body: &[u8], layout: &Layout, checksum: i32, lods: usize) -> Vec<u8> {
    let mut b = Buf::default();
    b.b.extend_from_slice(b"IDSV");
    b.i32(4);
    b.i32(checksum);
    b.i32(lods.max(1) as i32);
    for _ in 0..8 {
        b.i32(layout.total as i32);
    }
    b.i32(0);
    b.i32(VVD_HEADER as i32);
    b.i32(VVD_HEADER as i32);
    b.i32((VVD_HEADER + layout.total * VERTEX_SIZE) as i32);
    debug_assert_eq!(b.pos(), VVD_HEADER);
    b.b.extend_from_slice(body);
    b.b
}

const HEADER: usize = 36;
const BODYPART: usize = 8;
const MODEL: usize = 8;
const LOD: usize = 12;
const MESH: usize = 9;
const STRIP_GROUP: usize = 25;
const STRIP: usize = 27;
const VERTEX: usize = 9;
const GROUP_HW_SKINNED: u8 = 0x02;
const STRIP_TRILIST: u8 = 0x01;

pub(crate) fn vtx(parts: &[Part], checksum: i32, switches: &[f32]) -> Vec<u8> {
    let lods = switches.len().max(1);
    let mut b = Buf::default();
    b.i32(7);
    b.i32(24);
    b.u16(53);
    b.u16(9);
    b.i32(3);
    b.i32(checksum);
    b.i32(lods as i32);
    let replacement_at = b.pos();
    b.i32(0);
    b.i32(parts.len() as i32);
    b.i32(HEADER as i32);
    debug_assert_eq!(b.pos(), HEADER);
    let bodyparts_at = b.pos();
    b.zeros(parts.len() * BODYPART);
    let models_at = b.pos();
    b.zeros(parts.len() * MODEL);
    let lods_at = b.pos();
    b.zeros(parts.len() * lods * LOD);
    for (i, part) in parts.iter().enumerate() {
        let bp = bodyparts_at + i * BODYPART;
        let model = models_at + i * MODEL;
        let first_lod = lods_at + i * lods * LOD;
        b.set_i32(bp, 1);
        b.set_i32(bp + 4, (model - bp) as i32);
        b.set_i32(model, lods as i32);
        b.set_i32(model + 4, (first_lod - model) as i32);
        for level in 0..lods {
            let lod = first_lod + level * LOD;
            b.set_i32(lod, part.meshes.len() as i32);
            let meshes_at = b.pos();
            b.set_i32(lod + 4, (meshes_at - lod) as i32);
            let switch = switches.get(level).copied().unwrap_or(0.0);
            b.set_i32(lod + 8, switch.to_bits() as i32);
            b.zeros(part.meshes.len() * MESH);
            for (j, mesh) in part.meshes.iter().enumerate() {
                let tris: &[[u16; 3]] =
                    if level == 0 { &mesh.tris } else { mesh.lods.get(level - 1).map_or(&[][..], Vec::as_slice) };
                let m = meshes_at + j * MESH;
                if tris.is_empty() {
                    continue;
                }
                let mut local = vec![u16::MAX; mesh.verts.len()];
                let mut used: Vec<u16> = Vec::new();
                for t in tris {
                    for &v in t {
                        if local[usize::from(v)] == u16::MAX {
                            local[usize::from(v)] = used.len() as u16;
                            used.push(v);
                        }
                    }
                }
                used.sort_unstable();
                for (k, &v) in used.iter().enumerate() {
                    local[usize::from(v)] = k as u16;
                }
                let group = b.pos();
                b.set_i32(m, 1);
                b.set_i32(m + 4, (group - m) as i32);
                let nv = used.len();
                let ni = tris.len() * 3;
                let strip = group + STRIP_GROUP;
                let verts = strip + STRIP;
                let indices = verts + nv * VERTEX;
                b.i32(nv as i32);
                b.i32((verts - group) as i32);
                b.i32(ni as i32);
                b.i32((indices - group) as i32);
                b.i32(1);
                b.i32((strip - group) as i32);
                b.u8(GROUP_HW_SKINNED);
                debug_assert_eq!(b.pos(), strip);
                b.i32(ni as i32);
                b.i32(0);
                b.i32(nv as i32);
                b.i32(0);
                b.i16(0);
                b.u8(STRIP_TRILIST);
                b.i32(0);
                b.i32((indices + ni * 2 - strip) as i32);
                debug_assert_eq!(b.pos(), verts);
                for &v in &used {
                    b.b.extend_from_slice(&[0, 1, 2, 0]);
                    b.u16(v);
                    b.b.extend_from_slice(&[0, 0, 0]);
                }
                for t in tris {
                    let l = t.map(|v| local[usize::from(v)]);
                    b.u16(l[0]);
                    b.u16(l[2]);
                    b.u16(l[1]);
                }
            }
        }
    }
    let at = b.pos();
    b.set_i32(replacement_at, at as i32);
    for _ in 0..lods {
        b.i32(0);
        b.i32(0);
    }
    b.b
}
