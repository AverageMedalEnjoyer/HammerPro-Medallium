use std::path::PathBuf;

use hp_app::AppState;
use hp_editor::ObjRef;
use hp_ui_entities::{EntityDialogs, EntityPanel, EntityTab, ModelBrowser};

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: egui_wgpu::Renderer,
}

fn gpu() -> Option<Gpu> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))
    .ok()?;
    let (device, queue) = pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor { label: Some("screenshot"), ..Default::default() }),
    )
    .ok()?;
    let renderer =
        egui_wgpu::Renderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb, egui_wgpu::RendererOptions::default());
    Some(Gpu { device, queue, renderer })
}

fn scratch() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-ui-entities/scratch");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn shot(
    gpu: &mut Gpu,
    state: &mut AppState,
    name: &str,
    size: [u32; 2],
    clicks: &[egui::Pos2],
    mut f: impl FnMut(&mut egui::Ui, &mut AppState),
) {
    let ctx = egui::Context::default();
    hp_app::theme::apply(&ctx, hp_app::settings::ThemeChoice::Dark, 1.0);
    let mut last = None;
    let mut frames: Vec<Vec<egui::Event>> = vec![Vec::new(), Vec::new()];
    for p in clicks {
        frames.push(vec![egui::Event::PointerMoved(*p)]);
        frames.push(vec![egui::Event::PointerButton {
            pos: *p,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        frames.push(vec![egui::Event::PointerButton {
            pos: *p,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        frames.push(Vec::new());
    }
    frames.push(Vec::new());
    frames.push(Vec::new());
    for (frame, events) in frames.into_iter().enumerate() {
        let input = egui::RawInput {
            time: Some(frame as f64 * 0.25),
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0] as f32, size[1] as f32))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| f(ui, state));
        for (id, deltas) in &out.textures_delta.set {
            for delta in deltas {
                gpu.renderer.update_texture(&gpu.device, &gpu.queue, *id, delta);
            }
        }
        for id in &out.textures_delta.free {
            gpu.renderer.free_texture(id);
        }
        out.textures_delta.clear();
        last = Some(out);
    }
    let Some(out) = last else { return };
    let prims = ctx.tessellate(out.shapes, 1.0);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("screenshot target"),
        size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: size, pixels_per_point: 1.0 };
    let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("screenshot") });
    let extra = gpu.renderer.update_buffers(&gpu.device, &gpu.queue, &mut enc, &prims, &screen);
    {
        let pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("screenshot pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.05, g: 0.05, b: 0.06, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let mut pass = pass.forget_lifetime();
        gpu.renderer.render(&mut pass, &prims, &screen);
    }
    let [w, h] = size;
    let row = w * 4;
    let padded = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("screenshot readback"),
        size: u64::from(padded) * u64::from(h),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    gpu.queue.submit(extra.into_iter().chain([enc.finish()]));
    let (tx, rx) = std::sync::mpsc::channel();
    buf.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    if gpu.device.poll(wgpu::PollType::wait_indefinitely()).is_err() {
        return;
    }
    if !matches!(rx.recv(), Ok(Ok(()))) {
        return;
    }
    let mut pixels = Vec::with_capacity((row * h) as usize);
    if let Ok(mapped) = buf.slice(..).get_mapped_range() {
        for y in 0..h as usize {
            let start = y * padded as usize;
            if let Some(r) = mapped.get(start..start + row as usize) {
                pixels.extend_from_slice(r);
            }
        }
    }
    buf.unmap();
    let path = scratch().join(format!("{name}.png"));
    match image::save_buffer(&path, &pixels, w, h, image::ExtendedColorType::Rgba8) {
        Ok(()) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("could not write {}: {e}", path.display()),
    }
}

fn dock(ui: &mut egui::Ui, f: impl FnOnce(&mut egui::Ui)) {
    egui::Panel::right("dock").exact_size(360.0).show(ui, f);
    egui::CentralPanel::default().show(ui, |_| {});
}

fn main() {
    let Some(mut gpu) = gpu() else {
        eprintln!("no GPU adapter: nothing rendered");
        return;
    };
    let bin = std::path::Path::new("D:/SteamLibrary/steamapps/common/GarrysMod/bin");
    let fgd = hp_fgd::Fgd::load_with_include_dirs([bin.join("garrysmod.fgd")], [bin]).ok().map(|f| f.resolve());
    let fgd = fgd.as_ref();
    let lang = std::env::args().nth(1).unwrap_or_default();
    let mut state = hp_app::testing::test_state();
    hp_ui_entities::register_translations();
    if lang == "fr" {
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
    }
    let suffix = if lang == "fr" { "_fr" } else { "" };
    let mut panel = EntityPanel::new();
    let none: &[egui::Pos2] = &[];

    state.doc.set_selection(&[ObjRef::Entity(5)]);
    let tabs = [
        (EntityTab::Properties, "light_props"),
        (EntityTab::Flags, "light_flags"),
        (EntityTab::Raw, "light_raw"),
        (EntityTab::Inputs, "light_inputs"),
    ];
    for (tab, name) in tabs {
        panel.set_tab(tab);
        shot(&mut gpu, &mut state, &format!("{name}{suffix}"), [700, 760], none, |ui, state| {
            dock(ui, |ui| panel.ui_with_fgd(ui, state, fgd))
        });
    }
    state.doc.set_selection(&[ObjRef::Entity(6)]);
    panel.set_tab(EntityTab::Outputs);
    shot(
        &mut gpu,
        &mut state,
        &format!("trigger_outputs{suffix}"),
        [700, 760],
        &[egui::pos2(500.0, 143.0)],
        |ui, state| dock(ui, |ui| panel.ui_with_fgd(ui, state, fgd)),
    );
    panel.set_tab(EntityTab::Properties);
    shot(&mut gpu, &mut state, &format!("trigger_props{suffix}"), [700, 760], none, |ui, state| {
        dock(ui, |ui| panel.ui_with_fgd(ui, state, fgd))
    });
    state.doc.set_selection(&[ObjRef::Entity(4), ObjRef::Entity(5)]);
    shot(&mut gpu, &mut state, &format!("multi{suffix}"), [700, 760], none, |ui, state| {
        dock(ui, |ui| panel.ui_with_fgd(ui, state, fgd))
    });
    state.doc.set_selection(&[ObjRef::Solid(2), ObjRef::Solid(3)]);
    shot(&mut gpu, &mut state, &format!("world_brushes{suffix}"), [700, 500], none, |ui, state| {
        dock(ui, |ui| panel.ui_with_fgd(ui, state, fgd))
    });
    state.doc.clear_selection();
    shot(&mut gpu, &mut state, &format!("worldspawn{suffix}"), [700, 600], none, |ui, state| {
        dock(ui, |ui| panel.ui_with_fgd(ui, state, fgd))
    });
    state.doc.set_selection(&[ObjRef::Entity(5)]);
    shot(
        &mut gpu,
        &mut state,
        &format!("class_picker{suffix}"),
        [700, 760],
        &[egui::pos2(500.0, 18.0)],
        |ui, state| dock(ui, |ui| panel.ui_with_fgd(ui, state, fgd)),
    );

    let mut browser = ModelBrowser::new();
    browser.select("models/props_c17/oildrum001.mdl");
    for _ in 0..20 {
        shot(&mut gpu, &mut state, &format!("models{suffix}"), [700, 800], none, |ui, state| {
            dock(ui, |ui| browser.ui(ui, state))
        });
        if browser.row_count() > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    shot(&mut gpu, &mut state, &format!("models{suffix}"), [700, 800], none, |ui, state| {
        dock(ui, |ui| browser.ui(ui, state))
    });

    let mut dialogs = EntityDialogs::new();
    state.ui.show_find_entities = true;
    state.ui.show_entity_report = true;
    state.ui.show_map_properties = true;
    dialogs.set_find_query("light");
    shot(&mut gpu, &mut state, &format!("dialogs{suffix}"), [1400, 900], none, |ui, state| {
        let ctx = ui.ctx().clone();
        dialogs.show_with_fgd(&ctx, state, fgd);
    });
}
