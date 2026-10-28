use egui::RichText;
use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::McpSettings;
use hp_app::{Action, AppState, t};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(McpPage)]
}

struct McpPage;

impl SettingsPage for McpPage {
    fn id(&self) -> &'static str {
        "mcp"
    }

    fn label(&self) -> String {
        t!("shell.mcp_page.title").to_owned()
    }

    fn icon(&self) -> &'static str {
        "🔌"
    }

    fn order(&self) -> i32 {
        page_order::KEYBOARD + 50
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        heading(ui, t!("shell.mcp_page.server"));
        ui.label(t!("shell.mcp_page.intro"));
        ui.add_space(4.0);
        let s = &mut state.settings.mcp;
        if ui
            .checkbox(&mut s.start_with_app, t!("shell.mcp_page.start_with_app"))
            .on_hover_text(t!("shell.mcp_page.start_with_app_tip"))
            .changed()
        {
            state.settings_dirty = true;
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let mcp = &state.runtime.mcp;
            let now = if mcp.running { t!("shell.mcp.pill_on") } else { t!("shell.mcp.pill_off") };
            ui.label(RichText::new(now).strong());
            let toggle = if mcp.running { t!("shell.mcp_page.switch_off") } else { t!("shell.mcp_page.switch_on") };
            if ui.button(toggle).clicked() {
                state.request(Action::ToggleMcpServer);
            }
            if ui.button(t!("shell.mcp_page.open_window")).clicked() {
                state.request(Action::McpPanel);
            }
        });
        ui.add_space(2.0);
        ui.label(RichText::new(t!("shell.mcp_page.security")).small().weak());

        heading(ui, t!("shell.mcp_page.tools"));
        if state.runtime.mcp.tools.is_empty() {
            ui.weak(t!("shell.mcp_page.no_tools"));
        } else {
            egui::Grid::new("hp_shell_mcp_tools").num_columns(2).spacing([16.0, 3.0]).striped(true).show(ui, |ui| {
                for (name, title) in &state.runtime.mcp.tools {
                    ui.label(RichText::new(name).monospace());
                    ui.label(title);
                    ui.end_row();
                }
            });
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        state.settings.mcp = McpSettings::default();
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.mcp_page.title", "MCP", "MCP"),
    ("shell.mcp_page.server", "MCP server", "Serveur MCP"),
    (
        "shell.mcp_page.intro",
        "The MCP server lets AI apps (Claude, Codex, Gemini) read and edit the map of a Hammer Pro window. Each window has its own server, off until you switch it on in the MCP menu.",
        "Le serveur MCP permet aux applications d'IA (Claude, Codex, Gemini) de lire et modifier la map d'une fenêtre Hammer Pro. Chaque fenêtre a son propre serveur, inactif tant que vous ne l'activez pas dans le menu MCP.",
    ),
    (
        "shell.mcp_page.start_with_app",
        "Start the MCP server when Hammer Pro starts",
        "Démarrer le serveur MCP au lancement de Hammer Pro",
    ),
    (
        "shell.mcp_page.start_with_app_tip",
        "Off by default: every window starts with its MCP server off",
        "Désactivé par défaut : chaque fenêtre démarre avec son serveur MCP inactif",
    ),
    ("shell.mcp_page.switch_on", "Switch on", "Activer"),
    ("shell.mcp_page.switch_off", "Switch off", "Désactiver"),
    ("shell.mcp_page.open_window", "Install and setup...", "Installation et configuration..."),
    (
        "shell.mcp_page.security",
        "Local only: the server listens on 127.0.0.1 and every connection needs a secret token. Tools edit the map and save or open .vmf files only; they never run programs.",
        "Local uniquement : le serveur écoute sur 127.0.0.1 et chaque connexion exige un jeton secret. Les outils modifient la map et n'enregistrent ou n'ouvrent que des fichiers .vmf ; ils ne lancent jamais de programme.",
    ),
    ("shell.mcp_page.tools", "Tools exposed to AI apps", "Outils proposés aux applications d'IA"),
    ("shell.mcp_page.no_tools", "No tools", "Aucun outil"),
];
