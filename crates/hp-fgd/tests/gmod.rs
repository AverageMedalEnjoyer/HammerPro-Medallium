use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hp_fgd::{ClassKind, Fgd, PropertyType, ResolvedFgd};

const GMOD: &str = r"D:\SteamLibrary\steamapps\common\GarrysMod";

fn gmod_file(rel: &str) -> Option<PathBuf> {
    let path = Path::new(GMOD).join(rel);
    if path.is_file() {
        Some(path)
    } else {
        eprintln!("skipping: {} not found", path.display());
        None
    }
}

fn garrysmod_fgd() -> Option<PathBuf> {
    gmod_file(r"bin\garrysmod.fgd")
}

fn hammerplusplus_fgd() -> Option<PathBuf> {
    gmod_file(r"bin\win64\hammerplusplus\hammerplusplus_fgd.fgd")
}

fn report(label: &str, fgd: &Fgd, resolved: &ResolvedFgd) {
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for class in &fgd.classes {
        *kinds.entry(class.kind.directive().to_owned()).or_default() += 1;
    }
    eprintln!(
        "{label}: {} declared classes {kinds:?}; resolved {} (point {}, solid {}); {} parse warnings, {} resolve warnings",
        fgd.classes.len(),
        resolved.len(),
        resolved.point_classes().len(),
        resolved.solid_classes().len(),
        fgd.warnings.len(),
        resolved.warnings.len()
    );
    for w in fgd.warnings.iter().chain(&resolved.warnings) {
        eprintln!("  warning: {w}");
    }
    let items: usize = resolved
        .iter()
        .map(|c| c.props.len() + c.inputs.len() + c.outputs.len() + c.helpers.len() + c.bases.len())
        .sum();
    let largest = resolved.iter().map(|c| c.props.len()).max().unwrap_or(0);
    eprintln!("  {items} resolved items in total, largest class has {largest} keyvalues");
}

#[test]
fn garrysmod_fgd_parses_and_resolves() {
    let Some(path) = garrysmod_fgd() else { return };
    let fgd = Fgd::load(&path).expect("load garrysmod.fgd");
    let r = fgd.resolve();
    report("garrysmod.fgd", &fgd, &r);
    assert!(fgd.warnings.is_empty(), "parse warnings: {:#?}", fgd.warnings);
    assert!(r.warnings.is_empty(), "resolve warnings: {:#?}", r.warnings);
    assert_eq!(fgd.map_size, Some((-16384, 16384)));
    assert!(r.point_classes().len() > 300, "{}", r.point_classes().len());
    assert!(r.solid_classes().len() > 80, "{}", r.solid_classes().len());

    let prop_static = r.class("prop_static").expect("prop_static");
    let studio = prop_static.studio().expect("prop_static has a model helper");
    assert!(studio.is_prop && studio.model.is_none());
    assert!(prop_static.has_helper("studioprop"));
    assert_eq!(prop_static.sphere_props(), ["fademindist", "fademaxdist"]);
    assert_eq!(prop_static.color(), Some([255, 255, 0]));
    assert_eq!(prop_static.property("model").map(|p| &p.kind), Some(&PropertyType::Studio));

    let light = r.class("light").expect("light");
    assert!(light.is_point());
    assert_eq!(light.iconsprite(), Some("editor/light.vmt"));
    assert!(light.has_helper("light"));
    assert!(light.property("_light").is_some(), "inherited from @BaseClass Light");
    assert_eq!(light.color(), Some([180, 10, 180]), "color inherited from @BaseClass Light");
    let spot = r.class("light_spot").expect("light_spot");
    assert!(spot.has_helper("lightprop") && spot.has_helper("lightcone"));
    assert_eq!(spot.studio().and_then(|s| s.model).as_deref(), Some("models/editor/spot.mdl"));

    let door = r.class("func_door").expect("func_door");
    assert!(door.is_solid());
    assert!(door.input("Open").is_some() && door.input("Close").is_some());
    assert!(door.output("OnOpen").is_some());
    assert!(door.input("Kill").is_some(), "inherited from Targetname");
    let flags = door.spawnflags().expect("func_door spawnflags");
    assert!(flags.flags.iter().any(|f| f.value == 2048 && f.display == "Starts locked"));
    assert_eq!(door.default_spawnflags(), 1024, "Touch Opens is on by default");
    let kv = door.default_keyvalues();
    assert!(kv.contains(&("spawnflags".to_owned(), "1024".to_owned())));
    assert!(kv.contains(&("speed".to_owned(), "100".to_owned())));
    let rotating = r.class("func_door_rotating").expect("func_door_rotating");
    let rot_flags = &rotating.spawnflags().expect("spawnflags").flags;
    assert!(rot_flags.iter().any(|f| f.value == 2), "own flag");
    assert!(rot_flags.iter().any(|f| f.value == 1024), "flag merged from Door");

    let start = r.class("info_player_start").expect("info_player_start");
    assert_eq!(start.studio().and_then(|s| s.model).as_deref(), Some("models/editor/playerstart.mdl"));
    assert!(start.size().is_some() || start.studio().is_some());
    let ladder = r.class("func_ladderendpoint").expect("func_ladderendpoint");
    assert_eq!(ladder.size(), Some(([-16.0, -16.0, 0.0], [16.0, 16.0, 72.0])));

    let lua_run = r.class("lua_run").expect("lua_run");
    assert_eq!(lua_run.iconsprite(), Some("editor/lua_run.vmt"));
    assert!(lua_run.input("RunPassedCode").is_some());
    let sky = r.class("env_skypaint").expect("env_skypaint");
    assert_eq!(sky.property("topcolor").map(|p| &p.kind), Some(&PropertyType::Color1));
    let ladder_brush = r.class("func_ladder").expect("func_ladder");
    assert!(ladder_brush.description.contains("toolsinvisibleladder"));
    let worldspawn = r.class("worldspawn").expect("worldspawn");
    assert!(worldspawn.is_solid());
    assert!(r.class("npc_combine_s").is_some_and(|c| c.kind == ClassKind::Npc));
    assert!(!r.all_inputs().is_empty() && !r.all_outputs().is_empty());
}

