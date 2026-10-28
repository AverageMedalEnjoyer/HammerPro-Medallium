use ahash::AHashMap;
use glam::DVec3;

use crate::brush::MapBrush;
use crate::emit::Buf;
use crate::materials::contents;
use crate::planes::PlaneSet;
use crate::portals::Portals;
use crate::tree::{Frag, Tree, split_frag};

pub const MAX_MAP_AREAS: i32 = hp_bsp::limits::AREAS.max() as i32;
pub const MAX_MAP_AREAPORTALS: usize = hp_bsp::limits::AREAPORTALS.max_usize();
pub const MAX_MAP_PORTALVERTS: usize = hp_bsp::limits::PORTALVERTS.max_usize();
pub const SKY_CLUSTER_KEY: u32 = u32::MAX;

pub fn is_areaportal_class(class: &str) -> bool {
    class.len() >= 15 && class.as_bytes()[..15].eq_ignore_ascii_case(b"func_areaportal")
}

#[derive(Clone, Debug)]
pub struct AreaPortal {
    pub entity: usize,
    pub number: u16,
    pub areas: [i32; 2],
    pub leading: [Option<usize>; 2],
}

pub fn areaportal_entities(entities: &[&hp_vmf::Entity]) -> Vec<AreaPortal> {
    let mut out = Vec::new();
    for (ei, e) in entities.iter().enumerate().skip(1) {
        if is_areaportal_class(&e.classname) {
            let number = out.len() as u16 + 1;
            out.push(AreaPortal { entity: ei, number, areas: [0; 2], leading: [None; 2] });
        }
    }
    out
}

#[derive(Clone, Debug, Default)]
pub struct Areas {
    pub count: i32,
    pub portals: Vec<AreaPortal>,
    pub leak: Option<Vec<DVec3>>,
}

fn leaves_in_order(tree: &Tree) -> Vec<usize> {
    let mut out = Vec::new();
    let mut stack = vec![tree.head];
    while let Some(n) = stack.pop() {
        if tree.nodes[n].is_leaf() {
            out.push(n);
        } else {
            let [f, b] = tree.nodes[n].children;
            stack.push(b);
            stack.push(f);
        }
    }
    out
}

fn areaportal_of(tree: &Tree, brushes: &[MapBrush], index: &AHashMap<usize, usize>, n: usize) -> Option<(usize, u32)> {
    let node = &tree.nodes[n];
    if node.contents & contents::AREAPORTAL == 0 {
        return None;
    }
    node.brushes.iter().filter_map(|f| brushes.get(f.original)).find_map(|b| {
        if b.contents & contents::AREAPORTAL == 0 {
            return None;
        }
        index.get(&b.entity).map(|k| (*k, b.id))
    })
}

fn other_side(portals: &Portals, pid: usize, n: usize) -> usize {
    let p = &portals.list[pid];
    if p.nodes[0] == n { p.nodes[1] } else { p.nodes[0] }
}

