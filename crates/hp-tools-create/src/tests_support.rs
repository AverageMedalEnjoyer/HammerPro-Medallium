#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use egui::{Pos2, Rect};
use glam::{DVec2, DVec3};
use hp_app::settings::Settings;
use hp_app::state::{
    AppState, AssetStatus, AssetsApi, Gfx, ImportOptions, MaterialInfo, ModelInfo, ModelState, PreviewCamera,
    SceneOptions, ThumbState,
};
use hp_app::testing::MockAssets;
use hp_app::view::{Camera2d, Camera3d};
use hp_app::{ViewInfo, ViewKind};

pub(crate) fn settle() -> AppState {
    let state = hp_app::testing::test_state();
    crate::register_translations();
    state
}

pub(crate) fn info_2d(kind: ViewKind, zoom: f64) -> ViewInfo {
    let index = match kind {
        ViewKind::Top => 1,
        ViewKind::Front => 2,
        ViewKind::Side => 3,
        ViewKind::Perspective => 0,
    };
    ViewInfo {
        index,
        kind,
        rect: Rect::from_min_size(Pos2::new(0.0, 0.0), egui::vec2(800.0, 600.0)),
        cam3d: Camera3d::default(),
        cam2d: Camera2d { center: DVec2::ZERO, zoom },
        pixels_per_point: 1.0,
        grid: 16.0,
    }
}

pub(crate) fn info_3d(pos: DVec3, yaw_deg: f64, pitch_deg: f64) -> ViewInfo {
    ViewInfo {
        index: 0,
        kind: ViewKind::Perspective,
        rect: Rect::from_min_size(Pos2::new(0.0, 0.0), egui::vec2(800.0, 600.0)),
        cam3d: Camera3d { pos, yaw_deg, pitch_deg, ..Camera3d::default() },
        cam2d: Camera2d::default(),
        pixels_per_point: 1.0,
        grid: 16.0,
    }
}

pub(crate) fn info_3d_looking_down(pos: DVec3) -> ViewInfo {
    info_3d(pos, 0.0, -89.0)
}

pub(crate) const TEST_FGD: &str = r#"
@BaseClass = Targetname [ targetname(target_source) : "Name" : : "The name." ]
@BaseClass = Angles [ angles(angle) : "Pitch Yaw Roll (Y Z X)" : "0 0 0" ]
@PointClass base(Angles) size(-16 -16 0, 16 16 72) color(0 255 0) = info_player_start : "Player spawn point." []
@PointClass base(Angles) studio("models/editor/playerstart.mdl") = info_player_model : "Spawn point drawn with its model, like Garry's Mod." []
@PointClass base(Targetname) color(255 255 0) = light : "An invisible light source."
[
    _light(color255) : "Brightness" : "255 255 255 200"
    spawnflags(flags) = [ 1 : "Initially dark" : 0 ]
]
@PointClass base(Targetname, Angles) studioprop() = prop_physics : "A physics prop."
[
    model(studio) : "World Model"
    skin(integer) : "Skin" : 0
    spawnflags(flags) = [ 1 : "Start asleep" : 1  2 : "Motion disabled" : 0 ]
]
@PointClass size(-8 -8 -8, 8 8 8) = infodecal : "A static decal."
[
    texture(decal) : "Texture"
    LowPriority(choices) : "Low Priority" : 0 = [ 0 : "No" 1 : "Yes" ]
]
@PointClass = info_overlay : "An overlay."
[
    material(material) : "Material"
    sides(sidelist) : "Brush faces"
    StartU(float) : "U Start" : "0.0"
    EndU(float) : "U End" : "1.0"
    fademindist(float) : "Start Fade Dist" : "-1"
]
@SolidClass = func_detail : "Detail brushes." []
@SolidClass base(Targetname) = trigger_multiple : "A trigger."
[
    wait(integer) : "Delay Before Reset" : 1
    spawnflags(flags) = [ 1 : "Clients" : 1 ]
]
"#;

pub(crate) struct FgdAssets {
    inner: MockAssets,
    fgd: hp_fgd::ResolvedFgd,
    pub(crate) models: Vec<ModelInfo>,
}

