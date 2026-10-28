use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};

use hp_app::AppState;

pub fn normalize_material_into(name: &str, out: &mut String) {
    out.clear();
    let name = name.trim();
    for c in name.chars() {
        if c == '\\' {
            out.push('/');
        } else {
            out.push(c.to_ascii_lowercase());
        }
    }
    if out.starts_with("materials/") {
        out.replace_range(.."materials/".len(), "");
    }
    if out.ends_with(".vmt") {
        let n = out.len() - 4;
        out.truncate(n);
    }
    while out.starts_with('/') {
        out.remove(0);
    }
    while out.ends_with('/') {
        out.pop();
    }
}

pub fn normalize_material(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    normalize_material_into(name, &mut out);
    out
}

pub fn leaf_name(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

pub fn folder_of(name: &str) -> &str {
    name.rfind('/').map_or("", |i| &name[..i])
}

pub fn is_tool_material(name: &str, info: Option<&hp_app::state::MaterialInfo>) -> bool {
    name.starts_with("tools/") || info.is_some_and(|i| i.tool.is_some())
}

pub fn material_sizes<'a>(
    state: &mut AppState,
    names: impl IntoIterator<Item = &'a str>,
) -> HashMap<String, (u32, u32)> {
    let mut out = HashMap::new();
    for name in names {
        let key = normalize_material(name);
        if key.is_empty() || out.contains_key(&key) {
            continue;
        }
        let loaded = state
            .assets
            .material_info(&key)
            .filter(|info| info.width > 0 && info.height > 0)
            .map(|info| (info.width, info.height));
        let size = loaded.or_else(|| state.assets.fs().and_then(|fs| fs.material_texture_size(&key)));
        if let Some(size) = size.filter(|(w, h)| *w > 0 && *h > 0) {
            out.insert(key, size);
        }
    }
    out
}

pub fn size_lookup(sizes: &HashMap<String, (u32, u32)>) -> impl Fn(&str) -> Option<(u32, u32)> + '_ {
    move |name: &str| sizes.get(&normalize_material(name)).copied()
}

pub fn reveal_in_file_manager(path: &Path) -> std::io::Result<()> {
    hp_app::os::reveal(path)
}

pub fn reveal_or_log(state: &mut AppState, path: &Path) {
    if let Err(e) = reveal_in_file_manager(path) {
        state.notify(hp_app::Severity::Error, hp_app::tf!("tex.err.open_folder", path.display(), e));
    }
}

pub struct PendingPick<T> {
    rx: Receiver<Option<T>>,
}

impl<T: Send + 'static> PendingPick<T> {
    pub fn spawn(ctx: &egui::Context, pick: impl FnOnce() -> Option<T> + Send + 'static) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        #[cfg(target_os = "macos")]
        {
            let _ = ctx;
            let _ = tx.send(pick());
        }
        #[cfg(not(target_os = "macos"))]
        {
            let ctx = ctx.clone();
            let spawned = std::thread::Builder::new().name("hp-file-dialog".into()).spawn(move || {
                let result = pick();
                let _ = tx.send(result);
                ctx.request_repaint();
            });
            if let Err(e) = spawned {
                log::warn!("cannot start the file dialog thread: {e}");
            }
        }
        Self { rx }
    }

    pub fn poll(&self) -> Option<Option<T>> {
        match self.rx.try_recv() {
            Ok(v) => Some(v),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(None),
        }
    }
}

pub fn take_key(ctx: &egui::Context, modifiers: egui::Modifiers, key: egui::Key) -> usize {
    ctx.input_mut(|i| {
        let before = i.events.len();
        i.events.retain(|e| {
            !matches!(e, egui::Event::Key { key: k, pressed: true, modifiers: m, .. } if *k == key && *m == modifiers)
        });
        before - i.events.len()
    })
}

