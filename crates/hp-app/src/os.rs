use std::path::{Path, PathBuf};

#[cfg(windows)]
fn windows_program(system32: bool, name: &str) -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os("SystemRoot")?);
    let path = if system32 { root.join("System32").join(name) } else { root.join(name) };
    path.is_file().then_some(path)
}

pub fn file_manager() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        windows_program(false, "explorer.exe")
    }
    #[cfg(target_os = "macos")]
    {
        Some(PathBuf::from("/usr/bin/open")).filter(|p| p.is_file())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        ["/usr/bin/xdg-open", "/bin/xdg-open", "/usr/local/bin/xdg-open"]
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
    }
}

pub fn open_folder(path: &Path, create: bool) {
    if create {
        let _ = std::fs::create_dir_all(path);
    }
    if !path.is_dir() {
        return;
    }
    let Some(program) = file_manager() else {
        log::warn!("no file manager found to open {}", path.display());
        return;
    };
    if let Err(e) = std::process::Command::new(program).arg(path).spawn() {
        log::warn!("cannot open {}: {e}", path.display());
    }
}

pub fn reveal(path: &Path) -> std::io::Result<()> {
    let program = file_manager().ok_or_else(|| std::io::Error::other("no file manager found"))?;
    let mut cmd = std::process::Command::new(program);
    if path.is_dir() {
        cmd.arg(path);
    } else if cfg!(windows) {
        cmd.arg(format!("/select,{}", path.to_string_lossy().replace('/', "\\")));
    } else if cfg!(target_os = "macos") {
        cmd.arg("-R").arg(path);
    } else {
        cmd.arg(path.parent().unwrap_or(path));
    }
    cmd.spawn().map(drop)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_file_manager_is_a_full_path() {
        if let Some(p) = super::file_manager() {
            assert!(p.is_absolute() && p.is_file(), "{}", p.display());
        }
    }
}
