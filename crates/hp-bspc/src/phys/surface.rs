const K_SRC2IVP: f32 = 0.0254;
const K_IVP2SRC: f32 = 39.370_08;

pub type HullLedgeFn<'a> = dyn FnMut(&[[f32; 3]]) -> Option<Vec<u8>> + 'a;

#[derive(Clone, Debug)]
pub struct ConvertParams {
    pub build_outer_hull: bool,
    pub build_drag_axis_areas: bool,
    pub drag_area_epsilon: f32,
    pub fast_inertia: bool,
}

impl Default for ConvertParams {
    fn default() -> Self {
        ConvertParams {
            build_outer_hull: false,
            build_drag_axis_areas: false,
            drag_area_epsilon: 0.25,
            fast_inertia: false,
        }
    }
}

impl ConvertParams {
    pub fn vbsp_brush_entity(convex_count: usize, size: [f32; 3]) -> Self {
        let mut min_area = -1.0f32;
        for i in 0..3 {
            let a = size[(i + 1) % 3] * size[(i + 2) % 3];
            if min_area < 0.0 || a < min_area {
                min_area = a;
            }
        }
        let eps = (min_area * 1e-2f32).clamp(1.0, 1024.0);
        ConvertParams {
            build_outer_hull: convex_count > 1,
            build_drag_axis_areas: true,
            drag_area_epsilon: eps,
            fast_inertia: false,
        }
    }
}

#[inline]
fn ru32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
#[inline]
fn ri32(b: &[u8], o: usize) -> i32 {
    ru32(b, o) as i32
}
#[inline]
fn rf32(b: &[u8], o: usize) -> f32 {
    f32::from_bits(ru32(b, o))
}
#[inline]
fn ru16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
#[inline]
fn wu32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
#[inline]
fn wi32(b: &mut [u8], o: usize, v: i32) {
    wu32(b, o, v as u32)
}
#[inline]
fn wf32(b: &mut [u8], o: usize, v: f32) {
    wu32(b, o, v.to_bits())
}
#[inline]
fn wu16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}

#[inline]
fn cvtt(f: f32) -> i32 {
    if f.is_nan() || !(-2147483648.0..2147483648.0).contains(&f) { i32::MIN } else { f as i32 }
}

#[inline]
fn len3(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

#[derive(Clone, Copy)]
struct LedgeRef<'a> {
    b: &'a [u8],
    o: usize,
}

impl<'a> LedgeRef<'a> {
    fn n_tris(&self) -> usize {
        let n = ru16(self.b, self.o + 12) as i16;
        if n < 0 { 0 } else { n as usize }
    }
    fn point_base(&self) -> isize {
        self.o as isize + ri32(self.b, self.o) as isize
    }
    fn sp(&self, t: usize, e: usize) -> usize {
        ru16(self.b, self.o + 16 + t * 16 + 4 + e * 4) as usize
    }
    fn pt_off(&self, idx: usize) -> usize {
        (self.point_base() + (idx as isize) * 16) as usize
    }
    fn pt(&self, idx: usize) -> [f32; 3] {
        let p = self.pt_off(idx);
        [rf32(self.b, p), rf32(self.b, p + 4), rf32(self.b, p + 8)]
    }
    fn pt16(&self, idx: usize) -> [u8; 16] {
        let p = self.pt_off(idx);
        let mut r = [0u8; 16];
        r.copy_from_slice(&self.b[p..p + 16]);
        r
    }
}

fn ledge_bbox(l: LedgeRef) -> ([f32; 3], [f32; 3]) {
    let n = l.n_tris();
    let p = l.pt(l.sp(0, 0));
    let mut mn = p;
    let mut mx = p;
    for t in 0..n {
        for e in 0..3 {
            let v = l.pt(l.sp(t, e));
            for k in 0..3 {
                if mn[k] <= v[k] {
                    if mx[k] < v[k] {
                        mx[k] = v[k];
                    }
                } else {
                    mn[k] = v[k];
                }
            }
        }
    }
    (mn, mx)
}

const NONE: usize = usize::MAX;

#[derive(Clone)]
struct Node {
    c: [f32; 3],
    r: f32,
    bx: [u8; 3],
    ledge: usize,
    left: usize,
    right: usize,
}

