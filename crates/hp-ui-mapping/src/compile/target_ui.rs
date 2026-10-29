use egui::{RichText, Ui};
use hp_app::{AppState, t};

pub(super) fn target_game_ui(ui: &mut Ui, state: &mut AppState, running: bool) {
    let ctx = ui.ctx().clone();
    let list = hp_app::games::configs(Some(&ctx));
    let current = state.settings.game.active_game().to_owned();
    ui.horizontal(|ui| {
        ui.label(t!("target.label")).on_hover_text(t!("target.tip"));
        if list.scanning && list.entries.is_empty() {
            ui.spinner();
            ui.label(RichText::new(t!("target.searching")).weak());
            return;
        }
        ui.add_enabled_ui(!running, |ui| {
            if let Some(id) = hp_app::games::config_combo(ui, "hp_compile_target", &current, &list.entries) {
                hp_app::games::choose_game(state, &id, true);
            }
        });
    });
    let Some(e) = list.entries.iter().find(|e| e.key() == current) else { return };
    ui.label(RichText::new(e.dir.display().to_string()).weak().small());
}
