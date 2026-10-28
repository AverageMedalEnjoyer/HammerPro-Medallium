use std::borrow::Cow;
use std::fmt;
use std::fs::File;
use std::iter::FusedIterator;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use memmap2::Mmap;

use crate::error::VpkError;
use crate::index::{PathFilter, PathIndex, Paths, normalize_path};
use crate::tree::{self, DIR_ARCHIVE, ExtRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VpkEntry {
    pub(crate) crc: u32,
    pub(crate) preload_offset: u32,
    pub(crate) preload_len: u16,
    pub(crate) archive_index: u16,
    pub(crate) offset: u32,
    pub(crate) length: u32,
}

impl VpkEntry {
    #[inline]
    pub fn size(&self) -> u64 {
        u64::from(self.preload_len) + u64::from(self.length)
    }

    #[inline]
    pub fn crc(&self) -> u32 {
        self.crc
    }

    #[inline]
    pub fn preload_len(&self) -> u16 {
        self.preload_len
    }

    #[inline]
    pub fn archive_index(&self) -> Option<u16> {
        (self.archive_index != DIR_ARCHIVE).then_some(self.archive_index)
    }

    #[inline]
    pub fn archive_offset(&self) -> u32 {
        self.offset
    }

    #[inline]
    pub fn archive_len(&self) -> u32 {
        self.length
    }
}

pub struct VpkArchive {
    path: PathBuf,
    part_stem: PathBuf,
    version: u32,
    dir: Mmap,
    tree_start: usize,
    data_start: usize,
    index: PathIndex,
    entries: Vec<VpkEntry>,
    ext_ranges: Vec<ExtRange>,
    parts: Box<[OnceLock<Mmap>]>,
}

const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<VpkArchive>();
};

fn map_file(path: &Path) -> Result<Mmap, VpkError> {
    let file = File::open(path).map_err(|e| VpkError::io(path, e))?;
    unsafe { Mmap::map(&file) }.map_err(|e| VpkError::io(path, e))
}

fn part_stem(path: &Path) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    let stem_len = if lower.ends_with("_dir.vpk") {
        name.len() - "_dir.vpk".len()
    } else if lower.ends_with(".vpk") {
        name.len() - ".vpk".len()
    } else {
        name.len()
    };
    let stem = name.get(..stem_len).unwrap_or(&name);
    path.with_file_name(stem)
}

impl VpkArchive {
    pub fn open(dir_path: impl AsRef<Path>) -> Result<Self, VpkError> {
        let path = dir_path.as_ref().to_path_buf();
        let dir = map_file(&path)?;
        let header = tree::parse_header(&dir)?;
        let tree = tree::parse_tree(&dir, &header)?;
        let parts = std::iter::repeat_with(OnceLock::new).take(tree.part_count).collect();
        Ok(Self {
            part_stem: part_stem(&path),
            path,
            version: header.version,
            dir,
            tree_start: header.tree_start,
            data_start: header.tree_end,
            index: tree.index,
            entries: tree.entries,
            ext_ranges: tree.ext_ranges,
            parts,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn part_count(&self) -> usize {
        self.parts.len()
    }

    pub fn part_path(&self, index: u16) -> PathBuf {
        let mut name = self.part_stem.clone().into_os_string();
        name.push(format!("_{index:03}.vpk"));
        PathBuf::from(name)
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, path: &str) -> bool {
        self.find(path).is_some()
    }

    pub fn entry(&self, path: &str) -> Option<&VpkEntry> {
        self.find(path).map(|(_, entry)| entry)
    }

    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>, VpkError> {
        let mut out = Vec::new();
        Ok(self.read_into(path, &mut out)?.then_some(out))
    }

    pub fn read_into(&self, path: &str, buf: &mut Vec<u8>) -> Result<bool, VpkError> {
        buf.clear();
        let Some((i, entry)) = self.find(path) else {
            return Ok(false);
        };
        let (preload, data) = self.slices(i, entry)?;
        buf.reserve_exact(preload.len() + data.len());
        buf.extend_from_slice(preload);
        buf.extend_from_slice(data);
        Ok(true)
    }

    pub fn read_cow(&self, path: &str) -> Result<Option<Cow<'_, [u8]>>, VpkError> {
        let Some((i, entry)) = self.find(path) else {
            return Ok(None);
        };
        let (preload, data) = self.slices(i, entry)?;
        Ok(Some(if preload.is_empty() {
            Cow::Borrowed(data)
        } else if data.is_empty() {
            Cow::Borrowed(preload)
        } else {
            let mut joined = Vec::with_capacity(preload.len() + data.len());
            joined.extend_from_slice(preload);
            joined.extend_from_slice(data);
            Cow::Owned(joined)
        }))
    }

    pub fn read_checked(&self, path: &str) -> Result<Option<Vec<u8>>, VpkError> {
        let Some((i, entry)) = self.find(path) else {
            return Ok(None);
        };
        let (preload, data) = self.slices(i, entry)?;
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(preload);
        hasher.update(data);
        let actual = hasher.finalize();
        if actual != entry.crc {
            return Err(VpkError::CrcMismatch { path: self.index.path(i).to_owned(), expected: entry.crc, actual });
        }
        let mut out = Vec::with_capacity(preload.len() + data.len());
        out.extend_from_slice(preload);
        out.extend_from_slice(data);
        Ok(Some(out))
    }

    pub fn paths(&self) -> Paths<'_> {
        self.index.iter()
    }

    pub fn iter(&self) -> Entries<'_> {
        Entries { paths: self.index.iter(), entries: self.entries.iter() }
    }

