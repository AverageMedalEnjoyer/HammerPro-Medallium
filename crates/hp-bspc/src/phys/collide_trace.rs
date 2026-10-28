fn angle_vectors(a: [f32; 3]) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let k = f32::from_bits(0x3c8e_fa35);
    let y = a[1] * k;
    let (sy, cy) = (y.sin(), y.cos());
    let p = a[0] * k;
    let (sp, cp) = (p.sin(), p.cos());
    let r = a[2] * k;
    let (sr, cr) = (r.sin(), r.cos());
    let f = [cp * cy, cp * sy, -sp];
    let srsp = sr * sp;
    let rt = [cr * sy - srsp * cy, -cr * cy - srsp * sy, -sr * cp];
    let crsp = cr * sp;
    let up = [crsp * cy + sr * sy, crsp * sy - sr * cy, cr * cp];
    (f, rt, up)
}

#[derive(Clone, Copy, Debug)]
pub struct Placement {
    r: [[f32; 3]; 3],
    vv: [f32; 3],
    m: [[f32; 4]; 3],
    moved: bool,
}

impl Placement {
    pub fn new(origin: [f32; 3], angles: [f32; 3]) -> Placement {
        let (f, rt, up) = angle_vectors(angles);
        let r = [[f[0], -up[0], -rt[0]], [-f[2], -(-up[2]), -(-rt[2])], [f[1], -up[1], -rt[1]]];
        let k2 = K_SRC2IVP;
        let vv = [k2 * origin[0], -(k2 * origin[2]), k2 * origin[1]];
        let k = K_IVP2SRC;
        let nk = -k;
        let m = [
            [k * r[0][0], k * r[0][1], k * r[0][2], k * vv[0]],
            [k * r[2][0], k * r[2][1], k * r[2][2], k * vv[2]],
            [nk * r[1][0], nk * r[1][1], nk * r[1][2], nk * vv[1]],
        ];
        let moved = !(origin[0] == 0.0 && origin[1] == 0.0 && origin[2] == 0.0);
        Placement { r, vv, m, moved }
    }

    #[inline]
    fn rotate(&self, p: [f32; 3]) -> [f32; 3] {
        let m = &self.m;
        [
            (p[0] * m[0][0] + p[1] * m[0][1]) + p[2] * m[0][2],
            (p[0] * m[1][0] + p[1] * m[1][1]) + p[2] * m[1][2],
            (p[0] * m[2][0] + p[1] * m[2][1]) + p[2] * m[2][2],
        ]
    }

    #[inline]
    fn point(&self, p: [f32; 3]) -> [f32; 3] {
        let r = self.rotate(p);
        [r[0] + self.m[0][3], r[1] + self.m[1][3], r[2] + self.m[2][3]]
    }

    #[inline]
    fn dir_in(&self, d: [f32; 3]) -> [f32; 3] {
        let v = [d[0], -d[2], d[1]];
        let r = &self.r;
        [
            (v[0] * r[0][0] + v[1] * r[1][0]) + v[2] * r[2][0],
            (v[0] * r[0][1] + v[1] * r[1][1]) + v[2] * r[2][1],
            (v[0] * r[0][2] + v[1] * r[1][2]) + v[2] * r[2][2],
        ]
    }

    #[inline]
    fn point_in(&self, p: [f32; 3]) -> [f32; 3] {
        let v = [p[0] - self.vv[0], p[1] - self.vv[1], p[2] - self.vv[2]];
        let r = &self.r;
        [
            (v[0] * r[0][0] + v[1] * r[1][0]) + v[2] * r[2][0],
            (v[0] * r[0][1] + v[1] * r[1][1]) + v[2] * r[2][1],
            (v[0] * r[0][2] + v[1] * r[1][2]) + v[2] * r[2][2],
        ]
    }
}

type TraceRecord = (u8, Vec<usize>, [u16; 8]);

pub struct TraceSurface {
    s: Vec<u8>,
    ledges: Vec<usize>,
    recs: Vec<Option<TraceRecord>>,
    sweep_radius: f32,
}

