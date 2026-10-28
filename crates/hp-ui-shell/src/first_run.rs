use egui::{Color32, Key, Modifiers, RichText, Ui};
use hp_app::actions::KeymapPreset;
use hp_app::i18n::Lang;
use hp_app::settings::NavScheme;
use hp_app::{AppState, Severity, t, tf};

use crate::common::{EscapeGuard, accent_text, modal_layer};
use crate::game_ui::{GameJobs, custom_dirs_list, default_is_gmod, game_section, gmod_root_section, mount_toggles};

pub const STEPS: usize = 3;

#[derive(Debug, Default)]
pub struct FirstRunWizard {
    pub step: usize,
    was_open: bool,
    escape: EscapeGuard,
}

impl FirstRunWizard {
    pub fn next(&mut self, state: &mut AppState) -> bool {
        if self.step + 1 < STEPS {
            self.step += 1;
            false
        } else {
            finish(state);
            self.step = 0;
            true
        }
    }

    pub fn back(&mut self) {
        self.step = self.step.saturating_sub(1);
    }

    pub fn skip(&mut self, state: &mut AppState) {
        state.settings.language_chosen = true;
        state.settings_dirty = true;
        state.ui.show_first_run = false;
        self.was_open = false;
        self.step = 0;
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState, jobs: &mut GameJobs) {
        let open = state.ui.show_first_run;
        if open && !self.was_open {
            self.step = 0;
            self.escape.reset();
        }
        self.was_open = open;
        if !open {
            return;
        }
        let id = egui::Id::unique("hp_shell_first_run");
        let layer = modal_layer(id);
        if self.escape.should_close(ctx, layer) {
            self.skip(state);
            return;
        }
        if self.step == 1 && state.settings.game.gmod_root.is_none() && !jobs.auto_detect_done {
            jobs.auto_detect_done = true;
            jobs.start_detect(ctx);
        }
        let enter = !ctx.text_edit_focused() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        if enter && self.next(state) {
            state.ui.show_first_run = false;
            self.was_open = false;
            return;
        }

        let mut close = false;
        let mut skip = false;
        egui::Modal::new(id).backdrop_color(Color32::from_black_alpha(140)).show(ctx, |ui| {
            ui.set_width(660.0);
            self.header(ui);
            ui.separator();
            ui.add_space(6.0);
            let page_height = 300.0;
            ui.allocate_ui_with_layout(
                egui::vec2(660.0, page_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_min_size(egui::vec2(660.0, page_height));
                    egui::ScrollArea::vertical().max_height(page_height).auto_shrink([false, false]).show(ui, |ui| {
                        match self.step {
                            0 => language_step(ui, state),
                            1 => game_step(ui, state, jobs),
                            _ => content_step(ui, state, jobs),
                        }
                    });
                },
            );
            ui.add_space(6.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(t!("shell.wizard.skip")).on_hover_text(t!("shell.wizard.skip_tip")).clicked() {
                    skip = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let last = self.step + 1 == STEPS;
                    let label = if last { t!("shell.wizard.finish") } else { t!("shell.wizard.next") };
                    let b = egui::Button::new(RichText::new(label).strong().color(hp_app::theme::on_accent()))
                        .fill(hp_app::theme::accent())
                        .min_size(egui::vec2(96.0, 26.0));
                    if ui.add(b).clicked() && self.next(state) {
                        close = true;
                    }
                    if self.step > 0
                        && ui.add(egui::Button::new(t!("shell.wizard.back")).min_size(egui::vec2(80.0, 26.0))).clicked()
                    {
                        self.back();
                    }
                });
            });
        });
        if skip {
            self.skip(state);
        } else if close {
            state.ui.show_first_run = false;
            self.was_open = false;
        } else {
            self.escape.finish(ctx, layer);
        }
    }

    fn header(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            hp_app::widgets::logo(ui, 28.0);
            ui.label(RichText::new(t!("shell.wizard.title")).heading().strong().color(accent_text(ui.visuals())));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for i in (0..STEPS).rev() {
                    let accent = accent_text(ui.visuals());
                    let color = if i == self.step {
                        accent
                    } else if i < self.step {
                        accent.gamma_multiply(0.5)
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    ui.label(RichText::new("⏺").color(color));
                }
                ui.label(
                    RichText::new(tf!("shell.wizard.step", self.step + 1, STEPS))
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
            });
        });
    }
}

pub fn finish(state: &mut AppState) {
    state.settings.language_chosen = true;
    state.settings_dirty = true;
    state.settings.sync_game();
    state.assets.remount(&state.settings);
    state.assets.reload_fgd(&state.settings);
    state.notify(Severity::Success, t!("shell.wizard.done"));
}

