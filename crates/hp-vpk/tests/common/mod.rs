#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct TestFile {
    pub path: String,
    pub data: Vec<u8>,
    pub preload: usize,
    pub archive: Option<u16>,
    pub crc_override: Option<u32>,
}

impl TestFile {
    pub fn new(path: &str, data: &[u8], preload: usize, archive: Option<u16>) -> Self {
        Self {
            path: path.to_owned(),
            data: data.to_vec(),
            preload: preload.min(data.len()),
            archive,
            crc_override: None,
        }
    }
}

pub fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-vpk/scratch").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn split(path: &str) -> (String, String, String) {
    let (dir, file) = match path.rfind('/') {
        Some(i) => (&path[..i], &path[i + 1..]),
        None => ("", path),
    };
    let (name, ext) = match file.rfind('.') {
        Some(i) => (&file[..i], &file[i + 1..]),
        None => (file, ""),
    };
    let blank = |s: &str| if s.is_empty() { " ".to_owned() } else { s.to_owned() };
    (blank(ext), blank(dir), name.to_owned())
}

type DirGroups<'a> = BTreeMap<String, Vec<(String, &'a TestFile)>>;

pub fn write_vpk(dir: &Path, base: &str, version: u32, files: &[TestFile]) -> PathBuf {
    let mut groups: BTreeMap<String, DirGroups<'_>> = BTreeMap::new();
    for f in files {
        let (ext, d, name) = split(&f.path);
        groups.entry(ext).or_default().entry(d).or_default().push((name, f));
    }
    let mut parts: BTreeMap<u16, Vec<u8>> = BTreeMap::new();
    let mut embedded = Vec::new();
    let mut tree = Vec::new();
    for (ext, dirs) in &groups {
        tree.extend_from_slice(ext.as_bytes());
        tree.push(0);
        for (d, names) in dirs {
            tree.extend_from_slice(d.as_bytes());
            tree.push(0);
            for (name, f) in names {
                tree.extend_from_slice(name.as_bytes());
                tree.push(0);
                let crc = f.crc_override.unwrap_or_else(|| crc32fast::hash(&f.data));
                let preload = &f.data[..f.preload];
                let rest = &f.data[f.preload..];
                let (index, sink) = match f.archive {
                    Some(i) => (i, parts.entry(i).or_default()),
                    None => (0x7FFF, &mut embedded),
                };
                let offset = sink.len() as u32;
                sink.extend_from_slice(rest);
                tree.extend_from_slice(&crc.to_le_bytes());
                tree.extend_from_slice(&(preload.len() as u16).to_le_bytes());
                tree.extend_from_slice(&index.to_le_bytes());
                tree.extend_from_slice(&offset.to_le_bytes());
                tree.extend_from_slice(&(rest.len() as u32).to_le_bytes());
                tree.extend_from_slice(&0xFFFFu16.to_le_bytes());
                tree.extend_from_slice(preload);
            }
            tree.push(0);
        }
        tree.push(0);
    }
    tree.push(0);

    let mut out = Vec::new();
    out.extend_from_slice(&0x55AA_1234u32.to_le_bytes());
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&(tree.len() as u32).to_le_bytes());
    if version == 2 {
        out.extend_from_slice(&(embedded.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&48u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
    }
    out.extend_from_slice(&tree);
    out.extend_from_slice(&embedded);
    if version == 2 {
        out.extend_from_slice(&[0u8; 48]);
    }
    let dir_path = dir.join(format!("{base}_dir.vpk"));
    fs::write(&dir_path, out).expect("write dir vpk");
    for (index, data) in parts {
        fs::write(dir.join(format!("{base}_{index:03}.vpk")), data).expect("write part");
    }
    dir_path
}

pub fn noise(len: usize, mut seed: u64) -> Vec<u8> {
    (0..len)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 24) as u8
        })
        .collect()
}
