use std::path::{Path, PathBuf};

use hp_app::runtime::{FileStamp, file_stamp};
use hp_app::settings::Settings;

pub(crate) const MAX_FILES: usize = 20_000;
const MAX_DEPTH: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PrefabRoot {
    pub path: PathBuf,
    pub label_key: &'static str,
    pub writable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PrefabEntry {
    pub path: PathBuf,
    pub name: String,
    pub root: usize,
    pub folder: String,
    pub stamp: FileStamp,
    pub search_key: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Library {
    pub roots: Vec<PrefabRoot>,
    pub entries: Vec<PrefabEntry>,
    pub categories: Vec<(usize, String)>,
    pub truncated: bool,
}

pub(crate) fn roots(settings: &Settings) -> Vec<PrefabRoot> {
    let p = &settings.placement;
    let (main, label_key) = match hp_app::games::config_prefab_dir(settings) {
        Some(dir) => (dir, "prefabs.root.config"),
        None => (p.library_dir(), "prefabs.root.library"),
    };
    let mut out = vec![PrefabRoot { path: main, label_key, writable: true }];
    for d in &p.prefab_dirs {
        out.push(PrefabRoot { path: d.clone(), label_key: "prefabs.root.extra", writable: false });
    }
    if p.game_prefabs
        && let Some(root) = &settings.game.gmod_root
    {
        let bin = root.join("bin");
        for dir in [bin.join("prefabs"), bin.join("win64").join("prefabs")] {
            out.push(PrefabRoot { path: dir, label_key: "prefabs.root.game", writable: false });
        }
    }
    let mut seen: Vec<String> = Vec::new();
    out.retain(|r| {
        let key = r.path.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_ascii_lowercase();
        if seen.contains(&key) {
            false
        } else {
            seen.push(key);
            true
        }
    });
    out
}

fn skipped(name: &str) -> bool {
    name.starts_with('.') || name.ends_with(".tmp")
}

fn walk(root: &Path, index: usize, out: &mut Vec<PrefabEntry>, truncated: &mut bool) {
    let mut stack: Vec<(PathBuf, String, usize)> = vec![(root.to_path_buf(), String::new(), 0)];
    while let Some((dir, folder, depth)) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            if out.len() >= MAX_FILES {
                *truncated = true;
                return;
            }
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if skipped(&name) {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                if depth < MAX_DEPTH {
                    let sub = if folder.is_empty() { name } else { format!("{folder}/{name}") };
                    stack.push((path, sub, depth + 1));
                }
                continue;
            }
            let is_vmf = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("vmf"));
            if !kind.is_file() || !is_vmf {
                continue;
            }
            let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let search_key = format!("{} {}", stem.to_lowercase(), folder.to_lowercase());
            out.push(PrefabEntry {
                stamp: file_stamp(&path),
                path,
                name: stem,
                root: index,
                folder: folder.clone(),
                search_key,
            });
        }
    }
}

pub(crate) fn scan(roots: Vec<PrefabRoot>) -> Library {
    let mut entries = Vec::new();
    let mut truncated = false;
    for (i, r) in roots.iter().enumerate() {
        walk(&r.path, i, &mut entries, &mut truncated);
    }
    entries.sort_by(|a, b| {
        (a.root, a.folder.to_lowercase(), a.name.to_lowercase()).cmp(&(
            b.root,
            b.folder.to_lowercase(),
            b.name.to_lowercase(),
        ))
    });
    let mut categories: Vec<(usize, String)> = entries.iter().map(|e| (e.root, e.folder.clone())).collect();
    categories.dedup();
    Library { roots, entries, categories, truncated }
}

pub(crate) fn subfolders(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack: Vec<(PathBuf, String, usize)> = vec![(root.to_path_buf(), String::new(), 0)];
    while let Some((dir, rel, depth)) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            if out.len() >= 1000 {
                break;
            }
            let Ok(kind) = entry.file_type() else { continue };
            let name = entry.file_name().to_string_lossy().into_owned();
            if !kind.is_dir() || kind.is_symlink() || skipped(&name) {
                continue;
            }
            let sub = if rel.is_empty() { name } else { format!("{rel}/{name}") };
            if depth < MAX_DEPTH {
                stack.push((entry.path(), sub.clone(), depth + 1));
            }
            out.push(sub);
        }
    }
    out.sort_by_key(|s| s.to_lowercase());
    out
}

const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2",
    "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

