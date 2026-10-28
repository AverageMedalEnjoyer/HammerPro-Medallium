use glam::Vec3;

use crate::brush::MapBrush;
use crate::brushbsp::{self, BBrush, DetailScreen, FWinding, Visibility};
use crate::faces::{Face, Faces};
use crate::materials::contents;
use crate::planes::PlaneSet;
use crate::texinfo::TexInfos;
use crate::tree::Tree;
use crate::winding::Winding;

const ALL_VISIBLE: u32 = 0xff;
const TRANSPARENT: u32 = contents::GRATE | contents::WINDOW;

#[derive(Clone)]
struct DFace {
    w: FWinding,
    plane: usize,
    texinfo: i32,
    contents: u32,
    source: (usize, usize),
    gone: bool,
}

fn original_side(planes: &PlaneSet, mb: &MapBrush, plane: usize) -> Option<usize> {
    let (p1, _) = planes.getf(plane);
    let mut best = None;
    let mut bestdot = 0.0f32;
    for (i, s) in mb.sides.iter().enumerate() {
        if s.bevel || s.no_split {
            continue;
        }
        if s.plane & !1 == plane & !1 {
            return Some(i);
        }
        let (p2, _) = planes.getf(s.plane & !1);
        let dot = p1.dot(p2);
        if dot > bestdot {
            bestdot = dot;
            best = Some(i);
        }
    }
    best
}

fn box_overlap(a: &BBrush, b: &BBrush) -> bool {
    (0..3).all(|i| !(a.mins[i] > b.maxs[i] || a.maxs[i] < b.mins[i]))
}

fn clip_face_to_brush(planes: &PlaneSet, f: &DFace, b: &BBrush, out: &mut Vec<DFace>) -> bool {
    let planenum = f.plane & !1;
    let Some(found) = b.sides.iter().position(|s| s.plane & !1 == planenum) else { return false };
    let offset = (b.maxs + b.mins) * -0.5;
    let mut sorted: Vec<usize> = Vec::new();
    for (i, s) in b.sides.iter().enumerate() {
        if s.bevel {
            continue;
        }
        if planes.get(s.plane).kind <= 2 {
            sorted.insert(0, i);
        } else {
            sorted.push(i);
        }
    }
    let mut current = f.w.clone();
    let mut i = 0;
    while i < sorted.len() {
        let index = sorted[i];
        if index == found {
            i += 1;
            continue;
        }
        let (n, d) = planes.getf(b.sides[index].plane);
        let (front, back) = brushbsp::clip_epsilon_offset(&mut current, n, d, 0.001, offset);
        let Some(back) = back.filter(|w| !brushbsp::is_tiny(w)) else {
            out.clear();
            break;
        };
        if let Some(front) = front.filter(|w| !brushbsp::is_tiny(w)) {
            out.insert(0, DFace { w: front, gone: false, ..f.clone() });
        }
        current = back;
        i += 1;
    }
    out.is_empty() && i == sorted.len()
}

fn clip_face_to_brush_list(planes: &PlaneSet, f: &DFace, cut: &[&BBrush]) -> Option<Vec<DFace>> {
    if f.gone {
        return None;
    }
    let mut list = vec![DFace { gone: false, ..f.clone() }];
    let mut clipped = false;
    for b in cut {
        let mut k = 0;
        while k < list.len() {
            if list[k].gone {
                k += 1;
                continue;
            }
            let mut pieces = Vec::new();
            if clip_face_to_brush(planes, &list[k], b, &mut pieces) {
                clipped = true;
                list[k].gone = true;
            } else if !pieces.is_empty() {
                clipped = true;
                list[k].gone = true;
                let count = pieces.len();
                for p in pieces {
                    list.insert(0, p);
                }
                k += count;
            }
            k += 1;
        }
    }
    clipped.then_some(list)
}

