use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::bsp::Bsp;
use crate::entities::{Entity, write_entities};
use crate::error::{BspError, Result};
use crate::gamelump::GameLumps;
use crate::header::{HEADER_LEN, Header, LUMP_COUNT, LumpInfo, lump};

enum Slot {
    Keep,
    Replace { data: Vec<u8>, version: u32 },
    GameLumps { lumps: GameLumps, version: u32 },
}

pub struct BspEdit {
    src: Bsp,
    slots: Vec<Slot>,
    map_revision: i32,
    entities_last: bool,
}

impl std::fmt::Debug for BspEdit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BspEdit").field("src", &self.src).finish()
    }
}

impl BspEdit {
    pub(crate) fn new(src: Bsp) -> BspEdit {
        let map_revision = src.map_revision();
        BspEdit { src, slots: (0..LUMP_COUNT).map(|_| Slot::Keep).collect(), map_revision, entities_last: false }
    }

    pub fn entities_last(&mut self, last: bool) {
        self.entities_last = last;
    }

    pub fn source(&self) -> &Bsp {
        &self.src
    }

    fn check(index: usize) -> Result<()> {
        if index >= LUMP_COUNT { Err(BspError::BadLump(index)) } else { Ok(()) }
    }

    pub fn replace_lump(&mut self, index: usize, data: Vec<u8>) -> Result<()> {
        Self::check(index)?;
        let version = self.src.header().lumps[index].version;
        self.replace_lump_with_version(index, data, version)
    }

    pub fn replace_lump_with_version(&mut self, index: usize, data: Vec<u8>, version: u32) -> Result<()> {
        Self::check(index)?;
        if u32::try_from(data.len()).is_err() {
            return Err(BspError::TooLarge(format!("lump {index}")));
        }
        self.slots[index] = if index == lump::GAME_LUMP {
            let old = self.src.header().lumps[index].offset;
            Slot::GameLumps { lumps: GameLumps::parse(&data, old)?, version }
        } else {
            Slot::Replace { data, version }
        };
        Ok(())
    }

    pub fn remove_lump(&mut self, index: usize) -> Result<()> {
        self.replace_lump(index, Vec::new())
    }

    pub fn set_game_lumps(&mut self, lumps: GameLumps) {
        let version = self.src.header().lumps[lump::GAME_LUMP].version;
        self.slots[lump::GAME_LUMP] = Slot::GameLumps { lumps, version };
    }

    pub fn set_entities(&mut self, entities: &[Entity]) -> Result<()> {
        self.replace_lump(lump::ENTITIES, write_entities(entities))
    }

    pub fn set_pakfile(&mut self, zip: Vec<u8>) -> Result<()> {
        self.replace_lump(lump::PAKFILE, zip)
    }

    pub fn set_map_revision(&mut self, revision: i32) {
        self.map_revision = revision;
    }

    pub fn to_vec(&self) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(self.src.len());
        self.write_into(&mut out)?;
        Ok(out)
    }

    pub fn save(self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let tmp = temp_path(path);
        let result = (|| {
            let file = File::create(&tmp).map_err(|e| BspError::io(&tmp, e))?;
            let mut w = BufWriter::with_capacity(1 << 20, file);
            self.write_into(&mut w).map_err(|e| match e {
                BspError::Io { source, .. } => BspError::io(&tmp, source),
                other => other,
            })?;
            let file = w.into_inner().map_err(|e| BspError::io(&tmp, e.into_error()))?;
            file.sync_all().map_err(|e| BspError::io(&tmp, e))?;
            Ok(())
        })();
        if let Err(e) = result {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        drop(self);
        if let Err(e) = std::fs::rename(&tmp, path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(BspError::io(path, e));
        }
        Ok(())
    }

    pub fn write_into(&self, w: &mut impl Write) -> Result<()> {
        let src_header = self.src.header();
        let last_entities = self.entities_last;
        let mut order: Vec<usize> =
            (0..LUMP_COUNT).filter(|&i| i != lump::PAKFILE && !(last_entities && i == lump::ENTITIES)).collect();
        order.sort_by_key(|&i| (src_header.lumps[i].offset, i));
        order.push(lump::PAKFILE);
        if last_entities {
            order.push(lump::ENTITIES);
        }

        let mut header = Header {
            version: src_header.version,
            lumps: [LumpInfo::default(); LUMP_COUNT],
            map_revision: self.map_revision,
            layout: src_header.layout,
        };
        let mut pos = HEADER_LEN as u64;
        let mut game_bytes: Option<Vec<u8>> = None;
        for &i in &order {
            pos = align4(pos);
            let src = src_header.lumps[i];
            let (len, version, four_cc) = match &self.slots[i] {
                Slot::Keep if i == lump::GAME_LUMP => {
                    let bytes = self.kept_game_lumps(pos)?;
                    let len = bytes.len();
                    game_bytes = Some(bytes);
                    (len, src.version, if src.uncompressed_len().is_some() { [0; 4] } else { src.four_cc })
                }
                Slot::Keep => (src.len as usize, src.version, src.four_cc),
                Slot::Replace { data, version } => (data.len(), *version, [0; 4]),
                Slot::GameLumps { lumps, version } => {
                    let bytes = lumps.to_bytes(u32::try_from(pos).map_err(|_| too_large())?)?;
                    let len = bytes.len();
                    game_bytes = Some(bytes);
                    (len, *version, [0; 4])
                }
            };
            let offset = if len == 0 { 0 } else { u32::try_from(pos).map_err(|_| too_large())? };
            header.lumps[i] = LumpInfo { offset, len: len as u32, version, four_cc };
            pos += len as u64;
        }
        if pos > i32::MAX as u64 {
            return Err(too_large());
        }
        let io = |e: std::io::Error| BspError::io(PathBuf::new(), e);
        w.write_all(&header.to_bytes()).map_err(io)?;
        let mut written = HEADER_LEN as u64;
        for &i in &order {
            let info = header.lumps[i];
            if info.len == 0 {
                continue;
            }
            let pad = u64::from(info.offset) - written;
            w.write_all(&[0u8; 3][..pad as usize]).map_err(io)?;
            let data: &[u8] = match &self.slots[i] {
                Slot::Keep if i == lump::GAME_LUMP => game_bytes.as_deref().unwrap_or(&[]),
                Slot::Keep => self.src.lump_raw(i)?,
                Slot::Replace { data, .. } => data,
                Slot::GameLumps { .. } => game_bytes.as_deref().unwrap_or(&[]),
            };
            w.write_all(data).map_err(io)?;
            written = u64::from(info.offset) + data.len() as u64;
        }
        w.flush().map_err(io)?;
        Ok(())
    }

    fn kept_game_lumps(&self, new_offset: u64) -> Result<Vec<u8>> {
        let new_offset = u32::try_from(new_offset).map_err(|_| too_large())?;
        let info = self.src.header().lumps[lump::GAME_LUMP];
        if info.len == 0 {
            return Ok(Vec::new());
        }
        if info.uncompressed_len().is_some() {
            return self.src.game_lumps()?.to_bytes(new_offset);
        }
        if new_offset == info.offset {
            return Ok(self.src.lump_raw(lump::GAME_LUMP)?.to_vec());
        }
        self.src.game_lumps()?.to_bytes(new_offset)
    }
}

fn too_large() -> BspError {
    BspError::TooLarge("the BSP would be larger than 2 GiB".into())
}

fn align4(n: u64) -> u64 {
    (n + 3) & !3
}

fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "map.bsp".into());
    let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    path.with_file_name(format!(".{name}.{}.{unique}.tmp", std::process::id()))
}
