use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rayon::prelude::*;

use crate::error::{BspError, Result};
use crate::lzma;

pub const METHOD_STORED: u16 = 0;
pub const METHOD_DEFLATE: u16 = 8;
pub const METHOD_LZMA: u16 = 14;

const SIG_LOCAL: u32 = 0x0403_4b50;
const SIG_CENTRAL: u32 = 0x0201_4b50;
const SIG_END: u32 = 0x0605_4b50;
const LOCAL_LEN: usize = 30;
const CENTRAL_LEN: usize = 46;
const END_LEN: usize = 22;
const DOS_DATE: u16 = 0x0021;
const MAX_NAME: usize = 260;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

const RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2",
    "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9", "conin$", "conout$",
];

pub fn normalize_entry_name(name: &str) -> Result<String> {
    let bad = || BspError::InvalidPath(name.chars().take(MAX_NAME).collect());
    if name.is_empty() || name.len() > MAX_NAME || !name.is_ascii() {
        return Err(bad());
    }
    let lower = name.replace('\\', "/").to_ascii_lowercase();
    if lower.starts_with('/') {
        return Err(bad());
    }
    for c in lower.split('/') {
        if c.is_empty() || c == "." || c == ".." || c.ends_with('.') || c.ends_with(' ') {
            return Err(bad());
        }
        if c.chars().any(|ch| ch.is_ascii_control() || matches!(ch, ':' | '*' | '?' | '"' | '<' | '>' | '|')) {
            return Err(bad());
        }
        let stem = c.split('.').next().unwrap_or(c).trim_end();
        if RESERVED.contains(&stem) {
            return Err(bad());
        }
    }
    Ok(lower)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PakEntry {
    pub name: String,
    pub key: String,
    pub method: u16,
    pub crc32: u32,
    pub compressed_size: u32,
    pub size: u32,
    local_offset: usize,
}

impl PakEntry {
    pub fn engine_readable(&self) -> bool {
        matches!(self.method, METHOD_STORED | METHOD_LZMA)
    }

    pub fn is_compressed(&self) -> bool {
        self.method != METHOD_STORED
    }
}

pub struct PakArchive<'a> {
    data: Cow<'a, [u8]>,
    entries: Vec<PakEntry>,
    index: HashMap<String, usize>,
}

impl std::fmt::Debug for PakArchive<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PakArchive").field("entries", &self.entries.len()).field("bytes", &self.data.len()).finish()
    }
}