pub(crate) fn valid_component(name: &str) -> bool {
    if name.is_empty() || name.chars().count() > 80 || name != name.trim() {
        return false;
    }
    if name.chars().any(|c| c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')) {
        return false;
    }
    if name.ends_with('.') || name == "." || name == ".." || name.starts_with('.') {
        return false;
    }
    let stem = name.split('.').next().unwrap_or(name).to_ascii_lowercase();
    !RESERVED.contains(&stem.as_str())
}

pub(crate) fn folder_components(folder: &str) -> Option<Vec<String>> {
    let parts: Vec<&str> = folder.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    if parts.len() > MAX_DEPTH {
        return None;
    }
    parts.iter().all(|p| valid_component(p)).then(|| parts.iter().map(|p| (*p).to_owned()).collect())
}

pub(crate) fn prefab_path(root: &Path, folder: &str, name: &str) -> Option<PathBuf> {
    let name = name.trim();
    let name = name.strip_suffix(".vmf").or_else(|| name.strip_suffix(".VMF")).unwrap_or(name);
    if !valid_component(name) {
        return None;
    }
    let mut path = root.to_path_buf();
    for c in folder_components(folder)? {
        path.push(c);
    }
    path.push(format!("{name}.vmf"));
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_safe() {
        for good in ["door", "Door 01", "porte_été", "a.b"] {
            assert!(valid_component(good), "{good}");
        }
        for bad in ["", " x", "x ", "..", ".", ".hidden", "a/b", "a\\b", "c:", "con", "COM1.vmf", "x.", "a*", "a\u{1}"]
        {
            assert!(!valid_component(bad), "{bad}");
        }
        let root = Path::new("lib");
        assert_eq!(prefab_path(root, "doors/metal", "big.vmf"), Some(PathBuf::from("lib/doors/metal/big.vmf")));
        assert_eq!(prefab_path(root, "", "big"), Some(PathBuf::from("lib/big.vmf")));
        assert_eq!(prefab_path(root, "../up", "x"), None);
        assert_eq!(prefab_path(root, "a", "../x"), None);
        assert_eq!(prefab_path(root, "a", "nul"), None);
        assert_eq!(folder_components("a\\b//c"), Some(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]));
    }

    #[test]
    fn scans_folders_and_categories() {
        let dir = crate::test_scratch("prefab_library");
        let lib_dir = dir.join("lib");
        std::fs::create_dir_all(lib_dir.join("Doors/Metal")).expect("dirs");
        std::fs::create_dir_all(lib_dir.join(".git")).expect("dirs");
        std::fs::write(lib_dir.join("crate.vmf"), "x").expect("w");
        std::fs::write(lib_dir.join("Doors/Metal/big.VMF"), "x").expect("w");
        std::fs::write(lib_dir.join("Doors/wood.vmf"), "x").expect("w");
        std::fs::write(lib_dir.join("Doors/wood.vmx"), "x").expect("w");
        std::fs::write(lib_dir.join(".prefab.vmf.1.tmp"), "x").expect("w");
        std::fs::write(lib_dir.join(".git/ignored.vmf"), "x").expect("w");
        let roots = vec![
            PrefabRoot { path: lib_dir.clone(), label_key: "prefabs.root.library", writable: true },
            PrefabRoot { path: dir.join("missing"), label_key: "prefabs.root.extra", writable: false },
        ];
        let lib = scan(roots);
        let names: Vec<(&str, &str)> = lib.entries.iter().map(|e| (e.folder.as_str(), e.name.as_str())).collect();
        assert_eq!(names, [("", "crate"), ("Doors", "wood"), ("Doors/Metal", "big")]);
        assert_eq!(lib.categories.len(), 3);
        assert!(!lib.truncated);
        assert!(lib.entries.iter().all(|e| e.stamp.is_some()));
        assert_eq!(subfolders(&lib_dir), ["Doors", "Doors/Metal"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn roots_follow_the_settings() {
        let mut s = Settings::default();
        s.game.gmod_root = Some(PathBuf::from("C:/Games/GarrysMod"));
        s.placement.prefab_dirs = vec![PathBuf::from("D:/team/prefabs"), PathBuf::from("d:\\TEAM\\prefabs\\")];
        let r = roots(&s);
        assert_eq!(r.len(), 4, "{r:?}");
        assert!(r[0].writable && !r[1].writable);
        s.placement.game_prefabs = false;
        assert_eq!(roots(&s).len(), 2);
    }
}