#[test]
fn hammerplusplus_extensions_apply() {
    let (Some(gmod), Some(hpp)) = (garrysmod_fgd(), hammerplusplus_fgd()) else { return };
    let fgd = Fgd::load_many([&gmod, &hpp]).expect("load FGDs");
    let r = fgd.resolve();
    report("garrysmod.fgd + hammerplusplus_fgd.fgd", &fgd, &r);
    assert!(fgd.warnings.is_empty(), "parse warnings: {:#?}", fgd.warnings);
    assert!(r.warnings.is_empty(), "resolve warnings: {:#?}", r.warnings);

    let light = r.class("light").expect("light");
    let lights: Vec<_> = light.helpers_named("light").map(|h| h.args.clone()).collect();
    assert_eq!(lights, [vec!["point".to_owned()]]);
    assert_eq!(light.iconsprite(), Some("editor/light.vmt"));

    let spot = r.class("light_spot").expect("light_spot");
    assert_eq!(spot.studio().and_then(|s| s.model).as_deref(), Some("models/editor/spot_color.mdl"));
    let env = r.class("light_environment").expect("light_environment");
    assert_eq!(env.helpers_named("light").count(), 2);
    assert!(env.has_helper("direction"));

    let door = r.class("func_door").expect("func_door");
    assert!(door.helpers_named("direction").any(|h| h.args == ["movedir"]));
    let particles = r.class("info_particle_system").expect("info_particle_system");
    let effect = particles.property("effect_name").expect("effect_name");
    assert_eq!(effect.kind, PropertyType::ParticleSystem);
    assert_eq!(effect.display_name, "Particle System Name");

    let text = r.class("editor_text").expect("editor_text");
    assert!(text.has_helper("worldtext"));
    assert_eq!(text.property("textsize").and_then(|p| p.default.as_deref()), Some("10"));
    assert!(r.point_classes().iter().any(|n| n == "editor_text"));
}

#[test]
fn ttt_fgd_with_sample_maps() {
    let (Some(gmod), Some(ttt)) = (garrysmod_fgd(), gmod_file(r"garrysmod\gamemodes\terrortown\ttt.fgd")) else {
        return;
    };
    let fgd = Fgd::load_many([&gmod, &ttt]).expect("load FGDs");
    let r = fgd.resolve();
    report("garrysmod.fgd + ttt.fgd", &fgd, &r);
    assert!(fgd.warnings.is_empty(), "parse warnings: {:#?}", fgd.warnings);
    assert!(r.warnings.is_empty(), "resolve warnings: {:#?}", r.warnings);
    let random = r.class("ttt_random_weapon").expect("ttt_random_weapon");
    assert_eq!(random.spawnflags().map(|p| p.flags.len()), Some(9));

    let maps_dir = Path::new(GMOD).join(r"garrysmod\gamemodes\terrortown\mapexamples");
    let mut vmfs: Vec<PathBuf> =
        std::fs::read_dir(&maps_dir).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    vmfs.push(PathBuf::from(r"C:\Users\oracle\Desktop\hammermcptest\test.vmf"));
    let mut checked = 0;
    for vmf in vmfs.iter().filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("vmf"))) {
        let Ok(bytes) = std::fs::read(vmf) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("\"classname\"") else { continue };
            let class = rest.trim().trim_matches('"');
            checked += 1;
            assert!(r.contains(class), "{}: unknown class {class}", vmf.display());
        }
    }
    eprintln!("checked {checked} entity classnames from {} sample VMFs", vmfs.len());
}

