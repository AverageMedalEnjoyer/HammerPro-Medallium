use glam::DVec3;

use crate::brush::MapBrush;
use crate::emit::{Buf, Lumps};
use crate::faces::Faces;
use crate::materials::{contents, surf};
use crate::planes::PlaneSet;
use crate::portals::Portals;
use crate::texinfo::TexInfos;
use crate::tree::Tree;

pub fn is_occluder_class(class: &str) -> bool {
    class.eq_ignore_ascii_case("func_occluder")
}

#[derive(Clone, Debug, Default)]
pub struct Occluder {
    pub entity: usize,
    pub polys: Vec<(usize, Vec<u32>)>,
    pub area: i32,
}

#[derive(Clone, Debug, Default)]
pub struct Occluders {
    pub list: Vec<Occluder>,
    pub vertices: Vec<DVec3>,
    global: Vec<u32>,
}

pub fn occluder_numbers(entities: &[&hp_vmf::Entity]) -> Vec<(usize, usize)> {
    let any = entities.iter().skip(1).any(|e| is_occluder_class(&e.classname) && !e.solids.is_empty());
    if !any {
        return Vec::new();
    }
    entities
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, e)| is_occluder_class(&e.classname))
        .enumerate()
        .map(|(k, (ei, _))| (ei, k))
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn build(
    sources: &[(usize, Vec<usize>)],
    entities: &[&hp_vmf::Entity],
    planes: &mut PlaneSet,
    brush_contents: &[u32],
    brushes: &[MapBrush],
    tex: &TexInfos,
    world: &Tree,
    world_portals: &Portals,
    log: &mut dyn FnMut(&str),
) -> Option<Occluders> {
    let all: Vec<usize> = sources.iter().flat_map(|(_, ids)| ids.iter().copied()).collect();
    if all.is_empty() {
        return None;
    }
    let _ = brush_contents;
    let mut tree =
        crate::brushbsp::model_tree(planes, brushes, &brushes.iter().map(|b| b.contents).collect::<Vec<_>>(), &all);
    let mut portals = Portals::make(&mut tree, planes);
    let mut faces = Faces::make(&mut tree, &mut portals, planes, brushes, tex, false, true);
    faces.merge(&mut tree, planes, brushes);
    faces.subdivide(&mut tree, planes, tex);
    faces.fix_tjuncs(&mut tree, false);
    let mut order: Vec<usize> = Vec::new();
    for n in &tree.nodes {
        if !n.is_leaf() {
            order.extend(n.faces.iter().copied());
        }
    }
    let mut out = Occluders { vertices: faces.vertices.clone(), ..Occluders::default() };
    for (ei, _) in occluder_numbers(entities) {
        let mut occ = Occluder { entity: ei, polys: Vec::new(), area: -1 };
        for &f in order.iter().rev() {
            let face = &faces.list[f];
            if face.verts.len() < 3 || brushes.get(face.source.0).is_none_or(|b| b.entity != ei) {
                continue;
            }
            let flags = tex.infos.get(face.texinfo as usize).map_or(0, |t| t.flags);
            if flags & surf::NODRAW != 0 && flags & surf::TRIGGER == 0 {
                continue;
            }
            occ.polys.push((face.plane, face.verts.clone()));
        }
        out.list.push(occ);
    }
    let out_vertices = out.vertices.clone();
    for occ in &mut out.list {
        let name = entities.get(occ.entity).and_then(|e| e.get("targetname")).unwrap_or("").to_owned();
        let mut warned = false;
        let mut found = -1i32;
        for cross in [false, true] {
            for (_, points) in &occ.polys {
                for &k in points {
                    let leaf = world.leaf_at(planes, out_vertices[k as usize]);
                    let mut set = |area: i32| {
                        if found <= 0 {
                            found = area;
                        } else if area != 0 && found != area && !warned {
                            warned = true;
                            log(&format!("Occluder \"{name}\" straddles multiple areas. This is invalid!"));
                        }
                    };
                    set(world.nodes[leaf].area);
                    for &pid in &world.nodes[leaf].portals {
                        let q = &world_portals.list[pid];
                        if q.on_node.is_none() {
                            continue;
                        }
                        let other = if q.nodes[0] == leaf { q.nodes[1] } else { q.nodes[0] };
                        if !cross && world.nodes[other].contents & contents::AREAPORTAL != 0 {
                            continue;
                        }
                        set(world.nodes[other].area);
                    }
                }
            }
            if found > 0 {
                break;
            }
        }
        occ.area = found;
    }
    let polys: usize = out.list.iter().map(|o| o.polys.len()).sum();
    log(&format!("{} occluders, {polys} occluder polygons", out.list.len()));
    Some(out)
}

impl Occluders {
    pub fn register(&mut self, lumps: &mut Lumps) {
        lumps.begin_vertices();
        self.global = self.vertices.iter().map(|p| lumps.vertex(*p)).collect();
    }

    pub fn lump(&self, lumps: &mut Lumps) -> Vec<u8> {
        let mut data = Buf::default();
        let mut polys = Buf::default();
        let mut indices: Vec<u32> = Vec::new();
        let mut poly_count = 0i32;
        for occ in &self.list {
            let first = poly_count;
            let (mut lo, mut hi) = (DVec3::splat(f64::from(f32::MAX)), DVec3::splat(-f64::from(f32::MAX)));
            for (plane, points) in &occ.polys {
                polys.i32(indices.len() as i32);
                polys.i32(points.len() as i32);
                polys.i32(*plane as i32);
                for &k in points {
                    let v = self.global.get(k as usize).copied().unwrap_or(0);
                    let at = lumps.vertex_point(v);
                    lo = lo.min(at);
                    hi = hi.max(at);
                    indices.push(v);
                }
                poly_count += 1;
            }
            data.i32(0);
            data.i32(first);
            data.i32(poly_count - first);
            data.vec3(lo);
            data.vec3(hi);
            data.i32(occ.area);
        }
        let mut out = Buf::default();
        out.i32(self.list.len() as i32);
        out.0.extend_from_slice(&data.0);
        out.i32(poly_count);
        out.0.extend_from_slice(&polys.0);
        out.i32(indices.len() as i32);
        for i in indices {
            out.i32(i as i32);
        }
        out.0
    }
}
