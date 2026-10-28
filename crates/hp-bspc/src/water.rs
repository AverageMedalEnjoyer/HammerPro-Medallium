use ahash::AHashMap;
use glam::DVec3;

use crate::brush::MapBrush;
use crate::emit::{Buf, lump};
use crate::materials::{contents, surf};
use crate::planes::PlaneSet;
use crate::portals::Portals;
use crate::tree::Tree;
use crate::winding::ON_EPSILON;

const MAX_COORD: f64 = 16384.0;
pub const FAR_FROM_WATER: u16 = 65535;

#[derive(Clone, Copy, Debug)]
struct WaterLeaf {
    leaf: usize,
    normal: DVec3,
    dist: f64,
    min_z: f64,
    has_surface: bool,
    texinfo: i32,
}

fn is_lower(a: &WaterLeaf, b: &WaterLeaf) -> bool {
    if a.has_surface && b.has_surface {
        if b.normal.z > a.normal.z {
            return false;
        }
        return true;
    }
    a.has_surface
}

fn tree_leaves(tree: &Tree) -> Vec<usize> {
    let mut out = Vec::new();
    let mut stack = vec![tree.head];
    while let Some(n) = stack.pop() {
        let Some(node) = tree.nodes.get(n) else { continue };
        if node.is_leaf() {
            if n != tree.outside {
                out.push(n);
            }
        } else {
            stack.extend(node.children);
        }
    }
    out.sort_unstable();
    out
}

#[derive(Clone, Debug)]
pub struct WaterModel {
    pub model: usize,
    pub contents: u32,
    pub normal: DVec3,
    pub dist: f64,
    pub has_surface: bool,
    pub min_z: f64,
    pub texinfo: i32,
    pub record: usize,
    pub leaves: Vec<usize>,
    pub brushes: Vec<usize>,
}

impl WaterModel {
    pub fn depth(&self) -> i32 {
        (self.dist as f32 - self.min_z as f32) as i32
    }
}

#[derive(Clone, Debug, Default)]
pub struct WaterData {
    pub records: Vec<(f32, f32, i32)>,
    pub models: Vec<WaterModel>,
}

impl WaterData {
    pub fn lump(&self) -> Vec<u8> {
        let mut b = Buf::default();
        for (z, min_z, ti) in &self.records {
            b.f32(*z);
            b.f32(*min_z);
            b.i16(*ti as i16);
            b.i16(0);
        }
        b.0
    }

    pub fn add_model(
        &mut self,
        model: usize,
        tree: &mut Tree,
        portals: &Portals,
        planes: &PlaneSet,
        brushes: &[MapBrush],
    ) {
        let leaves = tree_leaves(tree);
        let mut written = vec![false; tree.nodes.len()];
        for &l in &leaves {
            written[l] = true;
        }
        for n in &mut tree.nodes {
            n.water = None;
        }
        let mut list: Vec<WaterLeaf> = Vec::new();
        for &l in &leaves {
            let node = &tree.nodes[l];
            if node.contents & contents::MASK_WATER == 0 {
                continue;
            }
            let mut w = WaterLeaf {
                leaf: l,
                normal: DVec3::Z,
                dist: MAX_COORD,
                min_z: MAX_COORD,
                has_surface: false,
                texinfo: -1,
            };
            for &pid in &node.portals {
                let p = &portals.list[pid];
                let Some((bi, si)) = p.side else { continue };
                let Some(side) = brushes.get(bi).and_then(|b| b.sides.get(si)) else { continue };
                let other = p.nodes[usize::from(p.nodes[0] == l)];
                let oc = tree.nodes[other].contents;
                if oc & (contents::MASK_WATER | contents::MASK_SOLID) != 0 {
                    continue;
                }
                let plane = planes.get(side.plane);
                if w.has_surface {
                    if w.normal.z > plane.normal.z {
                        continue;
                    }
                    if w.normal.z == plane.normal.z && w.dist >= plane.dist {
                        continue;
                    }
                }
                if plane.normal.z <= 0.0 {
                    continue;
                }
                w.dist = plane.dist;
                w.normal = plane.normal;
                w.has_surface = true;
                w.texinfo = side.texinfo;
            }
            let at = list.iter().position(|x| is_lower(&w, x)).unwrap_or(list.len());
            list.insert(at, w);
        }
        let mut visited = vec![false; tree.nodes.len()];
        for base in &list {
            let mut base = *base;
            let base_contents = tree.nodes[base.leaf].contents;
            let base_water = base_contents & contents::MASK_WATER;
            let crosses = |pid: usize, base: &WaterLeaf| {
                if !base.has_surface {
                    return false;
                }
                portals.list[pid].winding.points.iter().any(|q| q.dot(base.normal) - base.dist > ON_EPSILON)
            };
            let mut group = Vec::new();
            let mut stack = vec![base.leaf];
            while let Some(l) = stack.pop() {
                if !written[l] || visited[l] || tree.nodes[l].contents & base_water == 0 {
                    continue;
                }
                if tree.nodes[l].portals.iter().any(|&pid| crosses(pid, &base)) {
                    continue;
                }
                visited[l] = true;
                group.push(l);
                base.min_z = base.min_z.min(tree.nodes[l].mins.z);
                for &pid in &tree.nodes[l].portals {
                    let p = &portals.list[pid];
                    stack.push(p.nodes[usize::from(p.nodes[0] == l)]);
                }
            }
            if group.is_empty() {
                continue;
            }
            if base.texinfo < 0 {
                base.texinfo = tree.nodes[base.leaf]
                    .brushes
                    .iter()
                    .filter_map(|f| brushes.get(f.original))
                    .filter(|b| b.contents & base_water != 0)
                    .flat_map(|b| b.sides.iter())
                    .find(|s| !s.bevel && s.texinfo >= 0 && s.surf & (surf::NODRAW | surf::SKIP | surf::HINT) == 0)
                    .map_or(0, |s| s.texinfo);
            }
            let rec = (base.dist as f32, base.min_z as f32, base.texinfo);
            let id = match self.records.iter().position(|r| *r == rec) {
                Some(i) => i,
                None => {
                    self.records.push(rec);
                    self.records.len() - 1
                }
            };
            let mut in_leaves: Vec<usize> = Vec::new();
            for &l in &group {
                if model == 0 {
                    tree.nodes[l].water = Some(id as u16);
                }
                let node = &tree.nodes[l];
                for b in node.brushes.iter().map(|f| f.original).chain(node.extra_brushes.iter().copied()) {
                    if brushes.get(b).is_some_and(|x| x.contents & base_contents != 0) && !in_leaves.contains(&b) {
                        in_leaves.push(b);
                    }
                }
            }
            in_leaves.sort_unstable();
            self.models.push(WaterModel {
                model,
                contents: base_contents,
                normal: base.normal,
                dist: base.dist,
                has_surface: base.has_surface,
                min_z: base.min_z,
                texinfo: base.texinfo,
                record: id,
                leaves: group,
                brushes: in_leaves,
            });
        }
    }
}

