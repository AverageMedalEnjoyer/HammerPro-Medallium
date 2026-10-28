use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { compile_actions {
    CompileNow => "compile_now", File, FileRun, sc(NONE, Key::F9), Ready;
    CancelCompile => "cancel_compile", File, FileRun, None, Ready;
    BatchCompile => "batch_compile", File, FileRun, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::CancelCompile => {
            let c = &state.runtime.compile;
            Some(c.progress.is_some() || c.background.is_some())
        }
        _ => None,
    }
}

pub(super) fn check(_state: &AppState, _action: Action) -> Option<Check> {
    None
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::CompileNow => Some("⏵"),
        Action::CancelCompile => Some("⏹"),
        Action::BatchCompile => Some("📋"),
        _ => None,
    }
}

pub(super) const TR: TrTable = &[
    ("action.compile_now", "Compile now", "Compiler maintenant"),
    ("action.cancel_compile", "Cancel compile", "Annuler la compilation"),
    ("action.batch_compile", "Batch compile...", "Compilation par lots..."),
];
