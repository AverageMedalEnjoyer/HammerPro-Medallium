use std::path::{Path, PathBuf};

pub const GMOD_APP_ID: u32 = 4000;

pub(crate) fn path_key(path: &Path) -> String {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut key = canonical.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        key.make_ascii_lowercase();
        if let Some(stripped) = key.strip_prefix("//?/") {
            key = stripped.to_owned();
        }
    }
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }
    key
}

pub(crate) fn is_gmod_root(path: &Path) -> bool {
    path.join("garrysmod").is_dir()
}

pub fn detect_workshop_dir(gmod_root: &Path) -> Option<PathBuf> {
    let steamapps = gmod_root.parent()?.parent()?;
    let dir = steamapps.join("workshop").join("content").join(GMOD_APP_ID.to_string());
    dir.is_dir().then_some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workshop_dir_requires_layout() {
        assert_eq!(detect_workshop_dir(Path::new("/")), None);
    }
}
