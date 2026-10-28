use std::sync::Arc;

use ahash::AHashMap;
use hp_vmt::Material;
use parking_lot::RwLock;

use crate::fs::GameFs;

#[derive(Default)]
struct CacheState {
    generation: u64,
    map: AHashMap<Box<str>, Option<Arc<Material>>>,
}

#[derive(Default)]
pub(crate) struct MaterialCache {
    state: RwLock<CacheState>,
}

impl GameFs {
    pub fn load_material(&self, name: &str) -> Option<Material> {
        self.material(name).map(|m| (*m).clone())
    }

    pub fn material(&self, name: &str) -> Option<Arc<Material>> {
        let key = hp_vmt::normalize_material_name(name);
        if key.is_empty() {
            return None;
        }
        let generation = self.generation();
        {
            let state = self.inner.materials.state.read();
            if state.generation == generation
                && let Some(hit) = state.map.get(key.as_str())
            {
                return hit.clone();
            }
        }
        let loaded = self.load_material_uncached(&key).map(Arc::new);
        let mut state = self.inner.materials.state.write();
        if state.generation < generation {
            state.map.clear();
            state.generation = generation;
        }
        if state.generation == generation {
            state.map.insert(key.into_boxed_str(), loaded.clone());
        }
        loaded
    }

    pub fn material_source(&self, name: &str) -> Option<u32> {
        let key = hp_vmt::normalize_material_name(name);
        if key.is_empty() {
            return None;
        }
        self.which_source(&format!("materials/{key}.vmt"))
    }

    fn load_material_uncached(&self, key: &str) -> Option<Material> {
        let bytes = self.read(&format!("materials/{key}.vmt"))?;
        let material = match hp_vmt::parse_bytes(&bytes) {
            Ok(m) => m,
            Err(e) => {
                log::debug!("cannot parse material {key}: {e}");
                return None;
            }
        };
        if !material.is_patch() {
            return Some(material);
        }
        match hp_vmt::resolve_patch(&material, &|path| self.read_text(path)) {
            Ok(resolved) => Some(resolved),
            Err(e) => {
                log::debug!("cannot resolve patch material {key}: {e}");
                Some(material)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::GameFs;
    use crate::test_util::TempDir;

    #[test]
    fn loads_resolves_patches_and_caches() {
        let tmp = TempDir::new("material");
        tmp.write(
            "g/materials/base/floor.vmt",
            b"\"LightmappedGeneric\" { \"$basetexture\" \"base/floor\" \"$surfaceprop\" \"concrete\" }",
        );
        tmp.write(
            "g/materials/maps/x/floor.vmt",
            b"patch { include \"materials/base/floor.vmt\" insert { \"$envmap\" \"env_cubemap\" } }",
        );
        tmp.write("g/materials/broken/patch.vmt", b"patch { include \"materials/missing.vmt\" }");
        let fs = GameFs::empty();
        fs.add_game_dir(&tmp.path().join("g")).unwrap();

        let m = fs.load_material("Materials\\Base\\Floor.vmt").unwrap();
        assert_eq!(m.base_texture().as_deref(), Some("base/floor"));
        let p = fs.material("maps/x/floor").unwrap();
        assert_eq!(p.shader, "LightmappedGeneric");
        assert_eq!(p.get("$envmap"), Some("env_cubemap"));
        assert_eq!(p.base_texture().as_deref(), Some("base/floor"));
        assert!(std::sync::Arc::ptr_eq(&p, &fs.material("maps/x/floor").unwrap()));
        assert!(fs.material("broken/patch").unwrap().is_patch());
        assert!(fs.material("missing").is_none());
        assert!(fs.material("").is_none());

        tmp.write("g/materials/missing.vmt", b"\"UnlitGeneric\" { }");
        fs.refresh();
        assert!(fs.material("missing").is_some());
        assert!(!fs.material("broken/patch").unwrap().is_patch());
    }

    #[test]
    fn concurrent_lookups() {
        use rayon::prelude::*;
        let tmp = TempDir::new("material-par");
        for i in 0..50 {
            tmp.write(
                &format!("g/materials/m/{i}.vmt"),
                format!("\"UnlitGeneric\" {{ \"$basetexture\" \"m/{i}\" }}").as_bytes(),
            );
        }
        let fs = GameFs::empty();
        fs.add_game_dir(&tmp.path().join("g")).unwrap();
        let found = (0..2000).into_par_iter().filter(|i| fs.material(&format!("m/{}", i % 60)).is_some()).count();
        assert_eq!(found, 33 * 50 + 20);
    }
}