pub fn flood(
    tree: &mut Tree,
    portals: &Portals,
    brushes: &[MapBrush],
    aps: Vec<AreaPortal>,
    log: &mut dyn FnMut(&str),
) -> Areas {
    let mut areas = Areas { count: 0, portals: aps, leak: None };
    let index: AHashMap<usize, usize> = areas.portals.iter().enumerate().map(|(i, a)| (a.entity, i)).collect();
    for n in &mut tree.nodes {
        n.area = 0;
    }
    let leaves = leaves_in_order(tree);
    for &leaf in &leaves {
        let node = &tree.nodes[leaf];
        if node.area != 0 || node.contents & (contents::SOLID | contents::AREAPORTAL) != 0 || node.occupied == 0 {
            continue;
        }
        areas.count += 1;
        let area = areas.count;
        tree.nodes[leaf].area = area;
        let mut stack: Vec<(usize, usize)> = vec![(leaf, 0)];
        while let Some(&(n, i)) = stack.last() {
            let Some(&pid) = tree.nodes[n].portals.get(i) else {
                stack.pop();
                continue;
            };
            if let Some(top) = stack.last_mut() {
                top.1 += 1;
            }
            let other = other_side(portals, pid, n);
            if (tree.nodes[n].contents | tree.nodes[other].contents) & contents::SOLID != 0 {
                continue;
            }
            if tree.nodes[other].contents & contents::AREAPORTAL != 0 {
                let Some((k, id)) = areaportal_of(tree, brushes, &index, other) else { continue };
                let ap = &mut areas.portals[k];
                if ap.areas[0] == area || ap.areas[1] == area {
                    continue;
                }
                if ap.areas[1] != 0 {
                    log(&format!("WARNING: areaportal entity {} (brush {id}) touches > 2 areas", ap.entity));
                    continue;
                }
                let slot = usize::from(ap.areas[0] != 0);
                ap.areas[slot] = area;
                ap.leading[slot] = Some(pid);
                continue;
            }
            if tree.nodes[other].area != 0 {
                continue;
            }
            tree.nodes[other].area = area;
            stack.push((other, 0));
        }
    }
    let mut reported = vec![false; areas.portals.len()];
    for &leaf in &leaves {
        if tree.nodes[leaf].area != 0 {
            continue;
        }
        let Some((k, id)) = areaportal_of(tree, brushes, &index, leaf) else { continue };
        tree.nodes[leaf].area = areas.portals[k].areas[0];
        if areas.portals[k].areas[1] == 0 && !reported[k] {
            reported[k] = true;
            if areas.leak.is_none() {
                areas.leak = leak_around(tree, portals, leaf);
                if areas.leak.is_some() {
                    log("Areaportal leak ! (the .lin file shows the way around it)");
                }
            }
            log(&format!("Brush {id}: areaportal brush doesn't touch two areas"));
        }
    }
    areas
}

fn leak_around(tree: &Tree, portals: &Portals, leaf: usize) -> Option<Vec<DVec3>> {
    let solid = |n: usize| tree.nodes[n].contents & contents::SOLID != 0;
    let ap = |n: usize| tree.nodes[n].contents & contents::AREAPORTAL != 0;
    if solid(leaf) {
        return None;
    }
    let start = tree.nodes[leaf].portals.iter().copied().find(|&pid| {
        let o = other_side(portals, pid, leaf);
        !solid(o) && !ap(o)
    })?;
    let first = other_side(portals, start, leaf);
    let mut dist = vec![0u32; tree.nodes.len()];
    dist[first] = 1;
    let mut queue = std::collections::VecDeque::from([first]);
    while let Some(n) = queue.pop_front() {
        for &pid in &tree.nodes[n].portals {
            let o = other_side(portals, pid, n);
            if dist[o] != 0 || solid(n) || solid(o) || ap(n) || ap(o) {
                continue;
            }
            dist[o] = dist[n] + 1;
            queue.push_back(o);
        }
    }
    let best = tree.nodes[leaf]
        .portals
        .iter()
        .copied()
        .filter(|&pid| pid != start)
        .max_by_key(|&pid| dist[other_side(portals, pid, leaf)])
        .filter(|&pid| dist[other_side(portals, pid, leaf)] > 0)?;
    let mut path = vec![portals.list[best].winding.center()];
    let mut n = other_side(portals, best, leaf);
    path.push((tree.nodes[n].mins + tree.nodes[n].maxs) * 0.5);
    let mut guard = 0;
    while dist[n] > 1 && guard < 1_000_000 {
        guard += 1;
        let next = tree.nodes[n]
            .portals
            .iter()
            .copied()
            .map(|pid| (pid, other_side(portals, pid, n)))
            .filter(|&(_, o)| dist[o] != 0 && dist[o] < dist[n])
            .min_by_key(|&(_, o)| dist[o]);
        let Some((pid, o)) = next else { break };
        path.push(portals.list[pid].winding.center());
        n = o;
    }
    path.push(portals.list[start].winding.center());
    Some(path)
}

