use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::value::RawValue;
use serde_json::{Map, Value, json};

use crate::SERVER_NAME;
use crate::process;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Client {
    ClaudeCode,
    ClaudeDesktop,
    Codex,
    Gemini,
}

impl Client {
    pub const ALL: [Client; 4] = [Client::ClaudeCode, Client::ClaudeDesktop, Client::Codex, Client::Gemini];

    pub fn id(self) -> &'static str {
        match self {
            Client::ClaudeCode => "claude_code",
            Client::ClaudeDesktop => "claude_desktop",
            Client::Codex => "codex",
            Client::Gemini => "gemini",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Client::ClaudeCode => "Claude Code",
            Client::ClaudeDesktop => "Claude Desktop",
            Client::Codex => "Codex",
            Client::Gemini => "Gemini CLI",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    NotInstalled,
    NotConfigured,
    Configured,
    OtherExe(String),
    Error(String),
}

impl Status {
    pub fn is_configured(&self) -> bool {
        *self == Status::Configured
    }
}

#[derive(Clone, Debug)]
pub struct Env {
    pub exe: PathBuf,
    pub home: PathBuf,
    pub appdata: Option<PathBuf>,
    pub local_appdata: Option<PathBuf>,
    pub codex_home: PathBuf,
    pub search_path: Vec<PathBuf>,
    pub cli_timeout: Duration,
}

impl Env {
    pub fn detect(exe: PathBuf) -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        let appdata = dirs::config_dir();
        let codex_home = std::env::var_os("CODEX_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"));
        let mut search_path: Vec<PathBuf> =
            std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
        search_path.push(home.join(".local").join("bin"));
        search_path.push(home.join(".claude").join("local"));
        search_path.push(home.join(".npm-global").join("bin"));
        if let Some(a) = &appdata {
            search_path.push(a.join("npm"));
        }
        if !cfg!(windows) {
            search_path.push(PathBuf::from("/usr/local/bin"));
            search_path.push(PathBuf::from("/opt/homebrew/bin"));
        }
        Self {
            exe,
            home,
            appdata,
            local_appdata: dirs::data_local_dir(),
            codex_home,
            search_path,
            cli_timeout: Duration::from_secs(60),
        }
    }

    fn exe_text(&self) -> String {
        self.exe.to_string_lossy().into_owned()
    }
}

pub fn find_cli(env: &Env, name: &str) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) {
        ["exe", "cmd", "bat"].iter().map(|e| format!("{name}.{e}")).collect()
    } else {
        vec![name.to_owned()]
    };
    env.search_path.iter().flat_map(|dir| names.iter().map(move |n| dir.join(n))).find(|p| p.is_file())
}

