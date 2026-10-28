use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

use rayon::prelude::*;

use crate::edit::EditOutcome;
use crate::{Material, VmtError, parse, parse_bytes, resolve_patch, write};

const GMOD: &str = r"D:\SteamLibrary\steamapps\common\GarrysMod";

#[derive(Clone, Copy)]
enum RootKind {
    Materials,
    Addons,
    Nested,
}

fn collect_vmts(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_vmts(&path, out);
        } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("vmt")) {
            out.push(path);
        }
    }
}

fn material_name(root: &Path, kind: RootKind, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let rel = rel.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    let rel = match kind {
        RootKind::Materials => rel.as_str(),
        RootKind::Addons => rel.split_once("/materials/").map_or(rel.as_str(), |(_, r)| r),
        RootKind::Nested => rel.split_once('/').map_or(rel.as_str(), |(_, r)| r),
    };
    crate::normalize_material_name(rel)
}

#[derive(Default)]
struct FileReport {
    error: Option<String>,
    shader: String,
    tool: Option<&'static str>,
    flags: Vec<&'static str>,
    has_preview: bool,
    is_patch: bool,
    empty: bool,
    edit_fallbacks: usize,
    edits: usize,
}

fn check_file(name: &str, bytes: &[u8]) -> FileReport {
    let mut r = FileReport::default();
    let m = match parse_bytes(bytes) {
        Ok(m) => m,
        Err(VmtError::NoShader) if !bytes.contains(&b'{') => {
            r.empty = true;
            return r;
        }
        Err(e) => {
            r.error = Some(format!("parse: {e}"));
            return r;
        }
    };
    r.shader = m.shader.to_ascii_lowercase();
    r.tool = m.tool_kind(name).map(|k| k.as_str());
    r.flags = m.compile_flags().names().collect();
    r.has_preview = m.preview_texture().is_some();
    r.is_patch = m.is_patch();

    match parse(&write(&m)) {
        Ok(back) if back == m => {}
        Ok(_) => r.error = Some("write/parse round trip mismatch".to_owned()),
        Err(e) => r.error = Some(format!("re-parse of written text: {e}")),
    }

    let text = String::from_utf8_lossy(bytes);
    let text = crate::material::decode_utf16(bytes).map_or(text, std::borrow::Cow::Owned);
    let mut probe = |result: Result<EditOutcome, VmtError>, key: &str, want: Option<&str>| {
        r.edits += 1;
        match result {
            Ok(outcome) => {
                if outcome.fallback {
                    r.edit_fallbacks += 1;
                }
                let Ok(mut edited) = parse(&outcome.text) else {
                    r.error = Some(format!("edited text does not parse ({key})"));
                    return;
                };
                if edited.is_patch() {
                    let fake_base = Material::new("X");
                    let mut merged = fake_base;
                    for b in &edited.other_blocks {
                        if b.name.eq_ignore_ascii_case("insert") || b.name.eq_ignore_ascii_case("replace") {
                            for (k, v) in &b.props {
                                merged.set(k, v);
                            }
                        }
                    }
                    edited = merged;
                }
                if edited.get(key) != want {
                    r.error = Some(format!("edit of {key} gave {:?}, want {want:?}", edited.get(key)));
                }
            }
            Err(e) => r.error = Some(format!("edit {key}: {e}")),
        }
    };
    probe(crate::set_param_in_text_detailed(&text, "$hp_probe", "1"), "$hp_probe", Some("1"));
    if m.has("$basetexture") {
        probe(
            crate::set_param_in_text_detailed(&text, "$basetexture", "hp/probe value"),
            "$basetexture",
            Some("hp/probe value"),
        );
    }
    if m.has("%keywords") {
        probe(crate::remove_param_in_text_detailed(&text, "%keywords"), "%keywords", None);
    }
    if m.has("$surfaceprop") {
        probe(crate::remove_param_in_text_detailed(&text, "$surfaceprop"), "$surfaceprop", None);
    }
    r
}

