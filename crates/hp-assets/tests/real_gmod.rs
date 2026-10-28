mod common;

use std::time::{Duration, Instant};

use common::{frames_until, real_gmod, real_settings};
use glam::DVec3;
use hp_app::AssetsApi;
use hp_app::state::{AssetStatus, ModelState, ThumbState};
use hp_assets::AssetManager;

fn entity(class: &str, props: &[(&str, &str)]) -> hp_vmf::Entity {
    let mut e = hp_vmf::Entity { classname: class.to_owned(), ..Default::default() };
    for (k, v) in props {
        e.set(k, *v);
    }
    e
}

#[test]
fn real_game_content() {
    let Some(root) = real_gmod() else {
        eprintln!("Garry's Mod not found: test skipped");
        return;
    };
    hp_app::register_translations();
    hp_assets::register_translations();
    let start = Instant::now();
    let mut am = AssetManager::new(&real_settings(&root));
    assert_eq!(am.status(), AssetStatus::Mounting);
    assert!(am.wait_until_mounted(Duration::from_secs(120)), "mount timed out");
    eprintln!("mounted in {:?}", start.elapsed());
    assert_eq!(am.status(), AssetStatus::Ready);
    let (materials, models, sounds) = (am.materials().len(), am.models().len(), am.sounds().len());
    eprintln!("{materials} materials, {models} models, {sounds} sounds");
    assert!(materials > 1000 && models > 1000 && sounds > 100);
    let fgd = am.fgd().expect("FGD loaded");
    assert!(fgd.class("prop_static").is_some() && fgd.class("info_player_start").is_some());
    eprintln!("light_spot studio helper: {:?}", fgd.class("light_spot").and_then(|c| c.studio()));

    let info = frames_until(&mut am, Duration::from_secs(30), |am, _| am.material_info("concrete/concretefloor001a"))
        .expect("material info");
    eprintln!("{info:?}");
    assert!(!info.missing);
    assert_eq!(info.shader.to_ascii_lowercase(), "lightmappedgeneric");
    assert!(info.width >= 256 && info.height >= 256);
    assert_eq!(info.surface_prop.as_deref().map(str::to_ascii_lowercase).as_deref(), Some("concrete"));
    let trigger = frames_until(&mut am, Duration::from_secs(30), |am, _| am.material_info("tools/toolstrigger"))
        .expect("material info");
    assert_eq!(trigger.tool, Some(hp_vmt::ToolKind::Trigger));

    for name in
        ["concrete/concretefloor001a", "tools/toolsnodraw", "nature/blendgrassgravel001a", "glass/glasswindow007a"]
    {
        let state = frames_until(&mut am, Duration::from_secs(30), |am, ctx| match am.thumbnail(ctx, name) {
            ThumbState::Loading => None,
            other => Some(other),
        });
        eprintln!("thumbnail {name}: {state:?}");
        assert!(matches!(state, Some(ThumbState::Ready { .. })), "{name}: {state:?}");
    }

    let drum = frames_until(&mut am, Duration::from_secs(30), |am, _| {
        match am.model_info("models/props_c17/oildrum001.mdl") {
            ModelState::Loading => None,
            other => Some(other),
        }
    });
    let Some(ModelState::Ready(drum)) = drum else { panic!("oil drum: {drum:?}") };
    eprintln!("{drum:?}");
    assert!(drum.triangles > 100 && drum.skins >= 1 && !drum.materials.is_empty());
    assert!(drum.bounds_max.z - drum.bounds_min.z > 30.0);

    let start_ent = entity("info_player_start", &[("origin", "0 0 0"), ("angles", "0 0 0")]);
    let first = am.entity_bounds(&start_ent).expect("bounds");
    eprintln!("info_player_start bounds before load: {first:?}");
    let loaded = frames_until(&mut am, Duration::from_secs(30), |am, _| {
        let b = am.entity_bounds(&start_ent)?;
        (b != first).then_some(b)
    });
    eprintln!("info_player_start bounds with model: {loaded:?}");
    let b = loaded.unwrap_or(first);
    assert!(b.size().z > 60.0 && b.size().z < 90.0, "{b:?}");

    let prop = entity(
        "prop_static",
        &[("origin", "100 0 0"), ("angles", "0 90 0"), ("model", "models/props_c17/oildrum001.mdl")],
    );
    let b = frames_until(&mut am, Duration::from_secs(30), |am, _| {
        let b = am.entity_bounds(&prop)?;
        (b.size() != DVec3::splat(16.0)).then_some(b)
    })
    .expect("prop bounds");
    eprintln!("prop bounds: {b:?}");
    assert!(b.size().z > 30.0);
    assert!(b.contains_point(DVec3::new(100.0, 0.0, 20.0)));
}

#[test]
fn posed_prop_keeps_its_bounds() {
    let Some(root) = real_gmod() else { return };
    hp_app::register_translations();
    hp_assets::register_translations();
    let mut am = AssetManager::new(&real_settings(&root));
    assert!(am.wait_until_mounted(Duration::from_secs(120)), "mount timed out");
    let turret = entity(
        "prop_dynamic",
        &[("origin", "0 0 0"), ("model", "models/combine_turrets/floor_turret.mdl"), ("DefaultAnim", "deploy")],
    );
    let b = frames_until(&mut am, Duration::from_secs(30), |am, _| {
        let b = am.entity_bounds(&turret)?;
        (b.size().x > 20.0).then_some(b)
    });
    let info = frames_until(&mut am, Duration::from_secs(30), |am, _| {
        match am.model_info("models/combine_turrets/floor_turret.mdl") {
            ModelState::Loading => None,
            other => Some(other),
        }
    });
    eprintln!("bounds {b:?} info {:?}", info.map(|i| matches!(i, ModelState::Ready(_))));
    assert!(b.is_some(), "the posed turret has its model bounds");
}