pub fn remove_areaportal_brushes(tree: &mut Tree, brushes: &[MapBrush]) {
    for n in &mut tree.nodes {
        n.brushes.retain(|f| brushes.get(f.original).is_none_or(|b| b.contents != contents::AREAPORTAL));
    }
}

pub type AreaLumps = (Vec<u8>, Vec<u8>, Vec<u8>);

pub fn lumps(
    tree: &Tree,
    portals: &Portals,
    planes: &PlaneSet,
    areas: &Areas,
    log: &mut dyn FnMut(&str),
) -> Result<AreaLumps, String> {
    if areas.count > MAX_MAP_AREAS {
        return Err(format!(
            "Map is split into too many unique areas (max = {MAX_MAP_AREAS})\nProbably too many areaportals"
        ));
    }
    let area_of = |n: usize| tree.nodes[n].area;
    let mut area_lump = Buf::default();
    area_lump.i32(0);
    area_lump.i32(0);
    let mut ap_lump = Buf::default();
    for _ in 0..3 {
        ap_lump.i32(0);
    }
    let mut ap_count = 1usize;
    let mut verts: Vec<DVec3> = Vec::new();
    let between: Vec<usize> = (0..portals.list.len())
        .filter(|&i| {
            let p = &portals.list[i];
            let [a, b] = p.nodes;
            p.on_node.is_some()
                && tree.nodes[a].occupied != 0
                && tree.nodes[b].occupied != 0
                && tree.nodes[a].area != tree.nodes[b].area
        })
        .collect();
    for src in 1..=areas.count {
        let first = ap_count;
        for ap in &areas.portals {
            if ap.areas[0] != src && ap.areas[1] != src {
                continue;
            }
            let side = usize::from(ap.areas[0] == src);
            let mut lead = ap.leading[0];
            if let Some(pid) = lead {
                let p = &portals.list[pid];
                if area_of(p.nodes[0]) == area_of(p.nodes[1]) {
                    lead = ap.leading[1];
                }
            }
            let Some(pid) = lead else { continue };
            let p = &portals.list[pid];
            let Some(on) = p.on_node else { continue };
            let other = ap.areas[side];
            let mut plane = tree.nodes[on].plane.unwrap_or(0) as i32;
            if area_of(p.nodes[0]) == other {
                plane ^= 1;
            }
            let hull = clip_portal_geometry(tree, portals, planes, &between, pid, src, other);
            if hull.len() >= 32 {
                log(&format!("Warning: area portal has {} verts. Could be a vbsp bug.", hull.len()));
            }
            if verts.len() + hull.len() >= MAX_MAP_PORTALVERTS {
                let at = p.winding.points.first().copied().unwrap_or_default();
                return Err(format!(
                    "MAX_MAP_PORTALVERTS (probably a broken areaportal near {:.1} {:.1} {:.1} ",
                    at.x, at.y, at.z
                ));
            }
            ap_lump.u16(ap.number);
            ap_lump.u16(other as u16);
            ap_lump.u16(verts.len() as u16);
            ap_lump.u16(hull.len() as u16);
            ap_lump.i32(plane);
            verts.extend(hull);
            ap_count += 1;
        }
        area_lump.i32((ap_count - first) as i32);
        area_lump.i32(first as i32);
    }
    if ap_count > MAX_MAP_AREAPORTALS {
        return Err(format!("Error! MAX_MAP_AREAPORTALS exceeded: {ap_count} of {MAX_MAP_AREAPORTALS}"));
    }
    let mut vert_lump = Buf::default();
    for v in &verts {
        vert_lump.vec3(*v);
    }
    log(&format!("{} areas, {} areaportals", areas.count + 1, ap_count));
    Ok((area_lump.0, ap_lump.0, vert_lump.0))
}

