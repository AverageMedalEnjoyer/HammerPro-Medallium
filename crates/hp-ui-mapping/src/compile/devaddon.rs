use std::path::{Component, Path, PathBuf};

pub(crate) const DEV_ADDON: &str = "hammerpro_dev";

pub(crate) fn dev_addon_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("addons").join(DEV_ADDON)
}

pub(crate) fn is_safe_material(name: &str) -> bool {
    !name.is_empty()
        && name.len() < 260
        && !name.chars().any(|c| c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        && Path::new(name).components().all(|c| matches!(c, Component::Normal(_)))
        && !name.starts_with(['/', '\\'])
}

#[derive(Debug, Default)]
pub(crate) struct DevAddonReport {
    pub written: usize,
    pub failed: Vec<(String, String)>,
}

pub(crate) fn export(fs: &hp_fs::GameFs, game_dir: &Path, materials: &[String]) -> Result<DevAddonReport, String> {
    if hp_compile::sandbox::is_sandboxed() {
        return Err(hp_compile::sandbox::REFUSED.to_owned());
    }
    let dir = dev_addon_dir(game_dir);
    if !game_dir.is_dir() {
        return Err(format!("{}: not a folder", game_dir.display()));
    }
    let safe: Vec<String> = materials.iter().filter(|m| is_safe_material(m)).cloned().collect();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let report = fs.export_custom_materials(&safe, &dir).map_err(|e| e.to_string())?;
    if report.written.is_empty() && report.up_to_date.is_empty() && report.failed.is_empty() {
        let _ = std::fs::remove_dir(&dir);
    }
    Ok(DevAddonReport {
        written: report.written.len(),
        failed: report.failed.into_iter().map(|f| (f.path, f.error)).collect(),
    })
}

pub(crate) fn export_switch_lua(game_dir: &Path) -> Result<bool, String> {
    if hp_compile::sandbox::is_sandboxed() {
        return Err(hp_compile::sandbox::REFUSED.to_owned());
    }
    if !game_dir.is_dir() {
        return Err(format!("{}: not a folder", game_dir.display()));
    }
    let dir = dev_addon_dir(game_dir);
    let mut wrote = false;
    for (vpath, content) in hp_editor::sky::switch_lua_files() {
        let path = crate::release::files::safe_join(&dir, vpath).ok_or_else(|| "bad Lua path".to_owned())?;
        if std::fs::read(&path).is_ok_and(|old| old == content.as_bytes()) {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let tmp = path.with_extension("lua.tmp");
        std::fs::write(&tmp, content).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("{}: {e}", path.display())
        })?;
        wrote = true;
    }
    Ok(wrote)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_names_cannot_escape() {
        assert!(is_safe_material("custom/brick"));
        assert!(is_safe_material("a/b/c_d-e"));
        for bad in ["", "../evil", "a/../../b", "/abs", "\\abs", "C:/x", "a\u{0}b", "a?b", "c:x"] {
            assert!(!is_safe_material(bad), "{bad:?}");
        }
    }

    fn tga(w: u16, h: u16) -> Vec<u8> {
        let mut v = vec![0u8, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        v.extend_from_slice(&w.to_le_bytes());
        v.extend_from_slice(&h.to_le_bytes());
        v.extend_from_slice(&[24, 0]);
        v.resize(v.len() + usize::from(w) * usize::from(h) * 3, 128);
        v
    }

    #[test]
    fn the_skybox_switch_script_goes_to_the_dev_addon() {
        let dir = crate::test_scratch("dev_addon_switch");
        let game = dir.join("garrysmod");
        assert!(export_switch_lua(&game).is_err(), "no game folder");
        std::fs::create_dir_all(&game).expect("game");
        assert_eq!(export_switch_lua(&game), Ok(true));
        let addon = dev_addon_dir(&game);
        assert_eq!(
            std::fs::read_to_string(addon.join("lua/entities/point_hammerpro_skybox.lua")).expect("ent lua"),
            hp_editor::sky::SWITCH_LUA
        );
        assert_eq!(
            std::fs::read_to_string(addon.join("lua/autorun/hammerpro_skybox.lua")).expect("engine lua"),
            hp_editor::sky::SWITCH_ENGINE_LUA
        );
        assert_eq!(export_switch_lua(&game), Ok(false), "up to date");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn custom_materials_go_to_the_dev_addon() {
        let dir = crate::test_scratch("dev_addon");
        let pics = dir.join("pics");
        std::fs::create_dir_all(&pics).expect("pics");
        std::fs::write(pics.join("wall.tga"), tga(16, 16)).expect("tga");
        let game = dir.join("garrysmod");
        std::fs::create_dir_all(&game).expect("game");
        let fs = hp_fs::GameFs::empty();
        fs.add_custom_dir(hp_fs::CustomDir::with_prefix(&pics, "pics")).expect("custom dir");
        let used = vec!["pics/wall".to_owned(), "concrete/floor".to_owned(), "../../escape".to_owned()];
        let r = export(&fs, &game, &used).expect("export");
        assert!(r.written >= 1, "{r:?}");
        let addon = dev_addon_dir(&game);
        assert!(addon.join("materials/pics/wall.vmt").is_file());
        assert!(addon.join("materials/pics/wall.vtf").is_file());
        let again = export(&fs, &game, &used).expect("export");
        assert_eq!(again.written, 0, "up to date");
        let other = dir.join("other_game");
        std::fs::create_dir_all(&other).expect("other");
        export(&fs, &other, &["concrete/floor".to_owned()]).expect("export");
        assert!(!dev_addon_dir(&other).exists());
        assert!(export(&fs, &dir.join("missing"), &used).is_err());
    }
}
