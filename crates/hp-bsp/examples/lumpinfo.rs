fn main() {
    let path = std::env::args().nth(1).expect("bsp path");
    let bsp = hp_bsp::Bsp::open(&path).expect("open");
    println!("version {} revision {}", bsp.version(), bsp.map_revision());
    for i in 0..hp_bsp::LUMP_COUNT {
        let info = bsp.lump_info(i).unwrap();
        if info.len > 0 || info.version != 0 {
            println!(
                "{i:2} {:28} len {:10} ver {} cc {:?}",
                hp_bsp::lump_name(i),
                info.len,
                info.version,
                info.four_cc
            );
        }
    }
}
