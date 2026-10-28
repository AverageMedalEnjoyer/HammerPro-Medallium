fn main() {
    let mut args = std::env::args().skip(1);
    let input = args.next().expect("bsp");
    let output = args.next().expect("vmf");
    let d = hp_decompile::decompile_file(std::path::Path::new(&input), &hp_decompile::Options::default())
        .expect("decompile");
    std::fs::write(&output, hp_vmf::write(&d.map)).expect("write");
    println!("{:#?}", d.report);
}