fn language_step(ui: &mut Ui, state: &mut AppState) {
    ui.label(RichText::new(t!("shell.wizard.welcome")).size(16.0).strong());
    ui.add_space(4.0);
    ui.label(t!("shell.wizard.language_intro"));
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let current = hp_app::i18n::lang();
        for lang in Lang::ALL {
            let on = current == lang;
            let text = RichText::new(lang.native_name()).size(18.0);
            let mut b = egui::Button::selectable(on, text).min_size(egui::vec2(170.0, 48.0));
            if on {
                b = b.stroke(egui::Stroke::new(1.5, hp_app::theme::accent()));
            }
            if ui.add(b).clicked() && !on {
                crate::menu::switch_language(state, lang);
            }
        }
    });
    ui.add_space(4.0);
    ui.label(RichText::new(t!("shell.wizard.language_later")).small().color(ui.visuals().weak_text_color()));
    ui.add_space(12.0);
    coming_from(ui, state);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComingFrom {
    HammerPro,
    Hammer,
    HammerPlusPlus,
}

impl ComingFrom {
    const ALL: [ComingFrom; 3] = [ComingFrom::HammerPro, ComingFrom::Hammer, ComingFrom::HammerPlusPlus];

    fn preset(self) -> KeymapPreset {
        match self {
            ComingFrom::HammerPro => KeymapPreset::HammerPro,
            ComingFrom::Hammer => KeymapPreset::Hammer,
            ComingFrom::HammerPlusPlus => KeymapPreset::HammerPlusPlus,
        }
    }

    fn scheme(self) -> NavScheme {
        match self {
            ComingFrom::HammerPro => NavScheme::HammerPro,
            ComingFrom::Hammer => NavScheme::Hammer,
            ComingFrom::HammerPlusPlus => NavScheme::HammerPlusPlus,
        }
    }

    fn label_key(self) -> &'static str {
        match self {
            ComingFrom::HammerPro => "shell.wizard.from_none",
            ComingFrom::Hammer => "shell.wizard.from_hammer",
            ComingFrom::HammerPlusPlus => "shell.wizard.from_hammerpp",
        }
    }

    fn current(state: &AppState) -> Option<ComingFrom> {
        let w = &state.settings.workflow;
        ComingFrom::ALL.into_iter().find(|c| c.preset() == w.keymap_preset && c.scheme() == w.nav_scheme)
    }
}

pub fn apply_coming_from(state: &mut AppState, from: ComingFrom) {
    crate::settings_dialog::apply_keymap_preset(state, from.preset());
    state.settings.workflow.nav_scheme = from.scheme();
    state.settings_dirty = true;
}

fn coming_from(ui: &mut Ui, state: &mut AppState) {
    ui.label(RichText::new(t!("shell.wizard.from_title")).strong());
    ui.label(RichText::new(t!("shell.wizard.from_intro")).small());
    ui.add_space(4.0);
    let current = ComingFrom::current(state);
    ui.horizontal(|ui| {
        for from in ComingFrom::ALL {
            let on = current == Some(from);
            let mut b = egui::Button::selectable(on, t!(from.label_key())).min_size(egui::vec2(170.0, 30.0));
            if on {
                b = b.stroke(egui::Stroke::new(1.5, hp_app::theme::accent()));
            }
            if ui.add(b).clicked() && !on {
                apply_coming_from(state, from);
            }
        }
    });
}

fn game_step(ui: &mut Ui, state: &mut AppState, jobs: &mut GameJobs) {
    ui.label(RichText::new(t!("shell.wizard.game_title")).size(16.0).strong());
    ui.add_space(4.0);
    ui.label(t!("shell.wizard.game_intro"));
    ui.add_space(8.0);
    game_section(ui, state, jobs);
    if !default_is_gmod(state) {
        return;
    }
    ui.add_space(8.0);
    gmod_root_section(ui, state, jobs);
    ui.add_space(8.0);
    if let Some(scan) = &jobs.last_scan
        && scan.is_valid()
        && state.settings.game.gmod_root.is_some()
    {
        let color = crate::common::severity_color(ui.visuals(), Severity::Success);
        ui.label(RichText::new(format!("✔ {}", t!("shell.wizard.game_ok"))).color(color));
    }
}

fn content_step(ui: &mut Ui, state: &mut AppState, jobs: &mut GameJobs) {
    ui.label(RichText::new(t!("shell.wizard.content_title")).size(16.0).strong());
    ui.add_space(4.0);
    ui.label(t!("shell.wizard.content_intro"));
    ui.add_space(8.0);
    if default_is_gmod(state) {
        mount_toggles(ui, state);
        ui.add_space(8.0);
    }
    crate::common::heading(ui, t!("shell.game.custom_dirs"));
    custom_dirs_list(ui, state, jobs);
}

