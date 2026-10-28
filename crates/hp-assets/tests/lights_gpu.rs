mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{fake_install, fake_settings};
use glam::DVec3;
use hp_app::AssetsApi;
use hp_app::state::{Gfx, SceneOptions};
use hp_assets::AssetManager;
use hp_editor::{Document, ObjRef};
use hp_render::ObjectFlags;

fn gfx() -> Option<Gfx> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))
    .ok()?;
    if !adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VERTEX_STORAGE) {
        return None;
    }
    let features = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("hp-assets lights test"),
        required_features: features,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .ok()?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let egui_renderer = egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());
    let rs = egui_wgpu::RenderState {
        adapter,
        available_adapters: Vec::new(),
        instance,
        device,
        queue,
        target_format: format,
        renderer: Arc::new(egui::mutex::RwLock::new(egui_renderer)),
    };
    Some(Gfx::new(&rs))
}

fn settle(am: &mut AssetManager, doc: &mut Document, gfx: &mut Gfx, o: &SceneOptions) -> bool {
    let ctx = egui::Context::default();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        am.begin_frame(&ctx, Some(gfx));
        am.sync_scene(doc, gfx, o);
        let s = am.stats();
        if s.queued_jobs == 0
            && s.pending_uploads == 0
            && s.textures_loading == 0
            && s.models_loading == 0
            && s.materials_loading == 0
        {
            am.begin_frame(&ctx, Some(gfx));
            am.sync_scene(doc, gfx, o);
            return true;
        }
        if Instant::now() > deadline {
            eprintln!("not settled: {s:?}");
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn block(id: u32, lo: DVec3, hi: DVec3, material: &str) -> hp_vmf::Solid {
    let p = DVec3::new;
    let planes = [
        [p(lo.x, hi.y, hi.z), p(hi.x, hi.y, hi.z), p(hi.x, lo.y, hi.z)],
        [p(lo.x, lo.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, hi.y, lo.z)],
        [p(lo.x, hi.y, hi.z), p(lo.x, lo.y, hi.z), p(lo.x, lo.y, lo.z)],
        [p(hi.x, hi.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, lo.y, hi.z)],
        [p(hi.x, hi.y, hi.z), p(lo.x, hi.y, hi.z), p(lo.x, hi.y, lo.z)],
        [p(hi.x, lo.y, lo.z), p(lo.x, lo.y, lo.z), p(lo.x, lo.y, hi.z)],
    ];
    let sides = planes
        .iter()
        .enumerate()
        .map(|(k, plane)| hp_vmf::Side {
            id: id * 10 + k as u32,
            plane: *plane,
            material: material.to_owned(),
            uaxis: hp_vmf::TexAxis { axis: DVec3::X, shift: 0.0, scale: 0.25 },
            vaxis: hp_vmf::TexAxis { axis: DVec3::NEG_Y, shift: 0.0, scale: 0.25 },
            ..Default::default()
        })
        .collect();
    hp_vmf::Solid { id, sides, ..Default::default() }
}

#[test]
fn lighting_preview_lights_follow_the_game() {
    hp_app::register_translations();
    hp_assets::register_translations();
    let Some(mut gfx) = gfx() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let root = fake_install("lights_gpu");
    let mut am = AssetManager::new(&fake_settings(&root));
    assert!(am.wait_until_mounted(Duration::from_secs(30)));
    let mut map = hp_vmf::Map::new_empty();
    map.world.solids.push(block(1, DVec3::new(-512.0, -512.0, -16.0), DVec3::new(512.0, 512.0, 0.0), "TEST/BRICK"));
    map.world.solids.push(block(2, DVec3::new(100.0, 100.0, 0.0), DVec3::new(164.0, 164.0, 128.0), "TEST/BRICK"));
    map.world.solids.push(block(
        3,
        DVec3::new(-164.0, -164.0, 0.0),
        DVec3::new(-100.0, -100.0, 128.0),
        "TOOLS/TOOLSTRIGGER",
    ));
    let mut ent = |id: u32, class: &str, props: &[(&str, &str)]| {
        let mut e = hp_vmf::Entity { id, classname: class.to_owned(), ..Default::default() };
        for (k, v) in props {
            e.set(k, *v);
        }
        map.entities.push(e);
    };
    ent(10, "light", &[("origin", "132 132 64"), ("_light", "255 200 150 300"), ("_quadratic_attn", "1")]);
    ent(11, "light", &[("origin", "-132 -132 64"), ("_light", "255 200 150 300"), ("_quadratic_attn", "1")]);
    ent(12, "light", &[("origin", "0 0 64"), ("_light", "255 200 150 300"), ("_quadratic_attn", "1")]);
    ent(13, "light_environment", &[("angles", "-45 30 0"), ("pitch", "-45"), ("_light", "255 255 255 200")]);
    ent(
        14,
        "light_dynamic",
        &[("origin", "300 0 50"), ("_light", "255 255 255"), ("brightness", "2"), ("distance", "256")],
    );
    ent(20, "prop_static", &[("origin", "-300 0 0"), ("disableshadows", "1")]);
    ent(21, "prop_static", &[("origin", "-300 100 0")]);
    let mut doc = Document::from_map(map);
    let lit = SceneOptions { show_models: true, show_helpers: true, show_tool_textures: true, lighting_preview: true };
    assert!(settle(&mut am, &mut doc, &mut gfx, &lit));
    assert_eq!(gfx.renderer.light_count(), 4);
    let flags = |gfx: &Gfx, id: u32| gfx.renderer.object_flags(id | 0x8000_0000).expect("entity object");
    assert!(flags(&gfx, 20).contains(ObjectFlags::NO_SHADOW));
    assert!(!flags(&gfx, 21).contains(ObjectFlags::NO_SHADOW));
    doc.translate(&[ObjRef::Solid(2)], DVec3::new(0.0, 300.0, 0.0)).expect("move");
    assert!(settle(&mut am, &mut doc, &mut gfx, &lit));
    assert_eq!(gfx.renderer.light_count(), 5);
    doc.translate(&[ObjRef::Entity(12)], DVec3::new(132.0, 432.0, 0.0)).expect("move");
    assert!(settle(&mut am, &mut doc, &mut gfx, &lit));
    assert_eq!(gfx.renderer.light_count(), 4);
    let plain = SceneOptions { lighting_preview: false, ..lit };
    assert!(settle(&mut am, &mut doc, &mut gfx, &plain));
    assert_eq!(gfx.renderer.light_count(), 0);
}
