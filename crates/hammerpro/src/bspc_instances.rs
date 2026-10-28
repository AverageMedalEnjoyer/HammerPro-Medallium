use std::path::{Path, PathBuf};
use std::sync::Arc;

use hp_editor::instances::{
    ExpandOptions, InstanceIssue, KeyFixup, LoadRequest, Loaded, candidate_paths, content_path, expand, is_instance,
    normalize_file,
};
use hp_vmf::{Entity, Map, Solid};

const BUILD_DIR: &str = ".hammerpro_build";

pub(crate) struct InstanceFiles<'a> {
    pub(crate) vmf: &'a Path,
    pub(crate) game_dir: Option<&'a Path>,
    pub(crate) fs: Option<&'a hp_fs::GameFs>,
}

pub(crate) fn source_map_path(vmf: &Path) -> PathBuf {
    let dir = vmf.parent();
    let in_build = dir.and_then(Path::file_name).is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(BUILD_DIR));
    match (in_build, dir.and_then(Path::parent), vmf.file_name()) {
        (true, Some(up), Some(name)) => up.join(name),
        _ => vmf.to_path_buf(),
    }
}

fn instance_path(game_dir: Option<&Path>) -> Option<String> {
    let text = std::fs::read_to_string(game_dir?.join("gameinfo.txt")).ok()?;
    let kv = hp_kv::parse(&text).ok()?;
    let info = kv.child("GameInfo").unwrap_or(&kv);
    info.get("InstancePath").map(|p| p.trim().replace('\\', "/")).filter(|p| !p.is_empty())
}

pub(crate) fn load_fgd(gmod_root: Option<&Path>) -> Option<hp_fgd::ResolvedFgd> {
    let path = gmod_root?.join("bin").join("garrysmod.fgd");
    let fgd = hp_fgd::Fgd::load(&path).ok()?;
    Some(fgd.resolve())
}

