use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use ahash::{AHashMap, AHashSet};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use hp_fs::{GameFs, SourceKind};
use hp_render::Renderer;

use crate::AssetManager;
use crate::queue::Shared;

/// How often every watched file is looked at again while Hammer Pro is the window in use.
pub(crate) const POLL_INTERVAL: Duration = Duration::from_secs(3);
pub(crate) const SEND_INTERVAL: Duration = Duration::from_secs(2);
/// Files looked at in one go; a long list is checked a slice at a time so the disk never gets a burst.
const SLICE: usize = 256;
const SLICE_PAUSE: Duration = Duration::from_millis(15);
/// While Hammer Pro is minimised or in the background nothing is polled; this is how often the
/// watcher looks whether it is back in front.
const IDLE_WAIT: Duration = Duration::from_millis(500);
const MAX_FILES: usize = 50_000;

pub(crate) struct WatchList {
    pub epoch: u64,
    pub fs: GameFs,
    pub materials: Vec<String>,
    pub models: Vec<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Changes {
    pub epoch: u64,
    pub materials: Vec<String>,
    pub models: Vec<String>,
}

type Stamp = Option<(u64, SystemTime)>;

fn stamp(path: &std::path::Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Material(usize),
    Model(usize),
}

struct Watched {
    path: PathBuf,
    source: u32,
    owners: Vec<Owner>,
    stamp: Stamp,
}

struct Resolved {
    list: WatchList,
    files: Vec<Watched>,
}

fn material_files(fs: &GameFs, name: &str) -> Vec<String> {
    let mut out = vec![format!("materials/{name}.vmt")];
    if let Some(mat) = fs.material(name) {
        let mat = mat.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false);
        for tex in
            [mat.base_texture(), mat.base_texture2(), mat.bump_map(), mat.preview_texture()].into_iter().flatten()
        {
            let tex = hp_vmt::normalize_texture_path(&tex);
            if !tex.is_empty() {
                out.push(format!("materials/{tex}.vtf"));
            }
        }
    }
    out
}

fn model_files(path: &str) -> Vec<String> {
    let stem = path.strip_suffix(".mdl").unwrap_or(path);
    vec![path.to_owned(), format!("{stem}.vvd")]
}

fn resolve(list: WatchList) -> Resolved {
    let mut by_path: AHashMap<PathBuf, usize> = AHashMap::new();
    let mut files: Vec<Watched> = Vec::new();
    let mut add = |vpath: &str, owner: Owner, files: &mut Vec<Watched>| {
        if files.len() >= MAX_FILES {
            return;
        }
        let Some((source, path)) = list.fs.watch_path(vpath) else { return };
        match by_path.get(&path) {
            Some(&i) => {
                if !files[i].owners.contains(&owner) {
                    files[i].owners.push(owner);
                }
            }
            None => {
                by_path.insert(path.clone(), files.len());
                files.push(Watched { stamp: stamp(&path), path, source, owners: vec![owner] });
            }
        }
    };
    for (i, name) in list.materials.iter().enumerate() {
        for vpath in material_files(&list.fs, name) {
            add(&vpath, Owner::Material(i), &mut files);
        }
    }
    for (i, model) in list.models.iter().enumerate() {
        for vpath in model_files(model) {
            add(&vpath, Owner::Model(i), &mut files);
        }
    }
    Resolved { list, files }
}

fn check(resolved: &mut Resolved, range: std::ops::Range<usize>) -> Option<Changes> {
    let mut materials: AHashSet<usize> = AHashSet::new();
    let mut models: AHashSet<usize> = AHashSet::new();
    let mut custom_sources: Vec<u32> = Vec::new();
    let range = range.start.min(resolved.files.len())..range.end.min(resolved.files.len());
    for file in &mut resolved.files[range] {
        let now = stamp(&file.path);
        if now == file.stamp {
            continue;
        }
        file.stamp = now;
        for owner in &file.owners {
            match owner {
                Owner::Material(i) => materials.insert(*i),
                Owner::Model(i) => models.insert(*i),
            };
        }
        if !custom_sources.contains(&file.source) {
            custom_sources.push(file.source);
        }
    }
    if materials.is_empty() && models.is_empty() {
        return None;
    }
    let fs = &resolved.list.fs;
    custom_sources.retain(|id| fs.source(*id).is_some_and(|s| matches!(s.kind, SourceKind::Custom(_))));
    fs.refresh_sources(&custom_sources);
    let mut out = Changes { epoch: resolved.list.epoch, ..Changes::default() };
    out.materials = materials.into_iter().filter_map(|i| resolved.list.materials.get(i).cloned()).collect();
    out.models = models.into_iter().filter_map(|i| resolved.list.models.get(i).cloned()).collect();
    out.materials.sort_unstable();
    out.models.sort_unstable();
    Some(out)
}

