use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{CompileError, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolPaths {
    pub vbsp: PathBuf,
    pub vvis: PathBuf,
    pub vrad: PathBuf,
    pub game_exe: Option<PathBuf>,
    pub game_dir: PathBuf,
    pub maps_dir: PathBuf,
}

impl ToolPaths {
    pub fn for_game_dir(game_dir: &Path) -> ToolPaths {
        let game_dir = strip_trailing_separators(game_dir);
        ToolPaths {
            vbsp: PathBuf::new(),
            vvis: PathBuf::new(),
            vrad: PathBuf::new(),
            game_exe: None,
            maps_dir: game_dir.join("maps"),
            game_dir,
        }
    }

    pub fn gmod_root(&self) -> Option<PathBuf> {
        let parent = self.game_dir.parent()?;
        (!parent.as_os_str().is_empty()).then(|| parent.to_path_buf())
    }

    pub fn build_tools(&self) -> [(&'static str, &Path); 3] {
        [("vbsp", self.vbsp.as_path()), ("vvis", self.vvis.as_path()), ("vrad", self.vrad.as_path())]
    }

    pub fn unset_tools(&self) -> Vec<&'static str> {
        self.build_tools().into_iter().filter(|(_, p)| p.as_os_str().is_empty()).map(|(n, _)| n).collect()
    }

    pub fn missing_files(&self) -> Vec<PathBuf> {
        let mut missing: Vec<PathBuf> = self
            .build_tools()
            .into_iter()
            .map(|(_, p)| p)
            .filter(|p| !p.as_os_str().is_empty() && !p.is_file())
            .map(Path::to_path_buf)
            .collect();
        let gameinfo = self.game_dir.join("gameinfo.txt");
        if !gameinfo.is_file() {
            missing.push(gameinfo);
        }
        missing
    }
}

pub fn default_threads() -> u32 {
    let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    u32::try_from(cpus.saturating_sub(1).max(1)).unwrap_or(u32::MAX)
}

pub fn pack_files_from_dir(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let io_err = |source| CompileError::Io { path: dir.clone(), source };
        for entry in std::fs::read_dir(&dir).map_err(io_err)? {
            let entry = entry.map_err(io_err)?;
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            let file_type = entry.file_type().map_err(io_err)?;
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file()
                && let Ok(rel) = path.strip_prefix(root)
            {
                let rel = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
                out.push((rel, path));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

pub(crate) fn strip_trailing_separators(path: &Path) -> PathBuf {
    let s = path.as_os_str().to_string_lossy();
    let trimmed = s.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() || trimmed.ends_with(':') {
        return path.to_path_buf();
    }
    if trimmed.len() == s.len() { path.to_path_buf() } else { PathBuf::from(trimmed) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::scratch;

    #[test]
    fn tools_are_reported_when_unset_or_missing() {
        let root = scratch("tools_missing");
        let game = root.join("game");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("gameinfo.txt"), "x").unwrap();
        let mut t = ToolPaths::for_game_dir(&game);
        assert_eq!(t.unset_tools(), vec!["vbsp", "vvis", "vrad"]);
        assert!(t.missing_files().is_empty());
        t.vbsp = root.join("vbsp.exe");
        assert_eq!(t.missing_files(), vec![root.join("vbsp.exe")]);
        std::fs::write(root.join("vbsp.exe"), b"MZ").unwrap();
        assert!(t.missing_files().is_empty());
        assert_eq!(t.maps_dir, game.join("maps"));
    }

    #[test]
    fn trailing_separators() {
        assert_eq!(strip_trailing_separators(Path::new(r"C:\a\b\")), PathBuf::from(r"C:\a\b"));
        assert_eq!(strip_trailing_separators(Path::new("/a/b//")), PathBuf::from("/a/b"));
        assert_eq!(strip_trailing_separators(Path::new(r"C:\")), PathBuf::from(r"C:\"));
        assert_eq!(strip_trailing_separators(Path::new("/")), PathBuf::from("/"));
    }

    #[test]
    fn threads() {
        assert!(default_threads() >= 1);
    }

    #[test]
    fn pack_dir_walk() {
        let root = scratch("pack_walk");
        std::fs::create_dir_all(root.join("materials/hp")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("materials/hp/a.vmt"), "x").unwrap();
        std::fs::write(root.join("materials/hp/a.vtf"), "x").unwrap();
        std::fs::write(root.join(".git/config"), "x").unwrap();
        let files = pack_files_from_dir(&root).unwrap();
        let names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(names, vec!["materials/hp/a.vmt", "materials/hp/a.vtf"]);
        assert!(files.iter().all(|f| f.1.is_file()));
        assert!(pack_files_from_dir(&root.join("nope")).is_err());
    }
}
