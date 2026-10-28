use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rayon::prelude::*;

use crate::addon_json::AddonJson;
use crate::error::GmaError;
use crate::whitelist;

#[derive(Clone, Debug)]
pub enum GmaSource {
    Bytes(Vec<u8>),
    File(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RejectReason {
    NotWhitelisted,
    BadName,
    Empty,
    Ignored,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GmaSummary {
    pub files: usize,
    pub bytes: u64,
}

#[derive(Debug)]
pub struct GmaWriter {
    meta: AddonJson,
    author: String,
    entries: Vec<(String, GmaSource)>,
}

pub fn check_name(name: &str) -> Result<String, RejectReason> {
    let lower = name.replace('\\', "/").to_ascii_lowercase();
    let bad = lower.is_empty()
        || !lower.is_ascii()
        || lower.starts_with('/')
        || lower.len() > 1024
        || lower.split('/').any(|c| c.is_empty() || c == "." || c == ".." || c.ends_with('.') || c.ends_with(' '))
        || lower.chars().any(|c| c.is_ascii_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'));
    if bad {
        return Err(RejectReason::BadName);
    }
    if !whitelist::is_allowed(&lower) {
        return Err(RejectReason::NotWhitelisted);
    }
    Ok(lower)
}

impl GmaWriter {
    pub fn new(meta: AddonJson) -> GmaWriter {
        GmaWriter { meta, author: "Author Name".to_owned(), entries: Vec::new() }
    }

    pub fn set_author(&mut self, author: &str) {
        self.author = author.chars().filter(|&c| c != '\0').collect();
    }

    pub fn add(&mut self, name: &str, source: GmaSource) -> Result<(), RejectReason> {
        let key = check_name(name)?;
        if self.meta.is_ignored(&key) {
            return Err(RejectReason::Ignored);
        }
        if let GmaSource::Bytes(b) = &source
            && b.is_empty()
        {
            return Err(RejectReason::Empty);
        }
        match self.entries.iter_mut().find(|(n, _)| *n == key) {
            Some(slot) => slot.1 = source,
            None => self.entries.push((key, source)),
        }
        Ok(())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(n, _)| n.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn write(self, path: &Path) -> Result<GmaSummary, GmaError> {
        self.write_with(path, &AtomicBool::new(false), &|_, _| {})
    }

    pub fn write_with(
        mut self,
        path: &Path,
        cancel: &AtomicBool,
        progress: &(dyn Fn(u64, u64) + Sync),
    ) -> Result<GmaSummary, GmaError> {
        self.entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        let sums: Vec<(u64, u32)> = self
            .entries
            .par_iter()
            .map(|(name, source)| {
                if cancel.load(Ordering::Relaxed) {
                    return Err(GmaError::Corrupt("cancelled".into()));
                }
                let (size, crc) = match source {
                    GmaSource::Bytes(b) => (b.len() as u64, crc32fast::hash(b)),
                    GmaSource::File(p) => hash_file(p)?,
                };
                if size == 0 {
                    return Err(GmaError::Corrupt(format!("{name} is empty (gmad refuses empty files)")));
                }
                Ok((size, crc))
            })
            .collect::<Result<_, _>>()?;
        let total: u64 = sums.iter().map(|s| s.0).sum();

        let tmp = temp_path(path);
        let result = self.stream(&tmp, &sums, total, cancel, progress);
        match result {
            Ok(summary) => {
                std::fs::rename(&tmp, path).map_err(|e| {
                    let _ = std::fs::remove_file(&tmp);
                    GmaError::io(path, e)
                })?;
                Ok(summary)
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    fn stream(
        &self,
        tmp: &Path,
        sums: &[(u64, u32)],
        total: u64,
        cancel: &AtomicBool,
        progress: &(dyn Fn(u64, u64) + Sync),
    ) -> Result<GmaSummary, GmaError> {
        let file = File::create(tmp).map_err(|e| GmaError::io(tmp, e))?;
        let mut out = HashingWriter {
            inner: BufWriter::with_capacity(1 << 20, file),
            hasher: crc32fast::Hasher::new(),
            written: 0,
        };
        let io = |e: std::io::Error| GmaError::io(tmp, e);
        let timestamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let cstr = |out: &mut HashingWriter, s: &str| -> std::io::Result<()> {
            let clean: String = s.chars().filter(|&c| c != '\0').collect();
            out.write_all(clean.as_bytes())?;
            out.write_all(&[0])
        };
        out.write_all(b"GMAD").map_err(io)?;
        out.write_all(&[3]).map_err(io)?;
        out.write_all(&0u64.to_le_bytes()).map_err(io)?;
        out.write_all(&timestamp.to_le_bytes()).map_err(io)?;
        out.write_all(&[0]).map_err(io)?;
        cstr(&mut out, self.meta.title.trim()).map_err(io)?;
        cstr(&mut out, &self.meta.gma_description()).map_err(io)?;
        cstr(&mut out, &self.author).map_err(io)?;
        out.write_all(&1i32.to_le_bytes()).map_err(io)?;
        for (i, ((name, _), (size, crc))) in self.entries.iter().zip(sums).enumerate() {
            out.write_all(&(i as u32 + 1).to_le_bytes()).map_err(io)?;
            cstr(&mut out, name).map_err(io)?;
            out.write_all(&(*size as i64).to_le_bytes()).map_err(io)?;
            out.write_all(&crc.to_le_bytes()).map_err(io)?;
        }
        out.write_all(&0u32.to_le_bytes()).map_err(io)?;
        let done = AtomicU64::new(0);
        let mut buf = vec![0u8; 1 << 20];
        for ((name, source), (size, crc)) in self.entries.iter().zip(sums) {
            if cancel.load(Ordering::Relaxed) {
                return Err(GmaError::Corrupt("cancelled".into()));
            }
            let changed = || GmaError::Corrupt(format!("{name} changed while the archive was written"));
            match source {
                GmaSource::Bytes(b) => out.write_all(b).map_err(io)?,
                GmaSource::File(p) => {
                    let mut f = File::open(p).map_err(|e| GmaError::io(p, e))?;
                    let mut hasher = crc32fast::Hasher::new();
                    let mut n = 0u64;
                    loop {
                        let k = f.read(&mut buf).map_err(|e| GmaError::io(p, e))?;
                        if k == 0 {
                            break;
                        }
                        n += k as u64;
                        if n > *size {
                            return Err(changed());
                        }
                        hasher.update(&buf[..k]);
                        out.write_all(&buf[..k]).map_err(io)?;
                    }
                    if n != *size || hasher.finalize() != *crc {
                        return Err(changed());
                    }
                }
            }
            progress(done.fetch_add(*size, Ordering::Relaxed) + size, total);
        }
        let crc = out.hasher.clone().finalize();
        out.inner.write_all(&crc.to_le_bytes()).map_err(io)?;
        let bytes = out.written + 4;
        let file = out.inner.into_inner().map_err(|e| GmaError::io(tmp, e.into_error()))?;
        file.sync_all().map_err(io)?;
        Ok(GmaSummary { files: self.entries.len(), bytes })
    }
}

struct HashingWriter {
    inner: BufWriter<File>,
    hasher: crc32fast::Hasher,
    written: u64,
}

impl Write for HashingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn hash_file(path: &Path) -> Result<(u64, u32), GmaError> {
    let mut f = File::open(path).map_err(|e| GmaError::io(path, e))?;
    let mut hasher = crc32fast::Hasher::new();
    let mut buf = vec![0u8; 256 << 10];
    let mut n = 0u64;
    loop {
        let k = f.read(&mut buf).map_err(|e| GmaError::io(path, e))?;
        if k == 0 {
            break;
        }
        n += k as u64;
        hasher.update(&buf[..k]);
    }
    Ok((n, hasher.finalize()))
}

fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "addon.gma".into());
    path.with_file_name(format!(".{name}.{}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GmaArchive;

    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/hp-gma/scratch")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trip_with_the_reader() {
        let dir = scratch("writer");
        let loose = dir.join("x.vtf");
        std::fs::write(&loose, vec![9u8; 300_000]).unwrap();
        let meta = AddonJson {
            title: "Test map".into(),
            tags: vec!["build".into()],
            ignore: vec!["*.psd".into()],
            description: "desc".into(),
            ..AddonJson::default()
        };
        let mut w = GmaWriter::new(meta);
        w.add("Maps/GM_Test.bsp", GmaSource::Bytes(b"VBSP data".to_vec())).unwrap();
        w.add("materials/custom/x.vtf", GmaSource::File(loose.clone())).unwrap();
        assert_eq!(
            w.add("maps/gm_test_particles.txt", GmaSource::Bytes(b"x".to_vec())),
            Err(RejectReason::NotWhitelisted)
        );
        assert_eq!(w.add("materials/a.psd", GmaSource::Bytes(b"x".to_vec())), Err(RejectReason::NotWhitelisted));
        assert_eq!(w.add("../maps/x.bsp", GmaSource::Bytes(b"x".to_vec())), Err(RejectReason::BadName));
        assert_eq!(w.add("maps/x.bsp", GmaSource::Bytes(Vec::new())), Err(RejectReason::Empty));
        let out = dir.join("test.gma");
        let summary = w.write(&out).unwrap();
        assert_eq!(summary.files, 2);
        assert_eq!(summary.bytes, std::fs::metadata(&out).unwrap().len());

        let gma = GmaArchive::open(&out).unwrap();
        assert_eq!(gma.name(), "Test map");
        assert_eq!(gma.addon_type(), Some("map"));
        assert_eq!(gma.tags(), ["build"]);
        assert_eq!(gma.description(), "desc");
        assert_eq!(gma.read_checked("maps/gm_test.bsp").unwrap().unwrap(), b"VBSP data");
        assert_eq!(gma.read_checked("materials/custom/x.vtf").unwrap().unwrap().len(), 300_000);
        let bytes = std::fs::read(&out).unwrap();
        let (body, tail) = bytes.split_at(bytes.len() - 4);
        assert_eq!(u32::from_le_bytes(tail.try_into().unwrap()), crc32fast::hash(body));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_and_empty_files_fail_without_output() {
        let dir = scratch("writer-fail");
        let empty = dir.join("empty.vtf");
        std::fs::write(&empty, b"").unwrap();
        let mut w = GmaWriter::new(AddonJson { title: "T".into(), ..AddonJson::default() });
        w.add("materials/e.vtf", GmaSource::File(empty)).unwrap();
        let out = dir.join("a.gma");
        assert!(w.write(&out).is_err());
        let mut w = GmaWriter::new(AddonJson { title: "T".into(), ..AddonJson::default() });
        w.add("materials/m.vtf", GmaSource::File(dir.join("missing.vtf"))).unwrap();
        assert!(w.write(&out).is_err());
        assert!(!out.exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
