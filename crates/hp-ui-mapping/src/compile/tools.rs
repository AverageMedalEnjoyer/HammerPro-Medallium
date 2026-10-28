use std::path::Path;

use hp_app::settings::CompileSettings;
use hp_app::{t, tf};
use hp_compile::{Severity as LogSeverity, ToolPaths};

#[derive(Clone, Debug)]
pub(crate) struct Resolved {
    pub tools: ToolPaths,
    pub messages: Vec<(LogSeverity, String)>,
}

pub(crate) fn resolve(root: &Path, s: &CompileSettings) -> Result<Resolved, String> {
    let Some(mut tools) = ToolPaths::detect(root) else {
        return Err(tf!("compile.err_no_game_dir", root.display()));
    };
    if let Some(p) = s
        .target
        .as_deref()
        .and_then(|t| hp_fs::resolve_game(t, s.target_dir.as_deref()))
        .filter(|p| p.installed && !p.is_gmod())
        && p.exe.is_some()
    {
        tools.game_exe = p.exe;
    }
    let messages =
        apply_overrides(&mut tools, s, Path::is_file).into_iter().map(|w| (LogSeverity::Warning, w)).collect();
    Ok(Resolved { tools, messages })
}

pub(crate) fn apply_overrides(
    tools: &mut ToolPaths,
    s: &CompileSettings,
    exists: impl Fn(&Path) -> bool,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Some(p) = s.game_exe_path.as_ref().filter(|p| !p.as_os_str().is_empty()) {
        if exists(p) {
            tools.game_exe = Some(p.clone());
        } else {
            warnings.push(tf!("compile.game_exe_missing", p.display()));
        }
    }
    if let Some(dir) = s.maps_dir_override.as_ref().filter(|p| !p.as_os_str().is_empty()) {
        if dir.is_absolute() {
            tools.maps_dir = dir.clone();
        } else {
            warnings.push(tf!("compile.maps_dir_relative", dir.display()));
        }
    }
    warnings
}

pub(crate) fn tools_text(tools: &ToolPaths) -> String {
    tf!("compile.compiler_builtin", env!("CARGO_PKG_VERSION"), tools.game_dir.display())
}

pub(crate) fn no_game_text(root: Option<&Path>) -> String {
    match root {
        Some(root) => tf!("compile.err_no_game_dir", root.display()),
        None => t!("compile.err_no_game").to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn the_running_executable_compiles() {
        crate::register_translations();
        let dir = crate::test_scratch("compile_tools_resolve");
        let root = dir.join("GarrysMod");
        let game = root.join("garrysmod");
        std::fs::create_dir_all(&game).expect("game");
        std::fs::write(game.join("gameinfo.txt"), "\"GameInfo\" {}").expect("gameinfo");
        let r = resolve(&root, &CompileSettings::default()).expect("resolve");
        assert_eq!(r.tools.compiler, std::env::current_exe().expect("exe"));
        assert_eq!(r.tools.game_dir, game);
        assert!(r.messages.is_empty());
        assert!(tools_text(&r.tools).contains(&game.display().to_string()));
        assert!(resolve(&dir.join("nothing"), &CompileSettings::default()).is_err());
        let s = CompileSettings { game_exe_path: Some(dir.join("missing.exe")), ..CompileSettings::default() };
        assert_eq!(resolve(&root, &s).expect("resolve").messages.len(), 1);
    }

    #[test]
    fn game_exe_and_maps_folder_overrides() {
        crate::register_translations();
        let mut t = ToolPaths {
            compiler: PathBuf::from("C:/HammerPro/hammerpro.exe"),
            game_exe: None,
            game_dir: PathBuf::from("C:/Games/GarrysMod/garrysmod"),
            maps_dir: PathBuf::from("C:/Games/GarrysMod/garrysmod/maps"),
        };
        let maps = std::path::absolute("C:/Other/maps").expect("abs");
        let s = CompileSettings {
            game_exe_path: Some(PathBuf::from("D:/game/hl2.exe")),
            maps_dir_override: Some(maps.clone()),
            ..CompileSettings::default()
        };
        let warnings = apply_overrides(&mut t, &s, |_| true);
        assert!(warnings.is_empty());
        assert_eq!(t.game_exe, Some(PathBuf::from("D:/game/hl2.exe")));
        assert_eq!(t.maps_dir, maps);
        let bad = CompileSettings { maps_dir_override: Some(PathBuf::from("maps")), ..CompileSettings::default() };
        assert_eq!(apply_overrides(&mut t, &bad, |_| true).len(), 1);
    }
}
