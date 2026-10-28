pub const COLUMN: f32 = 240.0;
pub const ROW: f32 = 64.0;
const GAP: f32 = 80.0;
const SWEEPS: usize = 4;

pub fn layout(n: usize, links: &[(usize, usize)], fixed: &[Option<[f32; 2]>]) -> Vec<[f32; 2]> {
    let mut out = vec![[0.0f32; 2]; n];
    let free: Vec<bool> = (0..n).map(|i| fixed.get(i).copied().flatten().is_none()).collect();
    let mut top = 0.0f32;
    let mut left = 0.0f32;
    let mut any_fixed = false;
    for (i, p) in fixed.iter().enumerate().take(n) {
        if let Some(p) = p {
            out[i] = *p;
            if !any_fixed {
                top = p[1];
                left = p[0];
            }
            any_fixed = true;
            top = top.max(p[1]);
            left = left.min(p[0]);
        }
    }
    let origin = if any_fixed { [left, top + ROW * 2.0] } else { [0.0, 0.0] };

    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut pred: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(a, b) in links {
        if a < n && b < n && a != b && free[a] && free[b] {
            succ[a].push(b);
            pred[b].push(a);
        }
    }
    for v in succ.iter_mut().chain(pred.iter_mut()) {
        v.sort_unstable();
        v.dedup();
    }

    let mut group = vec![usize::MAX; n];
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for start in 0..n {
        if !free[start] || group[start] != usize::MAX {
            continue;
        }
        let g = groups.len();
        let mut members = vec![start];
        group[start] = g;
        let mut k = 0;
        while k < members.len() {
            let v = members[k];
            k += 1;
            for &w in succ[v].iter().chain(pred[v].iter()) {
                if group[w] == usize::MAX {
                    group[w] = g;
                    members.push(w);
                }
            }
        }
        groups.push(members);
    }

    let mut blocks: Vec<Block> = groups.iter().map(|members| layered(members, &succ, &pred)).collect();
    blocks.sort_by_key(|b| std::cmp::Reverse(b.0.len()));
    let total_area: f32 = blocks.iter().map(|(_, s)| (s[0] + GAP) * (s[1] + GAP)).sum();
    let shelf_width = (total_area.sqrt() * 1.6).max(COLUMN * 4.0);
    let (mut x, mut y, mut shelf_h) = (0.0f32, 0.0f32, 0.0f32);
    for (nodes, size) in blocks {
        if x > 0.0 && x + size[0] > shelf_width {
            x = 0.0;
            y += shelf_h + GAP;
            shelf_h = 0.0;
        }
        for (i, p) in nodes {
            out[i] = [origin[0] + x + p[0], origin[1] + y + p[1]];
        }
        x += size[0] + GAP;
        shelf_h = shelf_h.max(size[1]);
    }
    out
}

type Block = (Vec<(usize, [f32; 2])>, [f32; 2]);

fn layered(members: &[usize], succ: &[Vec<usize>], pred: &[Vec<usize>]) -> Block {
    if members.len() == 1 {
        return (vec![(members[0], [0.0, 0.0])], [COLUMN, ROW]);
    }
    let mut state: std::collections::HashMap<usize, u8> = members.iter().map(|&m| (m, 0u8)).collect();
    let mut order = Vec::with_capacity(members.len());
    let mut back: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    let mut roots: Vec<usize> = members.iter().copied().filter(|&m| pred[m].is_empty()).collect();
    roots.extend(members.iter().copied());
    for root in roots {
        if state[&root] != 0 {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        state.insert(root, 1);
        while let Some(&mut (v, ref mut k)) = stack.last_mut() {
            if let Some(&w) = succ[v].get(*k) {
                *k += 1;
                match state[&w] {
                    0 => {
                        state.insert(w, 1);
                        stack.push((w, 0));
                    }
                    1 => {
                        back.insert((v, w));
                    }
                    _ => {}
                }
            } else {
                state.insert(v, 2);
                order.push(v);
                stack.pop();
            }
        }
    }
    order.reverse();
    let mut layer: std::collections::HashMap<usize, usize> = members.iter().map(|&m| (m, 0)).collect();
    for &v in &order {
        let lv = layer[&v];
        for &w in &succ[v] {
            if !back.contains(&(v, w)) && layer[&w] < lv + 1 {
                layer.insert(w, lv + 1);
            }
        }
    }
    let depth = layer.values().copied().max().unwrap_or(0) + 1;
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); depth];
    for &v in &order {
        layers[layer[&v]].push(v);
    }
    let mut pos: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
    for l in &layers {
        for (i, &v) in l.iter().enumerate() {
            pos.insert(v, i as f32);
        }
    }
    for _ in 0..SWEEPS {
        for down in [true, false] {
            let range: Vec<usize> =
                if down { (1..depth).collect() } else { (0..depth.saturating_sub(1)).rev().collect() };
            for li in range {
                let neighbors = |v: usize| if down { &pred[v] } else { &succ[v] };
                let mut keyed: Vec<(f32, usize)> = layers[li]
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| {
                        let ns: Vec<f32> = neighbors(v).iter().filter_map(|w| pos.get(w).copied()).collect();
                        let key = if ns.is_empty() { i as f32 } else { ns.iter().sum::<f32>() / ns.len() as f32 };
                        (key, v)
                    })
                    .collect();
                keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
                layers[li] = keyed.into_iter().map(|(_, v)| v).collect();
                for (i, &v) in layers[li].iter().enumerate() {
                    pos.insert(v, i as f32);
                }
            }
        }
    }
    let tallest = layers.iter().map(Vec::len).max().unwrap_or(1);
    let height = tallest as f32 * ROW;
    let mut out = Vec::with_capacity(members.len());
    for (li, l) in layers.iter().enumerate() {
        let offset = (height - l.len() as f32 * ROW) * 0.5;
        for (i, &v) in l.iter().enumerate() {
            out.push((v, [li as f32 * COLUMN, offset + i as f32 * ROW]));
        }
    }
    (out, [depth as f32 * COLUMN, height])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chains_go_left_to_right() {
        let p = layout(4, &[(0, 1), (1, 2)], &[None; 4]);
        assert!(p[0][0] < p[1][0] && p[1][0] < p[2][0]);
        assert_eq!(p[0][1], p[1][1]);
        assert!(p[3][0] > p[2][0] || p[3][1] > p[2][1]);
    }

    #[test]
    fn cycles_and_fixed_nodes() {
        let fixed = [None, None, None, Some([500.0, 500.0])];
        let p = layout(4, &[(0, 1), (1, 2), (2, 0), (2, 3)], &fixed);
        assert_eq!(p[3], [500.0, 500.0]);
        let mut seen: Vec<(i64, i64)> = p[..3].iter().map(|q| (q[0] as i64, q[1] as i64)).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 3);
        assert!(p[..3].iter().all(|q| q[1] > 500.0));
    }

    #[test]
    fn big_graphs_stay_fast_and_finite() {
        let n = 5000;
        let links: Vec<(usize, usize)> = (0..n - 1).map(|i| (i, (i * 7 + 1) % n)).collect();
        let t = std::time::Instant::now();
        let p = layout(n, &links, &vec![None; n]);
        assert!(t.elapsed().as_secs_f32() < 2.0);
        assert!(p.iter().all(|q| q[0].is_finite() && q[1].is_finite()));
        let p = layout(2, &[(0, 9), (5, 1), (0, 0)], &[None, None]);
        assert_eq!(p.len(), 2);
    }
}
