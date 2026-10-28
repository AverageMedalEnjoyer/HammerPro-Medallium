use std::fs;
use std::path::{Path, PathBuf};

use ahash::AHashMap;
use rayon::prelude::*;

use crate::error::FsError;
use crate::path::{normalize_path, under_prefix};

pub(crate) const ASSET_DIRS: &[&str] =
    &["materials", "models", "sound", "particles", "maps", "resource", "scripts", "scenes"];

const MAX_DEPTH: u32 = 48;

const MAX_LINKS: u32 = 4;

pub(crate) fn walk_files(
    root: &Path,
    top_dirs: Option<&[&str]>,
    keep_file: &(dyn Fn(&str) -> bool + Sync),
) -> Result<Vec<String>, FsError> {
    let read = fs::read_dir(root).map_err(|e| FsError::io(root, e))?;
    let walker = Walker { top_dirs, keep_file };
    let mut files = walker.walk_entries(read, "", 0, 0);
    files.par_sort_unstable();
    Ok(files)
}

enum Kind {
    Dir { link: bool },
    File,
}

fn classify(entry: &fs::DirEntry) -> Option<(String, Kind)> {
    let name = entry.file_name().into_string().ok()?;
    let file_type = entry.file_type().ok()?;
    let link = file_type.is_symlink();
    let (is_dir, is_file) = if link {
        let meta = fs::metadata(entry.path()).ok()?;
        (meta.is_dir(), meta.is_file())
    } else {
        (file_type.is_dir(), file_type.is_file())
    };
    if is_dir {
        Some((name, Kind::Dir { link }))
    } else if is_file {
        Some((name, Kind::File))
    } else {
        None
    }
}

struct Walker<'a> {
    top_dirs: Option<&'a [&'a str]>,
    keep_file: &'a (dyn Fn(&str) -> bool + Sync),
}

impl Walker<'_> {
    fn walk(&self, abs: &Path, rel: &str, depth: u32, links: u32) -> Vec<String> {
        match fs::read_dir(abs) {
            Ok(read) => self.walk_entries(read, rel, depth, links),
            Err(e) => {
                log::debug!("cannot list {}: {e}", abs.display());
                Vec::new()
            }
        }
    }

    fn walk_entries(&self, read: fs::ReadDir, rel: &str, depth: u32, links: u32) -> Vec<String> {
        let at_root = depth == 0;
        let join = |name: &str| if at_root { name.to_owned() } else { format!("{rel}/{name}") };
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        for entry in read.flatten() {
            let Some((name, kind)) = classify(&entry) else {
                continue;
            };
            match kind {
                Kind::Dir { link } => {
                    let wanted = !at_root
                        || self.top_dirs.is_none_or(|allowed| allowed.iter().any(|a| a.eq_ignore_ascii_case(&name)));
                    let links = links + u32::from(link);
                    if wanted && depth < MAX_DEPTH && links <= MAX_LINKS && !name.starts_with('.') {
                        dirs.push((entry.path(), join(&name), links));
                    }
                }
                Kind::File => {
                    if (!at_root || self.top_dirs.is_none()) && (self.keep_file)(&name) {
                        files.push(join(&name));
                    }
                }
            }
        }
        match dirs.as_slice() {
            [] => {}
            [(path, child, links)] => files.extend(self.walk(path, child, depth + 1, *links)),
            _ => {
                let nested: Vec<Vec<String>> =
                    dirs.par_iter().map(|(path, child, links)| self.walk(path, child, depth + 1, *links)).collect();
                files.extend(nested.into_iter().flatten());
            }
        }
        files
    }
}

#[derive(Debug)]
pub(crate) struct DirIndex {
    root: PathBuf,
    files: AHashMap<Box<str>, Box<str>>,
}

impl DirIndex {
    pub(crate) fn build_game_dir(root: &Path) -> Result<Self, FsError> {
        let rels = walk_files(root, Some(ASSET_DIRS), &|_| true)?;
        Ok(Self::from_rels(root, rels))
    }

    pub(crate) fn from_rels(root: &Path, rels: Vec<String>) -> Self {
        let mut files = AHashMap::with_capacity(rels.len());
        for rel in rels {
            let key: Box<str> = normalize_path(&rel).into();
            files.entry(key).or_insert_with(|| rel.into_boxed_str());
        }
        Self { root: root.to_path_buf(), files }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn len(&self) -> usize {
        self.files.len()
    }

    #[inline]
    pub(crate) fn contains(&self, vpath: &str) -> bool {
        self.files.contains_key(vpath)
    }

    pub(crate) fn disk_path(&self, vpath: &str) -> Option<PathBuf> {
        self.files.get(vpath).map(|rel| self.root.join(&**rel))
    }

    pub(crate) fn for_each_path<'a>(&'a self, prefix: &str, ext: Option<&str>, f: &mut dyn FnMut(&'a str)) {
        for key in self.files.keys() {
            if under_prefix(key, prefix) && ext.is_none_or(|e| crate::path::extension(key) == e) {
                f(key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempDir;

    #[test]
    fn indexes_asset_dirs_case_insensitively() {
        let tmp = TempDir::new("dirindex");
        tmp.write("Materials/Concrete/Floor01.VMT", b"x");
        tmp.write("materials/concrete/sub/a.vtf", b"y");
        tmp.write("models/props/crate.mdl", b"m");
        tmp.write("lua/autorun/x.lua", b"l");
        tmp.write("readme.txt", b"r");
        let index = DirIndex::build_game_dir(tmp.path()).unwrap();
        assert_eq!(index.len(), 3);
        assert!(index.contains("materials/concrete/floor01.vmt"));
        assert!(index.contains("materials/concrete/sub/a.vtf"));
        assert!(!index.contains("lua/autorun/x.lua"));
        assert!(!index.contains("readme.txt"));
        let disk = index.disk_path("materials/concrete/floor01.vmt").unwrap();
        assert_eq!(fs::read(disk).unwrap(), b"x");
        let mut vmts = Vec::new();
        index.for_each_path("materials", Some("vmt"), &mut |p| vmts.push(p.to_owned()));
        assert_eq!(vmts, ["materials/concrete/floor01.vmt"]);
        let mut all = Vec::new();
        index.for_each_path("", None, &mut |p| all.push(p));
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn walk_filters_files_and_reports_missing_root() {
        let tmp = TempDir::new("walk");
        tmp.write("a.png", b"");
        tmp.write("b.txt", b"");
        tmp.write("x/y/z.png", b"");
        let files = walk_files(tmp.path(), None, &|n| n.ends_with(".png")).unwrap();
        assert_eq!(files, ["a.png", "x/y/z.png"]);
        assert!(walk_files(&tmp.path().join("missing"), None, &|_| true).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn link_loops_are_bounded() {
        let tmp = TempDir::new("walk-loop");
        tmp.write("d/a.vmt", b"");
        std::os::unix::fs::symlink(tmp.path().join("d"), tmp.path().join("d/l1")).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("d"), tmp.path().join("d/l2")).unwrap();
        let files = walk_files(tmp.path(), None, &|_| true).unwrap();
        assert!(files.contains(&"d/a.vmt".to_owned()));
        assert!(files.len() < 64, "{}", files.len());
    }
}
