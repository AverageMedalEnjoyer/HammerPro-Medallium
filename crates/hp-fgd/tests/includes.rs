use std::fs;
use std::path::{Path, PathBuf};

use hp_fgd::{Fgd, FgdError};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-fgd/scratch").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn write(dir: &Path, rel: &str, text: &[u8]) -> PathBuf {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(&path, text).expect("write scratch file");
    path
}

#[test]
fn includes_are_relative_once_and_recursion_safe() {
    let dir = scratch("includes_basic");
    let main = write(
        &dir,
        "main.fgd",
        b"@include \"sub\\\\Base.FGD\"\n@include \"loop.fgd\"\n@include \"missing.fgd\"\n\
          @PointClass base(Targetname) = info_thing : \"Thing\" []\n",
    );
    write(
        &dir,
        "sub/base.fgd",
        b"@mapsize(-32768, 32768)\n@BaseClass = Targetname [ targetname(target_source) : \"Name\" ]\n\
          @include \"../main.fgd\"\n@include \"common.fgd\"\n",
    );
    write(&dir, "sub/common.fgd", b"@PointClass = common_ent []\n");
    write(&dir, "loop.fgd", b"@include \"loop.fgd\"\n@PointClass = loop_ent []\n");

    let fgd = Fgd::load(&main).expect("load main.fgd");
    let names: Vec<_> = fgd.classes.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Targetname", "common_ent", "loop_ent", "info_thing"]);
    assert_eq!(fgd.map_size, Some((-32768, 32768)));

    assert_eq!(fgd.warnings.len(), 3, "{:#?}", fgd.warnings);
    assert!(fgd.warnings.iter().filter(|w| w.contains("recursive")).count() == 2);
    assert!(fgd.warnings.iter().any(|w| w.contains("missing.fgd") && w.contains("main.fgd:3")));

    let resolved = fgd.resolve();
    assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);
    assert!(resolved.class("info_thing").unwrap().property("targetname").is_some());
}

#[test]
fn load_many_diamond_and_top_level_dirs() {
    let dir = scratch("includes_many");
    let base = write(&dir, "bin/base.fgd", b"@BaseClass = Targetname [ targetname(target_source) : \"Name\" ]\n@PointClass = info_target : \"Base version\" []\n");
    let game = write(
        &dir,
        "bin/game.fgd",
        b"@include \"base.fgd\"\n@PointClass base(Targetname) = info_target : \"Game version\" []\n",
    );
    let modfgd =
        write(&dir, "gamemodes/mymod/mod.fgd", b"@include \"base.fgd\"\n@PointClass base(Targetname) = mod_ent []\n");

    let fgd = Fgd::load_many([&game, &modfgd]).expect("load");
    assert!(fgd.warnings.is_empty(), "{:#?}", fgd.warnings);
    let resolved = fgd.resolve();
    assert_eq!(resolved.class("info_target").unwrap().description, "Game version");
    assert!(resolved.class("mod_ent").unwrap().property("targetname").is_some());

    let alone = Fgd::load(&modfgd).expect("load");
    assert_eq!(alone.warnings.len(), 1, "{:#?}", alone.warnings);
    assert!(alone.warnings[0].contains("not found"));
    let with_dir = Fgd::load_with_include_dirs([&modfgd], [dir.join("bin")]).expect("load");
    assert!(with_dir.warnings.is_empty(), "{:#?}", with_dir.warnings);
    assert!(with_dir.class("Targetname").is_some());

    let twice = Fgd::load_many([&game, &base]).expect("load");
    assert_eq!(twice.warnings.len(), 1);
    assert_eq!(twice.class("info_target").unwrap().description, "Game version");
}

#[test]
fn parse_with_base_dir_and_encodings() {
    let dir = scratch("includes_encoding");
    write(&dir, "latin.fgd", b"// \xA9 Valve\n@PointClass = latin_ent : \"Caf\xE9\" []\n");
    let mut utf16 = vec![0xFF, 0xFE];
    for unit in "@PointClass = wide_ent : \"Wide\" []\n".encode_utf16() {
        utf16.extend_from_slice(&unit.to_le_bytes());
    }
    write(&dir, "wide.fgd", &utf16);

    let fgd = Fgd::parse("@include \"latin.fgd\"\n@include \"wide.fgd\"\n", Some(&dir));
    assert!(fgd.warnings.is_empty(), "{:#?}", fgd.warnings);
    assert_eq!(fgd.class("latin_ent").unwrap().description, "Caf\u{e9}");
    assert_eq!(fgd.class("wide_ent").unwrap().description, "Wide");
}

#[test]
fn missing_top_level_file_is_an_error() {
    let dir = scratch("includes_missing");
    let err = Fgd::load(dir.join("nope.fgd")).unwrap_err();
    let FgdError::Io { path, .. } = &err;
    assert!(path.ends_with("nope.fgd"));
    assert!(err.to_string().contains("nope.fgd"));
}
