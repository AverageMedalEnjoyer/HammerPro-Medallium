use crate::disp::DispFace;

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn length(v: V3) -> f32 {
    ((v[0] * v[0] + v[1] * v[1]) + v[2] * v[2]).sqrt()
}

fn normalize(v: V3) -> V3 {
    let inv = 1.0f32 / (length(v) + 1.0e-10f32);
    [v[0] * inv, v[1] * inv, v[2] * inv]
}

#[derive(Clone, Debug)]
pub struct Core {
    pub power: u32,
    pub n: usize,
    pub points: [V3; 4],
    pub verts: Vec<V3>,
    pub field: Vec<(V3, f32)>,
    pub alphas: Vec<f32>,
}

impl Core {
    fn index(&self, x: i32, y: i32) -> usize {
        y as usize * self.n + x as usize
    }
}

pub fn core(d: &DispFace) -> Option<Core> {
    let power = d.info.power;
    if !(1..=4).contains(&power) || d.winding.points.len() != 4 {
        return None;
    }
    let n = (1usize << power) + 1;
    let w: Vec<V3> = d.winding.points.iter().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect();
    if w.iter().flatten().any(|v| !v.is_finite()) {
        return None;
    }
    let s = d.info.start_position;
    let start = [s.x as f32, s.y as f32, s.z as f32];
    let mut first = 0;
    let mut best = 999_999_999.0f32;
    for (i, p) in w.iter().enumerate() {
        let v = sub(start, *p);
        let d2 = (v[0] * v[0] + v[1] * v[1]) + v[2] * v[2];
        if d2 < best {
            best = d2;
            first = i;
        }
    }
    let points = [w[first], w[(first + 1) % 4], w[(first + 2) % 4], w[(first + 3) % 4]];
    let count = n * n;
    let mut field = Vec::with_capacity(count);
    let mut alphas = Vec::with_capacity(count);
    for k in 0..count {
        let nrm = d.info.normals.get(k).map_or([0.0; 3], |v| [v.x as f32, v.y as f32, v.z as f32]);
        let dist = d.info.distances.get(k).map_or(0.0, |v| *v as f32);
        let off = d.info.offsets.get(k).map_or([0.0; 3], |v| [v.x as f32, v.y as f32, v.z as f32]);
        let v = [nrm[0] * dist + off[0], nrm[1] * dist + off[1], nrm[2] * dist + off[2]];
        let len = ((v[1] * v[1] + v[0] * v[0]) + v[2] * v[2]).sqrt();
        field.push((normalize(v), len));
        alphas.push(d.info.alphas.get(k).map_or(0.0, |a| *a as f32));
    }
    let oo = 1.0f32 / (n - 1) as f32;
    let e0 = sub(points[1], points[0]).map(|c| c * oo);
    let e1 = sub(points[2], points[3]).map(|c| c * oo);
    let mut verts = Vec::with_capacity(count);
    for i in 0..n {
        let fi = i as f32;
        let a = [e0[0] * fi + points[0][0], e0[1] * fi + points[0][1], e0[2] * fi + points[0][2]];
        let b = [e1[0] * fi + points[3][0], e1[1] * fi + points[3][1], e1[2] * fi + points[3][2]];
        let seg = sub(b, a).map(|c| c * oo);
        for j in 0..n {
            let fj = j as f32;
            let (fv, fd) = field[i * n + j];
            let mut v = [0.0f32; 3];
            for c in 0..3 {
                let flat = seg[c] * fj + a[c];
                v[c] = (flat + 0.0) + fv[c] * fd;
            }
            verts.push(v);
        }
    }
    Some(Core { power, n, points, verts, field, alphas })
}

const WINDING: [((i32, i32), i32); 9] = [
    ((1, -1), 3),
    ((0, -1), -1),
    ((-1, -1), 2),
    ((-1, 0), -1),
    ((-1, 1), 1),
    ((0, 1), -1),
    ((1, 1), 0),
    ((1, 0), -1),
    ((1, -1), 3),
];
const CHILDREN: [(i32, i32); 4] = [(1, 1), (-1, 1), (-1, -1), (1, -1)];

