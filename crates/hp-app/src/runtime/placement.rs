use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::SystemTime;

use hp_editor::prefab::Prefab;
use serde::{Deserialize, Serialize};

use crate::sidecar::{Sidecar, SidecarSection};

const CACHE_CAPACITY: usize = 24;
const CACHE_BYTES: u64 = 256 << 20;

#[derive(Clone, Debug)]
pub enum PrefabLoad {
    Loading,
    Ready(Arc<Prefab>),
    Failed(String),
}

pub type FileStamp = Option<(SystemTime, u64)>;

pub fn file_stamp(path: &Path) -> FileStamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

struct CacheEntry {
    stamp: FileStamp,
    load: PrefabLoad,
    last_used: u64,
}

type Done = (PathBuf, FileStamp, Result<Prefab, String>);

pub struct PrefabCache {
    entries: HashMap<PathBuf, CacheEntry>,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    clock: u64,
}

impl Default for PrefabCache {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { entries: HashMap::new(), tx, rx, clock: 0 }
    }
}

impl std::fmt::Debug for PrefabCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrefabCache").field("entries", &self.entries.len()).finish_non_exhaustive()
    }
}

impl PrefabCache {
    pub fn poll(&mut self) {
        while let Ok((path, stamp, result)) = self.rx.try_recv() {
            let Some(entry) = self.entries.get_mut(&path) else { continue };
            if entry.stamp != stamp {
                continue;
            }
            entry.load = match result {
                Ok(p) => PrefabLoad::Ready(Arc::new(p)),
                Err(e) => PrefabLoad::Failed(e),
            };
        }
    }

    pub fn get(&mut self, path: &Path, repaint: Option<&egui::Context>) -> PrefabLoad {
        self.poll();
        self.clock += 1;
        let clock = self.clock;
        if let Some(e) = self.entries.get_mut(path) {
            e.last_used = clock;
            return e.load.clone();
        }
        self.start(path.to_path_buf(), file_stamp(path), repaint);
        PrefabLoad::Loading
    }

    pub fn peek(&self, path: &Path) -> Option<Arc<Prefab>> {
        match self.entries.get(path).map(|e| &e.load) {
            Some(PrefabLoad::Ready(p)) => Some(Arc::clone(p)),
            _ => None,
        }
    }

    pub fn refresh(&mut self, path: &Path, repaint: Option<&egui::Context>) {
        let stamp = file_stamp(path);
        if self.entries.get(path).is_some_and(|e| e.stamp == stamp && !matches!(e.load, PrefabLoad::Failed(_))) {
            return;
        }
        self.start(path.to_path_buf(), stamp, repaint);
    }

    pub fn note_stamp(&mut self, path: &Path, stamp: FileStamp) {
        if self.entries.get(path).is_some_and(|e| e.stamp != stamp) {
            self.entries.remove(path);
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn start(&mut self, path: PathBuf, stamp: FileStamp, repaint: Option<&egui::Context>) {
        self.evict();
        self.clock += 1;
        self.entries.insert(path.clone(), CacheEntry { stamp, load: PrefabLoad::Loading, last_used: self.clock });
        let tx = self.tx.clone();
        let repaint = repaint.cloned();
        let worker_path = path.clone();
        let spawned = std::thread::Builder::new().name("hp-prefab-load".to_owned()).spawn(move || {
            let result = Prefab::load(&worker_path).map_err(|e| e.to_string());
            let _ = tx.send((worker_path, stamp, result));
            if let Some(ctx) = repaint {
                ctx.request_repaint();
            }
        });
        if let Err(e) = spawned
            && let Some(entry) = self.entries.get_mut(&path)
        {
            entry.load = PrefabLoad::Failed(e.to_string());
        }
    }

    fn bytes(&self) -> u64 {
        self.entries
            .values()
            .filter(|e| matches!(e.load, PrefabLoad::Ready(_)))
            .filter_map(|e| e.stamp.map(|(_, len)| len))
            .sum()
    }

    fn evict(&mut self) {
        while self.entries.len() >= CACHE_CAPACITY || self.bytes() > CACHE_BYTES {
            let oldest = self
                .entries
                .iter()
                .filter(|(_, e)| !matches!(e.load, PrefabLoad::Loading))
                .min_by_key(|(_, e)| e.last_used)
                .map(|(p, _)| p.clone());
            match oldest {
                Some(p) => {
                    self.entries.remove(&p);
                }
                None => break,
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatePrefabRequest {
    pub path: PathBuf,
    pub overwrite: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrefabTokens {
    pub next: u32,
}

impl SidecarSection for PrefabTokens {
    const NAME: &'static str = "placement";
}

impl PrefabTokens {
    pub fn take(sidecar: &mut Sidecar, doc: &hp_editor::Document, prefab: &Prefab) -> Option<u32> {
        if !prefab.has_token() {
            return None;
        }
        let tokens: PrefabTokens = sidecar.get();
        let n = doc.prefab_token(prefab, tokens.next.max(1));
        sidecar.set(&PrefabTokens { next: n.saturating_add(1) });
        Some(n)
    }
}

#[derive(Debug, Default)]
pub struct PlacementRuntime {
    pub selected_prefab: Option<PathBuf>,
    pub armed_prefab: Option<PathBuf>,
    pub prefabs: PrefabCache,
    pub create_open: bool,
    pub create_request: Option<CreatePrefabRequest>,
    pub library_serial: u64,
    pub open_path: Option<PathBuf>,
    pub sprinkle_refresh: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_loads_on_a_worker_and_reports_failures() {
        let dir = std::env::temp_dir().join(format!("hp_app_prefab_cache_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let good = dir.join("box.vmf");
        let bad = dir.join("empty.vmf");
        std::fs::write(&good, "entity { \"classname\" \"info_target\" \"origin\" \"1 2 3\" }").expect("write");
        std::fs::write(&bad, "versioninfo { }").expect("write");
        let mut cache = PrefabCache::default();
        assert!(matches!(cache.get(&good, None), PrefabLoad::Loading));
        let _ = cache.get(&bad, None);
        let _ = cache.get(&dir.join("missing.vmf"), None);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let a = cache.get(&good, None);
            let b = cache.get(&bad, None);
            if !matches!(a, PrefabLoad::Loading) && !matches!(b, PrefabLoad::Loading) {
                assert!(matches!(a, PrefabLoad::Ready(ref p) if p.entity_count() == 1), "{a:?}");
                assert!(matches!(b, PrefabLoad::Failed(_)), "{b:?}");
                break;
            }
            assert!(std::time::Instant::now() < deadline, "loads finish");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(cache.len(), 3);
        cache.note_stamp(&good, None);
        assert_eq!(cache.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tokens_advance_per_map() {
        let prefab =
            Prefab::parse("entity { \"classname\" \"info_target\" \"targetname\" \"t_&i\" \"origin\" \"0 0 0\" }")
                .expect("prefab");
        let mut sidecar = Sidecar::in_memory();
        let doc = hp_editor::Document::new_empty();
        assert_eq!(PrefabTokens::take(&mut sidecar, &doc, &prefab), Some(1));
        assert_eq!(PrefabTokens::take(&mut sidecar, &doc, &prefab), Some(2));
        let plain = Prefab::parse("entity { \"classname\" \"info_target\" \"origin\" \"0 0 0\" }").expect("plain");
        assert_eq!(PrefabTokens::take(&mut sidecar, &doc, &plain), None);
        assert_eq!(sidecar.get::<PrefabTokens>().next, 3);
    }
}
