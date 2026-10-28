mod common;

use std::borrow::Cow;
use std::fs;
use std::sync::Arc;

use common::{TestFile, noise, scratch_dir, write_vpk};
use hp_vpk::{VpkArchive, VpkError};
use rayon::prelude::*;

fn sample_files() -> Vec<TestFile> {
    vec![
        TestFile::new("materials/Concrete/Floor01.VMT", b"\"LightmappedGeneric\"\n{\n}\n", 26, Some(0)),
        TestFile::new("materials/concrete/floor01.vtf", &noise(5000, 1), 16, Some(0)),
        TestFile::new("models/props/crate.mdl", &noise(70_000, 2), 0, Some(1)),
        TestFile::new("models/props/crate.vvd", &noise(1234, 3), 0, Some(2)),
        TestFile::new("scripts/game.txt", b"embedded text data", 0, None),
        TestFile::new("scripts/other.txt", b"PRELOAD+embedded", 8, None),
        TestFile::new("empty.dat", b"", 0, Some(0)),
        TestFile::new("readme.txt", b"root file", 0, Some(0)),
        TestFile::new("bin/noext", b"no extension", 0, None),
        TestFile::new("materials/concrete/floor02.vmt", b"\"UnlitGeneric\" {}", 0, Some(0)),
    ]
}

fn check_archive(vpk: &VpkArchive, files: &[TestFile]) {
    assert_eq!(vpk.len(), files.len());
    assert!(!vpk.is_empty());
    for f in files {
        let lower = f.path.to_ascii_lowercase();
        assert!(vpk.contains(&lower), "{lower}");
        assert!(vpk.contains(&f.path.replace('/', "\\").to_ascii_uppercase()));
        let entry = vpk.entry(&lower).expect("entry");
        assert_eq!(entry.size(), f.data.len() as u64, "{lower}");
        assert_eq!(entry.crc(), crc32fast::hash(&f.data));
        assert_eq!(usize::from(entry.preload_len()), f.preload);
        assert_eq!(entry.archive_index(), f.archive);

        let data = vpk.read(&lower).unwrap().expect("present");
        assert_eq!(data, f.data, "{lower}");
        let checked = vpk.read_checked(&lower).unwrap().expect("present");
        assert_eq!(checked, f.data);
        let mut buf = vec![1, 2, 3];
        assert!(vpk.read_into(&f.path, &mut buf).unwrap());
        assert_eq!(buf, f.data);
        let cow = vpk.read_cow(&lower).unwrap().expect("present");
        assert_eq!(&*cow, &f.data[..]);
        let split = f.preload > 0 && f.preload < f.data.len();
        assert_eq!(matches!(cow, Cow::Owned(_)), split, "{lower}");
    }
    assert_eq!(vpk.read("missing/file.txt").unwrap(), None);
    assert!(!vpk.read_into("missing/file.txt", &mut Vec::new()).unwrap());
    assert!(vpk.read_cow("missing").unwrap().is_none());
    assert!(vpk.entry("").is_none());

    let mut paths: Vec<&str> = vpk.paths().collect();
    assert_eq!(paths.len(), vpk.paths().len());
    paths.sort_unstable();
    let mut expected: Vec<String> = files.iter().map(|f| f.path.to_ascii_lowercase()).collect();
    expected.sort_unstable();
    assert_eq!(paths, expected);
    assert_eq!(vpk.iter().count(), files.len());
    for (path, entry) in vpk {
        assert_eq!(vpk.entry(path), Some(entry));
    }
}

#[test]
fn v1_and_v2_roundtrip() {
    for version in [1, 2] {
        let dir = scratch_dir(&format!("roundtrip_v{version}"));
        let files = sample_files();
        let path = write_vpk(&dir, "pak01", version, &files);
        let vpk = VpkArchive::open(&path).unwrap();
        assert_eq!(vpk.version(), version);
        assert_eq!(vpk.part_count(), 3);
        assert_eq!(vpk.path(), path);
        assert_eq!(vpk.part_path(2), dir.join("pak01_002.vpk"));
        check_archive(&vpk, &files);
        let dbg = format!("{vpk:?}");
        assert!(dbg.contains("VpkArchive") && dbg.contains("files: 10"), "{dbg}");
    }
}

