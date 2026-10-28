use crate::bsp::Bsp;
use crate::error::Result;
use crate::pak::{PakArchive, PakEntry, PakWriter};

pub fn parse_cubemap_name(name: &str, map: &str) -> Option<([i32; 3], bool)> {
    let lower = name.replace('\\', "/").to_ascii_lowercase();
    let prefix = format!("materials/maps/{}/c", map.to_ascii_lowercase());
    let rest = lower.strip_prefix(&prefix)?;
    let (coords, hdr) =
        if let Some(r) = rest.strip_suffix(".hdr.vtf") { (r, true) } else { (rest.strip_suffix(".vtf")?, false) };
    let mut it = coords.split('_').map(|s| s.parse::<i32>().ok());
    let p = [it.next()??, it.next()??, it.next()??];
    if it.next().is_some() {
        return None;
    }
    Some((p, hdr))
}

pub fn cubemap_name(map: &str, origin: [i32; 3], hdr: bool) -> String {
    let ext = if hdr { "hdr.vtf" } else { "vtf" };
    format!("materials/maps/{}/c{}_{}_{}.{ext}", map.to_ascii_lowercase(), origin[0], origin[1], origin[2])
}

fn vtf_width(data: &[u8]) -> Option<u32> {
    if data.get(..4)? != b"VTF\0" {
        return None;
    }
    Some(u32::from(u16::from_le_bytes([*data.get(16)?, *data.get(17)?])))
}

fn placeholder_crcs(pak: &PakArchive<'_>, map: &str) -> [Option<u32>; 2] {
    let base = format!("materials/maps/{}/cubemapdefault", map.to_ascii_lowercase());
    [pak.find(&format!("{base}.vtf")).map(|e| e.crc32), pak.find(&format!("{base}.hdr.vtf")).map(|e| e.crc32)]
}

fn built(entry: &PakEntry, hdr: bool, defaults: [Option<u32>; 2]) -> bool {
    defaults[usize::from(hdr)] != Some(entry.crc32)
}

pub fn built_origins(pak: &PakArchive<'_>, map: &str) -> Vec<[i32; 3]> {
    let defaults = placeholder_crcs(pak, map);
    let mut out: Vec<[i32; 3]> = pak
        .entries()
        .iter()
        .filter_map(|e| parse_cubemap_name(&e.key, map).filter(|(_, hdr)| built(e, *hdr, defaults)).map(|(o, _)| o))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

pub fn read_built(pak: &PakArchive<'_>, map: &str, origins: &[[i32; 3]]) -> Vec<[Option<Vec<u8>>; 2]> {
    let defaults = placeholder_crcs(pak, map);
    origins
        .iter()
        .map(|o| {
            [false, true].map(|hdr| {
                let entry = pak.find(&cubemap_name(map, *o, hdr))?;
                if !built(entry, hdr, defaults) {
                    return None;
                }
                pak.read(entry).ok().map(|d| d.into_owned())
            })
        })
        .collect()
}

const CUBEMAP_FLAGS: u32 = 0x0004 | 0x0008 | 0x0200 | 0x4000;
const FORMAT_RGBA8888: u32 = 0;
const FORMAT_RGBA16161616F: u32 = 24;

fn half(v: f32) -> u16 {
    let v = if v.is_finite() { v.clamp(-65504.0, 65504.0) } else { 0.0 };
    let x = v.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let exp = ((x >> 23) & 0xff) as i32 - 127 + 15;
    let mant = x & 0x7f_ffff;
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        let m = mant | 0x80_0000;
        let shift = (14 - exp) as u32;
        let h = m >> shift;
        let round = (m >> (shift - 1)) & 1;
        return sign | (h + round) as u16;
    }
    let h = ((exp as u32) << 10) | (mant >> 13);
    let round = (mant >> 12) & 1;
    sign | (h + round).min(0x7bff) as u16
}