fn leaf_node(l: LedgeRef, ledge: usize) -> Node {
    let (mn, mx) = ledge_bbox(l);
    let mut c = [0f32; 3];
    for k in 0..3 {
        c[k] = 0.5 * mx[k] + 0.5 * mn[k];
    }
    let r = len3([mx[0] - c[0], mx[1] - c[1], mx[2] - c[2]]);
    let f = f32::from_bits(0x4379ffff) / r;
    let mut bx = [0u8; 3];
    for k in 0..3 {
        bx[k] = (cvtt((mx[k] - c[k]) * f) as u8).wrapping_add(1);
    }
    Node { c, r, bx, ledge, left: NONE, right: NONE }
}

fn list_bbox(nodes: &[Node], list: &[usize]) -> ([f32; 3], [f32; 3]) {
    let mut mn = [1e6f32; 3];
    let mut mx = [-1e6f32; 3];
    for &i in list {
        let n = &nodes[i];
        let h = 0.004f32 * n.r;
        for k in 0..3 {
            let hk = (n.bx[k] as f32) * h;
            let lo = n.c[k] - hk;
            let hi = n.c[k] + hk;
            if lo < mn[k] {
                mn[k] = lo;
            }
            if hi > mx[k] {
                mx[k] = hi;
            }
        }
    }
    (mn, mx)
}

#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn build_tree(nodes: &mut Vec<Node>, list: &[usize]) -> usize {
    if list.len() == 1 {
        return list[0];
    }
    let (mn, mx) = list_bbox(nodes, list);
    let mut c = [0f32; 3];
    for k in 0..3 {
        c[k] = 0.5 * mx[k] + 0.5 * mn[k];
    }
    let r = len3([mx[0] - c[0], mx[1] - c[1], mx[2] - c[2]]);
    let f = -1.0f32 / (0.004f32 * r);
    let mut bx = [0u8; 3];
    for k in 0..3 {
        bx[k] = 1u8.wrapping_sub(cvtt((mx[k] - c[k]) * f) as u8);
    }
    let me = nodes.len();
    nodes.push(Node { c, r, bx, ledge: NONE, left: NONE, right: NONE });
    let (l, rr) = if list.len() == 2 {
        (vec![list[0]], vec![list[1]])
    } else {
        let n = list.len();
        let eps = f32::from_bits(0x358637bd);
        let mut parts: Vec<(Vec<usize>, Vec<usize>)> = Vec::new();
        let mut cost = [0f32; 3];
        for (ax, cost_ax) in cost.iter_mut().enumerate() {
            let mut sum = 0f32;
            for &i in list {
                sum += nodes[i].c[ax];
            }
            let avg = sum / (n as f32);
            let lo = avg - eps;
            let hi = avg + eps;
            let mut left: Vec<usize> = Vec::new();
            let mut right: Vec<usize> = Vec::new();
            let mut flag = true;
            for (idx, &i) in list.iter().enumerate() {
                let v = nodes[i].c[ax];
                if !(lo <= v) {
                    left.push(i);
                } else if hi < v {
                    right.push(i);
                } else {
                    let j = if idx == n - 1 { list[idx - 1] } else { list[idx + 1] };
                    let w = nodes[j].c[ax];
                    if w < lo {
                        right.push(i);
                    } else if hi < w {
                        left.push(i);
                    } else if !left.is_empty() {
                        if right.is_empty() {
                            right.push(i);
                        } else if flag {
                            left.push(i);
                            flag = false;
                        } else {
                            right.push(i);
                            flag = true;
                        }
                    } else {
                        left.push(i);
                    }
                }
            }
            let (a0, a1) = list_bbox(nodes, &left);
            let (b0, b1) = list_bbox(nodes, &right);
            let va = (a1[1] - a0[1]).abs() * (a1[0] - a0[0]).abs() * (a1[2] - a0[2]).abs();
            let vb = (b1[1] - b0[1]).abs() * (b1[0] - b0[0]).abs() * (b1[2] - b0[2]).abs();
            *cost_ax = va + vb;
            parts.push((left, right));
        }
        let mut best = f32::from_bits(0x60ad78ec);
        if cost[0] < best {
            best = cost[0];
        }
        let mut ax = 0usize;
        if cost[1] < best {
            best = cost[1];
            if !parts[1].0.is_empty() && !parts[1].1.is_empty() {
                ax = 1;
            }
        }
        if cost[2] < best && !parts[2].0.is_empty() && !parts[2].1.is_empty() {
            ax = 2;
        }
        let p = parts.swap_remove(ax);
        (p.0, p.1)
    };
    let a = build_tree(nodes, &l);
    let b = build_tree(nodes, &rr);
    nodes[me].left = a;
    nodes[me].right = b;
    me
}

