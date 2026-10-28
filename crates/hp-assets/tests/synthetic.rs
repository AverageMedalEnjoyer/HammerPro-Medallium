mod common;

use std::time::Duration;

use common::{fake_install, fake_settings, frames_until};
use glam::DVec3;
use hp_app::AssetsApi;
use hp_app::state::{AssetStatus, ImportOptions, ModelState, ThumbState};
use hp_assets::AssetManager;

fn init() {
    hp_app::register_translations();
    hp_assets::register_translations();
}

fn mounted(name: &str) -> (std::path::PathBuf, AssetManager) {
    init();
    let root = fake_install(name);
    let mut am = AssetManager::new(&fake_settings(&root));
    assert!(am.wait_until_mounted(Duration::from_secs(30)), "mount timed out");
    assert_eq!(am.status(), AssetStatus::Ready);
    (root, am)
}

fn entity(class: &str, props: &[(&str, &str)]) -> hp_vmf::Entity {
    let mut e = hp_vmf::Entity { classname: class.to_owned(), ..Default::default() };
    for (k, v) in props {
        e.set(k, *v);
    }
    e
}

#[test]
fn mount_lists_and_labels() {
    let (root, am) = mounted("syn_mount");
    let mats = am.materials();
    assert!(mats.iter().any(|m| m.name == "test/brick"), "{mats:?}");
    assert!(mats.windows(2).all(|w| w[0].name <= w[1].name), "sorted");
    let brick = mats.iter().find(|m| m.name == "test/brick").map(|m| m.source).unwrap_or(0);
    assert!(!am.source_label(brick).starts_with('#'), "{}", am.source_label(brick));
    assert_eq!(am.source_label(9999), "#9999");
    assert!(am.fs().is_some());
    assert!(am.fgd().is_some_and(|f| f.class("info_player_start").is_some()));
    assert!(am.generation() > 1);
    assert_eq!(am.game_root(), Some(root.as_path()));
    let line = am.status_line().unwrap_or_default();
    assert!(line.contains("materials") || line.contains("matériaux") || line.contains("textures"), "{line}");
}

#[test]
fn material_info_and_thumbnails() {
    let (_root, mut am) = mounted("syn_info");
    let info = frames_until(&mut am, Duration::from_secs(20), |am, _| am.material_info("TEST/Brick")).expect("info");
    assert_eq!(info.name, "test/brick");
    assert_eq!((info.width, info.height), (64, 64));
    assert_eq!(info.shader, "LightmappedGeneric");
    assert_eq!(info.surface_prop.as_deref(), Some("brick"));
    assert!(!info.missing && info.tool.is_none());
    let nodraw =
        frames_until(&mut am, Duration::from_secs(20), |am, _| am.material_info("tools/toolsnodraw")).expect("info");
    assert_eq!(nodraw.tool, Some(hp_vmt::ToolKind::Nodraw));
    let missing = frames_until(&mut am, Duration::from_secs(20), |am, _| am.material_info("no/such")).expect("info");
    assert!(missing.missing);

    let thumb = frames_until(&mut am, Duration::from_secs(20), |am, ctx| match am.thumbnail(ctx, "test/brick") {
        ThumbState::Ready { size, .. } => Some(size),
        _ => None,
    });
    assert_eq!(thumb, Some([64, 64]));
    let glass = frames_until(&mut am, Duration::from_secs(20), |am, ctx| match am.thumbnail(ctx, "test/glass") {
        ThumbState::Loading => None,
        other => Some(other),
    });
    assert!(matches!(glass, Some(ThumbState::Ready { size: [32, 32], .. })), "{glass:?}");
    let none = frames_until(&mut am, Duration::from_secs(20), |am, ctx| match am.thumbnail(ctx, "no/such") {
        ThumbState::Loading => None,
        other => Some(other),
    });
    assert_eq!(none, Some(ThumbState::Missing));
    assert!(am.stats().thumbnails >= 2);
}

