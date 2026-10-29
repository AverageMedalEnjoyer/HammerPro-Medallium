use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use hp_fs::gameconfig::{self, ConfigEntry};
use parking_lot::Mutex;

use crate::settings::{MapGame, Settings};
use crate::{AppState, Severity, tf};

fn remount(state: &mut AppState) {
    hp_fs::gameconfig::invalidate();
    adopt_active(state);
    state.assets.remount(&state.settings);
    state.assets.reload_fgd(&state.settings);
    let name =
        state.settings.game.active_profile().map_or_else(|| state.settings.game.active_game().to_owned(), |p| p.name);
    state.log(Severity::Info, tf!("app.game_switched", name));
}

/// Copies what the active game configuration says into the settings the rest of the editor reads:
/// the game root (Garry's Mod mounting) and the folder maps are saved in.
pub fn adopt_active(state: &mut AppState) {
    let profile = state.settings.game.active_profile();
    let tools = gameconfig::build_tools(state.settings.game.active_game());
    let g = &mut state.settings.game;
    g.gmod_root = profile.as_ref().filter(|p| p.is_gmod() && !p.root.as_os_str().is_empty()).map(|p| p.root.clone());
    if let Some(dir) = tools.and_then(|t| t.map_dir).filter(|d| !d.as_os_str().is_empty()) {
        g.map_dir = Some(dir);
    }
    state.settings.sync_game();
}

pub fn apply_map_game(state: &mut AppState) -> bool {
    let memory: MapGame = state.sidecar.get();
    let id = memory.id.trim().to_owned();
    let changed = state.settings.set_map_game((!id.is_empty()).then_some(id.as_str()));
    if changed {
        remount(state);
    }
    changed
}

pub fn choose_game(state: &mut AppState, id: &str, for_map: bool) -> bool {
    let id = id.trim();
    let before = state.settings.game.active_game().to_owned();
    if for_map {
        let stored = if id.eq_ignore_ascii_case(&state.settings.game.game) { String::new() } else { id.to_owned() };
        state.sidecar.set(&MapGame { id: stored });
        state.settings.set_map_game(Some(id));
    } else {
        state.settings.game.game = id.to_owned();
        state.settings_dirty = true;
        let memory: MapGame = state.sidecar.get();
        let own = memory.id.trim().to_owned();
        state.settings.set_map_game((!own.is_empty()).then_some(own.as_str()));
    }
    let changed = !before.eq_ignore_ascii_case(state.settings.game.active_game());
    if changed {
        remount(state);
    }
    changed
}

/// Re-reads the active configuration after it was edited and saved, and remounts its content.
pub fn reload_active(state: &mut AppState) {
    refresh_entries();
    remount(state);
}

#[derive(Default)]
struct Scan {
    files: Vec<PathBuf>,
    entries: Vec<ConfigEntry>,
    scanning: bool,
    started: bool,
    cancel: Option<Arc<AtomicBool>>,
}

static SCAN: Mutex<Scan> = Mutex::new(Scan { files: Vec::new(), entries: Vec::new(), scanning: false, started: false, cancel: None });

#[derive(Clone, Debug, Default)]
pub struct ConfigList {
    pub entries: Vec<ConfigEntry>,
    pub files: usize,
    pub scanning: bool,
}

fn cache_path() -> PathBuf {
    Settings::default_path().with_file_name("gameconfigs.txt")
}

fn load_cache() -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(cache_path()) else { return Vec::new() };
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(PathBuf::from).filter(|p| p.is_file()).collect()
}

fn save_cache(files: &[PathBuf]) {
    let text: String = files.iter().map(|p| format!("{}\n", p.display())).collect();
    let path = cache_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, text);
}

fn add_file_locked(scan: &mut Scan, file: PathBuf) -> bool {
    let key = file.to_string_lossy().to_ascii_lowercase();
    if scan.files.iter().any(|f| f.to_string_lossy().to_ascii_lowercase() == key) {
        return false;
    }
    scan.entries.extend(gameconfig::entries_of_file(&file));
    scan.files.push(file);
    true
}

fn refresh_entries() {
    let mut scan = SCAN.lock();
    let files = scan.files.clone();
    scan.entries = gameconfig::entries_of_files(&files);
}