pub fn is_top_dialog(ctx: &egui::Context, layer: egui::LayerId) -> bool {
    ctx.memory(|m| {
        if let Some(modal) = m.top_modal_layer() {
            return modal == layer;
        }
        let areas = m.areas();
        let top = m
            .layer_ids()
            .filter(|l| l.order == egui::Order::Middle && areas.is_visible(l) && areas.parent_layer(*l).is_none())
            .last();
        top == Some(layer)
    })
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DialogEscape {
    popup_last: (u64, bool),
    popup_before: bool,
}

impl DialogEscape {
    pub fn begin(&mut self, ctx: &egui::Context) {
        let (pass, open) = self.popup_last;
        let open_last_pass = open && pass.saturating_add(1) == ctx.cumulative_pass_nr();
        self.popup_before = open_last_pass || egui::Popup::is_any_open(ctx);
    }

    pub fn update(&mut self, ctx: &egui::Context, window: Option<&egui::Response>) -> bool {
        let popup = std::mem::take(&mut self.popup_before);
        self.popup_last = (ctx.cumulative_pass_nr(), egui::Popup::is_any_open(ctx));
        let Some(w) = window else { return false };
        !popup && is_top_dialog(ctx, w.layer_id) && take_key(ctx, egui::Modifiers::NONE, egui::Key::Escape) > 0
    }
}

pub const IMPORT_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tga", "bmp"];

pub const SURFACE_PROPS: &[&str] = &[
    "default",
    "default_silent",
    "concrete",
    "concrete_block",
    "brick",
    "rock",
    "boulder",
    "gravel",
    "dirt",
    "mud",
    "sand",
    "grass",
    "snow",
    "ice",
    "wood",
    "wood_crate",
    "wood_plank",
    "wood_panel",
    "wood_solid",
    "wood_furniture",
    "metal",
    "solidmetal",
    "metalpanel",
    "metal_box",
    "metalgrate",
    "metalvent",
    "metalvehicle",
    "chainlink",
    "glass",
    "glassbottle",
    "tile",
    "ceiling_tile",
    "plaster",
    "carpet",
    "cardboard",
    "paper",
    "plastic",
    "plastic_barrel",
    "rubber",
    "rubbertire",
    "porcelain",
    "computer",
    "flesh",
    "water",
    "slime",
    "wade",
    "ladder",
    "foliage",
    "cloth",
    "upholstery",
];

pub fn short_path(path: &Path, max_chars: usize) -> String {
    let full = path.display().to_string();
    if full.chars().count() <= max_chars {
        return full;
    }
    let tail: Vec<char> = full.chars().rev().take(max_chars.saturating_sub(3)).collect();
    let tail: String = tail.into_iter().rev().collect();
    format!("...{tail}")
}

pub fn folder_label(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    fn key(p: &Path) -> String {
        let s = p.to_string_lossy().replace('\\', "/");
        let s = s.trim_end_matches('/').to_owned();
        if cfg!(windows) { s.to_lowercase() } else { s }
    }
    key(a) == key(b)
}

pub fn sorted_paths(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort();
    paths.dedup();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization() {
        assert_eq!(normalize_material("Materials\\Concrete\\Floor01.VMT"), "concrete/floor01");
        assert_eq!(normalize_material("  TOOLS/TOOLSNODRAW "), "tools/toolsnodraw");
        assert_eq!(normalize_material("/dev/x/"), "dev/x");
        assert_eq!(normalize_material("Custom/Textures_Ete/mur brique Été"), "custom/textures_ete/mur brique Été");
        assert_eq!(normalize_material("ÉTÉ/X"), "ÉtÉ/x");
        assert_eq!(leaf_name("a/b/c"), "c");
        assert_eq!(leaf_name("c"), "c");
        assert_eq!(folder_of("a/b/c"), "a/b");
        assert_eq!(folder_of("c"), "");
    }

    #[test]
    fn tools() {
        assert!(is_tool_material("tools/toolsclip", None));
        assert!(!is_tool_material("concrete/x", None));
        let info = hp_app::state::MaterialInfo { tool: Some(hp_vmt::ToolKind::Nodraw), ..Default::default() };
        assert!(is_tool_material("custom/nodraw", Some(&info)));
    }

    #[test]
    fn paths() {
        assert!(same_path(Path::new("C:/a/b/"), Path::new("C:/a/b")));
        assert!(!same_path(Path::new("C:/a/b"), Path::new("C:/a/c")));
        assert_eq!(short_path(Path::new("abc"), 10), "abc");
        let s = short_path(Path::new("C:/very/long/path/to/some/folder"), 12);
        assert!(s.starts_with("...") && s.chars().count() == 12, "{s}");
        assert_eq!(folder_label(Path::new("C:/x/My Folder")), "My Folder");
    }
}
