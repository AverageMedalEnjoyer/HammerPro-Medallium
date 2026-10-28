use std::collections::HashMap;

pub use super::qhull::QhFacet;

pub type QhullFn<'a> = dyn FnMut(&[[f64; 3]], Option<f64>) -> Result<Vec<QhFacet>, i32> + 'a;

pub type HalfspaceFn<'a> = dyn FnMut(&[[f32; 4]], f32, &mut QhullFn) -> Option<Vec<u8>> + 'a;

const IVP_SCALE: f32 = 0.0254;

pub fn source_to_ivp(p: [f32; 3]) -> [f32; 3] {
    [IVP_SCALE * p[0], -(IVP_SCALE * p[2]), IVP_SCALE * p[1]]
}

pub fn pointsoup_to_ledge(points: &[[f32; 3]], qhull: &mut QhullFn) -> Option<Vec<u8>> {
    if points.len() < 3 {
        return None;
    }
    if points.len() == 3 {
        return triangle_ledge(points[0], points[1], points[2]);
    }
    pointsoup_driver(points, qhull)
}

const TRI_TEMPLATE: [u8; 48] = [
    0x30, 0, 0, 0, 0, 0, 0, 0, 0x04, 0x06, 0, 0, 0x02, 0, 0, 0, 0x00, 0x10, 0, 0, 0x00, 0, 0x04, 0, 0x01, 0, 0x05, 0,
    0x02, 0, 0x03, 0, 0x01, 0, 0, 0, 0x01, 0, 0xfc, 0x7f, 0x00, 0, 0xfd, 0x7f, 0x02, 0, 0xfb, 0x7f,
];

#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn triangle_ledge(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Option<Vec<u8>> {
    let f3 = (c[1] - a[1]) * (b[2] - a[2]) - (c[2] - a[2]) * (b[1] - a[1]);
    let f5 = (c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0]);
    let f4 = (c[2] - a[2]) * (b[0] - a[0]) - (c[0] - a[0]) * (b[2] - a[2]);
    let thr = f32::from_bits(0x2b8cbccc);
    let s = (f4 * f4 + f3 * f3) + f5 * f5;
    if !(s >= thr) {
        return None;
    }
    let mut out = Vec::with_capacity(96);
    out.extend_from_slice(&TRI_TEMPLATE);
    for p in [a, b, c] {
        for pk in p {
            out.extend_from_slice(&pk.to_le_bytes());
        }
        out.extend_from_slice(&0f32.to_le_bytes());
    }
    Some(out)
}

fn bits3(p: &[f32; 3]) -> [u32; 3] {
    [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]
}

fn pointsoup_driver(points: &[[f32; 3]], qhull: &mut QhullFn) -> Option<Vec<u8>> {
    let mut seen: HashMap<[u32; 3], ()> = HashMap::new();
    let mut pts: Vec<[f32; 3]> = Vec::new();
    for p in points {
        if seen.insert(bits3(p), ()).is_none() {
            pts.push(*p);
        }
    }
    let joggle_mul = 1.2000000476837158f64;
    let joggle_add = 1.2000000428887523e-12f64;
    let joggle_max = 0.019999999552965164f64;
    let mut joggle = 9.999999960041972e-13f64;
    let mut normal_mode = true;
    let mut old_n = pts.len();
    loop {
        let dpts: Vec<[f64; 3]> = pts.iter().map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
        let mut res = if normal_mode { qhull(&dpts, None) } else { Err(1) };
        if res.is_err() {
            res = qhull(&dpts, Some(joggle));
            if res.is_err() {
                normal_mode = false;
                joggle = joggle * joggle_mul + joggle_add;
                if joggle_max <= joggle {
                    return None;
                }
                continue;
            }
        }
        let facets = res.unwrap();
        let n = pts.len();
        let mut used = vec![0u8; n];
        let mut removed = vec![0u8; n];
        if let Some(l) = facets_to_ledge(&pts, &facets, &mut used, &mut removed) {
            return Some(l);
        }
        let np: Vec<[f32; 3]> = (0..n).filter(|&i| used[i] != 0 && removed[i] == 0).map(|i| pts[i]).collect();
        pts = np;
        let nn = pts.len();
        if nn == old_n {
            normal_mode = false;
            joggle = joggle * joggle_mul + joggle_add;
        }
        old_n = nn;
        if nn == 3 {
            return triangle_ledge(pts[0], pts[1], pts[2]);
        }
        if nn < 4 {
            return None;
        }
        if joggle_max <= joggle {
            return None;
        }
    }
}

