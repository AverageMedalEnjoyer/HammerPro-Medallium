use std::io::Read;

use lzma_rust2::{LzmaOptions, LzmaReader, LzmaWriter};

use crate::error::{BspError, Result};

pub const VALVE_LZMA_ID: [u8; 4] = *b"LZMA";
pub const VALVE_HEADER_LEN: usize = 17;
pub const ZIP_HEADER_LEN: usize = 9;
pub const MAX_DECOMPRESSED: u64 = 1 << 30;
const MEM_LIMIT_KIB: u32 = 256 * 1024;

fn props_dict(props: &[u8]) -> Result<(u8, u32)> {
    let [p, d0, d1, d2, d3] = <[u8; 5]>::try_from(props).map_err(|_| BspError::Decompress("short header".into()))?;
    if p >= 225 {
        return Err(BspError::Decompress(format!("invalid properties byte {p}")));
    }
    let dict = u32::from_le_bytes([d0, d1, d2, d3]);
    let need = lzma_rust2::lzma_get_memory_usage_by_props(dict, p).map_err(|e| BspError::Decompress(e.to_string()))?;
    if need > MEM_LIMIT_KIB {
        return Err(BspError::Decompress(format!("dictionary of {dict} bytes is too large")));
    }
    Ok((p, dict))
}

pub(crate) fn decompress_raw(stream: &[u8], props: &[u8], size: u64) -> Result<Vec<u8>> {
    if size > MAX_DECOMPRESSED {
        return Err(BspError::TooLarge(format!("{size} bytes once decompressed")));
    }
    let (p, dict) = props_dict(props)?;
    let mut reader =
        LzmaReader::new_with_props(stream, size, p, dict, None).map_err(|e| BspError::Decompress(e.to_string()))?;
    let cap = size.min(stream.len() as u64 * 8 + 4096) as usize;
    let mut out = Vec::with_capacity(cap);
    reader.read_to_end(&mut out).map_err(|e| BspError::Decompress(e.to_string()))?;
    if out.len() as u64 != size {
        return Err(BspError::Decompress(format!("expected {size} bytes, got {}", out.len())));
    }
    Ok(out)
}

pub fn is_valve_lzma(data: &[u8]) -> bool {
    data.len() >= VALVE_HEADER_LEN && data[..4] == VALVE_LZMA_ID
}

pub fn decompress_valve(data: &[u8]) -> Result<Vec<u8>> {
    if !is_valve_lzma(data) {
        return Err(BspError::Decompress("missing LZMA header".into()));
    }
    let actual = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let lzma_size = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let stream = data
        .get(VALVE_HEADER_LEN..VALVE_HEADER_LEN.saturating_add(lzma_size))
        .ok_or_else(|| BspError::Decompress("compressed data is truncated".into()))?;
    decompress_raw(stream, &data[12..17], u64::from(actual))
}

fn options(level: u32) -> LzmaOptions {
    let mut opts = LzmaOptions::with_preset(level.min(9));
    opts.dict_size = opts.dict_size.min(1 << 24);
    opts
}

fn compress_stream(data: &[u8], level: u32) -> Result<(u8, u32, Vec<u8>)> {
    let opts = options(level);
    let dict = opts.dict_size;
    let mut writer =
        LzmaWriter::new(Vec::with_capacity(data.len() / 2 + 64), &opts, false, false, Some(data.len() as u64))
            .map_err(|e| BspError::Compress(e.to_string()))?;
    let props = writer.props();
    std::io::Write::write_all(&mut writer, data).map_err(|e| BspError::Compress(e.to_string()))?;
    let out = writer.finish().map_err(|e| BspError::Compress(e.to_string()))?;
    Ok((props, dict, out))
}

pub fn compress_valve(data: &[u8], level: u32) -> Result<Vec<u8>> {
    let len = u32::try_from(data.len()).map_err(|_| BspError::TooLarge("lump larger than 4 GiB".into()))?;
    let (props, dict, stream) = compress_stream(data, level)?;
    let stream_len = u32::try_from(stream.len()).map_err(|_| BspError::TooLarge("compressed lump".into()))?;
    let mut out = Vec::with_capacity(VALVE_HEADER_LEN + stream.len());
    out.extend_from_slice(&VALVE_LZMA_ID);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&stream_len.to_le_bytes());
    out.push(props);
    out.extend_from_slice(&dict.to_le_bytes());
    out.extend_from_slice(&stream);
    Ok(out)
}

pub(crate) fn compress_zip(data: &[u8], level: u32) -> Result<Vec<u8>> {
    let (props, dict, stream) = compress_stream(data, level)?;
    let mut out = Vec::with_capacity(ZIP_HEADER_LEN + stream.len());
    out.extend_from_slice(&[9, 20, 5, 0, props]);
    out.extend_from_slice(&dict.to_le_bytes());
    out.extend_from_slice(&stream);
    Ok(out)
}

pub(crate) fn decompress_zip(data: &[u8], size: u64) -> Result<Vec<u8>> {
    if data.len() < 4 {
        return Err(BspError::Decompress("missing LZMA header".into()));
    }
    let props_len = usize::from(u16::from_le_bytes([data[2], data[3]]));
    if props_len != 5 {
        return Err(BspError::Decompress(format!("unexpected LZMA properties size {props_len}")));
    }
    let props = data.get(4..9).ok_or_else(|| BspError::Decompress("short LZMA header".into()))?;
    decompress_raw(&data[ZIP_HEADER_LEN.min(data.len())..], props, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valve_round_trip() {
        let data: Vec<u8> = (0..100_000u32).flat_map(|i| (i % 251).to_le_bytes()).collect();
        let packed = compress_valve(&data, 6).unwrap();
        assert!(packed.len() < data.len() / 4);
        assert_eq!(decompress_valve(&packed).unwrap(), data);
    }

    #[test]
    fn zip_round_trip() {
        let data = b"hello hello hello hello hello world".repeat(100);
        let packed = compress_zip(&data, 6).unwrap();
        assert_eq!(decompress_zip(&packed, data.len() as u64).unwrap(), data);
    }

    #[test]
    fn hostile_headers_fail_cleanly() {
        let mut bad = compress_valve(b"abcdefabcdef", 6).unwrap();
        bad[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decompress_valve(&bad).is_err());
        let mut bad = compress_valve(b"abcdefabcdef", 6).unwrap();
        bad[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decompress_valve(&bad).is_err());
        let mut bad = compress_valve(b"abcdefabcdef", 6).unwrap();
        bad[13..17].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decompress_valve(&bad).is_err());
        assert!(decompress_zip(&[9, 20, 5], 10).is_err());
        assert!(decompress_zip(&[9, 20, 5, 0, 93, 0, 0, 1, 0, 1, 2, 3], 1 << 40).is_err());
        assert!(decompress_valve(b"LZMA").is_err());
    }
}
