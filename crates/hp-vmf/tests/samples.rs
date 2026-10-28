use std::path::{Path, PathBuf};
use std::time::Instant;

use hp_vmf::{CordonStyle, Map};

const TTT_DIR: &str = r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\gamemodes\terrortown\mapexamples";
const EXTRA_SAMPLE: &str = r"C:\Users\oracle\Desktop\hammermcptest\test.vmf";

fn sample_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(TTT_DIR)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("vmf")))
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    if Path::new(EXTRA_SAMPLE).is_file() {
        paths.push(PathBuf::from(EXTRA_SAMPLE));
    }
    paths
}

fn load_sample(name: &str) -> Option<(Vec<u8>, Map)> {
    let path = Path::new(TTT_DIR).join(name);
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("skipping: {} not found", path.display());
        return None;
    };
    let map = hp_vmf::parse_bytes(&bytes).expect("sample parses");
    Some((bytes, map))
}

#[test]
fn round_trip_every_sample() {
    let paths = sample_paths();
    if paths.is_empty() {
        eprintln!("skipping: no sample VMF found");
        return;
    }
    for path in paths {
        let bytes = std::fs::read(&path).unwrap();
        let t0 = Instant::now();
        let m1 = hp_vmf::parse_bytes(&bytes).unwrap();
        let parse_time = t0.elapsed();
        let t0 = Instant::now();
        let t1 = hp_vmf::write(&m1);
        let write_time = t0.elapsed();
        let m2 = hp_vmf::parse(&t1).unwrap();
        assert!(m1 == m2, "{}: parse(write(map)) differs", path.display());
        let t2 = hp_vmf::write(&m2);
        assert!(t1 == t2, "{}: second write differs", path.display());

        let mut reference = hp_vmf::from_kv(hp_kv::parse_bytes(&bytes).unwrap()).unwrap();
        reference.format = m1.format;
        assert!(m1 == reference, "{}: fast path differs from reference path", path.display());

        assert_same_kv(&hp_kv::parse_bytes(&bytes).unwrap(), &hp_kv::parse(&t1).unwrap(), &path);

        eprintln!(
            "{}: {} bytes, parse {:?}, write {:?}",
            path.file_name().unwrap().to_string_lossy(),
            bytes.len(),
            parse_time,
            write_time
        );
    }
}

