use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use hp_vtf::{
    DecodeOptions, EncodeOptions, ImageFormat, MipSelect, VtfError, decode_low_res_rgba8, decode_rgba8, encode,
    raw_mips, read_info, required_len,
};

const ROOTS: &[&str] = &[
    r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\materials",
    r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\addons",
    r"D:\SteamLibrary\steamapps\common\GarrysMod\bin\win64\hammerplusplus\materials",
];

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => collect(&path, out),
            Ok(_) if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("vtf")) => {
                out.push(path);
            }
            _ => {}
        }
    }
}

struct Checked {
    format: ImageFormat,
    version: (u32, u32),
    ends_at_eof: bool,
}

fn check(data: &[u8]) -> Result<Checked, VtfError> {
    let info = read_info(data)?;
    let full = decode_rgba8(data, &DecodeOptions::default())?;
    assert_eq!((full.width, full.height), (info.width, info.height));
    assert_eq!(full.pixels.len() as u64, u64::from(info.width) * u64::from(info.height) * 4);

    let thumb_opts = DecodeOptions::thumbnail(128);
    let thumb = decode_rgba8(data, &thumb_opts)?;
    assert!(thumb.width <= 128 && thumb.height <= 128);

    let mut ends_at_eof = false;
    for frame in 0..info.frames {
        for face in 0..info.faces {
            let mips = raw_mips(data, frame, face)?;
            assert_eq!(mips.len() as u32, info.mip_count);
            if let Some(largest) = mips.first() {
                ends_at_eof |= largest.data.as_ptr_range().end == data.as_ptr_range().end;
            }
        }
    }
    decode_low_res_rgba8(data)?;

    let needed = required_len(data, &thumb_opts)?;
    let partial = data.get(..needed).ok_or(VtfError::Corrupt("prefix"))?;
    assert_eq!(decode_rgba8(partial, &thumb_opts)?, thumb);

    Ok(Checked { format: info.format, version: info.version, ends_at_eof })
}

#[test]
fn decode_all_loose_game_textures() {
    let mut files = Vec::new();
    for root in ROOTS {
        collect(Path::new(root), &mut files);
    }
    if files.is_empty() {
        eprintln!("hp-vtf corpus test skipped: no game textures found");
        return;
    }
    let start = Instant::now();
    let mut per_format: BTreeMap<String, usize> = BTreeMap::new();
    let mut per_version: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    let mut at_eof = 0usize;
    let mut bytes = 0u64;
    let mut failures = Vec::new();
    for path in &files {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("cannot read {}: {e}", path.display());
                continue;
            }
        };
        bytes += data.len() as u64;
        match check(&data) {
            Ok(c) => {
                *per_format.entry(c.format.name().to_owned()).or_default() += 1;
                *per_version.entry(c.version).or_default() += 1;
                at_eof += usize::from(c.ends_at_eof);
            }
            Err(e) => failures.push((path.clone(), e)),
        }
    }
    let ok: usize = per_format.values().sum();
    eprintln!(
        "hp-vtf corpus: {} files, {} decoded, {} failed, {:.1} MiB in {:.2?}",
        files.len(),
        ok,
        failures.len(),
        bytes as f64 / (1024.0 * 1024.0),
        start.elapsed()
    );
    eprintln!("  per format: {per_format:?}");
    eprintln!("  per version: {per_version:?}");
    eprintln!("  image data ends exactly at end of file: {at_eof} of {ok}");
    for (path, e) in &failures {
        eprintln!("  FAILED {}: {e}", path.display());
    }
    let unexplained: Vec<_> = failures.iter().filter(|(_, e)| !matches!(e, VtfError::BadSignature)).collect();
    assert!(unexplained.is_empty(), "unexplained failures: {unexplained:?}");
}

#[test]
fn reencode_sample_of_game_textures() {
    let mut files = Vec::new();
    for root in ROOTS {
        collect(Path::new(root), &mut files);
    }
    if files.is_empty() {
        eprintln!("hp-vtf re-encode test skipped: no game textures found");
        return;
    }
    let mut done = 0usize;
    let mut worst = 0f64;
    for path in files.iter().step_by(7) {
        let Ok(data) = std::fs::read(path) else { continue };
        let Ok(info) = read_info(&data) else { continue };
        if info.width.max(info.height) > 512 || info.is_cubemap() || !info.format.is_decodable() {
            continue;
        }
        let Ok(src) = decode_rgba8(&data, &DecodeOptions::default()) else { continue };
        let opts = EncodeOptions { resize_to_pow2: false, ..EncodeOptions::default() };
        let vtf = encode(&src, &opts).expect("encode");
        let back = decode_rgba8(&vtf, &DecodeOptions::default()).expect("decode re-encoded");
        assert_eq!((back.width, back.height), (src.width, src.height));
        let mut sum = 0u64;
        let mut n = 0u64;
        for (a, b) in src.pixels.chunks_exact(4).zip(back.pixels.chunks_exact(4)) {
            if a[3] >= 128 {
                sum += (0..3).map(|c| u64::from(a[c].abs_diff(b[c]))).sum::<u64>();
                n += 3;
            }
        }
        let err = if n > 0 { sum as f64 / n as f64 } else { 0.0 };
        worst = worst.max(err);
        assert!(err < 12.0, "{}: mean color error {err:.2}", path.display());
        let new_info = read_info(&vtf).expect("info");
        for mip in 0..new_info.mip_count {
            decode_rgba8(&vtf, &DecodeOptions { mip: MipSelect::Index(mip), ..DecodeOptions::default() }).expect("mip");
        }
        done += 1;
        if done >= 40 {
            break;
        }
    }
    eprintln!("hp-vtf re-encode: {done} textures, worst mean color error {worst:.2}");
}