pub(crate) const TR: hp_app::i18n::TrTable = &[
    ("shell.wizard.title", "Hammer Pro setup", "Configuration de Hammer Pro"),
    ("shell.wizard.step", "Step {0} of {1}", "Étape {0} sur {1}"),
    ("shell.wizard.welcome", "Welcome to Hammer Pro!", "Bienvenue dans Hammer Pro !"),
    ("shell.wizard.language_intro", "Choose the language of the interface.", "Choisissez la langue de l'interface."),
    (
        "shell.wizard.language_later",
        "You can change it at any time in View > Language or in the settings.",
        "Vous pouvez la changer à tout moment dans Affichage > Langue ou dans les paramètres.",
    ),
    ("shell.wizard.from_title", "Coming from Hammer or Hammer++?", "Vous venez de Hammer ou de Hammer++ ?"),
    (
        "shell.wizard.from_intro",
        "Hammer Pro can use their shortcuts and camera controls. You can change this later in the settings (Keyboard and Viewport).",
        "Hammer Pro peut reprendre leurs raccourcis et leurs contrôles de caméra. Vous pourrez changer cela plus tard dans les paramètres (Clavier et Vues).",
    ),
    ("shell.wizard.from_none", "No, Hammer Pro defaults", "Non, réglages Hammer Pro"),
    ("shell.wizard.from_hammer", "Hammer", "Hammer"),
    ("shell.wizard.from_hammerpp", "Hammer++", "Hammer++"),
    ("shell.wizard.game_title", "Which game do you map for?", "Pour quel jeu faites-vous des maps ?"),
    (
        "shell.wizard.game_intro",
        "Hammer Pro reads textures, models and entity definitions from the game folder, and compiles maps for that game. Garry's Mod is the default.",
        "Hammer Pro lit les textures, les modèles et les définitions d'entités dans le dossier du jeu, et compile les maps pour ce jeu. Garry's Mod est le jeu par défaut.",
    ),
    ("shell.wizard.game_ok", "Everything needed was found.", "Tout le nécessaire a été trouvé."),
    ("shell.wizard.content_title", "Content", "Contenu"),
    (
        "shell.wizard.content_intro",
        "Choose which content to load. You can add your own texture folders: images are converted for the game when you compile.",
        "Choisissez le contenu à charger. Vous pouvez ajouter vos propres dossiers de textures : les images sont converties pour le jeu à la compilation.",
    ),
    ("shell.wizard.back", "Back", "Retour"),
    ("shell.wizard.next", "Next", "Suivant"),
    ("shell.wizard.finish", "Start mapping", "Commencer"),
    ("shell.wizard.skip", "Skip", "Passer"),
    (
        "shell.wizard.skip_tip",
        "Keep the current settings (you can run this assistant again from the Help menu)",
        "Garder les paramètres actuels (vous pourrez relancer cet assistant depuis le menu Aide)",
    ),
    (
        "shell.wizard.done",
        "Setup complete. Loading game content...",
        "Configuration terminée. Chargement du contenu du jeu...",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coming_from_hammer_sets_keys_and_controls() {
        let mut state = hp_app::testing::test_state();
        assert_eq!(ComingFrom::current(&state), Some(ComingFrom::HammerPro));
        apply_coming_from(&mut state, ComingFrom::HammerPlusPlus);
        assert_eq!(state.settings.workflow.keymap_preset, KeymapPreset::HammerPlusPlus);
        assert_eq!(state.keys.preset(), KeymapPreset::HammerPlusPlus);
        assert_eq!(state.settings.workflow.nav_scheme, NavScheme::HammerPlusPlus);
        assert_eq!(ComingFrom::current(&state), Some(ComingFrom::HammerPlusPlus));
        assert!(state.settings_dirty);
        apply_coming_from(&mut state, ComingFrom::Hammer);
        assert_eq!(ComingFrom::current(&state), Some(ComingFrom::Hammer));
        crate::register_translations();
        hp_app::testing::run_ui(&mut state, language_step);
    }

    #[test]
    fn steps_and_finish() {
        let mut state = hp_app::testing::test_state();
        state.settings.language_chosen = false;
        let mut w = FirstRunWizard::default();
        assert!(!w.next(&mut state));
        assert_eq!(w.step, 1);
        w.back();
        w.back();
        assert_eq!(w.step, 0);
        assert!(!w.next(&mut state));
        assert!(!w.next(&mut state));
        assert_eq!(w.step, 2);
        assert!(w.next(&mut state));
        assert!(state.settings.language_chosen);
        assert!(state.settings_dirty);
        assert!(state.console.lines().any(|l| l.severity == Severity::Success));
    }
}