impl FgdAssets {
    pub(crate) fn new() -> Self {
        Self::from_text(TEST_FGD)
    }

    pub(crate) fn from_text(text: &str) -> Self {
        let fgd = hp_fgd::Fgd::parse(text, None);
        assert!(fgd.warnings.is_empty(), "{:#?}", fgd.warnings);
        Self { inner: MockAssets::default(), fgd: fgd.resolve(), models: Vec::new() }
    }
}

impl AssetsApi for FgdAssets {
    fn status(&self) -> AssetStatus {
        self.inner.status()
    }
    fn fs(&self) -> Option<&hp_fs::GameFs> {
        self.inner.fs()
    }
    fn fgd(&self) -> Option<&hp_fgd::ResolvedFgd> {
        Some(&self.fgd)
    }
    fn generation(&self) -> u64 {
        self.inner.generation()
    }
    fn remount(&mut self, settings: &Settings) {
        self.inner.remount(settings);
    }
    fn reload_fgd(&mut self, settings: &Settings) {
        self.inner.reload_fgd(settings);
    }
    fn materials(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.inner.materials()
    }
    fn models(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.inner.models()
    }
    fn sounds(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.inner.sounds()
    }
    fn source_label(&self, source: u32) -> String {
        self.inner.source_label(source)
    }
    fn material_info(&mut self, name: &str) -> Option<Arc<MaterialInfo>> {
        self.inner.material_info(name)
    }
    fn thumbnail(&mut self, ctx: &egui::Context, name: &str) -> ThumbState {
        self.inner.thumbnail(ctx, name)
    }
    fn model_info(&mut self, path: &str) -> ModelState {
        match self.models.iter().find(|m| m.path.eq_ignore_ascii_case(path)) {
            Some(m) => ModelState::Ready(Arc::new(m.clone())),
            None => self.inner.model_info(path),
        }
    }
    fn model_preview(
        &mut self,
        gfx: &mut Gfx,
        path: &str,
        camera: &PreviewCamera,
        size_px: [u32; 2],
    ) -> Option<egui::TextureId> {
        self.inner.model_preview(gfx, path, camera, size_px)
    }
    fn entity_bounds(&self, entity: &hp_vmf::Entity) -> Option<hp_geom::Aabb> {
        self.inner.entity_bounds(entity)
    }
    fn invalidate_material(&mut self, name: &str) {
        self.inner.invalidate_material(name);
    }
    fn import_images(&mut self, files: &[PathBuf], opts: &ImportOptions) -> Vec<Result<String, String>> {
        self.inner.import_images(files, opts)
    }
    fn begin_frame(&mut self, ctx: &egui::Context, gfx: Option<&mut Gfx>) {
        self.inner.begin_frame(ctx, gfx);
    }
    fn sync_scene(&mut self, doc: &mut hp_editor::Document, gfx: &mut Gfx, opts: &SceneOptions) {
        self.inner.sync_scene(doc, gfx, opts);
    }
    fn render_material_id(&mut self, name: &str) -> hp_render::MaterialId {
        self.inner.render_material_id(name)
    }
    fn status_line(&self) -> Option<String> {
        self.inner.status_line()
    }
}

pub(crate) fn fgd_state() -> AppState {
    fgd_state_with_models(Vec::new())
}

pub(crate) fn fgd_state_with_models(models: Vec<(String, DVec3, DVec3)>) -> AppState {
    let infos = models
        .into_iter()
        .map(|(path, bounds_min, bounds_max)| ModelInfo { path, bounds_min, bounds_max, ..Default::default() })
        .collect();
    fgd_state_with_model_infos(infos)
}

pub(crate) fn fgd_state_with_model_infos(models: Vec<ModelInfo>) -> AppState {
    let mut state = settle();
    let mut assets = FgdAssets::new();
    assets.models = models;
    state.assets = Box::new(assets);
    state
}

pub(crate) fn fgd_state_from(text: &str) -> AppState {
    let mut state = settle();
    state.assets = Box::new(FgdAssets::from_text(text));
    state
}