fn collect_ledges(nodes: &[Node], mut n: usize, out: &mut Vec<usize>) {
    while n != NONE {
        if nodes[n].ledge != NONE {
            out.push(nodes[n].ledge);
            return;
        }
        if nodes[n].left != NONE {
            collect_ledges(nodes, nodes[n].left, out);
        }
        n = nodes[n].right;
    }
}

const TRI_TEMPLATE: [u8; 48] = [
    0x30, 0, 0, 0, 0, 0, 0, 0, 0x04, 0x06, 0, 0, 0x02, 0, 0, 0, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x01,
    0x00, 0x05, 0x00, 0x02, 0x00, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0xfc, 0x7f, 0x00, 0x00, 0xfd, 0x7f,
    0x02, 0x00, 0xfb, 0x7f,
];

#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn triangle_ledge(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Option<Vec<u8>> {
    let f3 = (c[1] - a[1]) * (b[2] - a[2]) - (c[2] - a[2]) * (b[1] - a[1]);
    let f5 = (c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0]);
    let f4 = (c[2] - a[2]) * (b[0] - a[0]) - (c[0] - a[0]) * (b[2] - a[2]);
    if !(f32::from_bits(0x2b8cbccc) <= f4 * f4 + f3 * f3 + f5 * f5) {
        return None;
    }
    let mut v = vec![0u8; 96];
    v[..48].copy_from_slice(&TRI_TEMPLATE);
    for (i, p) in [a, b, c].iter().enumerate() {
        for (k, &pk) in p.iter().enumerate() {
            wf32(&mut v, 48 + i * 16 + k * 4, pk);
        }
    }
    Some(v)
}

fn pointsoup_to_ledge(pts: &[[f32; 3]], hull: &mut HullLedgeFn<'_>) -> Option<Vec<u8>> {
    if pts.len() < 3 {
        None
    } else if pts.len() == 3 {
        triangle_ledge(pts[0], pts[1], pts[2])
    } else {
        hull(pts)
    }
}

pub fn hull_points(ledges: &[&[u8]]) -> Vec<[f32; 3]> {
    let mut seen: std::collections::HashSet<[u8; 16]> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for li in (0..ledges.len()).rev() {
        let l = LedgeRef { b: ledges[li], o: 0 };
        for t in 0..l.n_tris() {
            for e in 0..3 {
                let idx = l.sp(t, e);
                let key = l.pt16(idx);
                if seen.insert(key) {
                    out.push(l.pt(idx));
                }
            }
        }
    }
    out
}

fn mark_virtual(ledges: &[&[u8]], h: &mut Vec<u8>) {
    use std::collections::{HashMap, HashSet};
    let mut ids: HashMap<[u8; 16], i32> = HashMap::new();
    let mut tris: HashSet<[i32; 3]> = HashSet::new();
    let mut edges: HashSet<[i32; 2]> = HashSet::new();
    let mut next = 0i32;
    for li in (0..ledges.len()).rev() {
        let l = LedgeRef { b: ledges[li], o: 0 };
        for t in 0..l.n_tris() {
            let mut id = [0i32; 3];
            for (e, id_e) in id.iter_mut().enumerate() {
                let key = l.pt16(l.sp(t, e));
                *id_e = *ids.entry(key).or_insert_with(|| {
                    next += 1;
                    next - 1
                });
            }
            tris.insert(id);
            for e in 0..3 {
                edges.insert([id[e], id[(e + 1) % 3]]);
            }
        }
    }
    let n = LedgeRef { b: h, o: 0 }.n_tris();
    for t in 0..n {
        let mut id = [0i32; 3];
        {
            let l = LedgeRef { b: h, o: 0 };
            for (e, id_e) in id.iter_mut().enumerate() {
                *id_e = *ids.get(&l.pt16(l.sp(t, e))).unwrap_or(&-1);
            }
        }
        let to = 16 + t * 16;
        if !tris.contains(&id) {
            let v = ru32(h, to) | 0x8000_0000;
            wu32(h, to, v);
        }
        for e in 0..3 {
            if !edges.contains(&[id[e], id[(e + 1) % 3]]) {
                let o = to + 4 + e * 4;
                let v = ru32(h, o) | 0x8000_0000;
                wu32(h, o, v);
            }
        }
    }
}

#[derive(Default)]
struct VolAcc {
    s: [f32; 3],
    area: f32,
    vol: f32,
}

