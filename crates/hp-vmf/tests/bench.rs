use std::path::PathBuf;
use std::time::Instant;

use glam::DVec3;
use hp_vmf::{Connection, EditorInfo, Entity, Map, Side, Solid, TexAxis};

fn cube(id: u32, first_side: u32, min: DVec3, max: DVec3) -> Solid {
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
                    material: "CONCRETE/CONCRETEFLOOR001A".into(),
                    uaxis: TexAxis { axis: u, shift: 0.0, scale: 0.25 },
                    vaxis: TexAxis { axis: v, shift: 0.0, scale: 0.25 },
                    ..Side::default()
                }
            })
            .collect(),
        editor: EditorInfo { color: [0, 180, 255], ..EditorInfo::default() },
        ..Solid::default()
    }
}

fn big_map(solids: u32, entities: u32) -> Map {
    let mut map = Map::new_empty();
    let mut next_id = 2u32;
    let mut next_side = 1u32;
    map.world.solids.reserve(solids as usize);
    for i in 0..solids {
        let x = f64::from(i % 400) * 64.0 - 12_800.0;
        let y = f64::from((i / 400) % 400) * 64.0 - 12_800.0;
        let z = f64::from(i / 160_000) * 64.0;
        map.world.solids.push(cube(next_id, next_side, DVec3::new(x, y, z), DVec3::new(x + 48.0, y + 48.0, z + 32.5)));
        next_id += 1;
        next_side += 6;
    }
    for i in 0..entities {
        let mut e = Entity {
            id: next_id,
            classname: if i % 3 == 0 { "prop_static".into() } else { "info_target".into() },
            props: vec![
                ("origin".into(), format!("{} {} {}", i % 1000, i / 1000, 16)),
                ("angles".into(), "0 90 0".into()),
                ("targetname".into(), format!("ent_{i}")),
                ("model".into(), "models/props_c17/oildrum001.mdl".into()),
            ],
            ..Entity::default()
        };
        if i % 4 == 0 {
            e.connections.push(Connection {
                output: "OnUser1".into(),
                target: format!("ent_{}", i + 1),
                input: "FireUser1".into(),
                delay: 0.5,
                separator: '\x1b',
                ..Connection::default()
            });
        }
        e.editor.logical_pos = Some("[0 0]".into());
        map.entities.push(e);
        next_id += 1;
    }
    map
}

#[test]
#[ignore = "benchmark: run in release with --ignored --nocapture"]
fn big_map_parse_and_write() {
    let t0 = Instant::now();
    let map = big_map(100_000, 10_000);
    eprintln!("generate 100k solids + 10k entities: {:?}", t0.elapsed());

    let _ = hp_vmf::write(&Map::new_empty());

    let t0 = Instant::now();
    let text = hp_vmf::write(&map);
    let write_time = t0.elapsed();
    eprintln!("write: {:?} ({:.1} MB)", write_time, text.len() as f64 / 1e6);

    let t0 = Instant::now();
    let parsed = hp_vmf::parse(&text).unwrap();
    let parse_time = t0.elapsed();
    eprintln!("parse (parallel spans): {parse_time:?}");
    assert_eq!(parsed.world.solids.len(), 100_000);
    assert_eq!(parsed.entities.len(), 10_000);
    assert!(parsed == map, "big map round trip");

    let t0 = Instant::now();
    let tree = hp_kv::parse(&text).unwrap();
    let kv_time = t0.elapsed();
    let t0 = Instant::now();
    let reference = hp_vmf::from_kv(tree).unwrap();
    eprintln!("reference: hp_kv::parse {kv_time:?} + from_kv {:?}", t0.elapsed());
    assert!(reference == parsed);

    let t0 = Instant::now();
    let again = hp_vmf::write(&parsed);
    eprintln!("write again: {:?}", t0.elapsed());
    assert!(again == text);

    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-vmf/scratch/bench");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("big.vmf");
    let t0 = Instant::now();
    hp_vmf::save(&parsed, &path).unwrap();
    eprintln!("save (write + atomic replace, fsync): {:?}", t0.elapsed());
    let t0 = Instant::now();
    let loaded = hp_vmf::load(&path).unwrap();
    eprintln!("load (read + parse): {:?}", t0.elapsed());
    assert_eq!(loaded.world.solids.len(), 100_000);

    assert!(write_time.as_secs_f64() < 1.0, "write must stay well under a second");
}
