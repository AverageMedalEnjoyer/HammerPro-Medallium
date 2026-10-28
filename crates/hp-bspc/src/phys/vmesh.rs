use super::surface;

type V3 = [f32; 3];

const K: f32 = 0.0254;

fn rd(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

pub fn virtual_mesh(verts: &[V3], indices: &[u16]) -> Option<Vec<u8>> {
    if indices.len() < 3 || indices.iter().any(|&i| usize::from(i) >= verts.len()) {
        return None;
    }
    let count = indices.len();
    let whole = hull_of(verts, indices, 0, count)?;
    let hulls = match pack_map(&whole, verts) {
        Some(_) => vec![whole],
        None => {
            let half = count / 2;
            vec![hull_of(verts, indices, 0, half)?, hull_of(verts, indices, half, half)?]
        }
    };
    let mut out = vec![hulls.len() as u8, 0, 0, 0];
    let mut heads = Vec::new();
    for h in &hulls {
        let n = n_tris(h);
        out.extend_from_slice(&[n as u8, 0, ((n * 3) >> 1) as u8, 0, 0]);
        heads.push(out.len() - 5);
    }
    for (h, at) in hulls.iter().zip(heads) {
        pack(h, verts, &mut out, at);
    }
    Some(out)
}

fn n_tris(l: &[u8]) -> usize {
    i16::from_le_bytes([l[12], l[13]]).max(0) as usize
}

#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn hull_of(verts: &[V3], idx: &[u16], start: usize, count: usize) -> Option<Vec<u8>> {
    let v = |k: usize| verts[usize::from(idx[k])];
    let (v0, v1, v2) = (v(0), v(1), v(2));
    let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
    let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
    let mut n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
    let inv = 1.0f32 / (((n[0] * n[0] + n[1] * n[1]) + n[2] * n[2]).sqrt() + f32::EPSILON);
    n = n.map(|c| c * inv);
    let dot = |p: V3| (n[1] * p[1] + n[0] * p[0]) + n[2] * p[2];
    let d0 = dot(v0);
    let flat = (0..count.min(idx.len())).all(|k| !(dot(v(k)) - d0 > 0.01));
    let lift = n.map(|c| c * 0.09);
    let to_ivp = |p: V3| [p[0] * K, -(p[2] * K), p[1] * K];
    let mut ledges = Vec::new();
    for t in start / 3..(start + count) / 3 {
        if 3 * t + 2 >= idx.len() {
            break;
        }
        let mut a = v(3 * t);
        if flat && t == start / 3 {
            a = [a[0] + lift[0], a[1] + lift[1], a[2] + lift[2]];
        }
        if let Some(l) = surface::triangle_ledge(to_ivp(a), to_ivp(v(3 * t + 1)), to_ivp(v(3 * t + 2))) {
            ledges.push(l);
        }
    }
    if ledges.is_empty() {
        return None;
    }
    let s = surface::soup_surface(ledges, &mut |p| super::ledge::pointsoup_to_ledge(p, &mut super::ledge::ivp_qhull))?;
    let root = rd(&s, 32) as usize;
    let lo = rd(&s, root + 4) as i32;
    if lo == 0 {
        return None;
    }
    let at = (root as isize + lo as isize) as usize;
    let nt = n_tris(&s[at..]);
    let po = (at as isize + rd(&s, at) as i32 as isize) as usize;
    let max_pt = (0..nt)
        .flat_map(|t| (0..3).map(move |e| (t, e)))
        .map(|(t, e)| (rd(&s, at + 16 * (t + 1) + 4 + 4 * e) & 0xffff) as usize)
        .max()?;
    let mut l = s[at..at + 16 * (nt + 1)].to_vec();
    let base = l.len();
    l[0..4].copy_from_slice(&(base as i32).to_le_bytes());
    l.extend_from_slice(s.get(po..po + 16 * (max_pt + 1))?);
    Some(l)
}

