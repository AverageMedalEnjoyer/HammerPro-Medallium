#[derive(Clone, Copy, Debug, Default)]
pub struct Simplex {
    pub p: [[f32; 3]; 4],
    pub n: i32,
}

#[inline]
fn dyxz(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[1] * b[1] + a[0] * b[0] + a[2] * b[2]
}

#[inline]
fn neg3(a: [f32; 3]) -> [f32; 3] {
    [-a[0], -a[1], -a[2]]
}

#[inline]
fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
pub fn gjk_normalize(v: [f32; 3]) -> [f32; 3] {
    let s = (v[0] * v[0] + v[1] * v[1]) + v[2] * v[2];
    let f = 1.0f32 / (s.sqrt() + f32::from_bits(0x34000000));
    [v[0] * f, v[1] * f, v[2] * f]
}

#[inline]
fn edge_dir(e: [f32; 3], d: [f32; 3]) -> [f32; 3] {
    let cx = d[2] * e[1] - d[1] * e[2];
    let cy = d[0] * e[2] - d[2] * e[0];
    let cz = d[1] * e[0] - d[0] * e[1];
    [cy * e[2] - cz * e[1], cz * e[0] - cx * e[2], cx * e[1] - cy * e[0]]
}

pub fn gjk_s1(s: &mut Simplex, b: [f32; 3], v: &mut [f32; 3]) {
    let a = s.p[0];
    let f5 = a[0] - b[0];
    let f6 = a[2] - b[2];
    let f7 = a[1] - b[1];
    if 0.0 <= b[1] * f7 + b[0] * f5 + b[2] * f6 {
        s.n = 1;
        s.p[0] = b;
        *v = b;
    } else {
        s.n = 2;
        s.p[1] = b;
        let f2 = b[0] * f6 - b[2] * f5;
        let f4 = b[1] * f5 - b[0] * f7;
        let f3 = b[2] * f7 - b[1] * f6;
        *v = [f2 * f6 - f4 * f7, f4 * f5 - f3 * f6, f3 * f7 - f2 * f5];
        if f2 * f2 + f3 * f3 + f4 * f4 <= f32::from_bits(0x322bcc77) {
            *v = [f7 - f6 * 0.0, f6 * 0.0 - f5, f5 * 0.0 - f7 * 0.0];
        }
    }
}

pub fn gjk_s2(s: &mut Simplex, c: [f32; 3], v: &mut [f32; 3]) {
    let a = s.p[0];
    let b = s.p[1];
    let f10 = a[0] - c[0];
    let f11 = a[1] - c[1];
    let f4 = b[2] - c[2];
    let f12 = a[2] - c[2];
    let f13 = b[0] - c[0];
    let f14 = b[1] - c[1];
    let f6 = f4 * f11 - f14 * f12;
    let f8 = f12 * f13 - f4 * f10;
    let f9 = f14 * f10 - f11 * f13;
    let t1 = (f9 * f13 - f6 * f4) * c[1] + (f8 * f4 - f9 * f14) * c[0] + (f6 * f14 - f8 * f13) * c[2];
    if 0.0 <= t1 {
        let t2 = (f6 * f12 - f9 * f10) * c[1] + (f9 * f11 - f8 * f12) * c[0] + (f8 * f10 - f6 * f11) * c[2];
        if 0.0 < t2 {
            s.n = 3;
            if 0.0 <= f8 * c[1] + f6 * c[0] + f9 * c[2] {
                s.p[2] = s.p[1];
                s.p[1] = c;
                *v = [f6, f8, f9];
            } else {
                s.p[2] = c;
                *v = [-f6, -f8, -f9];
            }
            return;
        }
    } else if f14 * c[1] + f13 * c[0] + f4 * c[2] < 0.0 {
        s.n = 2;
        s.p[0] = c;
        *v = edge_dir([f13, f14, f4], c);
        return;
    }
    if 0.0 <= f11 * c[1] + f10 * c[0] + f12 * c[2] {
        s.n = 1;
        s.p[0] = c;
        *v = c;
    } else {
        s.n = 2;
        s.p[1] = c;
        *v = edge_dir([f10, f11, f12], c);
    }
}

