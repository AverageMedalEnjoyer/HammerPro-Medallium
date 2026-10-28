mod common;

use std::borrow::Cow;
use std::fs;
use std::sync::Arc;

use common::{GmaSpec, build_gma, compress_legacy, noise, scratch_dir};
use hp_gma::{GmaArchive, GmaError};
use rayon::prelude::*;

fn sample_spec(version: u8) -> GmaSpec {
    let mut spec = GmaSpec { version, required: vec!["hl2".to_owned()], ..GmaSpec::default() };
    spec = spec
        .file("materials/Models/Crate.VMT", b"\"VertexLitGeneric\" { \"$basetexture\" \"models/crate\" }")
        .file("materials/models/crate.vtf", &noise(20_000, 1))
        .file("models/crate.mdl", &noise(5000, 2))
        .file("lua/autorun/test.lua", b"print('hi')")
        .file("empty.txt", b"")
        .file("data/noext", b"x")
        .file("materials/models/other.vmt", b"\"UnlitGeneric\" {}");
    spec
}

fn check(gma: &GmaArchive, spec: &GmaSpec) {
    assert_eq!(gma.len(), spec.files.len());
    assert!(!gma.is_empty());
    assert_eq!(gma.format_version(), spec.version);
    assert_eq!(gma.steam_id(), spec.steam_id);
    assert_eq!(gma.timestamp(), spec.timestamp);
    assert_eq!(gma.name(), spec.name);
    assert_eq!(gma.raw_description(), spec.description);
    assert_eq!(gma.author(), spec.author);
    assert_eq!(gma.addon_version(), spec.addon_version);
    if spec.version > 1 {
        assert_eq!(gma.required_content(), spec.required);
    } else {
        assert!(gma.required_content().is_empty());
    }
    let total: u64 = spec.files.iter().map(|f| f.1.len() as u64).sum();
    assert_eq!(gma.data_size(), total);
    for (path, data, _) in &spec.files {
        let lower = path.to_ascii_lowercase();
        assert!(gma.contains(path));
        assert!(gma.contains(&path.replace('/', "\\").to_ascii_uppercase()));
        let entry = gma.entry(&lower).expect("entry");
        assert_eq!(entry.size(), data.len() as u64);
        assert_eq!(entry.crc(), crc32fast::hash(data));
        assert_eq!(gma.read(&lower).unwrap().as_ref(), Some(data));
        assert_eq!(gma.read_checked(path).unwrap().as_ref(), Some(data));
        let mut buf = vec![9; 3];
        assert!(gma.read_into(&lower, &mut buf).unwrap());
        assert_eq!(&buf, data);
        let cow = gma.read_cow(&lower).unwrap().unwrap();
        assert!(matches!(cow, Cow::Borrowed(_)));
        assert_eq!(&*cow, &data[..]);
    }
    assert_eq!(gma.read("missing.txt").unwrap(), None);
    assert!(!gma.read_into("missing.txt", &mut Vec::new()).unwrap());
    assert!(gma.read_cow("").unwrap().is_none());
    let paths: Vec<&str> = gma.paths().collect();
    let expected: Vec<String> = spec.files.iter().map(|f| f.0.to_ascii_lowercase()).collect();
    assert_eq!(paths, expected);
    assert_eq!(gma.paths().len(), spec.files.len());
    for (path, entry) in gma {
        assert_eq!(gma.entry(path), Some(entry));
    }
    assert_eq!(gma.iter().len(), spec.files.len());
}

#[test]
fn plain_roundtrip_all_versions() {
    let dir = scratch_dir("plain");
    for version in 1..=3 {
        let spec = sample_spec(version);
        let path = dir.join(format!("v{version}.gma"));
        fs::write(&path, build_gma(&spec)).unwrap();
        let gma = GmaArchive::open(&path).unwrap();
        assert!(!gma.is_legacy());
        assert_eq!(gma.path(), Some(path.as_path()));
        check(&gma, &spec);
        assert_eq!(gma.description(), "A test addon");
        assert_eq!(gma.addon_type(), Some("model"));
        assert_eq!(gma.tags(), ["build", "scenic"]);
        let dbg = format!("{gma:?}");
        assert!(dbg.contains("Test Addon") && dbg.contains("files: 7"), "{dbg}");

        let mem = GmaArchive::from_bytes(build_gma(&spec)).unwrap();
        assert_eq!(mem.path(), None);
        check(&mem, &spec);
    }
}