fn clip_portal_geometry(
    tree: &Tree,
    portals: &Portals,
    planes: &PlaneSet,
    between: &[usize],
    lead: usize,
    src: i32,
    dst: i32,
) -> Vec<DVec3> {
    let lp = &portals.list[lead].plane;
    let mut points: Vec<DVec3> = Vec::new();
    for p in between.iter().map(|&i| &portals.list[i]) {
        let Some(on) = p.on_node else { continue };
        let [a, b] = p.nodes;
        let (aa, ab) = (tree.nodes[a].area, tree.nodes[b].area);
        if !((aa == src && ab == dst) || (aa == dst && ab == src)) {
            continue;
        }
        let mp = planes.get(tree.nodes[on].plane.unwrap_or(0));
        let dot = mp.normal.dot(lp.normal).abs();
        if (1.0 - dot).abs() >= 0.01 {
            continue;
        }
        if (lp.normal * lp.dist).distance_squared(mp.normal * mp.dist) >= 0.01 {
            continue;
        }
        points.extend_from_slice(&p.winding.points);
    }
    let (f, r, u) = angle_basis(lp.normal);
    let flat: Vec<[f32; 2]> = points
        .iter()
        .map(|q| {
            let v = f * q.x + r * q.y + u * q.z;
            [v.y as f32, v.z as f32]
        })
        .collect();
    convex_2d(&flat).into_iter().map(|i| points[i]).collect()
}

fn angle_basis(n: DVec3) -> (DVec3, DVec3, DVec3) {
    let (pitch, yaw) = if n.x == 0.0 && n.y == 0.0 {
        (if n.z > 0.0 { 270.0f64 } else { 90.0 }, 0.0f64)
    } else {
        let mut yaw = n.y.atan2(n.x).to_degrees();
        if yaw < 0.0 {
            yaw += 360.0;
        }
        let mut pitch = (-n.z).atan2((n.x * n.x + n.y * n.y).sqrt()).to_degrees();
        if pitch < 0.0 {
            pitch += 360.0;
        }
        (pitch, yaw)
    };
    let (sy, cy) = yaw.to_radians().sin_cos();
    let (sp, cp) = pitch.to_radians().sin_cos();
    let (sr, cr) = (0.0f64, 1.0f64);
    let forward = DVec3::new(cp * cy, cp * sy, -sp);
    let right = DVec3::new(-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp);
    let up = DVec3::new(cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp);
    (forward, right, up)
}

fn angle_offset(base: f32, mut test: f32) -> f32 {
    let two_pi = 2.0 * std::f32::consts::PI;
    while test > base {
        test -= two_pi;
    }
    (base - test) % two_pi
}

fn convex_2d(points: &[[f32; 2]]) -> Vec<usize> {
    const MAX: usize = 512;
    let dist2 = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2);
    let mut map: Vec<usize> = Vec::new();
    for i in 0..points.len() {
        if !map.iter().any(|&j| dist2(points[i], points[j]) < 0.1 * 0.1) && map.len() < MAX {
            map.push(i);
        }
    }
    if map.is_empty() {
        return Vec::new();
    }
    let mut best = 0;
    for i in 1..map.len() {
        let (a, b) = (points[map[i]], points[map[best]]);
        if a[0] < b[0] || (a[0] == b[0] && a[1] < b[1]) {
            best = i;
        }
    }
    let mut out = vec![map[best]];
    let mut edge = [0.0f32, 1.0];
    loop {
        let start = points[out[out.len() - 1]];
        let edge_angle = edge[1].atan2(edge[0]);
        let mut min_index: Option<usize> = None;
        let mut min_angle = 5000.0f32;
        for &m in &map {
            let to = [points[m][0] - start[0], points[m][1] - start[1]];
            let d2 = to[0] * to[0] + to[1] * to[1];
            if d2 <= 0.1 {
                continue;
            }
            let angle = angle_offset(edge_angle, to[1].atan2(to[0]));
            if (angle - min_angle).abs() < 0.00001 {
                let current = min_index.map_or(0.0, |c| dist2(start, points[c]));
                if min_index != Some(out[0]) && d2 > current {
                    min_angle = angle;
                    min_index = Some(m);
                }
            } else if angle < min_angle {
                min_angle = angle;
                min_index = Some(m);
            }
        }
        match min_index {
            None => break,
            Some(m) if m == out[0] => break,
            Some(m) => {
                if out.len() >= MAX || out.contains(&m) {
                    break;
                }
                out.push(m);
            }
        }
        let (a, b) = (points[out[out.len() - 1]], points[out[out.len() - 2]]);
        edge = [a[0] - b[0], a[1] - b[1]];
    }
    out
}

