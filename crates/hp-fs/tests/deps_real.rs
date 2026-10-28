use std::path::PathBuf;
use std::time::Instant;

use hp_fs::deps::{DepRoot, DepsCache, PackPolicy, ResolveOptions, RootKind};
use hp_fs::{GameFs, MountOptions};

const DEFAULT_GMOD_DIR: &str = r"D:\SteamLibrary\steamapps\common\GarrysMod";

fn gmod_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("HAMMERPRO_GMOD_DIR").map_or_else(|| PathBuf::from(DEFAULT_GMOD_DIR), PathBuf::from);
    dir.join("garrysmod").is_dir().then_some(dir)
}

fn roots_of(bsp: &hp_bsp::Bsp) -> Vec<DepRoot> {
    let mut roots: Vec<DepRoot> = bsp
        .texture_names()
        .unwrap()
        .into_iter()
        .map(|t| DepRoot { kind: RootKind::Material, name: t, why: "face".into() })
        .collect();
    for e in bsp.entities().unwrap() {
        for (k, v) in &e.pairs {
            let kind = match k.to_ascii_lowercase().as_str() {
                "model" if v.ends_with(".mdl") => RootKind::Model,
                "skyname" => RootKind::Skybox,
                "effect_name" => RootKind::Particle,
                "soundscape" => RootKind::Soundscape,
                "message" if e.classname() == "ambient_generic" => RootKind::Sound,
                _ => continue,
            };
            roots.push(DepRoot { kind, name: v.clone(), why: format!("{} {k}", e.classname()) });
        }
    }
    if let Ok(g) = bsp.game_lumps()
        && let Some(sprp) = g.get(hp_bsp::gamelump::id::STATIC_PROPS)
        && let Ok(data) = sprp.data()
        && data.len() >= 4
    {
        let n = i32::from_le_bytes(data[..4].try_into().unwrap()).max(0) as usize;
        for i in 0..n.min((data.len() - 4) / 128) {
            let raw = &data[4 + i * 128..4 + (i + 1) * 128];
            let end = raw.iter().position(|&b| b == 0).unwrap_or(128);
            let name = String::from_utf8_lossy(&raw[..end]).into_owned();
            roots.push(DepRoot { kind: RootKind::Model, name, why: "static prop".into() });
        }
    }
    roots
}

#[test]
fn real_maps_resolve_fast_and_completely() {
    let Some(root) = gmod_dir() else {
        eprintln!("skipped: Garry's Mod not found");
        return;
    };
    let fs = GameFs::for_gmod(&root, &MountOptions::default()).expect("mount");
    let cache = DepsCache::new();
    for map in ["gm_construct", "gm_flatgrass", "rp_demon_slayer_kasty", "union_blue_lock_v2"] {
        let path = root.join("garrysmod/maps").join(format!("{map}.bsp"));
        let Ok(bsp) = hp_bsp::Bsp::open(&path) else { continue };
        let roots = roots_of(&bsp);
        let start = Instant::now();
        let g = fs.resolve_dependencies(&roots, &ResolveOptions { map_name: map, ..ResolveOptions::default() }, &cache);
        let cold = start.elapsed();
        let start = Instant::now();
        let again =
            fs.resolve_dependencies(&roots, &ResolveOptions { map_name: map, ..ResolveOptions::default() }, &cache);
        let warm = start.elapsed();
        let shipped = g.shipped(PackPolicy::default()).count();
        eprintln!(
            "{map}: {} roots, {} files, {} shipped, {} missing, {} warnings, cold {:.0} ms, warm {:.0} ms",
            roots.len(),
            g.nodes.len(),
            shipped,
            g.missing.len(),
            g.warnings.len(),
            cold.as_secs_f64() * 1000.0,
            warm.as_secs_f64() * 1000.0
        );
        for m in g.missing.iter().take(10) {
            eprintln!("  missing {:?} {}", m.kind, m.name);
        }
        assert_eq!(again.nodes.len(), g.nodes.len());
        if map.starts_with("gm_") {
            assert!(g.nodes.len() > 20);
            assert!(g.missing.is_empty(), "{:?}", g.missing);
            assert!(g.nodes.iter().any(|n| n.vpath.ends_with(".vtf")));
        }
    }
}
