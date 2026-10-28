use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::LabSettings;
use hp_app::{AppState, t};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(LabPage)]
}

struct LabPage;

impl SettingsPage for LabPage {
    fn id(&self) -> &'static str {
        "lab"
    }

    fn label(&self) -> String {
        t!("shell.lab.page").to_owned()
    }

    fn icon(&self) -> &'static str {
        "🗺"
    }

    fn order(&self) -> i32 {
        page_order::VIEWPORT + 30
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let s = &mut state.settings.lab;
        let mut changed = false;
        heading(ui, t!("shell.lab.nav"));
        changed |= ui.checkbox(&mut s.show_nav_mesh, t!("action.toggle_nav_mesh")).changed();
        ui.add_enabled_ui(s.show_nav_mesh, |ui| {
            changed |= ui
                .checkbox(&mut s.nav_fill, t!("shell.lab.nav_fill"))
                .on_hover_text(t!("shell.lab.nav_fill_tip"))
                .changed();
            changed |= ui
                .checkbox(&mut s.nav_connections, t!("shell.lab.nav_connections"))
                .on_hover_text(t!("shell.lab.nav_connections_tip"))
                .changed();
        });
        ui.label(egui::RichText::new(t!("shell.lab.nav_colors")).small().weak());
        heading(ui, t!("shell.lab.particles"));
        changed |= ui
            .checkbox(&mut s.show_particles, t!("action.toggle_particles"))
            .on_hover_text(t!("shell.lab.particles_tip"))
            .changed();
        ui.add_enabled_ui(s.show_particles, |ui| {
            changed |= ui.checkbox(&mut s.particles_selected_only, t!("action.toggle_particles_selected")).changed();
        });
        if changed {
            state.settings_dirty = true;
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        state.settings.lab = LabSettings::default();
        state.settings_dirty = true;
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.lab.page", "Nav mesh and particles", "Nav mesh et particules"),
    ("shell.lab.nav", "Navigation mesh", "Maillage de navigation"),
    ("shell.lab.nav_fill", "Fill the areas in the 3D view", "Remplir les zones dans la vue 3D"),
    (
        "shell.lab.nav_fill_tip",
        "Without the fill, only the outlines of the areas are drawn",
        "Sans remplissage, seuls les contours des zones sont dessinés",
    ),
    ("shell.lab.nav_connections", "Connections between areas", "Connexions entre les zones"),
    (
        "shell.lab.nav_connections_tip",
        "White: both ways. Orange with an arrow: one way.",
        "Blanc : dans les deux sens. Orange avec une flèche : à sens unique.",
    ),
    (
        "shell.lab.nav_colors",
        "Area outlines: green walkable, blue crouch, yellow jump, red avoid, cyan stairs, magenta cut off from the part where players spawn.",
        "Contours des zones : vert praticable, bleu accroupi, jaune saut, rouge à éviter, cyan escalier, magenta coupée de la partie où les joueurs apparaissent.",
    ),
    ("shell.lab.particles", "Particles", "Particules"),
    (
        "shell.lab.particles_tip",
        "Plays the effects of info_particle_system near the camera in the 3D view (approximate colors, no textures)",
        "Joue les effets des info_particle_system proches de la caméra dans la vue 3D (couleurs approchées, sans textures)",
    ),
];