impl<'a> PakArchive<'a> {
    pub fn parse(data: impl Into<Cow<'a, [u8]>>) -> Result<PakArchive<'a>> {
        let data = data.into();
        if data.is_empty() {
            return Ok(PakArchive { data, entries: Vec::new(), index: HashMap::new() });
        }
        let (entries, index) = parse_directory(&data)?;
        Ok(PakArchive { data, entries, index })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn byte_len(&self) -> usize {
        self.data.len()
    }

    pub fn entries(&self) -> &[PakEntry] {
        &self.entries
    }

    pub fn find(&self, name: &str) -> Option<&PakEntry> {
        let key = name.replace('\\', "/").to_ascii_lowercase();
        self.index.get(key.trim_start_matches('/')).map(|&i| &self.entries[i])
    }

    pub fn contains(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    pub fn any_compressed(&self) -> bool {
        self.entries.iter().any(PakEntry::is_compressed)
    }

    pub fn raw(&self, entry: &PakEntry) -> Result<&[u8]> {
        let local = self.data.get(entry.local_offset..).ok_or_else(|| BspError::zip("local header out of bounds"))?;
        if u32_at(local, 0) != Some(SIG_LOCAL) {
            return Err(BspError::zip(format!("{}: bad local header", entry.name)));
        }
        let n = usize::from(u16_at(local, 26).unwrap_or(0));
        let e = usize::from(u16_at(local, 28).unwrap_or(0));
        let start = LOCAL_LEN + n + e;
        local
            .get(start..start + entry.compressed_size as usize)
            .ok_or_else(|| BspError::zip(format!("{}: data out of bounds", entry.name)))
    }

    pub fn read(&self, entry: &PakEntry) -> Result<Cow<'_, [u8]>> {
        let raw = self.raw(entry)?;
        let data = match entry.method {
            METHOD_STORED => {
                if raw.len() != entry.size as usize {
                    return Err(BspError::zip(format!("{}: stored size mismatch", entry.name)));
                }
                Cow::Borrowed(raw)
            }
            METHOD_LZMA => Cow::Owned(lzma::decompress_zip(raw, u64::from(entry.size))?),
            method => return Err(BspError::UnsupportedMethod { name: entry.name.clone(), method }),
        };
        if crc32fast::hash(&data) != entry.crc32 {
            return Err(BspError::CrcMismatch { name: entry.name.clone() });
        }
        Ok(data)
    }

    pub fn read_name(&self, name: &str) -> Result<Option<Cow<'_, [u8]>>> {
        match self.find(name) {
            Some(e) => self.read(e).map(Some),
            None => Ok(None),
        }
    }

    pub fn to_writer(&self) -> Result<PakWriter> {
        let mut w = PakWriter::new();
        for e in &self.entries {
            let raw = self.raw(e)?;
            w.insert(
                e.key.clone(),
                Source::Stored { method: e.method, crc: e.crc32, size: e.size, data: raw.to_vec() },
            );
        }
        Ok(w)
    }
}

type Directory = (Vec<PakEntry>, HashMap<String, usize>);

fn parse_directory(data: &[u8]) -> Result<Directory> {
    let min = data.len().saturating_sub(END_LEN + 0xFFFF);
    let end = (min..=data.len().saturating_sub(END_LEN))
        .rev()
        .find(|&p| u32_at(data, p) == Some(SIG_END))
        .ok_or_else(|| BspError::zip("no end of central directory"))?;
    let count = usize::from(u16_at(data, end + 10).unwrap_or(0));
    let cd_size = u32_at(data, end + 12).unwrap_or(0) as usize;
    let cd_ofs = u32_at(data, end + 16).unwrap_or(0) as usize;
    let cd_start = end.checked_sub(cd_size).ok_or_else(|| BspError::zip("central directory size out of bounds"))?;
    let shift = cd_start as i64 - cd_ofs as i64;
    if count > cd_size / CENTRAL_LEN {
        return Err(BspError::zip("entry count does not fit the central directory"));
    }
    let mut entries = Vec::with_capacity(count);
    let mut index = HashMap::with_capacity(count);
    let mut at = cd_start;
    for _ in 0..count {
        if u32_at(data, at) != Some(SIG_CENTRAL) {
            return Err(BspError::zip("bad central directory entry"));
        }
        let flags = u16_at(data, at + 8).unwrap_or(0);
        let method = u16_at(data, at + 10).unwrap_or(0);
        let crc32 = u32_at(data, at + 16).unwrap_or(0);
        let compressed_size = u32_at(data, at + 20).unwrap_or(0);
        let size = u32_at(data, at + 24).unwrap_or(0);
        let n = usize::from(u16_at(data, at + 28).unwrap_or(0));
        let e = usize::from(u16_at(data, at + 30).unwrap_or(0));
        let c = usize::from(u16_at(data, at + 32).unwrap_or(0));
        let local = u32_at(data, at + 42).unwrap_or(0);
        let name_bytes =
            data.get(at + CENTRAL_LEN..at + CENTRAL_LEN + n).ok_or_else(|| BspError::zip("name out of bounds"))?;
        at += CENTRAL_LEN + n + e + c;
        if flags & 1 != 0 {
            return Err(BspError::zip("encrypted entries are not supported"));
        }
        let local_offset = local as i64 + shift;
        if local_offset < 0 || local_offset as usize >= data.len() {
            return Err(BspError::zip("local header out of bounds"));
        }
        let name = String::from_utf8_lossy(name_bytes).into_owned();
        if name.ends_with('/') && size == 0 {
            continue;
        }
        let key = name.replace('\\', "/").to_ascii_lowercase().trim_start_matches('/').to_owned();
        if key.is_empty() {
            continue;
        }
        let entry = PakEntry {
            name,
            key: key.clone(),
            method,
            crc32,
            compressed_size,
            size,
            local_offset: local_offset as usize,
        };
        index.entry(key).or_insert(entries.len());
        entries.push(entry);
    }
    Ok((entries, index))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PakCompression {
    #[default]
    Store,
    Lzma,
}

pub fn must_stay_stored(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".wav", ".mp3", ".ogg"].iter().any(|e| lower.ends_with(e))
}

#[derive(Clone, Copy, Debug)]
pub struct PakOptions {
    pub compression: PakCompression,
    pub level: u32,
}

impl Default for PakOptions {
    fn default() -> Self {
        PakOptions { compression: PakCompression::Store, level: 6 }
    }
}

enum Source {
    Bytes(Vec<u8>),
    File(PathBuf),
    Stored { method: u16, crc: u32, size: u32, data: Vec<u8> },
}

struct Encoded {
    method: u16,
    crc: u32,
    size: u32,
    data: Vec<u8>,
}

#[derive(Default)]
pub struct PakWriter {
    entries: HashMap<String, Source>,
    order: Vec<String>,
}

impl std::fmt::Debug for PakWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PakWriter").field("entries", &self.entries.len()).finish()
    }
}

