pub mod cache;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hp_geom::Aabb;

use hp_editor::Id;
use hp_editor::instances::{ExpandOptions, Expansion, KeyFixup};

use crate::settings::{InstanceDisplay, Settings};

pub use cache::{CONTENT_ROOT, InstanceCache, InstanceFile, InstanceSolid};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InstanceScene {
    pub display: InstanceDisplay,
    pub show_context: bool,
}

impl Default for InstanceScene {
    fn default() -> Self {
        Self { display: InstanceDisplay::Tinted, show_context: true }
    }
}

impl InstanceScene {
    pub fn from_settings(s: &Settings) -> Self {
        Self { display: s.instances.display, show_context: s.instances.show_context }
    }
}

thread_local! {
    static SCENE: Cell<InstanceScene> = Cell::new(InstanceScene::default());
    static MAIN_MAP: RefCell<Option<Arc<Path>>> = const { RefCell::new(None) };
    static BOUNDS: RefCell<(u64, HashMap<u64, Option<Aabb>>)> = RefCell::new((0, HashMap::new()));
    static CONTEXT: RefCell<Option<InstanceContext>> = const { RefCell::new(None) };
}

const MAX_BOUNDS: usize = 16_384;

pub fn publish_scene(scene: InstanceScene) {
    SCENE.with(|s| s.set(scene));
}

pub fn scene() -> InstanceScene {
    SCENE.with(Cell::get)
}

pub fn publish_main_map(path: Option<&Path>) {
    MAIN_MAP.with(|m| {
        let mut m = m.borrow_mut();
        if m.as_deref() != path {
            *m = path.map(Arc::from);
        }
    });
}

pub fn main_map() -> Option<Arc<Path>> {
    MAIN_MAP.with(|m| m.borrow().clone())
}

pub fn publish_context(context: Option<&InstanceContext>) {
    CONTEXT.with(|c| {
        let mut c = c.borrow_mut();
        if c.as_ref() != context {
            *c = context.cloned();
        }
    });
}

pub fn context() -> Option<InstanceContext> {
    CONTEXT.with(|c| c.borrow().clone())
}

#[derive(Clone, Debug, PartialEq)]
pub struct InstanceContext {
    pub parent: PathBuf,
    pub entity: Id,
}

#[derive(Debug, Default)]
pub struct InstancesRuntime {
    pub create_open: bool,
    pub confirm_collapse_all: bool,
    pub pending_collapse: Option<Vec<Id>>,
    pub contexts: HashMap<String, InstanceContext>,
    pub any_cache: Cell<Option<(u64, bool)>>,
}

impl InstancesRuntime {
    pub fn cache(&self) -> InstanceCache {
        InstanceCache::shared()
    }

    pub fn map_has_instances(&self, doc: &hp_editor::Document) -> bool {
        let rev = doc.revision();
        if let Some((r, any)) = self.any_cache.get()
            && r == rev
        {
            return any;
        }
        let any = doc.scene().entities().values().any(|n| hp_editor::instances::is_instance(&n.entity));
        self.any_cache.set(Some((rev, any)));
        any
    }
}

pub fn selected_instances(doc: &hp_editor::Document) -> Vec<Id> {
    doc.selection()
        .objects
        .iter()
        .filter_map(|o| match *o {
            hp_editor::ObjRef::Entity(id) => {
                doc.scene().entity(id).filter(|n| hp_editor::instances::is_instance(&n.entity)).map(|_| id)
            }
            hp_editor::ObjRef::Solid(_) => None,
        })
        .collect()
}

pub fn key_fixup(fgd: &hp_fgd::ResolvedFgd, class: &str, key: &str) -> Option<KeyFixup> {
    use hp_fgd::PropertyType as T;
    let c = fgd.class(class)?;
    if key.is_empty() {
        return Some(KeyFixup::Keep);
    }
    Some(match c.property(key).map(|p| &p.kind) {
        Some(T::TargetSource | T::TargetDestination) => KeyFixup::Name,
        Some(T::Origin) => KeyFixup::Origin,
        Some(T::Angle) => KeyFixup::Angle,
        Some(T::AngleNegativePitch) => KeyFixup::AngleNegativePitch,
        Some(T::Axis) => KeyFixup::Axis,
        _ => KeyFixup::Keep,
    })
}

pub fn expand_one(
    cache: &InstanceCache,
    entity: &hp_vmf::Entity,
    main: Option<&Path>,
    fgd: Option<&hp_fgd::ResolvedFgd>,
    skip_hidden: bool,
) -> Expansion {
    let kinds = |c: &str, k: &str| fgd.and_then(|f| key_fixup(f, c, k));
    let opts = ExpandOptions { skip_hidden, kinds: Some(&kinds), main_map: main, first_auto_index: 1 };
    let mut source = cache.source(main);
    hp_editor::instances::expand(&[entity], &mut source, &opts).pop().unwrap_or_default()
}