fn run(lists: &Receiver<WatchList>, changes: &Sender<Changes>, shared: &Shared) {
    hp_app::background::background_current_thread();
    let mut current: Option<Resolved> = None;
    let mut cursor = 0usize;
    let mut next_sweep = Instant::now() + POLL_INTERVAL;
    let mut was_idle = false;
    loop {
        let wait = if cursor > 0 { SLICE_PAUSE } else { next_sweep.saturating_duration_since(Instant::now()).min(IDLE_WAIT) };
        match lists.recv_timeout(wait) {
            Ok(list) => {
                let list = lists.try_iter().last().unwrap_or(list);
                current = Some(resolve(list));
                cursor = 0;
                next_sweep = Instant::now() + POLL_INTERVAL;
                continue;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if !hp_app::background::app_active() {
            was_idle = true;
            cursor = 0;
            next_sweep = Instant::now() + IDLE_WAIT;
            continue;
        }
        if was_idle {
            was_idle = false;
            next_sweep = Instant::now();
        }
        if cursor == 0 && Instant::now() < next_sweep {
            continue;
        }
        let Some(resolved) = &mut current else {
            next_sweep = Instant::now() + POLL_INTERVAL;
            continue;
        };
        let end = (cursor + SLICE).min(resolved.files.len());
        let found = check(resolved, cursor..end);
        cursor = if end >= resolved.files.len() { 0 } else { end };
        if cursor == 0 {
            next_sweep = Instant::now() + POLL_INTERVAL;
        }
        if let Some(found) = found {
            if changes.send(found).is_err() {
                return;
            }
            shared.request_repaint();
        }
    }
}

pub(crate) struct Watcher {
    tx: Sender<WatchList>,
    rx: Receiver<Changes>,
    last: Option<(Instant, u64)>,
}

impl Watcher {
    pub(crate) fn start(shared: Arc<Shared>) -> Option<Self> {
        let (tx, lists) = crossbeam_channel::unbounded();
        let (changes, rx) = crossbeam_channel::unbounded();
        std::thread::Builder::new()
            .name("hp-assets-watch".to_owned())
            .spawn(move || run(&lists, &changes, &shared))
            .map_err(|e| log::warn!("cannot start the content watcher: {e}"))
            .ok()?;
        Some(Self { tx, rx, last: None })
    }
}

fn fingerprint<'a>(names: impl Iterator<Item = &'a str>) -> u64 {
    let state = ahash::RandomState::with_seeds(1, 2, 3, 4);
    names.fold(0u64, |acc, n| acc ^ state.hash_one(n)).wrapping_add(1)
}

impl AssetManager {
    pub fn set_live_reload(&mut self, on: bool) {
        self.watcher = if on { Watcher::start(Arc::clone(&self.shared)) } else { None };
    }

    pub(crate) fn update_watch(&mut self) {
        let Some(watcher) = &mut self.watcher else { return };
        if watcher.last.is_some_and(|(at, _)| at.elapsed() < SEND_INTERVAL) {
            return;
        }
        let Some(fs) = self.fs.clone() else { return };
        let ready = |m: &&String| matches!(self.models.map.get(*m), Some(crate::models::ModelEntry::Ready(_)));
        let models_print = fingerprint(self.models.map.keys().filter(ready).map(String::as_str));
        let print =
            fingerprint(self.mats.users.keys().map(String::as_str)) ^ models_print.rotate_left(17) ^ self.mount_gen;
        let changed = watcher.last.is_none_or(|(_, p)| p != print);
        watcher.last = Some((Instant::now(), print));
        if !changed {
            return;
        }
        let list = WatchList {
            epoch: self.mount_gen,
            fs,
            materials: self.mats.users.keys().cloned().collect(),
            models: self.models.map.keys().filter(ready).cloned().collect(),
        };
        let _ = watcher.tx.send(list);
    }

    pub(crate) fn poll_watcher(&mut self, renderer: &mut Renderer) {
        let Some(watcher) = &self.watcher else { return };
        let found: Vec<Changes> = watcher.rx.try_iter().collect();
        for changes in found {
            if changes.epoch != self.mount_gen {
                continue;
            }
            for name in &changes.materials {
                self.invalidate_material_impl(name);
            }
            for path in &changes.models {
                self.reload_model(renderer, path);
            }
            let mut names: Vec<&str> = changes.materials.iter().chain(&changes.models).map(String::as_str).collect();
            let total = names.len();
            names.truncate(8);
            let mut list = names.join(", ");
            if total > names.len() {
                list.push_str(", ");
                list.push_str(&hp_app::tf!("assets.more_sources", total - names.len()));
            }
            log::info!("reloaded from disk: {list}");
            self.messages.push((hp_app::Severity::Info, hp_app::tf!("assets.reloaded", list)));
            self.generation += 1;
        }
    }

    pub(crate) fn reload_model(&mut self, renderer: &mut Renderer, path: &str) {
        self.models.map.remove(path);
        self.shared.bounds.write().remove(path);
        let keys: Vec<(String, u32)> = self.models.render.keys().filter(|(p, _)| p == path).cloned().collect();
        for key in keys {
            if let Some(m) = self.models.render.remove(&key)
                && let Some(id) = m.id
            {
                renderer.destroy_model(id);
            }
        }
        self.scene.force_entities = true;
        if let Some(p) = &mut self.preview {
            p.needs_render = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-assets/scratch").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn rewrite(path: &std::path::Path, bytes: &[u8]) {
        let before = stamp(path);
        for _ in 0..50 {
            std::fs::write(path, bytes).unwrap();
            if stamp(path) != before {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn edited_materials_and_models_are_found() {
        let dir = scratch("watcher");
        let game = dir.join("game");
        std::fs::create_dir_all(game.join("materials/brick")).unwrap();
        std::fs::create_dir_all(game.join("models/props")).unwrap();
        std::fs::write(game.join("materials/brick/wall.vmt"), "LightmappedGeneric { $basetexture brick/wall }")
            .unwrap();
        std::fs::write(game.join("materials/brick/wall.vtf"), b"vtf1").unwrap();
        std::fs::write(game.join("materials/brick/other.vmt"), "LightmappedGeneric { $basetexture brick/x }").unwrap();
        std::fs::write(game.join("models/props/crate.mdl"), b"mdl1").unwrap();
        let fs = GameFs::empty();
        fs.add_game_dir(&game).unwrap();
        let list = WatchList {
            epoch: 7,
            fs,
            materials: vec!["brick/wall".to_owned(), "brick/other".to_owned(), "brick/missing".to_owned()],
            models: vec!["models/props/crate.mdl".to_owned()],
        };
        let mut resolved = resolve(list);
        assert_eq!(resolved.files.len(), 4, "two VMTs, one texture, one model (the missing ones are skipped)");
        assert_eq!(check(&mut resolved, 0..usize::MAX), None, "nothing changed");
        rewrite(&game.join("materials/brick/wall.vtf"), b"vtf2 longer");
        rewrite(&game.join("models/props/crate.mdl"), b"mdl2 longer");
        let changes = check(&mut resolved, 0..usize::MAX).expect("changes");
        assert_eq!(changes.epoch, 7);
        assert_eq!(changes.materials, vec!["brick/wall".to_owned()]);
        assert_eq!(changes.models, vec!["models/props/crate.mdl".to_owned()]);
        assert_eq!(check(&mut resolved, 0..usize::MAX), None, "reported once");
        std::fs::remove_file(game.join("materials/brick/other.vmt")).unwrap();
        assert_eq!(check(&mut resolved, 0..usize::MAX).expect("deleted").materials, vec!["brick/other".to_owned()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fingerprints_ignore_the_order() {
        assert_eq!(fingerprint(["a", "b"].into_iter()), fingerprint(["b", "a"].into_iter()));
        assert_ne!(fingerprint(["a"].into_iter()), fingerprint(["a", "b"].into_iter()));
        assert_eq!(model_files("models/a.mdl"), vec!["models/a.mdl".to_owned(), "models/a.vvd".to_owned()]);
    }
}