#[test]
fn stale_thumbnails_are_dropped() {
    let (_root, mut am) = mounted("syn_stale");
    let names: Vec<String> = (0..200).map(|i| format!("missing/m{i}")).collect();
    let _ = frames_until(&mut am, Duration::from_secs(5), |am, ctx| {
        for n in &names {
            am.thumbnail(ctx, n);
        }
        Some(())
    });
    let ready = frames_until(&mut am, Duration::from_secs(20), |am, ctx| match am.thumbnail(ctx, "test/brick") {
        ThumbState::Ready { .. } => Some(()),
        _ => None,
    });
    assert!(ready.is_some());
    let settled = frames_until(&mut am, Duration::from_secs(20), |am, _| (am.stats().queued_jobs == 0).then_some(()));
    assert!(settled.is_some());
}

#[test]
fn entity_bounds_and_models() {
    let (_root, mut am) = mounted("syn_bounds");
    let start = entity("info_player_start", &[("origin", "10 20 30")]);
    let b = am.entity_bounds(&start).expect("bounds");
    assert_eq!(b.min, DVec3::new(-6.0, 4.0, 30.0));
    assert_eq!(b.max, DVec3::new(26.0, 36.0, 102.0));
    let unknown = entity("some_unknown_class", &[("origin", "0 0 0")]);
    let b = am.entity_bounds(&unknown).expect("bounds");
    assert_eq!(b.size(), DVec3::splat(16.0));
    let prop = entity("prop_static", &[("origin", "0 0 0"), ("model", "models/does/not_exist.mdl")]);
    assert_eq!(am.entity_bounds(&prop).map(|b| b.size()), Some(DVec3::splat(16.0)));
    let state =
        frames_until(&mut am, Duration::from_secs(20), |am, _| match am.model_info("models/does/not_exist.mdl") {
            ModelState::Loading => None,
            other => Some(other),
        });
    assert!(matches!(state, Some(ModelState::Failed(_))), "{state:?}");
    assert_eq!(am.entity_bounds(&prop).map(|b| b.size()), Some(DVec3::splat(16.0)));
}

