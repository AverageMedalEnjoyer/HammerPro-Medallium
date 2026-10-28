use std::path::{Path, PathBuf};

use hp_bsp::Bsp;

pub const MAX_FILES: usize = 100_000;
pub const MAX_TOTAL_BYTES: u64 = 4 << 30;
pub const MAX_FILE_BYTES: u64 = 512 << 20;

const WANTED: &[&str] = &["materials/", "models/", "particles/"];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extracted {
    pub files: usize,
    pub bytes: u64,
    pub skipped: usize,
}

pub fn safe_join(dest: &Path, name: &str) -> Option<PathBuf> {
    let key = hp_bsp::normalize_entry_name(name).ok()?;
    let mut out = dest.to_path_buf();
    for part in key.split('/') {
        out.push(part);
    }
    out.starts_with(dest).then_some(out)
}

fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

pub fn extract_content(bsp: &Bsp, dest: &Path) -> Result<Extracted, hp_bsp::BspError> {
    let pak = bsp.pakfile()?;
    let mut out = Extracted::default();
    for entry in pak.entries() {
        if !WANTED.iter().any(|w| entry.key.starts_with(w)) {
            continue;
        }
        if out.files >= MAX_FILES
            || u64::from(entry.size) > MAX_FILE_BYTES
            || out.bytes + u64::from(entry.size) > MAX_TOTAL_BYTES
        {
            out.skipped += 1;
            continue;
        }
        let Some(path) = safe_join(dest, &entry.name) else {
            out.skipped += 1;
            continue;
        };
        let Ok(data) = pak.read(entry) else {
            out.skipped += 1;
            continue;
        };
        if write_atomic(&path, &data).is_err() {
            out.skipped += 1;
            continue;
        }
        out.files += 1;
        out.bytes += data.len() as u64;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_never_leave_the_folder() {
        let dest = Path::new("cache/map");
        assert_eq!(safe_join(dest, "materials/a/b.vmt"), Some(dest.join("materials").join("a").join("b.vmt")));
        assert_eq!(safe_join(dest, r"Materials\A.VMT"), Some(dest.join("materials").join("a.vmt")));
        for bad in ["../evil.vmt", "/abs.vmt", "c:/x.vmt", "materials/../../x", "materials/con.vmt", ""] {
            assert_eq!(safe_join(dest, bad), None, "{bad}");
        }
    }

    #[test]
    fn content_is_copied_from_a_pakfile() {
        let mut w = hp_bsp::PakWriter::new();
        w.add("materials/test/a.vmt", b"LightmappedGeneric {}".to_vec()).expect("add");
        w.add("lua/autorun/x.lua", b"print(1)".to_vec()).expect("add");
        let zip = w.finish(hp_bsp::PakOptions::default()).expect("zip");
        let data = hp_bsp::synthetic_bsp(20, &[(hp_bsp::lump::PAKFILE, zip)]);
        let bsp = Bsp::from_vec(data).expect("bsp");
        let dir = std::env::temp_dir().join(format!("hp_decompile_pak_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let got = extract_content(&bsp, &dir).expect("extract");
        assert_eq!(got.files, 1, "only materials, models and particles");
        let text = std::fs::read_to_string(dir.join("materials").join("test").join("a.vmt")).expect("written");
        assert!(text.contains("LightmappedGeneric"));
        assert!(!dir.join("lua").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
