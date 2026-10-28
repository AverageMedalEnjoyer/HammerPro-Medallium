struct ByName;

impl hp_bspc::MaterialSource for ByName {
    fn facts(&self, name: &str) -> hp_bspc::MaterialFacts {
        hp_bspc::materials::facts_by_name(name)
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args.next().expect("input vmf");
    let output = args.next().expect("output bsp");
    let text = std::fs::read_to_string(&input).expect("read");
    let map = hp_vmf::parse(&text).expect("parse");
    let mut log = |l: &str| println!("{l}");
    let name = std::path::Path::new(&output).file_stem().map(|s| s.to_string_lossy().into_owned());
    let opts = hp_bspc::CompileOptions { map_name: name, ..Default::default() };
    match hp_bspc::compile(&map, &ByName, &hp_bspc::props::NoModels, &opts, &mut log) {
        Ok(out) => {
            std::fs::write(&output, &out.bsp).expect("write");
            println!("wrote {} ({} bytes), leak: {}", output, out.bsp.len(), out.leak.is_some());
            if let Some(path) = &out.leak {
                let lin: String = path
                    .iter()
                    .map(|p| {
                        format!(
                            "{} {} {}
",
                            p.x, p.y, p.z
                        )
                    })
                    .collect();
                std::fs::write(format!("{output}.lin"), lin).ok();
                for p in path.iter().take(4).chain(path.iter().rev().take(4)) {
                    println!("leak point {:.0} {:.0} {:.0}", p.x, p.y, p.z);
                }
                println!("{} leak points", path.len());
            }
        }
        Err(e) => println!("FAILED: {e}"),
    }
}
