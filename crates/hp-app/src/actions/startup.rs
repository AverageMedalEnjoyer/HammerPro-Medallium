use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { startup_actions {
    ExportCopy => "export_copy", File, FileImport, None, Ready;
    ExportAgain => "export_again", File, FileImport, sc(ALT, Key::B), Ready;
    ExportObj => "export_obj", File, FileImport, None, Ready;
    ReloadContent => "reload_content", Tools, ToolsContent, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(_state: &AppState, _action: Action) -> Option<bool> {
    None
}

pub(super) fn check(_state: &AppState, _action: Action) -> Option<Check> {
    None
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::ReloadContent => Some("⟳"),
        _ => None,
    }
}

pub(super) const TR: TrTable = &[
    ("action.export_copy", "Export a copy...", "Exporter une copie..."),
    ("action.export_again", "Export again", "Exporter à nouveau"),
    ("action.export_obj", "Export OBJ...", "Exporter en OBJ..."),
    ("action.reload_content", "Reload content from disk", "Recharger le contenu depuis le disque"),
];
