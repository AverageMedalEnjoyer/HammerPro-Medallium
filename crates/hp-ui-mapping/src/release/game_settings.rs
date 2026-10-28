use std::path::{Path, PathBuf};

pub(crate) struct Backup {
    config: Option<(PathBuf, Vec<u8>)>,
    #[cfg(windows)]
    registry: Option<(String, Vec<reg::Value>)>,
}

pub(crate) fn backup(game_dir: &Path) -> Backup {
    let config_path = game_dir.join("cfg").join("config.cfg");
    let config = super::files::read_limited(&config_path, 4 << 20).map(|b| (config_path, b));
    #[cfg(windows)]
    {
        let folder = game_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let key = format!("Software\\Valve\\Source\\{folder}\\Settings");
        let registry = (!folder.is_empty()).then(|| reg::read_all(&key).map(|v| (key, v))).flatten();
        Backup { config, registry }
    }
    #[cfg(not(windows))]
    Backup { config }
}

impl Backup {
    pub(crate) fn restore(&self) {
        if let Some((path, bytes)) = &self.config
            && std::fs::read(path).ok().as_deref() != Some(bytes.as_slice())
        {
            let _ = super::files::write_atomic(path, bytes);
        }
        #[cfg(windows)]
        if let Some((key, values)) = &self.registry {
            reg::write_changed(key, values);
        }
    }
}

#[cfg(windows)]
mod reg {
    use windows_sys::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, RegCloseKey, RegEnumValueW, RegOpenKeyExW, RegSetValueExW,
    };

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct Value {
        name: Vec<u16>,
        kind: u32,
        data: Vec<u8>,
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn open(subkey: &str, access: u32) -> Option<HKEY> {
        let path = wide(subkey);
        let mut key: HKEY = std::ptr::null_mut();
        let r = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut key) };
        (r == ERROR_SUCCESS).then_some(key)
    }

    pub(crate) fn read_all(subkey: &str) -> Option<Vec<Value>> {
        let key = open(subkey, KEY_READ)?;
        let mut out = Vec::new();
        for index in 0..1024u32 {
            let mut name = vec![0u16; 256];
            let mut name_len = name.len() as u32;
            let mut data = vec![0u8; 64 << 10];
            let mut data_len = data.len() as u32;
            let mut kind = 0u32;
            let r = unsafe {
                RegEnumValueW(
                    key,
                    index,
                    name.as_mut_ptr(),
                    &mut name_len,
                    std::ptr::null(),
                    &mut kind,
                    data.as_mut_ptr(),
                    &mut data_len,
                )
            };
            if r == ERROR_NO_MORE_ITEMS {
                break;
            }
            if r != ERROR_SUCCESS {
                continue;
            }
            name.truncate((name_len as usize).min(255));
            name.push(0);
            data.truncate(data_len as usize);
            out.push(Value { name, kind, data });
        }
        unsafe { RegCloseKey(key) };
        Some(out)
    }

    pub(crate) fn write_changed(subkey: &str, values: &[Value]) {
        let current = read_all(subkey).unwrap_or_default();
        let Some(key) = open(subkey, KEY_SET_VALUE) else { return };
        for v in values {
            if current.contains(v) {
                continue;
            }
            let Ok(len) = u32::try_from(v.data.len()) else { continue };
            unsafe { RegSetValueExW(key, v.name.as_ptr(), 0, v.kind, v.data.as_ptr(), len) };
        }
        unsafe { RegCloseKey(key) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_is_put_back() {
        let dir = crate::test_scratch("release_game_settings");
        let game = dir.join("hp_test_game_that_does_not_exist");
        std::fs::create_dir_all(game.join("cfg")).unwrap();
        let cfg = game.join("cfg/config.cfg");
        std::fs::write(&cfg, b"bind w +forward\n").unwrap();
        let b = backup(&game);
        std::fs::write(&cfg, b"changed\n").unwrap();
        b.restore();
        assert_eq!(std::fs::read(&cfg).unwrap(), b"bind w +forward\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn registry_round_trip_reads_without_writing() {
        assert!(reg::read_all("Software\\HammerPro\\NoSuchKey\\Settings").is_none());
        reg::write_changed("Software\\HammerPro\\NoSuchKey\\Settings", &[]);
    }
}
