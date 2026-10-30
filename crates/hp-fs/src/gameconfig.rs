//! Classic Hammer game configurations: `gameconfig.txt` (Hammer) and
//! `hammerplusplus_gameconfig.txt` (Hammer++).
//!
//! This module finds those files, reads and writes them without losing the keys it does not
//! know, turns one `"Configs" / "Games" / "<name>"` entry into a [`GameProfile`] and answers
//! which `vbsp`, `vvis`, `vrad` and `studiomdl` an entry points to.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

pub use hp_kv::Block as KvBlock;
use hp_kv::{Block, TextEncoding};
use parking_lot::Mutex;

use crate::detect::path_key;
use crate::games::{Branch, GameProfile, LightingPasses};

pub const FILE_NAMES: [&str; 2] = ["gameconfig.txt", "hammerplusplus_gameconfig.txt"];

const ID_PREFIX: &str = "cfg:";

pub fn is_config_file_name(name: &str) -> bool {
    FILE_NAMES.iter().any(|n| n.eq_ignore_ascii_case(name))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigKind {
    Hammer,
    HammerPlusPlus,
}

impl ConfigKind {
    pub fn of_file(path: &Path) -> ConfigKind {
        let name = path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if name.starts_with("hammerplusplus") { ConfigKind::HammerPlusPlus } else { ConfigKind::Hammer }
    }

    pub fn label(self) -> &'static str {
        match self {
            ConfigKind::Hammer => "Hammer",
            ConfigKind::HammerPlusPlus => "Hammer++",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConfigId {
    pub file: PathBuf,
    pub name: String,
}

impl ConfigId {
    pub fn new(file: impl Into<PathBuf>, name: impl Into<String>) -> Self {
        ConfigId { file: file.into(), name: name.into() }
    }

    pub fn to_id(&self) -> String {
        format!("{ID_PREFIX}{}|{}", self.file.display(), self.name)
    }

    pub fn parse(id: &str) -> Option<ConfigId> {
        let rest = id.trim().strip_prefix(ID_PREFIX)?;
        let (file, name) = rest.split_once('|')?;
        (!file.is_empty() && !name.is_empty()).then(|| ConfigId::new(file, name))
    }
}

pub fn is_config_id(id: &str) -> bool {
    ConfigId::parse(id).is_some()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigEntry {
    pub id: ConfigId,
    pub kind: ConfigKind,
    pub dir: PathBuf,
}

impl ConfigEntry {
    pub fn key(&self) -> String {
        self.id.to_id()
    }

    pub fn label(&self) -> String {
        format!("{}  [{}]", self.id.name, self.kind.label())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameConfig {
    pub name: String,
    pub game_dir: String,
    pub game_data: Vec<String>,
    pub default_point_entity: String,
    pub default_solid_entity: String,
    pub texture_scale: String,
    pub lightmap_scale: String,
    pub cordon_texture: String,
    pub game_exe_dir: String,
    pub map_dir: String,
    pub prefab_dir: String,
    pub game_exe: String,
    pub bsp: String,
    pub vis: String,
    pub light: String,
    pub mdl: String,
    pub bsp_dir: String,
}

fn game_data_index(key: &str) -> Option<usize> {
    let head = key.get(..8)?;
    if head.eq_ignore_ascii_case("GameData") { key.get(8..)?.parse().ok() } else { None }
}

impl GameConfig {
    pub fn from_block(entry: &Block) -> GameConfig {
        let hammer = entry.child("Hammer");
        let h = |k: &str| hammer.and_then(|b| b.get(k)).unwrap_or("").to_owned();
        let mut data: Vec<(usize, String)> = hammer
            .map(|b| b.props.iter().filter_map(|(k, v)| game_data_index(k).map(|i| (i, v.clone()))).collect())
            .unwrap_or_default();
        data.sort_by_key(|(i, _)| *i);
        GameConfig {
            name: entry.name.clone(),
            game_dir: entry.get("GameDir").unwrap_or("").to_owned(),
            game_data: data.into_iter().map(|(_, v)| v).collect(),
            default_point_entity: h("DefaultPointEntity"),
            default_solid_entity: h("DefaultSolidEntity"),
            texture_scale: h("DefaultTextureScale"),
            lightmap_scale: h("DefaultLightmapScale"),
            cordon_texture: h("CordonTexture"),
            game_exe_dir: h("GameExeDir"),
            map_dir: h("MapDir"),
            prefab_dir: h("PrefabDir"),
            game_exe: h("GameExe"),
            bsp: h("BSP"),
            vis: h("Vis"),
            light: h("Light"),
            mdl: h("MDL"),
            bsp_dir: h("BSPDir"),
        }
    }

    pub fn apply_to(&self, entry: &mut Block) {
        entry.name = self.name.clone();
        entry.set("GameDir", self.game_dir.clone());
        if entry.child("Hammer").is_none() {
            entry.push_child(Block::new("Hammer"));
        }
        let Some(h) = entry.child_mut("Hammer") else { return };
        h.props.retain(|(k, _)| game_data_index(k).is_none());
        for (i, v) in self.game_data.iter().enumerate() {
            h.props.insert(i, (format!("GameData{i}"), v.clone()));
        }
        let mut put = |key: &str, value: &str| {
            if value.trim().is_empty() {
                h.remove(key);
            } else {
                h.set(key, value.trim());
            }
        };
        put("DefaultTextureScale", &self.texture_scale);
        put("DefaultLightmapScale", &self.lightmap_scale);
        put("DefaultSolidEntity", &self.default_solid_entity);
        put("DefaultPointEntity", &self.default_point_entity);
        put("GameExe", &self.game_exe);
        put("BSP", &self.bsp);
        put("Vis", &self.vis);
        put("Light", &self.light);
        put("MDL", &self.mdl);
        put("GameExeDir", &self.game_exe_dir);
        put("MapDir", &self.map_dir);
        put("BSPDir", &self.bsp_dir);
        put("PrefabDir", &self.prefab_dir);
        put("CordonTexture", &self.cordon_texture);
    }

    pub fn new_entry(name: &str) -> Block {
        let mut entry = Block::new(name);
        let mut hammer = Block::new("Hammer");
        hammer.set("TextureFormat", "5");
        hammer.set("MapFormat", "4");
        hammer.set("DefaultTextureScale", "0.250000");
        hammer.set("DefaultLightmapScale", "16");
        hammer.set("DefaultSolidEntity", "func_detail");
        hammer.set("DefaultPointEntity", "info_player_start");
        hammer.set("CordonTexture", "tools\\toolsskybox");
        hammer.set("MaterialExcludeCount", "0");
        entry.push_child(hammer);
        entry
    }
}

fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "'"))
}

fn write_block_quoted(b: &Block, indent: usize, out: &mut String) {
    let tabs = "\t".repeat(indent);
    out.push_str(&format!("{tabs}{}\n{tabs}{{\n", quoted(&b.name)));
    for (k, v) in &b.props {
        out.push_str(&format!("{tabs}\t{}\t\t{}\n", quoted(k), quoted(v)));
    }
    for c in &b.children {
        write_block_quoted(c, indent + 1, out);
    }
    out.push_str(&format!("{tabs}}}\n"));
}

pub fn write_config(root: &Block) -> String {
    let mut out = String::new();
    for c in &root.children {
        write_block_quoted(c, 0, &mut out);
    }
    out
}

pub fn write_entry(entry: &Block) -> String {
    let mut out = String::new();
    write_block_quoted(entry, 0, &mut out);
    out
}

pub fn parse_entry(text: &str) -> Result<Block, String> {
    let (root, issues) = hp_kv::parse_with_issues(text);
    if !issues.is_clean() {
        return Err("the text is not valid KeyValues (a quote or a brace is missing)".to_owned());
    }
    match root.children.into_iter().next() {
        Some(entry) if !entry.name.trim().is_empty() => Ok(entry),
        _ => Err("expected: \"Game name\" { ... }".to_owned()),
    }
}

#[derive(Clone, Debug)]
pub struct ConfigFile {
    pub path: PathBuf,
    pub root: Block,
    pub encoding: TextEncoding,
}

impl ConfigFile {
    pub fn load(path: &Path) -> std::io::Result<ConfigFile> {
        let bytes = std::fs::read(path)?;
        let (text, encoding) = hp_kv::decode(&bytes);
        let root = hp_kv::parse(&text).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        Ok(ConfigFile { path: path.to_path_buf(), root, encoding })
    }

    pub fn save(&self) -> std::io::Result<()> {
        let text = write_config(&self.root);
        let (bytes, _) = hp_kv::encode(&text, self.encoding);
        std::fs::write(&self.path, bytes)?;
        invalidate();
        Ok(())
    }

    fn games_block(&self) -> Option<&Block> {
        self.root.child("Configs")?.child("Games")
    }

    pub fn names(&self) -> Vec<String> {
        self.games_block().map(|g| g.children.iter().map(|c| c.name.clone()).collect()).unwrap_or_default()
    }

    pub fn game(&self, name: &str) -> Option<&Block> {
        self.games_block()?.children.iter().find(|c| c.name == name)
    }

    pub fn replace_game(&mut self, name: &str, entry: Block) {
        if self.root.child("Configs").is_none() {
            self.root.push_child(Block::new("Configs"));
        }
        let Some(configs) = self.root.child_mut("Configs") else { return };
        if configs.child("Games").is_none() {
            configs.push_child(Block::new("Games"));
        }
        let Some(games) = configs.child_mut("Games") else { return };
        match games.children.iter_mut().find(|c| c.name == name) {
            Some(slot) => *slot = entry,
            None => games.children.push(entry),
        }
    }
}

pub fn entries_of_file(path: &Path) -> Vec<ConfigEntry> {
    let Ok(file) = ConfigFile::load(path) else { return Vec::new() };
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let kind = ConfigKind::of_file(path);
    file.names().into_iter().map(|name| ConfigEntry { id: ConfigId::new(path, name), kind, dir: dir.clone() }).collect()
}

pub fn entries_of_files(files: &[PathBuf]) -> Vec<ConfigEntry> {
    files.iter().flat_map(|f| entries_of_file(f)).collect()
}

fn steam_root_of(file: &Path) -> Option<PathBuf> {
    let mut cur = file.parent();
    while let Some(dir) = cur {
        if dir.file_name().is_some_and(|n| n.eq_ignore_ascii_case("steamapps")) {
            return dir.parent().map(Path::to_path_buf);
        }
        cur = dir.parent();
    }
    None
}

pub fn expand_vars(value: &str, config_file: &Path) -> String {
    if !value.contains('$') {
        return value.trim().to_owned();
    }
    let steam = steam_root_of(config_file)
        .or_else(|| {
            ["ProgramFiles(x86)", "ProgramFiles"]
                .iter()
                .filter_map(|v| std::env::var_os(v))
                .map(|p| PathBuf::from(p).join("Steam"))
                .find(|p| p.is_dir())
        })
        .unwrap_or_default();
    let steam_text = steam.display().to_string();
    let common = steam.join("steamapps").join("common").display().to_string();
    value.replace("$SteamUserDir", &common).replace("$SteamDir", &steam_text).trim().to_owned()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildTools {
    pub vbsp: Option<PathBuf>,
    pub vvis: Option<PathBuf>,
    pub vrad: Option<PathBuf>,
    pub studiomdl: Option<PathBuf>,
    pub game_exe: Option<PathBuf>,
    pub bsp_dir: Option<PathBuf>,
    pub map_dir: Option<PathBuf>,
    pub game_dir: PathBuf,
    pub config_dir: PathBuf,
}

fn exe_names(stem: &str) -> [String; 2] {
    [format!("{stem}.exe"), stem.to_owned()]
}

fn find_tool(configured: &str, file: &Path, root: &Path, stem: &str) -> Option<PathBuf> {
    let configured = expand_vars(configured, file);
    if !configured.is_empty() {
        return Some(PathBuf::from(configured));
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = file.parent() {
        dirs.extend([d.to_path_buf(), d.join("x64"), d.join("win64")]);
    }
    if !root.as_os_str().is_empty() {
        let bin = root.join("bin");
        dirs.extend([bin.clone(), bin.join("x64"), bin.join("win64")]);
    }
    for d in dirs {
        for n in exe_names(stem) {
            let p = d.join(n);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

pub fn build_tools_of(file: &Path, cfg: &GameConfig) -> BuildTools {
    let ex = |s: &str| {
        let s = expand_vars(s, file);
        (!s.is_empty()).then(|| PathBuf::from(s))
    };
    let root = ex(&cfg.game_exe_dir)
        .or_else(|| ex(&cfg.game_dir).and_then(|g| g.parent().map(Path::to_path_buf)))
        .unwrap_or_default();
    BuildTools {
        vbsp: find_tool(&cfg.bsp, file, &root, "vbsp"),
        vvis: find_tool(&cfg.vis, file, &root, "vvis"),
        vrad: find_tool(&cfg.light, file, &root, "vrad"),
        studiomdl: find_tool(&cfg.mdl, file, &root, "studiomdl"),
        game_exe: ex(&cfg.game_exe),
        bsp_dir: ex(&cfg.bsp_dir),
        map_dir: ex(&cfg.map_dir),
        game_dir: ex(&cfg.game_dir).unwrap_or_default(),
        config_dir: file.parent().map(Path::to_path_buf).unwrap_or_default(),
    }
}

fn config_of(id: &str) -> Option<(ConfigId, ConfigFile, GameConfig)> {
    let cid = ConfigId::parse(id)?;
    let file = ConfigFile::load(&cid.file).ok()?;
    let cfg = GameConfig::from_block(file.game(&cid.name)?);
    Some((cid, file, cfg))
}

/// The folder prefabs are read from and saved to: the `PrefabDir` of the configuration.
pub fn prefab_dir(id: &str) -> Option<PathBuf> {
    let (cid, _, cfg) = config_of(id)?;
    let dir = expand_vars(&cfg.prefab_dir, &cid.file);
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

pub fn build_tools(id: &str) -> Option<BuildTools> {
    let (cid, _, cfg) = config_of(id)?;
    Some(build_tools_of(&cid.file, &cfg))
}

fn build_profile(cid: &ConfigId, cfg: &GameConfig) -> GameProfile {
    let ex = |s: &str| PathBuf::from(expand_vars(s, &cid.file));
    let game_dir = ex(&cfg.game_dir);
    let mut p = GameProfile::from_game_dir(&game_dir).unwrap_or_else(|| GameProfile {
        id: String::new(),
        name: String::new(),
        app_id: 0,
        root: game_dir.parent().map(Path::to_path_buf).unwrap_or_default(),
        game_dir: game_dir.clone(),
        branch: Some(Branch::Source2013),
        unsupported: None,
        exe: None,
        fgds: Vec::new(),
        fgd_dirs: Vec::new(),
        source_mod: false,
        mapbase: false,
        installed: false,
        lighting: LightingPasses::Both,
        skybox: "sky_day01_01".to_owned(),
    });
    p.id = cid.to_id();
    p.name = cfg.name.clone();
    if !cfg.game_exe_dir.trim().is_empty() {
        p.root = ex(&cfg.game_exe_dir);
    }
    if !cfg.game_exe.trim().is_empty() {
        p.exe = Some(ex(&cfg.game_exe));
    }
    let mut fgds: Vec<PathBuf> = Vec::new();
    for g in cfg.game_data.iter().map(|g| ex(g)) {
        if g.is_file() && !fgds.iter().any(|f| path_key(f) == path_key(&g)) {
            fgds.push(g);
        }
    }
    for g in std::mem::take(&mut p.fgds) {
        if !fgds.iter().any(|f| path_key(f) == path_key(&g)) {
            fgds.push(g);
        }
    }
    for f in &fgds {
        if let Some(dir) = f.parent()
            && !p.fgd_dirs.iter().any(|d| path_key(d) == path_key(dir))
        {
            p.fgd_dirs.push(dir.to_path_buf());
        }
    }
    p.fgds = fgds;
    p
}

type Stamp = (Option<SystemTime>, u64);

static PROFILES: LazyLock<Mutex<HashMap<String, (Stamp, Option<GameProfile>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn stamp_of(path: &Path) -> Stamp {
    std::fs::metadata(path).map(|m| (m.modified().ok(), m.len())).unwrap_or((None, 0))
}

pub fn invalidate() {
    PROFILES.lock().clear();
}

pub fn profile_of(id: &str) -> Option<GameProfile> {
    let cid = ConfigId::parse(id)?;
    let stamp = stamp_of(&cid.file);
    if let Some((s, p)) = PROFILES.lock().get(id)
        && *s == stamp
    {
        return p.clone();
    }
    let built = config_of(id).map(|(cid, _, cfg)| build_profile(&cid, &cfg));
    PROFILES.lock().insert(id.to_owned(), (stamp, built.clone()));
    built
}

const SKIP_DIRS: [&str; 14] = [
    "windows",
    "$recycle.bin",
    "system volume information",
    "$windows.~bt",
    "winsxs",
    "node_modules",
    ".git",
    "appdata",
    "programdata",
    "shadercache",
    "compatdata",
    "workshop",
    "downloading",
    "temp",
];

const MAX_DEPTH: usize = 14;

fn cheap_key(p: &Path) -> String {
    let mut k = p.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    while k.len() > 1 && k.ends_with('/') {
        k.pop();
    }
    k
}

fn drive_roots() -> Vec<PathBuf> {
    if cfg!(windows) {
        ('C'..='Z').map(|l| PathBuf::from(format!("{l}:\\"))).filter(|p| p.is_dir()).collect()
    } else {
        std::env::var_os("HOME").map(PathBuf::from).into_iter().collect()
    }
}

fn priority_roots() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(p) = std::env::var_os(var) {
            out.push(PathBuf::from(p).join("Steam"));
        }
    }
    for root in drive_roots() {
        for sub in ["Steam", "SteamLibrary", "Steam Library", "Program Files (x86)\\Steam", "Program Files\\Steam", "Games"] {
            out.push(root.join(sub));
        }
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        out.push(home.join(".steam"));
        out.push(home.join(".local").join("share").join("Steam"));
    }
    let mut seen = HashSet::new();
    out.retain(|p| p.is_dir() && seen.insert(cheap_key(p)));
    out
}

fn walk(root: &Path, skip: &HashSet<String>, cancel: &AtomicBool, on_found: &mut dyn FnMut(PathBuf)) {
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let name = entry.file_name().to_string_lossy().into_owned();
            if kind.is_file() {
                if is_config_file_name(&name) {
                    on_found(entry.path());
                }
            } else if kind.is_dir() && depth < MAX_DEPTH {
                let lower = name.to_ascii_lowercase();
                let path = entry.path();
                if SKIP_DIRS.contains(&lower.as_str()) || skip.contains(&cheap_key(&path)) {
                    continue;
                }
                stack.push((path, depth + 1));
            }
        }
    }
}

/// Looks for every `gameconfig.txt` and `hammerplusplus_gameconfig.txt` on the computer.
///
/// The Steam folders are walked first so the usual files show up at once, then every drive.
pub fn scan_computer(cancel: &AtomicBool, on_found: &mut dyn FnMut(PathBuf)) {
    let mut seen: HashSet<String> = HashSet::new();
    let mut report = |p: PathBuf| {
        if seen.insert(cheap_key(&p)) {
            on_found(p);
        }
    };
    let seeds = priority_roots();
    let none = HashSet::new();
    for seed in &seeds {
        walk(seed, &none, cancel, &mut report);
    }
    let skip: HashSet<String> = seeds.iter().map(|p| cheap_key(p)).collect();
    for root in drive_roots() {
        walk(&root, &skip, cancel, &mut report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\"Configs\"\n{\n\t\"Games\"\n\t{\n\t\t\"Test Game\"\n\t\t{\n\t\t\t\"GameDir\"\t\t\"C:\\Games\\X\\game\"\n\t\t\t\"Hammer\"\n\t\t\t{\n\t\t\t\t\"GameData0\"\t\t\"C:\\Games\\X\\bin\\a.fgd\"\n\t\t\t\t\"GameData1\"\t\t\"C:\\Games\\X\\bin\\b.fgd\"\n\t\t\t\t\"BSP\"\t\t\"C:\\Games\\X\\bin\\vbsp.exe\"\n\t\t\t\t\"Previous\"\t\t\"1\"\n\t\t\t}\n\t\t}\n\t}\n\t\"SDKVersion\"\t\t\"5\"\n}\n";

    #[test]
    fn ids_round_trip() {
        let id = ConfigId::new("C:\\a\\GameConfig.txt", "My | Game");
        assert_eq!(ConfigId::parse(&id.to_id()), Some(id));
        assert!(ConfigId::parse("gmod").is_none());
    }

    #[test]
    fn fields_round_trip_and_keep_unknown_keys() {
        let root = hp_kv::parse(SAMPLE).expect("parse");
        let entry = root.child("Configs").and_then(|c| c.child("Games")).and_then(|g| g.children.first()).expect("entry");
        let mut cfg = GameConfig::from_block(entry);
        assert_eq!(cfg.game_data.len(), 2);
        assert_eq!(cfg.bsp, "C:\\Games\\X\\bin\\vbsp.exe");
        cfg.vis = "C:\\Games\\X\\bin\\vvis.exe".to_owned();
        cfg.game_data.remove(0);
        let mut changed = entry.clone();
        cfg.apply_to(&mut changed);
        assert_eq!(changed.child("Hammer").and_then(|h| h.get("Previous")), Some("1"));
        assert_eq!(changed.child("Hammer").and_then(|h| h.get("GameData0")), Some("C:\\Games\\X\\bin\\b.fgd"));
        assert!(changed.child("Hammer").and_then(|h| h.get("GameData1")).is_none());
        let text = write_entry(&changed);
        let back = parse_entry(&text).expect("reparse");
        assert_eq!(GameConfig::from_block(&back), cfg);
        assert!(parse_entry("\"x\" {").is_err());
    }

    #[test]
    fn variables_follow_the_steam_folder() {
        let file: PathBuf = ["D:", "Steam", "steamapps", "common", "Foo", "bin", "GameConfig.txt"].iter().collect();
        let out = expand_vars("$SteamUserDir/Foo", &file);
        assert!(out.contains("steamapps") && out.ends_with("Foo"), "{out}");
        assert!(expand_vars("$SteamDir/x", &file).starts_with("D:"));
    }

    #[test]
    fn tools_come_from_the_entry() {
        let root = hp_kv::parse(SAMPLE).expect("parse");
        let entry = root.child("Configs").and_then(|c| c.child("Games")).and_then(|g| g.children.first()).expect("entry");
        let tools = build_tools_of(Path::new("C:\\Games\\X\\bin\\GameConfig.txt"), &GameConfig::from_block(entry));
        assert_eq!(tools.vbsp, Some(PathBuf::from("C:\\Games\\X\\bin\\vbsp.exe")));
        assert_eq!(tools.vvis, None);
    }
}