impl PakWriter {
    pub fn new() -> PakWriter {
        PakWriter::default()
    }

    fn insert(&mut self, key: String, source: Source) {
        if self.entries.insert(key.clone(), source).is_none() {
            self.order.push(key);
        }
    }

    pub fn add(&mut self, name: &str, data: Vec<u8>) -> Result<()> {
        let key = normalize_entry_name(name)?;
        self.insert(key, Source::Bytes(data));
        Ok(())
    }

    pub fn add_file(&mut self, name: &str, path: impl Into<PathBuf>) -> Result<()> {
        let key = normalize_entry_name(name)?;
        self.insert(key, Source::File(path.into()));
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let key = name.replace('\\', "/").to_ascii_lowercase();
        let key = key.trim_start_matches('/');
        let removed = self.entries.remove(key).is_some();
        if removed {
            self.order.retain(|k| k != key);
        }
        removed
    }

    pub fn contains(&self, name: &str) -> bool {
        let key = name.replace('\\', "/").to_ascii_lowercase();
        self.entries.contains_key(key.trim_start_matches('/'))
    }

    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.entries.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn finish(self, opts: PakOptions) -> Result<Vec<u8>> {
        self.finish_with(opts, &AtomicBool::new(false), &|_, _| {})
    }

    pub fn finish_with(
        self,
        opts: PakOptions,
        cancel: &AtomicBool,
        progress: &(dyn Fn(u64, u64) + Sync),
    ) -> Result<Vec<u8>> {
        let mut items: Vec<(String, Source)> = self.entries.into_iter().collect();
        items.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        let encoded = encode_all(&items, opts, cancel, progress)?;
        assemble(items.iter().map(|(n, _)| n.as_str()), &encoded, DOS_DATE, &[])
    }

    pub fn finish_vbsp(self) -> Result<Vec<u8>> {
        let mut entries = self.entries;
        let items: Vec<(String, Source)> =
            self.order.into_iter().filter_map(|k| entries.remove(&k).map(|s| (k, s))).collect();
        let encoded = encode_all(&items, PakOptions::default(), &AtomicBool::new(false), &|_, _| {})?;
        let mut comment = [0u8; 32];
        comment[..6].copy_from_slice(b"XZP1 0");
        assemble(items.iter().map(|(n, _)| n.as_str()), &encoded, 0, &comment)
    }
}

