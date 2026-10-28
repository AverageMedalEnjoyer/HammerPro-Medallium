fn main() {
    let path = std::env::args().nth(1).expect("bsp");
    let bsp = hp_bsp::Bsp::open(&path).expect("open");
    let get = |i: usize| bsp.lump(i).unwrap().into_owned();
    let (br, sides, texinfo, texdata, table, strings) = (get(18), get(19), get(6), get(2), get(44), get(43));
    let i = |d: &[u8], o: usize| i32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let name = |ti: i16| -> String {
        if ti < 0 {
            return "-".into();
        }
        let td = i(&texinfo, ti as usize * 72 + 68) as usize;
        let id = i(&texdata, td * 32 + 12) as usize;
        let ofs = i(&table, id * 4) as usize;
        let end = strings[ofs..].iter().position(|b| *b == 0).unwrap();
        String::from_utf8_lossy(&strings[ofs..ofs + end]).into_owned()
    };
    let mut map: std::collections::BTreeMap<i32, (usize, std::collections::BTreeSet<String>)> = Default::default();
    for b in 0..br.len() / 12 {
        let (first, count, contents) = (i(&br, b * 12) as usize, i(&br, b * 12 + 4) as usize, i(&br, b * 12 + 8));
        let e = map.entry(contents).or_default();
        e.0 += 1;
        for s in first..first + count {
            let ti = i16::from_le_bytes(sides[s * 8 + 2..s * 8 + 4].try_into().unwrap());
            if e.1.len() < 6 {
                e.1.insert(name(ti));
            }
        }
    }
    for (c, (n, names)) in map {
        println!("{c:#010x} x{n}: {:?}", names);
    }
}
