use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;

use hp_vtf::{
    DecodeOptions, EncodeFormat, EncodeOptions, EncodeQuality, MAX_HEADER_LEN, RgbaImage, decode_rgba8, encode, is_vtf,
    required_len,
};
use rayon::prelude::*;

const DEFAULT_ROOTS: &[&str] = &[
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
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("vtf")) {
            out.push(path);
        }
    }
}

fn thumbnail_from_disk(path: &Path, opts: &DecodeOptions) -> Option<RgbaImage> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; MAX_HEADER_LEN];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);
    let needed = required_len(&buf, opts).ok()?;
    if needed > buf.len() {
        let have = buf.len();
        buf.resize(needed, 0);
        file.read_exact(buf.get_mut(have..)?).ok()?;
    }
    decode_rgba8(&buf, opts).ok()
}

fn ms_per(total: std::time::Duration, count: usize) -> f64 {
    total.as_secs_f64() * 1000.0 / count.max(1) as f64
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let roots: Vec<&str> =
        if args.is_empty() { DEFAULT_ROOTS.to_vec() } else { args.iter().map(String::as_str).collect() };
    let mut files = Vec::new();
    for root in &roots {
        collect(Path::new(root), &mut files);
    }
    if files.is_empty() {
        eprintln!("no VTF files found");
        return;
    }

    let t = Instant::now();
    let blobs: Vec<Vec<u8>> = files.par_iter().filter_map(|p| std::fs::read(p).ok()).filter(|d| is_vtf(d)).collect();
    let total_bytes: usize = blobs.iter().map(Vec::len).sum();
    println!("loaded {} VTF files ({:.1} MiB) in {:.2?}", blobs.len(), total_bytes as f64 / 1048576.0, t.elapsed());
    let n = blobs.len();
    let opts = DecodeOptions::thumbnail(128);

    for b in blobs.iter().take(50) {
        let _ = decode_rgba8(b, &opts);
    }

    let t = Instant::now();
    let ok = blobs.iter().filter(|b| decode_rgba8(b, &opts).is_ok()).count();
    let seq = t.elapsed();
    println!("thumbnail(128) in memory, 1 thread: {ok}/{n} ok, {:.2?} total, {:.4} ms/texture", seq, ms_per(seq, n));

    let t = Instant::now();
    let ok = blobs.par_iter().filter(|b| decode_rgba8(b, &opts).is_ok()).count();
    let par = t.elapsed();
    println!(
        "thumbnail(128) in memory, {} threads: {ok}/{n} ok, {:.2?} total, {:.4} ms/texture (wall)",
        rayon::current_num_threads(),
        par,
        ms_per(par, n)
    );

    let t = Instant::now();
    let ok = files.par_iter().filter(|p| thumbnail_from_disk(p, &opts).is_some()).count();
    let disk = t.elapsed();
    println!(
        "thumbnail(128) from disk with prefix reads, parallel: {ok} ok, {:.2?} total, {:.4} ms/texture (wall)",
        disk,
        ms_per(disk, files.len())
    );

    let t = Instant::now();
    let ok = blobs.iter().filter(|b| decode_rgba8(b, &DecodeOptions::default()).is_ok()).count();
    let full = t.elapsed();
    println!(
        "full resolution decode, sequential files: {ok}/{n} ok, {:.2?} total, {:.3} ms/texture",
        full,
        ms_per(full, n)
    );

    let size = 1024u32;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            pixels.extend_from_slice(&[(x ^ y) as u8, (x * 3) as u8, (y * 5) as u8, (x + y) as u8]);
        }
    }
    let img = RgbaImage::new(size, size, pixels).unwrap_or_default();
    for quality in [EncodeQuality::Fast, EncodeQuality::Normal] {
        for format in [EncodeFormat::Dxt1, EncodeFormat::Dxt5, EncodeFormat::Bgra8888] {
            let t = Instant::now();
            let out = encode(&img, &EncodeOptions { format, quality, ..EncodeOptions::default() });
            println!(
                "encode 1024x1024 {format:?} {quality:?} with mips: {:.2?} ({} bytes)",
                t.elapsed(),
                out.map(|v| v.len()).unwrap_or(0)
            );
        }
    }
}