#[test]
fn without_trailing_crc_and_plain_description() {
    let spec = GmaSpec { trailing_crc: false, description: "Old style plain description".to_owned(), ..sample_spec(3) };
    let gma = GmaArchive::from_bytes(build_gma(&spec)).unwrap();
    check(&gma, &spec);
    assert_eq!(gma.description(), "Old style plain description");
    assert_eq!(gma.addon_type(), None);
    assert!(gma.tags().is_empty());
}

#[test]
fn legacy_roundtrip() {
    let dir = scratch_dir("legacy");
    for known_size in [true, false] {
        let spec = sample_spec(3);
        let compressed = compress_legacy(&build_gma(&spec), known_size);
        let path = dir.join(format!("123_{known_size}_legacy.bin"));
        fs::write(&path, &compressed).unwrap();
        let gma = GmaArchive::open(&path).unwrap();
        assert!(gma.is_legacy());
        check(&gma, &spec);
        assert_eq!(gma.tags(), ["build", "scenic"]);

        let mem = GmaArchive::from_bytes(compressed).unwrap();
        assert!(mem.is_legacy());
        check(&mem, &spec);
    }
}

#[test]
fn legacy_with_large_header() {
    let mut spec = GmaSpec::default();
    for i in 0..6000 {
        spec = spec.file(
            &format!("materials/some/really/long/directory/name/for/testing/texture_{i:05}.vmt"),
            format!("\"LightmappedGeneric\" {{ \"$basetexture\" \"t{i}\" }}").as_bytes(),
        );
    }
    let gma = GmaArchive::from_bytes(compress_legacy(&build_gma(&spec), true)).unwrap();
    assert_eq!(gma.len(), 6000);
    assert_eq!(gma.paths_with("materials", Some("vmt")).count(), 6000);
    assert_eq!(
        gma.read("materials/some/really/long/directory/name/for/testing/texture_05999.vmt").unwrap().unwrap(),
        b"\"LightmappedGeneric\" { \"$basetexture\" \"t5999\" }"
    );
}

#[test]
fn paths_with_prefix_and_extension() {
    let gma = GmaArchive::from_bytes(build_gma(&sample_spec(3))).unwrap();
    let collect =
        |prefix: &str, ext: Option<&str>| -> Vec<String> { gma.paths_with(prefix, ext).map(str::to_owned).collect() };
    assert_eq!(collect("materials", Some("vmt")), ["materials/models/crate.vmt", "materials/models/other.vmt"]);
    assert_eq!(collect("MATERIALS\\Models\\", Some(".VMT")), collect("materials", Some("vmt")));
    assert_eq!(collect("materials/mod", Some("vmt")), Vec::<String>::new());
    assert_eq!(collect("", Some("")), ["data/noext"]);
    assert_eq!(collect("lua", None), ["lua/autorun/test.lua"]);
    assert_eq!(collect("", None).len(), gma.len());
    assert_eq!(collect("", Some("mdl")), ["models/crate.mdl"]);
}

#[test]
fn duplicates_keep_first_and_offsets_stay_right() {
    let spec =
        GmaSpec::default().file("a.txt", b"first").file("A.TXT", b"second copy").file("b.txt", b"after duplicate");
    let gma = GmaArchive::from_bytes(build_gma(&spec)).unwrap();
    assert_eq!(gma.len(), 2);
    assert_eq!(gma.read("a.txt").unwrap().unwrap(), b"first");
    assert_eq!(gma.read("b.txt").unwrap().unwrap(), b"after duplicate");
}