pub fn sky_areas(tree: &Tree, planes: &PlaneSet, entities: &[&hp_vmf::Entity]) -> Result<Vec<i32>, String> {
    let mut out = Vec::new();
    for e in entities {
        if !e.classname.eq_ignore_ascii_case("sky_camera") {
            continue;
        }
        let o = e
            .get("origin")
            .map(|v| v.split_whitespace().filter_map(|x| x.parse::<f64>().ok()).collect::<Vec<_>>())
            .filter(|v| v.len() == 3)
            .map_or(DVec3::ZERO, |v| DVec3::new(v[0], v[1], v[2]));
        let leaf = tree.leaf_at(planes, o);
        if tree.nodes[leaf].contents & contents::SOLID != 0 {
            return Err(format!("Error! Entity sky_camera in solid volume! at {:.1} {:.1} {:.1}", o.x, o.y, o.z));
        }
        out.push(tree.nodes[leaf].area);
    }
    Ok(out)
}

pub fn forced_clusters(
    tree: &Tree,
    planes: &PlaneSet,
    brushes: &[MapBrush],
    visclusters: &[Vec<usize>],
    sky: &[i32],
    sky_vis: bool,
) -> Vec<Option<u32>> {
    let mut out = vec![None; tree.nodes.len()];
    for leaf in leaves_in_order(tree) {
        let node = &tree.nodes[leaf];
        if node.contents & contents::SOLID != 0 {
            continue;
        }
        if let Some(v) = node.volume.as_ref()
            && let Some(k) = viscluster_of(v, planes, brushes, visclusters)
        {
            out[leaf] = Some(k as u32);
            continue;
        }
        if !sky_vis && node.area > 0 && sky.contains(&node.area) {
            out[leaf] = Some(SKY_CLUSTER_KEY);
        }
    }
    out
}

