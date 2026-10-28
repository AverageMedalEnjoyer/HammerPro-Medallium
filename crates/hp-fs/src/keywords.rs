use std::sync::Arc;

use ahash::{AHashMap, AHashSet};
use rayon::prelude::*;

use crate::fs::GameFs;
use crate::path::strip_extension;
use crate::source::{Source, SourceData, SourceOrigin};

type KeywordList = Box<[Box<str>]>;

#[derive(Debug, Default)]
pub struct MaterialKeywords {
    map: AHashMap<Box<str>, KeywordList>,
    covered: AHashSet<u32>,
    scanned: usize,
    complete: bool,
}

impl MaterialKeywords {
    pub fn get(&self, material: &str) -> impl Iterator<Item = &str> {
        self.map.get(material).into_iter().flat_map(|k| k.iter().map(|s| &**s))
    }

    pub fn has_keywords(&self, material: &str) -> bool {
        self.map.contains_key(material)
    }

    pub fn covers(&self, source: u32) -> bool {
        self.complete || self.covered.contains(&source)
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn scanned(&self) -> usize {
        self.scanned
    }
}

fn contains_ignore_case(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle))
}

enum Scan {
    Keywords(Vec<Box<str>>),
    Patch,
    None,
}

fn scan(bytes: &[u8]) -> Scan {
    let head = bytes.get(..bytes.len().min(512)).unwrap_or(bytes);
    let patch = contains_ignore_case(head, b"patch") && contains_ignore_case(bytes, b"include");
    if patch {
        return Scan::Patch;
    }
    if !contains_ignore_case(bytes, b"%keywords") {
        return Scan::None;
    }
    match hp_vmt::parse_bytes(bytes) {
        Ok(m) => Scan::Keywords(m.keywords().map(Box::from).collect()),
        Err(_) => Scan::None,
    }
}

fn is_quick(source: &Source) -> bool {
    !matches!(source.origin, SourceOrigin::Workshop | SourceOrigin::Download)
        && !matches!(&source.data, SourceData::Gma(g) if g.is_legacy())
}

impl GameFs {
    pub fn material_keywords(&self) -> Arc<MaterialKeywords> {
        let generation = self.generation();
        if let Some((built, index)) = self.inner.assets.keywords.lock().as_ref()
            && *built == generation
        {
            return Arc::clone(index);
        }
        let quick = self
            .inner
            .assets
            .keywords_quick
            .lock()
            .as_ref()
            .filter(|(g, _)| *g == generation)
            .map(|(_, k)| Arc::clone(k));
        let index = Arc::new(match quick {
            Some(quick) => {
                let mut index = self.build_keywords(&|s| !quick.covered.contains(&s.id));
                index.map.extend(quick.map.iter().map(|(k, v)| (k.clone(), v.clone())));
                index.covered.extend(quick.covered.iter().copied());
                index.scanned += quick.scanned;
                index.complete =
                    self.snapshot().iter().filter(|s| s.is_enabled()).all(|s| index.covered.contains(&s.id));
                index
            }
            None => self.build_keywords(&|_| true),
        });
        let mut slot = self.inner.assets.keywords.lock();
        if slot.as_ref().is_none_or(|(built, _)| *built <= generation) {
            *slot = Some((generation, Arc::clone(&index)));
        }
        index
    }

    pub fn material_keywords_quick(&self) -> Arc<MaterialKeywords> {
        let generation = self.generation();
        if let Some((built, index)) = self.inner.assets.keywords.lock().as_ref()
            && *built == generation
        {
            return Arc::clone(index);
        }
        if let Some((built, index)) = self.inner.assets.keywords_quick.lock().as_ref()
            && *built == generation
        {
            return Arc::clone(index);
        }
        let index = Arc::new(self.build_keywords(&is_quick));
        let mut slot = self.inner.assets.keywords_quick.lock();
        if slot.as_ref().is_none_or(|(built, _)| *built <= generation) {
            *slot = Some((generation, Arc::clone(&index)));
        }
        index
    }

