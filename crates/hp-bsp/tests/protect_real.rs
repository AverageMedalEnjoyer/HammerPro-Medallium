use std::path::Path;

use hp_bsp::{Bsp, LUMP_COUNT, Lock, ProtectOptions, lock_hash, lump, protect, server_copy};

const MAPS: &str = r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\maps";

#[test]
fn real_maps_server_copy_matches_the_protected_copy() {
    let Ok(dir) = std::fs::read_dir(MAPS) else {
        eprintln!("skipping: {MAPS} not found");
        return;
    };
    let opts = ProtectOptions {
        watermark: "test".into(),
        strip_original_faces: true,
        randomize_materials: true,
        salt: "salt".into(),
        lock: Some(Lock { hash: lock_hash("key"), block: false }),
        ..Default::default()
    };
    let mut checked = 0;
    for entry in dir.flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsp")) {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let full = Bsp::read(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        let (client_bytes, report) = protect(&full, &opts).unwrap_or_else(|e| panic!("{name}: {e}"));
        let server_bytes = server_copy(&client_bytes, &full, &opts).unwrap();
        let client = Bsp::from_vec(client_bytes).unwrap();
        let server = Bsp::from_vec(server_bytes).unwrap();
        for i in (0..LUMP_COUNT).filter(|&i| i != lump::ENTITIES) {
            assert_eq!(client.lump_raw(i).unwrap(), server.lump_raw(i).unwrap(), "{name}: lump {i} differs");
            assert_eq!(client.header().lumps[i].offset, server.header().lumps[i].offset, "{name}: lump {i} moved");
        }
        let props = |b: &Bsp| b.game_lumps().unwrap().lumps.len();
        assert_eq!(props(&client), props(&full), "{name}: game lumps");
        assert_eq!(props(&server), props(&full), "{name}: game lumps");
        assert_eq!(server.entities().unwrap().len(), full.entities().unwrap().len(), "{name}: entities");
        eprintln!(
            "{name}: ok ({} logic entities removed for players, {} materials randomized)",
            report.entities_removed, report.materials_randomized
        );
        checked += 1;
    }
    assert!(checked > 0 || !Path::new(MAPS).is_dir());
}
