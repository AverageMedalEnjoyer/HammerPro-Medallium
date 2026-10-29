use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};

use hp_app::settings::Settings;
use hp_app::state::{
    AssetStatus, AssetsApi, Gfx, ImportOptions, MaterialInfo, ModelState, PreviewCamera, SceneOptions, ThumbState,
};
use hp_app::{AppState, Tool};

struct Loaded {
    name: String,
    texture: Option<hp_fs::LoadedTexture>,
}

struct PreviewAssets {
    fs: hp_fs::GameFs,
    infos: HashMap<String, Arc<MaterialInfo>>,
    thumbs: HashMap<String, (ThumbState, Option<egui::TextureHandle>)>,
    request: Sender<String>,
    results: Receiver<Loaded>,
}

impl PreviewAssets {
    fn new(fs: hp_fs::GameFs) -> Self {
        let (request, rx_req) = std::sync::mpsc::channel::<String>();
        let (tx_res, results) = std::sync::mpsc::channel::<Loaded>();
        let worker_fs = fs.clone();
        std::thread::spawn(move || {
            while let Ok(name) = rx_req.recv() {
                let texture = worker_fs
                    .material(&name)
                    .and_then(|m| m.preview_texture())
                    .and_then(|t| worker_fs.load_texture(&t, Some(128)).ok());
                if tx_res.send(Loaded { name, texture }).is_err() {
                    return;
                }
            }
        });
        Self { fs, infos: HashMap::new(), thumbs: HashMap::new(), request, results }
    }
}

impl AssetsApi for PreviewAssets {
    fn status(&self) -> AssetStatus {
        AssetStatus::Ready
    }
    fn fs(&self) -> Option<&hp_fs::GameFs> {
        Some(&self.fs)
    }
    fn fgd(&self) -> Option<&hp_fgd::ResolvedFgd> {
        None
    }
    fn generation(&self) -> u64 {
        self.fs.generation()
    }
    fn remount(&mut self, _settings: &Settings) {
        self.fs.refresh();
    }
    fn reload_fgd(&mut self, _settings: &Settings) {}
    fn materials(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.fs.materials()
    }
    fn models(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.fs.models()
    }
    fn sounds(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.fs.sounds()
    }
    fn source_label(&self, source: u32) -> String {
        self.fs.source(source).map(|s| s.label).unwrap_or_default()
    }
    fn material_info(&mut self, name: &str) -> Option<Arc<MaterialInfo>> {
        if let Some(i) = self.infos.get(name) {
            return Some(i.clone());
        }
        let m = self.fs.material(name);
        let (width, height) =
            m.as_ref().and_then(|m| m.preview_texture()).and_then(|t| self.fs.texture_size(&t)).unwrap_or((0, 0));
        let info = Arc::new(MaterialInfo {
            name: name.to_owned(),
            width,
            height,
            shader: m.as_ref().map(|m| m.shader.clone()).unwrap_or_default(),
            tool: m.as_ref().and_then(|m| m.tool_kind(name)),
            translucent: m.as_ref().is_some_and(|m| m.translucent()),
            alpha_test: m.as_ref().is_some_and(|m| m.alpha_test()),
            surface_prop: m.as_ref().and_then(|m| m.surface_prop().map(str::to_owned)),
            keywords: m.as_ref().map(|m| m.keywords().map(str::to_owned).collect()).unwrap_or_default(),
            missing: m.is_none(),
            ..MaterialInfo::default()
        });
        self.infos.insert(name.to_owned(), info.clone());
        Some(info)
    }
    fn thumbnail(&mut self, ctx: &egui::Context, name: &str) -> ThumbState {
        while let Ok(done) = self.results.try_recv() {
            let entry = match done.texture {
                Some(t) => {
                    let img = egui::ColorImage::from_rgba_unmultiplied([t.width as usize, t.height as usize], &t.rgba);
                    let handle = ctx.load_texture(&done.name, img, egui::TextureOptions::LINEAR);
                    let state = ThumbState::Ready { texture: handle.id(), size: [t.original_width, t.original_height] };
                    (state, Some(handle))
                }
                None => (ThumbState::Missing, None),
            };
            self.thumbs.insert(done.name, entry);
            ctx.request_repaint();
        }
        if let Some((s, _)) = self.thumbs.get(name) {
            return *s;
        }
        self.thumbs.insert(name.to_owned(), (ThumbState::Loading, None));
        let _ = self.request.send(name.to_owned());
        ThumbState::Loading
    }
    fn model_info(&mut self, _path: &str) -> ModelState {
        ModelState::Failed("not available in the preview".into())
    }
    fn model_preview(&mut self, _: &mut Gfx, _: &str, _: &PreviewCamera, _: [u32; 2]) -> Option<egui::TextureId> {
        None
    }
    fn entity_bounds(&self, entity: &hp_vmf::Entity) -> Option<hp_geom::Aabb> {
        let o = entity.origin()?;
        Some(hp_geom::Aabb::new(o - glam::DVec3::splat(8.0), o + glam::DVec3::splat(8.0)))
    }
    fn invalidate_material(&mut self, name: &str) {
        self.infos.remove(name);
        self.thumbs.remove(name);
    }
    fn import_images(&mut self, files: &[PathBuf], _opts: &ImportOptions) -> Vec<Result<String, String>> {
        files.iter().map(|_| Err("not available in the preview".to_owned())).collect()
    }
    fn begin_frame(&mut self, _ctx: &egui::Context, _gfx: Option<&mut Gfx>) {}
    fn sync_scene(&mut self, doc: &mut hp_editor::Document, _gfx: &mut Gfx, _opts: &SceneOptions) {
        let _ = doc.take_changes();
    }
    fn render_material_id(&mut self, _name: &str) -> hp_render::MaterialId {
        0
    }
    fn status_line(&self) -> Option<String> {
        None
    }
}

