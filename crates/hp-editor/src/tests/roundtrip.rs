use glam::DVec3;
use hp_vmf::{Entity, Group, Map, Solid, VisGroup};

use super::{cube, sample_vmfs, vmf};
use crate::{Document, ObjRef, Owner};

#[test]
fn sample_vmfs_round_trip() {
    let files = sample_vmfs();
    if files.is_empty() {
        eprintln!("note: sample VMFs not found, skipping");
        return;
    }
    for path in files {
        let src = std::fs::read_to_string(&path).expect("read");
        let map = hp_vmf::parse(&src).expect("parse");
        let expected = hp_vmf::write(&map);
        let t = std::time::Instant::now();
        let doc = Document::from_map(map.clone());
        let load = t.elapsed();
        assert_eq!(doc.to_map(), map, "{}: map differs", path.display());
        assert_eq!(vmf(&doc), expected, "{}: text differs", path.display());
        let stats = doc.stats();
        assert_eq!(stats.solids, map.all_solids().count());
        assert_eq!(stats.entities, map.entities.len());
        eprintln!("{}: {} solids, {} entities, built in {load:?}", path.display(), stats.solids, stats.entities);
        let _ = doc.check_problems(None);
        let mut doc = doc;
        let all: Vec<ObjRef> = doc.scene().units().collect();
        doc.translate(&all, DVec3::new(16.0, 0.0, 0.0)).expect("move");
        assert_ne!(vmf(&doc), expected);
        assert!(doc.undo());
        assert_eq!(vmf(&doc), expected, "{}: undo of move-all differs", path.display());
    }
}

#[test]
fn open_and_save_file() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-editor/scratch");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("roundtrip_save.vmf");
    let mut doc = Document::new_empty();
    doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("create");
    assert!(doc.is_dirty());
    doc.save(&path).expect("save");
    assert!(!doc.is_dirty());
    assert_eq!(doc.path(), Some(path.as_path()));
    let loaded = Document::open(&path).expect("open");
    assert_eq!(loaded.scene().solids().len(), 1);
    assert_eq!(vmf(&loaded), vmf(&doc));
    assert!(doc.undo());
    assert!(doc.is_dirty());
    assert!(doc.redo());
    assert!(!doc.is_dirty());
    doc.save_current().expect("save again");
    assert_eq!(loaded.scene().version_info().map_version + 1, doc.scene().version_info().map_version);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("vmx"));
}

pub(crate) fn feature_map() -> Map {
    let mut map = Map::new_empty();
    let solid = |id: u32, at: DVec3| {
        let mut s = Solid { id, sides: cube(at, 32.0), ..Solid::default() };
        for (i, side) in s.sides.iter_mut().enumerate() {
            side.id = id * 10 + i as u32;
        }
        s
    };
    let mut s2 = solid(2, DVec3::ZERO);
    s2.editor.group_id = Some(50);
    s2.editor.visgroup_ids = vec![7];
    let mut s3 = solid(3, DVec3::new(64.0, 0.0, 0.0));
    s3.hidden = true;
    s3.editor.group_id = Some(50);
    map.world.solids = vec![s2, s3, solid(4, DVec3::new(128.0, 0.0, 0.0))];
    map.world.groups = vec![Group { id: 50, ..Group::default() }];
    let mut door = Entity { id: 10, classname: "func_door".into(), ..Entity::default() };
    door.set("targetname", "door1");
    door.solids = vec![solid(11, DVec3::new(0.0, 64.0, 0.0)), solid(12, DVec3::new(0.0, 128.0, 0.0))];
    let mut light = Entity { id: 20, classname: "light".into(), hidden: true, ..Entity::default() };
    light.set("origin", "0 0 64");
    let mut button = Entity { id: 30, classname: "func_button".into(), ..Entity::default() };
    button.solids = vec![solid(31, DVec3::new(0.0, 256.0, 0.0))];
    button.connections.push(hp_vmf::Connection::parse("OnPressed", "door1,Open,,0,-1"));
    map.entities = vec![door, light, button];
    map.visgroups = vec![VisGroup {
        name: "Detail".into(),
        id: 7,
        color: [1, 2, 3],
        children: vec![VisGroup { name: "Sub".into(), id: 8, ..VisGroup::default() }],
        ..VisGroup::default()
    }];
    map.extra_blocks.push(hp_kv::Block::new("palette_plus"));
    map
}

