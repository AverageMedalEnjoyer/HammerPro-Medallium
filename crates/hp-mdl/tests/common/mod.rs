#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hp_mdl::{LoadOptions, Model, load, read_header, sibling_paths};

struct Entry {
    source: PathBuf,
    offset: u64,
    len: u64,
    preload: Vec<u8>,
}

#[derive(Default)]
pub struct Archive {
    entries: HashMap<String, Entry>,
}

impl Archive {
    pub fn files_with_ext(&self, ext: &str) -> Vec<String> {
        let mut out: Vec<String> = self.entries.keys().filter(|p| p.ends_with(ext)).cloned().collect();
        out.sort();
        out
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        let entry = self.entries.get(&path.to_ascii_lowercase().replace('\\', "/"))?;
        let mut data = entry.preload.clone();
        if entry.len > 0 {
            let mut f = File::open(&entry.source).ok()?;
            f.seek(SeekFrom::Start(entry.offset)).ok()?;
            let start = data.len();
            data.resize(start + usize::try_from(entry.len).ok()?, 0);
            f.read_exact(&mut data[start..]).ok()?;
        }
        Some(data)
    }
}

fn cstr(data: &[u8], pos: &mut usize) -> Option<String> {
    let rest = data.get(*pos..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    let s = String::from_utf8_lossy(&rest[..end]).into_owned();
    *pos += end + 1;
    Some(s)
}

fn le<const N: usize>(data: &[u8], pos: &mut usize) -> Option<[u8; N]> {
    let bytes: [u8; N] = data.get(*pos..*pos + N)?.try_into().ok()?;
    *pos += N;
    Some(bytes)
}

pub fn open_vpk(dir_path: &Path) -> Option<Archive> {
    let data = std::fs::read(dir_path).ok()?;
    let mut pos = 0;
    let sig = u32::from_le_bytes(le(&data, &mut pos)?);
    if sig != 0x55aa_1234 {
        return None;
    }
    let version = u32::from_le_bytes(le(&data, &mut pos)?);
    let tree_size = u32::from_le_bytes(le(&data, &mut pos)?) as usize;
    let header_size = match version {
        1 => 12,
        2 => 28,
        _ => return None,
    };
    pos = header_size;
    let stem = dir_path.file_name()?.to_str()?.strip_suffix("_dir.vpk")?.to_string();
    let folder = dir_path.parent()?.to_path_buf();
    let mut archive = Archive::default();
    loop {
        let ext = cstr(&data, &mut pos)?;
        if ext.is_empty() {
            break;
        }
        loop {
            let dir = cstr(&data, &mut pos)?;
            if dir.is_empty() {
                break;
            }
            loop {
                let name = cstr(&data, &mut pos)?;
                if name.is_empty() {
                    break;
                }
                let _crc = le::<4>(&data, &mut pos)?;
                let preload_len = u16::from_le_bytes(le(&data, &mut pos)?) as usize;
                let index = u16::from_le_bytes(le(&data, &mut pos)?);
                let offset = u32::from_le_bytes(le(&data, &mut pos)?) as u64;
                let len = u32::from_le_bytes(le(&data, &mut pos)?) as u64;
                let _term = le::<2>(&data, &mut pos)?;
                let preload = data.get(pos..pos + preload_len)?.to_vec();
                pos += preload_len;
                let (source, offset) = if index == 0x7fff {
                    (dir_path.to_path_buf(), (header_size + tree_size) as u64 + offset)
                } else {
                    (folder.join(format!("{stem}_{index:03}.vpk")), offset)
                };
                let path = if dir == " " { format!("{name}.{ext}") } else { format!("{dir}/{name}.{ext}") };
                archive.entries.insert(path.to_ascii_lowercase(), Entry { source, offset, len, preload });
            }
        }
    }
    Some(archive)
}

fn parse_gma_index(data: &[u8]) -> Option<(Vec<(String, u64)>, u64)> {
    let mut pos = 0;
    if data.get(0..4)? != b"GMAD" {
        return None;
    }
    pos += 4;
    let version = le::<1>(data, &mut pos)?[0];
    pos += 16;
    if version > 1 {
        while !cstr(data, &mut pos)?.is_empty() {}
    }
    for _ in 0..3 {
        cstr(data, &mut pos)?;
    }
    pos += 4;
    let mut files = Vec::new();
    loop {
        let num = u32::from_le_bytes(le(data, &mut pos)?);
        if num == 0 {
            break;
        }
        let name = cstr(data, &mut pos)?;
        let size = i64::from_le_bytes(le(data, &mut pos)?);
        let _crc = le::<4>(data, &mut pos)?;
        files.push((name, u64::try_from(size).ok()?));
    }
    Some((files, pos as u64))
}

pub fn open_gma(path: &Path) -> Option<Archive> {
    let file_len = std::fs::metadata(path).ok()?.len();
    for limit in [4u64 << 20, 64 << 20] {
        let mut head = Vec::new();
        File::open(path).ok()?.take(limit).read_to_end(&mut head).ok()?;
        let Some((files, data_start)) = parse_gma_index(&head) else {
            if file_len <= limit {
                return None;
            }
            continue;
        };
        let mut offset = data_start;
        let mut archive = Archive::default();
        for (name, len) in files {
            let key = name.to_ascii_lowercase().replace('\\', "/");
            archive.entries.insert(key, Entry { source: path.to_path_buf(), offset, len, preload: Vec::new() });
            offset += len;
        }
        return Some(archive);
    }
    None
}

pub fn check_model(model: &Model) -> Result<(), String> {
    for (i, mesh) in model.meshes.iter().enumerate() {
        if mesh.indices.len() % 3 != 0 {
            return Err(format!("mesh {i}: index count not a multiple of 3"));
        }
        if let Some(&bad) = mesh.indices.iter().find(|&&ix| ix as usize >= mesh.vertices.len()) {
            return Err(format!("mesh {i}: index {bad} out of range ({} vertices)", mesh.vertices.len()));
        }
        if mesh.vertices.iter().any(|v| v.pos.iter().chain(&v.normal).chain(&v.uv).any(|c| !c.is_finite())) {
            return Err(format!("mesh {i}: non finite vertex data"));
        }
        if !model.materials.is_empty() && mesh.material_index >= model.materials.len() {
            return Err(format!("mesh {i}: material index {} out of range", mesh.material_index));
        }
    }
    let (min, max) = model.bounds;
    if !model.meshes.is_empty() && (0..3).any(|k| min[k] > max[k]) {
        return Err("inverted bounds".into());
    }
    Ok(())
}

pub struct ModelFiles {
    pub mdl: Vec<u8>,
    pub vvd: Vec<u8>,
    pub vtx: Vec<u8>,
}

pub fn read_model(path: &str, read: impl Fn(&str) -> Option<Vec<u8>>) -> Result<ModelFiles, String> {
    let (vvd_path, vtx_paths) = sibling_paths(path);
    let mdl = read(path).ok_or("mdl unreadable")?;
    let vvd = read(&vvd_path).ok_or("no vvd file")?;
    let vtx = vtx_paths.iter().find_map(|p| read(p)).ok_or("no vtx file")?;
    Ok(ModelFiles { mdl, vvd, vtx })
}

#[derive(Default)]
pub struct Report {
    pub per_version: BTreeMap<i32, (usize, usize)>,
    pub failures: Vec<String>,
    pub incomplete: Vec<String>,
    pub triangles: usize,
    pub vertices: usize,
    pub load_time: Duration,
    pub slowest: Vec<(Duration, usize, String)>,
}

impl Report {
    pub fn add(&mut self, name: &str, files: Result<ModelFiles, String>) {
        let files = match files {
            Ok(f) => f,
            Err(e) => {
                self.incomplete.push(format!("{name}: {e}"));
                return;
            }
        };
        let version = read_header(&files.mdl).map_or(-1, |h| h.version);
        let entry = self.per_version.entry(version).or_default();
        entry.0 += 1;
        let t = Instant::now();
        let result = load(&files.mdl, &files.vvd, &files.vtx, &LoadOptions::default());
        let elapsed = t.elapsed();
        self.load_time += elapsed;
        match result.map_err(|e| e.to_string()).and_then(|m| check_model(&m).map(|()| m)) {
            Ok(model) => {
                entry.1 += 1;
                self.triangles += model.triangle_count();
                self.vertices += model.vertex_count();
                self.slowest.push((elapsed, model.triangle_count(), name.to_string()));
            }
            Err(e) => self.failures.push(format!("{name}: {e}")),
        }
    }

    pub fn summary(&mut self, title: &str) -> String {
        self.slowest.sort_by_key(|s| std::cmp::Reverse(s.0));
        let loaded: usize = self.per_version.values().map(|v| v.1).sum();
        let seen: usize = self.per_version.values().map(|v| v.0).sum();
        let mut s = String::new();
        let _ = writeln!(s, "== {title}: {loaded}/{seen} loaded, {} incomplete sets", self.incomplete.len());
        for (version, (seen, ok)) in &self.per_version {
            let _ = writeln!(s, "  version {version}: {ok}/{seen}");
        }
        let _ = writeln!(
            s,
            "  triangles {}, vertices {}, load total {:?} (avg {:?})",
            self.triangles,
            self.vertices,
            self.load_time,
            self.load_time / loaded.max(1) as u32
        );
        for (d, tris, name) in self.slowest.iter().take(3) {
            let _ = writeln!(s, "  slowest: {d:?} {tris} tris {name}");
        }
        for f in self.failures.iter().take(40) {
            let _ = writeln!(s, "  FAIL {f}");
        }
        for f in self.incomplete.iter().take(10) {
            let _ = writeln!(s, "  incomplete {f}");
        }
        s
    }
}