fn visible_brush_sides(planes: &PlaneSet, brushes: &[MapBrush], contents_of: &[u32], list: &[BBrush]) -> Vec<DFace> {
    use rayon::prelude::*;
    let per_brush: Vec<Vec<DFace>> =
        (0..list.len()).into_par_iter().map(|bi| brush_visible_sides(planes, brushes, contents_of, list, bi)).collect();
    let mut total: Vec<DFace> = per_brush.into_iter().flatten().collect();
    total.reverse();
    total
}

fn brush_visible_sides(
    planes: &PlaneSet,
    brushes: &[MapBrush],
    contents_of: &[u32],
    list: &[BBrush],
    bi: usize,
) -> Vec<DFace> {
    let b = &list[bi];
    let mb = &brushes[b.original];
    let mc = contents_of[b.original];
    if mc & ALL_VISIBLE == 0 {
        return Vec::new();
    }
    let mut faces: Vec<DFace> = Vec::new();
    for s in &b.sides {
        let Some(w) = &s.winding else { continue };
        if s.contents & ALL_VISIBLE == 0 {
            continue;
        }
        let Some(si) = original_side(planes, mb, s.plane) else { continue };
        let os = &mb.sides[si];
        faces.insert(
            0,
            DFace {
                w: w.clone(),
                plane: os.plane,
                texinfo: os.texinfo,
                contents: os.contents,
                source: (b.original, si),
                gone: false,
            },
        );
    }
    let cut: Vec<&BBrush> = list
        .iter()
        .enumerate()
        .filter(|&(wi, w)| {
            let wc = contents_of[w.original];
            wi != bi && !(wc & TRANSPARENT != 0 && mc & TRANSPARENT == 0) && wc & ALL_VISIBLE != 0 && box_overlap(b, w)
        })
        .map(|(_, w)| w)
        .collect();
    if !cut.is_empty() {
        let mut k = 0;
        while k < faces.len() {
            if let Some(pieces) = clip_face_to_brush_list(planes, &faces[k], &cut) {
                let live = pieces.iter().filter(|p| !p.gone).count();
                if live <= 1 {
                    faces[k].gone = true;
                    let count = pieces.len();
                    for p in pieces {
                        faces.insert(0, p);
                    }
                    k += count;
                }
            }
            k += 1;
        }
    }
    faces.retain(|f| !f.gone);
    faces
}

fn filter_face(
    tree: &Tree,
    planes: &PlaneSet,
    node: usize,
    w: FWinding,
    face_plane: usize,
    out: &mut Vec<usize>,
) -> bool {
    let n = &tree.nodes[node];
    let Some(pl) = n.plane else {
        if n.contents & contents::SOLID != 0 {
            return false;
        }
        out.push(node);
        return true;
    };
    let (pn, pd) = planes.getf(pl);
    let offset = brushbsp::center(&w);
    let mut w = w;
    let (mut front, mut back, on) = brushbsp::classify_epsilon_offset(&mut w, pn, pd, 0.001, -offset);
    if let Some(on) = on {
        if planes.getf(face_plane).0.dot(pn) > 0.0 {
            front = Some(on);
        } else {
            back = Some(on);
        }
    }
    let mut referenced = false;
    let [c0, c1] = n.children;
    if let Some(f) = front {
        referenced = filter_face(tree, planes, c0, f, face_plane, out);
    }
    if let Some(b) = back {
        let test = filter_face(tree, planes, c1, b, face_plane, out);
        referenced = referenced || test;
    }
    referenced
}