fn srgb_byte(v: f32) -> u8 {
    let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
    let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

pub fn cubemap_vtf(size: u32, faces: &[Vec<[f32; 3]>; 6], hdr: bool) -> Vec<u8> {
    const HEADER: u32 = 88;
    let size = size.max(1);
    let n = (size * size) as usize;
    let mut chain: Vec<[Vec<[f32; 3]>; 6]> = Vec::new();
    let first: [Vec<[f32; 3]>; 6] = std::array::from_fn(|f| {
        let mut v = faces[f].clone();
        v.resize(n, [0.0; 3]);
        v
    });
    chain.push(first);
    let mut side = size;
    while side > 1 {
        let next = (side / 2).max(1);
        let prev = chain.last().map_or_else(|| std::array::from_fn(|_| Vec::new()), Clone::clone);
        let level: [Vec<[f32; 3]>; 6] = std::array::from_fn(|f| {
            let mut out = Vec::with_capacity((next * next) as usize);
            for y in 0..next {
                for x in 0..next {
                    let mut sum = [0.0f32; 3];
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (x * 2 + dx).min(side - 1);
                        let sy = (y * 2 + dy).min(side - 1);
                        let t = prev[f][(sy * side + sx) as usize];
                        for k in 0..3 {
                            sum[k] += t[k] * 0.25;
                        }
                    }
                    out.push(sum);
                }
            }
            out
        });
        chain.push(level);
        side = next;
    }
    let mips = chain.len() as u8;
    let mut reflect = [0.0f32; 3];
    for f in &chain[0] {
        for t in f {
            for k in 0..3 {
                reflect[k] += t[k].clamp(0.0, 1.0) / (6 * n) as f32;
            }
        }
    }
    let format = if hdr { FORMAT_RGBA16161616F } else { FORMAT_RGBA8888 };
    let mut h: Vec<u8> = Vec::new();
    h.extend_from_slice(b"VTF\0");
    h.extend_from_slice(&7u32.to_le_bytes());
    h.extend_from_slice(&5u32.to_le_bytes());
    h.extend_from_slice(&HEADER.to_le_bytes());
    h.extend_from_slice(&(size as u16).to_le_bytes());
    h.extend_from_slice(&(size as u16).to_le_bytes());
    h.extend_from_slice(&CUBEMAP_FLAGS.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&0u16.to_le_bytes());
    h.extend_from_slice(&[0u8; 4]);
    for r in reflect {
        h.extend_from_slice(&r.to_le_bytes());
    }
    h.extend_from_slice(&[0u8; 4]);
    h.extend_from_slice(&1f32.to_le_bytes());
    h.extend_from_slice(&format.to_le_bytes());
    h.push(mips);
    h.extend_from_slice(&(-1i32).to_le_bytes());
    h.push(0);
    h.push(0);
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&[0u8; 3]);
    h.extend_from_slice(&1u32.to_le_bytes());
    h.extend_from_slice(&[0u8; 8]);
    h.extend_from_slice(&[0x30, 0, 0, 0]);
    h.extend_from_slice(&HEADER.to_le_bytes());
    debug_assert_eq!(h.len(), HEADER as usize);
    for level in chain.iter().rev() {
        for face in level {
            for t in face {
                if hdr {
                    for v in [t[0], t[1], t[2], 1.0] {
                        h.extend_from_slice(&half(v.max(0.0)).to_le_bytes());
                    }
                } else {
                    h.extend_from_slice(&[srgb_byte(t[0]), srgb_byte(t[1]), srgb_byte(t[2]), 255]);
                }
            }
        }
    }
    h
}

pub type CubemapFiles = ([i32; 3], Vec<u8>, Option<Vec<u8>>);

pub fn write_cubemaps(path: &std::path::Path, map: &str, cubes: &[CubemapFiles]) -> Result<usize> {
    let bsp = Bsp::open(path)?;
    let mut writer = bsp.pakfile()?.to_writer()?;
    let mut written = 0;
    for (origin, ldr, hdr) in cubes {
        writer.add(&cubemap_name(map, *origin, false), ldr.clone())?;
        written += 1;
        if let Some(h) = hdr {
            writer.add(&cubemap_name(map, *origin, true), h.clone())?;
            written += 1;
        }
    }
    let zip = writer.finish(crate::PakOptions::default())?;
    let mut edit = bsp.edit();
    edit.set_pakfile(zip)?;
    edit.save(path)?;
    Ok(written)
}

pub fn count_built(pak: &PakArchive<'_>, map: &str) -> usize {
    built_origins(pak, map).len()
}