impl TraceSurface {
    pub fn new(collide: &[u8]) -> TraceSurface {
        let s = surface_of(collide).to_vec();
        let ledges = surface_ledges(&s);
        let recs = records(&s, &ledges);
        let c = [rf32(&s, 0), rf32(&s, 4), rf32(&s, 8)];
        let len = (((c[0] * c[0] + c[1] * c[1]) + c[2] * c[2]) + f32::from_bits(0x3400_0000)).sqrt();
        let sweep_radius = (len + rf32(&s, 24)) * K_IVP2SRC;
        TraceSurface { s, ledges, recs, sweep_radius }
    }

    fn root(&self) -> usize {
        ri32(&self.s, 32) as usize
    }

    fn record_of(&self, ledge: usize) -> Option<&TraceRecord> {
        let i = self.ledges.iter().position(|&o| o == ledge)?;
        self.recs[i].as_ref()
    }

    fn support_of(&self, ledge: usize, place: &Placement) -> PlacedSupport<'_> {
        let l = LedgeRef { b: &self.s, o: ledge };
        let has_records = !self.ledges.is_empty() && self.ledges.len() < 0x41;
        if has_records {
            if let Some((kind, used, oct)) = self.record_of(ledge) {
                if kind & 6 != 0 && !place.moved {
                    return PlacedSupport::Simd(used.iter().map(|&i| place.rotate(l.pt(i))).collect());
                }
                if kind & 2 != 0 {
                    return PlacedSupport::Scan(l, used.clone());
                }
                if kind & 1 != 0 {
                    return PlacedSupport::Hill(l, Some(*oct));
                }
                return PlacedSupport::Hill(l, None);
            }
        } else if l.n_tris() == 2 {
            let p = |e: usize| place.rotate(l.pt(l.sp(0, e)));
            return PlacedSupport::Simd(vec![p(0), p(1), p(2), p(0)]);
        }
        PlacedSupport::Hill(l, None)
    }
}

enum PlacedSupport<'a> {
    Simd(Vec<[f32; 3]>),
    Scan(LedgeRef<'a>, Vec<usize>),
    Hill(LedgeRef<'a>, Option<[u16; 8]>),
}

impl PlacedSupport<'_> {
    fn support(&self, place: &Placement, dsrc: [f32; 3]) -> [f32; 3] {
        match self {
            PlacedSupport::Simd(pts) => simd_support(pts, dsrc),
            PlacedSupport::Scan(l, used) => {
                let d = place.dir_in(dsrc);
                let mut best_i = used[0];
                let mut best = dot_ivp(d, l.pt(best_i));
                for &i in &used[1..] {
                    let f = dot_ivp(d, l.pt(i));
                    if f > best {
                        best = f;
                        best_i = i;
                    }
                }
                place.point(l.pt(best_i))
            }
            PlacedSupport::Hill(l, oct) => {
                let d = place.dir_in(dsrc);
                let start = match oct {
                    Some(o) => {
                        let i = usize::from(d[0] < 0.0) + if d[1] < 0.0 { 2 } else { 0 } + if d[2] < 0.0 { 4 } else { 0 };
                        let s = o[i] as usize;
                        (s >> 2, s & 3)
                    }
                    None => (0, 0),
                };
                place.point(l.pt(hill_climb(*l, d, start)))
            }
        }
    }
}