pub fn depth_material(map: &str, material: &str, depth: i32) -> (String, String) {
    let base = crate::materials::material_path(material).to_ascii_lowercase();
    (format!("maps/{}/{}_depth_{}", map.to_ascii_lowercase(), base, depth), base)
}

fn i16_at(d: &[u8], o: usize) -> i16 {
    d.get(o..o + 2).map_or(0, |b| i16::from_le_bytes([b[0], b[1]]))
}

fn u16_at(d: &[u8], o: usize) -> u16 {
    d.get(o..o + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
}

fn i32_at(d: &[u8], o: usize) -> i32 {
    d.get(o..o + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn f32_at(d: &[u8], o: usize) -> f32 {
    d.get(o..o + 4).map_or(0.0, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn box_distance(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> f64 {
    let gap = |i: usize| (a.0[i] - b.1[i]).max(b.0[i] - a.1[i]).max(0.0);
    DVec3::new(gap(0), gap(1), gap(2)).length()
}

pub fn fog_volumes(data: &mut AHashMap<usize, (Vec<u8>, u32)>, pvs: Option<&[Vec<u8>]>) -> usize {
    let get = |i: usize| data.get(&i).map_or(Vec::new(), |(d, _)| d.clone());
    let leafs_version = data.get(&lump::LEAFS).map_or(0, |(_, v)| *v);
    let mut leafs = get(lump::LEAFS);
    let (leaffaces, faces, texinfo) = (get(lump::LEAFFACES), get(lump::FACES), get(lump::TEXINFO));
    let (surfedges, edges, verts) = (get(lump::SURFEDGES), get(lump::EDGES), get(lump::VERTEXES));
    let n = leafs.len() / 32;
    let contents_of = |l: &[u8], i: usize| i32_at(l, i * 32) as u32;
    let cluster_of = |l: &[u8], i: usize| i16_at(l, i * 32 + 4);
    let water_of = |l: &[u8], i: usize| i16_at(l, i * 32 + 28);
    let visible = |from: i16, to: i16| match pvs {
        None => true,
        Some(rows) => rows
            .get(from as usize)
            .is_some_and(|r| r.get(to as usize >> 3).is_some_and(|b| b & (1 << (to as usize & 7)) != 0)),
    };
    let mut by_cluster: AHashMap<i16, Vec<usize>> = AHashMap::new();
    for i in 0..n {
        let c = cluster_of(&leafs, i);
        if c >= 0 {
            by_cluster.entry(c).or_default().push(i);
        }
    }
    let mut clusters: Vec<i16> = by_cluster.keys().copied().collect();
    clusters.sort_unstable();
    for i in 0..n {
        let c = contents_of(&leafs, i) & !contents::TESTFOGVOLUME;
        leafs[i * 32..i * 32 + 4].copy_from_slice(&c.to_le_bytes());
    }
    let mut sources: Vec<i16> = (0..n)
        .filter(|&i| contents_of(&leafs, i) & (contents::SOLID | contents::SLIME) == 0 && water_of(&leafs, i) != -1)
        .map(|i| cluster_of(&leafs, i))
        .filter(|&c| c >= 0)
        .collect();
    sources.sort_unstable();
    sources.dedup();
    let mut marked = 0;
    for &to in &clusters {
        if !sources.iter().any(|&from| from != to && visible(from, to)) {
            continue;
        }
        for &k in &by_cluster[&to] {
            let kc = contents_of(&leafs, k);
            if kc & (contents::SOLID | contents::TESTFOGVOLUME) != 0 || water_of(&leafs, k) != -1 {
                continue;
            }
            leafs[k * 32..k * 32 + 4].copy_from_slice(&(kc | contents::TESTFOGVOLUME).to_le_bytes());
            marked += 1;
        }
    }
    let sees_water = |l: &[u8], i: usize| contents_of(l, i) & contents::TESTFOGVOLUME != 0 || water_of(l, i) != -1;
    let face_box = |f: usize| {
        let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
        let (first, count) = (i32_at(&faces, f * 56 + 4).max(0) as usize, i16_at(&faces, f * 56 + 8).max(0) as usize);
        for e in first..first + count {
            let se = i32_at(&surfedges, e * 4);
            let edge = se.unsigned_abs() as usize;
            for v in [u16_at(&edges, edge * 4), u16_at(&edges, edge * 4 + 2)] {
                let o = v as usize * 12;
                let p = DVec3::new(
                    f64::from(f32_at(&verts, o)),
                    f64::from(f32_at(&verts, o + 4)),
                    f64::from(f32_at(&verts, o + 8)),
                );
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
        (lo, hi)
    };
    let mut water_faces: Vec<Vec<(DVec3, DVec3)>> = vec![Vec::new(); n];
    for (i, list) in water_faces.iter_mut().enumerate() {
        if !sees_water(&leafs, i) {
            continue;
        }
        let (first, count) = (u16_at(&leafs, i * 32 + 20) as usize, u16_at(&leafs, i * 32 + 22) as usize);
        for k in first..first + count {
            let f = u16_at(&leaffaces, k * 2) as usize;
            let ti = i16_at(&faces, f * 56 + 10);
            if ti < 0 || f * 56 >= faces.len() {
                continue;
            }
            if i32_at(&texinfo, ti as usize * 72 + 64) as u32 & surf::WARP != 0 {
                list.push(face_box(f));
            }
        }
    }
    let leaf_box = |i: usize| {
        let s = |o: usize| f64::from(i16_at(&leafs, i * 32 + o));
        (DVec3::new(s(8), s(10), s(12)), DVec3::new(s(14), s(16), s(18)))
    };
    let cluster_faces: Vec<(i16, Vec<(DVec3, DVec3)>)> = clusters
        .iter()
        .map(|&c| (c, by_cluster[&c].iter().flat_map(|&k| water_faces[k].iter().copied()).collect::<Vec<_>>()))
        .filter(|(_, f)| !f.is_empty())
        .collect();
    let leafs_ref = &leafs;
    let best_of: Vec<u16> = {
        use rayon::prelude::*;
        (0..n)
            .into_par_iter()
            .map(|i| {
                let from = cluster_of(leafs_ref, i);
                let mut best = f64::from(FAR_FROM_WATER);
                if sees_water(leafs_ref, i) && from >= 0 {
                    let me = leaf_box(i);
                    for (to, list) in &cluster_faces {
                        if *to == from || !visible(from, *to) {
                            continue;
                        }
                        for b in list {
                            best = best.min(box_distance(me, *b));
                        }
                    }
                }
                best as u16
            })
            .collect()
    };
    let mut dist = Buf::default();
    for v in best_of {
        dist.u16(v);
    }
    data.insert(lump::LEAFS, (leafs, leafs_version));
    data.insert(lump::LEAFMINDISTTOWATER, (dist.0, 0));
    marked
}

pub fn ensure_lod_control(entities: &mut Vec<hp_bsp::Entity>) -> bool {
    if entities.iter().any(|e| e.get("classname").is_some_and(|c| c.eq_ignore_ascii_case("water_lod_control"))) {
        return false;
    }
    let mut e = hp_bsp::Entity::default();
    e.set("cheapwaterenddistance", "2000".to_owned());
    e.set("cheapwaterstartdistance", "1000".to_owned());
    e.set("classname", "water_lod_control".to_owned());
    entities.push(e);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials::{MaterialFacts, MaterialSource, facts_by_name, facts_from_vmt};

    struct Mats;

    fn vmt(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k: &str| pairs.iter().find(|(a, _)| *a == k).map(|(_, v)| (*v).to_owned())
    }

    impl MaterialSource for Mats {
        fn facts(&self, name: &str) -> MaterialFacts {
            let mut f = match name.to_ascii_lowercase().as_str() {
                "test/water" => facts_from_vmt(
                    "Water",
                    &vmt(&[("%compilewater", "1"), ("$bottommaterial", "test/water_beneath")]),
                    Some((256, 256)),
                    None,
                ),
                "test/water_beneath" | "test/water_nobottom" => {
                    facts_from_vmt("Water", &vmt(&[("%compilewater", "1")]), Some((256, 256)), None)
                }
                "test/slime" => facts_from_vmt("LightmappedGeneric", &vmt(&[("%compileslime", "1")]), None, None),
                _ => facts_by_name(name),
            };
            f.reflectivity = [0.5; 3];
            f
        }
    }

    fn block(id: &mut u32, lo: DVec3, hi: DVec3, mat: &str) -> String {
        let p = |x: f64, y: f64, z: f64| format!("({x} {y} {z})");
        let (a, b) = (lo, hi);
        let sides = [
            (
                format!("{} {} {}", p(a.x, b.y, b.z), p(b.x, b.y, b.z), p(b.x, a.y, b.z)),
                "[1 0 0 0] 0.25",
                "[0 -1 0 0] 0.25",
            ),
            (
                format!("{} {} {}", p(a.x, a.y, a.z), p(b.x, a.y, a.z), p(b.x, b.y, a.z)),
                "[1 0 0 0] 0.25",
                "[0 -1 0 0] 0.25",
            ),
            (
                format!("{} {} {}", p(a.x, b.y, b.z), p(a.x, a.y, b.z), p(a.x, a.y, a.z)),
                "[0 1 0 0] 0.25",
                "[0 0 -1 0] 0.25",
            ),
            (
                format!("{} {} {}", p(b.x, b.y, a.z), p(b.x, a.y, a.z), p(b.x, a.y, b.z)),
                "[0 1 0 0] 0.25",
                "[0 0 -1 0] 0.25",
            ),
            (
                format!("{} {} {}", p(b.x, b.y, b.z), p(a.x, b.y, b.z), p(a.x, b.y, a.z)),
                "[1 0 0 0] 0.25",
                "[0 0 -1 0] 0.25",
            ),
            (
                format!("{} {} {}", p(b.x, a.y, a.z), p(a.x, a.y, a.z), p(a.x, a.y, b.z)),
                "[1 0 0 0] 0.25",
                "[0 0 -1 0] 0.25",
            ),
        ];
        *id += 1;
        let mut s = format!("\tsolid\n\t{{\n\t\t\"id\" \"{id}\"\n");
        for (plane, u, v) in sides {
            *id += 1;
            s += &format!(
                "\t\tside\n\t\t{{\n\t\t\t\"id\" \"{id}\"\n\t\t\t\"plane\" \"{plane}\"\n\t\t\t\"material\" \"{mat}\"\n\t\t\t\"uaxis\" \"{u}\"\n\t\t\t\"vaxis\" \"{v}\"\n\t\t\t\"rotation\" \"0\"\n\t\t\t\"lightmapscale\" \"16\"\n\t\t\t\"smoothing_groups\" \"0\"\n\t\t}}\n"
            );
        }
        s + "\t}\n"
    }

    fn pool_map(water: &str, entities: &str) -> String {
        pool_map_long(water, entities, 512.0)
    }

    fn pool_map_long(water: &str, entities: &str, length: f64) -> String {
        let mut id = 1;
        let w = "DEV/DEV_MEASUREGENERIC01B";
        let t = 16.0;
        let (lo, hi) = (DVec3::ZERO, DVec3::new(length, 256.0, 256.0));
        let mut solids = String::new();
        solids += &block(&mut id, DVec3::new(lo.x - t, lo.y - t, lo.z - t), DVec3::new(hi.x + t, hi.y + t, lo.z), w);
        solids += &block(&mut id, DVec3::new(lo.x - t, lo.y - t, hi.z), DVec3::new(hi.x + t, hi.y + t, hi.z + t), w);
        solids += &block(&mut id, DVec3::new(lo.x - t, lo.y - t, lo.z), DVec3::new(lo.x, hi.y + t, hi.z), w);
        solids += &block(&mut id, DVec3::new(hi.x, lo.y - t, lo.z), DVec3::new(hi.x + t, hi.y + t, hi.z), w);
        solids += &block(&mut id, DVec3::new(lo.x, lo.y - t, lo.z), DVec3::new(hi.x, lo.y, hi.z), w);
        solids += &block(&mut id, DVec3::new(lo.x, hi.y, lo.z), DVec3::new(hi.x, hi.y + t, hi.z), w);
        solids += &block(&mut id, DVec3::new(240.0, 96.0, 0.0), DVec3::new(272.0, 160.0, 256.0), w);
        solids += &block(&mut id, DVec3::new(lo.x, lo.y, lo.z), DVec3::new(hi.x, hi.y, 64.0), water);
        format!(
            "versioninfo\n{{\n\t\"editorversion\" \"400\"\n\t\"mapversion\" \"1\"\n}}\nworld\n{{\n\t\"id\" \"1\"\n\t\"mapversion\" \"1\"\n\t\"classname\" \"worldspawn\"\n\t\"skyname\" \"sky_day01_01\"\n{solids}}}\n{entities}\
             entity\n{{\n\t\"id\" \"9000\"\n\t\"classname\" \"info_player_start\"\n\t\"origin\" \"32 32 128\"\n}}\n"
        )
    }

    fn compile_map(text: &str, vis: crate::VisLevel) -> (Vec<u8>, Vec<String>) {
        compile_named(text, vis, None)
    }

    fn compile_named(text: &str, vis: crate::VisLevel, name: Option<&str>) -> (Vec<u8>, Vec<String>) {
        let map = hp_vmf::parse(text).expect("vmf");
        let mut log = Vec::new();
        let opts = crate::CompileOptions { vis, map_name: name.map(str::to_owned), ..crate::CompileOptions::default() };
        let out = crate::compile(&map, &Mats, &crate::props::NoModels, &opts, &mut |l| log.push(l.to_owned()))
            .expect("compile");
        assert!(out.leak.is_none(), "the pool leaks");
        (out.bsp, log)
    }

    fn lump_of(bsp: &[u8], i: usize) -> &[u8] {
        let (o, l) = (i32_at(bsp, 8 + i * 16) as usize, i32_at(bsp, 12 + i * 16) as usize);
        &bsp[o..o + l]
    }

    fn texinfo_name(bsp: &[u8], ti: i16) -> String {
        let (texinfo, texdata) = (lump_of(bsp, lump::TEXINFO), lump_of(bsp, lump::TEXDATA));
        let (table, strings) = (lump_of(bsp, lump::TEXDATA_STRING_TABLE), lump_of(bsp, lump::TEXDATA_STRING_DATA));
        let td = i32_at(texinfo, ti as usize * 72 + 68) as usize;
        let at = i32_at(table, i32_at(texdata, td * 32 + 12) as usize * 4) as usize;
        let end = strings[at..].iter().position(|b| *b == 0).unwrap() + at;
        String::from_utf8_lossy(&strings[at..end]).to_ascii_lowercase()
    }

    fn faces_of(bsp: &[u8]) -> Vec<(String, u32, f32)> {
        let (faces, texinfo, planes) =
            (lump_of(bsp, lump::FACES), lump_of(bsp, lump::TEXINFO), lump_of(bsp, lump::PLANES));
        (0..faces.len() / 56)
            .filter_map(|f| {
                let ti = i16_at(faces, f * 56 + 10);
                if ti < 0 {
                    return None;
                }
                let flags = i32_at(texinfo, ti as usize * 72 + 64) as u32;
                Some((texinfo_name(bsp, ti), flags, f32_at(planes, u16_at(faces, f * 56) as usize * 20 + 8)))
            })
            .collect()
    }

    fn brush_entity(id: u32, class: &str, lo: DVec3, hi: DVec3, material: &str) -> String {
        let mut bid = id * 100;
        format!("entity\n{{\n\t\"id\" \"{id}\"\n\t\"classname\" \"{class}\"\n{}}}\n", block(&mut bid, lo, hi, material))
    }

    fn phys_records(bsp: &[u8]) -> Vec<(i32, i32, String, Vec<Vec<u8>>)> {
        let d = lump_of(bsp, lump::PHYSCOLLIDE);
        let (mut at, mut out) = (0, Vec::new());
        loop {
            let (model, data, keys, count) =
                (i32_at(d, at), i32_at(d, at + 4) as usize, i32_at(d, at + 8) as usize, i32_at(d, at + 12));
            at += 16;
            if model == -1 {
                break;
            }
            let mut blobs = Vec::new();
            let mut b = at;
            for _ in 0..count {
                let n = i32_at(d, b) as usize;
                blobs.push(d[b + 4..b + 4 + n].to_vec());
                b += 4 + n;
            }
            let text = String::from_utf8_lossy(&d[at + data..at + data + keys]).trim_end_matches('\0').to_owned();
            out.push((model, count, text, blobs));
            at += data + keys;
        }
        out
    }

    fn pak_file(bsp: &[u8], name: &str) -> Option<String> {
        let pak = hp_bsp::PakArchive::parse(lump_of(bsp, lump::PAKFILE).to_vec()).ok()?;
        pak.read_name(name).ok().flatten().map(|d| String::from_utf8_lossy(&d).into_owned())
    }

    fn lod_controls(bsp: &[u8]) -> Vec<hp_bsp::Entity> {
        let ents = hp_bsp::Bsp::from_vec(bsp.to_vec()).unwrap().entities().unwrap();
        ents.into_iter().filter(|e| e.get("classname") == Some("water_lod_control")).collect()
    }

    #[test]
    fn water_flags_like_vbsp() {
        let water = Mats.facts("test/water");
        assert_eq!(water.contents, contents::WATER);
        assert_eq!(water.surf, surf::WARP | surf::NOSHADOWS | surf::NODECALS | surf::NOLIGHT);
        assert_eq!(water.bottom_material.as_deref(), Some("test/water_beneath"));
        let slime = Mats.facts("test/slime");
        assert_eq!(slime.contents, contents::SLIME);
        assert_eq!(slime.surf, surf::NODECALS);
        assert_eq!(box_distance((DVec3::ZERO, DVec3::ONE), (DVec3::new(4.0, 5.0, 0.5), DVec3::splat(9.0))), 5.0);
        assert_eq!(box_distance((DVec3::ZERO, DVec3::ONE), (DVec3::ONE, DVec3::splat(2.0))), 0.0);
    }

    #[test]
    fn pool_surfaces_volumes_and_fog_leaves() {
        let (bsp, log) = compile_map(&pool_map("test/water", ""), crate::VisLevel::Full);
        let faces = faces_of(&bsp);
        let water: Vec<_> = faces.iter().filter(|f| f.0.starts_with("test/water")).collect();
        assert!(water.iter().any(|f| f.0 == "test/water" && f.2 > 0.99), "{water:?}");
        assert!(water.iter().any(|f| f.0 == "test/water_beneath" && f.2 < -0.99), "{water:?}");
        assert!(water.iter().all(|f| (f.0 == "test/water") == (f.2 > 0.0)), "{water:?}");
        assert!(
            water.iter().all(|f| f.1 == surf::WARP | surf::NOSHADOWS | surf::NODECALS | surf::NOLIGHT),
            "{water:?}"
        );
        let lw = lump_of(&bsp, lump::LEAFWATERDATA);
        assert_eq!(lw.len(), 12, "{lw:?}");
        assert_eq!((f32_at(lw, 0), f32_at(lw, 4)), (64.0, 0.0));
        assert_eq!(texinfo_name(&bsp, i16_at(lw, 8)), "test/water");
        let (leafs, dist) = (lump_of(&bsp, lump::LEAFS), lump_of(&bsp, lump::LEAFMINDISTTOWATER));
        assert_eq!(dist.len(), leafs.len() / 16);
        let (mut water_leaves, mut fog_leaves) = (0, 0);
        for i in 0..leafs.len() / 32 {
            let c = i32_at(leafs, i * 32) as u32;
            let id = i16_at(leafs, i * 32 + 28);
            let d = u16_at(dist, i * 2);
            if c & contents::WATER != 0 {
                assert_eq!(id, 0);
                assert_eq!(c & contents::TESTFOGVOLUME, 0);
                assert_eq!(d, 0, "leaf {i}");
                water_leaves += 1;
            } else if c & contents::SOLID != 0 || i16_at(leafs, i * 32 + 4) < 0 {
                assert_eq!(id, -1);
                assert_eq!(c & contents::TESTFOGVOLUME, 0);
            } else {
                assert_eq!(id, -1);
                assert_ne!(c & contents::TESTFOGVOLUME, 0, "air leaf {i} does not test for water");
                assert!(d < 256, "leaf {i}: {d}");
                fog_leaves += 1;
            }
        }
        assert!(water_leaves > 0 && fog_leaves > 0, "{water_leaves} {fog_leaves}");
        let corners: usize = (0..lump_of(&bsp, lump::FACES).len() / 56)
            .map(|f| i16_at(lump_of(&bsp, lump::FACES), f * 56 + 8) as usize)
            .sum();
        assert_eq!(lump_of(&bsp, lump::VERTNORMALINDICES).len(), corners * 2);
        assert!(!lump_of(&bsp, lump::VERTNORMALS).is_empty());
        let lod = lod_controls(&bsp);
        assert_eq!(lod.len(), 1);
        assert_eq!(lod[0].get("cheapwaterenddistance"), Some("2000"));
        assert!(log.iter().any(|l| l.contains("creating a default one")));
    }

    #[test]
    fn world_water_is_a_fluid_with_its_depth_patch() {
        let (bsp, _) = compile_named(&pool_map("test/water", ""), crate::VisLevel::Off, Some("pooltest"));
        let recs = phys_records(&bsp);
        let (model, count, text, blobs) = &recs[0];
        assert_eq!((*model, *count), (0, 2));
        assert!(
            text.contains(
                "fluid {\n\"index\" \"1\"\n\"surfaceprop\" \"water\"\n\"damping\" \"0.010000\"\n\"contents\" \"268435488\"\n\"surfaceplane\" \"0.000000 0.000000 1.000000 64.000000 \"\n\"currentvelocity\" \"0.000000 0.000000 0.000000 \"\n}\nvirtualterrain {}\nmaterialtable {"
            ),
            "{text}"
        );
        let f = |o: usize| f64::from(f32_at(&blobs[1], o));
        let (cx, cy, cz) = (f(28), f(32), f(36));
        assert!(
            (cx - 256.0 * 0.0254).abs() < 1e-4
                && (cy + 32.0 * 0.0254).abs() < 1e-4
                && (cz - 128.0 * 0.0254).abs() < 1e-4
        );
        let m = [512.0f64, 64.0, 256.0].map(|v| (v * 0.0254f64).powi(2) / 12.0);
        assert!(
            (f(40) - m[1].hypot(m[2])).abs() < 1e-3 && (f(44) - m[0].hypot(m[2])).abs() < 1e-3,
            "{} {}",
            f(40),
            f(44)
        );
        let patch = pak_file(&bsp, "materials/maps/pooltest/test/water_depth_64.vmt").expect("depth patch");
        assert_eq!(
            patch,
            "\"patch\"\r\n{\r\n\t\"include\"\t\t\"materials/test/water.vmt\"\r\n\t\"insert\"\r\n\t{\r\n\t\t\"$waterdepth\"\t\t\"64\"\r\n\t}\r\n}\r\n"
        );
        let (bsp, _) = compile_map(&pool_map("test/water", ""), crate::VisLevel::Off);
        assert!(pak_file(&bsp, "materials/maps/pooltest/test/water_depth_64.vmt").is_none());
        assert!(phys_records(&bsp)[0].2.contains("fluid {"));
    }

    #[test]
    fn water_brush_entities_are_solids_with_their_own_volume() {
        let ent = brush_entity(
            7000,
            "func_water_analog",
            DVec3::new(300.0, 100.0, 150.0),
            DVec3::new(400.0, 200.0, 182.0),
            "test/water",
        );
        let (bsp, _) = compile_named(&pool_map("test/water", &ent), crate::VisLevel::Off, Some("pooltest"));
        let recs = phys_records(&bsp);
        assert!(recs.iter().any(|r| r.0 == 1 && r.2.starts_with("solid {")), "{recs:?}");
        assert!(!recs.iter().any(|r| r.0 == 1 && r.2.contains("fluid")));
        let lw = lump_of(&bsp, lump::LEAFWATERDATA);
        assert_eq!(lw.len(), 24, "two volumes");
        assert_eq!((f32_at(lw, 12), f32_at(lw, 16)), (182.0, 150.0));
        assert!(pak_file(&bsp, "materials/maps/pooltest/test/water_depth_32.vmt").is_some());
    }

    #[test]
    fn vvis_skips_the_cluster_of_the_leaf() {
        let vc = brush_entity(
            7000,
            "func_viscluster",
            DVec3::new(1.0, 1.0, 1.0),
            DVec3::new(511.0, 255.0, 255.0),
            "TOOLS/TOOLSTRIGGER",
        );
        let (bsp, _) = compile_map(&pool_map("test/water", &vc), crate::VisLevel::Full);
        let (leafs, dist) = (lump_of(&bsp, lump::LEAFS), lump_of(&bsp, lump::LEAFMINDISTTOWATER));
        let open: Vec<usize> = (0..leafs.len() / 32).filter(|i| i16_at(leafs, i * 32 + 4) >= 0).collect();
        let clusters: std::collections::HashSet<i16> = open.iter().map(|i| i16_at(leafs, i * 32 + 4)).collect();
        assert_eq!(clusters.len(), 1, "one cluster");
        for i in open {
            assert_eq!(i32_at(leafs, i * 32) as u32 & contents::TESTFOGVOLUME, 0, "leaf {i}");
            assert_eq!(u16_at(dist, i * 2), FAR_FROM_WATER, "leaf {i}");
        }
    }

    #[test]
    fn leaf_flags_like_vbsp_and_vvis() {
        let flags = |bsp: &[u8]| {
            let leafs = lump_of(bsp, lump::LEAFS).to_vec();
            (0..leafs.len() / 32).map(|i| u16_at(&leafs, i * 32 + 6) >> 9).collect::<Vec<u16>>()
        };
        let (bsp, _) = compile_map(&pool_map("test/water", ""), crate::VisLevel::Full);
        let f = flags(&bsp);
        assert_eq!(f[0], 0);
        assert!(f[1..].iter().all(|v| *v == 1), "{f:?}");
        let fog = "entity
{
	\"id\" \"7000\"
	\"classname\" \"env_fog_controller\"
	\"origin\" \"64 64 128\"
	\"farz\" \"4000\"
}
";
        let (bsp, _) = compile_map(&pool_map("test/water", fog), crate::VisLevel::Full);
        let f = flags(&bsp);
        assert_eq!(f[0], 2);
        assert!(f[1..].iter().all(|v| *v == 3), "{f:?}");
    }

    #[test]
    fn existing_lod_control_and_no_bottom_material() {
        let lod = "entity\n{\n\t\"id\" \"8000\"\n\t\"classname\" \"water_lod_control\"\n\t\"origin\" \"8 8 128\"\n\t\"cheapwaterstartdistance\" \"500\"\n\t\"cheapwaterenddistance\" \"900\"\n}\n";
        let (bsp, _) = compile_map(&pool_map("test/water_nobottom", lod), crate::VisLevel::Off);
        let faces = faces_of(&bsp);
        assert!(faces.iter().any(|f| f.0 == "test/water_nobottom" && f.2 > 0.99));
        assert!(!faces.iter().any(|f| f.0 == "test/water_nobottom" && f.2 < 0.0), "{faces:?}");
        let lods = lod_controls(&bsp);
        assert_eq!(lods.len(), 1);
        assert_eq!(lods[0].get("cheapwaterenddistance"), Some("900"));
        let leafs = lump_of(&bsp, lump::LEAFS);
        let air: Vec<u32> = (0..leafs.len() / 32)
            .filter(|i| i16_at(leafs, i * 32 + 4) >= 0)
            .map(|i| i32_at(leafs, i * 32) as u32)
            .filter(|c| c & (contents::SOLID | contents::WATER) == 0)
            .collect();
        assert!(!air.is_empty() && air.iter().all(|c| c & contents::TESTFOGVOLUME == 0), "{air:x?}");
        let dist = lump_of(&bsp, lump::LEAFMINDISTTOWATER);
        assert_eq!(dist.len(), leafs.len() / 32 * 2);
        assert!(dist.iter().all(|b| *b == 0));
    }

    #[test]
    fn long_pools_are_cut_on_the_block_lines() {
        let (bsp, _) = compile_map(&pool_map_long("test/water", "", 1536.0), crate::VisLevel::Off);
        let (faces, verts, edges, surfedges) = (
            lump_of(&bsp, lump::FACES),
            lump_of(&bsp, lump::VERTEXES),
            lump_of(&bsp, lump::EDGES),
            lump_of(&bsp, lump::SURFEDGES),
        );
        let tops: Vec<(f32, f32)> = faces_of(&bsp)
            .iter()
            .enumerate()
            .filter(|(_, f)| f.0 == "test/water")
            .map(|(i, _)| {
                let (first, count) = (i32_at(faces, i * 56 + 4) as usize, i16_at(faces, i * 56 + 8) as usize);
                let xs: Vec<f32> = (first..first + count)
                    .map(|e| {
                        let se = i32_at(surfedges, e * 4);
                        let v = u16_at(edges, se.unsigned_abs() as usize * 4 + if se < 0 { 2 } else { 0 }) as usize;
                        f32_at(verts, v * 12)
                    })
                    .collect();
                (xs.iter().copied().fold(f32::MAX, f32::min), xs.iter().copied().fold(f32::MIN, f32::max))
            })
            .collect();
        assert!(tops.len() >= 2, "{tops:?}");
        assert!(tops.iter().all(|(lo, hi)| *hi <= 1024.0 || *lo >= 1024.0), "{tops:?}");
    }

    #[test]
    fn dry_maps_are_far_from_water() {
        let (bsp, log) = compile_map(&pool_map("DEV/DEV_MEASUREGENERIC01B", ""), crate::VisLevel::Full);
        let dist = lump_of(&bsp, lump::LEAFMINDISTTOWATER);
        assert!(dist.chunks_exact(2).all(|d| u16::from_le_bytes([d[0], d[1]]) == FAR_FROM_WATER));
        assert!(lump_of(&bsp, lump::LEAFWATERDATA).is_empty());
        assert!(lod_controls(&bsp).is_empty());
        let messages = log.iter().take_while(|l| l.as_str() != crate::stats::GEOMETRY_HEADER);
        assert!(!messages.into_iter().any(|l| l.contains("water")), "{log:?}");
    }
}
