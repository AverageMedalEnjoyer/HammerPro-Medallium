use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub(crate) fn safe_join(base: &Path, vpath: &str) -> Option<PathBuf> {
    let key = hp_bsp::normalize_entry_name(vpath).ok()?;
    let mut out = base.to_path_buf();
    for part in key.split('/') {
        out.push(part);
    }
    Some(out)
}

fn temp_next_to(path: &Path) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!(".{name}.{}.hptmp", std::process::id()))
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = temp_next_to(path);
    let result = (|| {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

pub(crate) fn copy_atomic(src: &Path, dest: &Path) -> io::Result<u64> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = temp_next_to(dest);
    let result = (|| {
        let mut input = File::open(src)?;
        let mut out = File::create(&tmp)?;
        let n = io::copy(&mut input, &mut out)?;
        out.sync_all()?;
        drop(out);
        std::fs::rename(&tmp, dest)?;
        Ok(n)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

pub(crate) fn stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    Some((meta.len(), u64::try_from(mtime).unwrap_or(u64::MAX)))
}

pub(crate) fn read_limited(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let f = File::open(path).ok()?;
    if f.metadata().ok()?.len() > limit {
        return None;
    }
    let mut out = Vec::new();
    f.take(limit).read_to_end(&mut out).ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_are_safe_and_writes_atomic() {
        let dir = crate::test_scratch("release_files");
        assert_eq!(safe_join(&dir, "Maps/A.bsp"), Some(dir.join("maps").join("a.bsp")));
        for bad in ["../x", "/x", "c:/x", "a/../../x", "maps/nul.txt", "é"] {
            assert_eq!(safe_join(&dir, bad), None, "{bad}");
        }
        let p = dir.join("sub/a.txt");
        write_atomic(&p, b"hello").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"hello");
        let q = dir.join("sub2/b.txt");
        assert_eq!(copy_atomic(&p, &q).unwrap(), 5);
        assert_eq!(stamp(&q).map(|s| s.0), Some(5));
        assert_eq!(read_limited(&q, 4), None);
        assert_eq!(read_limited(&q, 5).as_deref(), Some(&b"hello"[..]));
        let leftovers = std::fs::read_dir(dir.join("sub")).unwrap().count();
        assert_eq!(leftovers, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
