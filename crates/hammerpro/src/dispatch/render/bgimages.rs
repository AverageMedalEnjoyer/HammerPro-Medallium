use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use hp_app::{AppState, Severity, t, tf};
use hp_render::{BackgroundImage, OrthoAxis, TexFormat, TexId, TextureUpload};

pub(crate) const BLOCK: &str = "bgimages_plus";
const MAX_FILE: u64 = 256 * 1024 * 1024;
const MAX_SIDE: u32 = 16384;
const EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "bmp", "tga"];
const UNDO: &str = "Edit background images";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BgEntry {
    pub path: String,
    pub view: u8,
    pub center: [f64; 2],
    pub scale: f64,
    pub rotation: f64,
    pub opacity: f32,
}

impl Default for BgEntry {
    fn default() -> Self {
        Self { path: String::new(), view: 0, center: [0.0; 2], scale: 1.0, rotation: 0.0, opacity: 0.5 }
    }
}

const VIEWS: [&str; 3] = ["top", "front", "side"];

fn num(b: &hp_kv::Block, key: &str, fallback: f64) -> f64 {
    b.get(key).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).unwrap_or(fallback)
}

pub(crate) fn read_entries(doc: &hp_editor::Document) -> Vec<BgEntry> {
    let Some(block) = doc.extra_block(BLOCK) else { return Vec::new() };
    block
        .children_named("image")
        .take(64)
        .filter_map(|b| {
            let path = b.get("path")?.trim().to_owned();
            if path.is_empty() {
                return None;
            }
            let view = VIEWS.iter().position(|v| b.get("view").is_some_and(|x| x.trim().eq_ignore_ascii_case(v)));
            let mut center = [0.0; 2];
            if let Some(c) = b.get("center") {
                for (slot, v) in center.iter_mut().zip(c.split_whitespace()) {
                    *slot = v.parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(0.0);
                }
            }
            Some(BgEntry {
                path,
                view: view.unwrap_or(0) as u8,
                center,
                scale: num(b, "scale", 1.0).clamp(1e-3, 1e3),
                rotation: num(b, "rotation", 0.0),
                opacity: num(b, "opacity", 0.5).clamp(0.0, 1.0) as f32,
            })
        })
        .collect()
}

pub(crate) fn write_entries(state: &mut AppState, entries: &[BgEntry], tag: &str) {
    let doc = &mut state.doc;
    doc.begin_merge(UNDO, format!("bgimages:{tag}"));
    let result = if entries.is_empty() {
        doc.remove_extra_block(BLOCK).map(|_| ())
    } else {
        let mut block = hp_kv::Block::new(BLOCK);
        for e in entries {
            let mut b = hp_kv::Block::new("image");
            b.set("path", e.path.as_str());
            b.set("view", VIEWS[usize::from(e.view.min(2))]);
            b.set("center", format!("{} {}", e.center[0], e.center[1]));
            b.set("scale", format!("{}", e.scale));
            b.set("rotation", format!("{}", e.rotation));
            b.set("opacity", format!("{}", e.opacity));
            block.push_child(b);
        }
        doc.set_extra_block(BLOCK, block)
    };
    match result {
        Ok(()) => {
            doc.commit();
        }
        Err(e) => {
            doc.cancel();
            state.notify(Severity::Warning, tf!("render.bg_edit_failed", e));
        }
    }
}

pub(crate) fn corners(e: &BgEntry, size: [u32; 2]) -> [[f32; 3]; 4] {
    let (hw, hh) = (f64::from(size[0]) * e.scale * 0.5, f64::from(size[1]) * e.scale * 0.5);
    let (s, c) = e.rotation.to_radians().sin_cos();
    let at = |x: f64, y: f64| {
        let (r, u) = (e.center[0] + x * c - y * s, e.center[1] + x * s + y * c);
        let p = match e.view {
            1 => [0.0, r, u],
            2 => [r, 0.0, u],
            _ => [r, u, 0.0],
        };
        p.map(|v| v as f32)
    };
    [at(-hw, hh), at(hw, hh), at(hw, -hh), at(-hw, -hh)]
}

