pub mod process;

use std::fmt::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::plan::{PlanOptions, Step, StepKind, quote_arg, split_args};
use crate::tools::ToolPaths;

pub const DEFAULT_GAME_ARGS: &str = "-steam -game garrysmod -console -dev -hijack +map {map}";

pub const DEFAULT_STEAM_URL: &str = "steam://rungameid/4000//+map%20{map}";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum LaunchMethod {
    #[default]
    Exe,
    SteamUrl,
}

pub fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a.eq_ignore_ascii_case(flag))
}

pub fn game_args_for(user: &str, game_folder: &str) -> String {
    game_args_with(user, game_folder, &[])
}

pub fn game_args_with(user: &str, game_folder: &str, extra: &[&str]) -> String {
    let user_args = split_args(user);
    let mut args: Vec<String> = Vec::new();
    if !has_flag(&user_args, "-game") {
        if !has_flag(&user_args, "-steam") {
            args.push("-steam".to_owned());
        }
        args.push("-game".to_owned());
        args.push(if game_folder.is_empty() { "garrysmod".to_owned() } else { game_folder.to_owned() });
    }
    let mut out: Vec<String> = args.iter().map(|a| quote_arg(a)).collect();
    let user = user.trim();
    if !user.is_empty() {
        out.push(user.to_owned());
    }
    for flag in extra {
        if !flag.is_empty() && !has_flag(&user_args, flag) && !out.iter().any(|a| a.eq_ignore_ascii_case(flag)) {
            out.push(quote_arg(flag));
        }
    }
    if !has_flag(&user_args, "-hijack") && !has_flag(&user_args, "-nohijack") {
        out.push("-hijack".to_owned());
    }
    if !user.contains("{map}") && !has_flag(&user_args, "+map") {
        out.push("+map {map}".to_owned());
    }
    out.join(" ")
}

pub const STALE_ZIP_FLAG: &str = "-allowstalezip";