fn assert_same_kv(original: &hp_kv::Block, written: &hp_kv::Block, path: &Path) {
    fn norm(v: &str) -> String {
        v.split(' ')
            .map(|t| {
                let inner = t.trim_matches(|c| matches!(c, '(' | ')' | '[' | ']'));
                match inner.parse::<f64>() {
                    Ok(x) => t.replace(inner, &format!("{}", if x == 0.0 { 0.0 } else { x })),
                    Err(_) => t.to_owned(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn walk(b: &hp_kv::Block, prefix: &str, out: &mut Vec<String>) {
        for (k, v) in &b.props {
            out.push(format!("{prefix}/{}={}", k.to_ascii_lowercase(), norm(v)));
        }
        for c in &b.children {
            walk(c, &format!("{prefix}/{}", c.name.to_ascii_lowercase()), out);
        }
    }
    let mut a = Vec::new();
    let mut b = Vec::new();
    walk(original, "", &mut a);
    walk(written, "", &mut b);
    a.sort();
    b.sort();
    let mut missing = Vec::new();
    let mut bi = b.iter().peekable();
    for line in &a {
        loop {
            match bi.peek() {
                Some(x) if *x < line => {
                    bi.next();
                }
                Some(x) if *x == line => {
                    bi.next();
                    break;
                }
                _ => {
                    missing.push(line.clone());
                    break;
                }
            }
        }
    }
    assert!(missing.is_empty(), "{}: lines lost on write: {:?}", path.display(), &missing[..missing.len().min(10)]);
}

#[test]
fn ttt_traps_spot_checks() {
    let Some((_, map)) = load_sample("ttt_traps.vmf") else { return };
    assert_eq!(map.version_info.editor_build, 5004);
    assert_eq!(map.version_info.map_version, 17);
    assert_eq!(map.world.id, 1);
    assert_eq!(map.world.classname, "worldspawn");
    assert_eq!(map.world.get("skyname"), Some("sky_day01_01"));
    assert_eq!(map.world.get("mapversion"), None);
    assert_eq!(map.entities.len(), 65);
    assert_eq!(map.world.solids.len(), 24);
    assert_eq!(map.entities.iter().map(|e| e.solids.len()).sum::<usize>(), 51);
    assert_eq!(map.all_solids().count(), 75);
    assert_eq!(map.all_solids().map(|s| s.sides.len()).sum::<usize>(), 450);
    assert_eq!(map.entities.iter().map(|e| e.connections.len()).sum::<usize>(), 34);
    assert_eq!(map.entities.iter().filter(|e| !e.connections.is_empty()).count(), 15);
    assert_eq!(map.world.groups.len(), 5);
    assert_eq!(map.entities.iter().filter(|e| e.classname == "func_detail").count(), 11);
    assert!(map.entities.iter().all(|e| e.get("classname").is_none() && e.get("id").is_none()));
    assert_eq!(map.cordons.style, CordonStyle::Legacy);
    assert_eq!(map.cameras.active, -1);

    let first = &map.world.solids[0];
    assert_eq!(first.id, 284);
    assert_eq!(first.sides[0].id, 631);
    assert_eq!(first.sides[0].material, "DEV/DEV_MEASUREGENERIC01");
    assert_eq!(first.editor.color, [0, 139, 112]);
    assert_eq!(first.sides[0].normal(), glam::DVec3::Z);

    for c in map.entities.iter().flat_map(|e| &e.connections) {
        assert_eq!(c.separator, ',');
        assert!(!c.target.is_empty() && !c.input.is_empty());
    }
    assert!(map.max_object_id() >= 2019);
    assert!(map.entity_by_id(2019).is_some_and(|e| e.classname == "ttt_damageowner"));
}

#[test]
fn ttt_traitorcheck_esc_connections_and_new_cordons() {
    let Some((_, map)) = load_sample("ttt_traitorcheck.vmf") else { return };
    let conns: Vec<_> = map.entities.iter().flat_map(|e| &e.connections).collect();
    assert_eq!(conns.len(), 27);
    assert!(conns.iter().all(|c| c.separator == '\x1b'));
    let color = conns.iter().find(|c| c.output == "OnEqualTo").unwrap();
    assert_eq!(
        (color.target.as_str(), color.input.as_str(), color.param.as_str(), color.delay, color.times),
        ("indlight", "Color", "0 255 0", 0.0, -1)
    );
    let delayed = conns.iter().find(|c| c.delay > 1.0).unwrap();
    assert_eq!(delayed.delay, 1.9);
    assert_eq!(map.cordons.style, CordonStyle::Modern);
    assert_eq!(map.cordons.cordons.len(), 1);
    assert_eq!(map.cordons.cordons[0].name, "cordon");
    assert_eq!(map.cameras.active, 0);
    assert_eq!(map.cameras.cameras.len(), 1);
    let text = hp_vmf::write(&map);
    assert!(text.contains("\"OnEqualTo\" \"indlight\x1bColor\x1b0 255 0\x1b0\x1b-1\""));
    assert!(text.contains("cordons\n{\n\t\"active\" \"0\"\n\tcordon\n"));
}

#[test]
fn hammer_output_is_reproduced_byte_for_byte() {
    for name in ["ttt_traps.vmf", "ttt_traitorcheck.vmf", "ttt_weaponcheck.vmf"] {
        let Some((bytes, map)) = load_sample(name) else { continue };
        let original = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
        let written = hp_vmf::write(&map);
        let a: Vec<&str> = original.lines().collect();
        let b: Vec<&str> = written.lines().collect();
        let first_diff = a.iter().zip(&b).position(|(x, y)| x != y);
        assert!(
            first_diff.is_none() && a.len() == b.len(),
            "{name}: first difference at line {:?}: {:?} vs {:?}",
            first_diff.map(|i| i + 1),
            first_diff.and_then(|i| a.get(i)),
            first_diff.and_then(|i| b.get(i))
        );
        assert!(hp_vmf::write_bytes(&map).0 == bytes, "{name}: bytes differ");

        let crlf = original.replace('\n', "\r\n");
        let m = hp_vmf::parse_bytes(crlf.as_bytes()).unwrap();
        assert_eq!(m.format.line_ending, hp_vmf::LineEnding::CrLf);
        assert!(hp_vmf::write_bytes(&m).0 == crlf.as_bytes(), "{name}: CRLF bytes differ");

        let accented = crlf.replacen(
            "\"classname\" \"worldspawn\"",
            "\"classname\" \"worldspawn\"\r\n\t\"comment\" \"\u{e9}t\u{e9}\"",
            1,
        );
        let cp1252 = hp_kv::encode_windows_1252(&accented).expect("Windows-1252 text");
        let m = hp_vmf::parse_bytes(&cp1252).unwrap();
        assert_eq!(m.format.encoding, hp_vmf::TextEncoding::Windows1252);
        assert_eq!(m.world.get("comment"), Some("\u{e9}t\u{e9}"));
        assert!(hp_vmf::write_bytes(&m).0 == cp1252, "{name}: Windows-1252 bytes differ");
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

#[test]
fn mutated_samples_never_panic_and_paths_agree() {
    let paths = sample_paths();
    if paths.is_empty() {
        eprintln!("skipping: no sample VMF found");
        return;
    }
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let tokens: [&[u8]; 9] = [b"{", b"}", b"\"", b"//", b"[$X]", b"\n", b" hidden { ", b"\x1b", b"\xff"];
    for path in &paths {
        let original = std::fs::read(path).unwrap();
        for _ in 0..60 {
            let mut bytes = original.clone();
            for _ in 0..(1 + rng.below(6)) {
                let at = rng.below(bytes.len());
                match rng.below(3) {
                    0 => {
                        let len = rng.below(40).min(bytes.len() - at);
                        bytes.drain(at..at + len);
                    }
                    1 => {
                        let t = tokens[rng.below(tokens.len())];
                        bytes.splice(at..at, t.iter().copied());
                    }
                    _ => bytes.truncate(at),
                }
            }
            let fast = hp_vmf::parse_bytes(&bytes);
            let reference = hp_vmf::from_kv(hp_kv::parse_bytes(&bytes).unwrap()).map(|mut m| {
                if let Ok(f) = &fast {
                    m.format = f.format;
                }
                m
            });
            match (fast, reference) {
                (Ok(a), Ok(b)) => {
                    assert!(a == b, "fast and reference paths disagree on a mutation of {}", path.display());
                    let t1 = hp_vmf::write(&a);
                    let a2 = hp_vmf::parse(&t1).unwrap();
                    assert!(hp_vmf::write(&a2) == t1, "unstable write on a mutation of {}", path.display());
                }
                (Err(_), Err(_)) => {}
                (a, b) => panic!("paths disagree: fast ok={} reference ok={}", a.is_ok(), b.is_ok()),
            }
        }
    }
}