#[test]
fn synthetic_round_trip() {
    let map = feature_map();
    let map = hp_vmf::parse(&hp_vmf::write(&map)).expect("parse");
    let doc = Document::from_map(map.clone());
    assert_eq!(doc.to_map(), map);
    assert_eq!(doc.scene().entity_solid_ids(10), vec![11, 12]);
    assert!(doc.scene().is_brush_entity(30));
    assert!(!doc.scene().is_brush_entity(20));
    assert_eq!(doc.peek_next_object_id(), 51);
    assert_eq!(doc.peek_next_side_id(), 316);
    assert!(!doc.is_dirty());
    assert!(doc.pending_changes().everything);
}

#[test]
fn new_empty_round_trip() {
    let doc = Document::new_empty();
    let map = Map::new_empty();
    assert_eq!(doc.to_map(), map);
    assert_eq!(vmf(&doc), hp_vmf::write(&map));
    let parsed = Document::from_map(hp_vmf::parse(&vmf(&doc)).expect("parse"));
    assert_eq!(vmf(&parsed), vmf(&doc));
}

#[test]
fn duplicate_ids_are_renumbered() {
    let mut map = Map::new_empty();
    let mut a = Solid { id: 5, sides: cube(DVec3::ZERO, 16.0), ..Solid::default() };
    a.sides.iter_mut().enumerate().for_each(|(i, s)| s.id = i as u32 + 1);
    let b = Solid { id: 5, sides: cube(DVec3::new(32.0, 0.0, 0.0), 16.0), ..Solid::default() };
    map.world.solids = vec![a, b];
    map.entities = vec![
        Entity { id: 9, classname: "info_target".into(), ..Entity::default() },
        Entity { id: 9, classname: "info_target".into(), ..Entity::default() },
    ];
    let doc = Document::from_map(map);
    assert_eq!(doc.scene().solids().len(), 2);
    assert_eq!(doc.scene().entities().len(), 2);
    let problems = doc.check_problems(None);
    assert!(problems.iter().any(|p| p.code == "renumbered_ids" && p.objects.len() == 2));
    assert!(problems.iter().any(|p| p.code == "duplicate_side_id"));
}

#[test]
fn hostile_input_does_not_panic() {
    let mut map = Map::new_empty();
    let mut bad = Solid { id: 2, sides: cube(DVec3::ZERO, 16.0), ..Solid::default() };
    bad.sides[0].plane = [DVec3::ZERO, DVec3::ZERO, DVec3::ZERO];
    bad.sides[1].plane[0] = DVec3::new(f64::NAN, 0.0, 0.0);
    let mut disp = Solid { id: 3, sides: cube(DVec3::ZERO, 16.0), ..Solid::default() };
    let mut d = hp_vmf::DispInfo::new_flat(3, DVec3::ZERO, DVec3::Z);
    d.distances.truncate(3);
    d.power = 9;
    disp.sides[0].disp = Some(Box::new(d));
    map.world.solids = vec![bad, disp, Solid { id: 4, ..Solid::default() }];
    let mut doc = Document::from_map(map);
    let ray = hp_geom::Ray::new(DVec3::new(8.0, 8.0, 100.0), DVec3::NEG_Z);
    let _ = doc.pick_ray(&ray, &crate::PickOptions::default(), &super::no_bounds);
    let _ = doc.pick_2d(hp_geom::ViewAxis::Top, glam::DVec2::new(8.0, 8.0), 4.0, &super::no_bounds);
    let problems = doc.check_problems(None);
    assert!(problems.iter().any(|p| p.code == "invalid_solid"));
    let all: Vec<ObjRef> = doc.scene().units().collect();
    let _ = doc.translate(&all, DVec3::ONE);
    let _ = doc.rotate_axis(&all, DVec3::ZERO, DVec3::Z, 45.0);
    let _ = doc.copy(&all);
    let _ = doc.stats();
    let _ = doc.to_vmf_string();
}