struct Args {
    gmod: Option<PathBuf>,
    custom: Option<PathBuf>,
    tab: String,
    dialog: Option<String>,
    search: Option<String>,
    french: bool,
    screenshot: Option<PathBuf>,
    frames: u64,
}

fn parse_args() -> Args {
    let mut a = Args {
        gmod: None,
        custom: None,
        tab: "textures".into(),
        dialog: None,
        search: None,
        french: false,
        screenshot: None,
        frames: 90,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--gmod" => a.gmod = it.next().map(PathBuf::from),
            "--custom" => a.custom = it.next().map(PathBuf::from),
            "--tab" => a.tab = it.next().unwrap_or_default(),
            "--dialog" => a.dialog = it.next(),
            "--search" => a.search = it.next(),
            "--lang" => a.french = it.next().is_some_and(|l| l == "fr"),
            "--screenshot" => a.screenshot = it.next().map(PathBuf::from),
            "--frames" => a.frames = it.next().and_then(|n| n.parse().ok()).unwrap_or(90),
            _ => eprintln!("unknown argument {arg}"),
        }
    }
    a
}

struct PreviewApp {
    state: AppState,
    browser: hp_ui_textures::TextureBrowser,
    dialogs: hp_ui_textures::TextureDialogs,
    tool: Box<dyn Tool>,
    args: Args,
    frame: u64,
    shot_requested: bool,
}