pub fn gjk_s3(s: &mut Simplex, d: [f32; 3], v: &mut [f32; 3]) -> bool {
    let (dx, dy, dz) = (d[0], d[1], d[2]);
    let a = s.p[0];
    let b = s.p[1];
    let c = s.p[2];
    let f36 = b[0] - dx;
    let f37 = b[1] - dy;
    let f38 = b[2] - dz;
    let f39 = a[0] - dx;
    let f40 = a[1] - dy;
    let f41 = a[2] - dz;
    let f44 = c[2] - dz;
    let f42 = c[0] - dx;
    let f43 = c[1] - dy;
    let ea = [f39, f40, f41];
    let eb = [f36, f37, f38];
    let ec = [f42, f43, f44];
    let f6 = f38 * f40 - f37 * f41;
    let f7 = f36 * f41 - f38 * f39;
    let f8 = f37 * f39 - f36 * f40;
    let f9 = f44 * f37 - f43 * f38;
    let f10 = f42 * f38 - f44 * f36;
    let f11 = f43 * f36 - f42 * f37;
    let f28 = f43 * f41 - f44 * f40;
    let f35 = f44 * f39 - f42 * f41;
    let f12 = f42 * f40 - f43 * f39;
    let f29 = f7 * f38 - f8 * f37;
    let f13 = f8 * f36 - f6 * f38;
    let f14 = f6 * f37 - f7 * f36;
    let f15 = f8 * f40 - f7 * f41;
    let f16 = f6 * f41 - f8 * f39;
    let f17 = f7 * f39 - f6 * f40;
    let f30 = f10 * f44 - f11 * f43;
    let f18 = f11 * f42 - f9 * f44;
    let f19 = f9 * f43 - f10 * f42;
    let f20 = f11 * f37 - f10 * f38;
    let f21 = f9 * f38 - f11 * f36;
    let f22 = f10 * f36 - f9 * f37;
    let f31 = f35 * f41 - f12 * f40;
    let f23 = f12 * f39 - f28 * f41;
    let f24 = f28 * f40 - f35 * f39;
    let f25 = f12 * f43 - f35 * f44;
    let f26 = f28 * f44 - f12 * f42;
    let f27 = f35 * f42 - f28 * f43;
    let t = |m: [f32; 3]| dyxz(m, d);
    let mut bits = 0u8;
    if t([f6, f7, f8]) < 0.0 {
        bits |= 1;
    }
    if t([f9, f10, f11]) < 0.0 {
        bits |= 2;
    }
    if t([f28, f35, f12]) < 0.0 {
        bits |= 4;
    }
    macro_rules! vert_d {
        () => {{
            s.n = 1;
            s.p[0] = d;
            *v = d;
            return false;
        }};
    }
    macro_rules! edge_ad {
        () => {{
            s.n = 2;
            s.p[1] = d;
            *v = edge_dir(ea, d);
            return false;
        }};
    }
    macro_rules! edge_bd {
        () => {{
            s.n = 2;
            s.p[0] = d;
            *v = edge_dir(eb, d);
            return false;
        }};
    }
    macro_rules! edge_cd {
        () => {{
            s.p[0] = s.p[2];
            s.n = 2;
            s.p[1] = d;
            *v = edge_dir(ec, d);
            return false;
        }};
    }
    macro_rules! face_abd {
        () => {{
            s.n = 3;
            s.p[2] = d;
            *v = [-f6, -f7, -f8];
            return false;
        }};
    }
    macro_rules! face_bcd {
        () => {{
            s.n = 3;
            s.p[0] = d;
            *v = [-f9, -f10, -f11];
            return false;
        }};
    }
    macro_rules! face_cad {
        () => {{
            s.n = 3;
            s.p[1] = d;
            *v = [-f28, -f35, -f12];
            return false;
        }};
    }
    match bits {
        0 => {
            s.p[3] = d;
            s.n = 4;
            true
        }
        1 => {
            gjk_s2(s, d, v);
            false
        }
        2 => {
            s.p[0] = s.p[2];
            gjk_s2(s, d, v);
            false
        }
        4 => {
            s.p[1] = s.p[2];
            gjk_s2(s, d, v);
            false
        }
        3 => {
            if 0.0 <= t([f29, f13, f14]) {
                if 0.0 <= t([f15, f16, f17]) {
                    face_abd!();
                }
                if dyxz(d, ea) < 0.0 {
                    edge_ad!();
                }
                vert_d!();
            }
            if 0.0 <= t([f20, f21, f22]) {
                if t([f30, f18, f19]) < 0.0 {
                    edge_cd!();
                }
                face_bcd!();
            }
            if dyxz(d, eb) < 0.0 {
                edge_bd!();
            }
            vert_d!();
        }
        5 => {
            if 0.0 <= t([f31, f23, f24]) {
                if 0.0 <= t([f25, f26, f27]) {
                    face_cad!();
                }
                if dyxz(d, ec) < 0.0 {
                    edge_cd!();
                }
                vert_d!();
            }
            if 0.0 <= t([f15, f16, f17]) {
                if t([f29, f13, f14]) < 0.0 {
                    edge_bd!();
                }
                face_abd!();
            }
            if dyxz(d, ea) < 0.0 {
                edge_ad!();
            }
            vert_d!();
        }
        6 => {
            if 0.0 <= t([f30, f18, f19]) {
                if 0.0 <= t([f20, f21, f22]) {
                    face_bcd!();
                }
                if dyxz(d, eb) < 0.0 {
                    edge_bd!();
                }
                vert_d!();
            }
            if 0.0 <= t([f25, f26, f27]) {
                if 0.0 <= t([f31, f23, f24]) {
                    face_cad!();
                }
                edge_ad!();
            }
            if dyxz(d, ec) < 0.0 {
                edge_cd!();
            }
            vert_d!();
        }
        _ => {
            if 0.0 <= dyxz(d, ea) {
                if 0.0 <= dyxz(d, eb) {
                    if dyxz(d, ec) < 0.0 {
                        edge_cd!();
                    }
                    vert_d!();
                }
                edge_bd!();
            }
            edge_ad!();
        }
    }
}