fn key_fixup(fgd: &hp_fgd::ResolvedFgd, class: &str, key: &str) -> Option<KeyFixup> {
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

fn load(files: &InstanceFiles<'_>, main: &Path, inst_path: Option<&str>, req: &LoadRequest<'_>) -> Loaded {
    let read = |path: PathBuf, text: String| match hp_vmf::parse(&text) {
        Ok(map) => Loaded::Ready { path: Arc::from(path.as_path()), map: Arc::new(map) },
        Err(e) => Loaded::Failed(format!("{}: {e}", path.display())),
    };
    let containing = req.containing.filter(|p| p.is_absolute());
    for c in candidate_paths(req.file, Some(main), containing, files.game_dir) {
        if c.is_file() {
            return match std::fs::read(&c) {
                Ok(bytes) => read(std::path::absolute(&c).unwrap_or(c), String::from_utf8_lossy(&bytes).into_owned()),
                Err(e) => Loaded::Failed(format!("{}: {e}", c.display())),
            };
        }
    }
    let mut virtual_paths: Vec<String> = Vec::new();
    if let (Some(base), Some(f)) = (inst_path, normalize_file(req.file)) {
        let base = if base.ends_with('/') { base.to_owned() } else { format!("{base}/") };
        virtual_paths.push(format!("{base}{f}").to_ascii_lowercase());
    }
    virtual_paths.extend(content_path(req.file));
    for v in virtual_paths {
        if v.contains("..") {
            continue;
        }
        if let Some(dir) = files.game_dir
            && let Ok(bytes) = std::fs::read(dir.join(&v))
        {
            return read(dir.join(&v), String::from_utf8_lossy(&bytes).into_owned());
        }
        if let Some(bytes) = files.fs.and_then(|fs| fs.read(&v)) {
            return read(PathBuf::from(format!("content:/{v}")), String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    Loaded::Missing
}

fn max_ids(map: &Map) -> (u32, u32, u32) {
    let solids = map.world.solids.iter().chain(map.entities.iter().flat_map(|e| e.solids.iter()));
    let (mut brush, mut side) = (0u32, 0u32);
    for s in solids {
        brush = brush.max(s.id);
        for sd in &s.sides {
            side = side.max(sd.id);
        }
    }
    let ent = map.entities.iter().map(|e| e.id).chain(std::iter::once(map.world.id)).max().unwrap_or(0);
    (brush, side, ent)
}

fn renumber(solids: &mut [Solid], entities: &mut [Entity], base: (u32, u32, u32)) -> (u32, u32, u32) {
    let (b0, s0, e0) = base;
    let mut top = base;
    let solid = |s: &mut Solid, top: &mut (u32, u32, u32)| {
        s.id = s.id.saturating_add(b0);
        top.0 = top.0.max(s.id);
        for side in &mut s.sides {
            side.id = side.id.saturating_add(s0);
            top.1 = top.1.max(side.id);
        }
    };
    for s in solids.iter_mut() {
        solid(s, &mut top);
    }
    for e in entities.iter_mut() {
        e.id = e.id.saturating_add(e0);
        top.2 = top.2.max(e.id);
        for s in &mut e.solids {
            solid(s, &mut top);
        }
        for (k, v) in &mut e.props {
            if k.eq_ignore_ascii_case("sides") || k.eq_ignore_ascii_case("sides2") {
                let ids: Vec<String> = v
                    .split_whitespace()
                    .map(|x| x.parse::<u32>().map_or_else(|_| x.to_owned(), |n| n.saturating_add(s0).to_string()))
                    .collect();
                *v = ids.join(" ");
            }
        }
    }
    top
}

pub(crate) fn has_instances(map: &Map) -> bool {
    map.entities.iter().any(is_instance)
}

pub(crate) fn merge_instances(
    map: &mut Map,
    files: &InstanceFiles<'_>,
    gmod_root: Option<&Path>,
    log: &mut dyn FnMut(&str),
) -> usize {
    let fgd = load_fgd(gmod_root);
    if fgd.is_none() {
        log("Could not locate GameData file garrysmod.fgd: instance keys are fixed up by their names");
    }
    collapse(map, files, fgd.as_ref(), log)
}

pub(crate) fn collapse(
    map: &mut Map,
    files: &InstanceFiles<'_>,
    fgd: Option<&hp_fgd::ResolvedFgd>,
    log: &mut dyn FnMut(&str),
) -> usize {
    let roots: Vec<Entity> = map.entities.iter().filter(|e| is_instance(e) && !e.hidden).cloned().collect();
    if roots.is_empty() {
        return 0;
    }
    let main = source_map_path(files.vmf);
    let inst_path = instance_path(files.game_dir);
    let kinds = |c: &str, k: &str| fgd.and_then(|f| key_fixup(f, c, k));
    let opts = ExpandOptions { skip_hidden: true, kinds: Some(&kinds), main_map: Some(&main), first_auto_index: 1 };
    let mut source = |req: &LoadRequest<'_>| load(files, &main, inst_path.as_deref(), req);
    let refs: Vec<&Entity> = roots.iter().collect();
    let expansions = expand(&refs, &mut source, &opts);
    let mut top = max_ids(map);
    let mut merged = 0usize;
    for exp in &expansions {
        for issue in &exp.issues {
            match issue {
                InstanceIssue::NoFile | InstanceIssue::Pending => {}
                InstanceIssue::Missing { file, .. } => log(&format!("Could not open instance file {file}")),
                InstanceIssue::Failed { file, message } => {
                    log(&format!("Could not open instance file {file} ({message})"))
                }
                InstanceIssue::Cycle { file } => {
                    log(&format!("Warning: instance file {file} contains itself: not merged again"))
                }
                InstanceIssue::TooDeep { file } => {
                    log(&format!("Warning: instance file {file} is nested too deep: not merged"))
                }
                InstanceIssue::TooLarge => log("Warning: too many instances: the others are not merged"),
            }
        }
        for piece in &exp.pieces {
            let mut solids = piece.world_solids(true);
            let mut entities = piece.entities(opts.kinds, true);
            top = renumber(&mut solids, &mut entities, top);
            map.world.solids.extend(solids);
            map.entities.extend(entities);
            merged += 1;
        }
    }
    if merged > 0 {
        log(&format!("{merged} instance file(s) merged"));
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hp_bspc_instances_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cube_text(id: u32, side_id: u32, lo: [f64; 3], hi: [f64; 3]) -> String {
        let [x0, y0, z0] = lo;
        let [x1, y1, z1] = hi;
        let planes = [
            format!("({x0} {y1} {z1}) ({x1} {y1} {z1}) ({x1} {y0} {z1})"),
            format!("({x0} {y0} {z0}) ({x1} {y0} {z0}) ({x1} {y1} {z0})"),
            format!("({x0} {y1} {z1}) ({x0} {y0} {z1}) ({x0} {y0} {z0})"),
            format!("({x1} {y1} {z0}) ({x1} {y0} {z0}) ({x1} {y0} {z1})"),
            format!("({x1} {y1} {z1}) ({x0} {y1} {z1}) ({x0} {y1} {z0})"),
            format!("({x1} {y0} {z0}) ({x0} {y0} {z0}) ({x0} {y0} {z1})"),
        ];
        let mut t = format!("\tsolid\n\t{{\n\t\t\"id\" \"{id}\"\n");
        for (k, p) in planes.iter().enumerate() {
            t += &format!(
                "\t\tside\n\t\t{{\n\t\t\t\"id\" \"{}\"\n\t\t\t\"plane\" \"{p}\"\n\t\t\t\"material\" \"DEV/DEV_MEASUREGENERIC01B\"\n\t\t\t\"uaxis\" \"[1 0 0 0] 0.25\"\n\t\t\t\"vaxis\" \"[0 -1 0 0] 0.25\"\n\t\t\t\"lightmapscale\" \"16\"\n\t\t}}\n",
                side_id + k as u32
            );
        }
        t + "\t}\n"
    }

    fn entity_text(id: u32, class: &str, keys: &[(&str, &str)], extra: &str) -> String {
        let mut t = format!("entity\n{{\n\t\"id\" \"{id}\"\n\t\"classname\" \"{class}\"\n");
        for (k, v) in keys {
            t += &format!("\t\"{k}\" \"{v}\"\n");
        }
        t + extra + "}\n"
    }

    #[test]
    fn instances_are_merged_with_names_parameters_transforms_and_nesting() {
        let dir = scratch("merge");
        std::fs::create_dir_all(dir.join("instances")).unwrap();
        let inner = format!(
            "world\n{{\n\t\"id\" \"1\"\n\t\"classname\" \"worldspawn\"\n}}\n{}",
            entity_text(2, "info_target", &[("targetname", "deep"), ("origin", "0 0 0")], "")
        );
        std::fs::write(dir.join("instances").join("inner.vmf"), inner).unwrap();
        let child = format!(
            "world\n{{\n\t\"id\" \"1\"\n\t\"classname\" \"worldspawn\"\n{}}}\n{}{}{}",
            cube_text(2, 1, [0.0, 0.0, 0.0], [32.0, 32.0, 32.0]),
            entity_text(10, "light", &[("targetname", "lamp"), ("_light", "$color 200"), ("origin", "16 0 64")], ""),
            entity_text(
                11,
                "logic_relay",
                &[("targetname", "@global"), ("origin", "0 0 0")],
                "\tconnections\n\t{\n\t\t\"OnTrigger\" \"lamp\x1bTurnOn\x1b\x1b0\x1b-1\"\n\t\t\"OnTrigger\" \"!activator\x1bKill\x1b\x1b0\x1b-1\"\n\t}\n"
            ),
            entity_text(
                12,
                "func_instance",
                &[("targetname", "nest"), ("file", "instances/inner.vmf"), ("origin", "8 0 0")],
                ""
            )
        );
        std::fs::write(dir.join("instances").join("child.vmf"), child).unwrap();
        let main_text = format!(
            "world\n{{\n\t\"id\" \"1\"\n\t\"classname\" \"worldspawn\"\n{}}}\n{}{}{}",
            cube_text(2, 1, [-512.0, -512.0, -16.0], [512.0, 512.0, 0.0]),
            entity_text(
                20,
                "func_instance",
                &[
                    ("targetname", "a"),
                    ("file", "instances/child"),
                    ("origin", "100 0 0"),
                    ("angles", "0 90 0"),
                    ("replace01", "$color 255 0 0")
                ],
                ""
            ),
            entity_text(
                21,
                "func_instance",
                &[("file", "instances/child.vmf"), ("origin", "0 300 0"), ("fixup_style", "1")],
                ""
            ),
            entity_text(
                22,
                "func_instance",
                &[("targetname", "c"), ("file", "instances/missing.vmf"), ("fixup_style", "2")],
                ""
            )
        );
        let vmf = dir.join("main.vmf");
        std::fs::write(&vmf, &main_text).unwrap();
        let mut map = hp_vmf::parse(&main_text).unwrap();
        let mut lines = Vec::new();
        let files = InstanceFiles { vmf: &vmf, game_dir: None, fs: None };
        let n = collapse(&mut map, &files, None, &mut |l| lines.push(l.to_owned()));
        assert_eq!(n, 4, "{lines:?}");
        assert!(lines.iter().any(|l| l == "Could not open instance file instances/missing.vmf"), "{lines:?}");
        assert_eq!(map.world.solids.len(), 3);
        let moved = &map.world.solids[1];
        let xs: Vec<f64> = moved.sides.iter().flat_map(|s| s.plane.iter().map(|p| p.x)).collect();
        assert!(xs.iter().all(|x| (68.0 - 1e-6..=100.0 + 1e-6).contains(x)), "{xs:?}");
        let mut ids: Vec<u32> = map.world.solids.iter().map(|s| s.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), 3);
        let names: Vec<&str> = map.entities.iter().filter_map(|e| e.get("targetname")).collect();
        for n in ["a-lamp", "lamp-InstanceAuto2", "@global", "a-nest-deep", "nest-InstanceAuto2-deep"] {
            assert!(names.contains(&n), "{n} missing from {names:?}");
        }
        let lamp = map.entities.iter().find(|e| e.get("targetname") == Some("a-lamp")).unwrap();
        assert_eq!(lamp.get("_light"), Some("255 0 0 200"));
        let o = hp_editor::instances::parse_vec_lenient(lamp.get("origin").unwrap());
        assert!((o - glam::DVec3::new(100.0, 16.0, 64.0)).length() < 1e-3, "{o}");
        let relay = map.entities.iter().find(|e| e.classname == "logic_relay").unwrap();
        assert_eq!(relay.connections[0].target, "a-lamp");
        assert_eq!(relay.connections[1].target, "!activator");
        let mut eids: Vec<u32> = map.entities.iter().map(|e| e.id).collect();
        eids.sort_unstable();
        eids.dedup();
        assert_eq!(eids.len(), map.entities.len(), "entity ids stay unique");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_folder_snapshots_look_from_the_map_folder() {
        let p = Path::new("C:/maps/x/.hammerpro_build/m.vmf");
        assert_eq!(source_map_path(p), Path::new("C:/maps/x").join("m.vmf"));
        let q = Path::new("C:/maps/x/m.vmf");
        assert_eq!(source_map_path(q), q);
    }
}
