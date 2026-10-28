use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hp_gma::GmaArchive;
use rayon::prelude::*;

const DEFAULT_WORKSHOP_DIR: &str = r"D:\SteamLibrary\steamapps\workshop\content\4000";

fn workshop_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("HAMMERPRO_WORKSHOP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_WORKSHOP_DIR));
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("skipping: workshop folder not found at {}", dir.display());
        None
    }
}

fn addon_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(ids) = std::fs::read_dir(root) else {
        return out;
    };
    for id in ids.flatten() {
        let Ok(files) = std::fs::read_dir(id.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
            if matches!(ext.as_deref(), Some("gma" | "bin")) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn short(path: &Path) -> String {
    let parent = path.parent().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    format!("{parent}/{name}")
}

#[test]
fn open_every_workshop_addon_and_read_samples() {
    let Some(root) = workshop_dir() else { return };
    let addons = addon_files(&root);
    if addons.is_empty() {
        eprintln!("skipping: no addons in {}", root.display());
        return;
    }
    let mut files = 0usize;
    let mut data_bytes = 0u64;
    let mut legacy = 0usize;
    let mut total_open = Duration::ZERO;
    let mut slowest = (Duration::ZERO, String::new());
    let mut decompress_total = Duration::ZERO;
    let mut checked = 0usize;
    let mut crc_mismatches = Vec::new();
    let mut no_crc = 0usize;
    for path in &addons {
        let start = Instant::now();
        let gma = GmaArchive::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let open = start.elapsed();
        total_open += open;
        if open > slowest.0 {
            slowest = (open, short(path));
        }
        files += gma.len();
        data_bytes += gma.data_size();
        legacy += usize::from(gma.is_legacy());

        let paths: Vec<&str> = gma.paths().collect();
        let picks: Vec<&str> = [0, paths.len() / 2, paths.len().saturating_sub(1)]
            .into_iter()
            .filter_map(|i| paths.get(i).copied())
            .collect();
        let start = Instant::now();
        for p in &picks {
            let data = gma.read(p).unwrap_or_else(|e| panic!("{}: {p}: {e}", path.display()));
            let data = data.expect("listed path is readable");
            let entry = gma.entry(p).expect("listed path has an entry");
            assert_eq!(data.len() as u64, entry.size(), "{p}");
        }
        let reads = start.elapsed();
        for p in &picks {
            let entry = gma.entry(p).expect("listed path has an entry");
            checked += 1;
            match gma.read_checked(p) {
                Ok(_) if entry.crc() == 0 => no_crc += 1,
                Ok(_) => {}
                Err(e) => crc_mismatches.push(format!("{}: {p}: {e}", short(path))),
            }
        }
        if gma.is_legacy() {
            decompress_total += reads;
        }
        eprintln!(
            "{:<48} {:>6} files {:>9} KiB  v{} {:<6} open {:>9.3?}  3 reads {:>9.3?}  {:?} {:?} {:?}",
            short(path),
            gma.len(),
            gma.data_size() / 1024,
            gma.format_version(),
            if gma.is_legacy() { "legacy" } else { "" },
            open,
            reads,
            gma.name(),
            gma.addon_type().unwrap_or("-"),
            gma.tags()
        );
    }
    eprintln!(
        "{} addons ({legacy} legacy), {files} files, {} MiB of data; total open {:.3?} \
         (slowest {} {:.3?}); legacy first reads (full decompression) {:.3?}; \
         {checked} files CRC checked ({no_crc} without stored CRC), {} CRC mismatches {:?}",
        addons.len(),
        data_bytes / (1024 * 1024),
        total_open,
        slowest.1,
        slowest.0,
        decompress_total,
        crc_mismatches.len(),
        crc_mismatches
    );
    assert!(crc_mismatches.len() * 100 <= checked, "too many CRC mismatches");
}

#[test]
fn open_all_in_parallel() {
    let Some(root) = workshop_dir() else { return };
    let addons = addon_files(&root);
    let start = Instant::now();
    let opened: Vec<GmaArchive> =
        addons.par_iter().map(|p| GmaArchive::open(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).collect();
    let files: usize = opened.iter().map(GmaArchive::len).sum();
    let vmts: usize = opened.iter().map(|g| g.paths_with("materials", Some("vmt")).count()).sum();
    eprintln!(
        "opened {} addons ({files} files, {vmts} materials/**.vmt) in parallel in {:.3?}",
        opened.len(),
        start.elapsed()
    );
}

#[test]
fn known_addon_metadata() {
    let Some(root) = workshop_dir() else { return };
    let path = root.join("104603291").join("104603291.gma");
    if !path.is_file() {
        eprintln!("skipping: {} not found", path.display());
        return;
    }
    let gma = GmaArchive::open(&path).unwrap();
    assert_eq!(gma.name(), "Extended Spawnmenu");
    assert_eq!(gma.format_version(), 3);
    assert_eq!(gma.addon_type(), Some("tool"));
    assert_eq!(gma.tags(), ["build", "scenic"]);
    assert!(gma.raw_description().trim_start().starts_with('{'));
    assert!(!gma.is_empty());
    for p in gma.paths() {
        assert_eq!(gma.read_checked(p).unwrap().map(|d| d.len() as u64), gma.entry(p).map(|e| e.size()));
    }
}

#[test]
fn legacy_bins_decompress_fully() {
    let Some(root) = workshop_dir() else { return };
    let bins: Vec<PathBuf> = addon_files(&root)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bin")))
        .collect();
    if bins.is_empty() {
        eprintln!("skipping: no legacy .bin addons");
        return;
    }
    let start = Instant::now();
    let results: Vec<(usize, u64, usize)> = bins
        .par_iter()
        .map(|path| {
            let gma = GmaArchive::open(path).unwrap();
            assert!(gma.is_legacy(), "{}", path.display());
            let mut bad = 0;
            for (p, entry) in &gma {
                let data = gma.read_cow(p).unwrap().unwrap();
                assert_eq!(data.len() as u64, entry.size());
                bad += usize::from(entry.crc() != 0 && crc32fast::hash(&data) != entry.crc());
            }
            (gma.len(), gma.data_size(), bad)
        })
        .collect();
    let files: usize = results.iter().map(|r| r.0).sum();
    let bytes: u64 = results.iter().map(|r| r.1).sum();
    let bad: usize = results.iter().map(|r| r.2).sum();
    eprintln!(
        "{} legacy addons: {files} files, {} MiB decompressed and CRC checked in {:.3?} ({bad} mismatches)",
        bins.len(),
        bytes / (1024 * 1024),
        start.elapsed()
    );
    assert!(bad * 100 <= files);
}
