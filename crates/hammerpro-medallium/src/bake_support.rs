use std::path::{Path, PathBuf};

use hp_bspc::materials::{MaterialFacts, MaterialOptions, facts_by_name, facts_from_vmt_with};



pub(crate) struct GameMaterials {
    pub(crate) fs: hp_fs::GameFs,
    pub(crate) opts: MaterialOptions,
}

impl hp_bspc::MaterialSource for GameMaterials {
    fn facts(&self, name: &str) -> MaterialFacts {
        let Some(mat) = self.fs.material(name) else {
            let mut f = facts_by_name(name);
            f.found = false;
            f.has_surfaceprop = true;
            let mounted = !self.fs.sources().is_empty();
            if mounted && f.surf == 0 && f.contents == hp_bspc::materials::contents::SOLID {
                f.surf = if self.opts.light_if_missing { 0 } else { hp_bspc::materials::surf::NOLIGHT };
                f.width = 0;
                f.height = 0;
                f.reflectivity = [0.0; 3];
            }
            return f;
        };
        let mat = mat.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false);
        let base = mat.base_texture();
        let texture = base
            .clone()
            .or_else(|| ["$envmapmask", "$bumpmap", "$dudvmap", "$normalmap"].iter().find_map(|k| mat.texture(k)));
        let info = texture.as_deref().and_then(|t| self.fs.texture_info(t));
        const ERROR_SIZE: (i32, i32) = (128, 128);
        let tool = mat.get("%tooltexture").map(str::trim).filter(|t| !t.is_empty());
        let size = match tool {
            Some(t) => Some(self.fs.texture_info(t).map_or(ERROR_SIZE, |i| (i.width as i32, i.height as i32))),
            None => match &info {
                Some(i) => Some((i.width as i32, i.height as i32)),
                None => texture.is_some().then_some(ERROR_SIZE),
            },
        };
        let base_info = if base.is_some() { info.as_ref() } else { None };
        let base_missing = base.is_some() && base_info.is_none();
        let explicit = mat.get("$reflectivity").and_then(|v| {
            let n: Vec<f32> = v
                .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                .filter_map(|x| x.parse().ok())
                .collect();
            (n.len() == 3).then(|| [n[0], n[1], n[2]])
        });
        let reflectivity = explicit
            .or_else(|| base_info.map(|i| i.reflectivity))
            .or_else(|| base_missing.then_some([0.0; 3]))
            .filter(|r| r.iter().all(|v| v.is_finite()))
            .or(Some([0.2; 3]));
        let key = |k: &str| mat.get(k).map(str::to_owned);
        let mut f = facts_from_vmt_with(&mat.shader, &key, size, reflectivity, &self.opts);
        let by_name = facts_by_name(name);
        if by_name.surf != 0 && f.surf == 0 && f.contents == hp_bspc::materials::contents::SOLID {
            f.contents = by_name.contents;
            f.raw_contents = by_name.raw_contents;
            f.surf = by_name.surf;
        }
        f
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.fs.read(path).or_else(|| loose_file(&self.fs, path))
    }
}

fn loose_file(fs: &hp_fs::GameFs, path: &str) -> Option<Vec<u8>> {
    let rel = Path::new(path.trim_start_matches(['/', '\\']));
    if rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return None;
    }
    let sources = fs.sources();
    let dirs = [hp_fs::SourceOrigin::Extra, hp_fs::SourceOrigin::GameLoose]
        .iter()
        .flat_map(|o| sources.iter().filter(move |s| s.origin == *o).map(|s| s.path.clone()))
        .chain(fs.game_dir().map(Path::to_path_buf));
    dirs.filter(|d| d.is_dir()).find_map(|d| std::fs::read(d.join(rel)).ok())
}

pub(crate) struct GameModels<'a> {
    pub(crate) fs: &'a hp_fs::GameFs,
    pub(crate) allow_dynamic: bool,
}