pub fn tesselate(c: &Core, allowed: &dyn Fn(usize) -> bool) -> Vec<[usize; 3]> {
    fn node(c: &Core, allowed: &dyn Fn(usize) -> bool, x: i32, y: i32, level: u32, out: &mut Vec<[usize; 3]>) {
        let mut active = [false; 4];
        if level + 1 < c.power {
            let inc = 1 << (c.power - level - 2);
            for (k, (dx, dy)) in CHILDREN.iter().enumerate() {
                let (cx, cy) = (x + dx * inc, y + dy * inc);
                active[k] = allowed(c.index(cx, cy));
                if active[k] {
                    node(c, allowed, cx, cy, level + 1, out);
                }
            }
        }
        let inc = 1 << (c.power - level - 1);
        let center = c.index(x, y);
        let mut temp = [0usize; 2];
        let mut cur = 0;
        for ((dx, dy), child) in WINDING {
            if child >= 0 && active[child as usize] {
                cur = 0;
                continue;
            }
            let v = c.index(x + dx * inc, y + dy * inc);
            if allowed(v) {
                temp[cur] = v;
                cur += 1;
                if cur == 2 {
                    out.push([temp[0], temp[1], center]);
                    temp[0] = temp[1];
                    cur = 1;
                }
            }
        }
    }
    let mut out = Vec::new();
    let half = (c.n / 2) as i32;
    node(c, allowed, half, half, 0, &mut out);
    out
}

fn tri_infos(c: &Core) -> Vec<[usize; 3]> {
    fn node(c: &Core, x: i32, y: i32, level: u32, out: &mut Vec<[usize; 3]>) {
        if level + 1 < c.power {
            let inc = 1 << (c.power - level - 2);
            for (dx, dy) in CHILDREN {
                node(c, x + dx * inc, y + dy * inc, level + 1, out);
            }
            return;
        }
        let inc = 1 << (c.power - level - 1);
        let mut prev = None;
        for ((dx, dy), _) in WINDING {
            let v = c.index(x + dx * inc, y + dy * inc);
            if let Some(p) = prev {
                out.push([p, v, c.index(x, y)]);
            }
            prev = Some(v);
        }
    }
    let mut out = Vec::new();
    let half = (c.n / 2) as i32;
    node(c, half, half, 0, &mut out);
    out
}

fn barycentric(a: [f32; 2], b: [f32; 2], c: [f32; 2], p: [f32; 2]) -> [f32; 3] {
    let area2 = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    let inv = 1.0f32 / area2(a, b, c);
    [area2(b, c, p) * inv, area2(c, a, p) * inv, area2(a, b, p) * inv]
}

fn inside(bc: [f32; 3]) -> bool {
    bc.iter().all(|v| (0.0..=1.0).contains(v))
}

pub fn dispverts(c: &Core, allowed: &dyn Fn(usize) -> bool) -> Vec<(V3, f32, f32)> {
    snap(c, allowed).0
}

pub fn snap(c: &Core, allowed: &dyn Fn(usize) -> bool) -> (Vec<(V3, f32, f32)>, Vec<V3>) {
    let mut out: Vec<(V3, f32, f32)> = (0..c.n * c.n).map(|k| (c.field[k].0, c.field[k].1, c.alphas[k])).collect();
    let tris = tesselate(c, allowed);
    let mut touched = vec![false; c.n * c.n];
    for t in &tris {
        for &v in t {
            touched[v] = true;
        }
    }
    let coord = |k: usize| [(k % c.n) as f32, (k / c.n) as f32];
    let mut verts = c.verts.clone();
    for k in 0..c.n * c.n {
        if touched[k] {
            continue;
        }
        let p = coord(k);
        let Some((t, bc)) = tris.iter().find_map(|t| {
            let bc = barycentric(coord(t[0]), coord(t[1]), coord(t[2]), p);
            inside(bc).then_some((t, bc))
        }) else {
            continue;
        };
        let (a, b, cc) = (verts[t[0]], verts[t[1]], verts[t[2]]);
        let mut np = [0.0f32; 3];
        for q in 0..3 {
            np[q] = (a[q] * bc[0] + b[q] * bc[1]) + cc[q] * bc[2];
        }
        let off = sub(np, verts[k]);
        let (fv, fd, alpha) = out[k];
        out[k] = ([fv[0] * fd + off[0], fv[1] * fd + off[1], fv[2] * fd + off[2]], 1.0, alpha);
        verts[k] = np;
    }
    (out, verts)
}

