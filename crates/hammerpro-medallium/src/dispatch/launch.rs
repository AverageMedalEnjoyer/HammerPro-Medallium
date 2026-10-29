mod bridge;
mod worker;

use std::path::{Path, PathBuf};

use crossbeam_channel::{Receiver, Sender};
use hp_app::i18n::TrTable;
use hp_app::runtime::GameState;
use hp_app::settings::{LaunchTiming, Settings};
use hp_app::{Action, AppState, Severity, t, tf};
use hp_compile::launch::{self as game_launch, NOVID};

use super::Package;
use crate::app::HammerProApp;
use worker::{Cmd, Event, LoadRequest};

pub(crate) struct Launch;

#[derive(Default)]
pub(crate) struct State {
    worker: Option<(Sender<Cmd>, Receiver<Event>)>,
    worker_failed: bool,
    watched: Option<Option<PathBuf>>,
    compiling: bool,
    compile_launch: Option<String>,
    compile_started: Option<std::time::SystemTime>,
}

const HANDLED: &[Action] = &[Action::LaunchGame, Action::ReloadInGame, Action::ImportFromGame];

const ARGS_VERSION: u32 = 1;

impl Package for Launch {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::LaunchGame | Action::ReloadInGame => app.run_map_in_game(ctx),
            Action::ImportFromGame => app.import_from_game(ctx),
            _ => return false,
        }
        true
    }

    fn init(&self, app: &mut HammerProApp) {
        if migrate_args(&mut app.state.settings) {
            app.state.settings_dirty = true;
        }
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        app.launch_tick(ctx);
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

fn migrate_args(settings: &mut Settings) -> bool {
    if settings.launch.args_version >= ARGS_VERSION {
        return false;
    }
    settings.launch.args_version = ARGS_VERSION;
    let args = &mut settings.compile.game_args;
    if game_launch::arg_get(args, NOVID).is_none() {
        *args = game_launch::arg_set(args, NOVID, Some(&[]));
    }
    true
}

fn game_root(state: &AppState) -> Option<&Path> {
    if let Some(root) = state.assets.fs().and_then(hp_fs::GameFs::root) {
        return Some(root);
    }
    let configured = state.settings.game.gmod_root.as_deref().filter(|p| !p.as_os_str().is_empty())?;
    let is_mod_folder = configured.file_name().is_some_and(|n| n == "garrysmod");
    Some(if is_mod_folder { configured.parent().unwrap_or(configured) } else { configured })
}

const GAME_FOLDER: &str = "garrysmod";

fn extra_flags(state: &AppState) -> Vec<&'static str> {
    let mut extra = Vec::new();
    if state.settings.launch.skip_unused_content
        && let Some(used) = hp_app::runtime::content_use(state)
    {
        extra.extend(game_launch::content_skip_args(used.workshop, used.addons));
    }
    extra
}

fn chosen_exe(state: &AppState) -> Option<PathBuf> {
    hp_fs::gameconfig::build_tools(state.settings.game.active_game())
        .and_then(|t| t.game_exe)
        .filter(|p| !p.as_os_str().is_empty())
}

fn placement(state: &AppState) -> Option<([f64; 3], [f64; 3])> {
    if !state.settings.launch.spawn_at_camera || state.runtime.launch.bridge_installed != Some(true) {
        return None;
    }
    let cam = &state.cameras.cam3d;
    let pos = cam.pos.to_array();
    let ang = [-cam.pitch_deg, cam.yaw_deg, 0.0];
    (pos.iter().chain(&ang).all(|v| v.is_finite())).then_some((pos, ang))
}

fn launch_args(state: &AppState, map: Option<&str>) -> Vec<String> {
    let template = game_launch::game_args_with(&state.settings.compile.game_args, GAME_FOLDER, &extra_flags(state));
    match map {
        Some(map) => {
            let mut args = hp_compile::split_args(&template.replace("{map}", map));
            if !template.contains("{map}") {
                args.push("+map".to_owned());
                args.push(map.to_owned());
            }
            args
        }
        None => without_map(hp_compile::split_args(&template)),
    }
}