#[test]
fn parse_all_loose_game_materials() {
    let gmod = Path::new(GMOD);
    let mut roots = vec![
        (gmod.join(r"garrysmod\materials"), RootKind::Materials),
        (gmod.join(r"garrysmod\addons"), RootKind::Addons),
        (gmod.join(r"bin\win64\hammerplusplus\materials"), RootKind::Materials),
    ];
    if let Ok(extra) = std::env::var("HP_VMT_EXTRA_ROOTS") {
        roots.extend(
            extra.split(';').filter(|s| !s.trim().is_empty()).map(|s| (PathBuf::from(s.trim()), RootKind::Nested)),
        );
    }
    if !roots.iter().any(|(p, _)| p.is_dir()) {
        eprintln!("hp-vmt: Garry's Mod not found at {GMOD}, skipping real data test");
        return;
    }

    let t0 = Instant::now();
    let mut files: Vec<(String, PathBuf, Vec<u8>)> = Vec::new();
    for (root, kind) in &roots {
        let mut paths = Vec::new();
        collect_vmts(root, &mut paths);
        eprintln!("hp-vmt: {} vmt files under {}", paths.len(), root.display());
        for path in paths {
            match std::fs::read(&path) {
                Ok(bytes) => files.push((material_name(root, *kind, &path), path, bytes)),
                Err(e) => eprintln!("hp-vmt: cannot read {}: {e}", path.display()),
            }
        }
    }
    let read_time = t0.elapsed();
    let total_bytes: usize = files.iter().map(|(_, _, b)| b.len()).sum();

    let t1 = Instant::now();
    let parsed_ok = files.iter().filter(|(_, _, b)| parse_bytes(b).is_ok()).count();
    let parse_time = t1.elapsed();

    let t2 = Instant::now();
    let reports: Vec<FileReport> = files.par_iter().map(|(name, _, bytes)| check_file(name, bytes)).collect();
    let check_time = t2.elapsed();

    let mut failures = Vec::new();
    let mut shaders: BTreeMap<&str, usize> = BTreeMap::new();
    let mut tools: BTreeMap<&str, usize> = BTreeMap::new();
    let mut flags: BTreeMap<&str, usize> = BTreeMap::new();
    let (mut patches, mut no_preview, mut edits, mut fallbacks, mut empty) = (0, 0, 0, 0, 0);
    for ((name, path, _), r) in files.iter().zip(&reports) {
        if let Some(e) = &r.error {
            failures.push(format!("{} ({name}): {e}", path.display()));
            continue;
        }
        if r.empty {
            empty += 1;
            continue;
        }
        *shaders.entry(r.shader.as_str()).or_default() += 1;
        if let Some(t) = r.tool {
            *tools.entry(t).or_default() += 1;
        }
        for f in &r.flags {
            *flags.entry(f).or_default() += 1;
        }
        patches += usize::from(r.is_patch);
        no_preview += usize::from(!r.has_preview && !r.is_patch);
        edits += r.edits;
        fallbacks += r.edit_fallbacks;
    }

    let index: HashMap<&str, &[u8]> = files.iter().rev().map(|(n, _, b)| (n.as_str(), b.as_slice())).collect();
    let loose_root = gmod.join("garrysmod");
    let load = |p: &str| {
        let name = p.strip_prefix("materials/").unwrap_or(p);
        let name = name.strip_suffix(".vmt").unwrap_or(name);
        match index.get(name) {
            Some(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
            None => std::fs::read(loose_root.join(p)).ok().map(|b| String::from_utf8_lossy(&b).into_owned()),
        }
    };
    let (mut resolved, mut not_found, mut other_errors, mut resolved_no_preview) = (0, 0, 0, 0);
    for (_, path, bytes) in &files {
        let Ok(m) = parse_bytes(bytes) else { continue };
        if !m.is_patch() {
            continue;
        }
        match resolve_patch(&m, &load) {
            Ok(r) => {
                assert!(!r.is_patch(), "{} resolved to a patch", path.display());
                resolved += 1;
                resolved_no_preview += usize::from(r.preview_texture().is_none());
            }
            Err(VmtError::IncludeNotFound(_)) => not_found += 1,
            Err(e) => {
                other_errors += 1;
                eprintln!("hp-vmt: patch {}: {e}", path.display());
            }
        }
    }

    let mut shader_list: Vec<_> = shaders.into_iter().collect();
    shader_list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    eprintln!(
        "hp-vmt: {} files, {} KiB, read {:?}, parse (1 thread) {:?} ({} ok), full checks (rayon) {:?}",
        files.len(),
        total_bytes / 1024,
        read_time,
        parse_time,
        parsed_ok,
        check_time
    );
    eprintln!("hp-vmt: shaders: {shader_list:?}");
    eprintln!("hp-vmt: tool kinds: {tools:?}");
    eprintln!("hp-vmt: compile flags: {flags:?}");
    eprintln!(
        "hp-vmt: patches {patches} (resolved {resolved}, include not found {not_found}, other errors {other_errors}, resolved without preview {resolved_no_preview})"
    );
    eprintln!("hp-vmt: non patch materials without preview texture {no_preview}, empty files (no block) {empty}");
    eprintln!("hp-vmt: text edits {edits}, parse+write fallbacks {fallbacks}");
    for f in failures.iter().take(50) {
        eprintln!("hp-vmt: FAIL {f}");
    }
    assert!(failures.is_empty(), "{} material files failed", failures.len());
}
