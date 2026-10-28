use crate::bsp::Bsp;
use crate::edit::BspEdit;
use crate::error::{BspError, Result};
use crate::header::lump;
use crate::pak::{PakCompression, PakOptions, PakWriter};

fn replace_map_prefix(text: &str, old: &str, new: &str) -> Option<String> {
    let needle = format!("maps/{}/", old.to_ascii_lowercase());
    let lower = text.replace('\\', "/").to_ascii_lowercase();
    if !lower.contains(&needle) {
        return None;
    }
    let mut out = String::with_capacity(text.len() + 16);
    let mut last = 0;
    for (at, _) in lower.match_indices(&needle) {
        out.push_str(&text[last..at]);
        out.push_str("maps/");
        out.push_str(&new.to_ascii_lowercase());
        out.push('/');
        last = at + needle.len();
    }
    out.push_str(&text[last..]);
    Some(out)
}

pub fn rename_map(bsp: Bsp, old: &str, new: &str) -> Result<BspEdit> {
    if new.is_empty() || !new.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err(BspError::InvalidPath(new.to_owned()));
    }
    let pak = bsp.pakfile()?;
    let compression = if pak.any_compressed() { PakCompression::Lzma } else { PakCompression::Store };
    let mut writer = PakWriter::new();
    for e in pak.entries() {
        let data = pak.read(e)?;
        let key = replace_map_prefix(&e.key, old, new).unwrap_or_else(|| e.key.clone());
        let data = if key.ends_with(".vmt") {
            match std::str::from_utf8(&data).ok().and_then(|t| replace_map_prefix(t, old, new)) {
                Some(t) => t.into_bytes(),
                None => data.into_owned(),
            }
        } else {
            data.into_owned()
        };
        writer.add(&key, data)?;
    }
    let zip = writer.finish(PakOptions { compression, level: 6 })?;
    drop(pak);

    let data = bsp.lump(lump::TEXDATA_STRING_DATA)?.into_owned();
    let table = bsp.lump(lump::TEXDATA_STRING_TABLE)?.into_owned();
    let mut new_data = Vec::with_capacity(data.len() + 64);
    let mut new_table = Vec::with_capacity(table.len());
    let mut changed = false;
    for c in table.chunks_exact(4) {
        let ofs = u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize;
        let rest = data.get(ofs..).unwrap_or(&[]);
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        let name = String::from_utf8_lossy(&rest[..end]).into_owned();
        let name = match replace_map_prefix(&name, old, new) {
            Some(n) => {
                changed = true;
                n
            }
            None => name,
        };
        let at = u32::try_from(new_data.len()).map_err(|_| BspError::TooLarge("texture names".into()))?;
        new_table.extend_from_slice(&at.to_le_bytes());
        new_data.extend_from_slice(name.as_bytes());
        new_data.push(0);
    }
    let mut edit = bsp.edit();
    edit.set_pakfile(zip)?;
    if changed {
        edit.replace_lump(lump::TEXDATA_STRING_DATA, new_data)?;
        edit.replace_lump(lump::TEXDATA_STRING_TABLE, new_table)?;
    }
    Ok(edit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PakArchive, synthetic_bsp};

    #[test]
    fn renames_generated_paths_and_texture_names() {
        let mut w = PakWriter::new();
        w.add("materials/maps/GM_Old/custom/brick_0_0_64.vmt", b"\"patch\" { \"include\" \"materials/custom/brick.vmt\" \"insert\" { \"$envmap\" \"maps\\gm_old\\c0_0_64\" } }".to_vec()).unwrap();
        w.add("materials/maps/gm_old/c0_0_64.vtf", b"VTF\0data".to_vec()).unwrap();
        w.add("materials/custom/brick.vtf", b"VTF\0other".to_vec()).unwrap();
        let names = ["maps/gm_old/custom/brick_0_0_64", "tools/toolsnodraw"];
        let mut sdata = Vec::new();
        let mut stable = Vec::new();
        for n in names {
            stable.extend_from_slice(&(sdata.len() as u32).to_le_bytes());
            sdata.extend_from_slice(n.as_bytes());
            sdata.push(0);
        }
        let bytes = synthetic_bsp(
            20,
            &[
                (lump::TEXDATA_STRING_DATA, sdata),
                (lump::TEXDATA_STRING_TABLE, stable),
                (lump::PAKFILE, w.finish(PakOptions::default()).unwrap()),
            ],
        );
        let edit = rename_map(Bsp::from_vec(bytes).unwrap(), "gm_old", "gm_new_v2").unwrap();
        let out = Bsp::from_vec(edit.to_vec().unwrap()).unwrap();
        assert_eq!(out.texture_names().unwrap(), ["maps/gm_new_v2/custom/brick_0_0_64", "tools/toolsnodraw"]);
        let pak: PakArchive<'_> = out.pakfile().unwrap();
        let keys: Vec<&str> = pak.entries().iter().map(|e| e.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "materials/custom/brick.vtf",
                "materials/maps/gm_new_v2/c0_0_64.vtf",
                "materials/maps/gm_new_v2/custom/brick_0_0_64.vmt",
            ]
        );
        let vmt = pak.read_name("materials/maps/gm_new_v2/custom/brick_0_0_64.vmt").unwrap().unwrap();
        let text = String::from_utf8(vmt.into_owned()).unwrap();
        assert!(text.contains("maps/gm_new_v2/c0_0_64"), "{text}");
        assert!(text.contains("materials/custom/brick.vmt"));
        assert!(rename_map(Bsp::from_vec(synthetic_bsp(20, &[])).unwrap(), "a", "../x").is_err());
    }
}