fn without_map(args: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
        } else if a.eq_ignore_ascii_case("+map") {
            skip = true;
        } else if !a.contains("{map}") {
            out.push(a);
        }
    }
    out
}

impl HammerProApp {
    fn game_worker(&mut self, ctx: &egui::Context) -> Option<&Sender<Cmd>> {
        let pkg = &mut self.pkg.launch;
        if pkg.worker.is_none() && !pkg.worker_failed {
            pkg.worker = worker::spawn(Some(ctx.clone()));
            pkg.worker_failed = pkg.worker.is_none();
        }
        pkg.worker.as_ref().map(|(tx, _)| tx)
    }

    fn run_map_in_game(&mut self, ctx: &egui::Context) {
        let Some(map) = self.state.doc.path().and_then(Path::file_stem).map(|s| s.to_string_lossy().into_owned())
        else {
            self.state.notify(Severity::Info, t!("play.not_saved"));
            return;
        };
        if !self.state.settings.game.is_gmod() {
            self.run_in_other_game(&map);
            return;
        }
        let Some(root) = game_root(&self.state).map(Path::to_path_buf) else {
            self.state.notify(Severity::Warning, t!("play.no_game"));
            return;
        };
        let request = LoadRequest {
            root,
            map: map.clone(),
            args: launch_args(&self.state, Some(&map)),
            reuse: self.state.settings.launch.reuse_running,
            place: placement(&self.state),
            measure: self.state.settings.launch.load_report,
            exe: chosen_exe(&self.state),
        };
        if let Some(tx) = self.game_worker(ctx) {
            let _ = tx.send(Cmd::Load(request));
        }
    }

    fn run_in_other_game(&mut self, map: &str) {
        let Some(p) = self.state.settings.game.active_profile().filter(|p| p.installed) else {
            self.state.notify(Severity::Warning, t!("play.no_other_game"));
            return;
        };
        let Some(exe) = chosen_exe(&self.state).or_else(|| p.exe.clone()) else {
            self.state.notify(Severity::Warning, t!("play.no_exe"));
            return;
        };
        if !p.maps_dir().join(format!("{map}.bsp")).is_file() {
            self.state.notify(Severity::Info, tf!("play.not_built", map, p.maps_dir().display()));
            return;
        }
        let template = game_launch::game_args_with(&self.state.settings.compile.game_args, &p.game_arg(), &[]);
        let args = hp_compile::split_args(&template.replace("{map}", map));
        if hp_compile::sandbox::is_sandboxed() {
            self.state.log(Severity::Info, format!("{} {}", exe.display(), args.join(" ")));
            return;
        }
        match std::process::Command::new(&exe).args(&args).current_dir(&p.root).spawn() {
            Ok(_) => self.state.notify(Severity::Info, tf!("play.starting_other", p.name, map)),
            Err(e) => self.state.notify(Severity::Warning, tf!("play.start_failed", exe.display(), e)),
        }
    }

    fn import_from_game(&mut self, ctx: &egui::Context) {
        let Some(root) = game_root(&self.state).map(Path::to_path_buf) else {
            self.state.notify(Severity::Warning, t!("play.no_game"));
            return;
        };
        if self.state.runtime.launch.bridge_installed != Some(true) {
            self.state.notify(Severity::Info, t!("play.no_bridge"));
            return;
        }
        if let Some(tx) = self.game_worker(ctx) {
            let _ = tx.send(Cmd::DumpProps { root });
        }
        self.state.notify(Severity::Info, t!("play.asking_props"));
    }

    fn import_props(&mut self, props: &[bridge::GameProp]) {
        if props.is_empty() {
            self.state.notify(Severity::Info, t!("play.no_props"));
            return;
        }
        let map = bridge::props_to_map(props);
        match self.state.doc.import_map(map) {
            Ok(objs) => self.state.notify(Severity::Success, tf!("play.props_imported", objs.len())),
            Err(e) => self.state.notify(
                Severity::Warning,
                tf!("app.op_failed", Action::ImportFromGame.label(), super::edit_error_text(&e)),
            ),
        }
    }

