use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use hp_editor::instances::{
    InstanceParm, LoadRequest, Loaded, candidate_paths, content_path, is_hidden_object, map_parms, normalize_file,
    own_bounds, path_key,
};
use hp_editor::{Document, SolidNode};
use hp_geom::Aabb;
use hp_vmf::Map;

pub const MAX_FILE_BYTES: u64 = 256 << 20;
pub const MAX_FILES: usize = 512;
pub const MAX_TOTAL_BYTES: u64 = 512 << 20;
const MAX_LOOKUPS: usize = 8192;
const MAX_RECHECKS: usize = 256;
pub const POLL_INTERVAL: Duration = Duration::from_millis(1500);
pub const CONTENT_ROOT: &str = "<game content>";

#[derive(Clone, Debug)]
pub struct InstanceSolid {
    pub node: SolidNode,
    pub owner_class: Option<String>,
    pub owner_color: [u8; 3],
}

#[derive(Debug)]
pub struct InstanceFile {
    pub path: Arc<Path>,
    pub map: Arc<Map>,
    pub parms: Vec<InstanceParm>,
    pub bounds: Aabb,
    pub solids: Vec<InstanceSolid>,
    pub revision: u64,
    pub bytes: u64,
}

impl InstanceFile {
    pub fn is_content(&self) -> bool {
        self.path.starts_with(CONTENT_ROOT)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

impl Stamp {
    fn of(md: &std::fs::Metadata) -> Stamp {
        Stamp { modified: md.modified().ok(), len: md.len() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Origin {
    Disk(PathBuf),
    Content(String),
}

#[derive(Clone, Debug)]
enum FileState {
    Loading,
    Ready(Arc<InstanceFile>),
    Failed(String),
}

#[derive(Debug)]
struct FileSlot {
    state: FileState,
    origin: Origin,
    stamp: Option<Stamp>,
    last_used: u64,
    reloading: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct LookupKey {
    file: String,
    containing: String,
    main: String,
}

#[derive(Clone, Debug)]
struct LookupArgs {
    file: String,
    containing: Option<PathBuf>,
    main: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LookupState {
    Pending,
    Found(String),
    Missing,
}

#[derive(Debug)]
struct LookupSlot {
    state: LookupState,
    args: LookupArgs,
    last_used: u64,
}

#[derive(Debug)]
enum Job {
    Resolve(LookupKey),
    Reload(String),
    Poll,
}

#[derive(Default)]
struct State {
    files: HashMap<String, FileSlot>,
    lookups: HashMap<LookupKey, LookupSlot>,
    jobs: VecDeque<Job>,
    busy: bool,
    worker: bool,
    clock: u64,
    content: Option<hp_fs::GameFs>,
    content_id: Option<(u64, Option<PathBuf>)>,
    game_dir: Option<PathBuf>,
    last_poll: Option<Instant>,
    poll_queued: bool,
    next_revision: u64,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("files", &self.files.len())
            .field("lookups", &self.lookups.len())
            .field("jobs", &self.jobs.len())
            .finish_non_exhaustive()
    }
}

struct Inner {
    state: Mutex<State>,
    signal: Condvar,
    generation: AtomicU64,
    repaint: Mutex<Option<egui::Context>>,
}

#[derive(Clone)]
pub struct InstanceCache {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for InstanceCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstanceCache").field("generation", &self.generation()).finish_non_exhaustive()
    }
}

impl Default for InstanceCache {
    fn default() -> Self {
        Self::shared()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn absolute(p: &Path) -> PathBuf {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn content_key(vpath: &str) -> String {
    format!("{CONTENT_ROOT}/{vpath}")
}

impl InstanceCache {
    pub fn shared() -> InstanceCache {
        static SHARED: OnceLock<InstanceCache> = OnceLock::new();
        SHARED.get_or_init(InstanceCache::new).clone()
    }

    pub fn new() -> InstanceCache {
        InstanceCache {
            inner: Arc::new(Inner {
                state: Mutex::new(State::default()),
                signal: Condvar::new(),
                generation: AtomicU64::new(1),
                repaint: Mutex::new(None),
            }),
        }
    }

    pub fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::Acquire)
    }

    fn bump(&self) {
        self.inner.generation.fetch_add(1, Ordering::AcqRel);
        if let Some(ctx) = lock(&self.inner.repaint).as_ref() {
            ctx.request_repaint();
        }
    }

    pub fn set_repaint(&self, ctx: &egui::Context) {
        let mut r = lock(&self.inner.repaint);
        if r.is_none() {
            *r = Some(ctx.clone());
        }
    }

    pub fn set_content(&self, fs: Option<&hp_fs::GameFs>) {
        let id = fs.map(|f| (f.generation(), f.game_dir().map(Path::to_path_buf)));
        let mut st = lock(&self.inner.state);
        if st.content_id == id && st.content.is_some() == fs.is_some() {
            return;
        }
        st.content_id = id;
        st.content = fs.cloned();
        st.game_dir = fs.and_then(|f| f.game_dir().map(Path::to_path_buf));
        st.files.retain(|_, s| !matches!(s.origin, Origin::Content(_)));
        st.lookups.retain(|_, l| l.state != LookupState::Missing);
        drop(st);
        self.bump();
    }

    pub fn load(&self, req: &LoadRequest<'_>, main: Option<&Path>) -> Loaded {
        let Some(file) = normalize_file(req.file) else { return Loaded::Missing };
        let key = LookupKey {
            file: file.to_ascii_lowercase(),
            containing: req.containing.map(path_key).unwrap_or_default(),
            main: main.map(path_key).unwrap_or_default(),
        };
        let mut st = lock(&self.inner.state);
        st.clock += 1;
        let clock = st.clock;
        let found = match st.lookups.get_mut(&key) {
            Some(l) => {
                l.last_used = clock;
                l.state.clone()
            }
            None => {
                if st.lookups.len() >= MAX_LOOKUPS {
                    forget_oldest_lookups(&mut st);
                }
                let args = LookupArgs {
                    file,
                    containing: req.containing.map(Path::to_path_buf),
                    main: main.map(Path::to_path_buf),
                };
                st.lookups.insert(key.clone(), LookupSlot { state: LookupState::Pending, args, last_used: clock });
                self.queue(&mut st, Job::Resolve(key));
                return Loaded::Pending;
            }
        };
        match found {
            LookupState::Pending => Loaded::Pending,
            LookupState::Missing => Loaded::Missing,
            LookupState::Found(fkey) => match st.files.get_mut(&fkey) {
                Some(slot) => {
                    slot.last_used = clock;
                    match &slot.state {
                        FileState::Ready(f) => Loaded::Ready { path: Arc::clone(&f.path), map: Arc::clone(&f.map) },
                        FileState::Loading => Loaded::Pending,
                        FileState::Failed(m) => Loaded::Failed(m.clone()),
                    }
                }
                None => {
                    if let Some(l) = st.lookups.get_mut(&key) {
                        l.state = LookupState::Pending;
                    }
                    self.queue(&mut st, Job::Resolve(key));
                    Loaded::Pending
                }
            },
        }
    }

    pub fn source<'a>(&'a self, main: Option<&'a Path>) -> impl FnMut(&LoadRequest<'_>) -> Loaded + 'a {
        move |req: &LoadRequest<'_>| self.load(req, main)
    }

    pub fn file(&self, path: &Path) -> Option<Arc<InstanceFile>> {
        let key = path_key(path);
        let mut st = lock(&self.inner.state);
        st.clock += 1;
        let clock = st.clock;
        let slot = st.files.get_mut(&key)?;
        slot.last_used = clock;
        match &slot.state {
            FileState::Ready(f) => Some(Arc::clone(f)),
            _ => None,
        }
    }

    pub fn poll(&self) {
        let mut st = lock(&self.inner.state);
        if st.poll_queued || (st.files.is_empty() && st.lookups.is_empty()) {
            return;
        }
        let due = st.last_poll.is_none_or(|t| t.elapsed() >= POLL_INTERVAL);
        if due {
            st.poll_queued = true;
            self.queue(&mut st, Job::Poll);
        }
    }

    pub fn clear(&self) {
        let mut st = lock(&self.inner.state);
        st.files.clear();
        st.lookups.clear();
        st.jobs.retain(|j| matches!(j, Job::Poll));
        drop(st);
        self.bump();
    }

    pub fn invalidate(&self, path: &Path) {
        let key = path_key(&absolute(path));
        let mut st = lock(&self.inner.state);
        if st.files.contains_key(&key) {
            self.queue(&mut st, Job::Reload(key));
        }
        st.lookups.retain(|_, l| l.state != LookupState::Missing);
        drop(st);
        self.bump();
    }

    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut st = lock(&self.inner.state);
        loop {
            if st.jobs.is_empty() && !st.busy {
                return true;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            st = self.inner.signal.wait_timeout(st, left).unwrap_or_else(PoisonError::into_inner).0;
        }
    }

    pub fn file_count(&self) -> usize {
        lock(&self.inner.state).files.len()
    }

    fn queue(&self, st: &mut State, job: Job) {
        st.jobs.push_back(job);
        if !st.worker {
            st.worker = true;
            let me = self.clone();
            let spawned = std::thread::Builder::new().name("hp-instances".to_owned()).spawn(move || me.worker());
            if let Err(e) = spawned {
                log::error!("instance cache worker: {e}");
                st.worker = false;
                st.jobs.clear();
                for l in st.lookups.values_mut() {
                    if l.state == LookupState::Pending {
                        l.state = LookupState::Missing;
                    }
                }
            }
        }
        self.inner.signal.notify_all();
    }

    fn worker(self) {
        loop {
            let job = {
                let mut st = lock(&self.inner.state);
                loop {
                    if let Some(job) = st.jobs.pop_front() {
                        st.busy = true;
                        break job;
                    }
                    st.busy = false;
                    self.inner.signal.notify_all();
                    if Arc::strong_count(&self.inner) == 1 {
                        st.worker = false;
                        return;
                    }
                    st = self
                        .inner
                        .signal
                        .wait_timeout(st, Duration::from_secs(5))
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;
                }
            };
            let run = std::panic::AssertUnwindSafe(|| self.run(job));
            if std::panic::catch_unwind(run).is_err() {
                log::error!("instance cache: a job panicked");
            }
        }
    }

    fn run(&self, job: Job) {
        match job {
            Job::Resolve(key) => self.resolve(&key),
            Job::Reload(fkey) => {
                let origin = lock(&self.inner.state).files.get(&fkey).map(|s| s.origin.clone());
                if let Some(origin) = origin {
                    self.load_file(&fkey, &origin, true);
                }
            }
            Job::Poll => self.poll_disk(),
        }
    }

    fn resolve(&self, key: &LookupKey) {
        let (args, game_dir, content) = {
            let st = lock(&self.inner.state);
            let Some(l) = st.lookups.get(key) else { return };
            (l.args.clone(), st.game_dir.clone(), st.content.clone())
        };
        let found = find_file(&args, game_dir.as_deref(), content.as_ref());
        let state = match &found {
            Some((fkey, _)) => LookupState::Found(fkey.clone()),
            None => LookupState::Missing,
        };
        if let Some((fkey, origin)) = &found {
            let needs_load = {
                let mut st = lock(&self.inner.state);
                match st.files.get(fkey) {
                    Some(_) => false,
                    None => {
                        st.files.insert(
                            fkey.clone(),
                            FileSlot {
                                state: FileState::Loading,
                                origin: origin.clone(),
                                stamp: None,
                                last_used: 0,
                                reloading: false,
                            },
                        );
                        true
                    }
                }
            };
            if needs_load {
                self.load_file(fkey, origin, false);
            }
        }
        let mut st = lock(&self.inner.state);
        if let Some(l) = st.lookups.get_mut(key) {
            l.state = state;
        }
        drop(st);
        self.bump();
    }

    fn load_file(&self, fkey: &str, origin: &Origin, reload: bool) {
        let revision = {
            let mut st = lock(&self.inner.state);
            st.next_revision += 1;
            if let Some(s) = st.files.get_mut(fkey) {
                s.reloading = reload;
            }
            st.next_revision
        };
        let content = lock(&self.inner.state).content.clone();
        let stamp = match origin {
            Origin::Disk(p) => std::fs::metadata(p).ok().map(|md| Stamp::of(&md)),
            Origin::Content(_) => None,
        };
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_file(origin, content.as_ref(), revision)))
                .unwrap_or_else(|_| Err("internal error while reading the file".to_owned()));
        let mut st = lock(&self.inner.state);
        let clock = st.clock;
        match st.files.get_mut(fkey) {
            Some(slot) => {
                slot.reloading = false;
                slot.stamp = stamp;
                match result {
                    Ok(f) => slot.state = FileState::Ready(Arc::new(f)),
                    Err(e) if reload && matches!(slot.state, FileState::Ready(_)) => {
                        log::debug!("instance reload failed: {e}");
                    }
                    Err(e) => slot.state = FileState::Failed(e),
                }
                slot.last_used = slot.last_used.max(clock);
            }
            None => return,
        }
        evict(&mut st);
        drop(st);
        self.bump();
    }

    fn poll_disk(&self) {
        let (files, missing, game_dir, content) = {
            let mut st = lock(&self.inner.state);
            st.poll_queued = false;
            st.last_poll = Some(Instant::now());
            let files: Vec<(String, PathBuf, Option<Stamp>)> = st
                .files
                .iter()
                .filter(|(_, s)| !s.reloading)
                .filter_map(|(k, s)| match (&s.origin, &s.state) {
                    (Origin::Disk(p), FileState::Ready(_) | FileState::Failed(_)) => {
                        Some((k.clone(), p.clone(), s.stamp))
                    }
                    _ => None,
                })
                .collect();
            let missing: Vec<(LookupKey, LookupArgs)> = st
                .lookups
                .iter()
                .filter(|(_, l)| l.state == LookupState::Missing)
                .take(MAX_RECHECKS)
                .map(|(k, l)| (k.clone(), l.args.clone()))
                .collect();
            (files, missing, st.game_dir.clone(), st.content.clone())
        };
        let mut changed = false;
        for (fkey, path, stamp) in files {
            match std::fs::metadata(&path) {
                Ok(md) if md.is_file() => {
                    if stamp != Some(Stamp::of(&md)) {
                        self.load_file(&fkey, &Origin::Disk(path), true);
                    }
                }
                _ => {
                    let mut st = lock(&self.inner.state);
                    st.files.remove(&fkey);
                    st.lookups.retain(|_, l| l.state != LookupState::Found(fkey.clone()));
                    changed = true;
                }
            }
        }
        for (key, args) in missing {
            if find_file(&args, game_dir.as_deref(), content.as_ref()).is_some() {
                let mut st = lock(&self.inner.state);
                if let Some(l) = st.lookups.get_mut(&key) {
                    l.state = LookupState::Pending;
                }
                drop(st);
                self.resolve(&key);
            }
        }
        if changed {
            self.bump();
        }
    }
}

fn forget_oldest_lookups(st: &mut State) {
    let mut ages: Vec<u64> = st.lookups.values().map(|l| l.last_used).collect();
    ages.sort_unstable();
    let cut = ages.get(ages.len() / 2).copied().unwrap_or(0);
    st.lookups.retain(|_, l| l.last_used > cut || l.state == LookupState::Pending);
}

fn slot_bytes(s: &FileSlot) -> u64 {
    match &s.state {
        FileState::Ready(f) => f.bytes,
        _ => 0,
    }
}

fn evict(st: &mut State) {
    let mut total: u64 = st.files.values().map(slot_bytes).sum();
    if st.files.len() <= MAX_FILES && total <= MAX_TOTAL_BYTES {
        return;
    }
    let mut ready: Vec<(u64, String, u64)> = st
        .files
        .iter()
        .filter(|(_, s)| !matches!(s.state, FileState::Loading) && !s.reloading)
        .map(|(k, s)| (s.last_used, k.clone(), slot_bytes(s)))
        .collect();
    ready.sort_unstable();
    ready.pop();
    let mut count = st.files.len();
    for (_, k, bytes) in ready {
        if count <= MAX_FILES && total <= MAX_TOTAL_BYTES {
            break;
        }
        st.files.remove(&k);
        st.lookups.retain(|_, l| l.state != LookupState::Found(k.clone()));
        count -= 1;
        total = total.saturating_sub(bytes);
    }
}

fn find_file(args: &LookupArgs, game_dir: Option<&Path>, content: Option<&hp_fs::GameFs>) -> Option<(String, Origin)> {
    let containing_disk = args.containing.as_deref().filter(|p| !p.starts_with(CONTENT_ROOT));
    for c in candidate_paths(&args.file, args.main.as_deref(), containing_disk, game_dir) {
        if std::fs::metadata(&c).is_ok_and(|m| m.is_file()) {
            let abs = absolute(&c);
            return Some((path_key(&abs), Origin::Disk(abs)));
        }
    }
    let fs = content?;
    let mut vpaths: Vec<String> = Vec::new();
    if let Some(dir) = args
        .containing
        .as_deref()
        .and_then(|p| p.strip_prefix(CONTENT_ROOT).ok())
        .and_then(|p| p.parent())
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        && let Some(f) = normalize_file(&args.file)
        && !f.contains("..")
        && !Path::new(&f).is_absolute()
    {
        vpaths.push(format!("{dir}/{}", f.to_ascii_lowercase()));
    }
    vpaths.extend(content_path(&args.file));
    vpaths.into_iter().find(|v| fs.exists(v)).map(|v| (path_key(Path::new(&content_key(&v))), Origin::Content(v)))
}

fn read_file(origin: &Origin, content: Option<&hp_fs::GameFs>, revision: u64) -> Result<InstanceFile, String> {
    let (bytes, path) = match origin {
        Origin::Disk(p) => {
            let md = std::fs::metadata(p).map_err(|e| e.to_string())?;
            if md.len() > MAX_FILE_BYTES {
                return Err(format!("the file is too large ({} MB)", md.len() >> 20));
            }
            let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
            if bytes.len() as u64 > MAX_FILE_BYTES {
                return Err(format!("the file is too large ({} MB)", bytes.len() >> 20));
            }
            (bytes, p.clone())
        }
        Origin::Content(v) => {
            let fs = content.ok_or_else(|| "game content is not mounted".to_owned())?;
            let len = fs.read_with(v, |b| b.len() as u64).ok_or_else(|| "not found".to_owned())?;
            if len > MAX_FILE_BYTES {
                return Err(format!("the file is too large ({} MB)", len >> 20));
            }
            let bytes = fs.read(v).ok_or_else(|| "not found".to_owned())?;
            (bytes, PathBuf::from(content_key(v)))
        }
    };
    let len = bytes.len() as u64;
    let map = hp_vmf::parse_bytes(&bytes).map_err(|e| e.to_string())?;
    drop(bytes);
    Ok(prepare(map, path, len, revision))
}

fn prepare(map: Map, path: PathBuf, bytes: u64, revision: u64) -> InstanceFile {
    let parms = map_parms(&map);
    let bounds = own_bounds(&map, true);
    let map = Arc::new(map);
    let doc = Document::from_map((*map).clone());
    let scene = doc.scene();
    let mut solids: Vec<InstanceSolid> = Vec::with_capacity(scene.solids().len());
    let mut order: Vec<&SolidNode> = scene.solids().values().collect();
    order.sort_by_key(|n| n.order());
    for n in order {
        if is_hidden_object(n.solid.hidden, &n.solid.editor) {
            continue;
        }
        let (owner_class, owner_color) = match n.owner {
            hp_editor::Owner::World => (None, n.solid.editor.color),
            hp_editor::Owner::Entity(e) => match scene.entity(e) {
                Some(en) if is_hidden_object(en.entity.hidden, &en.entity.editor) => continue,
                Some(en) => (Some(en.entity.classname.clone()), en.entity.editor.color),
                None => (None, n.solid.editor.color),
            },
        };
        solids.push(InstanceSolid { node: n.clone(), owner_class, owner_color });
    }
    InstanceFile { path: Arc::from(path), map, parms, bounds, solids, revision, bytes }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOM: &str = r#"
world
{
	"id" "1"
	"classname" "worldspawn"
	solid
	{
		"id" "2"
		side { "id" "1" "plane" "(0 16 16) (16 16 16) (16 0 16)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
		side { "id" "2" "plane" "(0 0 0) (16 0 0) (16 16 0)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
		side { "id" "3" "plane" "(16 16 16) (0 16 16) (0 16 0)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
		side { "id" "4" "plane" "(16 0 0) (0 0 0) (0 0 16)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
		side { "id" "5" "plane" "(16 16 0) (16 0 0) (16 0 16)" "material" "DEV/A" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
		side { "id" "6" "plane" "(0 16 16) (0 0 16) (0 0 0)" "material" "DEV/A" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
	}
}
entity
{
	"id" "3"
	"classname" "func_instance_parms"
	"parm1" "$color color255 255 0 0"
}
"#;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hp_instances_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("maps").join("instances")).expect("temp dir");
        d
    }

    fn load_now(cache: &InstanceCache, file: &str, main: Option<&Path>) -> Loaded {
        let req = LoadRequest { file, containing: None, depth: 0 };
        for _ in 0..200 {
            match cache.load(&req, main) {
                Loaded::Pending => {
                    cache.wait_idle(Duration::from_secs(5));
                }
                other => return other,
            }
        }
        Loaded::Pending
    }

    #[test]
    fn files_are_found_loaded_once_and_reloaded_when_changed() {
        let dir = temp_dir("load");
        let main = dir.join("maps").join("sub").join("main.vmf");
        let inst = dir.join("maps").join("instances").join("room.vmf");
        std::fs::write(&inst, ROOM).expect("write");
        let cache = InstanceCache::new();
        let g0 = cache.generation();
        let Loaded::Ready { path, map } = load_now(&cache, "instances/room", Some(&main)) else { panic!("not loaded") };
        assert!(cache.generation() > g0);
        assert_eq!(map.world.solids.len(), 1);
        let f = cache.file(&path).expect("prepared");
        assert_eq!(f.parms.len(), 1);
        assert_eq!(f.solids.len(), 1);
        assert_eq!(f.bounds.max, glam::DVec3::splat(16.0));
        assert!(!f.is_content());
        let Loaded::Ready { map: again, .. } = load_now(&cache, "instances\\ROOM.vmf", Some(&main)) else {
            panic!("not loaded")
        };
        assert!(Arc::ptr_eq(&map, &again) || cfg!(not(windows)));
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&inst, ROOM.replace("\"id\" \"3\"", "\"id\" \"4\"\n\t\"origin\" \"0 0 0\"")).expect("write");
        cache.inner.state.lock().unwrap().last_poll = None;
        cache.poll();
        cache.wait_idle(Duration::from_secs(5));
        let f2 = cache.file(&path).expect("reloaded");
        assert_ne!(f2.revision, f.revision);
        assert!(matches!(load_now(&cache, "instances/later.vmf", Some(&main)), Loaded::Missing));
        std::fs::write(dir.join("maps").join("instances").join("later.vmf"), ROOM).expect("write");
        cache.inner.state.lock().unwrap().last_poll = None;
        cache.poll();
        cache.wait_idle(Duration::from_secs(5));
        assert!(matches!(load_now(&cache, "instances/later.vmf", Some(&main)), Loaded::Ready { .. }));
        std::fs::remove_file(&inst).expect("remove");
        cache.inner.state.lock().unwrap().last_poll = None;
        cache.poll();
        cache.wait_idle(Duration::from_secs(5));
        assert!(matches!(load_now(&cache, "instances/room.vmf", Some(&main)), Loaded::Missing));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_and_hostile_files_fail_cleanly() {
        let dir = temp_dir("broken");
        let main = dir.join("maps").join("main.vmf");
        std::fs::write(dir.join("maps").join("bin.vmf"), [0u8, 1, 2, 3, 0, 0, 0, 0xff]).expect("write");
        let cache = InstanceCache::new();
        assert!(matches!(load_now(&cache, "bin.vmf", Some(&main)), Loaded::Failed(_)));
        assert!(matches!(load_now(&cache, "\\\\server\\share\\x.vmf", Some(&main)), Loaded::Missing));
        assert!(matches!(load_now(&cache, "", Some(&main)), Loaded::Missing));
        assert!(matches!(load_now(&cache, "nope.vmf", None), Loaded::Missing));
        cache.clear();
        assert_eq!(cache.file_count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn instances_of_the_game_content_are_found() {
        let dir = temp_dir("content");
        std::fs::write(dir.join("maps").join("instances").join("shipped.vmf"), ROOM).expect("write");
        let fs = hp_fs::GameFs::empty();
        fs.add_game_dir(&dir).expect("mount");
        let cache = InstanceCache::new();
        assert!(matches!(load_now(&cache, "instances/shipped", None), Loaded::Missing), "not mounted yet");
        cache.set_content(Some(&fs));
        let Loaded::Ready { path, .. } = load_now(&cache, "instances/shipped", None) else { panic!("not found") };
        let f = cache.file(&path).expect("file");
        assert!(f.is_content(), "{}", path.display());
        assert_eq!(f.parms.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn eviction_keeps_recent_files() {
        let mut st = State::default();
        for i in 0..(MAX_FILES + 10) {
            st.files.insert(
                format!("f{i}"),
                FileSlot {
                    state: FileState::Failed(String::new()),
                    origin: Origin::Disk(PathBuf::new()),
                    stamp: None,
                    last_used: i as u64,
                    reloading: false,
                },
            );
        }
        evict(&mut st);
        assert_eq!(st.files.len(), MAX_FILES);
        assert!(!st.files.contains_key("f0") && st.files.contains_key(&format!("f{}", MAX_FILES + 9)));
        assert_eq!(absolute(Path::new("/a/b/../c/./d")), std::path::absolute(Path::new("/a/c/d")).unwrap());
    }
}