#[test]
fn single_file_vpk_without_parts() {
    let dir = scratch_dir("single");
    let files = vec![TestFile::new("a/b.txt", b"hello", 2, None), TestFile::new("c.txt", &noise(100_000, 9), 0, None)];
    let path = write_vpk(&dir, "single", 2, &files);
    let renamed = dir.join("single.vpk");
    fs::rename(&path, &renamed).unwrap();
    let vpk = VpkArchive::open(&renamed).unwrap();
    assert_eq!(vpk.part_count(), 0);
    check_archive(&vpk, &files);
}

#[test]
fn paths_with_prefix_and_extension() {
    let dir = scratch_dir("paths_with");
    let path = write_vpk(&dir, "pak01", 2, &sample_files());
    let vpk = VpkArchive::open(path).unwrap();
    let sorted = |it: &mut dyn Iterator<Item = &str>| {
        let mut v: Vec<String> = it.map(str::to_owned).collect();
        v.sort();
        v
    };
    assert_eq!(
        sorted(&mut vpk.paths_with("materials", Some("vmt"))),
        ["materials/concrete/floor01.vmt", "materials/concrete/floor02.vmt"]
    );
    assert_eq!(
        sorted(&mut vpk.paths_with("MATERIALS\\concrete\\", Some(".VMT"))),
        ["materials/concrete/floor01.vmt", "materials/concrete/floor02.vmt"]
    );
    assert_eq!(sorted(&mut vpk.paths_with("materials/conc", Some("vmt"))), Vec::<String>::new());
    assert_eq!(sorted(&mut vpk.paths_with("", Some("txt"))), ["readme.txt", "scripts/game.txt", "scripts/other.txt"]);
    assert_eq!(sorted(&mut vpk.paths_with("", Some(""))), ["bin/noext"]);
    assert_eq!(sorted(&mut vpk.paths_with("models", None)), ["models/props/crate.mdl", "models/props/crate.vvd"]);
    assert_eq!(vpk.paths_with("", None).count(), vpk.len());
    assert_eq!(vpk.paths_with("nothing", None).count(), 0);
    assert_eq!(vpk.paths_with("", Some("nope")).count(), 0);
}

#[test]
fn crc_mismatch_is_reported() {
    let dir = scratch_dir("crc");
    let mut f = TestFile::new("bad/file.bin", &noise(300, 4), 10, Some(0));
    f.crc_override = Some(0xDEAD_BEEF);
    let path = write_vpk(&dir, "pak01", 1, &[f.clone()]);
    let vpk = VpkArchive::open(path).unwrap();
    assert_eq!(vpk.read("bad/file.bin").unwrap().unwrap(), f.data);
    match vpk.read_checked("bad/file.bin") {
        Err(VpkError::CrcMismatch { path, expected, actual }) => {
            assert_eq!(path, "bad/file.bin");
            assert_eq!(expected, 0xDEAD_BEEF);
            assert_eq!(actual, crc32fast::hash(&f.data));
        }
        other => panic!("expected CRC mismatch, got {other:?}"),
    }
}

