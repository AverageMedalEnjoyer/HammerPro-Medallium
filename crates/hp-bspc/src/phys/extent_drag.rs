const WM: [[f32; 4]; 3] = [
    [K_IVP2SRC, -0.0, -0.0, 0.0],
    [0.0, -0.0, K_IVP2SRC, 0.0],
    [-0.0, -K_IVP2SRC, 0.0, 0.0],
];

fn trace_record(l: LedgeRef) -> TraceRecord {
    let mut kind = 0u8;
    let mut used: Vec<usize> = Vec::new();
    let flags = ru32(l.b, l.o + 8);
    let nt = ru16(l.b, l.o + 12) as i16;
    if flags & 3 == 0 && nt < 0x201 {
        let mut mn = 0x1ffffffusize;
        let mut mx = 0usize;
        for t in 0..l.n_tris() {
            for e in 0..3 {
                let v = l.sp(t, e);
                if v < mn {
                    mn = v;
                }
                if v > mx {
                    mx = v;
                }
            }
        }
        let range = (mx as isize - mn as isize) + 1;
        if range < 0x80 && range > 0 {
            let range = range as usize;
            let mut mark = vec![false; range];
            for t in 0..l.n_tris() {
                for e in 0..3 {
                    mark[l.sp(t, e) - mn] = true;
                }
            }
            let mut runs: Vec<usize> = Vec::new();
            let mut cur = mark[0];
            let mut cnt = 1usize;
            let mut overflow = false;
            for &m in mark.iter().skip(1) {
                if runs.len() > 0x7f {
                    overflow = true;
                    break;
                }
                if m != cur {
                    runs.push(cnt);
                    cnt = 0;
                    cur = m;
                }
                cnt += 1;
            }
            if !overflow && runs.len() > 0x7f {
                overflow = true;
            }
            if !overflow {
                runs.push(cnt);
                if runs.len() < 0xf {
                    kind = if runs.len() == 1 { 2 } else { 4 };
                    let mut idx = mn;
                    for (ri, &r) in runs.iter().enumerate() {
                        if ri & 1 == 0 {
                            for k in 0..r {
                                used.push(idx + k);
                            }
                        }
                        idx += r;
                    }
                }
            }
        }
    }
    let mut oct = [0u16; 8];
    if kind & 6 == 0 {
        kind = 1;
        for (i, o) in oct.iter_mut().enumerate() {
            let d = [
                if i & 1 != 0 { -1.0f32 } else { 1.0 },
                if i & 2 != 0 { -1.0f32 } else { 1.0 },
                if i & 4 != 0 { -1.0f32 } else { 1.0 },
            ];
            *o = octant_start(l, d) as u16;
        }
    }
    (kind, used, oct)
}

#[inline]
fn dot_ivp(d: [f32; 3], p: [f32; 3]) -> f32 {
    d[1] * p[1] + d[0] * p[0] + d[2] * p[2]
}

#[inline]
fn e_prev(e: (usize, usize)) -> (usize, usize) {
    (e.0, (e.1 + 2) % 3)
}
fn e_opp(l: LedgeRef, e: (usize, usize)) -> (usize, usize) {
    let w = ru32(l.b, l.o + 16 + e.0 * 16 + 4 + e.1 * 4) as i32;
    let opp = (w << 1) >> 17;
    let addr = (16 + e.0 * 16 + 4 + e.1 * 4) as isize + (opp as isize) * 4;
    let addr = addr as usize;
    ((addr - 16) / 16, ((addr - 16) % 16 - 4) / 4)
}
#[inline]
fn e_start(l: LedgeRef, e: (usize, usize)) -> usize {
    l.sp(e.0, e.1)
}

fn octant_start(l: LedgeRef, d: [f32; 3]) -> usize {
    let mut e = (0usize, 0usize);
    let mut tri_base = 0usize;
    let mut best_v = e_start(l, e);
    let mut best = dot_ivp(d, l.pt(best_v));
    let n = l.n_tris();
    let mut it = 0;
    while it < n {
        e = e_prev(e);
        let first = e_start(l, e);
        let mut u = first;
        let mut improved = false;
        loop {
            let f = dot_ivp(d, l.pt(u));
            if best < f {
                tri_base = e.0;
                best = f;
                best_v = u;
                improved = true;
                break;
            }
            e = e_prev(e_opp(l, e));
            u = e_start(l, e);
            if u == first {
                break;
            }
        }
        if !improved {
            break;
        }
        it += 1;
    }
    for k in 0..3 {
        if l.sp(tri_base, k) == best_v {
            return tri_base * 4 + k;
        }
    }
    tri_base * 4
}

