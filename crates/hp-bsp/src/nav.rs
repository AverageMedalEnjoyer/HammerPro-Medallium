use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::error::{BspError, Result};

pub const NAV_MAGIC: u32 = 0xFEED_FACE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavHeader {
    pub version: u32,
    pub subversion: u32,
    pub bsp_size: Option<u32>,
    pub analyzed: Option<bool>,
    pub bsp_size_offset: Option<usize>,
}

pub fn read_nav_header(data: &[u8]) -> Result<NavHeader> {
    let word = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if word(0) != Some(NAV_MAGIC) {
        return Err(BspError::corrupt("not a navigation mesh file"));
    }
    let version = word(4).ok_or_else(|| BspError::corrupt("truncated nav header"))?;
    if version == 0 || version > 64 {
        return Err(BspError::corrupt(format!("unsupported nav version {version}")));
    }
    let mut at = 8;
    let subversion = if version >= 10 {
        let s = word(at).ok_or_else(|| BspError::corrupt("truncated nav header"))?;
        at += 4;
        s
    } else {
        0
    };
    let (bsp_size, bsp_size_offset) = if version >= 4 {
        let s = word(at).ok_or_else(|| BspError::corrupt("truncated nav header"))?;
        let ofs = at;
        at += 4;
        (Some(s), Some(ofs))
    } else {
        (None, None)
    };
    let analyzed = if version >= 14 { data.get(at).map(|&b| b != 0) } else { None };
    Ok(NavHeader { version, subversion, bsp_size, analyzed, bsp_size_offset })
}

pub fn fixup_bsp_size(nav: &Path, bsp_size: u64) -> Result<bool> {
    let size = u32::try_from(bsp_size).map_err(|_| BspError::TooLarge("BSP larger than 4 GiB".into()))?;
    let mut file = OpenOptions::new().read(true).write(true).open(nav).map_err(|e| BspError::io(nav, e))?;
    let mut head = [0u8; 17];
    let n = read_up_to(&mut file, &mut head).map_err(|e| BspError::io(nav, e))?;
    let header = read_nav_header(&head[..n])?;
    let (Some(old), Some(ofs)) = (header.bsp_size, header.bsp_size_offset) else {
        return Ok(false);
    };
    if old == size {
        return Ok(false);
    }
    file.seek(SeekFrom::Start(ofs as u64)).map_err(|e| BspError::io(nav, e))?;
    file.write_all(&size.to_le_bytes()).map_err(|e| BspError::io(nav, e))?;
    file.sync_all().map_err(|e| BspError::io(nav, e))?;
    Ok(true)
}

fn read_up_to(file: &mut std::fs::File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match file.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nav(version: u32, size: u32) -> Vec<u8> {
        let mut v = NAV_MAGIC.to_le_bytes().to_vec();
        v.extend_from_slice(&version.to_le_bytes());
        if version >= 10 {
            v.extend_from_slice(&1u32.to_le_bytes());
        }
        v.extend_from_slice(&size.to_le_bytes());
        v.push(1);
        v.extend_from_slice(&[0xAB; 32]);
        v
    }

    #[test]
    fn header_and_fixup() {
        let h = read_nav_header(&nav(16, 1234)).unwrap();
        assert_eq!((h.version, h.subversion, h.bsp_size, h.analyzed), (16, 1, Some(1234), Some(true)));
        assert_eq!(read_nav_header(&nav(9, 77)).unwrap().bsp_size_offset, Some(8));
        assert!(read_nav_header(b"nope").is_err());
        assert!(read_nav_header(&NAV_MAGIC.to_le_bytes()).is_err());

        let dir = std::env::temp_dir().join(format!("hp_bsp_nav_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("gm_x.nav");
        let original = nav(16, 1234);
        std::fs::write(&path, &original).unwrap();
        assert!(fixup_bsp_size(&path, 99_999).unwrap());
        assert!(!fixup_bsp_size(&path, 99_999).unwrap());
        let after = std::fs::read(&path).unwrap();
        assert_eq!(read_nav_header(&after).unwrap().bsp_size, Some(99_999));
        assert_eq!(after.len(), original.len());
        assert_eq!(after[16..], original[16..]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