    fn build_keywords(&self, include: &(dyn Fn(&Source) -> bool + Sync)) -> MaterialKeywords {
        let materials = self.materials();
        let winner: AHashMap<&str, u32> = materials.iter().map(|e| (e.name.as_str(), e.source)).collect();
        let snapshot = self.snapshot();
        let sources: Vec<&Source> = snapshot.iter().map(|s| &**s).filter(|s| s.is_enabled() && include(s)).collect();
        type Found<'a> = (Vec<(Box<str>, KeywordList)>, Vec<&'a str>, usize);
        let scanned: Vec<Found<'_>> = sources
            .par_iter()
            .map(|source| {
                let mut found = Vec::new();
                let mut patches = Vec::new();
                let mut count = 0;
                source.for_each_path("materials", Some("vmt"), &mut |path| {
                    let name = strip_extension(path.strip_prefix("materials/").unwrap_or(path));
                    let Some((name, _)) = winner.get_key_value(name).filter(|(_, id)| **id == source.id) else {
                        return;
                    };
                    count += 1;
                    match source.read_cow(path).map(|bytes| scan(&bytes)) {
                        Some(Scan::Keywords(k)) if !k.is_empty() => {
                            found.push((Box::from(*name), k.into_boxed_slice()))
                        }
                        Some(Scan::Patch) => patches.push(*name),
                        _ => {}
                    }
                });
                (found, patches, count)
            })
            .collect();
        let mut index = MaterialKeywords::default();
        let mut patches: Vec<&str> = Vec::new();
        for (found, p, count) in scanned {
            index.map.extend(found);
            patches.extend(p);
            index.scanned += count;
        }
        let patched: Vec<(Box<str>, KeywordList)> = patches
            .par_iter()
            .filter_map(|name| {
                let keywords: Vec<Box<str>> = self.material(name)?.keywords().map(Box::from).collect();
                (!keywords.is_empty()).then(|| (Box::from(*name), keywords.into_boxed_slice()))
            })
            .collect();
        index.map.extend(patched);
        index.covered = sources.iter().map(|s| s.id).collect();
        index.complete = snapshot.iter().filter(|s| s.is_enabled()).all(|s| index.covered.contains(&s.id));
        index
    }
}

#[cfg(test)]
mod tests {
    use crate::test_util::TempDir;
    use crate::{CustomDir, GameFs};

    #[test]
    fn keywords_of_plain_and_patch_materials() {
        let tmp = TempDir::new("keywords");
        tmp.write("g/materials/a/plain.vmt", b"LightmappedGeneric { $basetexture a/plain }");
        tmp.write("g/materials/a/tagged.vmt", b"LightmappedGeneric { \"%Keywords\" \"Urban, concrete,\" }");
        tmp.write("g/materials/a/patched.vmt", b"patch { include \"materials/a/tagged.vmt\" insert { $nocull 1 } }");
        tmp.write("g/materials/a/broken.vmt", b"%keywords {{{");
        let fs = GameFs::empty();
        let id = fs.add_game_dir(&tmp.path().join("g")).unwrap()[0];
        let k = fs.material_keywords();
        assert_eq!(k.scanned(), 4);
        assert_eq!(k.get("a/tagged").collect::<Vec<_>>(), ["Urban", "concrete"]);
        assert_eq!(k.get("a/patched").collect::<Vec<_>>(), ["Urban", "concrete"]);
        assert!(!k.has_keywords("a/plain"));
        assert_eq!(k.get("a/missing").count(), 0);
        assert_eq!(k.len(), 2);
        assert!(k.is_complete() && k.covers(id));
        assert!(std::sync::Arc::ptr_eq(&k, &fs.material_keywords()));
        assert!(std::sync::Arc::ptr_eq(&k, &fs.material_keywords_quick()));
        fs.refresh();
        assert!(!std::sync::Arc::ptr_eq(&k, &fs.material_keywords()));
    }

    #[test]
    fn quick_index_then_complete_index() {
        let tmp = TempDir::new("keywords-quick");
        tmp.write("c/wall.vmt", b"LightmappedGeneric { %keywords custom }");
        tmp.write("g/materials/w/x.vmt", b"LightmappedGeneric { %keywords extra }");
        let fs = GameFs::empty();
        let custom = fs.add_custom_dir(CustomDir::with_prefix(tmp.path().join("c"), "mine")).unwrap();
        let game = fs.add_game_dir(&tmp.path().join("g")).unwrap()[0];
        let quick = fs.material_keywords_quick();
        assert!(quick.covers(custom) && quick.covers(game));
        assert!(quick.is_complete());
        assert_eq!(quick.get("mine/wall").collect::<Vec<_>>(), ["custom"]);
        let full = fs.material_keywords();
        assert_eq!(full.get("w/x").collect::<Vec<_>>(), ["extra"]);
        assert_eq!(full.len(), 2);
    }
}