fn vol_tri(acc: &mut VolAcc, l: LedgeRef, t: usize) {
    let p0 = l.pt(l.sp(t, 0));
    let p1 = l.pt(l.sp(t, 1));
    let p2 = l.pt(l.sp(t, 2));
    let nx = (p1[1] - p0[1]) * (p2[2] - p0[2]) - (p1[2] - p0[2]) * (p2[1] - p0[1]);
    let nz = (p1[0] - p0[0]) * (p2[1] - p0[1]) - (p1[1] - p0[1]) * (p2[0] - p0[0]);
    let ny = (p1[2] - p0[2]) * (p2[0] - p0[0]) - (p1[0] - p0[0]) * (p2[2] - p0[2]);
    let d = p0[0] * nx + p0[1] * ny + p0[2] * nz;
    let w = d * 0.25f32;
    acc.vol += d;
    acc.area += ny * ny + nx * nx + nz * nz;
    for p in [p0, p1, p2] {
        for (s, &pk) in acc.s.iter_mut().zip(p.iter()) {
            *s += w * pk;
        }
    }
}

#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn inertia_tri(acc: &mut [f64; 3], l: LedgeRef, t: usize, mc: [f32; 3], a: usize, b: usize, c: usize) {
    let p0 = l.pt(l.sp(t, 0));
    let p1 = l.pt(l.sp(t, 1));
    let p2 = l.pt(l.sp(t, 2));
    let d1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let d2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    let mut n = [d1[1] * d2[2] - d1[2] * d2[1], d1[2] * d2[0] - d1[0] * d2[2], d1[0] * d2[1] - d1[1] * d2[0]];
    let q = n[0] * n[0] + n[1] * n[1] + n[2] * n[2];
    if !(q < f32::from_bits(0x2edbe6ff)) {
        let inv = 1.0f32 / q.sqrt();
        n = [n[0] * inv, n[1] * inv, n[2] * inv];
    }
    let eps = f32::from_bits(0x358637bd);
    let nc = n[c];
    let nb = n[b];
    let (d8, d10, flag) = if nb.abs() <= nc.abs() {
        let d8 = if nc.abs() <= eps { 0.0f64 } else { ((nb * -0.5f32) / nc) as f64 };
        (d8, 0.0f64, true)
    } else {
        (0.0f64, ((nc * 0.5f32) / nb) as f64, false)
    };
    let tr = |p: [f32; 3]| -> [f32; 3] {
        let x = p[0] - mc[0];
        let z = p[2] - mc[2];
        let y = p[1] - mc[1];
        [y * 0.0 + x * 1.0 + z * 0.0, y * 1.0 + x * 0.0 + z * 0.0, y * 0.0 + x * 0.0 + z * 1.0]
    };
    let mut s1 = 0.0f64;
    let mut s3 = 0.0f64;
    let mut s2 = 0.0f64;
    for e in 0..3 {
        let pa = tr(l.pt(l.sp(t, e)));
        let pb = tr(l.pt(l.sp(t, (e + 1) % 3)));
        let dl = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let len = len3(dl);
        if len * eps <= dl[a].abs() {
            let fa = pa[a];
            let d20 = ((1.0f32 / dl[a]) * dl[b]) as f64;
            let d17 = pa[b] as f64 - (fa as f64) * d20;
            let (d6, d7, d20) = if flag {
                (d17 * d8 * d17, d20 * d8 * d20, (d8 + d8) * d17 * d20)
            } else {
                let d18 = ((1.0f32 / dl[a]) * dl[c]) as f64;
                let d21 = pa[c] as f64 - (fa as f64) * d18;
                let d7 = (d18 * d10 + d20) * d18;
                let d6 = (d21 * d10 + d17) * d21;
                let d20n = d18 * d17 + d21 * d20 + (d10 + d10) * d21 * d18;
                (d6, d7, d20n)
            };
            let d4 = fa as f64;
            let d17 = pb[a] as f64;
            let d22 = d17 * d17 * d17;
            let d21 = d4 * d4 * d4;
            let d5 = d22 * d17;
            let d19 = (d17 * d17 - d4 * d4) * 0.5;
            let d18 = d21 * d4;
            let d22 = (d22 - d21) * (1.0f64 / 3.0);
            let d21 = (d5 - d18) * 0.25;
            s1 += ((d17 - d4) * d6 + d19 * d20) + d22 * d7;
            s3 += (d5 * d17 - d18 * d4) * f64::from(0.2f32) * d7 + (d21 * d20 + d22 * d6);
            s2 += (d22 * d20 + d19 * d6) + d21 * d7;
        }
    }
    acc[0] += s1;
    acc[2] += s3;
    acc[1] += s2;
}