pub fn placed_aabb(c: &TraceSurface, origin: [f32; 3], angles: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let place = Placement::new(origin, angles);
    let dirs: [[f32; 3]; 6] = [[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let root = c.root();
    if ri32(&c.s, root) == 0 {
        let sup = c.support_of(c.ledges[0], &place);
        let mut mn = [0f32; 3];
        let mut mx = [0f32; 3];
        for k in 0..3 {
            mn[k] = sup.support(&place, dirs[k])[k];
            mx[k] = sup.support(&place, dirs[3 + k])[k];
        }
        return (mn, mx);
    }
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for &o in &c.ledges {
        let sup = c.support_of(o, &place);
        for d in dirs.iter() {
            let p = sup.support(&place, *d);
            for k in 0..3 {
                if p[k] < mn[k] {
                    mn[k] = p[k];
                }
                if p[k] > mx[k] {
                    mx[k] = p[k];
                }
            }
        }
    }
    (mn, mx)
}

fn gjk_start_solid(
    sweep: &PlacedSupport,
    sweep_place: &Placement,
    target: &PlacedSupport,
    target_place: &Placement,
    start: [f32; 3],
    v: &mut [f32; 3],
) -> bool {
    if v[0] == 0.0 && v[1] == 0.0 && v[2] == 0.0 {
        *v = [1.0, 0.0, 0.0];
    }
    let mut d = gjk_normalize(neg3(*v));
    let mut s = Simplex::default();
    for _ in 0..100 {
        let a = sweep.support(sweep_place, d);
        let a = [a[0] + start[0], a[1] + start[1], a[2] + start[2]];
        let b = target.support(target_place, neg3(d));
        let w = sub3(a, b);
        if dyxz(w, d) < 0.0 {
            return false;
        }
        match s.n {
            0 => {
                *v = w;
                s.n = 1;
                s.p[0] = w;
            }
            1 => gjk_s1(&mut s, w, v),
            2 => gjk_s2(&mut s, w, v),
            3 => {
                if gjk_s3(&mut s, w, v) {
                    return true;
                }
            }
            _ => return true,
        }
        d = gjk_normalize(neg3(*v));
    }
    false
}

pub fn trace_collide_start_solid(sweep: &TraceSurface, target: &TraceSurface, origin: [f32; 3], angles: [f32; 3]) -> bool {
    let sweep_place = Placement::new([0.0; 3], [0.0; 3]);
    let target_place = Placement::new([0.0; 3], angles);
    let start = [0.0f32 - origin[0], 0.0f32 - origin[1], 0.0f32 - origin[2]];
    let k = K_SRC2IVP;
    let mid = target_place.point_in([k * start[0], -(k * start[2]), k * start[1]]);
    let s = &target.s;
    let mut supports: std::collections::HashMap<usize, PlacedSupport> = std::collections::HashMap::new();
    for &sl in sweep.ledges.iter().rev() {
        let mut v = start;
        let sweep_sup = sweep.support_of(sl, &sweep_place);
        let extra = sweep.sweep_radius;
        let cull = |n: usize| {
            let (c, r) = node_sphere(s, n);
            let rr = (r + extra) * (r + extra);
            let dy = c[1] - mid[1];
            let dx = c[0] - mid[0];
            let dz = c[2] - mid[2];
            (dy * dy + dx * dx) + dz * dz < rr
        };
        let dist = |n: usize| {
            let (c, _) = node_sphere(s, n);
            let a = mid[1] - c[1];
            let b = mid[0] - c[0];
            let cc = mid[2] - c[2];
            (a * a + b * b) + cc * cc
        };
        let mut leaf = |node: usize, v: &mut [f32; 3]| -> bool {
            let lo = (node as isize + ri32(s, node + 4) as isize) as usize;
            let sup = supports.entry(lo).or_insert_with(|| target.support_of(lo, &target_place));
            gjk_start_solid(&sweep_sup, &sweep_place, sup, &target_place, start, v)
        };
        let root = target.root();
        if !cull(root) {
            continue;
        }
        if ri32(s, root) == 0 {
            if leaf(root, &mut v) {
                return true;
            }
            continue;
        }
        let mut stack: Vec<usize> = Vec::new();
        let mut node = root;
        loop {
            let mut have = true;
            while ri32(s, node) != 0 {
                let left = node + 28;
                let lpass = cull(left).then_some(left);
                let ld = if lpass.is_some() { dist(left) } else { f32::from_bits(0x6753_c21c) };
                let right = (node as isize + ri32(s, node) as isize) as usize;
                if !cull(right) {
                    match lpass {
                        Some(l) => node = l,
                        None => {
                            have = false;
                            break;
                        }
                    }
                } else if let Some(l) = lpass {
                    if dist(right) <= ld {
                        stack.push(l);
                        node = right;
                    } else {
                        stack.push(right);
                        node = l;
                    }
                } else {
                    node = right;
                }
            }
            if have && leaf(node, &mut v) {
                return true;
            }
            match stack.pop() {
                Some(n) => node = n,
                None => break,
            }
        }
    }
    false
}

pub fn convex_from_verts_full(points: &[[f32; 3]]) -> Option<Vec<u8>> {
    let mut stage = |planes: &[[f32; 4]], merge: f32, q: &mut super::ledge::QhullFn| -> Option<Vec<u8>> {
        let mut hs: Vec<[f32; 4]> = Vec::with_capacity(planes.len());
        for p in planes {
            add_halfspace_fast(&mut hs, *p);
        }
        super::ledge::pointsoup_to_ledge(&halfspace_corners_fast(&hs, merge), q)
    };
    super::ledge::convex_from_verts(points, &mut super::ledge::ivp_qhull, &mut stage)
}

pub fn convex_from_planes_full(planes: &[[f32; 4]], merge: f32) -> Option<Vec<u8>> {
    let mut hs: Vec<[f32; 4]> = Vec::with_capacity(planes.len());
    for p in planes {
        add_halfspace_fast(&mut hs, [-p[0], p[2], -p[1], -((-p[3]) * K_SRC2IVP)]);
    }
    super::ledge::pointsoup_to_ledge(&halfspace_corners_fast(&hs, K_SRC2IVP * merge), &mut super::ledge::ivp_qhull)
}

pub fn convex_to_collide_plain(ledges: &[Vec<u8>]) -> Option<Vec<u8>> {
    let mut hull = |p: &[[f32; 3]]| super::ledge::pointsoup_to_ledge(p, &mut super::ledge::ivp_qhull);
    convex_to_collide(ledges, &ConvertParams::default(), &mut hull)
}

fn add_halfspace_fast(list: &mut Vec<[f32; 4]>, h: [f32; 4]) {
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

#[inline]
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn intersect3_fast(a: &[f32; 4], b: &[f32; 4], c: &[f32; 4]) -> Option<[f32; 3]> {
    let (m0, m1, m2) = (a[0], a[1], a[2]);
    let (m4, m5, m6) = (b[0], b[1], b[2]);
    let (m8, m9, m10) = (c[0], c[1], c[2]);
    let c00 = m10 * m5 - m9 * m6;
    let c10 = m8 * m6 - m4 * m10;
    let c20 = m4 * m9 - m8 * m5;
    let det = (m1 * c10 + m0 * c00) + m2 * c20;
    if !(det.abs() >= f32::from_bits(0x2edb_e6ff)) {
        return None;
    }
    let c01 = m2 * m9 - m1 * m10;
    let c11 = m0 * m10 - m2 * m8;
    let c21 = m1 * m8 - m0 * m9;
    let c02 = m1 * m6 - m2 * m5;
    let c12 = m2 * m4 - m0 * m6;
    let c22 = m0 * m5 - m1 * m4;
    let s = 1.0f32 / det;
    let (wa, wb, wc) = (-a[3], -b[3], -c[3]);
    Some([
        ((c00 * s) * wa + (c01 * s) * wb) + (c02 * s) * wc,
        ((c10 * s) * wa + (c11 * s) * wb) + (c12 * s) * wc,
        ((c20 * s) * wa + (c21 * s) * wb) + (c22 * s) * wc,
    ])
}

fn halfspace_corners_fast(hs: &[[f32; 4]], merge: f32) -> Vec<[f32; 3]> {
    let m2 = merge * merge;
    let cand = corner_candidates(hs, Some(CORNER_MARGIN));
    let mut pts: Vec<[f32; 3]> = Vec::new();
    for p in cand {
        if !pts.iter().any(|q| dist2_fast(&p, q) < m2) {
            pts.push(p);
        }
    }
    let mut i = 0;
    while i < pts.len() {
        let mut l = pts.len() - 1;
        while l > i {
            if dist2_fast(&pts[i], &pts[l]) < m2 {
                pts.remove(l);
            }
            l -= 1;
        }
        i += 1;
    }
    pts
}

const CORNER_MARGIN: f64 = 1e-2;

#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn corner_candidates(hs: &[[f32; 4]], margin: Option<f64>) -> Vec<[f32; 3]> {
    let inside = f32::from_bits(0xb8d1_b717);
    let n = hs.len();
    let mut out_pts: Vec<[f32; 3]> = Vec::new();
    let mut recent = [0usize; 8];
    let out = |p: &[f32; 3], h: &[f32; 4]| !(((p[1] * h[1] + p[0] * h[0]) + p[2] * h[2]) + h[3] >= inside);
    let mut eval = |i: usize, j: usize, k: usize, out_pts: &mut Vec<[f32; 3]>| {
        let Some(p) = intersect3_fast(&hs[i], &hs[j], &hs[k]) else { return };
        if let Some(at) = recent.iter().position(|&f| out(&p, &hs[f])) {
            recent[..=at].rotate_right(1);
            return;
        }
        if let Some(f) = (0..n).find(|&f| out(&p, &hs[f])) {
            recent.rotate_right(1);
            recent[0] = f;
            return;
        }
        out_pts.push(p);
    };
    let Some(tau) = margin else {
        for i in 0..n {
            for j in i + 1..n {
                for k in j + 1..n {
                    eval(i, j, k, &mut out_pts);
                }
            }
        }
        return out_pts;
    };
    let h64: Vec<([f64; 3], f64)> = hs.iter().map(|h| ([f64::from(h[0]), f64::from(h[1]), f64::from(h[2])], f64::from(h[3]))).collect();
    let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let mut order: Vec<usize> = Vec::with_capacity(n + 6);
    for axis in 0..3 {
        for sign in [1.0, -1.0] {
            let best = (0..n).min_by(|&p, &q| (sign * h64[p].0[axis]).total_cmp(&(sign * h64[q].0[axis])));
            if let Some(b) = best
                && !order.contains(&b)
            {
                order.push(b);
            }
        }
    }
    let rest: Vec<usize> = (0..n).filter(|f| !order.contains(f)).collect();
    order.extend(rest);
    let faces: Vec<Vec<[f64; 3]>> = (0..n).map(|i| margin_face(&h64, &order, i, tau)).collect();
    let reach = 1.0 + faces.iter().flatten().map(|v| dot(*v, *v).sqrt()).fold(0.0, f64::max);
    let singular = corner_singular(reach, tau);
    let slack = 1e-9 * reach;
    let reached: Vec<Vec<usize>> = (0..n)
        .map(|i| {
            let face = &faces[i];
            let m = face.len().max(1) as f64;
            let c = face.iter().fold([0.0; 3], |s, p| [s[0] + p[0] / m, s[1] + p[1] / m, s[2] + p[2] / m]);
            let r = face.iter().map(|p| ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt()).fold(0.0, f64::max) * (1.0 + 1e-9) + 1e-9;
            (0..n)
                .filter(|&f| {
                    f != i
                        && dot(h64[f].0, c) + h64[f].1 - r * dot(h64[f].0, h64[f].0).sqrt() <= slack
                        && face.iter().any(|v| dot(h64[f].0, *v) + h64[f].1 <= slack)
                })
                .collect()
        })
        .collect();
    const SHORT: f64 = 0.25;
    let mut in_reach = vec![false; n];
    let mut ks: Vec<usize> = Vec::new();
    let mut r = vec![0f64; n];
    let mut dir = vec![(0f64, 0f64); n];
    let mut key = vec![0u64; n];
    let mut pos: Vec<usize> = vec![usize::MAX; n];
    let mut short: Vec<usize> = Vec::new();
    let mut ring: Vec<(u64, usize)> = Vec::new();
    for i in 0..n {
        let ni = h64[i].0;
        let a = if ni[0].abs() < 0.6 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let e1 = cross(ni, a);
        let l1 = dot(e1, e1).sqrt();
        let e1 = [e1[0] / l1, e1[1] / l1, e1[2] / l1];
        let e2 = cross(ni, e1);
        let l2 = dot(e2, e2).sqrt();
        let e2 = [e2[0] / l2, e2[1] / l2, e2[2] / l2];
        short.clear();
        ring.clear();
        for k in i + 1..n {
            let (mut x, mut y) = (dot(h64[k].0, e1), dot(h64[k].0, e2));
            if y < 0.0 || (y == 0.0 && x < 0.0) {
                (x, y) = (-x, -y);
            }
            r[k] = (x * x + y * y).sqrt();
            if r[k] > 0.0 {
                dir[k] = (x / r[k], y / r[k]);
            } else {
                dir[k] = (0.0, 0.0);
            }
            key[k] = if r[k] > 0.0 { (1.0 - x / (x.abs() + y)).max(0.0).to_bits() } else { 0 };
            pos[k] = usize::MAX;
            if r[k] < SHORT {
                short.push(k);
            } else {
                ring.push((key[k], k));
            }
        }
        ring.sort_unstable();
        for (at, &(_, k)) in ring.iter().enumerate() {
            pos[k] = at;
        }
        for &f in &reached[i] {
            in_reach[f] = true;
        }
        let m = ring.len();
        for j in i + 1..n {
            ks.clear();
            if in_reach[j] {
                ks.extend(reached[i].iter().copied().filter(|&k| k > j));
            }
            let u = cross(ni, h64[j].0);
            let mut near = |k: usize| {
                if k > j && dot(u, h64[k].0).abs() < singular {
                    ks.push(k);
                }
            };
            for &k in &short {
                near(k);
            }
            let s = singular / (r[j] * SHORT);
            if s >= 0.5 {
                for &(_, k) in &ring {
                    near(k);
                }
            } else if m > 0 {
                let s = s * 1.01 + 1e-9;
                let (dx, dy) = dir[j];
                let at = if pos[j] != usize::MAX { pos[j] } else { ring.partition_point(|&(q, _)| q < key[j]) % m };
                for step in 0..m {
                    let k = ring[(at + step) % m].1;
                    if (dx * dir[k].1 - dy * dir[k].0).abs() >= s {
                        break;
                    }
                    near(k);
                }
                for step in 1..m {
                    let k = ring[(at + m - step) % m].1;
                    if (dx * dir[k].1 - dy * dir[k].0).abs() >= s {
                        break;
                    }
                    near(k);
                }
            }
            ks.sort_unstable();
            ks.dedup();
            for &k in &ks {
                eval(i, j, k, &mut out_pts);
            }
        }
        for &f in &reached[i] {
            in_reach[f] = false;
        }
    }
    out_pts
}

fn corner_singular(reach: f64, tau: f64) -> f64 {
    1e-6 * reach / (tau - 1e-4)
}

fn margin_face(h64: &[([f64; 3], f64)], order: &[usize], i: usize, tau: f64) -> Vec<[f64; 3]> {
    let (n, w) = h64[i];
    let a = if n[0].abs() < 0.6 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let nn = dot(n, n);
    let e1 = cross(n, a);
    let l1 = dot(e1, e1).sqrt();
    let e1 = [e1[0] / l1, e1[1] / l1, e1[2] / l1];
    let e2 = cross(n, e1);
    let l2 = dot(e2, e2).sqrt();
    let e2 = [e2[0] / l2, e2[1] / l2, e2[2] / l2];
    let c = [-w * n[0] / nn, -w * n[1] / nn, -w * n[2] / nn];
    let big = 1e5;
    let mut poly: Vec<[f64; 3]> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .iter()
        .map(|&(s, t)| [c[0] + big * (s * e1[0] + t * e2[0]), c[1] + big * (s * e1[1] + t * e2[1]), c[2] + big * (s * e1[2] + t * e2[2])])
        .collect();
    let mut next: Vec<[f64; 3]> = Vec::new();
    let sphere = |poly: &[[f64; 3]]| {
        let m = poly.len().max(1) as f64;
        let c = poly.iter().fold([0.0; 3], |s, p| [s[0] + p[0] / m, s[1] + p[1] / m, s[2] + p[2] / m]);
        let r = poly.iter().map(|p| ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt()).fold(0.0, f64::max);
        (c, r * (1.0 + 1e-9) + 1e-9)
    };
    let (mut sc, mut sr) = sphere(&poly);
    for &f in order {
        let (nf, wf) = h64[f];
        if f == i || poly.is_empty() {
            continue;
        }
        let d = |x: [f64; 3]| dot(nf, x) + wf + tau;
        if d(sc) >= sr * dot(nf, nf).sqrt() || poly.iter().all(|&p| d(p) >= 0.0) {
            continue;
        }
        next.clear();
        for k in 0..poly.len() {
            let p = poly[k];
            let q = poly[(k + 1) % poly.len()];
            let (dp, dq) = (d(p), d(q));
            if dp >= 0.0 {
                next.push(p);
            }
            if (dp >= 0.0) != (dq >= 0.0) {
                let t = dp / (dp - dq);
                next.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1]), p[2] + t * (q[2] - p[2])]);
            }
        }
        std::mem::swap(&mut poly, &mut next);
        (sc, sr) = sphere(&poly);
    }
    poly
}