    fn launch_tick(&mut self, ctx: &egui::Context) {
        let watched = self.pkg.launch.watched.as_ref().map(Option::as_deref);
        if watched != Some(game_root(&self.state)) {
            let root = game_root(&self.state).map(Path::to_path_buf);
            self.pkg.launch.watched = Some(root.clone());
            if let Some(tx) = self.game_worker(ctx) {
                let _ = tx.send(Cmd::Watch(root));
            }
        }
        let events: Vec<Event> =
            self.pkg.launch.worker.as_ref().map(|(_, rx)| rx.try_iter().collect()).unwrap_or_default();
        for event in events {
            self.on_game_event(event);
        }

        let compiling = self.compile.is_running();
        let started = compiling && !self.pkg.launch.compiling;
        let finished = !compiling && self.pkg.launch.compiling;
        self.pkg.launch.compiling = compiling;
        let bridge_request = self.state.runtime.launch.bridge_request.take();
        if !started && !finished && bridge_request.is_none() {
            return;
        }
        let root = game_root(&self.state).map(Path::to_path_buf);
        if let Some(install) = bridge_request
            && let Some(root) = root.clone()
            && let Some(tx) = self.game_worker(ctx)
        {
            let _ = tx.send(Cmd::Bridge { root, install });
        }
        if started {
            self.pkg.launch.compile_started = Some(std::time::SystemTime::now());
            let launches = self.state.settings.compile.launch_game;
            self.pkg.launch.compile_launch = self.state.runtime.launch.compile_launch_map.clone().or_else(|| {
                self.state
                    .doc
                    .path()
                    .and_then(Path::file_stem)
                    .filter(|_| launches)
                    .map(|s| s.to_string_lossy().into_owned())
            });
        }
        let deferred = finished && std::mem::take(&mut self.state.runtime.launch.compile_launch_deferred);
        if deferred
            && let Some(map) = self.pkg.launch.compile_launch.clone()
            && let Some(root) = root.clone()
        {
            let request = LoadRequest {
                root,
                map: map.clone(),
                args: launch_args(&self.state, Some(&map)),
                reuse: self.state.settings.launch.reuse_running,
                place: None,
                measure: self.state.settings.launch.load_report,
                exe: chosen_exe(&self.state),
            };
            let since = self.pkg.launch.compile_started;
            if let Some(tx) = self.game_worker(ctx) {
                let _ = tx.send(Cmd::LoadBuilt { request, since });
            }
        } else if finished
            && self.state.settings.launch.load_report
            && let Some(map) = self.pkg.launch.compile_launch.clone()
            && let Some(root) = root.clone()
            && let Some(tx) = self.game_worker(ctx)
        {
            let _ = tx.send(Cmd::Measure { root, map });
        }
        if finished {
            self.pkg.launch.compile_launch = None;
            self.pkg.launch.compile_started = None;
            self.state.runtime.launch.compile_launch_map = None;
        }
        if started
            && self.state.settings.compile.launch_game
            && let Some((pos, ang)) = placement(&self.state)
            && let Some(map) = self.pkg.launch.compile_launch.clone()
            && let Some(root) = root.clone()
            && let Some(tx) = self.game_worker(ctx)
        {
            let _ = tx.send(Cmd::Place { root, map, pos, ang });
        }
        let s = &self.state.settings;
        if started
            && s.launch.timing == LaunchTiming::AtStart
            && s.compile.launch_game
            && !s.compile.launch_via_steam
            && !self.state.runtime.launch.game.is_running()
            && let Some(root) = root
        {
            let args = launch_args(&self.state, None);
            let exe = chosen_exe(&self.state);
            if let Some(tx) = self.game_worker(ctx) {
                let _ = tx.send(Cmd::Boot { root, args, exe });
            }
        }
    }