fn mass_props(ls: &[LedgeRef], fast: bool) -> ([f32; 3], [f32; 3]) {
    let bbox_approx = |ls: &[LedgeRef]| -> ([f32; 3], [f32; 3]) {
        let (mut mn, mut mx) = ledge_bbox(ls[0]);
        let mut i = ls.len() as isize - 1;
        while i > 0 {
            let (a, b) = ledge_bbox(ls[i as usize]);
            for k in [2usize, 1, 0] {
                if a[k] < mn[k] {
                    mn[k] = a[k];
                }
            }
            for k in [2usize, 1, 0] {
                if b[k] > mx[k] {
                    mx[k] = b[k];
                }
            }
            i -= 1;
        }
        let mut c = [0f32; 3];
        for k in 0..3 {
            c[k] = 0.5 * mn[k] + 0.5 * mx[k];
        }
        let dy = mn[1] - mx[1];
        let dx = mn[0] - mx[0];
        let dz = mn[2] - mx[2];
        let mut r = (dy * dy + dx * dx + dz * dz).sqrt() * 0.5f32;
        r = r * r * 0.5f32;
        (c, [r, r, r])
    };
    if fast {
        return bbox_approx(ls);
    }
    let mut acc = VolAcc::default();
    for l in ls.iter().rev() {
        for t in 0..l.n_tris() {
            vol_tri(&mut acc, *l, t);
        }
    }
    let sq = acc.area.sqrt();
    if acc.vol <= sq * sq * sq * f32::from_bits(0x3089705f) {
        return bbox_approx(ls);
    }
    let inv = 1.0f32 / acc.vol;
    let mc = [acc.s[0] * inv, acc.s[1] * inv, acc.s[2] * inv];
    let mut q = [0f32; 3];
    for (slot, (a, b, c)) in [(0usize, 1usize, 2usize), (1, 2, 0), (2, 0, 1)].iter().enumerate() {
        let mut s = [0f64; 3];
        for l in ls.iter().rev() {
            for t in 0..l.n_tris() {
                inertia_tri(&mut s, *l, t, mc, *a, *b, *c);
            }
        }
        if f64::from(f32::from_bits(0x2edbe6ff)) <= s[0] {
            let d5 = 1.0f64 / s[0];
            q[slot] = (d5 * s[2]) as f32;
        } else {
            q[slot] = 1.0;
        }
    }
    let ri =
        [(q[2] * q[2] + q[1] * q[1]).sqrt(), (q[2] * q[2] + q[0] * q[0]).sqrt(), (q[1] * q[1] + q[0] * q[0]).sqrt()];
    (mc, ri)
}

#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn radius_dev(ls: &[LedgeRef], mc: [f32; 3]) -> (f32, f32) {
    let mut rad = 0f32;
    let mut dev = 0f32;
    for l in ls.iter().rev() {
        let mut md = 0f32;
        let mut mv = 0f32;
        for t in 0..l.n_tris() {
            let p0 = l.pt(l.sp(t, 0));
            let p1 = l.pt(l.sp(t, 1));
            let p2 = l.pt(l.sp(t, 2));
            let a = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let b = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let nx = a[1] * b[2] - a[2] * b[1];
            let nz = a[0] * b[1] - a[1] * b[0];
            let ny = a[2] * b[0] - a[0] * b[2];
            let inv = 1.0f32 / (ny * ny + nx * nx + nz * nz);
            for v in [p0, p1, p2] {
                let dx = v[0] - mc[0];
                let dy = v[1] - mc[1];
                let dz = v[2] - mc[2];
                let d2 = dy * dy + dx * dx + dz * dz;
                if !(d2 <= md) {
                    md = d2;
                }
                let c1 = dy * nz - dz * ny;
                let c2 = dx * ny - dy * nx;
                let c3 = dz * nx - dx * nz;
                let dv = (c3 * c3 + c1 * c1 + c2 * c2) * inv;
                if !(dv <= mv) {
                    mv = dv;
                }
            }
        }
        let r = md.sqrt();
        if r > rad {
            rad = r;
        }
        let d = mv.sqrt();
        if d > dev {
            dev = d;
        }
    }
    (rad, dev)
}