#[test]
fn missing_and_truncated_parts() {
    let dir = scratch_dir("parts");
    let files = sample_files();
    let path = write_vpk(&dir, "pak01", 2, &files);
    let part1 = dir.join("pak01_001.vpk");
    let part1_bytes = fs::read(&part1).unwrap();
    fs::remove_file(&part1).unwrap();
    let truncated = dir.join("pak01_002.vpk");
    let len = fs::metadata(&truncated).unwrap().len();
    fs::write(&truncated, noise(len as usize / 2, 5)).unwrap();

    let vpk = VpkArchive::open(&path).unwrap();
    match vpk.read("models/props/crate.mdl") {
        Err(e @ VpkError::Io { .. }) => assert!(e.to_string().contains("pak01_001.vpk"), "{e}"),
        other => panic!("expected I/O error, got {other:?}"),
    }
    match vpk.read("models/props/crate.vvd") {
        Err(VpkError::OutOfBounds { path, archive }) => {
            assert_eq!(path, "models/props/crate.vvd");
            assert_eq!(archive, truncated);
        }
        other => panic!("expected out of bounds, got {other:?}"),
    }
    assert!(vpk.read("materials/concrete/floor01.vtf").unwrap().is_some());
    fs::write(&part1, part1_bytes).unwrap();
    assert_eq!(vpk.read("models/props/crate.mdl").unwrap().unwrap().len(), 70_000);
}

#[test]
fn concurrent_reads() {
    let dir = scratch_dir("concurrent");
    let files: Vec<TestFile> = (0..2000)
        .map(|i| {
            let data = noise(100 + i * 7, i as u64 + 1);
            TestFile::new(&format!("dir{}/file{i}.bin", i % 13), &data, i % 40, Some((i % 5) as u16))
        })
        .collect();
    let path = write_vpk(&dir, "pak01", 2, &files);
    let vpk = Arc::new(VpkArchive::open(path).unwrap());
    files.par_iter().for_each(|f| {
        let data = vpk.read_checked(&f.path).unwrap().unwrap();
        assert_eq!(data, f.data);
    });
    let handles: Vec<_> = (0..8)
        .map(|t| {
            let vpk = Arc::clone(&vpk);
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let mut total = 0usize;
                for p in vpk.paths().skip(t).step_by(8) {
                    assert!(vpk.read_into(p, &mut buf).unwrap());
                    total += buf.len();
                }
                total
            })
        })
        .collect();
    let total: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
    assert_eq!(total, files.iter().map(|f| f.data.len()).sum::<usize>());
}

#[test]
fn garbage_and_truncated_dir_files_never_panic() {
    let dir = scratch_dir("garbage");
    let bad = dir.join("bad_dir.vpk");
    for (i, bytes) in [vec![], vec![0x34], noise(11, 1), noise(5000, 2)].into_iter().enumerate() {
        fs::write(&bad, &bytes).unwrap();
        assert!(VpkArchive::open(&bad).is_err(), "case {i}");
    }
    assert!(matches!(VpkArchive::open(dir.join("missing_dir.vpk")), Err(VpkError::Io { .. })));

    let path = write_vpk(&dir, "pak01", 2, &sample_files());
    let valid = fs::read(&path).unwrap();
    for cut in 0..valid.len() - 48 - 40 {
        fs::write(&bad, &valid[..cut]).unwrap();
        assert!(VpkArchive::open(&bad).is_err(), "cut {cut}");
    }
}

#[test]
fn mutated_dir_files_never_panic() {
    let dir = scratch_dir("mutated");
    let path = write_vpk(&dir, "pak01", 2, &sample_files());
    let valid = fs::read(&path).unwrap();
    let target = dir.join("pak01_dir.vpk");
    let mut opened = 0;
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for round in 0..3000 {
        let mut bytes = valid.clone();
        for _ in 0..1 + round % 4 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let pos = (seed % bytes.len() as u64) as usize;
            bytes[pos] = (seed >> 32) as u8;
        }
        fs::write(&target, &bytes).unwrap();
        if let Ok(vpk) = VpkArchive::open(&target) {
            opened += 1;
            for p in vpk.paths() {
                let _ = vpk.read_checked(p);
                let _ = vpk.read_cow(p);
            }
            let _ = vpk.paths_with("materials", Some("vmt")).count();
        }
    }
    assert!(opened > 0);
}