pub fn carry_over(
    old: &PakArchive<'_>,
    new_bsp: &Bsp,
    map: &str,
    writer: &mut PakWriter,
    done: &mut std::collections::HashSet<String>,
) -> Result<usize> {
    let samples = new_bsp.cubemaps()?;
    let new_pak = new_bsp.pakfile()?;
    let new_defaults = placeholder_crcs(&new_pak, map);
    let old_defaults = placeholder_crcs(old, map);
    let mut copied = 0;
    for entry in old.entries() {
        let Some((origin, hdr)) = parse_cubemap_name(&entry.key, map) else { continue };
        if !built(entry, hdr, old_defaults) || done.contains(&entry.key) {
            continue;
        }
        let Some(&(_, size)) = samples.iter().find(|(o, _)| *o == origin) else { continue };
        if let Some(current) = new_pak.find(&entry.key)
            && built(current, hdr, new_defaults)
        {
            continue;
        }
        let Ok(data) = old.read(entry) else { continue };
        if (1..=16).contains(&size)
            && let Some(w) = vtf_width(&data)
            && w != 1u32 << (size - 1)
        {
            continue;
        }
        writer.add(&entry.key, data.into_owned())?;
        done.insert(entry.key.clone());
        copied += 1;
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(parse_cubemap_name("materials/maps/gm_X/c-12_0_64.hdr.vtf", "gm_x"), Some(([-12, 0, 64], true)));
        assert_eq!(parse_cubemap_name("materials/maps/gm_x/c1_2_3.vtf", "GM_X"), Some(([1, 2, 3], false)));
        assert_eq!(parse_cubemap_name("materials/maps/gm_x/c1_2.vtf", "gm_x"), None);
        assert_eq!(parse_cubemap_name("materials/maps/gm_x/c1_2_3_4.vtf", "gm_x"), None);
        assert_eq!(parse_cubemap_name("materials/maps/gm_y/c1_2_3.vtf", "gm_x"), None);
        assert_eq!(parse_cubemap_name("materials/maps/gm_x/cubemapdefault.vtf", "gm_x"), None);
        assert_eq!(cubemap_name("GM_X", [-1, 2, 3], true), "materials/maps/gm_x/c-1_2_3.hdr.vtf");
    }

    #[test]
    fn written_cubemaps_count_as_built() {
        let faces: [Vec<[f32; 3]>; 6] = std::array::from_fn(|f| vec![[f as f32 * 0.5, 0.25, 2.0]; 16]);
        let ldr = cubemap_vtf(4, &faces, false);
        let hdr = cubemap_vtf(4, &faces, true);
        assert_eq!(&ldr[..4], b"VTF\0");
        assert_eq!(u16::from_le_bytes([ldr[16], ldr[17]]), 4);
        assert_eq!(ldr[56], 3, "4, 2 and 1 texels");
        assert_eq!(ldr.len(), 88 + 6 * 21 * 4);
        assert_eq!(hdr.len(), 88 + 6 * 21 * 8);
        let at = 88 + 6 * 5 * 4 + 5 * 16 * 4;
        assert_eq!(&ldr[at..at + 4], &[255, 137, 255, 255], "sRGB, clamped to white");
        let at = 88 + 6 * 5 * 8 + 5 * 16 * 8;
        assert_eq!(u16::from_le_bytes([hdr[at + 4], hdr[at + 5]]), 0x4000, "2.0 kept as a half");
        assert_eq!(half(1.0), 0x3c00);
        assert_eq!(half(0.25), 0x3400);

        let dir = std::env::temp_dir().join(format!("hp-bsp-cubemaps-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("gm_w.bsp");
        let mut w = PakWriter::new();
        w.add("materials/maps/gm_w/cubemapdefault.vtf", b"VTF\0placeholder".to_vec()).expect("add");
        w.add("materials/maps/gm_w/c0_0_64.vtf", b"VTF\0placeholder".to_vec()).expect("add");
        let samples: Vec<u8> = [0i32, 0, 64, 0].iter().flat_map(|v| v.to_le_bytes()).collect();
        let data = crate::synthetic_bsp(
            20,
            &[
                (crate::lump::ENTITIES, b"{\n\"classname\" \"worldspawn\"\n}\n\0".to_vec()),
                (crate::lump::CUBEMAPS, samples),
                (crate::lump::PAKFILE, w.finish(crate::PakOptions::default()).expect("pak")),
            ],
        );
        std::fs::write(&path, data).expect("write");
        let written = write_cubemaps(&path, "gm_w", &[([0, 0, 64], ldr.clone(), Some(hdr.clone()))]).expect("cubemaps");
        assert_eq!(written, 2);
        let bsp = Bsp::read(&path).expect("read");
        let pak = bsp.pakfile().expect("pak");
        assert_eq!(built_origins(&pak, "gm_w"), vec![[0, 0, 64]]);
        let got = read_built(&pak, "gm_w", &[[0, 0, 64]]);
        assert_eq!(got[0][0].as_deref(), Some(&ldr[..]));
        assert_eq!(got[0][1].as_deref(), Some(&hdr[..]));
        assert!(!pak.any_compressed(), "stored like the game's");
        drop(pak);
        drop(bsp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn built_cubemaps_are_told_from_placeholders() {
        let mut w = PakWriter::new();
        let placeholder: &[u8] = b"VTF\0placeholder";
        for (name, data) in [
            ("materials/maps/gm_x/cubemapdefault.vtf", placeholder),
            ("materials/maps/gm_x/c0_0_64.vtf", placeholder),
            ("materials/maps/gm_x/c0_0_64.hdr.vtf", &[7u8; 40][..]),
            ("materials/maps/gm_x/c10_0_64.vtf", &[5u8; 40][..]),
        ] {
            w.add(name, data.to_vec()).expect("add");
        }
        let zip = w.finish(crate::PakOptions::default()).expect("pak");
        let pak = PakArchive::parse(zip).expect("parse");
        let got = read_built(&pak, "GM_X", &[[0, 0, 64], [10, 0, 64], [99, 0, 0]]);
        assert_eq!(got[0][0], None, "the LDR placeholder");
        assert_eq!(got[0][1].as_deref(), Some(&[7u8; 40][..]), "no HDR placeholder: built");
        assert_eq!(got[1][0].as_deref(), Some(&[5u8; 40][..]));
        assert_eq!(got[2], [None, None], "no file at all");
        assert_eq!(built_origins(&pak, "gm_x"), vec![[0, 0, 64], [10, 0, 64]]);
    }
}