fn hill_climb(l: LedgeRef, d: [f32; 3], start: (usize, usize)) -> usize {
    let mut e = start;
    let mut v = e_start(l, e);
    let mut visited = std::collections::HashSet::new();
    visited.insert(v);
    let mut best = dot_ivp(d, l.pt(v));
    let n = l.n_tris();
    let mut it = 0;
    while it < n {
        e = e_prev(e);
        let first = e_start(l, e);
        let mut u = first;
        let mut improved = false;
        loop {
            if !visited.contains(&u) {
                visited.insert(u);
                let f = dot_ivp(d, l.pt(u));
                if best < f {
                    v = u;
                    best = f;
                    improved = true;
                    break;
                }
            }
            e = e_prev(e_opp(l, e));
            u = e_start(l, e);
            if u == first {
                break;
            }
        }
        if !improved {
            break;
        }
        it += 1;
    }
    v
}

#[inline]
fn simd_tf(p: [f32; 3]) -> [f32; 3] {
    let m = &WM;
    [
        p[0] * m[0][0] + p[1] * m[0][1] + p[2] * m[0][2],
        p[1] * m[1][1] + p[0] * m[1][0] + p[2] * m[1][2],
        p[2] * m[2][2] + p[0] * m[2][0] + p[1] * m[2][1],
    ]
}

#[inline]
fn point_tf(p: [f32; 3]) -> [f32; 3] {
    let m = &WM;
    [
        p[1] * m[0][1] + p[0] * m[0][0] + p[2] * m[0][2] + m[0][3],
        p[1] * m[1][1] + p[0] * m[1][0] + p[2] * m[1][2] + m[1][3],
        p[1] * m[2][1] + p[0] * m[2][0] + p[2] * m[2][2] + m[2][3],
    ]
}

fn simd_support(pts: &[[f32; 3]], d: [f32; 3]) -> [f32; 3] {
    let n = pts.len();
    let groups = (n + 3) >> 2;
    let lane = |g: usize, j: usize| -> [f32; 3] {
        let i = 4 * g + j;
        if i < n { pts[i] } else { pts[n - 1] }
    };
    let dotv = |p: [f32; 3]| p[1] * d[1] + d[0] * p[0] + p[2] * d[2];
    let mut m = [0f32; 4];
    let mut idx = [0usize; 4];
    for j in 0..4 {
        m[j] = dotv(lane(0, j));
        idx[j] = j;
    }
    for g in 1..groups {
        for j in 0..4 {
            let f = dotv(lane(g, j));
            if m[j] < f {
                idx[j] = 4 * g + j;
            }
            m[j] = if f > m[j] { f } else { m[j] };
        }
    }
    let l0 = if m[2] > m[0] { m[2] } else { m[0] };
    let l1 = if m[3] > m[1] { m[3] } else { m[1] };
    let sel = if l0 < l1 {
        if m[1] < m[3] { idx[3] } else { idx[1] }
    } else if m[0] < m[2] {
        idx[2]
    } else {
        idx[0]
    };
    let g = sel >> 2;
    lane(g, sel & 3)
}

fn ledge_support(l: LedgeRef, rec: Option<&(u8, Vec<usize>, [u16; 8])>, dsrc: [f32; 3]) -> [f32; 3] {
    if let Some((kind, used, oct)) = rec {
        if kind & 6 != 0 {
            let pts: Vec<[f32; 3]> = used.iter().map(|&i| simd_tf(l.pt(i))).collect();
            return simd_support(&pts, dsrc);
        }
        let d = [dsrc[0], -dsrc[2], dsrc[1]];
        let o = (if d[0] < 0.0 { 1 } else { 0 }) + (if d[1] < 0.0 { 2 } else { 0 }) + (if d[2] < 0.0 { 4 } else { 0 });
        let s = oct[o] as usize;
        let v = hill_climb(l, d, (s >> 2, s & 3));
        return point_tf(l.pt(v));
    }
    if l.n_tris() == 2 {
        let pts = [simd_tf(l.pt(l.sp(0, 0))), simd_tf(l.pt(l.sp(0, 1))), simd_tf(l.pt(l.sp(0, 2))), simd_tf(l.pt(l.sp(0, 0)))];
        return simd_support(&pts, dsrc);
    }
    let d = [dsrc[0], -dsrc[2], dsrc[1]];
    let v = hill_climb(l, d, (0, 0));
    point_tf(l.pt(v))
}

