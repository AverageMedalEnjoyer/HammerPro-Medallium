use std::borrow::Cow;
use std::fmt;
use std::fs::File;
use std::iter::FusedIterator;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use memmap2::Mmap;

use crate::error::GmaError;
use crate::format::{self, Header, MAGIC, ParseError};
use crate::index::{PathFilter, PathIndex, Paths, normalize_path};
use crate::legacy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GmaEntry {
    pub(crate) offset: u64,
    pub(crate) size: u64,
    pub(crate) crc: u32,
}

impl GmaEntry {
    #[inline]
    pub fn size(&self) -> u64 {
        self.size
    }

    #[inline]
    pub fn crc(&self) -> u32 {
        self.crc
    }

    #[inline]
    pub fn offset(&self) -> u64 {
        self.offset
    }
}

enum Bytes {
    Mapped(Mmap),
    Owned(Vec<u8>),
}

impl Bytes {
    #[inline]
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Mapped(map) => map,
            Self::Owned(vec) => vec,
        }
    }
}

enum Storage {
    Plain(Bytes),
    Legacy { compressed: Bytes, data: OnceLock<Result<Vec<u8>, String>> },
}

struct Description {
    raw: String,
    text: Option<String>,
    addon_type: Option<String>,
    tags: Vec<String>,
}

impl Description {
    fn parse(raw: String) -> Self {
        let mut out = Self { raw, text: None, addon_type: None, tags: Vec::new() };
        let trimmed = out.raw.trim();
        if !trimmed.starts_with('{') {
            return out;
        }
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            return out;
        };
        out.text = Some(map.get("description").and_then(serde_json::Value::as_str).unwrap_or_default().to_owned());
        out.addon_type = map.get("type").and_then(serde_json::Value::as_str).map(str::to_owned);
        out.tags = map
            .get("tags")
            .and_then(serde_json::Value::as_array)
            .map(|tags| tags.iter().filter_map(serde_json::Value::as_str).map(str::to_owned).collect())
            .unwrap_or_default();
        out
    }
}

pub struct GmaArchive {
    path: Option<PathBuf>,
    storage: Storage,
    version: u8,
    steam_id: u64,
    timestamp: u64,
    required_content: Vec<String>,
    name: String,
    description: Description,
    author: String,
    addon_version: i32,
    index: PathIndex,
    entries: Vec<GmaEntry>,
    data_end: u64,
}

const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<GmaArchive>();
};

