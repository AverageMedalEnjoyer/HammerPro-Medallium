use std::borrow::Cow;

use crate::error::{BspError, Result};
use crate::lzma;

pub const FLAG_COMPRESSED: u16 = 1;

pub mod id {
    pub const STATIC_PROPS: u32 = u32::from_be_bytes(*b"sprp");
    pub const DETAIL_PROPS: u32 = u32::from_be_bytes(*b"dprp");
    pub const DETAIL_PROP_LIGHTING: u32 = u32::from_be_bytes(*b"dplt");
    pub const DETAIL_PROP_LIGHTING_HDR: u32 = u32::from_be_bytes(*b"dplh");
}

pub fn id_text(id: u32) -> String {
    id.to_be_bytes().iter().map(|&b| if b.is_ascii_graphic() { b as char } else { '?' }).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameLump {
    pub id: u32,
    pub flags: u16,
    pub version: u16,
    pub stored: Vec<u8>,
    pub len: u32,
}

impl GameLump {
    pub fn new(id: u32, version: u16, data: Vec<u8>) -> Result<GameLump> {
        let len = u32::try_from(data.len()).map_err(|_| BspError::TooLarge("game lump".into()))?;
        Ok(GameLump { id, flags: 0, version, stored: data, len })
    }

    pub fn is_compressed(&self) -> bool {
        self.flags & FLAG_COMPRESSED != 0
    }

    pub fn data(&self) -> Result<Cow<'_, [u8]>> {
        if !self.is_compressed() {
            return Ok(Cow::Borrowed(&self.stored));
        }
        if self.stored.is_empty() && self.len == 0 {
            return Ok(Cow::Borrowed(&[]));
        }
        let data = lzma::decompress_valve(&self.stored)?;
        if data.len() != self.len as usize {
            return Err(BspError::corrupt(format!(
                "game lump {} has the wrong size once decompressed",
                id_text(self.id)
            )));
        }
        Ok(Cow::Owned(data))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameLumps {
    pub lumps: Vec<GameLump>,
}

struct DirEntry {
    id: u32,
    flags: u16,
    version: u16,
    ofs: i64,
    len: u32,
}

impl GameLumps {
    pub fn parse(data: &[u8], lump_offset: u32) -> Result<GameLumps> {
        if data.is_empty() {
            return Ok(GameLumps::default());
        }
        let count = data.get(..4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(-1);
        let max = (data.len().saturating_sub(4)) / 16;
        if count < 0 || count as usize > max {
            return Err(BspError::corrupt(format!("game lump count {count} does not fit the lump")));
        }
        let count = count as usize;
        let mut dir = Vec::with_capacity(count);
        for i in 0..count {
            let at = 4 + i * 16;
            let b = &data[at..at + 16];
            dir.push(DirEntry {
                id: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                flags: u16::from_le_bytes([b[4], b[5]]),
                version: u16::from_le_bytes([b[6], b[7]]),
                ofs: i64::from(i32::from_le_bytes([b[8], b[9], b[10], b[11]])),
                len: u32::from_le_bytes([b[12], b[13], b[14], b[15]]),
            });
        }
        let fits = |base: i64| {
            dir.iter().enumerate().all(|(i, e)| {
                if e.len == 0 && e.id == 0 {
                    return true;
                }
                let start = e.ofs - base;
                let stored = stored_len(&dir, i, base, data.len());
                start >= 0 && stored.is_some_and(|n| (start as u64 + n as u64) <= data.len() as u64)
            })
        };
        let base = if fits(i64::from(lump_offset)) {
            i64::from(lump_offset)
        } else if fits(0) {
            0
        } else {
            return Err(BspError::corrupt("a game lump lies outside of lump 35"));
        };
        let mut lumps = Vec::with_capacity(count);
        for (i, e) in dir.iter().enumerate() {
            if e.id == 0 && e.len == 0 {
                continue;
            }
            let start = (e.ofs - base) as usize;
            let n = stored_len(&dir, i, base, data.len()).unwrap_or(0);
            lumps.push(GameLump {
                id: e.id,
                flags: e.flags,
                version: e.version,
                stored: data[start..start + n].to_vec(),
                len: e.len,
            });
        }
        Ok(GameLumps { lumps })
    }

    pub fn get(&self, id: u32) -> Option<&GameLump> {
        self.lumps.iter().find(|l| l.id == id)
    }

    pub fn set(&mut self, id: u32, version: u16, data: Vec<u8>) -> Result<()> {
        let lump = GameLump::new(id, version, data)?;
        match self.lumps.iter_mut().find(|l| l.id == id) {
            Some(slot) => *slot = lump,
            None => self.lumps.push(lump),
        }
        Ok(())
    }

    pub fn remove(&mut self, id: u32) -> bool {
        let before = self.lumps.len();
        self.lumps.retain(|l| l.id != id);
        before != self.lumps.len()
    }

    pub fn to_bytes(&self, lump_offset: u32) -> Result<Vec<u8>> {
        if self.lumps.is_empty() {
            return Ok(Vec::new());
        }
        let compressed = self.lumps.iter().any(GameLump::is_compressed);
        let entries = self.lumps.len() + usize::from(compressed);
        let dir_len = 4 + entries * 16;
        let total = dir_len + self.lumps.iter().map(|l| l.stored.len()).sum::<usize>();
        let too_large = || BspError::TooLarge("game lumps".into());
        if u64::from(lump_offset) + total as u64 > i32::MAX as u64 {
            return Err(too_large());
        }
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(&(entries as i32).to_le_bytes());
        let mut ofs = lump_offset as usize + dir_len;
        for l in &self.lumps {
            out.extend_from_slice(&l.id.to_le_bytes());
            out.extend_from_slice(&l.flags.to_le_bytes());
            out.extend_from_slice(&l.version.to_le_bytes());
            out.extend_from_slice(&(ofs as i32).to_le_bytes());
            out.extend_from_slice(&l.len.to_le_bytes());
            ofs += l.stored.len();
        }
        if compressed {
            out.extend_from_slice(&[0; 8]);
            out.extend_from_slice(&(ofs as i32).to_le_bytes());
            out.extend_from_slice(&[0; 4]);
        }
        for l in &self.lumps {
            out.extend_from_slice(&l.stored);
        }
        Ok(out)
    }
}

fn stored_len(dir: &[DirEntry], i: usize, base: i64, lump_len: usize) -> Option<usize> {
    let e = &dir[i];
    if e.flags & FLAG_COMPRESSED == 0 {
        return Some(e.len as usize);
    }
    let end = match dir.get(i + 1) {
        Some(next) => next.ofs - base,
        None => lump_len as i64,
    };
    let start = e.ofs - base;
    (end >= start).then(|| (end - start) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_at_new_offset() {
        let mut g = GameLumps::default();
        g.set(id::STATIC_PROPS, 10, vec![1, 2, 3, 4, 5]).unwrap();
        g.set(id::DETAIL_PROPS, 4, vec![9; 12]).unwrap();
        let bytes = g.to_bytes(5000).unwrap();
        let back = GameLumps::parse(&bytes, 5000).unwrap();
        assert_eq!(back, g);
        let moved = back.to_bytes(123_456).unwrap();
        assert_ne!(moved, bytes);
        assert_eq!(GameLumps::parse(&moved, 123_456).unwrap(), g);
        assert_eq!(id_text(id::STATIC_PROPS), "sprp");
        assert_eq!(&bytes[4..8], b"prps");
    }

    #[test]
    fn compressed_lumps_use_the_end_marker() {
        let raw: Vec<u8> = (0..4000u32).flat_map(|i| (i % 7).to_le_bytes()).collect();
        let stored = lzma::compress_valve(&raw, 6).unwrap();
        let g = GameLumps {
            lumps: vec![
                GameLump { id: id::STATIC_PROPS, flags: 1, version: 10, stored, len: raw.len() as u32 },
                GameLump::new(id::DETAIL_PROPS, 4, vec![7; 3]).unwrap(),
            ],
        };
        let bytes = g.to_bytes(2048).unwrap();
        let back = GameLumps::parse(&bytes, 2048).unwrap();
        assert_eq!(back, g);
        assert_eq!(back.get(id::STATIC_PROPS).unwrap().data().unwrap().as_ref(), raw.as_slice());
    }

    #[test]
    fn relative_offsets_and_garbage() {
        let mut g = GameLumps::default();
        g.set(id::STATIC_PROPS, 6, vec![1; 8]).unwrap();
        let bytes = g.to_bytes(0).unwrap();
        assert_eq!(GameLumps::parse(&bytes, 9999).unwrap(), g);
        assert!(GameLumps::parse(&[255, 255, 255, 127], 0).is_err());
        assert!(GameLumps::parse(&[1, 0, 0, 0], 0).is_err());
        let mut bad = bytes.clone();
        bad[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(GameLumps::parse(&bad, 0).is_err());
        assert!(GameLumps::parse(&[], 0).unwrap().lumps.is_empty());
    }
}