#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn luxel_size(c: &Core, lightmap: &[[f32; 4]; 2]) -> Option<([i32; 2], bool, [[f32; 2]; 4])> {
    let u: V3 = [lightmap[0][0], lightmap[0][1], lightmap[0][2]];
    let v: V3 = [lightmap[1][0], lightmap[1][1], lightmap[1][2]];
    let luxels = (1.0f32 / length(u)) as i32;
    if luxels <= 0 {
        return None;
    }
    let (nu, nv) = (normalize(u), normalize(v));
    let dot = |a: V3, b: V3| (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2];
    let du: Vec<f32> = c.points.iter().map(|p| dot(nu, *p)).collect();
    let dv: Vec<f32> = c.points.iter().map(|p| dot(nv, *p)).collect();
    let (mut lu, mut lv) = (0.0f32, 0.0f32);
    for i in 0..4 {
        lu = lu.max((du[(i + 1) % 4] - du[i]).abs());
        lv = lv.max((dv[(i + 1) % 4] - dv[i]).abs());
    }
    let long_u = !(lu < lv);
    let p = &c.points;
    let ulen = length(sub(p[3], p[0])).max(length(sub(p[2], p[1])));
    let vlen = length(sub(p[1], p[0])).max(length(sub(p[2], p[3])));
    let scale = 1.0f32 / luxels as f32;
    let uval = (((ulen * scale) as i32 + 1) as f32).min(125.0);
    let vval = (((vlen * scale) as i32 + 1) as f32).min(125.0);
    let swapped = if long_u { vval > uval } else { uval > vval };
    let corners = [[0.5, 0.5], [0.5, vval + 0.5], [uval + 0.5, vval + 0.5], [uval + 0.5, 0.5]];
    Some(([uval as i32, vval as i32], swapped, corners))
}

fn luxel_coords(c: &Core, corners: &[[f32; 2]; 4]) -> Vec<[f32; 2]> {
    let n = c.n;
    let oo = 1.0f32 / (n - 1) as f32;
    let e0 = [(corners[1][0] - corners[0][0]) * oo, (corners[1][1] - corners[0][1]) * oo];
    let e1 = [(corners[2][0] - corners[3][0]) * oo, (corners[2][1] - corners[3][1]) * oo];
    let mut out = Vec::with_capacity(n * n);
    for i in 0..n {
        let fi = i as f32;
        let a = [e0[0] * fi + corners[0][0], e0[1] * fi + corners[0][1]];
        let b = [e1[0] * fi + corners[3][0], e1[1] * fi + corners[3][1]];
        let seg = [(b[0] - a[0]) * oo, (b[1] - a[1]) * oo];
        for j in 0..n {
            let fj = j as f32;
            out.push([a[0] + seg[0] * fj, a[1] + seg[1] * fj]);
        }
    }
    out
}

