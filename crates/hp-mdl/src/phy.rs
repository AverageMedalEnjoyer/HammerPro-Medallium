use crate::error::{FileKind, MdlError};
use crate::reader::{Reader, Result};

const PHY_HEADER: usize = 16;
const VPHY_HEADER: usize = 28;
const SURFACE_HEADER: usize = 48;
const LEDGE: usize = 16;
const TRIANGLE: usize = 16;
const POINT: usize = 16;
const MAX_SOLIDS: usize = 1024;
const METERS_TO_UNITS: f32 = 1.0 / 0.0254;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CollisionSolid {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    pub ledges: Vec<std::ops::Range<usize>>,
}

impl CollisionSolid {
    pub fn piece_triangles(&self) -> impl Iterator<Item = &[u32; 3]> {
        let all = self.ledges.is_empty().then_some(&self.triangles[..]);
        let pieces = self.ledges.iter().filter_map(|r| self.triangles.get(r.clone()));
        all.into_iter().chain(pieces).flatten()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CollisionModel {
    pub solids: Vec<CollisionSolid>,
}

impl CollisionModel {
    pub fn edges(&self) -> Vec<[[f32; 3]; 2]> {
        let mut out = Vec::new();
        for s in &self.solids {
            let mut seen = std::collections::HashSet::new();
            for t in s.piece_triangles() {
                for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                    let key = (a.min(b), a.max(b));
                    if a != b
                        && seen.insert(key)
                        && let (Some(pa), Some(pb)) = (s.vertices.get(a as usize), s.vertices.get(b as usize))
                    {
                        out.push([*pa, *pb]);
                    }
                }
            }
        }
        out
    }

    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for p in self.solids.iter().flat_map(|s| &s.vertices) {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        (lo[0] <= hi[0]).then_some((lo, hi))
    }

    pub fn triangle_count(&self) -> usize {
        self.solids.iter().map(|s| s.triangles.len()).sum()
    }
}

pub fn parse_phy(data: &[u8]) -> std::result::Result<CollisionModel, MdlError> {
    let r = Reader::new(data, FileKind::Phy);
    let header_size = usize::try_from(r.i32(0)?).map_err(|_| r.invalid("header size", 0))?;
    if header_size < PHY_HEADER || header_size > data.len() {
        return Err(r.invalid("header size", header_size as i64));
    }
    let count = r.i32(8)?;
    let count = usize::try_from(count)
        .ok()
        .filter(|c| *c <= MAX_SOLIDS)
        .ok_or_else(|| r.invalid("solid count", i64::from(count)))?;
    let mut pos = header_size;
    let mut solids = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        let size = r.i32(pos)?;
        let size = usize::try_from(size).map_err(|_| r.invalid("solid size", i64::from(size)))?;
        let start = pos + 4;
        r.check(start, size, "solid")?;
        let solid_data = &data[start..start + size];
        solids.push(parse_solid(solid_data)?);
        pos = start + size;
    }
    Ok(CollisionModel { solids })
}

pub fn parse_phy_for_model(phy: &[u8], mdl: &[u8], pose: crate::Pose) -> std::result::Result<CollisionModel, MdlError> {
    let mut model = parse_phy(phy)?;
    if model.solids.len() < 2 {
        return Ok(model);
    }
    let studio = crate::mdl::parse(mdl)?;
    let posing = crate::pose::posing(mdl, &studio, pose);
    let names = solid_bone_names(phy);
    for (i, solid) in model.solids.iter_mut().enumerate() {
        let by_name = names.get(&i).and_then(|n| studio.bones.iter().position(|b| b.name.eq_ignore_ascii_case(n)));
        let bone = by_name.unwrap_or(i).min(studio.bones.len().saturating_sub(1));
        let Some(b) = studio.bones.get(bone) else { continue };
        let skin = posing.skin.as_ref().and_then(|s| s.get(bone)).copied().unwrap_or(crate::pose::IDENTITY);
        let Some(inv) = invert_affine(&b.pose_to_bone) else { continue };
        let m = crate::pose::mul(&skin, &inv);
        for v in &mut solid.vertices {
            let t = transform(&m, *v);
            if t.iter().all(|c| c.is_finite()) {
                *v = t;
            }
        }
    }
    Ok(model)
}

fn transform(m: &[[f32; 4]; 3], p: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + m[0][3],
        m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + m[1][3],
        m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + m[2][3],
    ]
}