fn axis(view: u8) -> OrthoAxis {
    match view {
        1 => OrthoAxis::Front,
        2 => OrthoAxis::Side,
        _ => OrthoAxis::Top,
    }
}

fn decode(path: &str) -> Result<TextureUpload, String> {
    let p = PathBuf::from(path);
    let ext = p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    if !EXTENSIONS.contains(&ext.as_str()) {
        return Err(t!("render.bg_not_image").to_owned());
    }
    let len = std::fs::metadata(&p).map_err(|e| e.to_string())?.len();
    if len > MAX_FILE {
        return Err(t!("render.bg_too_large").to_owned());
    }
    let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
    let mut reader =
        image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(1 << 30);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| e.to_string())?.to_rgba8();
    let (w, h) = img.dimensions();
    Ok(TextureUpload { width: w, height: h, format: TexFormat::Rgba8Srgb, levels: vec![img.into_raw()] })
}

enum Loaded {
    Loading,
    Ready(TexId, [u32; 2]),
    Failed(String),
}

pub(crate) struct BgImages {
    loaded: HashMap<String, Loaded>,
    tx: Sender<(String, Result<TextureUpload, String>)>,
    rx: Receiver<(String, Result<TextureUpload, String>)>,
    picked: Option<Receiver<Option<PathBuf>>>,
    synced: Option<(u64, usize)>,
    pub open: bool,
}

impl Default for BgImages {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { loaded: HashMap::new(), tx, rx, picked: None, synced: None, open: false }
    }
}

impl BgImages {
    pub fn tick(&mut self, state: &mut AppState) {
        let mut arrived = 0;
        while let Ok((path, result)) = self.rx.try_recv() {
            arrived += 1;
            let loaded = match (result, state.gfx.as_mut()) {
                (Ok(up), Some(gfx)) => match gfx.renderer.try_create_texture(&up) {
                    Ok(id) => Loaded::Ready(id, [up.width, up.height]),
                    Err(e) => Loaded::Failed(e.to_string()),
                },
                (Ok(_), None) => Loaded::Failed(String::new()),
                (Err(e), _) => Loaded::Failed(e),
            };
            self.loaded.insert(path, loaded);
        }
        if let Some(rx) = &self.picked
            && let Ok(picked) = rx.try_recv()
        {
            self.picked = None;
            if let Some(path) = picked {
                let mut entries = read_entries(&state.doc);
                let center = state.cameras.top.center;
                entries.push(BgEntry {
                    path: path.to_string_lossy().into_owned(),
                    center: [center.x, center.y],
                    ..Default::default()
                });
                write_entries(state, &entries, "add");
            }
        }
        let key = (state.doc.revision(), self.loaded.len() + arrived);
        if self.synced == Some(key) {
            return;
        }
        self.synced = Some(key);
        let entries = read_entries(&state.doc);
        let mut images = Vec::new();
        for e in &entries {
            match self.loaded.get(&e.path) {
                Some(Loaded::Ready(tex, size)) => images.push(BackgroundImage {
                    texture: *tex,
                    axis: axis(e.view),
                    corners: corners(e, *size),
                    opacity: e.opacity,
                }),
                Some(_) => {}
                None => {
                    self.loaded.insert(e.path.clone(), Loaded::Loading);
                    let (tx, path) = (self.tx.clone(), e.path.clone());
                    std::thread::spawn(move || {
                        let result = decode(&path);
                        let _ = tx.send((path, result));
                    });
                }
            }
        }
        let used: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        let stale: Vec<String> = self.loaded.keys().filter(|p| !used.contains(&p.as_str())).cloned().collect();
        for path in stale {
            if let Some(Loaded::Ready(tex, _)) = self.loaded.remove(&path)
                && let Some(gfx) = state.gfx.as_mut()
            {
                gfx.renderer.destroy_texture(tex);
            }
        }
        if let Some(gfx) = state.gfx.as_mut() {
            gfx.renderer.set_background_images(&images);
        }
    }

