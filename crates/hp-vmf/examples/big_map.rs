use glam::DVec3;
use hp_vmf::{Entity, Map, Side, Solid, TexAxis};

fn cube(id: u32, first_side: u32, min: DVec3, max: DVec3, material: &str) -> Solid {
    let (a, b) = (min, max);
    let planes = [
        [DVec3::new(a.x, b.y, b.z), DVec3::new(b.x, b.y, b.z), DVec3::new(b.x, a.y, b.z)],
        [DVec3::new(a.x, a.y, a.z), DVec3::new(b.x, a.y, a.z), DVec3::new(b.x, b.y, a.z)],
        [DVec3::new(a.x, b.y, b.z), DVec3::new(a.x, a.y, b.z), DVec3::new(a.x, a.y, a.z)],
        [DVec3::new(b.x, b.y, a.z), DVec3::new(b.x, a.y, a.z), DVec3::new(b.x, a.y, b.z)],
        [DVec3::new(b.x, b.y, b.z), DVec3::new(a.x, b.y, b.z), DVec3::new(a.x, b.y, a.z)],
        [DVec3::new(b.x, a.y, a.z), DVec3::new(a.x, a.y, a.z), DVec3::new(a.x, a.y, b.z)],
    ];
    let axes = [(DVec3::X, DVec3::NEG_Y), (DVec3::X, DVec3::NEG_Y), (DVec3::Y, DVec3::NEG_Z)];
    Solid {
        id,
        sides: planes
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let (u, v) = axes[i / 2];
                Side {
                    id: first_side + i as u32,
                    plane: *p,
                    material: material.to_owned(),
                    uaxis: TexAxis { axis: u, shift: 0.0, scale: 0.25 },
                    vaxis: TexAxis { axis: v, shift: 0.0, scale: 0.25 },
                    ..Side::default()
                }
            })
            .collect(),
        ..Solid::default()
    }
}

const MATERIALS: [&str; 4] =
    ["CONCRETE/CONCRETEFLOOR001A", "BRICK/BRICKWALL001A", "WOOD/WOODFLOOR005A", "METAL/METALWALL001A"];

fn big_map(solids: u32, entities: u32) -> Map {
    let mut map = Map::new_empty();
    let mut next_id = 2u32;
    let mut next_side = 1u32;
    let side = (f64::from(solids).sqrt().ceil() as u32).max(1);
    map.world.solids.reserve(solids as usize);
    for i in 0..solids {
        let x = f64::from(i % side) * 64.0 - f64::from(side) * 32.0;
        let y = f64::from((i / side) % side) * 64.0 - f64::from(side) * 32.0;
        let z = f64::from(i / (side * side)) * 64.0;
        let material = MATERIALS[i as usize % MATERIALS.len()];
        let max = DVec3::new(x + 48.0, y + 48.0, z + 32.0 + f64::from(i % 7) * 16.0);
        map.world.solids.push(cube(next_id, next_side, DVec3::new(x, y, z), max, material));
        next_id += 1;
        next_side += 6;
    }
    for i in 0..entities {
        let x = f64::from(i % side) * 64.0 - f64::from(side) * 32.0 + 24.0;
        let y = f64::from((i / side) % side) * 64.0 - f64::from(side) * 32.0 + 24.0;
        let (classname, model) = if i % 3 == 0 {
            ("prop_static", Some("models/props_c17/oildrum001.mdl"))
        } else if i % 3 == 1 {
            ("light", None)
        } else {
            ("info_target", None)
        };
        let mut props = vec![("origin".to_owned(), format!("{x} {y} 160")), ("angles".to_owned(), "0 90 0".to_owned())];
        if let Some(m) = model {
            props.push(("model".to_owned(), m.to_owned()));
        }
        props.push(("targetname".to_owned(), format!("ent_{i}")));
        map.entities.push(Entity { id: next_id, classname: classname.to_owned(), props, ..Entity::default() });
        next_id += 1;
    }
    if entities > 0 {
        map.entities.push(Entity {
            id: next_id,
            classname: "info_player_start".to_owned(),
            props: vec![("origin".to_owned(), "0 0 256".to_owned()), ("angles".to_owned(), "0 0 0".to_owned())],
            ..Entity::default()
        });
    }
    map
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(out) = args.next() else {
        eprintln!("usage: big_map <out.vmf> [solids] [entities]");
        std::process::exit(2);
    };
    let solids = args.next().and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let entities = args.next().and_then(|s| s.parse().ok()).unwrap_or(10_000);
    let map = big_map(solids, entities);
    if let Err(e) = hp_vmf::save(&map, std::path::Path::new(&out)) {
        eprintln!("error writing {out}: {e}");
        std::process::exit(1);
    }
    println!("{out}: {solids} solids, {entities} entities");
}