pub fn same_exe(a: &str, b: &Path) -> bool {
    let a = a.trim().trim_matches('"');
    let norm = |s: &str| {
        let s = if cfg!(windows) { s.replace('/', "\\").to_lowercase() } else { s.to_owned() };
        s.trim_end_matches(['\\', '/']).to_owned()
    };
    if norm(a) == norm(&b.to_string_lossy()) {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

pub fn config_path(env: &Env, client: Client) -> Option<PathBuf> {
    match client {
        Client::ClaudeCode => None,
        Client::ClaudeDesktop => Some(claude_desktop_dir(env)?.join("claude_desktop_config.json")),
        Client::Codex => Some(env.codex_home.join("config.toml")),
        Client::Gemini => Some(env.home.join(".gemini").join("settings.json")),
    }
}

pub fn status(env: &Env, client: Client) -> Status {
    if !client_installed(env, client) {
        return Status::NotInstalled;
    }
    match client {
        Client::ClaudeCode => claude_code_status(env),
        Client::ClaudeDesktop | Client::Gemini => match config_path(env, client) {
            Some(path) => json_status(env, &path),
            None => Status::NotInstalled,
        },
        Client::Codex => codex_status(env),
    }
}

pub fn install(env: &Env, client: Client) -> Status {
    if !client_installed(env, client) {
        return Status::NotInstalled;
    }
    let done = match client {
        Client::ClaudeCode => claude_code_install(env),
        Client::ClaudeDesktop | Client::Gemini => match config_path(env, client) {
            Some(path) => edit_json(&path, |servers| set_json_entry(servers, env)),
            None => return Status::NotInstalled,
        },
        Client::Codex => edit_codex(env, true),
    };
    match done {
        Ok(()) => status(env, client),
        Err(e) => Status::Error(e),
    }
}

pub fn uninstall(env: &Env, client: Client) -> Status {
    if !client_installed(env, client) {
        return Status::NotInstalled;
    }
    let done = match client {
        Client::ClaudeCode => claude_code_remove(env),
        Client::ClaudeDesktop | Client::Gemini => match config_path(env, client) {
            Some(path) if path.exists() => edit_json(&path, |servers| {
                servers.0.retain(|(k, _)| k != SERVER_NAME);
                Ok(())
            }),
            _ => Ok(()),
        },
        Client::Codex => edit_codex(env, false),
    };
    match done {
        Ok(()) => status(env, client),
        Err(e) => Status::Error(e),
    }
}

pub fn install_all(env: &Env) -> Vec<(Client, Status)> {
    Client::ALL
        .iter()
        .map(|&c| {
            let s = if client_installed(env, c) { install(env, c) } else { Status::NotInstalled };
            (c, s)
        })
        .collect()
}

pub fn snippet(client: Client, exe: &Path) -> String {
    let exe_text = exe.to_string_lossy();
    match client {
        Client::ClaudeCode => {
            format!("claude mcp add --scope user {SERVER_NAME} -- \"{exe_text}\" {}", crate::BRIDGE_ARG)
        }
        Client::ClaudeDesktop | Client::Gemini => {
            let v = json!({ "mcpServers": { SERVER_NAME: { "command": exe_text, "args": [crate::BRIDGE_ARG] } } });
            serde_json::to_string_pretty(&v).unwrap_or_default()
        }
        Client::Codex => {
            let mut doc = toml_edit::DocumentMut::new();
            let mut servers = toml_edit::Table::new();
            servers.set_implicit(true);
            let mut entry = toml_edit::Table::new();
            entry.insert("command", toml_edit::value(exe_text.as_ref()));
            entry.insert("args", toml_edit::value(bridge_args_toml()));
            servers.insert(SERVER_NAME, toml_edit::Item::Table(entry));
            doc.insert("mcp_servers", toml_edit::Item::Table(servers));
            doc.to_string().trim().to_owned()
        }
    }
}

fn client_installed(env: &Env, client: Client) -> bool {
    match client {
        Client::ClaudeCode => find_cli(env, "claude").is_some(),
        Client::ClaudeDesktop => claude_desktop_dir(env).is_some_and(|d| d.is_dir()),
        Client::Codex => env.codex_home.is_dir() || find_cli(env, "codex").is_some(),
        Client::Gemini => env.home.join(".gemini").is_dir() || find_cli(env, "gemini").is_some(),
    }
}

fn claude_desktop_dir(env: &Env) -> Option<PathBuf> {
    if let Some(local) = &env.local_appdata
        && let Ok(entries) = std::fs::read_dir(local.join("Packages"))
    {
        for e in entries.filter_map(Result::ok) {
            if e.file_name().to_string_lossy().starts_with("Claude_") {
                let dir = e.path().join("LocalCache").join("Roaming").join("Claude");
                if dir.is_dir() {
                    return Some(dir);
                }
            }
        }
    }
    env.appdata.as_ref().map(|a| a.join("Claude"))
}

fn claude_run(env: &Env, args: &[&str]) -> Result<process::CliOutput, String> {
    let cli = find_cli(env, "claude").ok_or_else(|| "Claude Code (claude) was not found".to_owned())?;
    process::run(&cli, args, env.cli_timeout).map_err(|e| format!("claude {}: {e}", args.join(" ")))
}

fn claude_code_status(env: &Env) -> Status {
    match claude_run(env, &["mcp", "get", SERVER_NAME]) {
        Ok(out) => parse_claude_get(&out, &env.exe),
        Err(e) => Status::Error(e),
    }
}

fn parse_claude_get(out: &process::CliOutput, exe: &Path) -> Status {
    let text = out.text();
    let field = |name: &str| {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(name).and_then(|r| r.strip_prefix(':')).map(|v| v.trim().to_owned()))
    };
    match field("Command") {
        Some(command) => {
            let args = field("Args").unwrap_or_default();
            if same_exe(&command, exe) && args.split_whitespace().any(|a| a == crate::BRIDGE_ARG) {
                Status::Configured
            } else {
                Status::OtherExe(format!("{command} {args}").trim().to_owned())
            }
        }
        None if text.to_ascii_lowercase().contains("no mcp server found") || !out.success() => Status::NotConfigured,
        None => Status::Error(first_line(&text)),
    }
}

fn claude_code_install(env: &Env) -> Result<(), String> {
    match claude_code_status(env) {
        Status::Configured => return Ok(()),
        Status::OtherExe(_) => {
            let _ = claude_run(env, &["mcp", "remove", "--scope", "user", SERVER_NAME]);
        }
        _ => {}
    }
    let exe = env.exe_text();
    let out = claude_run(env, &["mcp", "add", "--scope", "user", SERVER_NAME, "--", &exe, crate::BRIDGE_ARG])?;
    if out.success() { Ok(()) } else { Err(format!("claude mcp add failed: {}", first_line(&out.text()))) }
}

fn claude_code_remove(env: &Env) -> Result<(), String> {
    let out = claude_run(env, &["mcp", "remove", "--scope", "user", SERVER_NAME])?;
    match parse_claude_get(&out, &env.exe) {
        _ if out.success() => Ok(()),
        Status::NotConfigured => Ok(()),
        _ => Err(format!("claude mcp remove failed: {}", first_line(&out.text()))),
    }
}

fn first_line(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("no output");
    line.chars().take(300).collect()
}

#[derive(Debug, Default)]
struct RawObject(Vec<(String, Box<RawValue>)>);

impl RawObject {
    fn get(&self, key: &str) -> Option<&RawValue> {
        self.0.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v.as_ref())
    }

    fn set(&mut self, key: &str, value: Box<RawValue>) {
        match self.0.iter().position(|(k, _)| k == key) {
            Some(i) => {
                self.0[i].1 = value;
                let mut n = 0;
                self.0.retain(|(k, _)| {
                    n += usize::from(k == key);
                    k != key || n == 1
                });
            }
            None => self.0.push((key.to_owned(), value)),
        }
    }

    fn write(&self, indent: usize) -> String {
        if self.0.is_empty() {
            return "{}".to_owned();
        }
        let pad = " ".repeat(indent + 2);
        let mut out = String::from("{\n");
        for (i, (k, v)) in self.0.iter().enumerate() {
            out.push_str(&pad);
            out.push_str(&serde_json::to_string(k).unwrap_or_default());
            out.push_str(": ");
            out.push_str(v.get());
            if i + 1 < self.0.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str(&" ".repeat(indent));
        out.push('}');
        out
    }
}

impl<'de> Deserialize<'de> for RawObject {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = RawObject;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RawObject, A::Error> {
                let mut out = Vec::new();
                while let Some((k, v)) = map.next_entry::<String, Box<RawValue>>()? {
                    out.push((k, v));
                }
                Ok(RawObject(out))
            }
        }
        d.deserialize_map(V)
    }
}