    pub fn window(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !self.open {
            return;
        }
        let mut open = true;
        let mut entries = read_entries(&state.doc);
        let mut changed: Option<&'static str> = None;
        let mut remove = None;
        hp_app::chrome::Window::new(t!("render.bg_title")).open(&mut open).resizable(true).default_width(460.0).show(
            ctx,
            |ui| {
                if entries.is_empty() {
                    ui.weak(t!("render.bg_empty"));
                }
                for (i, e) in entries.iter_mut().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.horizontal(|ui| {
                            let name = PathBuf::from(&e.path).file_name().map(|n| n.to_string_lossy().into_owned());
                            ui.strong(name.unwrap_or_else(|| e.path.clone())).on_hover_text(&e.path);
                            match self.loaded.get(&e.path) {
                                Some(Loaded::Failed(why)) => {
                                    ui.colored_label(hp_app::theme::error(), tf!("render.bg_failed", why));
                                }
                                Some(Loaded::Loading) | None => {
                                    ui.weak(t!("render.bg_loading"));
                                }
                                Some(Loaded::Ready(..)) => {}
                            }
                            if ui.small_button("🗑").on_hover_text(t!("render.bg_remove")).clicked() {
                                remove = Some(i);
                            }
                        });
                        egui::Grid::new("hp_bg_image").num_columns(2).show(ui, |ui| {
                            ui.label(t!("render.bg_view"));
                            egui::ComboBox::from_id_salt("hp_bg_view")
                                .selected_text(t!(
                                    ["render.bg_top", "render.bg_front", "render.bg_side"][usize::from(e.view.min(2))]
                                ))
                                .show_ui(ui, |ui| {
                                    for (v, key) in
                                        ["render.bg_top", "render.bg_front", "render.bg_side"].into_iter().enumerate()
                                    {
                                        if ui.selectable_value(&mut e.view, v as u8, t!(key)).changed() {
                                            changed = Some("view");
                                        }
                                    }
                                });
                            ui.end_row();
                            ui.label(t!("render.bg_center"));
                            ui.horizontal(|ui| {
                                for v in &mut e.center {
                                    if ui.add(egui::DragValue::new(v).speed(4.0)).changed() {
                                        changed = Some("center");
                                    }
                                }
                            });
                            ui.end_row();
                            ui.label(t!("render.bg_scale"));
                            if ui.add(egui::DragValue::new(&mut e.scale).speed(0.01).range(0.001..=1000.0)).changed() {
                                changed = Some("scale");
                            }
                            ui.end_row();
                            ui.label(t!("render.bg_rotation"));
                            if ui.add(egui::DragValue::new(&mut e.rotation).speed(0.5).suffix("°")).changed() {
                                changed = Some("rotation");
                            }
                            ui.end_row();
                            ui.label(t!("render.bg_opacity"));
                            if ui.add(egui::Slider::new(&mut e.opacity, 0.0..=1.0)).changed() {
                                changed = Some("opacity");
                            }
                            ui.end_row();
                        });
                        ui.separator();
                    });
                }
                let picking = self.picked.is_some();
                if ui.add_enabled(!picking, egui::Button::new(t!("render.bg_add"))).clicked() {
                    let (tx, rx) = channel();
                    self.picked = Some(rx);
                    let filter = t!("render.bg_filter").to_owned();
                    std::thread::spawn(move || {
                        let file = rfd::FileDialog::new().add_filter(filter, &EXTENSIONS).pick_file();
                        let _ = tx.send(file);
                    });
                }
            },
        );
        if let Some(i) = remove {
            entries.remove(i);
            changed = Some("remove");
        }
        if let Some(tag) = changed {
            write_entries(state, &entries, tag);
        }
        self.open = open;
    }
}