fn compile(input: Vec<Vec<u8>>, outer_hull: bool, fast: bool, hull: &mut HullLedgeFn<'_>) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return None;
    }
    let mut arena: Vec<Vec<u8>> = input;
    let mut nodes: Vec<Node> = Vec::with_capacity(2 * n);
    for (i, b) in arena.iter().enumerate() {
        let nd = leaf_node(LedgeRef { b, o: 0 }, i);
        nodes.push(nd);
    }
    let list: Vec<usize> = (0..n).collect();
    let root = build_tree(&mut nodes, &list);
    let mut hull_ledges: Vec<usize> = Vec::new();
    if outer_hull && n > 1 {
        let mut ls = Vec::new();
        collect_ledges(&nodes, root, &mut ls);
        let refs: Vec<&[u8]> = ls.iter().map(|&i| arena[i].as_slice()).collect();
        let pts = hull_points(&refs);
        let mut h = pointsoup_to_ledge(&pts, hull)?;
        mark_virtual(&refs, &mut h);
        arena.push(h);
        let hi = arena.len() - 1;
        nodes[root].ledge = hi;
        hull_ledges.push(hi);
    }
    let order: Vec<usize> = (0..n).chain(hull_ledges.iter().cloned()).collect();
    let total = order.len();
    let pooled = total >= 2;
    let nnodes = nodes.len();
    let mut ntris = 0usize;
    let mut sum_size = 0usize;
    for &i in &order {
        ntris += LedgeRef { b: &arena[i], o: 0 }.n_tris();
        sum_size += ((ru32(&arena[i], 8) >> 8) as usize) * 16;
    }
    let mut out: Vec<u8> = vec![0; 48];
    let mut ledge_off = vec![0usize; arena.len()];
    if pooled {
        let pool_base = 48 + (ntris + total) * 16;
        let mut pool: Vec<u8> = Vec::new();
        let mut map: std::collections::HashMap<[u8; 12], u16> = std::collections::HashMap::new();
        let mut npts: usize = 0;
        for &i in &order {
            let src = &arena[i];
            let l = LedgeRef { b: src, o: 0 };
            let nt = l.n_tris();
            let off = out.len();
            ledge_off[i] = off;
            out.extend_from_slice(&src[..(nt + 1) * 16]);
            for t in 0..nt {
                for e in 0..3 {
                    let idx = l.sp(t, e);
                    let p16 = l.pt16(idx);
                    let mut key = [0u8; 12];
                    key.copy_from_slice(&p16[..12]);
                    let ni = match map.get(&key) {
                        Some(&v) => v,
                        None => {
                            let v = npts as u16;
                            npts += 1;
                            pool.extend_from_slice(&p16);
                            map.insert(key, v);
                            v
                        }
                    };
                    wu16(&mut out, off + 16 + t * 16 + 4 + e * 4, ni);
                }
            }
            wi32(&mut out, off, pool_base as i32 - off as i32);
        }
        debug_assert_eq!(out.len(), pool_base);
        out.extend_from_slice(&pool);
    } else {
        for &i in &order {
            let src = &arena[i];
            let sz = ((ru32(src, 8) >> 8) as usize) * 16;
            ledge_off[i] = out.len();
            out.extend_from_slice(&src[..sz]);
        }
        let _ = sum_size;
    }
    let node_base = out.len();
    out.resize(node_base + nnodes * 28, 0);
    let mut cursor = node_base;
    let mut node_off = vec![0usize; nnodes];
    fn wr(
        nodes: &[Node],
        ni: usize,
        out: &mut Vec<u8>,
        cursor: &mut usize,
        node_off: &mut [usize],
        ledge_off: &[usize],
    ) -> usize {
        let me = *cursor;
        *cursor += 28;
        node_off[ni] = me;
        let nd = &nodes[ni];
        for k in 0..3 {
            wf32(out, me + 8 + k * 4, nd.c[k]);
        }
        wf32(out, me + 20, nd.r);
        out[me + 24] = nd.bx[0];
        out[me + 25] = nd.bx[1];
        out[me + 26] = nd.bx[2];
        out[me + 27] = 0;
        if nd.left != NONE {
            if nd.ledge == NONE {
                wi32(out, me + 4, 0);
            } else {
                let lo = ledge_off[nd.ledge];
                wi32(out, me + 4, lo as i32 - me as i32);
                wi32(out, lo + 4, me as i32 - lo as i32);
                let f = (ru32(out, lo + 8) & 0xffff_fffd) | 1;
                wu32(out, lo + 8, f);
            }
            wr(nodes, nd.left, out, cursor, node_off, ledge_off);
            let r = wr(nodes, nd.right, out, cursor, node_off, ledge_off);
            wi32(out, me, r as i32 - me as i32);
        } else {
            let lo = ledge_off[nd.ledge];
            let f = ru32(out, lo + 8) & 0xffff_fffc;
            wu32(out, lo + 8, f);
            wi32(out, me, 0);
            wi32(out, me + 4, lo as i32 - me as i32);
        }
        me
    }
    wr(&nodes, root, &mut out, &mut cursor, &mut node_off, &ledge_off);
    let byte_size = out.len();
    wu32(&mut out, 28, (byte_size as u32) << 8);
    wi32(&mut out, 32, node_base as i32);
    wu32(&mut out, 36, 0);
    wu32(&mut out, 40, 0);
    wu32(&mut out, 44, 0x53505649);
    let leaves = surface_ledges(&out);
    let (mc, ri, ulr, dev) = {
        let refs: Vec<LedgeRef> = leaves.iter().map(|&o| LedgeRef { b: &out, o }).collect();
        let (mc, ri) = mass_props(&refs, fast);
        let (ulr, dev) = radius_dev(&refs, mc);
        (mc, ri, ulr, dev)
    };
    for k in 0..3 {
        wf32(&mut out, k * 4, mc[k]);
        wf32(&mut out, 12 + k * 4, ri[k]);
    }
    wf32(&mut out, 24, ulr);
    out[28] = cvtt(dev / (ulr * 0.004f32) + 1.0f32) as u8;
    if !leaves.is_empty() && leaves.len() < 0x41 {
        for (i, &lo) in leaves.iter().enumerate() {
            for (ni, nd) in nodes.iter().enumerate() {
                if nd.left == NONE && ledge_off[nd.ledge] == lo {
                    out[node_off[ni] + 27] = i as u8;
                }
            }
        }
    }
    Some(out)
}