fn viscluster_of(volume: &Frag, planes: &PlaneSet, brushes: &[MapBrush], visclusters: &[Vec<usize>]) -> Option<usize> {
    let threshold = volume.volume() * 0.10;
    visclusters.iter().position(|list| {
        let mut covered = 0.0;
        for &bi in list {
            let Some(b) = brushes.get(bi) else { continue };
            if b.mins.cmpgt(volume.maxs).any() || b.maxs.cmplt(volume.mins).any() {
                continue;
            }
            let mut piece = Some(volume.clone());
            for s in b.sides.iter().filter(|s| !s.bevel) {
                let Some(cur) = piece.take() else { break };
                piece = split_frag(&cur, s.plane, planes).1;
            }
            if let Some(p) = piece {
                covered += p.volume();
            }
        }
        covered > threshold
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hull_of_a_square_starts_at_the_lowest_left_point() {
        let pts = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [5.0, 5.0], [0.0, 0.05]];
        let hull = convex_2d(&pts);
        assert_eq!(hull.len(), 4);
        assert_eq!(hull[0], 0);
        assert!(!hull.contains(&4) && !hull.contains(&5));
    }

    #[test]
    fn the_basis_of_an_axial_normal() {
        let (f, r, u) = angle_basis(DVec3::X);
        assert!((f - DVec3::X).length() < 1e-9);
        assert!((r - DVec3::NEG_Y).length() < 1e-9);
        assert!((u - DVec3::Z).length() < 1e-9);
        let (f, _, _) = angle_basis(DVec3::NEG_Z);
        assert!((f - DVec3::NEG_Z).length() < 1e-9);
    }

    fn block(id: &mut u32, lo: [f64; 3], hi: [f64; 3], mat: &str) -> String {
        let p = |x: f64, y: f64, z: f64| format!("({x} {y} {z})");
        let ([x0, y0, z0], [x1, y1, z1]) = (lo, hi);
        let planes = [
            [p(x0, y1, z1), p(x1, y1, z1), p(x1, y0, z1)],
            [p(x0, y0, z0), p(x1, y0, z0), p(x1, y1, z0)],
            [p(x0, y1, z1), p(x0, y0, z1), p(x0, y0, z0)],
            [p(x1, y1, z0), p(x1, y0, z0), p(x1, y0, z1)],
            [p(x1, y1, z1), p(x0, y1, z1), p(x0, y1, z0)],
            [p(x1, y0, z0), p(x0, y0, z0), p(x0, y0, z1)],
        ];
        *id += 1;
        let mut s = format!(
            "	solid
	{{
		\"id\" \"{id}\"
"
        );
        for [a, b, c] in planes {
            *id += 1;
            s += &format!(
                "		side
		{{
			\"id\" \"{id}\"
			\"plane\" \"{a} {b} {c}\"
			\"material\" \"{mat}\"
			\"uaxis\" \"[1 0 0 0] 0.25\"
			\"vaxis\" \"[0 -1 0 0] 0.25\"
			\"lightmapscale\" \"16\"
		}}
"
            );
        }
        s + "	}
"
    }

    struct ByName;
    impl crate::MaterialSource for ByName {
        fn facts(&self, name: &str) -> crate::MaterialFacts {
            crate::materials::facts_by_name(name)
        }
    }

    fn i32_at(d: &[u8], o: usize) -> i32 {
        i32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
    }

    #[test]
    fn two_rooms_two_areas_one_occluder() {
        let w = "dev/dev_measuregeneric01b";
        let mut id = 1;
        let mut solids = String::new();
        for (lo, hi) in [
            ([-528.0, -272.0, -16.0], [544.0, 272.0, 0.0]),
            ([-528.0, -272.0, 256.0], [544.0, 272.0, 272.0]),
            ([-528.0, -272.0, 0.0], [-512.0, 272.0, 256.0]),
            ([528.0, -272.0, 0.0], [544.0, 272.0, 256.0]),
            ([-512.0, -272.0, 0.0], [528.0, -256.0, 256.0]),
            ([-512.0, 256.0, 0.0], [528.0, 272.0, 256.0]),
            ([0.0, -256.0, 0.0], [16.0, -64.0, 256.0]),
            ([0.0, 64.0, 0.0], [16.0, 256.0, 256.0]),
            ([0.0, -64.0, 128.0], [16.0, 64.0, 256.0]),
        ] {
            solids += &block(&mut id, lo, hi, w);
        }
        let mut ents = String::new();
        let mut ent = |id: &mut u32, kv: &str, solids: String| {
            *id += 1;
            ents += &format!(
                "entity
{{
	\"id\" \"{id}\"
{kv}{solids}}}
"
            );
        };
        let ap = block(&mut id, [6.0, -64.0, 0.0], [10.0, 64.0, 128.0], "tools/toolsareaportal");
        ent(
            &mut id,
            "	\"classname\" \"func_areaportal\"
	\"StartOpen\" \"1\"
",
            ap,
        );
        let occ = block(&mut id, [-300.0, -100.0, 0.0], [-280.0, 100.0, 200.0], "tools/toolsoccluder");
        ent(
            &mut id,
            "	\"classname\" \"func_occluder\"
	\"targetname\" \"occ\"
",
            occ,
        );
        let vc = block(&mut id, [-500.0, -250.0, 0.0], [-320.0, 250.0, 250.0], "tools/toolstrigger");
        ent(
            &mut id,
            "	\"classname\" \"func_viscluster\"
",
            vc,
        );
        ent(
            &mut id,
            "	\"classname\" \"info_player_start\"
	\"origin\" \"-60 0 8\"
",
            String::new(),
        );
        ent(
            &mut id,
            "	\"classname\" \"light\"
	\"origin\" \"270 0 200\"
",
            String::new(),
        );
        let text = format!(
            "versioninfo
{{
	\"mapversion\" \"1\"
}}
world
{{
	\"id\" \"1\"
	\"classname\" \"worldspawn\"
{solids}}}
{ents}"
        );
        let map = hp_vmf::parse(&text).expect("vmf");
        let mut lines = Vec::new();
        let out = crate::compile(&map, &ByName, &crate::props::NoModels, &crate::CompileOptions::default(), &mut |l| {
            lines.push(l.to_owned())
        })
        .unwrap_or_else(|e| {
            panic!(
                "{e}
{}",
                lines.join(
                    "
"
                )
            )
        });
        assert!(
            out.leak.is_none(),
            "{}",
            lines.join(
                "
"
            )
        );
        let bsp = hp_bsp::Bsp::from_vec(out.bsp.clone()).expect("bsp");
        let get = |i: usize| bsp.lump(i).map(|c| c.into_owned()).unwrap_or_default();
        let (areas, aps, verts) = (
            get(crate::emit::lump::AREAS),
            get(crate::emit::lump::AREAPORTALS),
            get(crate::emit::lump::CLIPPORTALVERTS),
        );
        assert_eq!(areas.len() / 8, 3);
        assert_eq!(aps.len() / 12, 3);
        assert_eq!(verts.len() / 12, 8);
        for k in [1, 2] {
            assert_eq!(u16::from_le_bytes([aps[k * 12], aps[k * 12 + 1]]), 1, "portal key");
            assert_eq!(u16::from_le_bytes([aps[k * 12 + 6], aps[k * 12 + 7]]), 4, "a quad opening");
        }
        for v in 0..8 {
            let x = f32::from_le_bytes([verts[v * 12], verts[v * 12 + 1], verts[v * 12 + 2], verts[v * 12 + 3]]);
            assert!(x == 6.0 || x == 10.0, "{x}");
        }
        let ents = bsp.entities().expect("entities");
        let ap = ents.iter().find(|e| e.classname() == "func_areaportal").expect("the areaportal stays");
        assert_eq!(ap.get("portalnumber"), Some("1"));
        assert!(!ents.iter().any(|e| e.classname() == "func_viscluster"));
        let occ = ents.iter().find(|e| e.classname() == "func_occluder").expect("the occluder stays");
        assert_eq!(occ.get("occludernumber"), Some("0"));
        assert!(occ.get("model").is_none_or(|m| m.is_empty()));
        let occlusion = get(crate::emit::lump::OCCLUSION);
        assert_eq!(i32_at(&occlusion, 0), 1);
        assert_eq!(i32_at(&occlusion, 4 + 8), 6);
        let area = i32_at(&occlusion, 4 + 36);
        assert!(area == 1 || area == 2, "{area}");
        assert!(crate::validate::validate(&out.bsp).is_empty());
        let leafs = get(crate::emit::lump::LEAFS);
        let mut clusters = std::collections::BTreeSet::new();
        let mut open = 0;
        for l in 0..leafs.len() / 32 {
            if i32_at(&leafs, l * 32) as u32 & contents::SOLID == 0 {
                open += 1;
                clusters.insert(i16::from_le_bytes([leafs[l * 32 + 4], leafs[l * 32 + 5]]));
            }
        }
        assert!(clusters.len() < open);
    }

    #[test]
    fn areaportal_class_names() {
        assert!(is_areaportal_class("func_areaportal"));
        assert!(is_areaportal_class("func_areaportalwindow"));
        assert!(is_areaportal_class("FUNC_AreaPortal"));
        assert!(!is_areaportal_class("func_area"));
        assert!(!is_areaportal_class("func_door"));
    }
}