fn raw(text: String) -> Result<Box<RawValue>, String> {
    RawValue::from_string(text).map_err(|e| e.to_string())
}

fn pretty_at(value: &Value, indent: usize) -> String {
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    text.replace('\n', &format!("\n{}", " ".repeat(indent)))
}

fn read_json_file(path: &Path) -> Result<RawObject, String> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(RawObject::default()),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let text = String::from_utf8_lossy(&bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    if text.trim().is_empty() {
        return Ok(RawObject::default());
    }
    serde_json::from_str(text).map_err(|e| {
        format!(
            "{} is not a plain JSON object ({e}): fix it, or add the entry by hand (see Manual setup)",
            path.display()
        )
    })
}

fn servers_of(obj: &RawObject) -> Result<RawObject, String> {
    match obj.get("mcpServers") {
        Some(raw) => serde_json::from_str(raw.get()).map_err(|_| "mcpServers is not a JSON object".to_owned()),
        None => Ok(RawObject::default()),
    }
}

fn json_status(env: &Env, path: &Path) -> Status {
    let entry = read_json_file(path).and_then(|obj| servers_of(&obj)).map(|servers| {
        servers.get(SERVER_NAME).map(|raw| serde_json::from_str::<Value>(raw.get()).unwrap_or(Value::Null))
    });
    match entry {
        Err(e) => Status::Error(e),
        Ok(None) => Status::NotConfigured,
        Ok(Some(entry)) => entry_status(env, entry.get("command"), entry.get("args")),
    }
}