    pub fn paths_with<'a>(&'a self, prefix: &str, extension: Option<&str>) -> impl Iterator<Item = &'a str> + use<'a> {
        let filter = PathFilter::new(prefix, extension);
        let ranges: Vec<std::ops::Range<usize>> = self
            .ext_ranges
            .iter()
            .filter(|r| filter.ext().is_none_or(|ext| &*r.ext == ext))
            .map(|r| r.start..r.end)
            .collect();
        ranges.into_iter().flatten().map(|i| self.index.path(i)).filter(move |path| filter.matches_prefix(path))
    }

    #[inline]
    fn find(&self, path: &str) -> Option<(usize, &VpkEntry)> {
        let normalized = normalize_path(path);
        let i = self.index.find(&normalized)?;
        self.entries.get(i).map(|entry| (i, entry))
    }

    fn slices(&self, i: usize, entry: &VpkEntry) -> Result<(&[u8], &[u8]), VpkError> {
        let preload = self
            .tree_start
            .checked_add(entry.preload_offset as usize)
            .and_then(|start| self.dir.get(start..))
            .and_then(|rest| rest.get(..usize::from(entry.preload_len)))
            .ok_or_else(|| self.out_of_bounds(i, self.path.clone()))?;
        if entry.length == 0 {
            return Ok((preload, &[]));
        }
        let (map, base) = if entry.archive_index == DIR_ARCHIVE {
            (&self.dir, self.data_start)
        } else {
            (self.part(entry.archive_index)?, 0)
        };
        let data = base
            .checked_add(entry.offset as usize)
            .and_then(|start| Some(start..start.checked_add(entry.length as usize)?))
            .and_then(|range| map.get(range))
            .ok_or_else(|| {
                let archive = if entry.archive_index == DIR_ARCHIVE {
                    self.path.clone()
                } else {
                    self.part_path(entry.archive_index)
                };
                self.out_of_bounds(i, archive)
            })?;
        Ok((preload, data))
    }

    fn out_of_bounds(&self, i: usize, archive: PathBuf) -> VpkError {
        VpkError::OutOfBounds { path: self.index.path(i).to_owned(), archive }
    }

    fn part(&self, index: u16) -> Result<&Mmap, VpkError> {
        let cell = self
            .parts
            .get(usize::from(index))
            .ok_or_else(|| VpkError::corrupt(format!("archive index {index} out of range")))?;
        if let Some(map) = cell.get() {
            return Ok(map);
        }
        let map = map_file(&self.part_path(index))?;
        Ok(cell.get_or_init(|| map))
    }
}

impl fmt::Debug for VpkArchive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VpkArchive")
            .field("path", &self.path)
            .field("version", &self.version)
            .field("files", &self.entries.len())
            .field("parts", &self.parts.len())
            .finish_non_exhaustive()
    }
}

impl<'a> IntoIterator for &'a VpkArchive {
    type Item = (&'a str, &'a VpkEntry);
    type IntoIter = Entries<'a>;

    fn into_iter(self) -> Entries<'a> {
        self.iter()
    }
}

#[derive(Debug, Clone)]
pub struct Entries<'a> {
    paths: Paths<'a>,
    entries: std::slice::Iter<'a, VpkEntry>,
}

impl<'a> Iterator for Entries<'a> {
    type Item = (&'a str, &'a VpkEntry);

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
    fn part_stem_variants() {
        let p = Path::new("dir/pak01_dir.vpk");
        assert_eq!(part_stem(p), Path::new("dir/pak01"));
        assert_eq!(part_stem(Path::new("dir/Pak01_DIR.VPK")), Path::new("dir/Pak01"));
        assert_eq!(part_stem(Path::new("single.vpk")), Path::new("single"));
        assert_eq!(part_stem(Path::new("weird")), Path::new("weird"));
    }

    #[test]
    fn entry_accessors() {
        let e =
            VpkEntry { crc: 7, preload_offset: 0, preload_len: 10, archive_index: DIR_ARCHIVE, offset: 5, length: 20 };
        assert_eq!(e.size(), 30);
        assert_eq!(e.crc(), 7);
        assert_eq!(e.archive_index(), None);
        assert_eq!(e.archive_offset(), 5);
        assert_eq!(e.archive_len(), 20);
        let e = VpkEntry { archive_index: 2, ..e };
        assert_eq!(e.archive_index(), Some(2));
    }
}