fn pack_map(l: &[u8], verts: &[V3]) -> Option<()> {
    if n_tris(l) * 3 > 0x200 {
        return None;
    }
    let (_, lo, hi) = point_map(l, verts);
    (hi - lo < 0x100).then_some(())
}

fn point_map(l: &[u8], verts: &[V3]) -> (Vec<i32>, i32, i32) {
    const K_INV: f32 = 39.370_08;
    let nt = n_tris(l);
    let sp = |t: usize, e: usize| (rd(l, 16 * (t + 1) + 4 + 4 * e) & 0xffff) as usize;
    let max = (0..nt).flat_map(|t| (0..3).map(move |e| (t, e))).map(|(t, e)| sp(t, e)).max().unwrap_or(0);
    let mut map = vec![-1i32; max + 1];
    let mut lo = verts.len() as i32;
    let mut hi = 0i32;
    let po = rd(l, 0) as usize;
    for t in 0..nt {
        for e in 0..3 {
            let p = sp(t, e);
            if map[p] >= 0 {
                continue;
            }
            let q = |k: usize| f32::from_bits(rd(l, po + 16 * p + 4 * k));
            let (px, py, pz) = (q(0) * K_INV, -(q(1) * K_INV), q(2) * K_INV);
            let mut best = f32::from_bits(0x6753_c21c);
            let mut at = -1i32;
            for (i, v) in verts.iter().enumerate() {
                let dx = px - v[0];
                let dz = py - v[2];
                let dy = pz - v[1];
                let d = ((dy * dy + dx * dx) + dz * dz).sqrt();
                if d < best {
                    best = d;
                    at = i as i32;
                }
            }
            map[p] = at;
            lo = lo.min(at);
            hi = hi.max(at);
        }
    }
    (map, lo, hi)
}

fn pack(l: &[u8], verts: &[V3], out: &mut Vec<u8>, head: usize) {
    let nt = n_tris(l);
    let (map, lo, _) = point_map(l, verts);
    let tri = |t: usize| rd(l, 16 * (t + 1));
    let edge = |slot: usize| rd(l, 16 + 4 * slot);
    let mut order: Vec<usize> = (0..nt).filter(|&t| tri(t) & 0x8000_0000 != 0).collect();
    let nvirt = order.len();
    order.extend((0..nt).filter(|&t| tri(t) & 0x8000_0000 == 0));
    let mut new_of = vec![0usize; nt];
    for (k, &t) in order.iter().enumerate() {
        new_of[t] = k;
    }
    let twin = |slot: usize| {
        let opp = ((edge(slot) << 1) as i32) >> 17;
        (slot as isize + opp as isize) as usize
    };
    let mut id = vec![-1i32; nt * 4];
    let mut list: Vec<usize> = Vec::new();
    for pass_virtual in [true, false] {
        for &t in &order {
            for e in 0..3 {
                let slot = 4 * t + 1 + e;
                let virt = edge(slot) & 0x8000_0000 != 0;
                let tw = twin(slot);
                if virt == pass_virtual && id.get(tw).is_some_and(|&x| x < 0) {
                    id[slot] = list.len() as i32;
                    list.push(slot);
                }
                if !pass_virtual && id[slot] < 0 {
                    id[slot] = id.get(tw).copied().unwrap_or(-1);
                }
            }
        }
        if pass_virtual {
            out[head + 3] = list.len() as u8;
        }
    }
    out[head + 1] = nvirt as u8;
    out[head + 4] = lo as u8;
    for &t in &order {
        let pierce = ((tri(t) >> 12) & 0xfff) as usize;
        out.push(id[4 * t + 1] as u8);
        out.push(id[4 * t + 2] as u8);
        out.push(id[4 * t + 3] as u8);
        out.push(new_of.get(pierce).copied().unwrap_or(0) as u8);
    }
    let start = |slot: usize| (edge(slot) & 0xffff) as usize;
    for &slot in &list {
        let next = if slot % 4 == 3 { slot - 2 } else { slot + 1 };
        out.push((map[start(slot)] - lo) as u8);
        out.push((map[start(next)] - lo) as u8);
    }
}