pub fn instance_bounds(
    cache: &InstanceCache,
    entity: &hp_vmf::Entity,
    main: Option<&Path>,
    fgd: Option<&hp_fgd::ResolvedFgd>,
) -> Option<Aabb> {
    if !hp_editor::instances::is_instance(entity) {
        return None;
    }
    let generation = cache.generation();
    let hash = {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        entity.props.hash(&mut h);
        main.hash(&mut h);
        fgd.map(|f| f as *const hp_fgd::ResolvedFgd as usize).hash(&mut h);
        h.finish()
    };
    let known = BOUNDS.with(|b| {
        let mut b = b.borrow_mut();
        if b.0 != generation {
            b.0 = generation;
            b.1.clear();
        }
        b.1.get(&hash).copied()
    });
    if let Some(bounds) = known {
        return bounds;
    }
    let exp = expand_one(cache, entity, main, fgd, true);
    let mut out = Aabb::EMPTY;
    for piece in &exp.pieces {
        if let Some(file) = cache.file(&piece.path) {
            out = out.union(&hp_editor::instances::transformed_aabb(&file.bounds, &piece.fixup.matrix));
        }
    }
    let bounds = (!out.is_empty() && out.min.is_finite() && out.max.is_finite()).then_some(out);
    if !exp.is_pending() {
        BOUNDS.with(|b| {
            let mut b = b.borrow_mut();
            if b.1.len() >= MAX_BOUNDS {
                b.1.clear();
            }
            b.1.insert(hash, bounds);
        });
    }
    bounds
}

type NamesMemo = Option<((u64, u64), Arc<Vec<String>>)>;

thread_local! {
    static NAMES: RefCell<NamesMemo> = const { RefCell::new(None) };
}

pub fn instance_target_names(doc: &hp_editor::Document, fgd: Option<&hp_fgd::ResolvedFgd>) -> Arc<Vec<String>> {
    let cache = InstanceCache::shared();
    let key = (doc.revision(), cache.generation());
    if let Some(names) = NAMES.with(|n| n.borrow().as_ref().filter(|(k, _)| *k == key).map(|(_, v)| Arc::clone(v))) {
        return names;
    }
    let kinds = |c: &str, k: &str| fgd.and_then(|f| key_fixup(f, c, k));
    let main = doc.path();
    let opts = ExpandOptions { skip_hidden: false, kinds: Some(&kinds), main_map: main, first_auto_index: 1 };
    let mut source = cache.source(main);
    let exps: Vec<Expansion> = doc.expand_instances(&mut source, &opts).into_iter().map(|(_, e)| e).collect();
    let names = Arc::new(hp_editor::instances::fixed_up_names(&exps, Some(&kinds)));
    NAMES.with(|n| *n.borrow_mut() = Some((key, Arc::clone(&names))));
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_display_is_published_per_thread() {
        assert_eq!(scene(), InstanceScene::default());
        let s = InstanceScene { display: InstanceDisplay::Hidden, show_context: false };
        publish_scene(s);
        assert_eq!(scene(), s);
        std::thread::spawn(|| assert_eq!(scene(), InstanceScene::default())).join().expect("thread");
        publish_scene(InstanceScene::default());
    }

    #[test]
    fn bounds_and_names_of_the_instances_of_a_map() {
        let dir = std::env::temp_dir().join(format!("hp_instances_rt_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let text = "world
{
	\"id\" \"1\"
	\"classname\" \"worldspawn\"
}
entity
{
	\"id\" \"2\"
	\"classname\" \"info_target\"
	\"targetname\" \"spot\"
	\"origin\" \"100 0 0\"
}
";
        std::fs::write(dir.join("spot.vmf"), text).expect("write");
        let mut doc = hp_editor::Document::new_empty();
        doc.set_path(Some(dir.join("main.vmf")));
        let id = doc
            .create_point_entity(
                "func_instance",
                glam::DVec3::new(0.0, 0.0, 64.0),
                &[("file".into(), "spot.vmf".into()), ("targetname".into(), "a".into())],
            )
            .expect("instance");
        let cache = InstanceCache::shared();
        let entity = doc.scene().entity(id).expect("entity").entity.clone();
        let mut bounds = None;
        for _ in 0..100 {
            bounds = instance_bounds(&cache, &entity, doc.path(), None);
            if bounds.is_some() {
                break;
            }
            cache.wait_idle(std::time::Duration::from_millis(50));
        }
        let b = bounds.expect("bounds once loaded");
        assert_eq!(b.center(), glam::DVec3::new(100.0, 0.0, 64.0));
        assert_eq!(instance_bounds(&cache, &entity, doc.path(), None), Some(b), "remembered");
        let names = instance_target_names(&doc, None);
        assert_eq!(*names, vec!["a-spot".to_owned()]);
        assert!(selected_instances(&doc).contains(&id));
        let rt = InstancesRuntime::default();
        assert!(rt.map_has_instances(&doc));
        assert!(!rt.map_has_instances(&hp_editor::Document::new_empty()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