pub fn surface_ledges(s: &[u8]) -> Vec<usize> {
    let root = ri32(s, 32) as usize;
    let mut out = Vec::new();
    fn rec(s: &[u8], mut n: usize, out: &mut Vec<usize>) {
        loop {
            let r = ri32(s, n);
            if r == 0 {
                break;
            }
            rec(s, n + 28, out);
            n = (n as isize + r as isize) as usize;
        }
        out.push((n as isize + ri32(s, n + 4) as isize) as usize);
    }
    let r = ri32(s, root);
    if r == 0 {
        out.push((root as isize + ri32(s, root + 4) as isize) as usize);
    } else {
        rec(s, root + 28, &mut out);
        rec(s, (root as isize + r as isize) as usize, &mut out);
    }
    out
}

fn vphy(surface: &[u8], drag: [f32; 3]) -> Vec<u8> {
    let mut v = Vec::with_capacity(28 + surface.len());
    v.extend_from_slice(b"VPHY");
    v.extend_from_slice(&0x100u32.to_le_bytes());
    v.extend_from_slice(&(surface.len() as u32).to_le_bytes());
    for d in drag {
        v.extend_from_slice(&d.to_bits().to_le_bytes());
    }
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(surface);
    v
}

pub fn convex_to_collide(ledges: &[Vec<u8>], params: &ConvertParams, hull: &mut HullLedgeFn<'_>) -> Option<Vec<u8>> {
    let s = compile(ledges.to_vec(), params.build_outer_hull, params.fast_inertia, hull)?;
    let mut drag = [1.0f32; 3];
    if params.build_drag_axis_areas {
        drag = drag_axis_areas(&s, params.drag_area_epsilon);
    }
    Some(vphy(&s, drag))
}

pub fn polysoup_to_collide(tris: &[([[f32; 3]; 3], i32)], hull: &mut HullLedgeFn<'_>) -> Option<Vec<u8>> {
    let cv = |p: [f32; 3]| [K_SRC2IVP * p[0], -(K_SRC2IVP * p[2]), K_SRC2IVP * p[1]];
    let ivp: Vec<([[f32; 3]; 3], i32)> = tris.iter().map(|(t, m)| (t.map(cv), *m)).collect();
    polysoup_ivp_to_collide(&ivp, hull)
}

pub fn polysoup_ivp_to_collide(tris: &[([[f32; 3]; 3], i32)], hull: &mut HullLedgeFn<'_>) -> Option<Vec<u8>> {
    let mut ledges = Vec::new();
    for (t, mat) in tris {
        if let Some(mut l) = triangle_ledge(t[0], t[1], t[2]) {
            let m = ((*mat as u32) & 0x7f) << 24;
            for i in 0..2 {
                let o = 16 + i * 16;
                let v = (ru32(&l, o) & 0x80ff_ffff) | m;
                wu32(&mut l, o, v);
            }
            ledges.push(l);
        }
    }
    if ledges.is_empty() {
        return None;
    }
    let s = compile(ledges, true, false, hull)?;
    Some(vphy(&s, [1.0; 3]))
}