fn solid_bone_names(data: &[u8]) -> std::collections::HashMap<usize, String> {
    let mut out = std::collections::HashMap::new();
    let text = String::from_utf8_lossy(data);
    let mut rest: &str = &text;
    while let Some(at) = rest.find("solid {") {
        rest = &rest[at + 7..];
        let block = rest.split('}').next().unwrap_or("");
        let mut quoted = block.split('"').skip(1).step_by(2);
        let (mut index, mut name) = (None, None);
        while let (Some(k), Some(v)) = (quoted.next(), quoted.next()) {
            match k {
                "index" => index = v.trim().parse::<usize>().ok(),
                "name" => name = Some(v.to_owned()),
                _ => {}
            }
        }
        if let (Some(i), Some(n)) = (index, name)
            && out.len() < MAX_SOLIDS
        {
            out.insert(i, n);
        }
    }
    out
}

fn invert_affine(m: &[[f32; 4]; 3]) -> Option<[[f32; 4]; 3]> {
    let a = |r: usize, c: usize| f64::from(m[r][c]);
    let det = a(0, 0) * (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1)) - a(0, 1) * (a(1, 0) * a(2, 2) - a(1, 2) * a(2, 0))
        + a(0, 2) * (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0));
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let r = [
        [
            (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1)) * inv,
            (a(0, 2) * a(2, 1) - a(0, 1) * a(2, 2)) * inv,
            (a(0, 1) * a(1, 2) - a(0, 2) * a(1, 1)) * inv,
        ],
        [
            (a(1, 2) * a(2, 0) - a(1, 0) * a(2, 2)) * inv,
            (a(0, 0) * a(2, 2) - a(0, 2) * a(2, 0)) * inv,
            (a(0, 2) * a(1, 0) - a(0, 0) * a(1, 2)) * inv,
        ],
        [
            (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0)) * inv,
            (a(0, 1) * a(2, 0) - a(0, 0) * a(2, 1)) * inv,
            (a(0, 0) * a(1, 1) - a(0, 1) * a(1, 0)) * inv,
        ],
    ];
    let t = [a(0, 3), a(1, 3), a(2, 3)];
    let mut out = [[0.0f32; 4]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = r[i][j] as f32;
        }
        out[i][3] = (-(r[i][0] * t[0] + r[i][1] * t[1] + r[i][2] * t[2])) as f32;
    }
    out.iter().flatten().all(|v| v.is_finite()).then_some(out)
}

