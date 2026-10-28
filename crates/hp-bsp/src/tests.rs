use std::path::{Path, PathBuf};

use crate::gamelump::id;
use crate::*;

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/hp-bsp/scratch")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sample_game_lumps() -> Vec<u8> {
    let mut g = GameLumps::default();
    g.set(id::STATIC_PROPS, 10, vec![1, 2, 3, 4, 5, 6, 7]).unwrap();
    g.set(id::DETAIL_PROPS, 4, vec![8; 9]).unwrap();
    g.to_bytes(0).unwrap()
}

fn sample_pak() -> Vec<u8> {
    let mut w = PakWriter::new();
    w.add("materials/maps/gm_t/c0_0_64.vtf", vec![5; 300]).unwrap();
    w.add("maps/gm_t_particles.txt", b"particles_manifest {}".to_vec()).unwrap();
    w.finish(PakOptions::default()).unwrap()
}

fn sample() -> Vec<u8> {
    synthetic_bsp(
        20,
        &[
            (lump::ENTITIES, b"{\n\"classname\" \"worldspawn\"\n}\n\0".to_vec()),
            (lump::PLANES_FOR_TEST, vec![1; 21]),
            (lump::GAME_LUMP, sample_game_lumps()),
            (lump::PAKFILE, sample_pak()),
            (lump::CUBEMAPS, [0i32, 0, 64, 0].iter().flat_map(|v| v.to_le_bytes()).collect()),
        ],
    )
}

mod lump {
    pub use crate::lump::*;
    pub const PLANES_FOR_TEST: usize = 1;
}

#[test]
fn reads_every_part() {
    let bsp = Bsp::from_vec(sample()).unwrap();
    assert_eq!(bsp.version(), 20);
    assert_eq!(bsp.entities().unwrap()[0].classname(), "worldspawn");
    assert_eq!(bsp.lump(lump::PLANES_FOR_TEST).unwrap().as_ref(), &[1; 21][..]);
    let g = bsp.game_lumps().unwrap();
    assert_eq!(g.get(id::STATIC_PROPS).unwrap().stored, vec![1, 2, 3, 4, 5, 6, 7]);
    let pak = bsp.pakfile().unwrap();
    assert_eq!(pak.len(), 2);
    assert_eq!(bsp.cubemaps().unwrap(), vec![([0, 0, 64], 0)]);
    assert!(!bsp.any_compressed());
}

#[test]
fn unchanged_edit_keeps_every_lump() {
    let bytes = sample();
    let bsp = Bsp::from_vec(bytes.clone()).unwrap();
    let out = bsp.edit().to_vec().unwrap();
    assert_eq!(out, bytes, "a file written in order round trips byte for byte");
}

#[test]
fn replacing_a_lump_moves_the_others_and_fixes_game_lumps() {
    let bsp = Bsp::from_vec(sample()).unwrap();
    let mut edit = bsp.edit();
    let mut ents = edit.source().entities().unwrap();
    ents.push(Entity {
        pairs: vec![("classname".into(), "info_player_start".into()), ("origin".into(), "0 0 8".into())],
    });
    edit.set_entities(&ents).unwrap();
    edit.replace_lump(lump::PLANES_FOR_TEST, vec![2; 5]).unwrap();
    let out = Bsp::from_vec(edit.to_vec().unwrap()).unwrap();
    assert_eq!(out.entities().unwrap().len(), 2);
    assert_eq!(out.lump(lump::PLANES_FOR_TEST).unwrap().as_ref(), &[2; 5][..]);
    let g = out.game_lumps().unwrap();
    assert_eq!(g.get(id::DETAIL_PROPS).unwrap().stored, vec![8; 9]);
    let raw = out.lump_raw(lump::GAME_LUMP).unwrap();
    let first = u32::from_le_bytes(raw[12..16].try_into().unwrap());
    assert_eq!(first, out.lump_info(lump::GAME_LUMP).unwrap().offset + 4 + 2 * 16);
    let h = out.header();
    assert!(h.lumps.iter().all(|l| l.offset % 4 == 0));
    let pak = h.lumps[lump::PAKFILE];
    assert_eq!((pak.offset + pak.len) as usize, out.len());
    assert_eq!(out.pakfile().unwrap().len(), 2);
}