fn encode_all(
    items: &[(String, Source)],
    opts: PakOptions,
    cancel: &AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<Vec<Encoded>> {
    let total = items.len() as u64;
    let done = AtomicU64::new(0);
    let encoded: Vec<Encoded> = items
        .par_iter()
        .map(|(name, source)| {
            if cancel.load(Ordering::Relaxed) {
                return Err(BspError::Compress("cancelled".into()));
            }
            let out = encode(name, source, opts);
            progress(done.fetch_add(1, Ordering::Relaxed) + 1, total);
            out
        })
        .collect::<Result<_>>()?;
    Ok(encoded)
}

fn encode(name: &str, source: &Source, opts: PakOptions) -> Result<Encoded> {
    let too_large = || BspError::TooLarge(format!("{name} is larger than 4 GiB"));
    let compress = opts.compression == PakCompression::Lzma && !must_stay_stored(name);
    let plain = |data: Vec<u8>| -> Result<Encoded> {
        let size = u32::try_from(data.len()).map_err(|_| too_large())?;
        let crc = crc32fast::hash(&data);
        if compress && data.len() >= 64 {
            let packed = lzma::compress_zip(&data, opts.level)?;
            if packed.len() < data.len() {
                return Ok(Encoded { method: METHOD_LZMA, crc, size, data: packed });
            }
        }
        Ok(Encoded { method: METHOD_STORED, crc, size, data })
    };
    match source {
        Source::Bytes(data) => plain(data.clone()),
        Source::File(path) => {
            let data = std::fs::read(path).map_err(|e| BspError::io(path, e))?;
            plain(data)
        }
        Source::Stored { method, crc, size, data } => {
            let keep = match *method {
                METHOD_STORED => !compress,
                METHOD_LZMA => compress,
                _ => false,
            };
            if keep {
                return Ok(Encoded { method: *method, crc: *crc, size: *size, data: data.clone() });
            }
            let raw = match *method {
                METHOD_STORED => data.clone(),
                METHOD_LZMA => lzma::decompress_zip(data, u64::from(*size))?,
                m => return Err(BspError::UnsupportedMethod { name: name.to_owned(), method: m }),
            };
            if crc32fast::hash(&raw) != *crc {
                return Err(BspError::CrcMismatch { name: name.to_owned() });
            }
            plain(raw)
        }
    }
}

fn assemble<'n>(
    names: impl Iterator<Item = &'n str>,
    encoded: &[Encoded],
    date: u16,
    comment: &[u8],
) -> Result<Vec<u8>> {
    let names: Vec<&str> = names.collect();
    let too_large = || BspError::TooLarge("pakfile larger than 4 GiB".into());
    if names.len() > usize::from(u16::MAX) {
        return Err(BspError::TooLarge(format!("{} files (the ZIP format allows 65535)", names.len())));
    }
    let total: usize =
        names.iter().zip(encoded).map(|(n, e)| LOCAL_LEN + CENTRAL_LEN + 2 * n.len() + e.data.len()).sum::<usize>()
            + END_LEN
            + comment.len();
    if total > u32::MAX as usize {
        return Err(too_large());
    }
    let mut out = Vec::with_capacity(total);
    let mut offsets = Vec::with_capacity(names.len());
    for (name, e) in names.iter().zip(encoded) {
        offsets.push(out.len() as u32);
        out.extend_from_slice(&SIG_LOCAL.to_le_bytes());
        out.extend_from_slice(&version_needed(e.method).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&e.method.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&date.to_le_bytes());
        out.extend_from_slice(&e.crc.to_le_bytes());
        out.extend_from_slice(&(e.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&e.size.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&e.data);
    }
    let cd_start = out.len();
    for ((name, e), ofs) in names.iter().zip(encoded).zip(&offsets) {
        out.extend_from_slice(&SIG_CENTRAL.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&version_needed(e.method).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&e.method.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&date.to_le_bytes());
        out.extend_from_slice(&e.crc.to_le_bytes());
        out.extend_from_slice(&(e.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&e.size.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&ofs.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
    }
    let cd_len = out.len() - cd_start;
    out.extend_from_slice(&SIG_END.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(names.len() as u16).to_le_bytes());
    out.extend_from_slice(&(names.len() as u16).to_le_bytes());
    out.extend_from_slice(&(cd_len as u32).to_le_bytes());
    out.extend_from_slice(&(cd_start as u32).to_le_bytes());
    out.extend_from_slice(&(comment.len() as u16).to_le_bytes());
    out.extend_from_slice(comment);
    Ok(out)
}

fn version_needed(method: u16) -> u16 {
    if method == METHOD_LZMA { 63 } else { 10 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut w = PakWriter::new();
        w.add("Materials\\Maps\\Test\\C0_0_0.vtf", vec![7; 5000]).unwrap();
        w.add("sound/ambient/loop.wav", vec![3; 5000]).unwrap();
        w.add("maps/test_particles.txt", b"particles_manifest\n{\n}\n".to_vec()).unwrap();
        w.finish(PakOptions::default()).unwrap()
    }

    #[test]
    fn stored_round_trip_is_deterministic() {
        let bytes = sample();
        assert_eq!(bytes, sample());
        let pak = PakArchive::parse(bytes.as_slice()).unwrap();
        assert_eq!(pak.len(), 3);
        assert!(!pak.any_compressed());
        let names: Vec<&str> = pak.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["maps/test_particles.txt", "materials/maps/test/c0_0_0.vtf", "sound/ambient/loop.wav"]);
        assert_eq!(pak.read_name("MATERIALS/maps/test/c0_0_0.VTF").unwrap().unwrap().as_ref(), &[7; 5000][..]);
        assert!(pak.read_name("nope").unwrap().is_none());
    }

    #[test]
    fn lzma_skips_audio_and_round_trips() {
        let pak = PakArchive::parse(sample()).unwrap();
        let w = pak.to_writer().unwrap();
        let packed = w.finish(PakOptions { compression: PakCompression::Lzma, level: 5 }).unwrap();
        let pak = PakArchive::parse(packed.as_slice()).unwrap();
        assert!(pak.find("materials/maps/test/c0_0_0.vtf").unwrap().is_compressed());
        assert!(!pak.find("sound/ambient/loop.wav").unwrap().is_compressed());
        assert_eq!(pak.read_name("materials/maps/test/c0_0_0.vtf").unwrap().unwrap().as_ref(), &[7; 5000][..]);
        let stored = pak.to_writer().unwrap().finish(PakOptions::default()).unwrap();
        assert_eq!(stored, sample());
    }

    #[test]
    fn hostile_names_are_refused() {
        for bad in
            ["", "/abs.txt", "../x.txt", "a/../b", "c:/x", "maps/con.txt", "a//b", "a/b.", "é.txt", "a\u{1}b", "x/aux"]
        {
            assert!(normalize_entry_name(bad).is_err(), "{bad:?}");
        }
        assert_eq!(normalize_entry_name("Maps\\A.TXT").unwrap(), "maps/a.txt");
    }

    #[test]
    fn hostile_archives_fail_cleanly() {
        let good = sample();
        assert!(PakArchive::parse(&good[..good.len() - 1]).is_err());
        assert!(PakArchive::parse(&[0u8; 100][..]).is_err());
        for i in 0..good.len() {
            let mut bad = good.clone();
            bad[i] ^= 0xA5;
            if let Ok(pak) = PakArchive::parse(bad.as_slice()) {
                for e in pak.entries() {
                    let _ = pak.read(e);
                }
            }
        }
        let mut huge = good.clone();
        let end = huge.len() - END_LEN;
        huge[end + 10..end + 12].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(PakArchive::parse(huge.as_slice()).is_err());
    }

    #[test]
    fn prepended_data_is_tolerated() {
        let mut shifted = vec![0u8; 77];
        shifted.extend_from_slice(&sample());
        let pak = PakArchive::parse(shifted.as_slice()).unwrap();
        assert_eq!(pak.read_name("sound/ambient/loop.wav").unwrap().unwrap().len(), 5000);
    }
}