impl GmaArchive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, GmaError> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|e| GmaError::io(path, e))?;
        let map = unsafe { Mmap::map(&file) }.map_err(|e| GmaError::io(path, e))?;
        let mut archive = Self::from_storage(Bytes::Mapped(map))?;
        archive.path = Some(path.to_path_buf());
        Ok(archive)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, GmaError> {
        Self::from_storage(Bytes::Owned(bytes))
    }

    fn from_storage(bytes: Bytes) -> Result<Self, GmaError> {
        let data = bytes.as_slice();
        let (header, storage) = if data.starts_with(MAGIC) {
            let header = format::parse(data).map_err(|e| match e {
                ParseError::Incomplete => GmaError::corrupt("archive ends inside its header"),
                ParseError::Invalid(e) => e,
            })?;
            if header.data_end > data.len() as u64 {
                return Err(GmaError::corrupt(format!(
                    "file data ends at byte {} but the archive only has {} bytes (truncated download?)",
                    header.data_end,
                    data.len()
                )));
            }
            (header, Storage::Plain(bytes))
        } else if legacy::looks_like_lzma(data) {
            let header = legacy::read_header(data)?;
            if let Some(size) = legacy::declared_size(data)
                && header.data_end > size
            {
                return Err(GmaError::corrupt(format!(
                    "file data ends at byte {} but the archive only has {size} bytes",
                    header.data_end
                )));
            }
            let storage = Storage::Legacy { compressed: bytes, data: OnceLock::new() };
            (header, storage)
        } else {
            return Err(GmaError::BadMagic);
        };
        let Header {
            version,
            steam_id,
            timestamp,
            required_content,
            name,
            description,
            author,
            addon_version,
            index,
            entries,
            data_end,
        } = header;
        Ok(Self {
            path: None,
            storage,
            version,
            steam_id,
            timestamp,
            required_content,
            name,
            description: Description::parse(description),
            author,
            addon_version,
            index,
            entries,
            data_end,
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn is_legacy(&self) -> bool {
        matches!(self.storage, Storage::Legacy { .. })
    }

    pub fn format_version(&self) -> u8 {
        self.version
    }

    pub fn steam_id(&self) -> u64 {
        self.steam_id
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub fn required_content(&self) -> &[String] {
        &self.required_content
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        self.description.text.as_deref().unwrap_or(&self.description.raw)
    }

    pub fn raw_description(&self) -> &str {
        &self.description.raw
    }

    pub fn author(&self) -> &str {
        &self.author
    }

    pub fn addon_version(&self) -> i32 {
        self.addon_version
    }

    pub fn addon_type(&self) -> Option<&str> {
        self.description.addon_type.as_deref()
    }

    pub fn tags(&self) -> &[String] {
        &self.description.tags
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn data_size(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }

    pub fn contains(&self, path: &str) -> bool {
        self.find(path).is_some()
    }

    pub fn entry(&self, path: &str) -> Option<&GmaEntry> {
        self.find(path).map(|(_, entry)| entry)
    }

    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>, GmaError> {
        Ok(self.read_cow(path)?.map(Cow::into_owned))
    }

    pub fn read_into(&self, path: &str, buf: &mut Vec<u8>) -> Result<bool, GmaError> {
        buf.clear();
        let Some((i, entry)) = self.find(path) else {
            return Ok(false);
        };
        buf.extend_from_slice(self.slice(i, entry)?);
        Ok(true)
    }

    pub fn read_cow(&self, path: &str) -> Result<Option<Cow<'_, [u8]>>, GmaError> {
        let Some((i, entry)) = self.find(path) else {
            return Ok(None);
        };
        Ok(Some(Cow::Borrowed(self.slice(i, entry)?)))
    }

    pub fn read_checked(&self, path: &str) -> Result<Option<Vec<u8>>, GmaError> {
        let Some((i, entry)) = self.find(path) else {
            return Ok(None);
        };
        let data = self.slice(i, entry)?;
        if entry.crc == 0 {
            return Ok(Some(data.to_vec()));
        }
        let actual = crc32fast::hash(data);
        if actual != entry.crc {
            return Err(GmaError::CrcMismatch { path: self.index.path(i).to_owned(), expected: entry.crc, actual });
        }
        Ok(Some(data.to_vec()))
    }

    pub fn paths(&self) -> Paths<'_> {
        self.index.iter()
    }

    pub fn iter(&self) -> Entries<'_> {
        Entries { paths: self.index.iter(), entries: self.entries.iter() }
    }

    pub fn paths_with<'a>(&'a self, prefix: &str, extension: Option<&str>) -> impl Iterator<Item = &'a str> + use<'a> {
        let filter = PathFilter::new(prefix, extension);
        self.index.iter().filter(move |path| filter.matches_prefix(path) && filter.matches_ext(path))
    }

    #[inline]
    fn find(&self, path: &str) -> Option<(usize, &GmaEntry)> {
        let normalized = normalize_path(path);
        let i = self.index.find(&normalized)?;
        self.entries.get(i).map(|entry| (i, entry))
    }

    fn data(&self) -> Result<&[u8], GmaError> {
        match &self.storage {
            Storage::Plain(bytes) => Ok(bytes.as_slice()),
            Storage::Legacy { compressed, data } => data
                .get_or_init(|| legacy::decompress(compressed.as_slice(), self.data_end))
                .as_deref()
                .map_err(|msg| GmaError::Decompress(msg.clone())),
        }
    }

    fn slice(&self, i: usize, entry: &GmaEntry) -> Result<&[u8], GmaError> {
        let data = self.data()?;
        let range = usize::try_from(entry.offset)
            .ok()
            .and_then(|start| Some(start..start.checked_add(usize::try_from(entry.size).ok()?)?));
        range
            .and_then(|range| data.get(range))
            .ok_or_else(|| GmaError::OutOfBounds { path: self.index.path(i).to_owned() })
    }
}

impl fmt::Debug for GmaArchive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GmaArchive")
            .field("path", &self.path)
            .field("name", &self.name)
            .field("legacy", &self.is_legacy())
            .field("files", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl<'a> IntoIterator for &'a GmaArchive {
    type Item = (&'a str, &'a GmaEntry);
    type IntoIter = Entries<'a>;

    fn into_iter(self) -> Entries<'a> {
        self.iter()
    }
}

#[derive(Debug, Clone)]
pub struct Entries<'a> {
    paths: Paths<'a>,
    entries: std::slice::Iter<'a, GmaEntry>,
}

impl<'a> Iterator for Entries<'a> {
    type Item = (&'a str, &'a GmaEntry);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some((self.paths.next()?, self.entries.next()?))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.entries.size_hint()
    }
}

impl ExactSizeIterator for Entries<'_> {}
impl FusedIterator for Entries<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn description_json() {
        let d = Description::parse(
            "{\n\t\"description\": \"Hello\",\n\t\"type\": \"tool\",\n\t\"tags\": [\"build\", 3, \"scenic\"]\n}"
                .to_owned(),
        );
        assert_eq!(d.text.as_deref(), Some("Hello"));
        assert_eq!(d.addon_type.as_deref(), Some("tool"));
        assert_eq!(d.tags, ["build", "scenic"]);

        let plain = Description::parse("Just text".to_owned());
        assert_eq!(plain.text, None);
        assert_eq!(plain.addon_type, None);
        assert!(plain.tags.is_empty());

        let broken = Description::parse("{ not json".to_owned());
        assert_eq!(broken.text, None);

        let partial = Description::parse("{\"type\": \"map\", \"tags\": \"oops\"}".to_owned());
        assert_eq!(partial.text.as_deref(), Some(""));
        assert_eq!(partial.addon_type.as_deref(), Some("map"));
        assert!(partial.tags.is_empty());
    }

    #[test]
    fn entry_accessors() {
        let e = GmaEntry { offset: 10, size: 20, crc: 30 };
        assert_eq!((e.offset(), e.size(), e.crc()), (10, 20, 30));
    }
}