impl eframe::App for PreviewApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frame += 1;
        self.state.frame.index = self.frame;
        egui::Panel::right("dock").default_size(360.0).resizable(true).show(ui, |ui| {
            ui.horizontal(|ui| {
                let tex = self.state.ui.right_tab == hp_app::state::RightTab::Textures;
                if ui.add(egui::Button::selectable(tex, "Textures")).clicked() {
                    self.state.ui.right_tab = hp_app::state::RightTab::Textures;
                }
                if ui.add(egui::Button::selectable(!tex, "Face edit")).clicked() {
                    self.state.ui.right_tab = hp_app::state::RightTab::Tool;
                }
            });
            ui.separator();
            if self.state.ui.right_tab == hp_app::state::RightTab::Textures {
                self.browser.ui(ui, &mut self.state);
            } else {
                self.tool.options_ui(ui, &mut self.state);
            }
        });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Hammer Pro texture panels preview");
            ui.label(format!("Current material: {}", self.state.current_material));
            for t in self.state.toasts.live() {
                ui.colored_label(t.severity.color(), &t.text);
            }
        });
        self.dialogs.show(&ctx, &mut self.state);
        if let Some(tool) = self.state.requested_tool.take() {
            self.state.tool = tool;
        }
        self.state.requested_actions.clear();

        if let Some(path) = self.args.screenshot.clone() {
            if self.frame >= self.args.frames && !self.shot_requested {
                self.shot_requested = true;
                let close_ctx = ctx.clone();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(
                    move |img: Arc<egui::ColorImage>| {
                        let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                        let r = image::save_buffer(
                            &path,
                            &bytes,
                            img.size[0] as u32,
                            img.size[1] as u32,
                            image::ExtendedColorType::Rgba8,
                        );
                        if let Err(e) = r {
                            eprintln!("cannot save the screenshot: {e}");
                        }
                        close_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        close_ctx.request_repaint();
                    },
                )));
            }
            ctx.request_repaint();
        }
    }
}

fn main() -> eframe::Result {
    let args = parse_args();
    hp_app::register_translations();
    hp_ui_textures::register_translations();
    if args.french {
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
    }
    let fs = match args.gmod.clone() {
        Some(root) => {
            let opts = hp_fs::MountOptions { mount_workshop: false, ..hp_fs::MountOptions::default() };
            hp_fs::GameFs::for_gmod(&root, &opts).unwrap_or_default()
        }
        None => hp_fs::GameFs::empty(),
    };
    let mut settings = Settings::default();
    if let Some(c) = &args.custom {
        hp_ui_textures::folders::add_custom_dir(&mut settings, c);
        if let Err(e) = fs.add_custom_dir(hp_fs::CustomDir::new(c)) {
            eprintln!("cannot mount {}: {e}", c.display());
        }
    }
    let mut state = AppState::new(settings, Box::new(PreviewAssets::new(fs)));
    let sample = PathBuf::from(r"C:\Users\oracle\Desktop\hammermcptest\test.vmf");
    if let Ok(doc) = hp_editor::Document::open(&sample) {
        state.doc = doc;
    } else if let Ok(map) = hp_vmf::parse(hp_app::testing::TEST_VMF) {
        state.doc = hp_editor::Document::from_map(map);
    }
    let mut browser = hp_ui_textures::TextureBrowser::new();
    if let Some(s) = &args.search {
        browser.set_search(s);
    }
    let mut tool = hp_ui_textures::texture_tool();
    if args.tab == "face" {
        state.ui.right_tab = hp_app::state::RightTab::Tool;
        let faces: Vec<hp_editor::FaceRef> = state
            .doc
            .scene()
            .solids()
            .values()
            .take(3)
            .flat_map(|n| (0..n.solid.sides.len().min(2)).map(move |i| hp_editor::FaceRef::new(n.solid.id, i)))
            .collect();
        state.doc.select_faces(&faces, hp_editor::SelectOp::Replace);
        state.tool = hp_app::ToolId::Texture;
        tool.activate(&mut state);
    }
    match args.dialog.as_deref() {
        Some("replace") => state.ui.show_replace_textures = true,
        Some("editor") => state.ui.show_material_editor = true,
        _ => {}
    }
    let app = PreviewApp {
        state,
        browser,
        dialogs: hp_ui_textures::TextureDialogs::new(),
        tool,
        args,
        frame: 0,
        shot_requested: false,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 860.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Hammer Pro texture preview",
        options,
        Box::new(move |cc| {
            hp_app::theme::apply(&cc.egui_ctx, hp_app::settings::ThemeChoice::Dark, 1.0);
            if let Some(dialog) = app.args.dialog.as_deref()
                && dialog == "import"
            {
                hp_ui_textures::shared::post_import(
                    &cc.egui_ctx,
                    vec![PathBuf::from("bricks.png"), PathBuf::from("wall.jpg")],
                );
            }
            Ok(Box::new(app))
        }),
    )
}
