fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let bsp = hp_bsp::Bsp::open(&a[0]).expect("open");
    let p: Vec<f32> = a[1..4].iter().map(|v| v.parse().unwrap()).collect();
    let get = |i: usize| bsp.lump(i).unwrap().into_owned();
    let (planes, nodes, leafs, models) = (get(1), get(5), get(10), get(14));
    let f = |d: &[u8], o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let i = |d: &[u8], o: usize| i32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let mut n = i(&models, 36);
    while n >= 0 {
        let o = n as usize * 32;
        let pl = i(&nodes, o) as usize * 20;
        let d = f(&planes, pl) * p[0] + f(&planes, pl + 4) * p[1] + f(&planes, pl + 8) * p[2] - f(&planes, pl + 12);
        n = if d >= 0.0 { i(&nodes, o + 4) } else { i(&nodes, o + 8) };
    }
    let leaf = (-1 - n) as usize;
    let o = leaf * 32;
    let first = u16::from_le_bytes(leafs[o + 24..o + 26].try_into().unwrap()) as usize;
    let count = u16::from_le_bytes(leafs[o + 26..o + 28].try_into().unwrap()) as usize;
    let lb = get(17);
    let br = get(18);
    let brushes: Vec<(u16, i32)> = (first..first + count)
        .map(|k| {
            let b = u16::from_le_bytes(lb[k * 2..k * 2 + 2].try_into().unwrap());
            (b, i(&br, b as usize * 12 + 8))
        })
        .collect();
    println!(
        "leaf {leaf} contents {:#x} cluster {} brushes {:?}",
        i(&leafs, o),
        i16::from_le_bytes(leafs[o + 4..o + 6].try_into().unwrap()),
        brushes
    );
}