pub fn lightmap_samples(c: &Core, lightmap: &[[f32; 4]; 2]) -> Vec<u8> {
    let Some((size, _, corners)) = luxel_size(c, lightmap) else { return Vec::new() };
    let uv = luxel_coords(c, &corners);
    let tris = tri_infos(c);
    let mut out = Vec::new();
    for y in 0..=size[1] {
        for x in 0..=size[0] {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let found = tris.iter().enumerate().find_map(|(i, t)| {
                let bc = barycentric(uv[t[0]], uv[t[1]], uv[t[2]], p);
                inside(bc).then_some((i, bc))
            });
            match found {
                Some((i, bc)) => {
                    if i < 255 {
                        out.push(i as u8);
                    } else {
                        out.push(255);
                        out.push((i - 255) as u8);
                    }
                    for b in bc {
                        out.push((b * 255.9f32) as u8);
                    }
                }
                None => out.extend_from_slice(&[0, 0, 0, 0]),
            }
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubNeighbor {
    pub index: u16,
    pub orientation: u8,
    pub span: u8,
    pub neighbor_span: u8,
}

impl Default for SubNeighbor {
    fn default() -> Self {
        SubNeighbor { index: 0xFFFF, orientation: 0, span: 0, neighbor_span: 0 }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Neighbors {
    pub edges: [[SubNeighbor; 2]; 4],
    pub corners: [Vec<u16>; 4],
}

const CORNER_TO_CORNER: u8 = 0;
const CORNER_TO_MIDPOINT: u8 = 1;
const MIDPOINT_TO_CORNER: u8 = 2;
const SPAN_FLIP: [u8; 3] = [CORNER_TO_CORNER, MIDPOINT_TO_CORNER, CORNER_TO_MIDPOINT];
const EDGE_FLIP: [bool; 4] = [false, false, true, true];
const ORIENTATION: [[u8; 4]; 4] = [[2, 3, 0, 1], [1, 2, 3, 0], [0, 1, 2, 3], [3, 0, 1, 2]];

fn equal(a: V3, b: V3, tol: f32) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= tol)
}

pub fn neighbors(cores: &[Option<Core>]) -> Vec<Neighbors> {
    let n = cores.len();
    let mut out = vec![Neighbors::default(); n];
    let boxes: Vec<Option<(V3, V3)>> = cores
        .iter()
        .map(|c| {
            c.as_ref().map(|c| {
                let mut lo = [1e24f32; 3];
                let mut hi = [-1e24f32; 3];
                for p in &c.points {
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
                (lo.map(|v| v - 0.1), hi.map(|v| v + 0.1))
            })
        })
        .collect();
    let find_edge = |c: &Core, p1: V3, p2: V3| {
        (0..4).find(|&e| equal(p1, c.points[e], 0.01) && equal(p2, c.points[(e + 1) & 3], 0.01))
    };
    let mut overflows = 0;
    for a in 0..n {
        let Some(ca) = cores[a].as_ref() else { continue };
        for b in a + 1..n {
            let Some(cb) = cores[b].as_ref() else { continue };
            let (Some(ba), Some(bb)) = (boxes[a], boxes[b]) else { continue };
            if (0..3).any(|k| ba.1[k] < bb.0[k] || ba.0[k] > bb.1[k]) {
                continue;
            }
            for (edge, &flip) in EDGE_FLIP.iter().enumerate() {
                let p0 = ca.points[edge];
                let p1 = ca.points[(edge + 1) & 3];
                let mid = [(p0[0] + p1[0]) * 0.5, (p0[1] + p1[1]) * 0.5, (p0[2] + p1[2]) * 0.5];
                let twice = |x: V3, y: V3| [x[0] * 2.0 - y[0], x[1] * 2.0 - y[1], x[2] * 2.0 - y[2]];
                let mut add = |sub: usize, span: u8, nb_edge: usize, nb_span: u8| {
                    add_neighbor(&mut out, a, edge, sub, span, b, nb_edge, nb_span);
                };
                if let Some(e) = find_edge(cb, p1, p0) {
                    add(0, CORNER_TO_CORNER, e, CORNER_TO_CORNER);
                } else if let Some(e) = find_edge(cb, p1, twice(p0, p1)) {
                    add(0, CORNER_TO_CORNER, e, CORNER_TO_MIDPOINT);
                } else if let Some(e) = find_edge(cb, twice(p1, p0), p0) {
                    add(0, CORNER_TO_CORNER, e, MIDPOINT_TO_CORNER);
                } else {
                    if let Some(e) = find_edge(cb, mid, p0) {
                        add(usize::from(flip), CORNER_TO_MIDPOINT, e, CORNER_TO_CORNER);
                    }
                    if let Some(e) = find_edge(cb, p1, mid) {
                        add(usize::from(!flip), MIDPOINT_TO_CORNER, e, CORNER_TO_CORNER);
                    }
                }
            }
            let is_edge_neighbor = out[a].corners.iter().any(|c| c.contains(&(b as u16)))
                || out[a].edges.iter().any(|e| e[0].index == b as u16 || e[1].index == b as u16);
            if is_edge_neighbor {
                continue;
            }
            let corner = |c: &Core, k: usize| {
                let (x, y) = [(0, 0), (0, c.n - 1), (c.n - 1, c.n - 1), (c.n - 1, 0)][k];
                c.verts[y * c.n + x]
            };
            let mut shared = 0;
            let (mut ka, mut kb) = (0, 0);
            for i in 0..4 {
                for j in 0..4 {
                    if equal(corner(ca, i), corner(cb, j), 0.001) {
                        ka = i;
                        kb = j;
                        shared += 1;
                    }
                }
            }
            if shared == 1 {
                if out[a].corners[ka].len() < 4 && out[b].corners[kb].len() < 4 {
                    out[a].corners[ka].push(b as u16);
                    out[b].corners[kb].push(a as u16);
                } else {
                    overflows += 1;
                }
            }
        }
    }
    let _ = overflows;
    out
}

#[allow(clippy::too_many_arguments)]
fn add_neighbor(
    out: &mut [Neighbors],
    a: usize,
    edge: usize,
    sub: usize,
    span: u8,
    b: usize,
    nb_edge: usize,
    nb_span: u8,
) {
    let flip = |e: usize, s: u8| if EDGE_FLIP[e] { SPAN_FLIP[s as usize] } else { s };
    let span = flip(edge, span);
    let nb_span = flip(nb_edge, nb_span);
    let nb_sub = usize::from(nb_span == MIDPOINT_TO_CORNER);
    if out[a].edges[edge][sub].index != 0xFFFF || out[b].edges[nb_edge][nb_sub].index != 0xFFFF {
        return;
    }
    out[a].edges[edge][sub] =
        SubNeighbor { index: b as u16, orientation: ORIENTATION[edge][nb_edge], span, neighbor_span: nb_span };
    out[b].edges[nb_edge][nb_sub] =
        SubNeighbor { index: a as u16, orientation: ORIENTATION[nb_edge][edge], span: nb_span, neighbor_span: span };
}

fn dot(a: V3, b: V3) -> f32 {
    (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub fn side_winding(planes: &[(V3, f32)], side: usize) -> Option<Vec<V3>> {
    let (normal, dist) = *planes.get(side)?;
    let mut axis = None;
    let mut max = -1.0f32;
    for (i, c) in normal.iter().enumerate() {
        if c.abs() > max {
            axis = Some(i);
            max = c.abs();
        }
    }
    let mut up = [0.0f32; 3];
    match axis? {
        0 | 1 => up[2] = 1.0,
        _ => up[0] = 1.0,
    }
    let v = dot(up, normal);
    up = [up[0] + normal[0] * -v, up[1] + normal[1] * -v, up[2] + normal[2] * -v];
    up = normalize(up);
    let org = normal.map(|c| c * dist);
    let right = cross(up, normal);
    const BIG: f32 = 16384.0 * 4.0;
    let up = up.map(|c| c * BIG);
    let right = right.map(|c| c * BIG);
    let mut w: Vec<V3> = vec![
        [org[0] - right[0] + up[0], org[1] - right[1] + up[1], org[2] - right[2] + up[2]],
        [org[0] + right[0] + up[0], org[1] + right[1] + up[1], org[2] + right[2] + up[2]],
        [org[0] + right[0] - up[0], org[1] + right[1] - up[1], org[2] + right[2] - up[2]],
        [org[0] - right[0] - up[0], org[1] - right[1] - up[1], org[2] - right[2] - up[2]],
    ];
    for (j, &(n, d)) in planes.iter().enumerate() {
        if j == side {
            continue;
        }
        w = chop(&w, n.map(|c| -c), -d, 0.01)?;
    }
    Some(w)
}

fn chop(w: &[V3], normal: V3, dist: f32, eps: f32) -> Option<Vec<V3>> {
    let dists: Vec<f32> = w.iter().map(|p| dot(*p, normal) - dist).collect();
    let sides: Vec<u8> = dists
        .iter()
        .map(|&d| {
            if d > eps {
                0
            } else if d < -eps {
                1
            } else {
                2
            }
        })
        .collect();
    if !sides.contains(&0) {
        return None;
    }
    if !sides.contains(&1) {
        return Some(w.to_vec());
    }
    let n = w.len();
    let mut out = Vec::with_capacity(n + 4);
    for i in 0..n {
        let p1 = w[i];
        if sides[i] == 2 {
            out.push(p1);
            continue;
        }
        if sides[i] == 0 {
            out.push(p1);
        }
        let next = sides[(i + 1) % n];
        if next == 2 || next == sides[i] {
            continue;
        }
        let p2 = w[(i + 1) % n];
        let t = dists[i] / (dists[i] - dists[(i + 1) % n]);
        let mut mid = [0.0f32; 3];
        for k in 0..3 {
            mid[k] = if normal[k] == 1.0 {
                dist
            } else if normal[k] == -1.0 {
                -dist
            } else {
                p1[k] + t * (p2[k] - p1[k])
            };
        }
        out.push(mid);
    }
    Some(out)
}