fn filter_brush(tree: &Tree, planes: &PlaneSet, node: usize, mut b: BBrush, out: &mut Vec<(usize, crate::tree::Frag)>) {
    let Some(pl) = tree.nodes[node].plane else {
        if tree.nodes[node].contents & contents::SOLID == 0 {
            out.push((node, b.to_frag(planes)));
        }
        return;
    };
    let [c0, c1] = tree.nodes[node].children;
    let (front, back) = brushbsp::split_brush(planes, &mut b, pl);
    if let Some(f) = front {
        filter_brush(tree, planes, c0, f, out);
    }
    if let Some(k) = back {
        filter_brush(tree, planes, c1, k, out);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn merge_detail_tree(
    tree: &mut Tree,
    holder: usize,
    planes: &mut PlaneSet,
    brushes: &[MapBrush],
    ids: &[usize],
    map_bounds: (Vec3, Vec3),
    tex: &TexInfos,
    faces: &mut Faces,
) {
    let contents_of: Vec<u32> = brushes.iter().map(|b| b.contents).collect();
    let list = brushbsp::make_brush_list(
        planes,
        brushes,
        ids,
        map_bounds.0,
        map_bounds.1,
        DetailScreen::OnlyDetail,
        Visibility::Windings,
    );
    if list.is_empty() {
        return;
    }
    let list = brushbsp::chop_brushes(planes, &contents_of, list);
    let planes = &*planes;
    let dfaces = visible_brush_sides(planes, brushes, &contents_of, &list);
    let mut by_plane: Vec<Vec<usize>> = vec![Vec::new(); planes.len()];
    for f in dfaces {
        let id = faces.list.len();
        faces.list.push(Face {
            plane: f.plane,
            texinfo: f.texinfo,
            winding: Winding::new(f.w.iter().map(|p| p.as_dvec3()).collect()),
            contents: f.contents,
            on_node: holder,
            merged: None,
            split: None,
            output: None,
            source: f.source,
            also: Vec::new(),
            verts: Vec::new(),
            piece: false,
            prim: None,
            smoothing: 0,
        });
        by_plane[f.plane].push(id);
    }
    let mut output: Vec<usize> = Vec::new();
    for list in by_plane.iter_mut() {
        if list.is_empty() {
            continue;
        }
        list.reverse();
        faces.merge_list(list, planes, brushes);
        output.extend_from_slice(list);
    }
    output.reverse();
    tree.nodes[holder].faces = output;
    faces.subdivide_node(tree, holder, tex);
    let all = std::mem::take(&mut tree.nodes[holder].faces);
    let reached: Vec<Option<Vec<usize>>> = {
        use rayon::prelude::*;
        let tree = &*tree;
        let faces = &*faces;
        all.par_iter()
            .map(|&f| {
                if !faces.list[f].live() {
                    return None;
                }
                let w: FWinding = faces.list[f].winding.points.iter().map(|p| p.as_vec3()).collect();
                let mut leaves = Vec::new();
                filter_face(tree, planes, tree.head, w, faces.list[f].plane, &mut leaves).then_some(leaves)
            })
            .collect()
    };
    let mut leaf_list: Vec<usize> = Vec::new();
    let mut added: ahash::AHashMap<usize, Vec<usize>> = ahash::AHashMap::new();
    let mut touched: Vec<usize> = Vec::new();
    for (&f, leaves) in all.iter().zip(reached) {
        let Some(leaves) = leaves else { continue };
        for l in leaves {
            added
                .entry(l)
                .or_insert_with(|| {
                    touched.push(l);
                    Vec::new()
                })
                .push(f);
        }
        leaf_list.push(f);
    }
    for l in touched {
        let mut list = added.remove(&l).unwrap_or_default();
        list.reverse();
        list.append(&mut tree.nodes[l].detail_faces);
        tree.nodes[l].detail_faces = list;
    }
    leaf_list.reverse();
    tree.nodes[holder].faces = leaf_list;
    let pieces: Vec<Vec<(usize, crate::tree::Frag)>> = {
        use rayon::prelude::*;
        let tree = &*tree;
        list.into_par_iter()
            .map(|b| {
                let mut out = Vec::new();
                filter_brush(tree, planes, tree.head, b, &mut out);
                out
            })
            .collect()
    };
    for (leaf, frag) in pieces.into_iter().flatten() {
        tree.nodes[leaf].brushes.insert(0, frag);
    }
}