#[test]
fn import_invalidate_and_remount() {
    let (root, mut am) = mounted("syn_import");
    let gen0 = am.generation();
    let src = root.join("incoming");
    std::fs::create_dir_all(&src).expect("folder");
    let px: Vec<u8> = [30u8, 200, 90, 255].repeat(32 * 32);
    image::save_buffer(src.join("Green Tile.png"), &px, 32, 32, image::ColorType::Rgba8).expect("png");
    let opts = ImportOptions {
        dest_dir: root.join("custom_art"),
        prefix: "custom/art".into(),
        surface_prop: Some("tile".into()),
        generate_mipmaps: true,
        overwrite: false,
    };
    let results = am.import_images(&[src.join("Green Tile.png"), src.join("missing.png")], &opts);
    assert_eq!(results[0].as_deref(), Ok("custom/art/green_tile"));
    assert!(results[1].is_err());
    assert!(am.generation() > gen0);
    let listed = frames_until(&mut am, Duration::from_secs(20), |am, _| {
        am.materials().iter().any(|m| m.name == "custom/art/green_tile").then_some(())
    });
    assert!(listed.is_some(), "imported material appears in the list");
    let info = frames_until(&mut am, Duration::from_secs(20), |am, _| am.material_info("custom/art/green_tile"))
        .expect("info");
    assert!(!info.missing);
    assert_eq!((info.width, info.height), (32, 32));

    let vmt = root.join("garrysmod/materials/test/brick.vmt");
    let _ = frames_until(&mut am, Duration::from_secs(20), |am, _| am.material_info("test/brick"));
    std::fs::write(&vmt, r#""LightmappedGeneric" { "$basetexture" "test/glass" "$surfaceprop" "glass" }"#)
        .expect("vmt");
    am.invalidate_material("test/brick");
    let info = frames_until(&mut am, Duration::from_secs(20), |am, _| am.material_info("test/brick")).expect("info");
    assert_eq!(info.surface_prop.as_deref(), Some("glass"));
    assert_eq!(info.base_texture.as_deref(), Some("test/glass"));

    am.remount(&fake_settings(&root));
    assert_eq!(am.status(), AssetStatus::Mounting);
    assert!(am.fs().is_some());
    assert!(!am.materials().is_empty());
    assert!(am.wait_until_mounted(Duration::from_secs(30)));
    assert_eq!(am.status(), AssetStatus::Ready);
}

#[test]
fn bad_root_fails() {
    init();
    let mut s = hp_app::settings::Settings::default();
    s.game.gmod_root = Some(common::scratch("syn_bad_root").join("nothing_here"));
    let mut am = AssetManager::new(&s);
    assert!(am.wait_until_mounted(Duration::from_secs(30)));
    assert!(matches!(am.status(), AssetStatus::Failed(_)), "{:?}", am.status());
    assert!(am.fs().is_none());
    assert!(am.status_line().is_some());
    let ctx = egui::Context::default();
    assert_eq!(am.thumbnail(&ctx, "test/brick"), ThumbState::Missing);
    assert!(am.material_info("test/brick").is_some_and(|i| i.missing));
    assert!(matches!(am.model_info("models/a.mdl"), ModelState::Failed(_)));
    let r = am.import_images(
        &[],
        &ImportOptions {
            dest_dir: common::scratch("syn_bad_root").join("out"),
            prefix: String::new(),
            surface_prop: None,
            generate_mipmaps: false,
            overwrite: false,
        },
    );
    assert!(r.is_empty());
}

#[test]
fn works_through_app_state() {
    init();
    let root = fake_install("syn_app");
    let mut am = AssetManager::new(&fake_settings(&root));
    assert!(am.wait_until_mounted(Duration::from_secs(30)));
    let mut state = hp_app::AppState::new(fake_settings(&root), Box::new(am));
    let map = hp_vmf::parse(hp_app::testing::TEST_VMF).expect("test map");
    state.doc = hp_editor::Document::from_map(map);
    let mut thumbs = Vec::new();
    for _ in 0..200 {
        hp_app::testing::run_ctx(&mut state, |ctx, state| {
            state.assets.begin_frame(ctx, None);
            thumbs.push(state.assets.thumbnail(ctx, "dev/dev_measuregeneric01b"));
            let _ = state.assets.material_info("concrete/concretefloor001a");
        });
        if thumbs.iter().any(|t| matches!(t, ThumbState::Ready { .. })) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(thumbs.iter().any(|t| matches!(t, ThumbState::Ready { size: [128, 128], .. })), "{:?}", thumbs.last());
    let player = state.doc.scene().entities().values().find(|n| n.entity.classname == "info_player_start");
    let b = player.and_then(|n| state.entity_bounds(&n.entity)).expect("bounds");
    assert_eq!(b.size(), DVec3::new(32.0, 32.0, 72.0));
}

#[test]
fn many_thumbnails_spread_over_frames() {
    init();
    let root = fake_install("syn_many");
    let mats = root.join("garrysmod/materials/many");
    std::fs::create_dir_all(&mats).expect("folder");
    let n = 150;
    for i in 0..n {
        let vmt = format!(r#""LightmappedGeneric" {{ "$basetexture" "many/t{i}" }}"#);
        std::fs::write(mats.join(format!("t{i}.vmt")), vmt).expect("vmt");
        let shade = (i * 37 % 255) as u8;
        std::fs::write(mats.join(format!("t{i}.vtf")), common::vtf(16, [shade, 255 - shade, 90, 255])).expect("vtf");
    }
    let mut am = AssetManager::new(&fake_settings(&root));
    assert!(am.wait_until_mounted(Duration::from_secs(30)));
    let names: Vec<String> = (0..n).map(|i| format!("many/t{i}")).collect();
    let mut frames = 0;
    let all = frames_until(&mut am, Duration::from_secs(30), |am, ctx| {
        frames += 1;
        let ready = names.iter().filter(|m| matches!(am.thumbnail(ctx, m), ThumbState::Ready { .. })).count();
        (ready == names.len()).then_some(ready)
    });
    assert_eq!(all, Some(n));
    assert!(frames >= 3, "texture creation is spread over frames ({frames})");
    assert_eq!(am.stats().thumbnails, n);
}