pub fn content_skip_args(uses_workshop: bool, uses_addons: bool) -> Vec<&'static str> {
    let mut out = Vec::new();
    if !uses_workshop {
        out.push("-noworkshop");
    }
    if !uses_addons {
        out.push("-noaddons");
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArgSpec {
    pub name: &'static str,
    pub values: usize,
}

impl ArgSpec {
    pub const fn flag(name: &'static str) -> Self {
        Self { name, values: 0 }
    }

    pub const fn with_value(name: &'static str) -> Self {
        Self { name, values: 1 }
    }
}

pub const NOVID: ArgSpec = ArgSpec::flag("-novid");
pub const WINDOWED: ArgSpec = ArgSpec::flag("-windowed");
pub const NOBORDER: ArgSpec = ArgSpec::flag("-noborder");
pub const WIDTH: ArgSpec = ArgSpec::with_value("-w");
pub const HEIGHT: ArgSpec = ArgSpec::with_value("-h");
pub const CONSOLE: ArgSpec = ArgSpec::flag("-console");
pub const DEV: ArgSpec = ArgSpec::flag("-dev");
pub const SV_LAN: ArgSpec = ArgSpec::with_value("+sv_lan");
pub const MAXPLAYERS: ArgSpec = ArgSpec::with_value("+maxplayers");
pub const CONDEBUG: ArgSpec = ArgSpec::flag("-condebug");

pub const FAST_START: &[ArgSpec] =
    &[NOVID, WINDOWED, NOBORDER, WIDTH, HEIGHT, CONSOLE, DEV, SV_LAN, MAXPLAYERS, CONDEBUG];

fn is_value(token: &str) -> bool {
    !token.starts_with('-') && !token.starts_with('+')
}

fn find_arg(parts: &[String], spec: ArgSpec) -> Option<std::ops::Range<usize>> {
    let start = parts.iter().position(|p| p.eq_ignore_ascii_case(spec.name))?;
    let mut end = start + 1;
    while end < parts.len() && end - start <= spec.values && is_value(&parts[end]) {
        end += 1;
    }
    Some(start..end)
}

pub fn arg_get(args: &str, spec: ArgSpec) -> Option<Vec<String>> {
    let parts = split_args(args);
    let range = find_arg(&parts, spec)?;
    Some(parts[range.start + 1..range.end].to_vec())
}

pub fn arg_set(args: &str, spec: ArgSpec, values: Option<&[&str]>) -> String {
    let mut parts = split_args(args);
    let mut at = None;
    while let Some(range) = find_arg(&parts, spec) {
        at.get_or_insert(range.start);
        parts.drain(range);
    }
    if let Some(values) = values {
        let mut new = vec![spec.name.to_owned()];
        new.extend(values.iter().take(spec.values).map(|v| (*v).to_owned()));
        let at = at.unwrap_or(parts.len()).min(parts.len());
        parts.splice(at..at, new);
    }
    parts.iter().map(|p| quote_arg(p)).collect::<Vec<_>>().join(" ")
}

pub fn spawn_game(exe: &Path, args: &[String]) -> std::io::Result<u32> {
    if crate::sandbox::is_sandboxed() {
        return Err(std::io::Error::other(crate::sandbox::REFUSED));
    }
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Some(dir) = exe.parent().filter(|d| d.is_dir()) {
        cmd.current_dir(dir);
    }
    let mut child = cmd.spawn()?;
    let pid = child.id();
    let _ = std::thread::Builder::new().name("hp-game-reaper".to_owned()).spawn(move || {
        let _ = child.wait();
    });
    Ok(pid)
}

pub fn launch_step(tools: &ToolPaths, map_name: &str, opts: &PlanOptions) -> Step {
    if opts.launch_method == LaunchMethod::Exe
        && let Some(exe) = &tools.game_exe
    {
        let mut args = split_args(&opts.game_args.replace("{map}", map_name));
        if !opts.game_args.contains("{map}") {
            args.push("+map".to_owned());
            args.push(map_name.to_owned());
        }
        return Step::new(StepKind::LaunchGame, exe, args);
    }
    let template = if opts.steam_url.trim().is_empty() { DEFAULT_STEAM_URL } else { opts.steam_url.as_str() };
    let url = template.replace("{map}", &url_encode(map_name));
    let (exe, mut args) = url_opener();
    args.push(url);
    Step::new(StepKind::LaunchGame, exe, args)
}

pub fn url_opener() -> (&'static str, Vec<String>) {
    if cfg!(windows) {
        ("rundll32.exe", vec!["url.dll,FileProtocolHandler".to_owned()])
    } else if cfg!(target_os = "macos") {
        ("open", Vec::new())
    } else {
        ("xdg-open", Vec::new())
    }
}

pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_args() {
        assert_eq!(
            game_args_for("-console -dev", "garrysmod"),
            "-steam -game garrysmod -console -dev -hijack +map {map}"
        );
        assert_eq!(game_args_for("", ""), "-steam -game garrysmod -hijack +map {map}");
        assert_eq!(
            game_args_for("-game garrysmod -windowed +map {map}", "x"),
            "-game garrysmod -windowed +map {map} -hijack"
        );
        assert_eq!(game_args_for("-nohijack -steam", "garrysmod"), "-game garrysmod -nohijack -steam +map {map}");
        assert_eq!(game_args_for("", "my mod"), "-steam -game \"my mod\" -hijack +map {map}");
    }

    #[test]
    fn extra_flags_go_after_the_user_arguments_once() {
        assert_eq!(
            game_args_with("-novid", "garrysmod", &["-noworkshop", "-novid", ""]),
            "-steam -game garrysmod -novid -noworkshop -hijack +map {map}"
        );
        assert_eq!(content_skip_args(false, true), vec!["-noworkshop"]);
        assert_eq!(content_skip_args(true, false), vec!["-noaddons"]);
        assert!(content_skip_args(true, true).is_empty());
        assert_eq!(
            game_args_with("", "garrysmod", &[STALE_ZIP_FLAG]),
            "-steam -game garrysmod -allowstalezip -hijack +map {map}"
        );
    }

    #[test]
    fn fast_start_options_live_in_the_argument_text() {
        let args = "-console -dev";
        assert_eq!(arg_get(args, CONSOLE), Some(vec![]));
        assert_eq!(arg_get(args, NOVID), None);
        let args = arg_set(args, NOVID, Some(&[]));
        assert_eq!(args, "-console -dev -novid");
        let args = arg_set(&args, WIDTH, Some(&["1280"]));
        let args = arg_set(&args, HEIGHT, Some(&["720"]));
        assert_eq!(args, "-console -dev -novid -w 1280 -h 720");
        assert_eq!(arg_get(&args, WIDTH), Some(vec!["1280".to_owned()]));
        let args = arg_set(&args, WIDTH, Some(&["1920"]));
        assert_eq!(args, "-console -dev -novid -w 1920 -h 720");
        assert_eq!(arg_set("-W 800 -console -w 640", WIDTH, None), "-console");
        assert_eq!(arg_set("-dev", DEV, None), "");
        let args = arg_set("-console", SV_LAN, Some(&["1"]));
        let args = arg_set(&args, MAXPLAYERS, Some(&["1"]));
        assert_eq!(args, "-console +sv_lan 1 +maxplayers 1");
        assert_eq!(arg_get("-w -console", WIDTH), Some(vec![]));
        let args = arg_set(r#"+exec "my cfg" -dev"#, NOVID, Some(&[]));
        assert_eq!(split_args(&args), vec!["+exec", "my cfg", "-dev", "-novid"]);
        for spec in FAST_START {
            let values: Vec<&str> = std::iter::repeat_n("1", spec.values).collect();
            let set = arg_set("-x", *spec, Some(&values));
            assert_eq!(arg_get(&set, *spec), Some(values.iter().map(|v| (*v).to_owned()).collect()));
            assert_eq!(arg_set(&set, *spec, None), "-x");
        }
    }

    #[test]
    fn flags_and_urls() {
        let args = split_args("-Console +MAP x");
        assert!(has_flag(&args, "-console") && has_flag(&args, "+map"));
        assert!(!has_flag(&args, "-dev"));
        assert_eq!(url_encode("gm_a b+c"), "gm_a%20b%2Bc");
        assert!(!url_opener().0.is_empty());
    }

    #[test]
    fn launch_step_uses_the_exe_or_the_url() {
        let mut tools = ToolPaths {
            vbsp: "vbsp.exe".into(),
            vvis: "vvis.exe".into(),
            vrad: "vrad.exe".into(),
            game_exe: Some("gmod.exe".into()),
            game_dir: "garrysmod".into(),
            maps_dir: "garrysmod/maps".into(),
        };
        let opts = PlanOptions { launch_game: true, game_args: "-dev".to_owned(), ..PlanOptions::default() };
        let step = launch_step(&tools, "gm_x", &opts);
        assert_eq!(step.kind, StepKind::LaunchGame);
        assert_eq!(step.args, vec!["-dev", "+map", "gm_x"]);
        tools.game_exe = None;
        let step = launch_step(&tools, "gm x", &opts);
        assert_eq!(step.args.last().map(String::as_str), Some("steam://rungameid/4000//+map%20gm%20x"));
    }
}