impl hp_bspc::props::ModelSource for GameModels<'_> {
    fn bounds(&self, path: &str) -> Option<(glam::DVec3, glam::DVec3)> {
        let bytes = self.fs.read(path)?;
        let info = hp_mdl::read_header(&bytes).ok()?;
        let (lo, hi) = info.bounds();
        let lo = glam::DVec3::new(f64::from(lo[0]), f64::from(lo[1]), f64::from(lo[2]));
        let hi = glam::DVec3::new(f64::from(hi[0]), f64::from(hi[1]), f64::from(hi[2]));
        (lo.cmplt(hi).all()).then_some((lo, hi))
    }

    fn loads(&self, path: &str) -> Option<bool> {
        (!self.fs.sources().is_empty()).then(|| model_header(self.fs, path).is_some())
    }

    fn is_static_prop(&self, path: &str) -> Option<bool> {
        Some(model_header(self.fs, path).is_some_and(|h| h.is_static_prop()))
    }

    fn static_prop_error(&self, path: &str) -> Option<String> {
        let h = model_header(self.fs, path)?;
        hp_bspc::props::static_prop_error(path, "prop_static", h.is_static_prop(), &h.keyvalues, self.allow_dynamic)
    }

    fn hull_center(&self, path: &str) -> Option<glam::DVec3> {
        model_header(self.fs, path).map(|h| hull_center(&h))
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.fs.read(path)
    }

    fn mesh_points(&self, path: &str) -> Option<Vec<Vec<glam::DVec3>>> {
        let mdl = self.fs.read(path)?;
        let vvd = self.fs.read(&hp_mdl::sibling_paths(path).0)?;
        let meshes = hp_mdl::raw_mesh_positions(&mdl, &vvd).ok()?;
        let out: Vec<Vec<glam::DVec3>> = meshes
            .into_iter()
            .filter(|m| !m.is_empty())
            .map(|m| {
                m.into_iter().map(|p| glam::DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))).collect()
            })
            .collect();
        (!out.is_empty()).then_some(out)
    }

    fn model_check(&self, path: &str) -> Option<hp_bspc::props::ModelCheck> {
        if self.fs.sources().is_empty() {
            return None;
        }
        Some(model_check(self.fs, path))
    }
}

fn model_check(fs: &hp_fs::GameFs, path: &str) -> hp_bspc::props::ModelCheck {
    let mut c = hp_bspc::props::ModelCheck::default();
    let Some(mdl) = fs.read(path) else { return c };
    c.exists = true;
    let Ok(h) = hp_mdl::read_header(&mdl) else { return c };
    c.loads = true;
    c.skins = h.num_skin_families;
    c.bodies = body_combinations(&mdl, h.num_body_parts);
    let i32_at = |d: &[u8], o: usize| d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let (vvd_path, vtx_paths) = hp_mdl::sibling_paths(path);
    if let Some(vvd) = fs.read(&vvd_path) {
        c.has_vvd = true;
        c.vvd_matches = i32_at(&vvd, 8) == Some(h.checksum);
        c.vertices = i32_at(&vvd, 16).map_or(0, |v| v.max(0) as u32);
    }
    if let Some(vtx) = vtx_paths.iter().find_map(|p| fs.read(p)) {
        c.has_vtx = true;
        c.vtx_matches = i32_at(&vtx, 16) == Some(h.checksum);
    }
    if let Some(phy) = fs.read(&hp_mdl::phy_path(path)) {
        c.has_phy = true;
        c.phy_solids = i32_at(&phy, 8).map_or(0, |v| v.max(0) as u32);
        c.phy_matches = i32_at(&phy, 12) == Some(h.checksum);
    }
    c
}

