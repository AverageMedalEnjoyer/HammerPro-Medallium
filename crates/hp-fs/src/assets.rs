use std::sync::Arc;

use parking_lot::Mutex;
use rayon::prelude::*;

use crate::fs::GameFs;
use crate::path::normalize_path;
use crate::source::Source;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetEntry {
    pub name: String,
    pub source: u32,
}

type Slot = Mutex<Option<(u64, Arc<Vec<AssetEntry>>)>>;

#[derive(Default)]
pub(crate) struct AssetCaches {
    materials: Slot,
    models: Slot,
    sounds: Slot,
    pub(crate) keywords: KeywordSlot,
    pub(crate) keywords_quick: KeywordSlot,
}

pub(crate) type KeywordSlot = Mutex<Option<(u64, Arc<crate::keywords::MaterialKeywords>)>>;

const SOUND_EXTENSIONS: &[&str] = &["wav", "mp3", "ogg"];

fn collect<F>(snapshot: &[Arc<Source>], prefix: &str, exts: &[Option<&str>], name_of: F) -> Vec<AssetEntry>
where
    F: Fn(&str) -> &str + Sync,
{
    let per_source: Vec<Vec<(&str, u32)>> = snapshot
        .par_iter()
        .enumerate()
        .map(|(rank, source)| {
            let mut names = Vec::new();
            if !source.is_enabled() {
                return names;
            }
            let rank = u32::try_from(rank).unwrap_or(u32::MAX);
            for ext in exts {
                source.for_each_path(prefix, *ext, &mut |path| {
                    let name = name_of(path);
                    if !name.is_empty() {
                        names.push((name, rank));
                    }
                });
            }
            names
        })
        .collect();
    let mut all: Vec<(&str, u32)> = Vec::with_capacity(per_source.iter().map(Vec::len).sum());
    for names in per_source {
        all.extend(names);
    }
    all.par_sort_unstable();
    all.dedup_by(|later, earlier| later.0 == earlier.0);
    all.into_par_iter()
        .map(|(name, rank)| AssetEntry {
            name: name.to_owned(),
            source: snapshot.get(rank as usize).map_or(0, |s| s.id),
        })
        .collect()
}

fn strip(path: &str, dir_len: usize, strip_ext: bool) -> &str {
    let rest = path.get(dir_len..).unwrap_or("");
    if strip_ext { crate::path::strip_extension(rest) } else { rest }
}

impl GameFs {
    fn cached(&self, slot: &Slot, build: impl FnOnce(&[Arc<Source>]) -> Vec<AssetEntry>) -> Arc<Vec<AssetEntry>> {
        let generation = self.generation();
        if let Some((built, list)) = slot.lock().as_ref()
            && *built == generation
        {
            return Arc::clone(list);
        }
        let list = Arc::new(build(&self.snapshot()));
        let mut guard = slot.lock();
        if guard.as_ref().is_none_or(|(built, _)| *built <= generation) {
            *guard = Some((generation, Arc::clone(&list)));
        }
        list
    }

    pub fn materials(&self) -> Arc<Vec<AssetEntry>> {
        self.cached(&self.inner.assets.materials, |snapshot| {
            collect(snapshot, "materials", &[Some("vmt")], |p| strip(p, "materials/".len(), true))
        })
    }

    pub fn models(&self) -> Arc<Vec<AssetEntry>> {
        self.cached(&self.inner.assets.models, |snapshot| collect(snapshot, "models", &[Some("mdl")], |p| p))
    }

    pub fn sounds(&self) -> Arc<Vec<AssetEntry>> {
        self.cached(&self.inner.assets.sounds, |snapshot| {
            let exts: Vec<Option<&str>> = SOUND_EXTENSIONS.iter().map(|e| Some(*e)).collect();
            collect(snapshot, "sound", &exts, |p| strip(p, "sound/".len(), false))
        })
    }

    pub fn list(&self, prefix: &str, ext: Option<&str>) -> Vec<AssetEntry> {
        let prefix = normalize_path(prefix);
        let prefix = prefix.trim_matches('/');
        let ext = ext.map(|e| e.trim_start_matches('.').to_ascii_lowercase());
        collect(&self.snapshot(), prefix, &[ext.as_deref()], |p| p)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_util::{TempDir, png};
    use crate::{CustomDir, GameFs};

    #[test]
    fn indexes_are_deduplicated_sorted_and_invalidated() {
        let tmp = TempDir::new("assets");
        tmp.write("a/materials/B/X.vmt", b"");
        tmp.write("a/materials/a.vmt", b"");
        tmp.write("a/models/m.mdl", b"");
        tmp.write("a/models/m.vvd", b"");
        tmp.write("a/sound/s/one.WAV", b"");
        tmp.write("a/sound/s/two.ogg", b"");
        tmp.write("a/sound/s/notes.txt", b"");
        tmp.write("b/materials/b/x.vmt", b"");
        tmp.write("b/materials/c.vmt", b"");
        tmp.write("c/Img.png", &png(1, 1, [0, 0, 0, 255]));
        let fs = GameFs::empty();
        let a = fs.add_game_dir(&tmp.path().join("a")).unwrap()[0];
        let b = fs.add_game_dir(&tmp.path().join("b")).unwrap()[0];
        let c = fs.add_custom_dir(CustomDir::with_prefix(tmp.path().join("c"), "pics")).unwrap();

        let mats = fs.materials();
        let names: Vec<(&str, u32)> = mats.iter().map(|e| (e.name.as_str(), e.source)).collect();
        assert_eq!(names, [("a", a), ("b/x", a), ("c", b), ("pics/img", c)]);
        assert!(std::sync::Arc::ptr_eq(&mats, &fs.materials()));

        let models = fs.models();
        assert_eq!(models.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["models/m.mdl"]);
        let sounds = fs.sounds();
        assert_eq!(sounds.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["s/one.wav", "s/two.ogg"]);

        let listed = fs.list("Materials/", Some(".VMT"));
        assert_eq!(listed.len(), 4);
        assert_eq!(listed[0].name, "materials/a.vmt");
        assert_eq!(fs.list("models", None).len(), 2);

        fs.set_source_enabled(a, false);
        let mats = fs.materials();
        let names: Vec<(&str, u32)> = mats.iter().map(|e| (e.name.as_str(), e.source)).collect();
        assert_eq!(names, [("b/x", b), ("c", b), ("pics/img", c)]);
        assert!(fs.models().is_empty());
    }
}
