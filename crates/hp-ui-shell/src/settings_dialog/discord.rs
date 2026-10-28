use egui::RichText;
use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::DiscordSettings;
use hp_app::{AppState, t};

use crate::common::heading;

const PORTAL_URL: &str = "https://discord.com/developers/applications";

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(DiscordPage)]
}

struct DiscordPage;

impl SettingsPage for DiscordPage {
    fn id(&self) -> &'static str {
        "discord"
    }

    fn label(&self) -> String {
        t!("shell.discord.title").to_owned()
    }

    fn icon(&self) -> &'static str {
        "💬"
    }

    fn order(&self) -> i32 {
        page_order::GENERAL + 50
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        heading(ui, t!("shell.discord.heading"));
        ui.label(t!("shell.discord.intro"));
        ui.add_space(4.0);
        let s = &mut state.settings.discord;
        let mut changed = ui.checkbox(&mut s.enabled, t!("shell.discord.enabled")).changed();
        ui.add_enabled_ui(s.enabled, |ui| {
            ui.indent("hp_shell_discord_privacy", |ui| {
                changed |= ui.checkbox(&mut s.show_map, t!("shell.discord.show_map")).changed();
                changed |= ui.checkbox(&mut s.show_elapsed, t!("shell.discord.show_elapsed")).changed();
                changed |= ui
                    .checkbox(&mut s.show_tool, t!("shell.discord.show_tool"))
                    .on_hover_text(t!("shell.discord.show_tool_tip"))
                    .changed();
            });
        });

        heading(ui, t!("shell.discord.application"));
        egui::Grid::new("hp_shell_discord").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t!("shell.discord.app_id"));
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut s.application_id)
                        .desired_width(220.0)
                        .hint_text("1234567890123456789"),
                )
                .changed();
            ui.end_row();
            ui.label(t!("shell.discord.project_url"));
            changed |= ui
                .add(egui::TextEdit::singleline(&mut s.project_url).desired_width(320.0).hint_text("https://..."))
                .on_hover_text(t!("shell.discord.project_url_tip"))
                .changed();
            ui.end_row();
        });
        if hp_discord::application_id(&s.application_id).is_none() {
            ui.add_space(4.0);
            let key = if s.application_id.trim().is_empty() { "shell.discord.no_id" } else { "shell.discord.bad_id" };
            ui.label(RichText::new(t!(key)).color(ui.visuals().warn_fg_color));
            ui.hyperlink_to(PORTAL_URL, PORTAL_URL);
        }
        ui.add_space(4.0);
        ui.label(RichText::new(t!("shell.discord.note")).small().weak());
        if changed {
            state.settings_dirty = true;
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        let s = &state.settings.discord;
        state.settings.discord = DiscordSettings {
            application_id: s.application_id.clone(),
            project_url: s.project_url.clone(),
            ..DiscordSettings::default()
        };
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.discord.title", "Discord", "Discord"),
    ("shell.discord.heading", "Discord Rich Presence", "Statut Discord (Rich Presence)"),
    (
        "shell.discord.intro",
        "Shows what you are mapping in your Discord profile, like Hammer++: the map, the active tool, the game and the time spent on the map.",
        "Affiche ce que vous mappez dans votre profil Discord, comme Hammer++ : la map, l'outil actif, le jeu et le temps passé sur la map.",
    ),
    ("shell.discord.enabled", "Show activity on Discord", "Afficher l'activité sur Discord"),
    ("shell.discord.show_map", "Show the map name", "Afficher le nom de la map"),
    ("shell.discord.show_elapsed", "Show the elapsed time", "Afficher le temps écoulé"),
    ("shell.discord.show_tool", "Show the active tool", "Afficher l'outil actif"),
    (
        "shell.discord.show_tool_tip",
        "Off: the target game is shown instead (and \"Compiling...\" during a compile)",
        "Désactivé : le jeu cible est affiché à la place (et « Compilation... » pendant une compilation)",
    ),
    ("shell.discord.application", "Discord application", "Application Discord"),
    ("shell.discord.app_id", "Discord application ID", "ID de l'application Discord"),
    ("shell.discord.project_url", "\"Get Hammer Pro\" button link", "Lien du bouton « Obtenir Hammer Pro »"),
    (
        "shell.discord.project_url_tip",
        "Optional web page (https://...) shown as a button under the activity; empty: no button",
        "Page web facultative (https://...) affichée en bouton sous l'activité ; vide : pas de bouton",
    ),
    (
        "shell.discord.no_id",
        "No application ID: nothing is shown on Discord yet. On the Discord Developer Portal, create an application named \"Hammer Pro\", upload the logo under Rich Presence > Art Assets with the key \"hammerpro\", then copy its Application ID (General Information page) into the field above.",
        "Aucun ID d'application : rien n'est encore affiché sur Discord. Sur le Discord Developer Portal, créez une application nommée « Hammer Pro », envoyez le logo dans Rich Presence > Art Assets avec la clé « hammerpro », puis copiez son Application ID (page General Information) dans le champ ci-dessus.",
    ),
    (
        "shell.discord.bad_id",
        "This is not a Discord application ID (a number of 17 to 20 digits, from the General Information page of the application).",
        "Ce n'est pas un ID d'application Discord (un nombre de 17 à 20 chiffres, sur la page General Information de l'application).",
    ),
    (
        "shell.discord.note",
        "Needs the Discord desktop app running on this computer, with \"Share your detected activities\" on in Discord's Activity Privacy settings. Turning the option off clears the activity at once.",
        "Nécessite l'application Discord ouverte sur cet ordinateur, avec « Partager vos activités détectées » activé dans les paramètres Confidentialité des activités de Discord. Désactiver l'option efface l'activité aussitôt.",
    ),
    ("shell.discord.activity_editing", "Editing {0}", "Édite {0}"),
    ("shell.discord.activity_editing_hidden", "Editing a map", "Édite une map"),
    ("shell.discord.activity_no_map", "No map open", "Aucune map ouverte"),
    ("shell.discord.activity_compiling", "Compiling...", "Compilation..."),
    ("shell.discord.activity_button", "Get Hammer Pro", "Obtenir Hammer Pro"),
];