#[inline]
fn dist2_fast(p: &[f32; 3], q: &[f32; 3]) -> f32 {
    let dx = p[0] - q[0];
    let dy = p[1] - q[1];
    let dz = p[2] - q[2];
    (dy * dy + dx * dx) + dz * dz
}

#[cfg(test)]
mod collide_trace_tests {
    use super::*;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }
    }

    fn unit(v: [f32; 3]) -> [f32; 3] {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / l, v[1] / l, v[2] / l]
    }

    fn tangent(c: [f32; 3], h: [f32; 3], normals: &[[f32; 3]]) -> Vec<[f32; 4]> {
        normals
            .iter()
            .map(|&n| {
                let o = [-n[0], -n[1], -n[2]];
                let reach = o[0].abs() * h[0] + o[1].abs() * h[1] + o[2].abs() * h[2];
                [n[0], n[1], n[2], -(n[0] * c[0] + n[1] * c[1] + n[2] * c[2]) + reach]
            })
            .collect()
    }

    #[test]
    fn corner_search_matches_every_triple() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for case in 0..24 {
            let far = if case % 3 == 0 { 400.0 } else { 15.0 };
            let c = [far * (rng.next() - 0.5), far * (rng.next() - 0.5), far * (rng.next() - 0.5)];
            let h = [0.05 + 3.0 * rng.next(), 0.05 + 3.0 * rng.next(), if case % 4 == 1 { 0.001 } else { 0.05 + 3.0 * rng.next() }];
            let mut normals: Vec<[f32; 3]> = Vec::new();
            match case % 4 {
                0 | 2 => {
                    let sides = 8 + case * 2;
                    for s in 0..sides {
                        let a = s as f32 * std::f32::consts::TAU / sides as f32 + 1e-3 * rng.next();
                        normals.push(unit([a.cos(), a.sin(), 1e-4 * (rng.next() - 0.5)]));
                    }
                    normals.push([0.0, 0.0, 1.0]);
                    normals.push([0.0, 0.0, -1.0]);
                }
                _ => {
                    for _ in 0..(30 + case * 3) {
                        normals.push(unit([rng.next() - 0.5, rng.next() - 0.5, rng.next() - 0.5]));
                    }
                }
            }
            let mut hs = Vec::new();
            for p in tangent(c, h, &normals) {
                add_halfspace_fast(&mut hs, p);
            }
            let full = corner_candidates(&hs, None);
            let fast = corner_candidates(&hs, Some(CORNER_MARGIN));
            let bits = |v: &[[f32; 3]]| v.iter().map(|p| p.map(f32::to_bits)).collect::<Vec<_>>();
            assert_eq!(bits(&full), bits(&fast), "case {case}");
        }
    }

    fn box_planes(lo: [f32; 3], hi: [f32; 3]) -> Vec<[f32; 4]> {
        vec![
            [1.0, 0.0, 0.0, hi[0]],
            [-1.0, 0.0, 0.0, -lo[0]],
            [0.0, 1.0, 0.0, hi[1]],
            [0.0, -1.0, 0.0, -lo[1]],
            [0.0, 0.0, 1.0, hi[2]],
            [0.0, 0.0, -1.0, -lo[2]],
        ]
    }

    fn surface(planes: &[[f32; 4]]) -> TraceSurface {
        let convex = convex_from_planes_full(planes, 0.0).unwrap();
        TraceSurface::new(&convex_to_collide_plain(&[convex]).unwrap())
    }

    #[test]
    fn placed_bounds_and_start_solid() {
        let corners: Vec<[f32; 3]> = (0..8).map(|i| [if i & 1 == 0 { -10.0 } else { 10.0 }, if i & 2 == 0 { -5.0 } else { 5.0 }, if i & 4 == 0 { 0.0 } else { 20.0 }]).collect();
        let prop = TraceSurface::new(&convex_to_collide_plain(&[convex_from_verts_full(&corners).unwrap()]).unwrap());
        let (lo, hi) = placed_aabb(&prop, [100.0, 0.0, 0.0], [0.0, 90.0, 0.0]);
        for (a, b) in lo.iter().zip([95.0, -10.0, 0.0]).chain(hi.iter().zip([105.0, 10.0, 20.0])) {
            assert!((a - b).abs() < 1e-3, "{lo:?} {hi:?}");
        }
        let solid = |lo: [f32; 3], hi: [f32; 3]| trace_collide_start_solid(&surface(&box_planes(lo, hi)), &prop, [100.0, 0.0, 0.0], [0.0, 90.0, 0.0]);
        assert!(solid([0.0, 0.0, 0.0], [200.0, 200.0, 200.0]));
        assert!(solid([104.0, 9.0, 19.0], [300.0, 300.0, 300.0]));
        assert!(!solid([106.0, -100.0, -100.0], [300.0, 300.0, 300.0]));
        assert!(!solid([-300.0, 11.0, -100.0], [300.0, 300.0, 300.0]));
    }
}
