use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(input) = args.next() else {
        eprintln!("usage: vmf_roundtrip <in.vmf> [out.vmf]");
        std::process::exit(2);
    };
    let t0 = Instant::now();
    let map = match hp_vmf::load(std::path::Path::new(&input)) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let load = t0.elapsed();
    let t0 = Instant::now();
    let text = hp_vmf::write(&map);
    let write = t0.elapsed();
    println!(
        "{input}: load {load:?}, write {write:?} ({} bytes), {} entities, {} solids, {} sides, max id {}",
        text.len(),
        map.entities.len(),
        map.all_solids().count(),
        map.all_solids().map(|s| s.sides.len()).sum::<usize>(),
        map.max_object_id()
    );
    if let Some(output) = args.next()
        && let Err(e) = std::fs::write(&output, text)
    {
        eprintln!("error writing {output}: {e}");
        std::process::exit(1);
    }
}