fn entry_status(env: &Env, command: Option<&Value>, args: Option<&Value>) -> Status {
    let command = command.and_then(Value::as_str).unwrap_or("");
    let args: Vec<&str> =
        args.and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    if same_exe(command, &env.exe) && args == [crate::BRIDGE_ARG] {
        Status::Configured
    } else {
        Status::OtherExe(format!("{command} {}", args.join(" ")).trim().to_owned())
    }
}

fn set_json_entry(servers: &mut RawObject, env: &Env) -> Result<(), String> {
    let mut entry = match servers.get(SERVER_NAME).map(|r| serde_json::from_str::<Value>(r.get())) {
        Some(Ok(Value::Object(m))) => m,
        _ => Map::new(),
    };
    entry.insert("command".into(), Value::String(env.exe_text()));
    entry.insert("args".into(), json!([crate::BRIDGE_ARG]));
    servers.set(SERVER_NAME, raw(pretty_at(&Value::Object(entry), 4))?);
    Ok(())
}

fn edit_json(path: &Path, f: impl FnOnce(&mut RawObject) -> Result<(), String>) -> Result<(), String> {
    let mut obj = read_json_file(path)?;
    let mut servers = servers_of(&obj)?;
    f(&mut servers)?;
    obj.set("mcpServers", raw(servers.write(2))?);
    let mut text = obj.write(0);
    text.push('\n');
    serde_json::from_str::<Value>(&text).map_err(|e| format!("internal error, nothing written: {e}"))?;
    write_config(path, &text)
}

fn bridge_args_toml() -> toml_edit::Array {
    let mut args = toml_edit::Array::new();
    args.push(crate::BRIDGE_ARG);
    args
}

fn read_codex(env: &Env) -> Result<(PathBuf, toml_edit::DocumentMut), String> {
    let path = env.codex_home.join("config.toml");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned();
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("{} is not valid TOML ({}): fix it first", path.display(), first_line(&e.to_string())))?;
    Ok((path, doc))
}

fn codex_status(env: &Env) -> Status {
    let (_, doc) = match read_codex(env) {
        Ok(d) => d,
        Err(e) => return Status::Error(e),
    };
    let Some(entry) = doc.get("mcp_servers").and_then(|s| s.as_table_like()).and_then(|s| s.get(SERVER_NAME)) else {
        return Status::NotConfigured;
    };
    let Some(entry) = entry.as_table_like() else {
        return Status::OtherExe(String::new());
    };
    let command = entry.get("command").and_then(|i| i.as_str()).map(|s| Value::String(s.to_owned()));
    let args = entry
        .get("args")
        .and_then(|i| i.as_array())
        .map(|a| Value::Array(a.iter().filter_map(|v| v.as_str()).map(|s| Value::String(s.to_owned())).collect()));
    entry_status(env, command.as_ref(), args.as_ref())
}

fn edit_codex(env: &Env, install: bool) -> Result<(), String> {
    let (path, mut doc) = read_codex(env)?;
    let root = doc.as_table_mut();
    if install {
        if !root.contains_key("mcp_servers") {
            let mut t = toml_edit::Table::new();
            t.set_implicit(true);
            root.insert("mcp_servers", toml_edit::Item::Table(t));
        }
        let inline_parent = !root.get("mcp_servers").is_some_and(toml_edit::Item::is_table);
        let servers = root
            .get_mut("mcp_servers")
            .and_then(toml_edit::Item::as_table_like_mut)
            .ok_or_else(|| format!("mcp_servers of {} is not a table", path.display()))?;
        let exe = env.exe_text();
        match servers.get_mut(SERVER_NAME).and_then(toml_edit::Item::as_table_like_mut) {
            Some(entry) => {
                entry.insert("command", toml_edit::value(exe.as_str()));
                entry.insert("args", toml_edit::value(bridge_args_toml()));
            }
            None if inline_parent => {
                let mut entry = toml_edit::InlineTable::new();
                entry.insert("command", exe.as_str().into());
                entry.insert("args", toml_edit::Value::Array(bridge_args_toml()));
                servers.insert(SERVER_NAME, toml_edit::Item::Value(toml_edit::Value::InlineTable(entry)));
            }
            None => {
                let mut entry = toml_edit::Table::new();
                entry.insert("command", toml_edit::value(exe.as_str()));
                entry.insert("args", toml_edit::value(bridge_args_toml()));
                servers.insert(SERVER_NAME, toml_edit::Item::Table(entry));
            }
        }
    } else {
        let Some(servers) = root.get_mut("mcp_servers").and_then(toml_edit::Item::as_table_like_mut) else {
            return Ok(());
        };
        if servers.remove(SERVER_NAME).is_none() {
            return Ok(());
        }
    }
    let text = doc.to_string();
    text.parse::<toml_edit::DocumentMut>().map_err(|e| format!("internal error, nothing written: {e}"))?;
    write_config(&path, &text)
}

pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".hammerpro.bak");
    path.with_file_name(name)
}

fn write_config(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    if let Ok(old) = std::fs::read_to_string(path)
        && old == text
    {
        return Ok(());
    }
    let bak = backup_path(path);
    if path.exists() && !bak.exists() {
        std::fs::copy(path, &bak).map_err(|e| format!("cannot back up {}: {e}", path.display()))?;
    }
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".hammerpro.tmp");
    let tmp = path.with_file_name(tmp_name);
    std::fs::write(&tmp, text).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("cannot replace {}: {e}", path.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::temp_dir;

    fn env(root: &Path) -> Env {
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        Env {
            exe: PathBuf::from(if cfg!(windows) {
                "C:\\Program Files\\Hammer Pro\\hammerpro.exe"
            } else {
                "/opt/hammer pro/hammerpro"
            }),
            codex_home: home.join(".codex"),
            home,
            appdata: Some(root.join("appdata")),
            local_appdata: Some(root.join("local")),
            search_path: vec![root.join("bin")],
            cli_timeout: Duration::from_secs(30),
        }
    }

    #[test]
    fn nothing_installed_touches_nothing() {
        let root = temp_dir("install_none");
        let env = env(&root);
        for (c, s) in install_all(&env) {
            assert_eq!(s, Status::NotInstalled, "{}", c.label());
        }
        assert!(!root.join("appdata").exists());
        assert!(!env.codex_home.exists());
        assert!(!env.home.join(".gemini").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn claude_desktop_keeps_the_rest_of_the_file() {
        let root = temp_dir("install_desktop");
        let env = env(&root);
        let dir = root.join("appdata").join("Claude");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("claude_desktop_config.json");
        let original = "{\n  \"zeta\": [1, 2,   3],\n  \"mcpServers\": {\n    \"other\": {\"command\": \"x\", \"args\": [\"-y\"]}\n  },\n  \"alpha\": {\"b\": 1.50, \"a\": \"\\u00e9\"}\n}\n";
        std::fs::write(&path, original).unwrap();
        assert_eq!(status(&env, Client::ClaudeDesktop), Status::NotConfigured);
        assert_eq!(install(&env, Client::ClaudeDesktop), Status::Configured);
        let text = std::fs::read_to_string(&path).unwrap();
        let zeta = text.find("\"zeta\": [1, 2,   3]").expect("zeta kept");
        let servers = text.find("\"mcpServers\"").unwrap();
        let alpha = text.find("\"alpha\": {\"b\": 1.50, \"a\": \"\\u00e9\"}").expect("alpha kept");
        assert!(zeta < servers && servers < alpha);
        assert!(text.contains("\"other\": {\"command\": \"x\", \"args\": [\"-y\"]}"));
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["mcpServers"][SERVER_NAME]["command"], env.exe_text());
        assert_eq!(v["mcpServers"][SERVER_NAME]["args"], json!(["--mcp"]));
        assert_eq!(std::fs::read_to_string(backup_path(&path)).unwrap(), original);
        assert_eq!(install(&env, Client::ClaudeDesktop), Status::Configured);
        assert_eq!(std::fs::read_to_string(backup_path(&path)).unwrap(), original);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text, "installing twice changes nothing");

        let mut other = env.clone();
        other.exe = root.join("old").join("hammerpro.exe");
        assert!(matches!(status(&other, Client::ClaudeDesktop), Status::OtherExe(c) if c.contains("Hammer Pro")));
        assert_eq!(install(&other, Client::ClaudeDesktop), Status::Configured);
        assert!(matches!(status(&env, Client::ClaudeDesktop), Status::OtherExe(_)));

        assert_eq!(uninstall(&env, Client::ClaudeDesktop), Status::NotConfigured);
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(v["mcpServers"].get(SERVER_NAME).is_none());
        assert_eq!(v["mcpServers"]["other"]["command"], "x");
        assert_eq!(v["zeta"], json!([1, 2, 3]));
        assert!(!path.with_file_name("claude_desktop_config.json.hammerpro.tmp").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn claude_desktop_store_package_and_new_files() {
        let root = temp_dir("install_store");
        let env = env(&root);
        let store = root
            .join("local")
            .join("Packages")
            .join("Claude_pzs8sxrjxfjjc")
            .join("LocalCache")
            .join("Roaming")
            .join("Claude");
        std::fs::create_dir_all(&store).unwrap();
        std::fs::create_dir_all(root.join("appdata").join("Claude")).unwrap();
        assert_eq!(config_path(&env, Client::ClaudeDesktop), Some(store.join("claude_desktop_config.json")));
        assert_eq!(install(&env, Client::ClaudeDesktop), Status::Configured);
        let path = store.join("claude_desktop_config.json");
        assert!(path.exists() && !backup_path(&path).exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn gemini_with_comments_is_left_alone() {
        let root = temp_dir("install_gemini");
        let env = env(&root);
        let dir = env.home.join(".gemini");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let commented = "{\n  // my theme\n  \"theme\": \"Dark\"\n}\n";
        std::fs::write(&path, commented).unwrap();
        assert!(matches!(status(&env, Client::Gemini), Status::Error(_)));
        assert!(matches!(install(&env, Client::Gemini), Status::Error(_)));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), commented);
        assert!(!backup_path(&path).exists());
        std::fs::write(&path, "\u{feff}{\"theme\": \"Dark\", \"mcpServers\": {}}").unwrap();
        assert_eq!(install(&env, Client::Gemini), Status::Configured);
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["theme"], "Dark");
        assert_eq!(v["mcpServers"][SERVER_NAME]["args"][0], "--mcp");
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\"args\"", "\"timeout\": 30000,\n      \"args\"")
            .replace("--mcp", "--old");
        std::fs::write(&path, text).unwrap();
        assert!(matches!(status(&env, Client::Gemini), Status::OtherExe(_)));
        assert_eq!(install(&env, Client::Gemini), Status::Configured);
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["mcpServers"][SERVER_NAME]["timeout"], 30000);
        assert_eq!(uninstall(&env, Client::Gemini), Status::NotConfigured);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn codex_toml_round_trip() {
        let root = temp_dir("install_codex");
        let env = env(&root);
        std::fs::create_dir_all(&env.codex_home).unwrap();
        let path = env.codex_home.join("config.toml");
        let original = "# my settings\nmodel = \"gpt-5\"  # inline comment\n\n[mcp_servers.other]\ncommand = \"npx\"\nargs = [\"-y\", \"thing\"]\n\n[profiles.fast]\nmodel = \"mini\"\n";
        std::fs::write(&path, original).unwrap();
        assert_eq!(status(&env, Client::Codex), Status::NotConfigured);
        assert_eq!(install(&env, Client::Codex), Status::Configured);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# my settings\nmodel = \"gpt-5\"  # inline comment\n"), "{text}");
        assert!(text.contains("[mcp_servers.other]\ncommand = \"npx\"\nargs = [\"-y\", \"thing\"]"));
        assert!(text.contains("[profiles.fast]\nmodel = \"mini\""));
        assert!(text.contains("[mcp_servers.hammer-pro]"), "{text}");
        let doc: toml_edit::DocumentMut = text.parse().unwrap();
        assert_eq!(doc["mcp_servers"][SERVER_NAME]["command"].as_str(), Some(env.exe_text().as_str()));
        assert_eq!(std::fs::read_to_string(backup_path(&path)).unwrap(), original);
        assert_eq!(install(&env, Client::Codex), Status::Configured);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert_eq!(uninstall(&env, Client::Codex), Status::NotConfigured);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains(SERVER_NAME));
        assert!(text.contains("[mcp_servers.other]"));
        std::fs::write(&path, "mcp_servers = { other = { command = \"x\" } }\n").unwrap();
        assert_eq!(install(&env, Client::Codex), Status::Configured);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(status(&env, Client::Codex), Status::NotConfigured);
        assert_eq!(install(&env, Client::Codex), Status::Configured);
        std::fs::write(&path, "model = \n").unwrap();
        assert!(matches!(install(&env, Client::Codex), Status::Error(_)));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "model = \n");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn claude_get_output() {
        let exe = Path::new(if cfg!(windows) { "C:\\Tools\\hammerpro.exe" } else { "/tools/hammerpro" });
        let ok = |stdout: &str| process::CliOutput { code: Some(0), stdout: stdout.to_owned(), stderr: String::new() };
        let shown = format!(
            "hammer-pro:\n  Scope: User config (available in all your projects)\n  Status: Connected\n  Type: stdio\n  Command: {}\n  Args: --mcp\n  Environment:\n",
            exe.display()
        );
        assert_eq!(parse_claude_get(&ok(&shown), exe), Status::Configured);
        let elsewhere = shown.replace(&exe.display().to_string(), "D:/old/hammerpro.exe");
        assert!(matches!(parse_claude_get(&ok(&elsewhere), exe), Status::OtherExe(c) if c.contains("old")));
        let missing = process::CliOutput {
            code: Some(1),
            stdout: String::new(),
            stderr: "No MCP server found with name: hammer-pro".into(),
        };
        assert_eq!(parse_claude_get(&missing, exe), Status::NotConfigured);
        assert!(matches!(parse_claude_get(&ok("something odd"), exe), Status::Error(_)));
    }

    #[cfg(windows)]
    #[test]
    fn claude_code_through_its_cli() {
        let root = temp_dir("install_claude_cli");
        let env = env(&root);
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let script = "@echo off\r\n\
            if \"%~2\"==\"add\" (\r\n  echo   Command: %~7> \"%~dp0state.txt\"\r\n  echo   Args: %~8>> \"%~dp0state.txt\"\r\n  echo %*> \"%~dp0last_add.txt\"\r\n  exit /b 0\r\n)\r\n\
            if \"%~2\"==\"get\" (\r\n  if exist \"%~dp0state.txt\" (\r\n    echo hammer-pro:\r\n    type \"%~dp0state.txt\"\r\n    exit /b 0\r\n  )\r\n  echo No MCP server found with name: %~3 1>&2\r\n  exit /b 1\r\n)\r\n\
            if \"%~2\"==\"remove\" (\r\n  del \"%~dp0state.txt\" 2>nul\r\n  exit /b 0\r\n)\r\n\
            exit /b 2\r\n";
        std::fs::write(bin.join("claude.cmd"), script).unwrap();
        assert_eq!(status(&env, Client::ClaudeCode), Status::NotConfigured);
        assert_eq!(install(&env, Client::ClaudeCode), Status::Configured);
        let add = std::fs::read_to_string(bin.join("last_add.txt")).unwrap();
        assert!(add.contains("mcp add --scope user hammer-pro --"), "{add}");
        assert!(add.contains("--mcp"), "{add}");
        assert_eq!(uninstall(&env, Client::ClaudeCode), Status::NotConfigured);
        assert!(!env.home.join(".claude.json").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn snippets_are_valid() {
        let exe = Path::new("C:\\Program Files\\Hammer Pro\\hammerpro.exe");
        let v: Value = serde_json::from_str(&snippet(Client::ClaudeDesktop, exe)).unwrap();
        assert_eq!(v["mcpServers"][SERVER_NAME]["command"], exe.to_string_lossy().as_ref());
        let doc: toml_edit::DocumentMut = snippet(Client::Codex, exe).parse().unwrap();
        assert_eq!(doc["mcp_servers"][SERVER_NAME]["args"][0].as_str(), Some("--mcp"));
        assert!(snippet(Client::ClaudeCode, exe).starts_with("claude mcp add --scope user hammer-pro -- \""));
        assert!(same_exe("c:/program files/hammer pro/hammerpro.exe", exe) == cfg!(windows));
    }
}
