use crate::settings::MapGame;
use crate::{AppState, Severity, tf};

fn remount(state: &mut AppState) {
    state.assets.remount(&state.settings);
    state.assets.reload_fgd(&state.settings);
    let name =
        state.settings.game.active_profile().map_or_else(|| state.settings.game.active_game().to_owned(), |p| p.name);
    state.log(Severity::Info, tf!("app.game_switched", name));
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

pub fn catalogue(ctx: Option<&egui::Context>) -> Option<Vec<hp_fs::GameProfile>> {
    let mut slot = CATALOGUE.lock();
    match &*slot {
        Catalogue::Ready(list) => Some(list.clone()),
        Catalogue::Searching => None,
        Catalogue::Idle => {
            *slot = Catalogue::Searching;
            let ctx = ctx.cloned();
            let spawned = std::thread::Builder::new().name("hp-games".into()).spawn(move || {
                let list = hp_fs::game_catalogue();
                *CATALOGUE.lock() = Catalogue::Ready(list);
                if let Some(c) = ctx {
                    c.request_repaint();
                }
            });
            if spawned.is_err() {
                let list = hp_fs::game_catalogue();
                *slot = Catalogue::Ready(list.clone());
                return Some(list);
            }
            None
        }
    }
}

pub fn refresh_catalogue() {
    hp_fs::refresh_games();
    *CATALOGUE.lock() = Catalogue::Idle;
}

enum Catalogue {
    Idle,
    Searching,
    Ready(Vec<hp_fs::GameProfile>),
}

static CATALOGUE: parking_lot::Mutex<Catalogue> = parking_lot::Mutex::new(Catalogue::Idle);

pub fn game_label(p: &hp_fs::GameProfile) -> String {
    if p.installed { p.name.clone() } else { tf!("app.game_not_installed", p.name) }
}

pub fn game_combo(ui: &mut egui::Ui, id_salt: &str, current: &str, list: &[hp_fs::GameProfile]) -> Option<String> {
    let shown = list.iter().find(|p| p.id.eq_ignore_ascii_case(current)).map_or_else(|| current.to_owned(), game_label);
    let mut picked = None;
    egui::ComboBox::from_id_salt(id_salt).selected_text(shown).width(320.0).show_ui(ui, |ui| {
        let mut was_installed = true;
        for p in list.iter().filter(|p| p.is_supported()) {
            if was_installed && !p.installed {
                ui.separator();
                was_installed = false;
            }
            let selected = p.id.eq_ignore_ascii_case(current);
            let r = ui.add(egui::Button::selectable(selected, game_label(p)));
            let r = if p.installed {
                r.on_hover_text(p.game_dir.display().to_string())
            } else {
                r.on_hover_text(crate::t!("app.game_not_installed_tip"))
            };
            if r.clicked() && !selected {
                picked = Some(p.id.clone());
            }
        }
    });
    picked
}

pub(crate) const TR: crate::i18n::TrTable = &[
    ("app.game_not_installed", "{0} (not installed)", "{0} (non installé)"),
    (
        "app.game_not_installed_tip",
        "Not found on this computer: maps compile in its format and limits with the content mounted now (choose its folder in Settings > Game if it is installed elsewhere).",
        "Introuvable sur cet ordinateur : les maps se compilent dans son format et ses limites avec le contenu monté actuellement (choisissez son dossier dans Paramètres > Jeu s'il est installé ailleurs).",
    ),
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
    fn the_map_and_the_default_game() {
        let mut state = crate::testing::test_state();
        assert!(state.settings.game.is_gmod());
        assert_eq!(state.settings.compile.target, None);
        assert!(choose_game(&mut state, "tf", true));
        assert_eq!(state.settings.game.active_game(), "tf");
        assert_eq!(state.settings.compile.target.as_deref(), Some("tf"));
        assert_eq!(state.sidecar.get::<MapGame>().id, "tf");
        assert_eq!(state.settings.game.game, "gmod", "the default stays");
        state.sidecar = crate::sidecar::Sidecar::in_memory();
        assert!(apply_map_game(&mut state));
        assert!(state.settings.game.is_gmod());
        assert_eq!(state.settings.compile.target, None);
        assert!(choose_game(&mut state, "portal2", false));
        assert_eq!(state.settings.game.game, "portal2");
        assert_eq!(state.settings.compile.target.as_deref(), Some("portal2"));
        assert!(state.settings_dirty);
        assert!(!choose_game(&mut state, "portal2", true));
        assert_eq!(state.sidecar.get::<MapGame>().id, "");
        assert!(!apply_map_game(&mut state));
    }
}