#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn gjk_face_toi(s: &Simplex, r: [f32; 3], c: f32) -> f32 {
    let p = &s.p;
    let big = f32::from_bits(0x6753c21c);
    let f3 = p[0][1] - p[2][1];
    let f1 = p[0][2] - p[2][2];
    let f5 = p[1][1] - p[2][1];
    let f2 = p[1][2] - p[2][2];
    let f7 = p[1][0] - p[2][0];
    let f4 = p[0][0] - p[2][0];
    let f8 = p[1][1] - p[0][1];
    let f6 = p[1][2] - p[0][2];
    let f9 = p[1][0] - p[0][0];
    let n = gjk_normalize([f2 * f3 - f5 * f1, f7 * f1 - f2 * f4, f5 * f4 - f7 * f3]);
    let e0 = gjk_normalize([n[2] * f3 - n[1] * f1, n[0] * f1 - n[2] * f4, n[1] * f4 - n[0] * f3]);
    let e1 = gjk_normalize([n[1] * f2 - n[2] * f5, n[2] * f7 - n[0] * f2, n[0] * f5 - n[1] * f7]);
    let e2 = gjk_normalize([n[2] * f8 - n[1] * f6, n[0] * f6 - n[2] * f9, n[1] * f9 - n[0] * f8]);
    let den = |e: [f32; 3]| r[0] * e[0] + r[1] * e[1] + r[2] * e[2];
    let num = |e: [f32; 3], q: [f32; 3]| e[1] * q[1] + e[0] * q[0] + e[2] * q[2];
    let d0 = den(e0);
    let t0 = if d0 > 0.0 { num(e0, p[0]) / d0 } else { big };
    let d1 = den(e1);
    let t1 = if d1 > 0.0 { num(e1, p[1]) / d1 } else { big };
    let d2 = den(e2);
    let t2 = if d2 > 0.0 { num(e2, p[1]) / d2 } else { big };
    let mut m = t0;
    let mut e = e0;
    if !(t1 >= m) {
        m = t1;
        e = e1;
    }
    if !(t2 >= m) {
        m = t2;
        e = e2;
    }
    let dd = r[1] * e[1] + r[0] * e[0] + r[2] * e[2];
    if dd > 0.0 { c / dd + m } else { big }
}

