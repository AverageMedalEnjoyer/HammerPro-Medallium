use crate::archive::GmaEntry;
use crate::error::GmaError;
use crate::index::{Added, PathIndex, PathIndexBuilder};

pub(crate) const MAGIC: &[u8; 4] = b"GMAD";
pub(crate) const MAX_VERSION: u8 = 3;

#[derive(Debug)]
pub(crate) enum ParseError {
    Incomplete,
    Invalid(GmaError),
}

impl From<GmaError> for ParseError {
    fn from(e: GmaError) -> Self {
        Self::Invalid(e)
    }
}

pub(crate) struct Header {
    pub version: u8,
    pub steam_id: u64,
    pub timestamp: u64,
    pub required_content: Vec<String>,
    pub name: String,
    pub description: String,
    pub author: String,
    pub addon_version: i32,
    pub index: PathIndex,
    pub entries: Vec<GmaEntry>,
    pub data_end: u64,
}

pub(crate) fn parse(data: &[u8]) -> Result<Header, ParseError> {
    let mut cur = Cursor { data, pos: 0 };
    let magic: [u8; 4] = cur.array()?;
    if &magic != MAGIC {
        return Err(GmaError::BadMagic.into());
    }
    let [version] = cur.array()?;
    if version > MAX_VERSION {
        return Err(GmaError::UnsupportedVersion(version).into());
    }
    let steam_id = cur.u64()?;
    let timestamp = cur.u64()?;
    let mut required_content = Vec::new();
    if version > 1 {
        loop {
            let s = cur.cstr()?;
            if s.is_empty() {
                break;
            }
            required_content.push(lossy(s));
        }
    }
    let name = lossy(cur.cstr()?);
    let description = lossy(cur.cstr()?);
    let author = lossy(cur.cstr()?);
    let addon_version = i32::from_le_bytes(cur.array()?);

    let mut builder = PathIndexBuilder::with_capacity(64, 4096);
    let mut entries: Vec<GmaEntry> = Vec::new();
    let mut data_len: u64 = 0;
    loop {
        let number = cur.u32()?;
        if number == 0 {
            break;
        }
        let raw_name = cur.cstr()?;
        let size = i64::from_le_bytes(cur.array()?);
        let crc = cur.u32()?;
        let size = u64::try_from(size)
            .map_err(|_| GmaError::Corrupt(format!("file #{number} has a negative size ({size})")))?;
        let offset = data_len;
        data_len = data_len.checked_add(size).ok_or_else(|| GmaError::Corrupt("file sizes overflow".to_owned()))?;
        builder.push(&String::from_utf8_lossy(raw_name));
        if let Added::New(_) = builder.finish() {
            entries.push(GmaEntry { offset, size, crc });
        }
    }

    let data_start = cur.pos as u64;
    let data_end =
        data_start.checked_add(data_len).ok_or_else(|| GmaError::Corrupt("file sizes overflow".to_owned()))?;
    for entry in &mut entries {
        entry.offset += data_start;
    }
    entries.shrink_to_fit();
    Ok(Header {
        version,
        steam_id,
        timestamp,
        required_content,
        name,
        description,
        author,
        addon_version,
        index: builder.build(),
        entries,
        data_end,
    })
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    #[inline]
    fn cstr(&mut self) -> Result<&'a [u8], ParseError> {
        let rest = self.data.get(self.pos..).unwrap_or_default();
        let len = memchr::memchr(0, rest).ok_or(ParseError::Incomplete)?;
        let s = rest.get(..len).ok_or(ParseError::Incomplete)?;
        self.pos += len + 1;
        Ok(s)
    }

    #[inline]
    fn array<const N: usize>(&mut self) -> Result<[u8; N], ParseError> {
        let end = self.pos.checked_add(N).ok_or(ParseError::Incomplete)?;
        let bytes = self.data.get(self.pos..end).ok_or(ParseError::Incomplete)?;
        let array = <[u8; N]>::try_from(bytes).map_err(|_| ParseError::Incomplete)?;
        self.pos = end;
        Ok(array)
    }

    #[inline]
    fn u32(&mut self) -> Result<u32, ParseError> {
        self.array().map(u32::from_le_bytes)
    }

    #[inline]
    fn u64(&mut self) -> Result<u64, ParseError> {
        self.array().map(u64::from_le_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gma(version: u8, files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(version);
        out.extend_from_slice(&7u64.to_le_bytes());
        out.extend_from_slice(&9u64.to_le_bytes());
        if version > 1 {
            out.extend_from_slice(b"req\0\0");
        }
        out.extend_from_slice(b"Name\0Desc\0Author\0");
        out.extend_from_slice(&1i32.to_le_bytes());
        for (i, (name, data)) in files.iter().enumerate() {
            out.extend_from_slice(&(i as u32 + 1).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.push(0);
            out.extend_from_slice(&(data.len() as i64).to_le_bytes());
            out.extend_from_slice(&crc32fast::hash(data).to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        for (_, data) in files {
            out.extend_from_slice(data);
        }
        out
    }

    #[test]
    fn parses_header_and_offsets() {
        for version in 1..=3 {
            let data = gma(version, &[("A/B.txt", b"hello"), ("c.lua", b"x"), ("a\\b.TXT", b"dup")]);
            let h = parse(&data).unwrap();
            assert_eq!(h.version, version);
            assert_eq!((h.steam_id, h.timestamp, h.addon_version), (7, 9, 1));
            assert_eq!(h.required_content, if version > 1 { vec!["req".to_owned()] } else { vec![] });
            assert_eq!((h.name.as_str(), h.description.as_str(), h.author.as_str()), ("Name", "Desc", "Author"));
            let paths: Vec<&str> = h.index.iter().collect();
            assert_eq!(paths, ["a/b.txt", "c.lua"]);
            let e0 = h.entries[0];
            assert_eq!(&data[e0.offset as usize..][..e0.size as usize], b"hello");
            let e1 = h.entries[1];
            assert_eq!(&data[e1.offset as usize..][..e1.size as usize], b"x");
            assert_eq!(h.data_end as usize, data.len());
        }
    }

    #[test]
    fn every_prefix_is_incomplete() {
        let data = gma(3, &[("a.txt", b"0123456789")]);
        let header_len = data.len() - 10;
        for cut in 0..header_len {
            assert!(matches!(parse(&data[..cut]), Err(ParseError::Incomplete)), "cut {cut}");
        }
        assert!(parse(&data[..header_len]).is_ok());
    }

    #[test]
    fn invalid_headers() {
        assert!(matches!(parse(b"GMAX\x03"), Err(ParseError::Invalid(GmaError::BadMagic))));
        assert!(matches!(parse(b"GMAD\x04"), Err(ParseError::Invalid(GmaError::UnsupportedVersion(4)))));
        let mut neg = gma(3, &[("a.txt", b"0123")]);
        let size_pos = neg.len() - 4 - 4 - 4 - 8;
        neg[size_pos..size_pos + 8].copy_from_slice(&(-5i64).to_le_bytes());
        assert!(matches!(parse(&neg), Err(ParseError::Invalid(GmaError::Corrupt(_)))));
        let mut huge = gma(3, &[("a.txt", b"0123"), ("b.txt", b"")]);
        let first = 4 + 1 + 16 + 5 + 17 + 4 + 4 + 6;
        huge[first..first + 8].copy_from_slice(&i64::MAX.to_le_bytes());
        let second = first + 8 + 4 + 4 + 6;
        huge[second..second + 8].copy_from_slice(&i64::MAX.to_le_bytes());
        assert!(matches!(parse(&huge), Err(ParseError::Invalid(GmaError::Corrupt(_)))));
    }
}
