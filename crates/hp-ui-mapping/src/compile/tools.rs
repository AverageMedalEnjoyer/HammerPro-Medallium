use std::path::Path;

use hp_app::settings::CompileSettings;
use hp_app::{t, tf};
use hp_compile::{Severity as LogSeverity, ToolPaths};

#[derive(Clone, Debug)]
pub(crate) struct Resolved {
    pub tools: ToolPaths,
    pub messages: Vec<(LogSeverity, String)>,
}

/// The programs of the selected game configuration: its `BSP`, `Vis` and `Light` keys (or the
/// tools next to its file), its game executable and the folder the compiled maps go to.
pub(crate) fn tools_of(id: &str) -> Option<ToolPaths> {
    let found = hp_fs::gameconfig::build_tools(id)?;
    let mut tools = ToolPaths::for_game_dir(&found.game_dir);
    tools.vbsp = found.vbsp.unwrap_or_default();
    tools.vvis = found.vvis.unwrap_or_default();
    tools.vrad = found.vrad.unwrap_or_default();
    tools.game_exe = found.game_exe.or_else(|| hp_fs::resolve_game(id).and_then(|p| p.exe));
    if let Some(dir) = found.bsp_dir.filter(|d| !d.as_os_str().is_empty()) {
        tools.maps_dir = dir;
    }
    Some(tools)
}

pub(crate) fn resolve(_root: &Path, s: &CompileSettings) -> Result<Resolved, String> {
    let id = s.target.as_deref().unwrap_or_default();
    let Some(tools) = tools_of(id) else {
        return Err(t!("compile.err_no_game").to_owned());
    };
    if let Some(name) = tools.unset_tools().first() {
        return Err(tf!("compile.err_tool_unset", name));
    }
    Ok(Resolved { tools, messages: Vec::new() })
}

pub(crate) fn tools_text(tools: &ToolPaths) -> String {
    tf!(
        "compile.tools_from_config",
        tools.vbsp.display(),
        tools.vvis.display(),
        tools.vrad.display()
    )
}

pub(crate) fn no_game_text(root: Option<&Path>) -> String {
    match root {
        Some(root) => tf!("compile.err_no_game_dir", root.display()),
        None => t!("compile.err_no_game").to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_configuration_nothing_compiles() {
        crate::register_translations();
        let r = resolve(Path::new("x"), &CompileSettings::default());
        assert!(r.is_err());
    }
}