fn parse_solid(data: &[u8]) -> Result<CollisionSolid> {
    let r = Reader::new(data, FileKind::Phy);
    let surface = if r.bytes(0, 4, "solid id")? == b"VPHY" { VPHY_HEADER } else { 0 };
    r.check(surface, SURFACE_HEADER, "compact surface")?;
    let tree = r.i32(surface + 32)?;
    let tree_end = usize::try_from(tree).ok().and_then(|t| surface.checked_add(t)).filter(|t| *t <= data.len());
    let mut ledge = surface + SURFACE_HEADER;
    let mut end = tree_end.unwrap_or(data.len());
    let mut out = CollisionSolid::default();
    let mut index_of: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
    while ledge + LEDGE <= end {
        let point_offset = r.i32(ledge)?;
        let has_children = r.u32(ledge + 8)? & 3 != 0;
        let n_triangles = r.i16(ledge + 12)?;
        let n_triangles =
            usize::try_from(n_triangles).map_err(|_| r.invalid("ledge triangles", i64::from(n_triangles)))?;
        let points = i64::try_from(ledge).unwrap_or(i64::MAX).checked_add(i64::from(point_offset));
        let points = points
            .and_then(|p| usize::try_from(p).ok())
            .filter(|p| *p <= data.len())
            .ok_or_else(|| r.invalid("ledge points", i64::from(point_offset)))?;
        if points < ledge + LEDGE {
            return Err(r.invalid("ledge points", i64::from(point_offset)));
        }
        end = end.min(points);
        let triangles = ledge + LEDGE;
        r.check(triangles, n_triangles * TRIANGLE, "ledge triangles")?;
        let first = out.triangles.len();
        for t in 0..n_triangles {
            let tri = triangles + t * TRIANGLE;
            let mut corners = [0u32; 3];
            for (e, corner) in corners.iter_mut().enumerate() {
                let edge = r.u32(tri + 4 + e * 4)?;
                let point = (edge & 0xffff) as usize;
                let at = points
                    .checked_add(point * POINT)
                    .filter(|a| a + 12 <= data.len())
                    .ok_or_else(|| r.invalid("triangle point", point as i64))?;
                let next = out.vertices.len() as u32;
                *corner = match index_of.get(&at) {
                    Some(i) => *i,
                    None => {
                        let p = [r.f32(at)?, r.f32(at + 4)?, r.f32(at + 8)?];
                        if !p.iter().all(|v| v.is_finite()) {
                            return Err(r.invalid("point", point as i64));
                        }
                        out.vertices.push([p[0] * METERS_TO_UNITS, p[2] * METERS_TO_UNITS, -p[1] * METERS_TO_UNITS]);
                        index_of.insert(at, next);
                        next
                    }
                };
            }
            out.triangles.push(corners);
        }
        if !has_children && out.triangles.len() > first {
            out.ledges.push(first..out.triangles.len());
        }
        ledge = triangles + n_triangles * TRIANGLE;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn tetra_phy(vphy: bool) -> Vec<u8> {
        let points: [[f32; 3]; 4] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]];
        let tris: [[u16; 3]; 4] = [[0, 1, 2], [0, 3, 1], [0, 2, 3], [1, 3, 2]];
        let mut solid = Vec::new();
        if vphy {
            solid.extend_from_slice(b"VPHY");
            solid.extend_from_slice(&[0u8; 24]);
        }
        let surface = solid.len();
        solid.extend_from_slice(&[0u8; 48]);
        let ledge = solid.len();
        let points_at = ledge + 16 + tris.len() * 16;
        solid.extend_from_slice(&((points_at - ledge) as i32).to_le_bytes());
        solid.extend_from_slice(&0i32.to_le_bytes());
        solid.extend_from_slice(&0u32.to_le_bytes());
        solid.extend_from_slice(&(tris.len() as i16).to_le_bytes());
        solid.extend_from_slice(&0i16.to_le_bytes());
        for t in tris {
            solid.extend_from_slice(&0u32.to_le_bytes());
            for p in t {
                solid.extend_from_slice(&u32::from(p).to_le_bytes());
            }
        }
        for p in points {
            for v in p {
                solid.extend_from_slice(&v.to_le_bytes());
            }
            solid.extend_from_slice(&0f32.to_le_bytes());
        }
        let tree = (solid.len() - surface) as i32;
        solid[surface + 32..surface + 36].copy_from_slice(&tree.to_le_bytes());
        solid.extend_from_slice(&[0u8; 28]);
        let mut out = Vec::new();
        out.extend_from_slice(&16i32.to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes());
        out.extend_from_slice(&1i32.to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes());
        out.extend_from_slice(&(solid.len() as i32).to_le_bytes());
        out.extend_from_slice(&solid);
        out.extend_from_slice(b"solid {\n\"index\" \"0\"\n}\n\0");
        out
    }

    #[test]
    fn tetrahedron_with_and_without_vphy_header() {
        for vphy in [true, false] {
            let m = parse_phy(&tetra_phy(vphy)).expect("parse");
            assert_eq!(m.solids.len(), 1);
            let s = &m.solids[0];
            assert_eq!(s.vertices.len(), 4, "shared points are welded");
            assert_eq!(s.triangles.len(), 4);
            assert_eq!(s.ledges, vec![0..4], "one convex piece");
            assert_eq!(m.edges().len(), 6);
            let u = 1.0 / 0.0254;
            assert!(s.vertices.contains(&[u, 0.0, 0.0]));
            assert!(s.vertices.contains(&[0.0, 0.0, u]), "IVP -Y is Source +Z");
            assert!(s.vertices.contains(&[0.0, u, 0.0]), "IVP +Z is Source +Y");
            let (lo, hi) = m.bounds().expect("bounds");
            assert_eq!((lo, hi), ([0.0; 3], [u, u, u]));
        }
    }

    #[test]
    fn hull_ledges_are_not_convex_pieces() {
        let mut data = tetra_phy(true);
        let flags = PHY_HEADER + 4 + VPHY_HEADER + SURFACE_HEADER + 8;
        data[flags] |= 1;
        let m = parse_phy(&data).expect("parse");
        assert_eq!(m.solids[0].triangles.len(), 4, "its triangles are still read");
        assert!(m.solids[0].ledges.is_empty(), "a hull over other ledges is no piece");
    }

    fn ledges_phy(points: &[[f32; 3]], ledges: &[(bool, &[[u16; 3]])]) -> Vec<u8> {
        let mut solid = Vec::new();
        solid.extend_from_slice(b"VPHY");
        solid.extend_from_slice(&[0u8; 24]);
        let surface = solid.len();
        solid.extend_from_slice(&[0u8; 48]);
        let points_at = solid.len() + ledges.iter().map(|(_, t)| LEDGE + t.len() * TRIANGLE).sum::<usize>();
        for (hull, tris) in ledges {
            let ledge = solid.len();
            solid.extend_from_slice(&((points_at - ledge) as i32).to_le_bytes());
            solid.extend_from_slice(&0i32.to_le_bytes());
            solid.extend_from_slice(&u32::from(*hull).to_le_bytes());
            solid.extend_from_slice(&(tris.len() as i16).to_le_bytes());
            solid.extend_from_slice(&0i16.to_le_bytes());
            for t in *tris {
                solid.extend_from_slice(&0u32.to_le_bytes());
                for p in t {
                    solid.extend_from_slice(&u32::from(*p).to_le_bytes());
                }
            }
        }
        for p in points {
            for v in p {
                solid.extend_from_slice(&v.to_le_bytes());
            }
            solid.extend_from_slice(&0f32.to_le_bytes());
        }
        let tree = (solid.len() - surface) as i32;
        solid[surface + 32..surface + 36].copy_from_slice(&tree.to_le_bytes());
        solid.extend_from_slice(&[0u8; 28]);
        let mut out = Vec::new();
        for v in [16i32, 0, 1, 0, solid.len() as i32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&solid);
        out.extend_from_slice(b"solid {\n\"index\" \"0\"\n}\n\0");
        out
    }

    #[test]
    fn concave_solids_collide_as_their_pieces() {
        let mut points = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]];
        points.extend(points.clone().iter().map(|p| [p[0] + 3.0, p[1], p[2]]));
        let a: [[u16; 3]; 4] = [[0, 1, 2], [0, 3, 1], [0, 2, 3], [1, 3, 2]];
        let b = a.map(|t| t.map(|i| i + 4));
        let hull: [[u16; 3]; 6] = [[0, 5, 6], [0, 4, 5], [0, 6, 7], [0, 7, 3], [0, 3, 2], [5, 7, 6]];
        let m = parse_phy(&ledges_phy(&points, &[(false, &a), (false, &b), (true, &hull)])).expect("parse");
        let s = &m.solids[0];
        assert_eq!(s.vertices.len(), 8);
        assert_eq!(s.triangles.len(), 14, "the hull is read");
        assert_eq!(s.ledges, vec![0..4, 4..8]);
        let pieces: Vec<[u32; 3]> = s.piece_triangles().copied().collect();
        assert_eq!(pieces, s.triangles[..8], "the hull is no part of the shape");
        let side = |i: u32| s.vertices[i as usize][0] > 50.0;
        assert!(pieces.iter().all(|t| t.iter().all(|i| side(*i) == side(t[0]))));
        assert_eq!(m.edges().len(), 12, "the wireframe draws the two pieces only");
        let bare = CollisionSolid { ledges: Vec::new(), ..s.clone() };
        assert_eq!(bare.piece_triangles().count(), 14);
    }

    #[test]
    fn bone_names_and_inverse_transforms() {
        assert!(solid_bone_names(&tetra_phy(true)).is_empty(), "the sample names no bone");
        let text = b"xx solid {\n\"index\" \"1\"\n\"name\" \"Arm\"\n} solid { \"index\" \"x\" }";
        let names = solid_bone_names(text);
        assert_eq!(names.get(&1).map(String::as_str), Some("Arm"));
        assert_eq!(names.len(), 1);
        let m = [[0.0, -1.0, 0.0, 5.0], [1.0, 0.0, 0.0, -2.0], [0.0, 0.0, 1.0, 3.0]];
        let inv = invert_affine(&m).expect("invertible");
        let p = [1.0f32, 2.0, 3.0];
        let back = transform(&inv, transform(&m, p));
        assert!((0..3).all(|i| (back[i] - p[i]).abs() < 1e-5), "{back:?}");
        assert!(invert_affine(&[[0.0; 4]; 3]).is_none());
        assert!(invert_affine(&[[f32::NAN; 4]; 3]).is_none());
    }

    #[test]
    fn hostile_files_fail_cleanly() {
        let good = tetra_phy(true);
        for len in 0..good.len() {
            let _ = parse_phy(&good[..len]);
        }
        for i in 0..good.len() {
            for v in [0u8, 0x7f, 0x80, 0xff] {
                let mut bad = good.clone();
                bad[i] = v;
                if let Ok(m) = parse_phy(&bad) {
                    assert!(m.triangle_count() <= bad.len() / 16);
                }
            }
        }
        let mut bad = good.clone();
        bad[8..12].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(parse_phy(&bad).is_err());
        assert!(parse_phy(&[]).is_err());
    }
}
