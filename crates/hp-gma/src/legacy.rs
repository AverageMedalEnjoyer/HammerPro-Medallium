use std::io::Read;

use lzma_rust2::LzmaReader;

use crate::error::GmaError;
use crate::format::{self, Header, ParseError};

const LZMA_HEADER_LEN: usize = 13;
pub(crate) const MAX_DECOMPRESSED: u64 = 2 << 30;
const MAX_HEADER: usize = 64 << 20;
const FIRST_HEADER_CHUNK: usize = 64 << 10;
const MAX_PREALLOC: u64 = 256 << 20;
const MEM_LIMIT_KIB: u32 = 256 * 1024;

pub(crate) fn looks_like_lzma(data: &[u8]) -> bool {
    let Some(header) = data.get(..LZMA_HEADER_LEN) else {
        return false;
    };
    let props = header.first().copied().unwrap_or(u8::MAX);
    let dict = header.get(1..5).and_then(|b| <[u8; 4]>::try_from(b).ok()).map_or(0, u32::from_le_bytes);
    props < 225 && (dict.is_power_of_two() || (dict % 3 == 0 && (dict / 3).is_power_of_two()))
}

pub(crate) fn declared_size(data: &[u8]) -> Option<u64> {
    let bytes: [u8; 8] = data.get(5..LZMA_HEADER_LEN)?.try_into().ok()?;
    let size = u64::from_le_bytes(bytes);
    (size != u64::MAX).then_some(size)
}

fn reader(compressed: &[u8]) -> Result<LzmaReader<&[u8]>, GmaError> {
    LzmaReader::new_mem_limit(compressed, MEM_LIMIT_KIB, None).map_err(|e| GmaError::Decompress(e.to_string()))
}

pub(crate) fn read_header(compressed: &[u8]) -> Result<Header, GmaError> {
    if let Some(size) = declared_size(compressed)
        && size > MAX_DECOMPRESSED
    {
        return Err(GmaError::Decompress(format!(
            "declared size of {size} bytes exceeds the {MAX_DECOMPRESSED} byte limit"
        )));
    }
    let mut reader = reader(compressed)?;
    let mut buf = Vec::new();
    let mut target = FIRST_HEADER_CHUNK;
    loop {
        let want = target.saturating_sub(buf.len());
        let got =
            (&mut reader).take(want as u64).read_to_end(&mut buf).map_err(|e| GmaError::Decompress(e.to_string()))?;
        let at_end = got < want;
        match format::parse(&buf) {
            Ok(header) => return Ok(header),
            Err(ParseError::Invalid(e)) => return Err(e),
            Err(ParseError::Incomplete) if at_end => {
                return Err(GmaError::corrupt("archive ends inside its header"));
            }
            Err(ParseError::Incomplete) if target >= MAX_HEADER => {
                return Err(GmaError::corrupt("header is larger than 64 MiB"));
            }
            Err(ParseError::Incomplete) => target = (target * 4).min(MAX_HEADER),
        }
    }
}

pub(crate) fn decompress(compressed: &[u8], data_end: u64) -> Result<Vec<u8>, String> {
    let declared = declared_size(compressed);
    let reader = reader(compressed).map_err(|e| e.to_string())?;
    let prealloc = declared.unwrap_or(0).min(MAX_PREALLOC);
    let mut out = Vec::with_capacity(usize::try_from(prealloc).unwrap_or(0));
    reader.take(MAX_DECOMPRESSED + 1).read_to_end(&mut out).map_err(|e| e.to_string())?;
    let len = out.len() as u64;
    if len > MAX_DECOMPRESSED {
        return Err(format!("decompressed size exceeds the {MAX_DECOMPRESSED} byte limit"));
    }
    if let Some(declared) = declared
        && declared != len
    {
        return Err(format!("decompressed {len} bytes but the header declares {declared}"));
    }
    if data_end > len {
        return Err(format!("file data ends at byte {data_end} but the archive only has {len} bytes"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_sniffing() {
        let header = |props: u8, dict: u32| {
            let mut h = [0u8; 13];
            h[0] = props;
            h[1..5].copy_from_slice(&dict.to_le_bytes());
            h
        };
        assert!(!looks_like_lzma(b""));
        assert!(!looks_like_lzma(&header(0x5D, 1 << 25)[..12]));
        assert!(looks_like_lzma(&header(0x5D, 1 << 25)));
        assert!(looks_like_lzma(&header(0x5D, 3 << 20)));
        assert!(!looks_like_lzma(&header(0xE1, 1 << 25)));
        assert!(!looks_like_lzma(&header(0x5D, 0)));
        assert!(!looks_like_lzma(&header(0x5D, 12345)));
        assert!(!looks_like_lzma(b"PK\x03\x04 zip file"));
        let mut h = [0u8; 13];
        h[5..].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(declared_size(&h), None);
        h[5..].copy_from_slice(&42u64.to_le_bytes());
        assert_eq!(declared_size(&h), Some(42));
        assert_eq!(declared_size(&h[..12]), None);
    }

    #[test]
    fn garbage_streams_error() {
        let mut data = vec![0x5D, 0, 0, 1, 0];
        data.extend_from_slice(&100u64.to_le_bytes());
        data.extend_from_slice(&[0xFF; 32]);
        assert!(read_header(&data).is_err());
        assert!(decompress(&data, 0).is_err());
        let mut bomb = vec![0x5D, 0, 0, 1, 0];
        bomb.extend_from_slice(&(MAX_DECOMPRESSED + 1).to_le_bytes());
        bomb.extend_from_slice(&[0; 32]);
        assert!(matches!(read_header(&bomb), Err(GmaError::Decompress(_))));
    }
}