fn surface_of(c: &[u8]) -> &[u8] {
    if c.len() >= 28 && &c[0..4] == b"VPHY" { &c[28..] } else { c }
}

#[inline]
fn to_src(p: [f32; 3]) -> [f32; 3] {
    [K_IVP2SRC * p[0], K_IVP2SRC * p[2], -(K_IVP2SRC * p[1])]
}

pub fn ledge_triangles(ledge: &[u8]) -> Vec<[[f32; 3]; 3]> {
    let l = LedgeRef { b: ledge, o: 0 };
    (0..l.n_tris()).map(|t| [0, 1, 2].map(|e| to_src(l.pt(l.sp(t, e))))).collect()
}

pub fn collide_triangles(c: &[u8]) -> Vec<[[f32; 3]; 3]> {
    let s = surface_of(c);
    if s.len() < 48 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for o in surface_ledges(s) {
        if o + 16 > s.len() {
            continue;
        }
        let l = LedgeRef { b: s, o };
        for t in 0..l.n_tris() {
            let ok = (0..3).all(|e| o + 16 + t * 16 + 16 <= s.len() && l.pt_off(l.sp(t, e)) + 12 <= s.len());
            if ok {
                out.push([0, 1, 2].map(|e| to_src(l.pt(l.sp(t, e)))));
            }
        }
    }
    out
}

fn ledge_volume(l: LedgeRef) -> f32 {
    let p0 = to_src(l.pt(l.sp(0, 0)));
    let mut v = 0f32;
    for t in 0..l.n_tris() {
        let a = to_src(l.pt(l.sp(t, 0)));
        let b = to_src(l.pt(l.sp(t, 1)));
        let c = to_src(l.pt(l.sp(t, 2)));
        let (x1, y1, z1) = (p0[0], p0[1], p0[2]);
        let f = (((c[0] - x1) * (b[2] - z1) - (c[2] - z1) * (b[0] - x1)) * (a[1] - y1)
            + ((c[2] - z1) * (b[1] - y1) - (c[1] - y1) * (b[2] - z1)) * (a[0] - x1)
            + ((c[1] - y1) * (b[0] - x1) - (c[0] - x1) * (b[1] - y1)) * (a[2] - z1))
            * f32::from_bits(0x3e2aaaab);
        v += f.abs();
    }
    v
}

fn ledge_area(l: LedgeRef) -> f32 {
    let mut s = 0f32;
    for t in 0..l.n_tris() {
        let a = to_src(l.pt(l.sp(t, 0)));
        let b = to_src(l.pt(l.sp(t, 1)));
        let c = to_src(l.pt(l.sp(t, 2)));
        let f1 = (c[0] - a[0]) * (b[2] - a[2]) - (c[2] - a[2]) * (b[0] - a[0]);
        let f2 = (c[2] - a[2]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[2] - a[2]);
        let f3 = (c[1] - a[1]) * (b[0] - a[0]) - (c[0] - a[0]) * (b[1] - a[1]);
        s += (f1 * f1 + f2 * f2 + f3 * f3).sqrt() * 0.5f32;
    }
    s
}

pub fn collide_volume(c: &[u8]) -> f32 {
    let s = surface_of(c);
    let mut v = 0f32;
    for o in surface_ledges(s) {
        v += ledge_volume(LedgeRef { b: s, o });
    }
    v
}

pub fn collide_surface_area(c: &[u8]) -> f32 {
    let s = surface_of(c);
    let mut v = 0f32;
    for o in surface_ledges(s) {
        v += ledge_area(LedgeRef { b: s, o });
    }
    v
}

pub fn convex_volume(ledge: &[u8]) -> f32 {
    ledge_volume(LedgeRef { b: ledge, o: 0 })
}

pub fn collide_mass_center(c: &[u8]) -> [f32; 3] {
    let s = surface_of(c);
    let m = [rf32(s, 0), rf32(s, 4), rf32(s, 8)];
    to_src(m)
}

pub fn collide_extent(c: &[u8], dir: [f32; 3]) -> [f32; 3] {
    extent_impl(surface_of(c), dir)
}

pub fn soup_surface(ledges: Vec<Vec<u8>>, hull: &mut HullLedgeFn<'_>) -> Option<Vec<u8>> {
    compile(ledges, true, false, hull)
}

include!("extent_drag.rs");
include!("gjk.rs");
include!("collide_trace.rs");