fn records(s: &[u8], leaves: &[usize]) -> Vec<Option<TraceRecord>> {
    let have = !leaves.is_empty() && leaves.len() < 0x41;
    leaves.iter().map(|&o| if have { Some(trace_record(LedgeRef { b: s, o })) } else { None }).collect()
}

fn extent_impl(s: &[u8], dir: [f32; 3]) -> [f32; 3] {
    let leaves = surface_ledges(s);
    let recs = records(s, &leaves);
    let root = ri32(s, 32) as usize;
    if ri32(s, root) == 0 {
        return ledge_support(LedgeRef { b: s, o: leaves[0] }, recs[0].as_ref(), dir);
    }
    let mut best = -1000000.0f32;
    let mut res = [0f32; 3];
    for (i, &o) in leaves.iter().enumerate() {
        let p = ledge_support(LedgeRef { b: s, o }, recs[i].as_ref(), dir);
        let f = p[1] * dir[1] + p[0] * dir[0] + p[2] * dir[2];
        if f > best {
            best = f;
            res = p;
        }
    }
    res
}

pub fn collide_aabb(c: &[u8]) -> ([f32; 3], [f32; 3]) {
    aabb_impl(surface_of(c))
}

fn aabb_impl(s: &[u8]) -> ([f32; 3], [f32; 3]) {
    let leaves = surface_ledges(s);
    let recs = records(s, &leaves);
    let dirs: [[f32; 3]; 6] = [[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let root = ri32(s, 32) as usize;
    if ri32(s, root) == 0 {
        let l = LedgeRef { b: s, o: leaves[0] };
        let mut mn = [0f32; 3];
        let mut mx = [0f32; 3];
        for k in 0..3 {
            mn[k] = ledge_support(l, recs[0].as_ref(), dirs[k])[k];
            mx[k] = ledge_support(l, recs[0].as_ref(), dirs[3 + k])[k];
        }
        return (mn, mx);
    }
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for (i, &o) in leaves.iter().enumerate() {
        let l = LedgeRef { b: s, o };
        for d in dirs.iter() {
            let p = ledge_support(l, recs[i].as_ref(), *d);
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

#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn drag_axis_areas(s: &[u8], eps: f32) -> [f32; 3] {
    let (mins, maxs) = aabb_impl(s);
    let ctx = TraceCtx::new(s);
    let mut out = [1.0f32; 3];
    let mut step = eps.sqrt();
    if eps.sqrt() <= f32::from_bits(0x38d1b717) {
        step = f32::from_bits(0x38d1b717);
    }
    let half = step * 0.5f32;
    for axis in 0..3 {
        let a = (axis + 1) % 3;
        let b = (axis + 2) % 3;
        let mut hits = 0i32;
        let mut count = 0i32;
        let mut u = half + mins[a];
        if !(maxs[a] <= u) {
            loop {
                let mut v = half + mins[b];
                if v < maxs[b] {
                    loop {
                        let mut st = [0f32; 3];
                        let mut en = [0f32; 3];
                        st[axis] = mins[axis] - 1.0f32;
                        en[axis] = maxs[axis] + 1.0f32;
                        st[a] = u;
                        en[a] = u;
                        st[b] = v;
                        en[b] = v;
                        let hit = ctx.ray_hit(st, en);
                        if hit {
                            hits += 1;
                        }
                        let next = v + step;
                        count += 1;
                        if !(next < maxs[b]) || next == v {
                            break;
                        }
                        v = next;
                    }
                }
                let next = u + step;
                if !(next < maxs[a]) || next == u {
                    break;
                }
                u = next;
            }
            if count < 1 {
                count = 1;
            }
        } else {
            count = 1;
        }
        out[axis] = hits as f32 / count as f32;
    }
    out
}
