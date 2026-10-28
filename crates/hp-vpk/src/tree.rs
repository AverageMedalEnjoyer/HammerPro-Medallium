use std::borrow::Cow;

use crate::archive::VpkEntry;
use crate::error::VpkError;
use crate::index::{Added, PathIndex, PathIndexBuilder};

pub(crate) const SIGNATURE: u32 = 0x55AA_1234;
pub(crate) const DIR_ARCHIVE: u16 = 0x7FFF;
const TERMINATOR: u16 = 0xFFFF;
const V1_HEADER_LEN: usize = 12;
const V2_HEADER_LEN: usize = 28;
const AVG_RECORD_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Header {
    pub version: u32,
    pub tree_start: usize,
    pub tree_end: usize,
}

pub(crate) fn parse_header(data: &[u8]) -> Result<Header, VpkError> {
    let mut cur = Cursor::new(data);
    let signature = cur.u32().map_err(|_| VpkError::corrupt("file too small for a VPK header"))?;
    if signature != SIGNATURE {
        return Err(VpkError::BadSignature(signature));
    }
    let version = cur.u32().map_err(|_| VpkError::corrupt("truncated VPK header"))?;
    let tree_len = cur.u32().map_err(|_| VpkError::corrupt("truncated VPK header"))?;
    let header_len = match version {
        1 => V1_HEADER_LEN,
        2 => V2_HEADER_LEN,
        other => return Err(VpkError::UnsupportedVersion(other)),
    };
    if data.len() < header_len {
        return Err(VpkError::corrupt("truncated VPK header"));
    }
    let tree_end = usize::try_from(tree_len)
        .ok()
        .and_then(|len| header_len.checked_add(len))
        .filter(|&end| end <= data.len())
        .ok_or_else(|| {
            VpkError::corrupt(format!(
                "directory tree of {tree_len} bytes extends past the end of the {} byte file",
                data.len()
            ))
        })?;
    Ok(Header { version, tree_start: header_len, tree_end })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtRange {
    pub ext: Box<str>,
    pub start: usize,
    pub end: usize,
}

pub(crate) struct Tree {
    pub index: PathIndex,
    pub entries: Vec<VpkEntry>,
    pub ext_ranges: Vec<ExtRange>,
    pub part_count: usize,
}

pub(crate) fn parse_tree(data: &[u8], header: &Header) -> Result<Tree, VpkError> {
    let tree = data
        .get(header.tree_start..header.tree_end)
        .ok_or_else(|| VpkError::corrupt("directory tree out of bounds"))?;
    let estimated = tree.len() / AVG_RECORD_LEN;
    let mut builder = PathIndexBuilder::with_capacity(estimated, tree.len() * 2);
    let mut entries: Vec<VpkEntry> = Vec::with_capacity(estimated);
    let mut ext_ranges: Vec<ExtRange> = Vec::new();
    let mut part_count = 0usize;
    let mut cur = Cursor::new(tree);

    loop {
        let raw_ext = cur.cstr()?;
        if raw_ext.is_empty() {
            break;
        }
        let ext = component(raw_ext);
        let range_start = entries.len();
        loop {
            let raw_dir = cur.cstr()?;
            if raw_dir.is_empty() {
                break;
            }
            let dir = component(raw_dir);
            loop {
                let raw_name = cur.cstr()?;
                if raw_name.is_empty() {
                    break;
                }
                let crc = cur.u32()?;
                let preload_len = cur.u16()?;
                let archive_index = cur.u16()?;
                let offset = cur.u32()?;
                let length = cur.u32()?;
                let terminator = cur.u16()?;
                if terminator != TERMINATOR {
                    return Err(VpkError::corrupt(format!(
                        "bad entry terminator {terminator:#06x} at tree offset {}",
                        cur.pos
                    )));
                }
                let preload_start = cur.skip(usize::from(preload_len))?;
                let preload_offset =
                    u32::try_from(preload_start).map_err(|_| VpkError::corrupt("directory tree too large"))?;

                builder.push(&dir);
                builder.push_slash();
                builder.push(&component(raw_name));
                if !ext.is_empty() {
                    builder.push_char('.');
                    builder.push(&ext);
                }
                if let Added::New(_) = builder.finish() {
                    if archive_index != DIR_ARCHIVE {
                        part_count = part_count.max(usize::from(archive_index) + 1);
                    }
                    entries.push(VpkEntry { crc, preload_offset, preload_len, archive_index, offset, length });
                }
            }
        }
        if entries.len() > range_start {
            ext_ranges.push(ExtRange {
                ext: ext.to_ascii_lowercase().into_boxed_str(),
                start: range_start,
                end: entries.len(),
            });
        }
    }

    entries.shrink_to_fit();
    Ok(Tree { index: builder.build(), entries, ext_ranges, part_count })
}

#[inline]
fn component(raw: &[u8]) -> Cow<'_, str> {
    if raw == b" " { Cow::Borrowed("") } else { String::from_utf8_lossy(raw) }
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn truncated(&self) -> VpkError {
        VpkError::corrupt(format!("directory tree truncated at offset {}", self.pos))
    }

    #[inline]
    fn cstr(&mut self) -> Result<&'a [u8], VpkError> {
        let rest = self.data.get(self.pos..).unwrap_or_default();
        let len = memchr::memchr(0, rest).ok_or_else(|| self.truncated())?;
        let s = rest.get(..len).ok_or_else(|| self.truncated())?;
        self.pos += len + 1;
        Ok(s)
    }

    #[inline]
    fn array<const N: usize>(&mut self) -> Result<[u8; N], VpkError> {
        let end = self.pos.checked_add(N).ok_or_else(|| self.truncated())?;
        let bytes = self.data.get(self.pos..end).ok_or_else(|| self.truncated())?;
        let array = <[u8; N]>::try_from(bytes).map_err(|_| self.truncated())?;
        self.pos = end;
        Ok(array)
    }

    #[inline]
    fn u16(&mut self) -> Result<u16, VpkError> {
        self.array().map(u16::from_le_bytes)
    }

    #[inline]
    fn u32(&mut self) -> Result<u32, VpkError> {
        self.array().map(u32::from_le_bytes)
    }

    #[inline]
    fn skip(&mut self, n: usize) -> Result<usize, VpkError> {
        let start = self.pos;
        let end = start.checked_add(n).filter(|&end| end <= self.data.len()).ok_or_else(|| self.truncated())?;
        self.pos = end;
        Ok(start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(version: u32, tree: &[u8], extra: usize) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&SIGNATURE.to_le_bytes());
        out.extend_from_slice(&version.to_le_bytes());
        out.extend_from_slice(&(tree.len() as u32).to_le_bytes());
        if version == 2 {
            out.extend_from_slice(&[0; 16]);
        }
        out.extend_from_slice(tree);
        out.extend(std::iter::repeat_n(0u8, extra));
        out
    }

    fn record(out: &mut Vec<u8>, name: &str, preload: &[u8], archive: u16, offset: u32, len: u32) {
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        out.extend_from_slice(&0x1234_5678u32.to_le_bytes());
        out.extend_from_slice(&(preload.len() as u16).to_le_bytes());
        out.extend_from_slice(&archive.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&TERMINATOR.to_le_bytes());
        out.extend_from_slice(preload);
    }

    #[test]
    fn header_errors() {
        assert!(matches!(parse_header(&[1, 2]), Err(VpkError::Corrupt(_))));
        assert!(matches!(parse_header(&[0; 12]), Err(VpkError::BadSignature(0))));
        let mut v3 = header(1, &[], 0);
        v3[4] = 3;
        assert!(matches!(parse_header(&v3), Err(VpkError::UnsupportedVersion(3))));
        let mut huge = header(2, &[0], 0);
        huge[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(parse_header(&huge), Err(VpkError::Corrupt(_))));
        let short_v2 = &header(2, &[], 0)[..20];
        assert!(matches!(parse_header(short_v2), Err(VpkError::Corrupt(_))));
    }

    #[test]
    fn minimal_trees() {
        let data = header(1, &[0], 0);
        let h = parse_header(&data).unwrap();
        assert_eq!(h, Header { version: 1, tree_start: 12, tree_end: 13 });
        let tree = parse_tree(&data, &h).unwrap();
        assert_eq!(tree.entries.len(), 0);
        assert_eq!(tree.part_count, 0);

        let mut t = Vec::new();
        t.extend_from_slice(b"vmt\0materials/x\0");
        record(&mut t, "a", b"pre", 3, 10, 20);
        record(&mut t, "A", b"", DIR_ARCHIVE, 0, 0);
        t.push(0);
        t.extend_from_slice(b" \0");
        record(&mut t, "b", b"", 0, 0, 1);
        t.extend_from_slice(b"\0\0");
        t.extend_from_slice(b" \0 \0");
        record(&mut t, "README", b"", DIR_ARCHIVE, 0, 0);
        t.extend_from_slice(b"\0\0\0");
        let data = header(2, &t, 4);
        let h = parse_header(&data).unwrap();
        let tree = parse_tree(&data, &h).unwrap();
        let paths: Vec<&str> = tree.index.iter().collect();
        assert_eq!(paths, ["materials/x/a.vmt", "b.vmt", "readme"]);
        assert_eq!(tree.part_count, 4);
        assert_eq!(tree.entries[0].preload_len, 3);
        let off = tree.entries[0].preload_offset as usize;
        assert_eq!(&t[off..off + 3], b"pre");
        assert_eq!(tree.ext_ranges.len(), 2);
        assert_eq!(&*tree.ext_ranges[0].ext, "vmt");
        assert_eq!((tree.ext_ranges[0].start, tree.ext_ranges[0].end), (0, 2));
        assert_eq!(&*tree.ext_ranges[1].ext, "");
    }

    #[test]
    fn truncated_and_bad_trees_error() {
        let mut t = Vec::new();
        t.extend_from_slice(b"vmt\0dir\0");
        record(&mut t, "a", b"xyz", 0, 0, 0);
        t.extend_from_slice(b"\0\0\0");
        for cut in 0..t.len() {
            let data = header(1, &t[..cut], 0);
            let h = parse_header(&data).unwrap();
            assert!(parse_tree(&data, &h).is_err(), "cut {cut}");
        }
        let mut bad = t.clone();
        let term_pos = b"vmt\0dir\0a\0".len() + 16;
        bad[term_pos] = 0;
        let data = header(1, &bad, 0);
        let h = parse_header(&data).unwrap();
        assert!(matches!(parse_tree(&data, &h), Err(VpkError::Corrupt(_))));
        let data = header(1, &t, 0);
        let h = parse_header(&data).unwrap();
        assert_eq!(parse_tree(&data, &h).unwrap().entries.len(), 1);
    }
}