#[test]
fn crc_mismatch_is_reported() {
    let mut spec = GmaSpec::default().file("bad.bin", &noise(100, 3));
    spec.files[0].2 = Some(0x0BAD_F00D);
    let gma = GmaArchive::from_bytes(build_gma(&spec)).unwrap();
    assert_eq!(gma.read("bad.bin").unwrap().unwrap(), spec.files[0].1);
    match gma.read_checked("bad.bin") {
        Err(GmaError::CrcMismatch { path, expected, actual }) => {
            assert_eq!(path, "bad.bin");
            assert_eq!(expected, 0x0BAD_F00D);
            assert_eq!(actual, crc32fast::hash(&spec.files[0].1));
        }
        other => panic!("expected CRC mismatch, got {other:?}"),
    }
    let mut spec = GmaSpec::default().file("nocrc.bin", &noise(100, 4));
    spec.files[0].2 = Some(0);
    let gma = GmaArchive::from_bytes(build_gma(&spec)).unwrap();
    assert_eq!(gma.entry("nocrc.bin").unwrap().crc(), 0);
    assert_eq!(gma.read_checked("nocrc.bin").unwrap().unwrap(), spec.files[0].1);
}

#[test]
fn truncated_archives() {
    let data = build_gma(&GmaSpec { trailing_crc: false, ..sample_spec(3) });
    let err = GmaArchive::from_bytes(data[..data.len() - 1].to_vec()).unwrap_err();
    assert!(matches!(err, GmaError::Corrupt(_)), "{err}");
    for cut in 0..data.len() {
        assert!(GmaArchive::from_bytes(data[..cut].to_vec()).is_err(), "cut {cut}");
    }
    let compressed = compress_legacy(&build_gma(&sample_spec(3)), true);
    for cut in (13..compressed.len()).step_by(97) {
        if let Ok(gma) = GmaArchive::from_bytes(compressed[..cut].to_vec()) {
            let first = gma.paths().next().map(str::to_owned);
            if let Some(p) = first {
                assert!(matches!(gma.read(&p), Err(GmaError::Decompress(_))), "cut {cut}");
                assert!(matches!(gma.read(&p), Err(GmaError::Decompress(_))));
            }
        }
    }
}

#[test]
fn bad_inputs() {
    assert!(matches!(GmaArchive::from_bytes(Vec::new()), Err(GmaError::BadMagic)));
    assert!(matches!(GmaArchive::from_bytes(b"PK\x03\x04 zip file".to_vec()), Err(GmaError::BadMagic)));
    let mut v4 = build_gma(&sample_spec(3));
    v4[4] = 4;
    assert!(matches!(GmaArchive::from_bytes(v4), Err(GmaError::UnsupportedVersion(4))));
    let not_gma = compress_legacy(b"this is not an addon at all, just text", true);
    assert!(matches!(GmaArchive::from_bytes(not_gma), Err(GmaError::BadMagic)));
    let dir = scratch_dir("bad_inputs");
    assert!(matches!(GmaArchive::open(dir.join("missing.gma")), Err(GmaError::Io { .. })));
}

#[test]
fn concurrent_first_reads_of_legacy_archive() {
    let mut spec = GmaSpec::default();
    for i in 0..500 {
        spec = spec.file(&format!("dir{}/f{i}.bin", i % 7), &noise(50 + i * 13, i as u64 + 1));
    }
    let gma = Arc::new(GmaArchive::from_bytes(compress_legacy(&build_gma(&spec), true)).unwrap());
    spec.files.par_iter().for_each(|(path, data, _)| {
        assert_eq!(&gma.read_checked(path).unwrap().unwrap(), data);
    });
    let handles: Vec<_> = (0..4)
        .map(|t| {
            let gma = Arc::clone(&gma);
            std::thread::spawn(move || {
                gma.paths().skip(t).step_by(4).filter(|p| gma.read(p).unwrap().is_some()).count()
            })
        })
        .collect();
    let read: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
    assert_eq!(read, 500);
}

#[test]
fn mutated_archives_never_panic() {
    let plain = build_gma(&sample_spec(3));
    let legacy = compress_legacy(&plain, true);
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut opened = 0;
    for round in 0..2000 {
        let source = if round % 2 == 0 { &plain } else { &legacy };
        let mut bytes = source.clone();
        for _ in 0..1 + round % 3 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let pos = (seed % bytes.len() as u64) as usize;
            bytes[pos] = (seed >> 40) as u8;
        }
        if let Ok(gma) = GmaArchive::from_bytes(bytes) {
            opened += 1;
            let paths: Vec<String> = gma.paths().map(str::to_owned).collect();
            for p in &paths {
                let _ = gma.read_checked(p);
                let _ = gma.read_cow(p);
            }
            let _ = gma.description();
        }
    }
    assert!(opened > 0);
}
