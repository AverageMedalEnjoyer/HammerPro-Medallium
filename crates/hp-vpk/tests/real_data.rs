use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hp_vpk::VpkArchive;
use rayon::prelude::*;

const DEFAULT_GMOD_DIR: &str = r"D:\SteamLibrary\steamapps\common\GarrysMod";

fn gmod_dir() -> Option<PathBuf> {
    let dir =
        std::env::var_os("HAMMERPRO_GMOD_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_GMOD_DIR));
    if dir.join("garrysmod").is_dir() {
        Some(dir)
    } else {
        eprintln!("skipping: Garry's Mod not found at {}", dir.display());
        None
    }
}

fn dir_vpks(gmod: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for sub in ["garrysmod", "sourceengine"] {
        let Ok(read) = std::fs::read_dir(gmod.join(sub)) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let is_dir_vpk =
                path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.to_ascii_lowercase().ends_with("_dir.vpk"));
            if is_dir_vpk {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

#[test]
fn open_every_dir_vpk_and_sample_reads() {
    let Some(gmod) = gmod_dir() else { return };
    let vpks = dir_vpks(&gmod);
    assert!(!vpks.is_empty(), "no *_dir.vpk found");
    let mut total_files = 0usize;
    let mut total_open = Duration::ZERO;
    let mut slowest = (Duration::ZERO, String::new());
    for path in &vpks {
        let start = Instant::now();
        let vpk = VpkArchive::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let open = start.elapsed();
        total_open += open;
        if open > slowest.0 {
            slowest = (open, name(path));
        }
        total_files += vpk.len();
        eprintln!(
            "{:<32} v{} {:>7} files {:>3} parts  open {:>9.3?}",
            name(path),
            vpk.version(),
            vpk.len(),
            vpk.part_count(),
            open
        );
        assert!(!vpk.is_empty());

        let start = Instant::now();
        for (p, entry) in &vpk {
            assert_eq!(vpk.entry(p), Some(entry), "{p}");
            assert!(vpk.contains(&p.to_ascii_uppercase()), "{p}");
        }
        let lookups = start.elapsed();

        let paths: Vec<&str> = vpk.paths().collect();
        let step = (paths.len() / 300).max(1);
        let sample: Vec<&str> = paths.iter().step_by(step).copied().collect();
        let start = Instant::now();
        let (bytes, bad_crc): (u64, usize) = sample
            .par_iter()
            .map(|p| {
                let data = vpk.read_cow(p).unwrap_or_else(|e| panic!("{p}: {e}")).expect("listed path is readable");
                let entry = vpk.entry(p).expect("listed path has an entry");
                assert_eq!(data.len() as u64, entry.size(), "{p}");
                let crc_ok = crc32fast::hash(&data) == entry.crc();
                if !crc_ok {
                    eprintln!("CRC mismatch: {p}");
                }
                (data.len() as u64, usize::from(!crc_ok))
            })
            .reduce(|| (0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
        eprintln!(
            "{:<32} {} lookups in {:.3?}, {} sampled reads ({} KiB, {} CRC mismatches) in {:.3?}",
            "",
            paths.len() * 2,
            lookups,
            sample.len(),
            bytes / 1024,
            bad_crc,
            start.elapsed()
        );
        assert!(bad_crc * 100 <= sample.len(), "too many CRC mismatches in {}", name(path));
    }
    eprintln!(
        "{} VPKs, {} files, total open {:.3?}, slowest {} ({:.3?})",
        vpks.len(),
        total_files,
        total_open,
        slowest.1,
        slowest.0
    );
}

#[test]
fn open_all_in_parallel() {
    let Some(gmod) = gmod_dir() else { return };
    let vpks = dir_vpks(&gmod);
    let start = Instant::now();
    let opened: Vec<VpkArchive> = vpks.par_iter().map(|p| VpkArchive::open(p).unwrap()).collect();
    let files: usize = opened.iter().map(VpkArchive::len).sum();
    eprintln!("opened {} VPKs ({} files) in parallel in {:.3?}", opened.len(), files, start.elapsed());
}

#[test]
fn known_material_and_texture() {
    let Some(gmod) = gmod_dir() else { return };
    let vpks: Vec<VpkArchive> = dir_vpks(&gmod).iter().map(|p| VpkArchive::open(p).unwrap()).collect();
    let find = |path: &str| vpks.iter().find(|v| v.contains(path));

    let vmt_path = "materials/concrete/concretefloor001a.vmt";
    let vmt_vpk = find(vmt_path).expect("HL2 concrete VMT is in a mounted VPK");
    let text = String::from_utf8(vmt_vpk.read(vmt_path).unwrap().unwrap()).expect("VMT is UTF-8");
    let root = hp_kv::parse(&text).expect("VMT parses as KeyValues");
    let shader = root.children.first().expect("VMT has a shader block");
    let tex = shader.get("$basetexture").expect("VMT has $basetexture");
    eprintln!("{vmt_path} in {}: shader {}, $basetexture {tex}", name(vmt_vpk.path()), shader.name);
    let vtf_path = format!("materials/{tex}.vtf");
    let vtf_vpk = find(&hp_vpk::normalize_path(&vtf_path)).expect("base texture is in a VPK");
    let vtf = vtf_vpk.read(&vtf_path).unwrap().unwrap();
    assert_eq!(&vtf[..4], b"VTF\0");
    let major = u32::from_le_bytes([vtf[4], vtf[5], vtf[6], vtf[7]]);
    assert_eq!(major, 7);
    let entry = vtf_vpk.entry(&vtf_path).unwrap();
    assert_eq!(vtf.len() as u64, entry.size());
    assert_eq!(crc32fast::hash(&vtf), entry.crc());
    eprintln!(
        "{vtf_path} in {}: {} bytes, preload {}, part {:?}",
        name(vtf_vpk.path()),
        vtf.len(),
        entry.preload_len(),
        entry.archive_index()
    );

    for v in &vpks {
        let start = Instant::now();
        let vmts = v.paths_with("materials", Some("vmt")).count();
        let elapsed = start.elapsed();
        if vmts > 0 {
            eprintln!("{:<32} {vmts:>6} materials/**.vmt listed in {elapsed:.3?}", name(v.path()));
        }
    }
    let total_vmts: usize = vpks.iter().map(|v| v.paths_with("materials/", Some(".vmt")).count()).sum();
    assert!(total_vmts > 1000, "{total_vmts}");
}

#[test]
#[ignore = "reads every file of every VPK (several GB)"]
fn verify_every_file() {
    let Some(gmod) = gmod_dir() else { return };
    let start = Instant::now();
    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut mismatches = Vec::new();
    for path in dir_vpks(&gmod) {
        let vpk = VpkArchive::open(&path).unwrap();
        let paths: Vec<&str> = vpk.paths().collect();
        let results: Vec<(u64, Option<String>)> = paths
            .par_iter()
            .map(|p| {
                let data = vpk.read_cow(p).unwrap().unwrap();
                let entry = vpk.entry(p).unwrap();
                assert_eq!(data.len() as u64, entry.size(), "{p}");
                let bad = (crc32fast::hash(&data) != entry.crc()).then(|| format!("{}: {p}", name(&path)));
                (data.len() as u64, bad)
            })
            .collect();
        files += paths.len();
        for (size, bad) in results {
            bytes += size;
            mismatches.extend(bad);
        }
    }
    eprintln!(
        "verified {files} files, {} MiB in {:.2?}, {} CRC mismatches: {mismatches:#?}",
        bytes / (1024 * 1024),
        start.elapsed(),
        mismatches.len()
    );
    assert!(mismatches.len() * 1000 <= files, "too many CRC mismatches");
}
