use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Map, TextEncoding, VmfError};

#[derive(Clone, Debug)]
pub struct SaveOptions {
    pub backup: bool,
    pub sync: bool,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self { backup: true, sync: true }
    }
}

#[derive(Debug, Default)]
pub struct SaveReport {
    pub bytes: usize,
    pub backup_path: Option<PathBuf>,
    pub backup_error: Option<io::Error>,
    pub encoding: TextEncoding,
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("vmx")
}

pub fn save_with(map: &Map, path: &Path, options: &SaveOptions) -> Result<SaveReport, VmfError> {
    let (bytes, encoding) = crate::write_bytes(map);
    let mut report = write_atomic(path, &bytes, options)?;
    report.encoding = encoding;
    Ok(report)
}

pub(crate) fn write_atomic(path: &Path, data: &[u8], options: &SaveOptions) -> Result<SaveReport, VmfError> {
    let file_name =
        path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "save path has no file name"))?;
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let tmp = dir.join(format!(
        ".{}.{}-{}.tmp",
        file_name.to_string_lossy(),
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    let result = (|| -> Result<SaveReport, VmfError> {
        {
            let mut f = fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
            f.write_all(data)?;
            if options.sync {
                f.sync_all()?;
            }
        }
        let mut report = SaveReport { bytes: data.len(), ..SaveReport::default() };
        if options.backup && path.is_file() {
            let backup = backup_path(path);
            if backup != path {
                match fs::copy(path, &backup) {
                    Ok(_) => report.backup_path = Some(backup),
                    Err(e) => report.backup_error = Some(e),
                }
            }
        }
        fs::rename(&tmp, path)?;
        Ok(report)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
