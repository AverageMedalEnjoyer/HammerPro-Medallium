use std::collections::HashMap;

use hp_bspc::detail_props::{BspLumps, DetailLump, sample_points};

fn main() {
    let path = std::env::args().nth(1).expect("bsp path");
    let per_face: usize = std::env::args().nth(2).and_then(|v| v.parse().ok()).unwrap_or(6000);
    let bsp = hp_bsp::Bsp::open(&path).expect("open");
    let lumps = BspLumps::of_bsp(&bsp);
    let games = bsp.game_lumps().expect("game lumps");
    let dprp = games.get(hp_bspc::detail_props::DPRP).expect("no dprp");
    let lump = DetailLump::parse(&dprp.data().unwrap()).expect("dprp");
    println!("{} objects, {} models, {} sprites", lump.objects.len(), lump.models.len(), lump.sprites.len());
    let cell = |p: glam::Vec3| ((p.x / 8.0).floor() as i32, (p.y / 8.0).floor() as i32, (p.z / 8.0).floor() as i32);
    let mut grid: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
    for (k, o) in lump.objects.iter().enumerate() {
        grid.entry(cell(o.origin)).or_default().push(k);
    }
    let faces = bsp.lump(hp_bspc::emit::lump::FACES).map_or(0, |d| d.len() / 56);
    let mut matched = vec![false; lump.objects.len()];
    let (mut disp_faces, mut brush_faces) = (0, 0);
    let mut steps: HashMap<usize, usize> = HashMap::new();
    for f in 0..faces {
        let corners = lumps.face_points(f);
        if corners.is_empty() {
            continue;
        }
        let (mut lo, mut hi) = (corners[0], corners[0]);
        for c in &corners {
            lo = lo.min(*c);
            hi = hi.max(*c);
        }
        let (lo, hi) = (cell(lo - glam::Vec3::splat(512.0)), cell(hi + glam::Vec3::splat(512.0)));
        let near = (hi.0 - lo.0 + 1) as i64 * (hi.1 - lo.1 + 1) as i64 * (hi.2 - lo.2 + 1) as i64 > grid.len() as i64
            || (lo.0..=hi.0).any(|x| (lo.1..=hi.1).any(|y| (lo.2..=hi.2).any(|z| grid.contains_key(&(x, y, z)))));
        if !near {
            continue;
        }
        let pts = sample_points(&lumps, f, per_face);
        let mut hits = Vec::new();
        for (i, p) in pts {
            let (cx, cy, cz) = cell(p);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        let Some(list) = grid.get(&(cx + dx, cy + dy, cz + dz)) else { continue };
                        for &k in list {
                            if (lump.objects[k].origin - p).abs().max_element() < 0.02 && !matched[k] {
                                matched[k] = true;
                                hits.push(i);
                            }
                        }
                    }
                }
            }
        }
        if !hits.is_empty() {
            let disp = i16::from_le_bytes(lumps.faces[f * 56 + 12..f * 56 + 14].try_into().unwrap()) >= 0;
            if disp {
                disp_faces += 1;
            } else {
                brush_faces += 1;
            }
            hits.sort_unstable();
            for w in hits.windows(2) {
                *steps.entry(w[1] - w[0]).or_default() += 1;
            }
        }
    }
    let n = matched.iter().filter(|m| **m).count();
    println!(
        "{n} of {} objects ({:.2}%) sit on a sample point of a face ({disp_faces} displacements, {brush_faces} brush faces)",
        lump.objects.len(),
        100.0 * n as f64 / lump.objects.len().max(1) as f64
    );
    let mut s: Vec<_> = steps.into_iter().collect();
    s.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    println!("random numbers between consecutive objects: {:?}", &s[..s.len().min(10)]);
}