fn body_combinations(mdl: &[u8], parts: u32) -> u32 {
    let i32_at = |o: usize| mdl.get(o..o + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let index = i32_at(236).max(0) as usize;
    (0..parts as usize).fold(1u32, |n, k| n.saturating_mul(i32_at(index + k * 16 + 4).max(1) as u32))
}

fn model_header(fs: &hp_fs::GameFs, path: &str) -> Option<hp_mdl::MdlHeaderInfo> {
    hp_mdl::read_header(&fs.read(path)?).ok()
}

fn hull_center(h: &hp_mdl::MdlHeaderInfo) -> glam::DVec3 {
    let (lo, hi) = (h.hull_min, h.hull_max);
    glam::DVec3::new(f64::from(lo[0] + hi[0]), f64::from(lo[1] + hi[1]), f64::from(lo[2] + hi[2])) * 0.5
}

pub(crate) struct LazyModels {
    gmod: Option<PathBuf>,
    game: Option<PathBuf>,
    profile: Option<hp_fs::GameProfile>,
    mount: hp_fs::MountOptions,
    search_paths: Vec<PathBuf>,
    fs: std::sync::OnceLock<Option<hp_fs::GameFs>>,
}

impl hp_bspc::props::ModelSource for LazyModels {
    fn bounds(&self, _path: &str) -> Option<(glam::DVec3, glam::DVec3)> {
        None
    }

    fn hull_center(&self, path: &str) -> Option<glam::DVec3> {
        model_header(self.game_fs()?, path).map(|h| hull_center(&h))
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.game_fs()?.read(path)
    }
}

impl LazyModels {
    pub(crate) fn of_game(game: Option<PathBuf>) -> LazyModels {
        LazyModels {
            gmod: None,
            game,
            profile: None,
            mount: hp_fs::MountOptions::default(),
            search_paths: Vec::new(),
            fs: std::sync::OnceLock::new(),
        }
    }

    fn game_fs(&self) -> Option<&hp_fs::GameFs> {
        self.fs
            .get_or_init(|| {
                let fs = match &self.profile {
                    Some(p) => hp_fs::GameFs::for_game(p, &self.mount).ok()?,
                    None => {
                        let root = self.gmod.clone()?;
                        hp_fs::GameFs::for_gmod(&root, &self.mount).ok()?
                    }
                };
                let own = self.profile.as_ref().map(|p| p.game_dir.clone());
                for g in self.game.iter().filter(|g| Some(*g) != own.as_ref()).chain(&self.search_paths) {
                    let _ = fs.add_game_dir(g);
                }
                Some(fs)
            })
            .as_ref()
    }
}

pub(crate) fn light_files(
    game: Option<&Path>,
    root: Option<&Path>,
    designer: Option<&str>,
    world: Option<&str>,
    map: Option<&Path>,
    log: &mut dyn FnMut(&str),
) -> Vec<hp_bspc::rad::LightFile> {
    let read = |p: &Path| {
        std::fs::read(p).ok().map(|b| hp_bspc::rad::LightFile {
            path: p.display().to_string(),
            text: String::from_utf8_lossy(&b).into_owned(),
        })
    };
    let mut out = Vec::new();
    let mut global = Vec::new();
    if let Some(g) = game {
        global.push(g.join("lights.rad"));
    }
    if let Some(root) = root.or_else(|| game.and_then(Path::parent)) {
        for dir in ["garrysmod", "sourceengine", "garrysmod/download", "bin", "bin/win64"] {
            global.push(root.join(dir).join("lights.rad"));
        }
    }
    match global.iter().find_map(|p| read(p)) {
        Some(f) => out.push(f),
        None => log("Warning: Couldn't open texlight file lights.rad."),
    }
    if let Some(d) = designer {
        let p = Path::new(d);
        let found = read(p).or_else(|| game.filter(|_| p.is_relative()).and_then(|g| read(&g.join(p))));
        match found {
            Some(f) => out.push(f),
            None => log(&format!("Warning: Couldn't open texlight file {d}.")),
        }
    }
    if let Some(w) = world.map(|w| w.trim().replace('\\', "/")).filter(|w| !w.is_empty()) {
        let p = Path::new(&w);
        let mut places: Vec<PathBuf> = Vec::new();
        if p.is_absolute() {
            places.push(p.to_path_buf());
        } else {
            places.extend(game.map(|g| g.join(p)));
            places.extend(map.and_then(Path::parent).map(|d| d.join(p)));
            places.extend(root.or_else(|| game.and_then(Path::parent)).map(|r| r.join(p)));
            places.extend(root.or_else(|| game.and_then(Path::parent)).map(|r| r.join("garrysmod").join(p)));
        }
        match places.iter().find_map(|p| read(p)) {
            Some(f) if out.iter().any(|o| same_file(&o.path, &f.path)) => {
                log(&format!("_lightsrad {w}: the -lights file, read once"))
            }
            Some(f) => {
                log(&format!("Map light file (_lightsrad): {}", f.path));
                out.push(f);
            }
            None => log(&format!("Warning: Couldn't open texlight file {w} (worldspawn _lightsrad).")),
        }
    }
    if let Some(f) = map.and_then(|m| read(&m.with_extension("rad"))) {
        out.push(f);
    }
    out
}

fn same_file(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        let p = std::path::absolute(Path::new(s)).unwrap_or_else(|_| PathBuf::from(s));
        p.to_string_lossy().replace('\\', "/").to_ascii_lowercase()
    };
    norm(a) == norm(b)
}

pub(crate) fn world_lights_rad(bsp: &[u8]) -> Option<String> {
    let file = hp_bsp::Bsp::from_vec(bsp.to_vec()).ok()?;
    let ents = file.entities().ok()?;
    ents.first()?.get("_lightsrad").map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned)
}
