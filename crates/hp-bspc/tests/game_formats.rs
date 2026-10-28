mod common;

use common::lumps::Lumps;
use common::{Vmf, block, room};
use glam::DVec3;
use hp_bsp::staticprops::{StaticPropFormat, StaticPropLump};
use hp_bsp::{Branch, LumpLayout};
use hp_bspc::CompileOptions;

const WALL: &str = "dev/dev_measuregeneric01b";

fn v(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

fn sample() -> Vmf {
    let mut m = Vmf::new();
    m.world.push(block(v(-256.0, -256.0, -16.0), v(256.0, 256.0, 0.0), WALL));
    m.world.extend(room(v(-256.0, -256.0, 0.0), v(256.0, 256.0, 256.0), WALL));
    m.world.push(block(v(-16.0, -16.0, 0.0), v(16.0, 16.0, 128.0), WALL));
    m.point("info_player_start", v(100.0, 100.0, 8.0), &[]);
    m.point("light", v(0.0, 100.0, 128.0), &[("_light", "255 255 255 200"), ("targetname", "lamp")]);
    m.point(
        "prop_static",
        v(-100.0, -100.0, 16.0),
        &[
            ("model", "models/props_c17/oildrum001.mdl"),
            ("angles", "0 90 0"),
            ("skin", "1"),
            ("rendercolor", "10 20 30"),
            ("renderamt", "40"),
            ("mincpulevel", "1"),
            ("maxgpulevel", "3"),
            ("disableX360", "1"),
            ("uniformscale", "1.5"),
            ("targetname", "drum"),
            ("mindxlevel", "80"),
        ],
    );
    m.point(
        "info_overlay",
        v(-64.0, 64.0, 0.0),
        &[
            ("material", WALL),
            ("sides", "6"),
            ("BasisNormal", "0 0 1"),
            ("BasisU", "1 0 0"),
            ("BasisV", "0 1 0"),
            ("StartU", "0"),
            ("EndU", "1"),
            ("StartV", "0"),
            ("EndV", "1"),
            ("uv0", "-16 -16 0"),
            ("uv1", "-16 16 0"),
            ("uv2", "16 16 0"),
            ("uv3", "16 -16 0"),
        ],
    );
    m
}

fn compile_for(m: &Vmf, branch: Branch) -> Vec<u8> {
    let map = hp_vmf::parse(&m.text()).expect("the VMF parses");
    let opts = CompileOptions { format: branch.format(), ..CompileOptions::default() };
    hp_bspc::compile(&map, &common::GameMats, &hp_bspc::props::NoModels, &opts, &mut |_| {})
        .unwrap_or_else(|e| panic!("{} compiles: {e}", branch.name()))
        .bsp
}

#[test]
fn every_branch_writes_its_format() {
    let m = sample();
    let gmod = compile_for(&m, Branch::GarrysMod);
    let gmod_lumps = Lumps::from_bytes(&gmod);
    for branch in Branch::ALL {
        let f = branch.format();
        let bytes = compile_for(&m, branch);
        assert_eq!(i32::from_le_bytes(bytes[4..8].try_into().unwrap()), f.version, "{branch:?}");
        let bsp = hp_bsp::Bsp::from_vec(bytes.clone()).expect("hp-bsp reads it");
        assert_eq!(bsp.version(), f.version);
        assert_eq!(bsp.header().layout, f.layout, "{branch:?}");
        let g = bsp.game_lumps().unwrap();
        let sprp = g.get(hp_bsp::gamelump::id::STATIC_PROPS).expect("sprp");
        assert_eq!(sprp.version, f.static_props.version(), "{branch:?}");
        let data = sprp.data().unwrap();
        let (lump, found) = StaticPropLump::parse(&data, sprp.version, Some(f.static_props)).unwrap();
        assert_eq!(found, Some(f.static_props));
        assert_eq!(lump.props.len(), 1);
        assert_eq!(
            data.len(),
            4 + 128 * lump.names.len() + 4 + 2 * lump.leaves.len() + 4 + f.static_props.size(),
            "{branch:?}"
        );
        let p = &lump.props[0];
        assert_eq!((p.origin, p.skin), ([-100.0, -100.0, 16.0], 1));
        match f.static_props {
            StaticPropFormat::V9 | StaticPropFormat::V10Csgo | StaticPropFormat::V11Csgo | StaticPropFormat::V8 => {
                assert_eq!(p.diffuse_modulation, [10, 20, 30, 40], "{branch:?}");
                assert_eq!((p.min_cpu_level, p.max_gpu_level), (1, 3));
            }
            StaticPropFormat::V11BlackMesa => assert_eq!(p.diffuse_modulation, [10, 20, 30, 40]),
            StaticPropFormat::V5Ship | StaticPropFormat::V6Bgt => assert_eq!(p.targetname, "drum"),
            _ => {}
        }
        if f.static_props == StaticPropFormat::V9 || f.static_props == StaticPropFormat::V11Csgo {
            assert!(p.disable_x360);
        }
        if f.static_props == StaticPropFormat::V11Csgo {
            assert_eq!(p.uniform_scale, 1.5);
        }
        if matches!(f.static_props, StaticPropFormat::V6 | StaticPropFormat::V10 | StaticPropFormat::V11BlackMesa) {
            assert_eq!(p.min_dx_level, if branch == Branch::GarrysMod { 0 } else { 80 }, "{branch:?}");
        }
        assert_eq!(g.get(hp_bsp::gamelump::id::DETAIL_PROPS).unwrap().version, f.detail_props_version);
        for l in [15, 54] {
            assert_eq!(bsp.lump_info(l).unwrap().version, f.worldlights_version, "{branch:?}");
        }
        let overlays = bsp.lump(hp_bsp::lump::OVERLAYS).unwrap().len() / 352;
        assert_eq!(overlays, 1);
        let levels = bsp.lump(61).unwrap();
        assert_eq!(levels.len(), if f.overlay_system_levels { 4 } else { 0 }, "{branch:?}");
        let world = &bsp.entities().unwrap()[0];
        assert_eq!(world.get("vbsp_exe").is_some(), branch == Branch::GarrysMod, "{branch:?}");
        assert!(world.get("world_mins").is_some());
        let l = Lumps::from_bytes(&bytes);
        for lump in [1usize, 3, 5, 7, 10, 12, 13, 18, 19, 26, 27, 29] {
            assert_eq!(l.raw(lump), gmod_lumps.raw(lump), "lump {lump} of {branch:?}");
        }
        assert!(
            hp_bspc::validate::validate(&bytes).is_empty(),
            "{branch:?}: {:?}",
            hp_bspc::validate::validate(&bytes)
        );
    }
}

#[test]
fn left4dead2_puts_the_version_first_in_the_lump_table() {
    let bytes = compile_for(&sample(), Branch::Left4Dead2);
    let bsp = hp_bsp::Bsp::from_vec(bytes.clone()).unwrap();
    let at = 8 + 7 * 16;
    let word = |k: usize| u32::from_le_bytes(bytes[at + 4 * k..at + 4 * k + 4].try_into().unwrap());
    let info = bsp.lump_info(7).unwrap();
    assert_eq!((word(0), word(1), word(2), word(3)), (1, info.offset, info.len, 0));
    assert_eq!(bsp.header().layout, LumpLayout::VersionFirst);
}

#[test]
fn garrys_mod_output_is_the_default() {
    let m = sample();
    let map = hp_vmf::parse(&m.text()).unwrap();
    let default =
        hp_bspc::compile(&map, &common::GameMats, &hp_bspc::props::NoModels, &CompileOptions::default(), &mut |_| {})
            .unwrap()
            .bsp;
    assert_eq!(default, compile_for(&m, Branch::GarrysMod));
}

#[test]
fn the_other_games_keep_valves_limits() {
    let mut m = Vmf::new();
    m.world = room(v(-512.0, -512.0, 0.0), v(512.0, 512.0, 256.0), WALL);
    m.point("info_player_start", v(0.0, 0.0, 8.0), &[]);
    for k in 0..33 {
        let name = format!("lamp{k}");
        m.point("light", v(-400.0 + f64::from(k) * 24.0, 0.0, 128.0), &[("targetname", name.as_str())]);
    }
    compile_for(&m, Branch::GarrysMod);
    let map = hp_vmf::parse(&m.text()).unwrap();
    for branch in [Branch::Source2013, Branch::Portal2, Branch::Csgo] {
        let opts = CompileOptions { format: branch.format(), ..CompileOptions::default() };
        let Err(err) = hp_bspc::compile(&map, &common::GameMats, &hp_bspc::props::NoModels, &opts, &mut |_| {}) else {
            panic!("fails")
        };
        assert!(err.to_string().contains("Too many switched lights (error at light lamp32), max = 32"), "{err}");
    }
    let mut m = Vmf::new();
    m.world = room(v(-256.0, -256.0, 0.0), v(256.0, 256.0, 256.0), WALL);
    m.point("info_player_start", v(0.0, 0.0, 8.0), &[]);
    for k in 0..513 {
        m.point("info_overlay", v(0.0, 0.0, 1.0 + f64::from(k) * 0.01), &[("material", WALL), ("sides", "")]);
    }
    compile_for(&m, Branch::GarrysMod);
    let map = hp_vmf::parse(&m.text()).unwrap();
    let opts = CompileOptions { format: Branch::Source2013.format(), ..CompileOptions::default() };
    let err = hp_bspc::compile(&map, &common::GameMats, &hp_bspc::props::NoModels, &opts, &mut |_| {}).err().unwrap();
    assert_eq!(err.to_string(), "Too Many Overlays!\nMAX_MAP_OVERLAYS = 512");
}

#[test]
fn lightmaps_stay_within_32_luxels_outside_garrys_mod() {
    let mut m = Vmf::new();
    m.world = room(v(-1024.0, -1024.0, 0.0), v(1024.0, 1024.0, 256.0), WALL);
    m.point("info_player_start", v(0.0, 0.0, 8.0), &[]);
    let map = hp_vmf::parse(&m.text()).unwrap();
    let mut log = Vec::new();
    let opts =
        CompileOptions { format: Branch::Source2013.format(), max_lightmap_dim: 125.0, ..CompileOptions::default() };
    let out =
        hp_bspc::compile(&map, &common::GameMats, &hp_bspc::props::NoModels, &opts, &mut |l| log.push(l.to_owned()))
            .unwrap();
    assert!(log.iter().any(|l| l.contains("-maxlightmapdim 125 is more than Source 2013 takes: 32 used")), "{log:#?}");
    let l = Lumps::from_bytes(&out.bsp);
    for f in l.faces() {
        assert!(f.lightmap_size[0] <= 33 && f.lightmap_size[1] <= 33, "{:?}", f.lightmap_size);
    }
}

#[test]
fn the_lighting_writes_the_world_lights_of_the_format() {
    let m = sample();
    for branch in [Branch::GarrysMod, Branch::Portal2, Branch::Left4Dead2] {
        let bytes = compile_for(&m, branch);
        let lit = hp_bspc::rad::light_bsp(
            &bytes,
            &hp_bspc::rad::RadOptions { bounces: 1, ..Default::default() },
            &mut |_| {},
        )
        .expect("lit");
        let bsp = hp_bsp::Bsp::from_vec(lit.clone()).unwrap();
        let f = branch.format();
        assert_eq!(bsp.version(), f.version);
        assert_eq!(bsp.header().layout, f.layout);
        let size = hp_bsp::format::worldlight_size(f.worldlights_version);
        for l in [15, 54] {
            let info = bsp.lump_info(l).unwrap();
            assert_eq!(info.version, f.worldlights_version, "{branch:?}");
            let data = bsp.lump(l).unwrap();
            assert_eq!(data.len(), size, "{branch:?}: one light");
        }
        assert!(hp_bspc::rad::check_lighting(&lit).is_empty(), "{:?}", hp_bspc::rad::check_lighting(&lit));
        assert_eq!(
            bsp.game_lumps().unwrap().get(hp_bsp::gamelump::id::STATIC_PROPS).unwrap().version,
            f.static_props.version()
        );
    }
}
