use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

pub const MAX_FILE_BYTES: u64 = 1 << 20;
pub const MAX_FILES: usize = 512;
const MAX_TYPES_PER_FILE: usize = 256;
const MAX_ENTRIES: usize = 1024;
const MAX_KEYS: usize = 128;
pub const DEFAULT_GRID: f64 = 64.0;
pub const GAME_DIR: &str = "scripts/hammer/sprinkle";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SprinkleSource {
    Disk(PathBuf),
    Game { path: String, disk: Option<PathBuf> },
}

impl SprinkleSource {
    pub fn disk_path(&self) -> Option<&Path> {
        match self {
            SprinkleSource::Disk(p) => Some(p),
            SprinkleSource::Game { disk, .. } => disk.as_deref(),
        }
    }

    pub fn label(&self) -> String {
        match self {
            SprinkleSource::Disk(p) => p.display().to_string(),
            SprinkleSource::Game { path, .. } => path.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SprinkleEntry {
    pub weight: f64,
    pub keyvalues: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SprinkleType {
    pub name: String,
    pub source: SprinkleSource,
    pub base: Vec<(String, String)>,
    pub grid: Option<[f64; 2]>,
    pub entries: Vec<SprinkleEntry>,
    classes: HashSet<String>,
    models: HashSet<String>,
    total_weight: f64,
}

fn reserved(key: &str) -> bool {
    ["grid", "origin", "id"].iter().any(|r| key.eq_ignore_ascii_case(r))
}

fn set_kv(kv: &mut Vec<(String, String)>, key: &str, value: &str) {
    match kv.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
        Some(slot) => value.clone_into(&mut slot.1),
        None => kv.push((key.to_owned(), value.to_owned())),
    }
}

fn model_key(m: &str) -> String {
    m.trim().replace('\\', "/").to_ascii_lowercase()
}

fn parse_grid(text: &str) -> Option<[f64; 2]> {
    let nums: Vec<f64> = text.split_ascii_whitespace().take(3).filter_map(|t| t.parse().ok()).collect();
    let ok = |v: f64| v.is_finite() && (1.0..=16384.0).contains(&v);
    match nums[..] {
        [a] if ok(a) => Some([a, a]),
        [a, b, ..] if ok(a) && ok(b) => Some([a, b]),
        _ => None,
    }
}

impl SprinkleType {
    fn new(name: String, source: SprinkleSource, base: Vec<(String, String)>, grid: Option<[f64; 2]>) -> Self {
        Self {
            name,
            source,
            base,
            grid,
            entries: Vec::new(),
            classes: HashSet::new(),
            models: HashSet::new(),
            total_weight: 0.0,
        }
    }

    fn finish(mut self) -> Option<Self> {
        self.entries.retain(|e| e.weight.is_finite() && e.weight > 0.0);
        if self.entries.is_empty() {
            return None;
        }
        self.total_weight = self.entries.iter().map(|e| e.weight).sum();
        for i in 0..self.entries.len() {
            let kv = self.entity_keyvalues(i);
            let class = kv.iter().find(|(k, _)| k.eq_ignore_ascii_case("classname")).map(|(_, v)| v.trim());
            if let Some(c) = class.filter(|c| !c.is_empty()) {
                self.classes.insert(c.to_ascii_lowercase());
            }
            if let Some((_, m)) = kv.iter().find(|(k, _)| k.eq_ignore_ascii_case("model")) {
                self.models.insert(model_key(m));
            }
        }
        (!self.classes.is_empty() && self.total_weight.is_finite()).then_some(self)
    }

    pub fn entity_keyvalues(&self, index: usize) -> Vec<(String, String)> {
        let mut kv: Vec<(String, String)> = Vec::with_capacity(self.base.len() + 4);
        let entry = self.entries.get(index).map(|e| e.keyvalues.as_slice()).unwrap_or(&[]);
        for (k, v) in self.base.iter().chain(entry) {
            if !reserved(k) {
                set_kv(&mut kv, k, v);
            }
        }
        kv
    }

    pub fn pick(&self, r: f64) -> Option<usize> {
        if self.entries.is_empty() {
            return None;
        }
        let r = if r.is_finite() { r.clamp(0.0, 1.0) } else { 0.0 };
        let mut target = r * self.total_weight;
        for (i, e) in self.entries.iter().enumerate() {
            if target < e.weight {
                return Some(i);
            }
            target -= e.weight;
        }
        Some(self.entries.len() - 1)
    }

    pub fn grid_or_default(&self) -> [f64; 2] {
        self.grid.unwrap_or([DEFAULT_GRID, DEFAULT_GRID])
    }

    pub fn owns(&self, entity: &hp_vmf::Entity) -> bool {
        if !self.classes.contains(&entity.classname.trim().to_ascii_lowercase()) {
            return false;
        }
        self.models.is_empty() || entity.get("model").is_some_and(|m| self.models.contains(&model_key(m)))
    }

    pub fn model_count(&self) -> usize {
        self.models.len()
    }
}

pub fn parse_file(text: &str, source: &SprinkleSource) -> (Vec<SprinkleType>, Vec<String>) {
    let (root, _) = hp_kv::parse_with_issues(text);
    let mut types = Vec::new();
    let mut warnings = Vec::new();
    for block in root.children.iter().take(MAX_TYPES_PER_FILE) {
        let name = block.name.trim();
        if name.is_empty() {
            continue;
        }
        let mut base: Vec<(String, String)> = Vec::new();
        let mut grid = block.get("grid").and_then(parse_grid);
        for b in block.children.iter().filter(|b| b.name.trim().eq_ignore_ascii_case("base")) {
            for (k, v) in b.props.iter().take(MAX_KEYS) {
                if k.eq_ignore_ascii_case("grid") {
                    grid = parse_grid(v).or(grid);
                } else if !reserved(k) {
                    set_kv(&mut base, k, v);
                }
            }
        }
        for (k, v) in block.props.iter().take(MAX_KEYS) {
            if !reserved(k) {
                set_kv(&mut base, k, v);
            }
        }
        let mut t = SprinkleType::new(name.to_owned(), source.clone(), base, grid);
        for b in block.children.iter().filter(|b| !b.name.trim().eq_ignore_ascii_case("base")).take(MAX_ENTRIES) {
            let Ok(weight) = b.name.trim().parse::<f64>() else {
                warnings.push(format!("{}: {name}: entry \"{}\" has no numeric weight", source.label(), b.name));
                continue;
            };
            let keyvalues: Vec<(String, String)> = b.props.iter().take(MAX_KEYS).cloned().collect();
            t.entries.push(SprinkleEntry { weight, keyvalues });
        }
        match t.finish() {
            Some(t) => types.push(t),
            None => warnings.push(format!("{}: {name}: no usable entry", source.label())),
        }
    }
    (types, warnings)
}

#[derive(Clone, Debug, Default)]
pub struct SprinkleLibrary {
    pub types: Vec<Arc<SprinkleType>>,
    pub warnings: Vec<String>,
    pub files: usize,
}

impl SprinkleLibrary {
    pub fn find(&self, name: &str) -> Option<&Arc<SprinkleType>> {
        let name = name.trim();
        self.types.iter().find(|t| t.name.eq_ignore_ascii_case(name))
    }
}

type CachedFile = (Option<(SystemTime, u64)>, u64, Vec<SprinkleType>, Vec<String>);

#[derive(Debug, Default)]
pub struct ScanCache {
    files: HashMap<SprinkleSource, CachedFile>,
}

#[derive(Clone, Debug, Default)]
pub struct ScanSources {
    pub fs: Option<hp_fs::GameFs>,
    pub dirs: Vec<PathBuf>,
}

pub fn default_dirs(gmod_root: Option<&Path>, user_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(root) = gmod_root {
        let bin = root.join("bin");
        dirs.push(bin.join("hammerplusplus").join("scripts").join("sprinkle"));
        dirs.push(bin.join("win64").join("hammerplusplus").join("scripts").join("sprinkle"));
        dirs.push(bin.join("scripts").join("hammer").join("sprinkle"));
        dirs.push(bin.join("win64").join("scripts").join("hammer").join("sprinkle"));
    }
    dirs.push(user_dir.to_path_buf());
    dirs
}

fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

fn max_depth(text: &str) -> usize {
    let b = text.as_bytes();
    let (mut i, mut depth, mut max) = (0usize, 0usize, 0usize);
    while i < b.len() {
        match b[i] {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' => {
                depth += 1;
                max = max.max(depth);
            }
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        i += 1;
    }
    max
}

const MAX_DEPTH: usize = 16;

fn parse_checked(text: &str, source: &SprinkleSource) -> (Vec<SprinkleType>, Vec<String>) {
    if max_depth(text) > MAX_DEPTH {
        return (Vec::new(), vec![format!("{}: blocks nested too deeply", source.label())]);
    }
    parse_file(text, source)
}

struct Scanner<'a> {
    cache: &'a mut ScanCache,
    seen: HashSet<SprinkleSource>,
    lib: SprinkleLibrary,
}

impl Scanner<'_> {
    fn add(
        &mut self,
        source: SprinkleSource,
        stamp: Option<(SystemTime, u64)>,
        generation: u64,
        read: impl FnOnce() -> Option<String>,
    ) {
        if !self.seen.insert(source.clone()) {
            return;
        }
        let cached =
            self.cache.files.get(&source).filter(|(s, g, _, _)| stamp.is_some() && *s == stamp && *g == generation);
        let (types, warnings) = match cached {
            Some((_, _, t, w)) => (t.clone(), w.clone()),
            None => {
                let Some(text) = read() else { return };
                let (t, w) = parse_checked(&text, &source);
                self.cache.files.insert(source, (stamp, generation, t.clone(), w.clone()));
                (t, w)
            }
        };
        self.lib.files += 1;
        self.lib.types.extend(types.into_iter().map(Arc::new));
        self.lib.warnings.extend(warnings);
    }
}

pub fn scan(sources: &ScanSources, cache: &mut ScanCache) -> SprinkleLibrary {
    let mut s = Scanner { cache, seen: HashSet::new(), lib: SprinkleLibrary::default() };
    for dir in &sources.dirs {
        let Ok(read_dir) = std::fs::read_dir(dir) else { continue };
        let mut files: Vec<PathBuf> = read_dir
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("txt")))
            .take(MAX_FILES)
            .collect();
        files.sort();
        for path in files {
            let st = stamp(&path);
            if st.is_some_and(|(_, len)| len > MAX_FILE_BYTES) {
                s.lib.warnings.push(format!("{}: file too large", path.display()));
                continue;
            }
            let p = path.clone();
            s.add(SprinkleSource::Disk(path), st, 0, move || {
                std::fs::read(&p).ok().map(|b| hp_kv::decode(&b).0.into_owned())
            });
        }
    }
    if let Some(fs) = &sources.fs {
        let generation = fs.generation();
        for entry in fs.list(GAME_DIR, Some("txt")).into_iter().take(MAX_FILES) {
            let disk = fs.disk_path(&entry.name);
            if disk.as_ref().is_some_and(|d| s.seen.contains(&SprinkleSource::Disk(d.clone()))) {
                continue;
            }
            let st = match &disk {
                Some(d) => stamp(d),
                None => Some((SystemTime::UNIX_EPOCH, 0)),
            };
            let name = entry.name.clone();
            s.add(SprinkleSource::Game { path: entry.name, disk }, st, generation, || {
                fs.read(&name).filter(|b| b.len() as u64 <= MAX_FILE_BYTES).map(|b| hp_kv::decode(&b).0.into_owned())
            });
        }
    }
    let mut lib = s.lib;
    lib.types.sort_by_key(|t| t.name.to_ascii_lowercase());
    lib
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#""Example: Jungle Foliage" //This is what shows up in Type
{
	base //The base values
	{
		"classname" "prop_static"
		grid "64 64" // The default grid size
	}
	10 //chance
	{
		"model" "models/props_island/island_plant01.mdl"
		"ignorenormals" "1"
	}
	30
	{
		"model" "models/props_island/island_plant02.mdl"
		"skin" "1"
	}
	junk { "model" "x.mdl" }
	0 { "model" "never.mdl" }
}
"Rocks" { base { "classname" "prop_static" } 1 { "model" "models/rock.mdl" "classname" "prop_physics" } }
"Empty" { base { "classname" "info_target" } }
"#;

    fn src() -> SprinkleSource {
        SprinkleSource::Disk(PathBuf::from("example.txt"))
    }

    #[test]
    fn parses_hammer_definitions() {
        let (types, warnings) = parse_file(EXAMPLE, &src());
        assert_eq!(types.len(), 2, "{warnings:?}");
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        let t = &types[0];
        assert_eq!(t.name, "Example: Jungle Foliage");
        assert_eq!(t.grid, Some([64.0, 64.0]));
        assert_eq!(t.entries.len(), 2, "zero weights are dropped");
        let kv = t.entity_keyvalues(1);
        assert_eq!(
            kv,
            vec![
                ("classname".to_owned(), "prop_static".to_owned()),
                ("model".to_owned(), "models/props_island/island_plant02.mdl".to_owned()),
                ("skin".to_owned(), "1".to_owned()),
            ]
        );
        assert_eq!(t.pick(0.0), Some(0));
        assert_eq!(t.pick(0.249), Some(0));
        assert_eq!(t.pick(0.26), Some(1));
        assert_eq!(t.pick(1.0), Some(1));
        assert_eq!(t.pick(f64::NAN), Some(0));
        assert_eq!(t.model_count(), 2);
        let rocks = &types[1];
        assert_eq!(rocks.grid_or_default(), [DEFAULT_GRID, DEFAULT_GRID]);
        let mut e = hp_vmf::Entity { classname: "prop_physics".to_owned(), ..Default::default() };
        e.set("model", "Models\\Rock.mdl");
        assert!(rocks.owns(&e));
        e.set("model", "models/other.mdl");
        assert!(!rocks.owns(&e));
        e.classname = "prop_static".to_owned();
        assert!(!rocks.owns(&e), "only the classes of the entries count");
    }

    #[test]
    fn junk_never_panics() {
        for text in [
            "",
            "{",
            "}}}}",
            "\"a\" { 1 { \"classname\" } }",
            "\"a\" { -5 { \"classname\" \"x\" } }",
            "a { 1e400 { classname x } }",
        ] {
            let (types, _) = parse_file(text, &src());
            assert!(types.iter().all(|t| t.total_weight.is_finite()));
        }
        assert_eq!(parse_grid("0 0"), None);
        assert_eq!(parse_grid("32"), Some([32.0, 32.0]));
        assert_eq!(parse_grid("nan 3"), None);
        let deep = "a {".repeat(100_000);
        let (types, warnings) = parse_checked(&deep, &src());
        assert!(types.is_empty() && warnings.len() == 1);
        assert_eq!(max_depth("a { \"{\" b { } // {{{{\n }"), 2);
    }

    #[test]
    fn scans_folders_with_a_cache() {
        let dir = std::env::temp_dir().join(format!("hp_sprinkle_scan_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("a.txt"), EXAMPLE).expect("write");
        std::fs::write(dir.join("b.TXT"), "\"Zeta\" { 1 { \"classname\" \"info_target\" } }").expect("write");
        std::fs::write(dir.join("c.cfg"), "\"Ignored\" { 1 { \"classname\" \"x\" } }").expect("write");
        let sources = ScanSources { fs: None, dirs: vec![dir.clone(), dir.join("missing")] };
        let mut cache = ScanCache::default();
        let lib = scan(&sources, &mut cache);
        let names: Vec<&str> = lib.types.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Example: Jungle Foliage", "Rocks", "Zeta"]);
        assert_eq!(lib.files, 2);
        assert!(lib.find("rocks").is_some());
        let again = scan(&sources, &mut cache);
        assert_eq!(again.types.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