#[test]
fn timing_all_gmod_fgds() {
    let (Some(gmod), Some(hpp)) = (garrysmod_fgd(), hammerplusplus_fgd()) else { return };
    let mut load_best = Duration::MAX;
    let mut resolve_best = Duration::MAX;
    let mut total = Duration::ZERO;
    const RUNS: u32 = 20;
    for _ in 0..RUNS {
        let start = Instant::now();
        let fgd = Fgd::load_many([&gmod, &hpp]).expect("load FGDs");
        let loaded = Instant::now();
        let resolved = fgd.resolve();
        let done = Instant::now();
        assert!(resolved.len() > 400);
        load_best = load_best.min(loaded - start);
        resolve_best = resolve_best.min(done - loaded);
        total += done - start;
    }
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    eprintln!(
        "[{profile}] load all GMod FGDs: best load {:.2} ms + resolve {:.2} ms, average total {:.2} ms",
        load_best.as_secs_f64() * 1e3,
        resolve_best.as_secs_f64() * 1e3,
        total.as_secs_f64() * 1e3 / f64::from(RUNS)
    );
    if !cfg!(debug_assertions) {
        assert!(load_best + resolve_best < Duration::from_millis(60));
    }
}

#[test]
fn hammer_pro_definitions_extend_the_game() {
    let Some(path) = garrysmod_fgd() else { return };
    let mut fgd = Fgd::load(&path).expect("garrysmod.fgd");
    let ours = Fgd::parse(include_str!("../../../assets/skybox/hammerpro.fgd"), None);
    assert!(ours.warnings.is_empty(), "{:?}", ours.warnings);
    fgd.merge(ours);
    let resolved = fgd.resolve();
    let switch = resolved.class("point_hammerpro_skybox").expect("switch");
    assert!(switch.is_point());
    assert!(switch.property("targetname").is_some(), "Targetname base");
    assert!(switch.property("skycamera").is_some());
    for input in ["Activate", "Deactivate", "Toggle"] {
        assert!(switch.input(input).is_some(), "{input}");
    }
    assert!(switch.output("OnActivated").is_some());
    let cam = resolved.class("sky_camera").expect("sky_camera");
    assert!(cam.property("targetname").is_some(), "sky cameras can be named");
    assert!(cam.property("scale").is_some(), "and keep their own keys");
    for class in ["light", "light_spot"] {
        let c = resolved.class(class).expect(class);
        assert_eq!(c.sphere_props(), ["_fifty_percent_distance", "_zero_percent_distance", "_distance"], "{class}");
        assert!(c.property("_distance").is_some_and(|p| p.description.contains("Hammer Pro")), "{class}");
        assert!(c.property("_light").is_some(), "{class} keeps its keys");
    }
    assert!(resolved.class("light_spot").unwrap().has_helper("lightcone"));
    assert_eq!(resolved.class("prop_door_rotating").unwrap().sphere_props(), ["fademindist", "fademaxdist"]);
    assert_eq!(resolved.class("func_lod").unwrap().sphere_props(), ["DisappearMinDist", "DisappearMaxDist"]);
    assert_eq!(resolved.class("env_lightglow").unwrap().sphere_props(), ["MaxDist", "OuterMaxDist"]);
    let obb = resolved.class("parallax_obb").expect("parallax_obb");
    assert!(obb.is_solid() && obb.property("targetname").is_some());
    let cube = resolved.class("env_cubemap").expect("env_cubemap");
    assert!(cube.property("parallaxobb").is_some_and(|p| p.description.contains("Garry's Mod")));
    assert!(cube.property("cubemapsize").is_some() && cube.property("sides").is_some());
    assert!(cube.has_helper("line"));
    assert!(cube.iconsprite().is_some());
}