enum GjkSup<'a> {
    Simd(Vec<[f32; 3]>),
    Hill(LedgeRef<'a>, (usize, usize), Option<[u16; 8]>),
}

impl<'a> GjkSup<'a> {
    fn new(l: LedgeRef<'a>, rec: Option<&(u8, Vec<usize>, [u16; 8])>) -> GjkSup<'a> {
        if let Some((kind, used, oct)) = rec {
            if kind & 6 != 0 {
                return GjkSup::Simd(used.iter().map(|&i| simd_tf(l.pt(i))).collect());
            }
            return GjkSup::Hill(l, (0, 0), Some(*oct));
        }
        if l.n_tris() == 2 {
            let p = |e: usize| simd_tf(l.pt(l.sp(0, e)));
            return GjkSup::Simd(vec![p(0), p(1), p(2), p(0)]);
        }
        GjkSup::Hill(l, (0, 0), None)
    }
    fn support(&self, dsrc: [f32; 3]) -> [f32; 3] {
        match self {
            GjkSup::Simd(pts) => simd_support(pts, dsrc),
            GjkSup::Hill(l, st, oct) => {
                let d = [dsrc[0], -dsrc[2], dsrc[1]];
                let s = match oct {
                    Some(o) => {
                        let i = (if d[0] < 0.0 { 1 } else { 0 }) + (if d[1] < 0.0 { 2 } else { 0 }) + (if d[2] < 0.0 { 4 } else { 0 });
                        let s = o[i] as usize;
                        (s >> 2, s & 3)
                    }
                    None => *st,
                };
                point_tf(l.pt(hill_climb(*l, d, s)))
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GjkRay {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub delta: [f32; 3],
    pub dir: [f32; 3],
    pub len: f32,
    pub len0: f32,
    pub inv: f32,
}

pub enum GjkOut {
    Miss,
    Enclosed,
    Near(f32),
}

#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn gjk_ray_ledge(sup: &GjkSup, ray: &GjkRay, v: &mut [f32; 3], eps: f32) -> GjkOut {
    if v[0] == 0.0 && v[1] == 0.0 && v[2] == 0.0 {
        *v = [1.0, 0.0, 0.0];
    }
    let support = |d: [f32; 3]| -> [f32; 3] {
        let ep = if d[1] * ray.delta[1] + d[0] * ray.delta[0] + d[2] * ray.delta[2] <= 0.0 { ray.start } else { ray.end };
        let w0 = [0.0f32 + ep[0], 0.0f32 + ep[1], 0.0f32 + ep[2]];
        let q = sup.support(neg3(d));
        sub3(w0, q)
    };
    let mut d = gjk_normalize(neg3(*v));
    let mut s = Simplex::default();
    let mut it = 0;
    let mut w;
    let mut best: f32;
    let mut best_d: [f32; 3];
    loop {
        w = support(d);
        let f18 = dyxz(w, d);
        if f18 < 0.0 {
            let f18 = f18.abs();
            if f18 < eps && 0.0 < ray.len {
                best = f18;
                best_d = d;
                break;
            }
            return GjkOut::Miss;
        }
        if s.n != 0 {
            let enclosed = match s.n {
                1 => {
                    gjk_s1(&mut s, w, v);
                    false
                }
                2 => {
                    gjk_s2(&mut s, w, v);
                    false
                }
                3 => gjk_s3(&mut s, w, v),
                _ => true,
            };
            if enclosed {
                return GjkOut::Enclosed;
            }
        } else {
            *v = w;
            s.n = 1;
            s.p[0] = w;
        }
        d = gjk_normalize(neg3(*v));
        it += 1;
        if it >= 100 {
            return GjkOut::Miss;
        }
    }
    if best > 0.0 {
        let mut k = 0;
        loop {
            match s.n {
                0 => {
                    *v = w;
                    s.n = 1;
                    s.p[0] = w;
                }
                1 => gjk_s1(&mut s, w, v),
                2 => gjk_s2(&mut s, w, v),
                3 => {
                    gjk_s3(&mut s, w, v);
                }
                _ => {}
            }
            d = gjk_normalize(neg3(*v));
            let wn = support(d);
            let f16 = -dyxz(wn, d);
            if f16 > eps {
                return GjkOut::Miss;
            }
            if f16 > best {
                best = f16;
                best_d = d;
            }
            let prev = -dyxz(w, d);
            w = wn;
            if f16 - prev > f32::from_bits(0xb8d1b717) {
                break;
            }
            k += 1;
            if k >= 20 {
                break;
            }
        }
    }
    let f17 = -dyxz(best_d, ray.delta);
    let ok = (f17 as f64) < -(eps as f64 * 0.1) || (f17 < f32::from_bits(0xb8d1b717) && (best as f64) < eps as f64 * 0.9);
    if !ok {
        return GjkOut::Miss;
    }
    let mut a0 = ray.len - (-(((eps - best) * ray.len0) / f17));
    if !(a0 >= 0.0) {
        let f19 = if s.n == 3 { gjk_face_toi(&s, ray.dir, eps) } else { 0.0 };
        a0 = 0.0;
        if ray.len > f19 {
            a0 = ray.len - f19;
        }
    }
    GjkOut::Near(a0)
}

struct CullRay {
    mid: [f32; 3],
    start: [f32; 3],
    dir: [f32; 3],
    delta: [f32; 3],
    klen: f32,
}

#[inline]
fn sphere_hit(c: &CullRay, center: [f32; 3], radius: f32) -> bool {
    let r = radius + 0.0f32;
    let r2 = r * r;
    let dy = center[1] - c.mid[1];
    let dx = center[0] - c.mid[0];
    let dz = center[2] - c.mid[2];
    let dist2 = if c.klen <= 0.0 {
        (dy * dy + dx * dx) + dz * dz
    } else {
        let cx = c.dir[1] * dz - c.dir[2] * dy;
        let cz = c.dir[0] * dy - c.dir[1] * dx;
        let cy = c.dir[2] * dx - c.dir[0] * dz;
        (cy * cy + cx * cx) + cz * cz
    };
    dist2 < r2
}

#[inline]
fn node_sphere(s: &[u8], n: usize) -> ([f32; 3], f32) {
    ([rf32(s, n + 8), rf32(s, n + 12), rf32(s, n + 16)], rf32(s, n + 20))
}

pub struct TraceCtx<'a> {
    s: &'a [u8],
    root: usize,
    sups: std::collections::HashMap<usize, GjkSup<'a>>,
}

impl<'a> TraceCtx<'a> {
    pub fn new(s: &'a [u8]) -> TraceCtx<'a> {
        let leaves = surface_ledges(s);
        let recs = records(s, &leaves);
        let mut sups = std::collections::HashMap::new();
        for (i, &o) in leaves.iter().enumerate() {
            sups.insert(o, GjkSup::new(LedgeRef { b: s, o }, recs[i].as_ref()));
        }
        TraceCtx { s, root: ri32(s, 32) as usize, sups }
    }

    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn ray_hit(&self, start: [f32; 3], end: [f32; 3]) -> bool {
        let s = self.s;
        let eps = f32::from_bits(0x3d000000);
        let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        let st0 = [0.0f32 + start[0], 0.0f32 + start[1], 0.0f32 + start[2]];
        let st = [-0.0f32 + st0[0], -0.0f32 + st0[1], -0.0f32 + st0[2]];
        let en = [delta[0] + st[0], delta[1] + st[1], delta[2] + st[2]];
        let len = (delta[1] * delta[1] + delta[0] * delta[0] + delta[2] * delta[2]).sqrt();
        let mut inv = 0.0f32;
        let mut dir = delta;
        if 0.0 < len {
            inv = 1.0f32 / len;
            dir = [delta[0] * inv, delta[1] * inv, delta[2] * inv];
        }
        let mut best_frac = if len <= f32::from_bits(0x322bcc77) { f32::from_bits(0x322bcc77) } else { len };
        let mut ray = GjkRay { start: st, end: en, delta, dir, len, len0: len, inv };
        let mut v = st;
        let k = K_SRC2IVP;
        let st_ivp = [k * st[0], -(k * st[2]), k * st[1]];
        let d_ivp = [k * delta[0], -(k * delta[2]), k * delta[1]];
        let mut dn = d_ivp;
        let q = (dn[0] * dn[0] + dn[1] * dn[1]) + dn[2] * dn[2];
        if !(q < f32::from_bits(0x2edbe6ff)) {
            let f = 1.0f32 / q.sqrt();
            dn = [dn[0] * f, dn[1] * f, dn[2] * f];
        }
        let mut cull = CullRay {
            mid: [d_ivp[0] * 0.5 + st_ivp[0], d_ivp[1] * 0.5 + st_ivp[1], d_ivp[2] * 0.5 + st_ivp[2]],
            start: st_ivp,
            dir: dn,
            delta: d_ivp,
            klen: k * len,
        };
        let mut frac = 1.0f32;
        let mut leaf = |node: usize, ray: &mut GjkRay, cull: &mut CullRay, v: &mut [f32; 3]| -> bool {
            let lo = (node as isize + ri32(s, node + 4) as isize) as usize;
            let sup = match self.sups.get(&lo) {
                Some(x) => x,
                None => return false,
            };
            let a0 = match gjk_ray_ledge(sup, ray, v, eps) {
                GjkOut::Miss => return false,
                GjkOut::Enclosed => return true,
                GjkOut::Near(a0) => a0,
            };
            if !(a0 < best_frac) {
                return false;
            }
            best_frac = a0;
            frac = a0 * ray.inv;
            if frac < 1.0 {
                return true;
            }
            let f5 = (eps + eps + a0) * ray.inv;
            if f5 < 1.0 {
                ray.len = f5 * ray.len0;
                let f6 = f5 * 0.5f32;
                ray.end = [f5 * ray.delta[0] + ray.start[0], f5 * ray.delta[1] + ray.start[1], f5 * ray.delta[2] + ray.start[2]];
                cull.klen = k * ray.len;
                cull.mid = [f6 * cull.delta[0] + cull.start[0], f6 * cull.delta[1] + cull.start[1], f6 * cull.delta[2] + cull.start[2]];
            }
            false
        };
        let root = self.root;
        let (c, r) = node_sphere(s, root);
        if !sphere_hit(&cull, c, r) {
            return false;
        }
        if ri32(s, root) == 0 {
            return leaf(root, &mut ray, &mut cull, &mut v);
        }
        let mut stack: Vec<usize> = Vec::new();
        let mut node = root;
        loop {
            let mut have = true;
            while ri32(s, node) != 0 {
                let left = node + 28;
                let (lc, lr) = node_sphere(s, left);
                let mut lpass = None;
                let mut ld = f32::from_bits(0x6753c21c);
                if sphere_hit(&cull, lc, lr) {
                    lpass = Some(left);
                    let a = cull.start[1] - lc[1];
                    let b = cull.start[0] - lc[0];
                    let cc = cull.start[2] - lc[2];
                    ld = (a * a + b * b) + cc * cc;
                }
                let right = (node as isize + ri32(s, node) as isize) as usize;
                let (rc, rr) = node_sphere(s, right);
                if !sphere_hit(&cull, rc, rr) {
                    match lpass {
                        Some(l) => node = l,
                        None => {
                            have = false;
                            break;
                        }
                    }
                } else if let Some(l) = lpass {
                    let a = cull.start[1] - rc[1];
                    let b = cull.start[0] - rc[0];
                    let cc = cull.start[2] - rc[2];
                    if (a * a + b * b) + cc * cc <= ld {
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
            if have && leaf(node, &mut ray, &mut cull, &mut v) {
                return true;
            }
            match stack.pop() {
                Some(n) => node = n,
                None => break,
            }
        }
        frac < 1.0
    }
}

pub fn trace_ray_hit(s: &[u8], start: [f32; 3], end: [f32; 3]) -> bool {
    TraceCtx::new(s).ray_hit(start, end)
}
