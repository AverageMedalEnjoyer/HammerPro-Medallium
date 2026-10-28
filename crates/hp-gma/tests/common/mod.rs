#![allow(dead_code)]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct GmaSpec {
    pub version: u8,
    pub steam_id: u64,
    pub timestamp: u64,
    pub required: Vec<String>,
    pub name: String,
    pub description: String,
    pub author: String,
    pub addon_version: i32,
    pub files: Vec<(String, Vec<u8>, Option<u32>)>,
    pub trailing_crc: bool,
}

impl Default for GmaSpec {
    fn default() -> Self {
        Self {
            version: 3,
            steam_id: 76561197960287930,
            timestamp: 1_700_000_000,
            required: Vec::new(),
            name: "Test Addon".to_owned(),
            description: "{\n\t\"description\": \"A test addon\",\n\t\"type\": \"model\",\n\t\"tags\": [\n\t\t\"build\",\n\t\t\"scenic\"\n\t]\n}"
                .to_owned(),
            author: "Author Name".to_owned(),
            addon_version: 1,
            files: Vec::new(),
            trailing_crc: true,
        }
    }
}

impl GmaSpec {
    pub fn file(mut self, path: &str, data: &[u8]) -> Self {
        self.files.push((path.to_owned(), data.to_vec(), None));
        self
    }
}

pub fn build_gma(spec: &GmaSpec) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"GMAD");
    out.push(spec.version);
    out.extend_from_slice(&spec.steam_id.to_le_bytes());
    out.extend_from_slice(&spec.timestamp.to_le_bytes());
    if spec.version > 1 {
        for r in &spec.required {
            out.extend_from_slice(r.as_bytes());
            out.push(0);
        }
        out.push(0);
    }
    for s in [&spec.name, &spec.description, &spec.author] {
        out.extend_from_slice(s.as_bytes());
        out.push(0);
    }
    out.extend_from_slice(&spec.addon_version.to_le_bytes());
    for (i, (path, data, crc)) in spec.files.iter().enumerate() {
        out.extend_from_slice(&(i as u32 + 1).to_le_bytes());
        out.extend_from_slice(path.as_bytes());
        out.push(0);
        out.extend_from_slice(&(data.len() as i64).to_le_bytes());
        out.extend_from_slice(&crc.unwrap_or_else(|| crc32fast::hash(data)).to_le_bytes());
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    for (_, data, _) in &spec.files {
        out.extend_from_slice(data);
    }
    if spec.trailing_crc {
        let crc = crc32fast::hash(&out);
        out.extend_from_slice(&crc.to_le_bytes());
    }
    out
}

pub fn compress_legacy(data: &[u8], known_size: bool) -> Vec<u8> {
    let options = lzma_rust2::LzmaOptions::with_preset(6);
    let size = known_size.then_some(data.len() as u64);
    let mut writer = lzma_rust2::LzmaWriter::new_use_header(Vec::new(), &options, size).unwrap();
    writer.write_all(data).unwrap();
    writer.finish().unwrap()
}

pub fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-gma/scratch").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
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