struct Poly {
    normal: [f32; 3],
    vids: Vec<usize>,
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - b[0] * a[1]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dist3(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    ((dy * dy + dx * dx) + dz * dz).sqrt()
}

fn poly_area(pts: &[[f32; 3]], p: &Poly) -> f32 {
    let n = p.vids.len();
    let p0 = pts[p.vids[0]];
    let mut s = 0f32;
    for i in 0..n {
        let a = pts[p.vids[i]];
        let b = pts[p.vids[(i + 1) % n]];
        let e = sub(b, a);
        let d = sub(a, p0);
        let c = cross(e, d);
        s += (c[1] * p.normal[1] + c[0] * p.normal[0]) + c[2] * p.normal[2];
    }
    s
}

fn poly_perimeter(pts: &[[f32; 3]], p: &Poly) -> f32 {
    let n = p.vids.len();
    let mut s = 0f32;
    for i in 0..n {
        s += dist3(pts[p.vids[i]], pts[p.vids[(i + 1) % n]]);
    }
    s
}

fn facets_to_ledge(pts: &[[f32; 3]], facets: &[QhFacet], used: &mut [u8], removed: &mut [u8]) -> Option<Vec<u8>> {
    let mut error = false;
    let mut polys: Vec<Poly> = Vec::with_capacity(facets.len());
    let neg1 = -1.0f32;
    for f in facets {
        let mut p = Poly { normal: [f.normal[0] as f32, f.normal[1] as f32, f.normal[2] as f32], vids: Vec::new() };
        for &v in &f.vertices {
            p.vids.push(v);
            used[v] = used[v].wrapping_add(1);
        }
        let mut area = poly_area(pts, &p);
        let perim = poly_perimeter(pts, &p);
        if area < 0.0 {
            p.normal = [-p.normal[0], -p.normal[1], -p.normal[2]];
            area *= neg1;
        }
        if (area as f64) < (perim as f64) * 0.004999999888241291f64 {
            let n = p.vids.len();
            let mut skip = false;
            let mut i1 = 0usize;
            let mut best = neg1;
            for i in 0..n {
                if removed[p.vids[i]] != 0 {
                    skip = true;
                    break;
                }
                let d = dist3(pts[p.vids[0]], pts[p.vids[i]]);
                if best < d {
                    i1 = i;
                    best = d;
                }
            }
            if !skip {
                let mut i2 = 0usize;
                let mut best = neg1;
                for i in 0..n {
                    error = true;
                    let d = dist3(pts[p.vids[i1]], pts[p.vids[i]]);
                    if best < d {
                        i2 = i;
                        best = d;
                    }
                }
                for i in 0..n {
                    if i != i2 && i != i1 {
                        let v = p.vids[i];
                        removed[v] = removed[v].wrapping_add(1);
                        break;
                    }
                }
            }
        }
        polys.push(p);
    }
    if error {
        return None;
    }
    let t = build_template(pts, &polys);
    template_to_ledge(&t)
}

struct Surf {
    normal: [f32; 3],
    lines: Vec<u16>,
    rev: Vec<u8>,
}

struct Template {
    pts: Vec<[f32; 3]>,
    lines: Vec<[u16; 2]>,
    surfs: Vec<Surf>,
}

fn tfind(tp: &[[f32; 3]], p: [f32; 3]) -> usize {
    for (i, q) in tp.iter().enumerate() {
        if p[0] == q[0] && p[1] == q[1] && p[2] == q[2] {
            return i;
        }
    }
    0
}

fn build_template(pts: &[[f32; 3]], polys: &[Poly]) -> Template {
    let mut tp: Vec<[f32; 3]> = Vec::new();
    for p in pts {
        if tfind(&tp, *p) == 0 {
            tp.push(*p);
        }
    }
    let mut lines: Vec<[u16; 2]> = Vec::new();
    let mut lhash: HashMap<(u16, u16), ()> = HashMap::new();
    for poly in polys {
        let n = poly.vids.len();
        for i in 0..n {
            let a = tfind(&tp, pts[poly.vids[i]]) as u16;
            let b = tfind(&tp, pts[poly.vids[(i + 1) % n]]) as u16;
            let key = if b < a { (a, b) } else { (b, a) };
            if lhash.insert(key, ()).is_none() {
                lines.push([a, b]);
            }
        }
    }
    let mut surfs = Vec::with_capacity(polys.len());
    let mut last_rev = 0u8;
    for poly in polys {
        let n = poly.vids.len();
        let mut vi: Vec<u16> = (0..n).map(|i| tfind(&tp, pts[poly.vids[i]]) as u16).collect();
        vi.push(vi[0]);
        let mut sl = Vec::with_capacity(n);
        let mut sr = Vec::with_capacity(n);
        for i in 0..n {
            let (a, b) = (vi[i], vi[i + 1]);
            let mut idx = 0u16;
            for (k, l) in lines.iter().enumerate() {
                if l[0] == a && l[1] == b {
                    last_rev = 0;
                    idx = k as u16;
                    break;
                }
                if l[1] == a && l[0] == b {
                    last_rev = 1;
                    idx = k as u16;
                    break;
                }
            }
            sl.push(idx);
            sr.push(1 - last_rev);
        }
        surfs.push(Surf { normal: poly.normal, lines: sl, rev: sr });
    }
    Template { pts: tp, lines, surfs }
}

#[derive(Clone, Copy)]
struct P2 {
    x: f32,
    y: f32,
    idx: u32,
}

#[derive(Clone, Copy)]
struct E2 {
    s: usize,
    e: usize,
    dx: f32,
    dy: f32,
}

const EPS_SIDE: u32 = 0x2edbe6ff;
const EPS_DENOM: u32 = 0x3089705f;
const EPS_DIST: u32 = 0x322bcc76;
const EPS_COL: u32 = 0x322bcc77;
const EPS_COL_NEG: u32 = 0xb22bcc77;
const EPS_PT: u32 = 0x38d1b717;

struct Poly2 {
    p: Vec<P2>,
    e: Vec<E2>,
}

impl Poly2 {
    fn mk(&mut self, s: usize, e: usize) -> usize {
        let a = self.p[s];
        let b = self.p[e];
        self.e.push(E2 { s, e, dx: b.x - a.x, dy: b.y - a.y });
        self.e.len() - 1
    }
    fn side_val(&self, e: usize, q: usize) -> f32 {
        let ed = self.e[e];
        let s = self.p[ed.s];
        let q = self.p[q];
        (q.y - s.y) * ed.dx + (s.x - q.x) * ed.dy
    }
    fn left(&self, e: usize, q: usize) -> bool {
        self.side_val(e, q) > f32::from_bits(EPS_SIDE)
    }
    fn same(&self, x: usize, a: usize, b: usize) -> bool {
        let e = self.e[x];
        (e.s == a && e.e == b) || (e.e == a && e.s == b)
    }
    fn line_dist(&self, x: usize, q: usize) -> f32 {
        let e = self.e[x];
        let s = self.p[e.s];
        let q = self.p[q];
        let l2 = e.dx * e.dx + e.dy * e.dy;
        if l2 < f32::from_bits(EPS_SIDE) {
            return (s.y - q.y) * (s.y - q.y) + (s.x - q.x) * (s.x - q.x);
        }
        ((q.y - s.y) * e.dx + (s.x - q.x) * e.dy) / l2.sqrt()
    }
    fn point_on(&self, a: usize, q: usize) -> bool {
        let e = self.e[a];
        let s = self.p[e.s];
        let q = self.p[q];
        let mut f4 = e.dx;
        let mut f2 = e.dx.abs();
        let f3;
        if f2 <= f32::from_bits(EPS_SIDE) {
            f4 = e.dy;
            f2 = e.dy.abs();
            if f2 <= f32::from_bits(EPS_SIDE) {
                return false;
            }
            f3 = q.y - s.y;
        } else {
            f3 = q.x - s.x;
        }
        let sg: f32 = if 0.0 < f4 { 1.0 } else { -1.0 };
        let eps = f32::from_bits(EPS_PT);
        eps < f3 * sg && f3 * sg < f2 - eps
    }
    fn overlap(&self, x: usize, a: usize) -> bool {
        let ex = self.e[x];
        let ea = self.e[a];
        let eps = f32::from_bits(EPS_COL);
        if ex.dx.abs() <= eps {
            if ex.dy.abs() <= eps {
                return self.point_on(a, ex.s);
            }
            let (lo, hi) =
                if ex.dy <= 0.0 { (self.p[ex.e].y, self.p[ex.s].y) } else { (self.p[ex.s].y, self.p[ex.e].y) };
            let hi = hi + f32::from_bits(EPS_COL_NEG);
            let y1 = self.p[ea.s].y;
            let y2 = self.p[ea.e].y;
            if y1 <= lo + eps && y2 <= lo + eps {
                return false;
            }
            if y1 < hi {
                return true;
            }
            y2 < hi
        } else {
            let (lo, hi) =
                if ex.dx <= 0.0 { (self.p[ex.e].x, self.p[ex.s].x) } else { (self.p[ex.s].x, self.p[ex.e].x) };
            let x1 = self.p[ea.s].x;
            let x2 = self.p[ea.e].x;
            if x1 <= lo + eps && x2 <= lo + eps {
                return false;
            }
            if x1 < hi - eps {
                return true;
            }
            x2 < hi - eps
        }
    }
    fn crosses(&self, x: usize, a: usize) -> bool {
        let ex = self.e[x];
        let ea = self.e[a];
        let den = ex.dy * ea.dx - ex.dx * ea.dy;
        if f32::from_bits(EPS_DENOM) <= den.abs() {
            if ex.s != ea.e && ex.s != ea.s && ex.e != ea.s && ex.e != ea.e {
                let (lo, hi) = if den <= 0.0 { (den, 0.0f32) } else { (0.0f32, den) };
                let p1 = self.p[ex.s];
                let p2 = self.p[ea.s];
                let t = ea.dx * (p2.y - p1.y) - ea.dy * (p2.x - p1.x);
                if lo <= t && t <= hi {
                    let u = ex.dx * (p2.y - p1.y) - ex.dy * (p2.x - p1.x);
                    if lo <= u && u <= hi {
                        return true;
                    }
                }
            }
            false
        } else {
            let d = self.line_dist(x, ea.s);
            if d.abs() < f32::from_bits(EPS_DIST) {
                return self.overlap(x, a);
            }
            false
        }
    }
}

fn tri_metric(p1: P2, p2: P2, q: P2) -> f32 {
    let dx2 = q.x - p2.x;
    let dy2 = q.y - p2.y;
    let dz2 = 0.0f32;
    let dx1 = q.x - p1.x;
    let dy1 = q.y - p1.y;
    let dz1 = 0.0f32;
    ((dy2 * dy2 + dx2 * dx2) + dz2 * dz2) + ((dy1 * dy1 + dx1 * dx1) + dz1 * dz1)
}

fn triangulate_surface(t: &Template, s: &Surf) -> Option<Vec<[u32; 3]>> {
    let n = s.normal;
    let a0 = f32::from_bits(n[0].to_bits() & 0x7fffffff);
    let a1 = f32::from_bits(n[1].to_bits() & 0x7fffffff);
    let a2 = f32::from_bits(n[2].to_bits() & 0x7fffffff);
    let mut k = if a0 < a1 { 1 } else { 0 };
    let m = if a0 < a1 { a1 } else { a0 };
    if m < a2 {
        k = 2;
    }
    let (ua, va) = match k {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let neg = n[k] < 0.0;
    let flip = if k == 1 { !neg } else { neg };
    let mut pg = Poly2 { p: Vec::new(), e: Vec::new() };
    let mut map: HashMap<u16, usize> = HashMap::new();
    let mut get = |pg: &mut Poly2, idx: u16| -> usize {
        if let Some(&i) = map.get(&idx) {
            return i;
        }
        let tp = t.pts[idx as usize];
        pg.p.push(P2 { x: tp[ua], y: tp[va], idx: idx as u32 });
        map.insert(idx, pg.p.len() - 1);
        pg.p.len() - 1
    };
    let nl = s.lines.len();
    let mut list: Vec<usize> = Vec::new();
    for i in (0..nl).rev() {
        let line = t.lines[s.lines[i] as usize];
        let c = s.rev[i] as usize;
        let iv = if flip { 1 - c } else { c };
        let st = get(&mut pg, line[iv]);
        let en = get(&mut pg, line[1 - iv]);
        let e = pg.mk(st, en);
        list.insert(0, e);
    }
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut iter = 0i32;
    while !list.is_empty() {
        if 100 < iter {
            return None;
        }
        let e = list[0];
        let p1 = pg.e[e].s;
        let p2 = pg.e[e].e;
        let mut heap: Vec<(f32, usize, usize)> = Vec::new();
        for (ord, &x) in list.iter().enumerate() {
            let d = tri_metric(pg.p[p1], pg.p[p2], pg.p[pg.e[x].s]);
            heap.push((d, ord, x));
        }
        loop {
            let mut bi = usize::MAX;
            for (i, h) in heap.iter().enumerate() {
                if bi == usize::MAX || h.0 < heap[bi].0 || (h.0 == heap[bi].0 && h.1 < heap[bi].1) {
                    bi = i;
                }
            }
            if bi == usize::MAX {
                break;
            }
            let c = heap.remove(bi).2;
            let p4 = pg.e[c].s;
            if c == e || p4 == p2 || !pg.left(e, p4) {
                continue;
            }
            let ea = pg.mk(p4, p1);
            let eb = pg.mk(p2, p4);
            let mut bad = false;
            for &x in &list {
                if (!pg.same(x, p4, p1) && pg.crosses(x, ea)) || (!pg.same(x, p2, p4) && pg.crosses(x, eb)) {
                    bad = true;
                    break;
                }
            }
            if bad {
                continue;
            }
            let mut inside = false;
            for &x in &list {
                let q = pg.e[x].s;
                if pg.left(e, q) && pg.left(eb, q) && pg.left(ea, q) {
                    inside = true;
                    break;
                }
            }
            if inside {
                let ci = list.iter().position(|&x| x == c).unwrap();
                if ci + 1 == list.len() {
                    return None;
                }
                continue;
            }
            tris.push([pg.p[p1].idx, pg.p[p4].idx, pg.p[p2].idx]);
            list.remove(0);
            let mut b18 = false;
            let mut b7 = false;
            let mut i = 0;
            while i < list.len() {
                let x = list[i];
                if pg.same(x, p4, p1) {
                    list.remove(i);
                    b18 = true;
                    continue;
                }
                if pg.same(x, p4, p2) {
                    list.remove(i);
                    b7 = true;
                    continue;
                }
                i += 1;
            }
            if !b18 {
                let ne = pg.mk(p1, p4);
                list.insert(0, ne);
            }
            if !b7 {
                let ne = pg.mk(p4, p2);
                list.insert(0, ne);
            }
            break;
        }
        iter += 1;
    }
    Some(tris)
}

struct Tri {
    v: [usize; 3],
    opp: [(usize, usize); 3],
    n: [f32; 3],
    pierce: Option<usize>,
}

pub fn plane_from_points(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 4] {
    let n = plane_raw(a, b, c);
    normalize_plane(n)
}

fn plane_raw(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 4] {
    let cy = c[1] - a[1];
    let by = b[1] - a[1];
    let bz = b[2] - a[2];
    let bx = b[0] - a[0];
    let cz = c[2] - a[2];
    let cx = c[0] - a[0];
    let x = cy * bz - cz * by;
    let z = cx * by - cy * bx;
    let y = cz * bx - cx * bz;
    let w = -((y * a[1] + x * a[0]) + z * a[2]);
    [x, y, z, w]
}

fn normalize_plane(p: [f32; 4]) -> [f32; 4] {
    let inv = 1.0f32 / ((p[0] * p[0] + p[1] * p[1]) + p[2] * p[2]).sqrt();
    [p[0] * inv, p[1] * inv, p[2] * inv, inv * p[3]]
}

fn template_to_ledge(t: &Template) -> Option<Vec<u8>> {
    let mut created: Vec<[usize; 3]> = Vec::new();
    for s in &t.surfs {
        let tris = triangulate_surface(t, s)?;
        for tr in tris.iter().rev() {
            let (i0, i1, i2) = (tr[0] as usize, tr[1] as usize, tr[2] as usize);
            let pa = t.pts[i0];
            let pb = t.pts[i1];
            let pc = t.pts[i2];
            let l78 = sub(pb, pc);
            let l88 = sub(pa, pc);
            let nn = cross(l78, l88);
            let sn = s.normal;
            let d = (nn[1] * sn[1] + nn[0] * sn[0]) + nn[2] * sn[2];
            let (e1, e2) = if d < 0.0 || d.is_nan() { (i0, i1) } else { (i1, i0) };
            created.push([i2, e1, e2]);
        }
    }
    let order: Vec<[usize; 3]> = created.into_iter().rev().collect();
    let mut tris: Vec<Tri> =
        order.iter().map(|v| Tri { v: *v, opp: [(usize::MAX, 0); 3], n: [0.0; 3], pierce: None }).collect();
    {
        let mut h: HashMap<(usize, usize), (usize, usize)> = HashMap::new();
        let nt = tris.len();
        for ci in 0..nt {
            let ti = nt - 1 - ci;
            for j in 0..3 {
                let a = tris[ti].v[j];
                let b = tris[ti].v[(j + 1) % 3];
                let key = if a < b { (a, b) } else { (b, a) };
                if let Some(&(ot, oj)) = h.get(&key) {
                    tris[ti].opp[j] = (ot, oj);
                    tris[ot].opp[oj] = (ti, j);
                } else {
                    h.insert(key, (ti, j));
                }
            }
        }
    }
    for tr in tris.iter_mut() {
        let p = plane_from_points(t.pts[tr.v[0]], t.pts[tr.v[2]], t.pts[tr.v[1]]);
        tr.n = [p[0], p[1], p[2]];
    }
    let init = f32::from_bits(0xb58637bd);
    for i in 0..tris.len() {
        if tris[i].pierce.is_some() {
            continue;
        }
        let mut best = init;
        let mut bi = None;
        for o in 0..tris.len() {
            let a = tris[o].n;
            let b = tris[i].n;
            let d = (a[1] * b[1] + b[0] * a[0]) + a[2] * b[2];
            if d < best {
                best = d;
                bi = Some(o);
            }
        }
        let bi = bi?;
        tris[i].pierce = Some(bi);
        tris[bi].pierce = Some(i);
    }
    let mut pidx: HashMap<usize, u16> = HashMap::new();
    let mut plist: Vec<usize> = Vec::new();
    for tr in &tris {
        for j in 0..3 {
            let v = tr.v[j];
            if let std::collections::hash_map::Entry::Vacant(e) = pidx.entry(v) {
                e.insert(plist.len() as u16);
                plist.push(v);
            }
        }
    }
    let nt = tris.len();
    let size = (nt + 1 + plist.len()) * 16;
    let mut out = vec![0u8; size];
    let po = (16 + 16 * nt) as i32;
    out[0..4].copy_from_slice(&po.to_le_bytes());
    let flags: u32 = (((size >> 4) as u32) << 8) | 4;
    out[8..12].copy_from_slice(&flags.to_le_bytes());
    out[12..14].copy_from_slice(&(nt as i16).to_le_bytes());
    for (ti, tr) in tris.iter().enumerate() {
        let b = 16 + 16 * ti;
        let pierce = tr.pierce.unwrap_or(0) as u32;
        let w0: u32 = (ti as u32 & 0xfff) | ((pierce & 0xfff) << 12);
        out[b..b + 4].copy_from_slice(&w0.to_le_bytes());
        for j in 0..3 {
            let (ot, oj) = tr.opp[j];
            let mine = (4 * ti + j + 1) as i64;
            let other = if ot == usize::MAX { 0 } else { (4 * ot + oj + 1) as i64 };
            let off = (other - mine) as u32;
            let ev: u32 = (pidx[&tr.v[j]] as u32) | ((off & 0x7fff) << 16);
            out[b + 4 + 4 * j..b + 8 + 4 * j].copy_from_slice(&ev.to_le_bytes());
        }
    }
    for (i, &v) in plist.iter().enumerate() {
        let b = po as usize + 16 * i;
        let p = t.pts[v];
        for k in 0..3 {
            out[b + 4 * k..b + 4 * k + 4].copy_from_slice(&p[k].to_le_bytes());
        }
    }
    Some(out)
}

pub fn ledge_rebuild_planes(ledge: &[u8]) -> Vec<[f32; 4]> {
    let rd = |o: usize| u32::from_le_bytes([ledge[o], ledge[o + 1], ledge[o + 2], ledge[o + 3]]);
    let nt = i16::from_le_bytes([ledge[12], ledge[13]]) as usize;
    let po = rd(0) as i32;
    let mut res = Vec::new();
    for t in 0..nt {
        let tb = 16 + 16 * t;
        let pt = |j: usize| -> [f32; 3] {
            let idx = (rd(tb + 4 + 4 * j) & 0xffff) as usize;
            let o = po as usize + 16 * idx;
            [f32::from_bits(rd(o)), f32::from_bits(rd(o + 4)), f32::from_bits(rd(o + 8))]
        };
        let a = pt(0);
        let b = pt(1);
        let c = pt(2);
        let raw = plane_raw(a, b, c);
        let len = ((raw[0] * raw[0] + raw[1] * raw[1]) + raw[2] * raw[2]).sqrt();
        if f32::from_bits(0x358637bd) < len {
            res.push(normalize_plane(raw));
        }
    }
    res
}

pub fn convex_from_verts(
    src_points: &[[f32; 3]],
    qhull: &mut QhullFn,
    halfspace: &mut HalfspaceFn<'_>,
) -> Option<Vec<u8>> {
    let ivp: Vec<[f32; 3]> = src_points.iter().map(|p| source_to_ivp(*p)).collect();
    let first = pointsoup_to_ledge(&ivp, qhull)?;
    let planes = ledge_rebuild_planes(&first);
    match halfspace(&planes, f32::from_bits(0x3c23d70a), qhull) {
        Some(l) => Some(l),
        None => Some(first),
    }
}

pub fn convex_from_planes(planes: &[[f32; 4]], merge: f32, qhull: &mut QhullFn) -> Option<Vec<u8>> {
    let mut hs: Vec<[f32; 4]> = Vec::with_capacity(planes.len());
    for p in planes {
        add_halfspace(&mut hs, [-p[0], p[2], -p[1], -((-p[3]) * IVP_SCALE)]);
    }
    pointsoup_to_ledge(&halfspace_corners(&hs, IVP_SCALE * merge), qhull)
}

pub fn planes_corners(planes: &[[f32; 4]], merge: f32) -> Vec<[f32; 3]> {
    let mut hs: Vec<[f32; 4]> = Vec::with_capacity(planes.len());
    for p in planes {
        add_halfspace(&mut hs, [-p[0], p[2], -p[1], -((-p[3]) * IVP_SCALE)]);
    }
    halfspace_corners(&hs, IVP_SCALE * merge)
}

fn add_halfspace(list: &mut Vec<[f32; 4]>, h: [f32; 4]) {
    let thr = f32::from_bits(0x3f7f_f972);
    let mut remove: Vec<usize> = Vec::new();
    for (i, e) in list.iter().enumerate() {
        let d = (e[1] * h[1] + e[0] * h[0]) + e[2] * h[2];
        if d > thr {
            if h[3] > e[3] {
                return;
            }
            remove.push(i);
        }
    }
    list.push(h);
    for &i in remove.iter().rev() {
        list.remove(i);
    }
}

#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn intersect3(a: &[f32; 4], b: &[f32; 4], c: &[f32; 4]) -> Option<[f32; 3]> {
    let (m0, m1, m2) = (a[0], a[1], a[2]);
    let (m4, m5, m6) = (b[0], b[1], b[2]);
    let (m8, m9, m10) = (c[0], c[1], c[2]);
    let c00 = m10 * m5 - m9 * m6;
    let c10 = m8 * m6 - m4 * m10;
    let c20 = m4 * m9 - m8 * m5;
    let c01 = m2 * m9 - m1 * m10;
    let c11 = m0 * m10 - m2 * m8;
    let c21 = m1 * m8 - m0 * m9;
    let c02 = m1 * m6 - m2 * m5;
    let c12 = m2 * m4 - m0 * m6;
    let c22 = m0 * m5 - m1 * m4;
    let det = (m1 * c10 + m0 * c00) + m2 * c20;
    if !(det.abs() >= f32::from_bits(0x2edb_e6ff)) {
        return None;
    }
    let s = 1.0f32 / det;
    let (wa, wb, wc) = (-a[3], -b[3], -c[3]);
    Some([
        ((c00 * s) * wa + (c01 * s) * wb) + (c02 * s) * wc,
        ((c10 * s) * wa + (c11 * s) * wb) + (c12 * s) * wc,
        ((c20 * s) * wa + (c21 * s) * wb) + (c22 * s) * wc,
    ])
}

#[inline]
fn dist2(p: &[f32; 3], q: &[f32; 3]) -> f32 {
    let dx = p[0] - q[0];
    let dy = p[1] - q[1];
    let dz = p[2] - q[2];
    (dy * dy + dx * dx) + dz * dz
}

fn halfspace_corners(hs: &[[f32; 4]], merge: f32) -> Vec<[f32; 3]> {
    let inside = f32::from_bits(0xb8d1_b717);
    let m2 = merge * merge;
    let n = hs.len();
    let mut pts: Vec<[f32; 3]> = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let Some(p) = intersect3(&hs[i], &hs[j], &hs[k]) else { continue };
                if !hs.iter().all(|h| ((p[1] * h[1] + p[0] * h[0]) + p[2] * h[2]) + h[3] >= inside) {
                    continue;
                }
                if !pts.iter().any(|q| dist2(&p, q) < m2) {
                    pts.push(p);
                }
            }
        }
    }
    let mut i = 0;
    while i < pts.len() {
        let mut l = pts.len() - 1;
        while l > i {
            if dist2(&pts[i], &pts[l]) < m2 {
                pts.remove(l);
            }
            l -= 1;
        }
        i += 1;
    }
    pts
}

pub fn ivp_qhull(points: &[[f64; 3]], joggle: Option<f64>) -> Result<Vec<QhFacet>, i32> {
    match joggle {
        None => super::qhull::qhull_ivp(points),
        Some(j) => super::qhull::qhull_ivp_joggle(points, j),
    }
}