fn start_scan(ctx: Option<egui::Context>) {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut scan = SCAN.lock();
        if scan.scanning {
            return;
        }
        scan.scanning = true;
        scan.started = true;
        scan.cancel = Some(cancel.clone());
    }
    let worker = std::thread::Builder::new().name("hp-gameconfigs".into()).spawn({
        let cancel = cancel.clone();
        let ctx = ctx.clone();
        move || {
            gameconfig::scan_computer(&cancel, &mut |file| {
                add_file_locked(&mut SCAN.lock(), file);
                if let Some(c) = &ctx {
                    c.request_repaint();
                }
            });
            let files = {
                let mut scan = SCAN.lock();
                scan.scanning = false;
                scan.files.clone()
            };
            if !cancel.load(Ordering::Relaxed) {
                save_cache(&files);
            }
            if let Some(c) = &ctx {
                c.request_repaint();
            }
        }
    });
    if worker.is_err() {
        SCAN.lock().scanning = false;
    }
}

/// The game configurations found on this computer. The first call loads the list of the previous
/// run (or starts the search when there is none); `rescan` searches again.
pub fn configs(ctx: Option<&egui::Context>) -> ConfigList {
    let first = {
        let mut scan = SCAN.lock();
        let first = !scan.started && !scan.scanning;
        if first {
            scan.started = true;
            for f in load_cache() {
                add_file_locked(&mut scan, f);
            }
        }
        first
    };
    if first && SCAN.lock().files.is_empty() {
        start_scan(ctx.cloned());
    }
    let scan = SCAN.lock();
    ConfigList { entries: scan.entries.clone(), files: scan.files.len(), scanning: scan.scanning }
}

pub fn rescan(ctx: Option<&egui::Context>) {
    {
        let mut scan = SCAN.lock();
        if scan.scanning {
            return;
        }
        scan.files.clear();
        scan.entries.clear();
    }
    start_scan(ctx.cloned());
}

pub fn add_config_file(path: &Path) -> bool {
    let added = add_file_locked(&mut SCAN.lock(), path.to_path_buf());
    if added {
        let files = SCAN.lock().files.clone();
        save_cache(&files);
    }
    added
}

pub fn entry_label(e: &ConfigEntry) -> String {
    format!("{} - {}", e.id.name, e.dir.display())
}

pub fn config_combo(ui: &mut egui::Ui, id_salt: &str, current: &str, list: &[ConfigEntry]) -> Option<String> {
    let shown = list.iter().find(|e| e.key() == current).map_or_else(
        || gameconfig::ConfigId::parse(current).map_or_else(|| current.to_owned(), |c| c.name),
        |e| e.id.name.clone(),
    );
    let mut picked = None;
    egui::ComboBox::from_id_salt(id_salt).selected_text(shown).width(360.0).show_ui(ui, |ui| {
        for e in list {
            let selected = e.key() == current;
            let r = ui.add(egui::Button::selectable(selected, e.label())).on_hover_text(e.dir.display().to_string());
            if r.clicked() && !selected {
                picked = Some(e.key());
            }
        }
    });
    picked
}

pub(crate) const TR: crate::i18n::TrTable = &[
    (
        "app.game_switched",
        "Game: {0}. Its content and entity definitions are loading.",
        "Jeu : {0}. Son contenu et ses définitions d'entités se chargent.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choosing_a_game_for_the_map_does_not_change_the_default() {
        let mut state = crate::testing::test_state();
        assert!(state.settings.game.active_game().is_empty());
        assert!(choose_game(&mut state, "cfg:/x/GameConfig.txt|One", false));
        assert_eq!(state.settings.game.game, "cfg:/x/GameConfig.txt|One");
        assert!(choose_game(&mut state, "cfg:/x/GameConfig.txt|Two", true));
        assert_eq!(state.settings.game.active_game(), "cfg:/x/GameConfig.txt|Two");
        assert_eq!(state.settings.game.game, "cfg:/x/GameConfig.txt|One", "the default stays");
        assert_eq!(state.sidecar.get::<MapGame>().id, "cfg:/x/GameConfig.txt|Two");
        state.sidecar = crate::sidecar::Sidecar::in_memory();
        assert!(apply_map_game(&mut state));
        assert_eq!(state.settings.game.active_game(), "cfg:/x/GameConfig.txt|One");
    }
}