    fn on_game_event(&mut self, event: Event) {
        if let Event::Props(props) = &event {
            self.import_props(props);
            return;
        }
        let state = &mut self.state;
        match event {
            Event::State(game) => {
                state.runtime.launch.game = game;
                if game == GameState::NotRunning {
                    state.runtime.launch.pending_map = None;
                }
            }
            Event::Started { map: Some(map) } => {
                state.runtime.launch.game = GameState::Starting;
                state.notify(Severity::Info, tf!("play.starting", map));
            }
            Event::Started { map: None } => {
                state.runtime.launch.game = GameState::Starting;
                state.notify(Severity::Info, t!("play.booting"));
            }
            Event::Sent { map } => {
                state.runtime.launch.pending_map = None;
                state.notify(Severity::Success, tf!("play.sent", map));
            }
            Event::Waiting { map } => {
                state.notify(Severity::Info, tf!("play.waiting", map));
                state.runtime.launch.pending_map = Some(map);
            }
            Event::NoBsp { map } => state.notify(Severity::Warning, tf!("play.no_bsp", map)),
            Event::NoExe => state.notify(Severity::Error, t!("play.no_exe")),
            Event::WindowTimeout { map } => {
                state.runtime.launch.pending_map = None;
                state.notify(Severity::Warning, tf!("play.window_timeout", map));
            }
            Event::Failed(e) => {
                state.runtime.launch.pending_map = None;
                state.notify(Severity::Error, tf!("play.failed", e));
            }
            Event::Bridge(installed) => state.runtime.launch.bridge_installed = Some(installed),
            Event::BridgeFailed(e) => state.notify(Severity::Error, tf!("play.bridge_failed", e)),
            Event::Props(_) => {}
            Event::PropsTimeout => state.notify(Severity::Warning, t!("play.props_timeout")),
            Event::Loaded { map, total, scripts } => {
                let secs = |d: std::time::Duration| format!("{:.1}", d.as_secs_f64());
                let text = match scripts {
                    Some(s) => tf!("play.loaded_phases", map, secs(total), secs(total.saturating_sub(s)), secs(s)),
                    None => tf!("play.loaded", map, secs(total)),
                };
                state.notify(Severity::Info, text);
                let mut phases = Vec::new();
                if let Some(s) = scripts {
                    phases.push((t!("play.phase_content").to_owned(), total.saturating_sub(s)));
                    phases.push((t!("play.phase_scripts").to_owned(), s));
                    phases.sort_by_key(|(_, d)| std::cmp::Reverse(*d));
                }
                state.runtime.launch.last_load = Some(hp_app::runtime::MapLoadReport { map, total, phases });
            }
        }
    }
}

