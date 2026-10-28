fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let bsp = hp_bsp::Bsp::open(&a[0]).expect("open");
    let get = |i: usize| bsp.lump(i).unwrap().into_owned();
    let (planes, br, sides, texinfo, texdata, table, strings) =
        (get(1), get(18), get(19), get(6), get(2), get(44), get(43));
    let f = |d: &[u8], o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
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
    for id in &a[1..] {
        let b: usize = id.parse().unwrap();
        let (first, count, contents) = (i(&br, b * 12) as usize, i(&br, b * 12 + 4) as usize, i(&br, b * 12 + 8));
        let mut desc = Vec::new();
        for s in first..first + count {
            let p = u16::from_le_bytes(sides[s * 8..s * 8 + 2].try_into().unwrap()) as usize * 20;
            let ti = i16::from_le_bytes(sides[s * 8 + 2..s * 8 + 4].try_into().unwrap());
            let disp = i16::from_le_bytes(sides[s * 8 + 4..s * 8 + 6].try_into().unwrap());
            let bevel = sides[s * 8 + 6];
            if bevel == 0 {
                desc.push(format!(
                    "({:.2} {:.2} {:.2}) {:.1} {} d{}",
                    f(&planes, p),
                    f(&planes, p + 4),
                    f(&planes, p + 8),
                    f(&planes, p + 12),
                    name(ti),
                    disp
                ));
            }
        }
        println!("brush {b} contents {contents:#x}\n  {}", desc.join("\n  "));
    }
}