pub(crate) const TR: &[(&str, &str, &str)] = &[
    ("render.bg_title", "Background images", "Images de fond"),
    ("render.bg_empty", "No background image yet.", "Aucune image de fond pour l'instant."),
    ("render.bg_add", "Add an image...", "Ajouter une image..."),
    ("render.bg_remove", "Remove this image", "Retirer cette image"),
    ("render.bg_filter", "Images", "Images"),
    ("render.bg_view", "View", "Vue"),
    ("render.bg_top", "Top", "Dessus"),
    ("render.bg_front", "Front", "Face"),
    ("render.bg_side", "Side", "Côté"),
    ("render.bg_center", "Center", "Centre"),
    ("render.bg_scale", "Units per pixel", "Unités par pixel"),
    ("render.bg_rotation", "Rotation", "Rotation"),
    ("render.bg_opacity", "Opacity", "Opacité"),
    ("render.bg_loading", "Loading...", "Chargement..."),
    ("render.bg_failed", "Cannot load: {0}", "Impossible de charger : {0}"),
    ("render.bg_not_image", "not a PNG, JPEG, BMP or TGA image", "ce n'est pas une image PNG, JPEG, BMP ou TGA"),
    ("render.bg_too_large", "the file is too large", "le fichier est trop gros"),
    (
        "render.bg_edit_failed",
        "Cannot change the background images: {0}",
        "Impossible de modifier les images de fond : {0}",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_round_trip_through_the_map() {
        let mut state = hp_app::testing::test_state();
        assert!(read_entries(&state.doc).is_empty());
        let e = BgEntry {
            path: "C:/plans/floor.png".into(),
            view: 2,
            center: [10.0, -4.5],
            scale: 2.0,
            rotation: 30.0,
            opacity: 0.75,
        };
        write_entries(&mut state, std::slice::from_ref(&e), "add");
        assert_eq!(read_entries(&state.doc), vec![e.clone()]);
        assert!(state.doc.undo_label().is_some_and(|l| l == UNDO), "{:?}", state.doc.undo_label());
        write_entries(&mut state, &[], "remove");
        assert!(state.doc.extra_block(BLOCK).is_none());
    }

    #[test]
    fn corners_follow_view_scale_and_rotation() {
        let e = BgEntry { center: [100.0, 50.0], scale: 2.0, ..Default::default() };
        let c = corners(&e, [10, 4]);
        assert_eq!(c[0], [90.0, 54.0, 0.0], "top left");
        assert_eq!(c[2], [110.0, 46.0, 0.0], "bottom right");
        let front = corners(&BgEntry { view: 1, ..e.clone() }, [10, 4]);
        assert_eq!(front[0], [0.0, 90.0, 54.0]);
        let turned = corners(&BgEntry { rotation: 90.0, ..e }, [10, 4]);
        assert!((turned[0][0] - 96.0).abs() < 1e-4 && (turned[0][1] - 40.0).abs() < 1e-4, "{:?}", turned[0]);
    }

    #[test]
    fn hostile_blocks_and_files() {
        let mut state = hp_app::testing::test_state();
        let mut block = hp_kv::Block::new(BLOCK);
        let mut b = hp_kv::Block::new("image");
        b.set("path", "x.png");
        b.set("scale", "nan");
        b.set("center", "1e999 abc");
        b.set("opacity", "7");
        b.set("view", "nowhere");
        block.push_child(b);
        block.push_child(hp_kv::Block::new("image"));
        let _ = state.doc.set_extra_block(BLOCK, block);
        let entries = read_entries(&state.doc);
        assert_eq!(entries.len(), 1, "images without a path are skipped");
        assert_eq!(
            (entries[0].scale, entries[0].center, entries[0].opacity, entries[0].view),
            (1.0, [0.0, 0.0], 1.0, 0)
        );
        assert!(decode("C:/definitely/missing.png").is_err());
        assert!(decode("C:/Windows/notepad.exe").is_err(), "only images are read");
    }

    #[test]
    fn table_is_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty(), "{:#?}", hp_app::i18n::lint_table(TR));
    }
}