pub(crate) const TR: TrTable = &[
    (
        "play.not_saved",
        "Save and compile the map first: the compiled map takes its name",
        "Enregistrez et compilez d'abord la map : la map compilée porte son nom",
    ),
    (
        "play.no_game",
        "Garry's Mod was not found: choose its folder in Settings > Game",
        "Garry's Mod est introuvable : choisissez son dossier dans Paramètres > Jeu",
    ),
    ("play.no_exe", "The game executable was not found", "L'exécutable du jeu est introuvable"),
    (
        "play.no_other_game",
        "The game of this map is not installed: choose its folder in Settings > Game",
        "Le jeu de cette map n'est pas installé : choisissez son dossier dans Paramètres > Jeu",
    ),
    (
        "play.not_built",
        "{0}.bsp is not in {1}: compile the map first (with Copy to the game)",
        "{0}.bsp n'est pas dans {1} : compilez d'abord la map (avec Copier dans le jeu)",
    ),
    ("play.starting_other", "Starting {0} on {1}", "Démarrage de {0} sur {1}"),
    ("play.start_failed", "Cannot start {0}: {1}", "Impossible de lancer {0} : {1}"),
    ("play.starting", "Starting Garry's Mod on {0}", "Démarrage de Garry's Mod sur {0}"),
    (
        "play.booting",
        "Starting Garry's Mod while the map compiles",
        "Démarrage de Garry's Mod pendant la compilation de la map",
    ),
    ("play.sent", "{0} is loading in the running game", "{0} se charge dans le jeu lancé"),
    (
        "play.waiting",
        "Garry's Mod is starting: {0} loads as soon as it is ready",
        "Garry's Mod démarre : {0} se chargera dès qu'il sera prêt",
    ),
    (
        "play.no_bsp",
        "{0}.bsp is not in the maps folder of the game: compile the map first",
        "{0}.bsp n'est pas dans le dossier maps du jeu : compilez d'abord la map",
    ),
    (
        "play.window_timeout",
        "The game window did not appear: {0} was not loaded",
        "La fenêtre du jeu n'est pas apparue : {0} n'a pas été chargée",
    ),
    ("play.failed", "Could not reach the game: {0}", "Impossible de joindre le jeu : {0}"),
    (
        "play.no_bridge",
        "Install the Hammer Pro bridge first (Settings > Game), then restart the game",
        "Installez d'abord le pont Hammer Pro (Paramètres > Jeu), puis redémarrez le jeu",
    ),
    ("play.bridge_failed", "Could not change the bridge addon: {0}", "Impossible de modifier l'addon pont : {0}"),
    ("play.asking_props", "Asking the game for its props...", "Demande des props au jeu..."),
    (
        "play.props_timeout",
        "The game did not answer: is a map running with the bridge installed?",
        "Le jeu n'a pas répondu : une map est-elle lancée avec le pont installé ?",
    ),
    ("play.no_props", "No props were spawned in game", "Aucun prop n'a été créé dans le jeu"),
    ("play.props_imported", "{0} props imported from the game", "{0} props importés depuis le jeu"),
    ("play.loaded", "{0} loaded in game in {1} s", "{0} chargée dans le jeu en {1} s"),
    (
        "play.loaded_phases",
        "{0} loaded in game in {1} s (files and content {2} s, entities and scripts {3} s)",
        "{0} chargée dans le jeu en {1} s (fichiers et contenu {2} s, entités et scripts {3} s)",
    ),
    ("play.phase_content", "Files and content", "Fichiers et contenu"),
    ("play.phase_scripts", "Entities and scripts", "Entités et scripts"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fast_start_defaults_are_added_once() {
        let mut s = Settings::default();
        s.compile.game_args = "-console -dev".to_owned();
        assert!(migrate_args(&mut s));
        assert_eq!(s.compile.game_args, "-console -dev -novid");
        s.compile.game_args = "-console".to_owned();
        assert!(!migrate_args(&mut s));
        assert_eq!(s.compile.game_args, "-console");
        let mut s = Settings::default();
        s.compile.game_args = "-NOVID".to_owned();
        assert!(migrate_args(&mut s));
        assert_eq!(s.compile.game_args, "-NOVID");
    }

    #[test]
    fn launch_arguments_name_the_map_or_none() {
        let mut state = hp_app::testing::test_state();
        state.settings.compile.game_args = "-console -novid".to_owned();
        assert_eq!(
            launch_args(&state, Some("gm_test")),
            ["-steam", "-game", "garrysmod", "-console", "-novid", "-hijack", "+map", "gm_test"]
        );
        assert_eq!(launch_args(&state, None), ["-steam", "-game", "garrysmod", "-console", "-novid", "-hijack"]);
        state.settings.compile.game_args = "+map {map} -dev".to_owned();
        assert_eq!(launch_args(&state, Some("x")), ["-steam", "-game", "garrysmod", "+map", "x", "-dev", "-hijack"]);
        assert_eq!(launch_args(&state, None), ["-steam", "-game", "garrysmod", "-dev", "-hijack"]);
        state.settings.launch.skip_unused_content = true;
        assert!(!launch_args(&state, Some("x")).iter().any(|a| a.starts_with("-no")));
    }

    #[test]
    fn the_game_root_accepts_the_mod_folder() {
        let mut state = hp_app::testing::test_state();
        assert_eq!(game_root(&state), None);
        state.settings.game.gmod_root = Some(PathBuf::from("C:/Games/GarrysMod/garrysmod"));
        assert_eq!(game_root(&state), Some(Path::new("C:/Games/GarrysMod")));
        state.settings.game.gmod_root = Some(PathBuf::from("C:/Games/GarrysMod"));
        assert_eq!(game_root(&state), Some(Path::new("C:/Games/GarrysMod")));
    }

    #[test]
    fn texts_are_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty());
        assert!(TR.iter().all(|(k, _, _)| k.starts_with("play.")));
    }
}