#[test]
fn hammer_plus_plus_ropes_round_trip() {
    let mut src = String::from(
        "versioninfo\n{\n\t\"editorversion\" \"400\"\n\t\"editorbuild\" \"10257\"\n\t\"mapversion\" \"3\"\n\t\"formatversion\" \"100\"\n\t\"prefab\" \"0\"\n}\nworld\n{\n\t\"id\" \"1\"\n\t\"mapversion\" \"3\"\n\t\"classname\" \"worldspawn\"\n\t\"skyname\" \"sky_day01_01\"\n}\n",
    );
    let keys = |name: &str, next: &str, extra: &str| {
        format!(
            "\t\"Barbed\" \"0\"\n\t\"Breakable\" \"0\"\n\t\"Collide\" \"0\"\n\t\"Dangling\" \"0\"\n\t\"MoveSpeed\" \"64\"\n\t\"NextKey\" \"{next}\"\n\t\"NoWind\" \"0\"\n\t\"origin\" \"-640 0 420\"\n{extra}\t\"RopeMaterial\" \"cable/cable.vmt\"\n\t\"Slack\" \"25\"\n\t\"spawnflags\" \"0\"\n\t\"Subdiv\" \"2\"\n\t\"targetname\" \"{name}\"\n\t\"TextureScale\" \"1\"\n\t\"Type\" \"0\"\n\t\"Width\" \"2\"\n"
        )
    };
    let editor = "\teditor\n\t{\n\t\t\"color\" \"220 30 220\"\n\t\t\"visgroupshown\" \"1\"\n\t\t\"visgroupautoshown\" \"1\"\n\t\t\"logicalpos\" \"[0 0]\"\n\t}\n";
    src += &format!(
        "entity\n{{\n\t\"id\" \"2\"\n\t\"classname\" \"move_rope\"\n{}\tconnections\n\t{{\n\t\t\"OnBreak\" \"r2\x1bBreak\x1b\x1b0\x1b-1\"\n\t}}\n{editor}}}\n",
        keys("r1", "r2", "\t\"PositionInterpolator\" \"2\"\n")
    );
    src += &format!(
        "entity\n{{\n\t\"id\" \"3\"\n\t\"classname\" \"keyframe_rope\"\n{}{editor}}}\n",
        keys("r2", "r3", "\t\"parentname\" \"mover\"\n")
    );
    src +=
        &format!("entity\n{{\n\t\"id\" \"4\"\n\t\"classname\" \"keyframe_rope\"\n{}{editor}}}\n", keys("r3", "", ""));
    src += "cameras\n{\n\t\"activecamera\" \"-1\"\n}\ncordons\n{\n\t\"active\" \"0\"\n}\n";
    let map = hp_vmf::parse(&src).expect("parse");
    let doc = Document::from_map(map.clone());
    let saved = vmf(&doc);
    let again = hp_vmf::parse(&saved).expect("parse saved");
    assert_eq!(again, map, "nothing lost or changed");
    let original: Vec<&Entity> = map.entities.iter().collect();
    assert_eq!(original.len(), 3);
    for (a, b) in original.iter().zip(&again.entities) {
        assert_eq!(a.classname, b.classname);
        assert_eq!(a.props, b.props, "keys in order, values as written");
        assert_eq!(a.connections, b.connections);
    }
    let r1 = &again.entities[0];
    assert_eq!(r1.props.len(), 17, "{:?}", r1.props);
    assert_eq!(r1.get("NextKey"), Some("r2"));
    assert_eq!(r1.get("PositionInterpolator"), Some("2"));
    assert_eq!(r1.connections[0].separator, hp_vmf::CONNECTION_ESC);
    assert_eq!(again.entities[1].get("parentname"), Some("mover"));
    assert_eq!(again.entities[2].get("NextKey"), Some(""));
    assert_eq!(hp_vmf::write(&again), saved);
}