#[test]
fn pakfile_replace_and_atomic_save_in_place() {
    let dir = scratch("save");
    let path = dir.join("gm_t.bsp");
    std::fs::write(&path, sample()).unwrap();
    let bsp = Bsp::open(&path).unwrap();
    let mut w = bsp.pakfile().unwrap().to_writer().unwrap();
    w.add("materials/custom/a.vmt", b"\"LightmappedGeneric\" {}".repeat(20)).unwrap();
    w.remove("maps/gm_t_particles.txt");
    let zip = w.finish(PakOptions { compression: PakCompression::Lzma, level: 5 }).unwrap();
    let mut edit = bsp.edit();
    edit.set_pakfile(zip).unwrap();
    edit.save(&path).unwrap();
    let back = Bsp::read(&path).unwrap();
    let pak = back.pakfile().unwrap();
    assert_eq!(pak.len(), 2);
    assert!(pak.find("materials/custom/a.vmt").unwrap().is_compressed());
    assert!(!pak.contains("maps/gm_t_particles.txt"));
    assert_eq!(back.game_lumps().unwrap().get(id::STATIC_PROPS).unwrap().stored.len(), 7);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn compressed_lumps_read_and_stay_compressed() {
    let ents = b"{\n\"classname\" \"worldspawn\"\n}\n\0".repeat(50);
    let packed = lzma::compress_valve(&ents, 6).unwrap();
    let mut bytes = synthetic_bsp(20, &[(lump::ENTITIES, packed.clone()), (lump::PLANES_FOR_TEST, vec![3; 8])]);
    let at = 8 + 12;
    bytes[at..at + 4].copy_from_slice(&(ents.len() as u32).to_le_bytes());
    let bsp = Bsp::from_vec(bytes).unwrap();
    assert!(bsp.is_compressed(lump::ENTITIES));
    assert_eq!(bsp.lump(lump::ENTITIES).unwrap().as_ref(), ents.as_slice());
    assert_eq!(bsp.entities().unwrap().len(), 1, "text stops at the first NUL");
    let out = Bsp::from_vec(bsp.edit().to_vec().unwrap()).unwrap();
    assert!(out.is_compressed(lump::ENTITIES));
    assert_eq!(out.lump_raw(lump::ENTITIES).unwrap(), packed.as_slice());
}

#[test]
fn hostile_files_never_panic() {
    assert!(matches!(Bsp::from_vec(b"VBSP".to_vec()), Err(BspError::Corrupt(_))));
    assert!(matches!(Bsp::from_vec(b"PK\x03\x04".to_vec()), Err(BspError::NotBsp)));
    let mut v = sample();
    v[4..8].copy_from_slice(&29i32.to_le_bytes());
    assert!(matches!(Bsp::from_vec(v), Err(BspError::UnsupportedVersion(29))));
    let good = sample();
    assert!(Bsp::from_vec(good[..good.len() - 1].to_vec()).is_err(), "truncated pakfile lump");
    for i in (0..good.len()).step_by(3) {
        let mut bad = good.clone();
        bad[i] = bad[i].wrapping_add(0x55);
        let Ok(bsp) = Bsp::from_vec(bad) else { continue };
        let _ = bsp.entities();
        let _ = bsp.game_lumps();
        let _ = bsp.cubemaps();
        let _ = bsp.texture_names();
        if let Ok(pak) = bsp.pakfile() {
            for e in pak.entries() {
                let _ = pak.read(e);
            }
        }
        let _ = bsp.edit().to_vec();
    }
}

#[test]
fn l4d2_layout_is_detected_and_kept() {
    let std_bytes = synthetic_bsp(21, &[(lump::ENTITIES, b"{\n}\n\0".to_vec()), (lump::PLANES_FOR_TEST, vec![1; 8])]);
    let h = Header::parse(&std_bytes).unwrap();
    let swapped = Header { layout: LumpLayout::VersionFirst, ..h.clone() };
    let mut bytes = std_bytes.clone();
    bytes[..HEADER_LEN].copy_from_slice(&swapped.to_bytes());
    let bsp = Bsp::from_vec(bytes.clone()).unwrap();
    assert_eq!(bsp.header().layout, LumpLayout::VersionFirst);
    assert_eq!(bsp.lump(lump::PLANES_FOR_TEST).unwrap().as_ref(), &[1; 8][..]);
    assert_eq!(bsp.edit().to_vec().unwrap(), bytes);
}

#[test]
fn cubemaps_carry_over() {
    let old = Bsp::from_vec(sample()).unwrap();
    let old_pak = old.pakfile().unwrap();
    assert_eq!(cubemaps::built_origins(&old_pak, "gm_t"), vec![[0, 0, 64]]);
    let samples: Vec<u8> = [0i32, 0, 64, 0].iter().flat_map(|v| v.to_le_bytes()).collect();
    let placeholder = b"VTF\0placeholder".to_vec();
    let mut pw = PakWriter::new();
    pw.add("materials/maps/gm_t/cubemapdefault.vtf", placeholder.clone()).unwrap();
    pw.add("materials/maps/gm_t/c0_0_64.vtf", placeholder).unwrap();
    let new = Bsp::from_vec(synthetic_bsp(
        20,
        &[(lump::CUBEMAPS, samples.clone()), (lump::PAKFILE, pw.finish(PakOptions::default()).unwrap())],
    ))
    .unwrap();
    assert!(cubemaps::built_origins(&new.pakfile().unwrap(), "gm_t").is_empty());
    let mut w = new.pakfile().unwrap().to_writer().unwrap();
    assert_eq!(cubemaps::carry_over(&old_pak, &new, "gm_t", &mut w, &mut Default::default()).unwrap(), 1);
    let zip = w.finish(PakOptions::default()).unwrap();
    let merged = PakArchive::parse(zip.as_slice()).unwrap();
    assert_eq!(merged.read_name("materials/maps/gm_t/c0_0_64.vtf").unwrap().unwrap().as_ref(), &[5; 300][..]);
    assert_eq!(cubemaps::count_built(&merged, "gm_t"), 1);
    let mut done = std::collections::HashSet::new();
    let mut w = new.pakfile().unwrap().to_writer().unwrap();
    assert_eq!(cubemaps::carry_over(&old_pak, &new, "gm_t", &mut w, &mut done).unwrap(), 1);
    assert_eq!(cubemaps::carry_over(&old_pak, &new, "gm_t", &mut w, &mut done).unwrap(), 0);
    let mut w = PakWriter::new();
    assert_eq!(cubemaps::carry_over(&old_pak, &old, "gm_t", &mut w, &mut Default::default()).unwrap(), 0);
    let moved = synthetic_bsp(20, &[(lump::CUBEMAPS, [0i32, 0, 65, 0].iter().flat_map(|v| v.to_le_bytes()).collect())]);
    let mut w = PakWriter::new();
    assert_eq!(
        cubemaps::carry_over(&old_pak, &Bsp::from_vec(moved).unwrap(), "gm_t", &mut w, &mut Default::default())
            .unwrap(),
        0
    );
}

fn real_maps() -> Vec<PathBuf> {
    let root = std::env::var_os("HP_GMOD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"D:\SteamLibrary\steamapps\common\GarrysMod"));
    ["gm_construct.bsp", "gm_flatgrass.bsp"]
        .iter()
        .map(|m| root.join("garrysmod/maps").join(m))
        .filter(|p| p.is_file())
        .collect()
}

#[test]
fn real_maps_read_and_rewrite() {
    let maps = real_maps();
    if maps.is_empty() {
        eprintln!("skipped: no Garry's Mod maps found");
        return;
    }
    for path in maps {
        let bsp = Bsp::open(&path).unwrap();
        assert_eq!(bsp.version(), 20);
        let ents = bsp.entities().unwrap();
        assert_eq!(ents[0].classname(), "worldspawn");
        let g = bsp.game_lumps().unwrap();
        assert!(g.get(id::STATIC_PROPS).is_some());
        assert!(!bsp.texture_names().unwrap().is_empty());
        let pak = bsp.pakfile().unwrap();
        for e in pak.entries() {
            pak.read(e).unwrap();
        }
        let zip = pak.to_writer().unwrap().finish(PakOptions::default()).unwrap();
        let pak_len = pak.len();
        drop(pak);
        let mut edit = Bsp::read(&path).unwrap().edit();
        edit.set_pakfile(zip).unwrap();
        let out = Bsp::from_vec(edit.to_vec().unwrap()).unwrap();
        for i in 0..LUMP_COUNT {
            if i != lump::PAKFILE && i != lump::GAME_LUMP {
                assert_eq!(out.lump(i).unwrap(), bsp.lump(i).unwrap(), "lump {i} of {}", path.display());
            }
        }
        assert_eq!(out.game_lumps().unwrap(), g);
        assert_eq!(out.pakfile().unwrap().len(), pak_len);
    }
}

#[test]
fn portal2_maps_match_the_portal2_format() {
    let dirs = [
        "E:/SteamLibrary/steamapps/common/Portal 2/portal2/maps",
        "D:/SteamLibrary/steamapps/common/Portal 2/portal2/maps",
        "C:/Program Files (x86)/Steam/steamapps/common/Portal 2/portal2/maps",
    ];
    let Some(dir) = dirs.iter().map(std::path::Path::new).find(|d| d.is_dir()) else {
        eprintln!("skipped: Portal 2 is not installed");
        return;
    };
    let format = crate::Branch::Portal2.format();
    let mut checked = 0;
    let mut maps: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "bsp"))
        .collect();
    maps.sort();
    for path in maps.iter().take(12) {
        let bsp = Bsp::open(path).unwrap();
        assert_eq!(bsp.version(), format.version, "{}", path.display());
        assert_eq!(bsp.header().layout, format.layout);
        for l in [lump::WORLDLIGHTS, lump::WORLDLIGHTS_HDR] {
            let info = bsp.lump_info(l).unwrap();
            assert_eq!(info.version, format.worldlights_version);
            assert_eq!(bsp.lump(l).unwrap().len() % crate::format::worldlight_size(info.version), 0);
        }
        let overlays = bsp.lump(lump::OVERLAYS).unwrap().len() / 352;
        assert_eq!(bsp.lump(61).unwrap().len(), overlays * 4, "{}", path.display());
        let g = bsp.game_lumps().unwrap();
        let sprp = g.get(id::STATIC_PROPS).unwrap();
        assert_eq!(sprp.version, format.static_props.version());
        let data = sprp.data().unwrap();
        let (props, found) = crate::staticprops::StaticPropLump::parse(&data, sprp.version, None).unwrap();
        assert_eq!(found.unwrap_or(format.static_props), format.static_props);
        for p in &props.props {
            assert!((p.prop_type as usize) < props.names.len());
            assert!(usize::from(p.first_leaf) + usize::from(p.leaf_count) <= props.leaves.len());
        }
        let mut original = data.to_vec();
        let rewritten = props.to_bytes(format.static_props);
        let start = original.len() - props.props.len() * 72;
        for k in 0..props.props.len() {
            original[start + k * 72 + 69..start + k * 72 + 72].fill(0);
        }
        assert_eq!(rewritten, original, "{}", path.display());
        let dprp = g.get(id::DETAIL_PROPS).unwrap();
        assert_eq!(dprp.version, format.detail_props_version);
        checked += 1;
    }
    assert!(checked > 0);
}
