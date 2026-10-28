fn main() {
    let path = std::env::args().nth(1).expect("bsp path");
    let bsp = hp_bsp::Bsp::open(&path).expect("open");
    let g = bsp.game_lumps().expect("game lumps");
    for id in [
        u32::from_be_bytes(*b"sprp"),
        u32::from_be_bytes(*b"dprp"),
        u32::from_be_bytes(*b"dplt"),
        u32::from_be_bytes(*b"dplh"),
    ] {
        if let Some(l) = g.get(id) {
            let d = l.data().unwrap();
            println!("{} version {} flags? size {}", hp_bsp::gamelump::id_text(id), l.version, d.len());
            if id == u32::from_be_bytes(*b"dprp") {
                dprp(&d);
            }
            if id == u32::from_be_bytes(*b"dplt") || id == u32::from_be_bytes(*b"dplh") {
                let n = i32::from_le_bytes(d[0..4].try_into().unwrap());
                println!("  {n} light style records ({} bytes each)", (d.len() - 4) / (n.max(1) as usize));
            }
            if id == u32::from_be_bytes(*b"sprp") {
                let names = i32::from_le_bytes(d[0..4].try_into().unwrap()) as usize;
                let at = 4 + names * 128;
                let leaves = i32::from_le_bytes(d[at..at + 4].try_into().unwrap()) as usize;
                let at2 = at + 4 + leaves * 2;
                let count = i32::from_le_bytes(d[at2..at2 + 4].try_into().unwrap()) as usize;
                let size = (d.len() - at2 - 4) / count.max(1);
                println!("  names {names} leaves {leaves} props {count} record size {size}");
                let b = &d[at2 + 4..at2 + 4 + size];
                println!("  first record {:?}", b);
                let mut seen = std::collections::BTreeMap::new();
                for k in 0..count {
                    let r = &d[at2 + 4 + k * size..at2 + 4 + (k + 1) * size];
                    *seen.entry((r[30], r[31], r[56.min(size)..size].to_vec())).or_insert(0) += 1;
                }
                for ((solid, flags, tail), n) in seen.iter().take(25) {
                    println!("  solid {solid} flags {flags:#x} tail {:?} x{n}", tail);
                }
            }
        }
    }
}

fn dprp(d: &[u8]) {
    let i32_at = |o: usize| i32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let f32_at = |o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let models = i32_at(0) as usize;
    let mut at = 4;
    let mut names = Vec::new();
    for k in 0..models {
        let raw = &d[at + k * 128..at + (k + 1) * 128];
        names.push(String::from_utf8_lossy(&raw[..raw.iter().position(|b| *b == 0).unwrap_or(128)]).into_owned());
    }
    at += models * 128;
    let sprites = i32_at(at) as usize;
    at += 4;
    println!("  {models} models, {sprites} sprites");
    for (k, n) in names.iter().enumerate() {
        println!("    model {k}: {n}");
    }
    for k in 0..sprites {
        let o = at + k * 32;
        let v: Vec<f32> = (0..8).map(|i| f32_at(o + i * 4)).collect();
        println!("    sprite {k}: ul {:?} lr {:?} tex {:?} {:?}", &v[0..2], &v[2..4], &v[4..6], &v[6..8]);
    }
    at += sprites * 32;
    let count = i32_at(at) as usize;
    at += 4;
    println!("  {count} objects, {} bytes each", (d.len() - at) / count.max(1));
    let mut by = std::collections::BTreeMap::new();
    let mut leaves = std::collections::BTreeSet::new();
    let mut sorted = true;
    let mut last_leaf = 0u16;
    for k in 0..count {
        let r = &d[at + k * 52..at + (k + 1) * 52];
        let model = u16::from_le_bytes([r[24], r[25]]);
        let leaf = u16::from_le_bytes([r[26], r[27]]);
        sorted &= leaf >= last_leaf;
        last_leaf = leaf;
        leaves.insert(leaf);
        let (sway, angle, size, orient, ty) = (r[37], r[38], r[39], r[40], r[44]);
        *by.entry((ty, model, orient, sway, angle, size)).or_insert(0) += 1;
        if k < 6 || k + 2 >= count {
            let f = |o: usize| f32::from_le_bytes(r[o..o + 4].try_into().unwrap());
            println!(
                "    #{k}: origin ({:.2} {:.2} {:.2}) angles ({:.2} {:.2} {:.2}) model {model} leaf {leaf} light {:?} styles {} x{} type {ty} scale {:.3} pad {:?} {:?}",
                f(0),
                f(4),
                f(8),
                f(12),
                f(16),
                f(20),
                &r[28..32],
                u32::from_le_bytes(r[32..36].try_into().unwrap()),
                r[36],
                f(48),
                &r[41..44],
                &r[45..48]
            );
        }
    }
    println!("  {} leaves, sorted by leaf: {sorted}", leaves.len());
    for ((ty, model, orient, sway, angle, size), n) in by {
        println!("    type {ty} model {model} orientation {orient} sway {sway} shape {angle}/{size}: {n}");
    }
}
