use std::path::{Path, PathBuf};

use glam::DVec3;
use hp_app::AppState;
use hp_app::runtime::instances::{InstanceCache, key_fixup};
use hp_editor::instances::{CollapseOptions, ExpandOptions, FixupStyle, InstanceSpec};
use hp_editor::prefab::{InsertOptions, Prefab, PrefabPlacement};
use hp_editor::{ObjRef, PickOptions, SelectOp};
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, MAX_NAME, R, check_class, edit, obj_json, v3};

const MAX_SPRINKLE: u64 = 2000;
const MAX_PREFABS: usize = 5000;
const MAX_PREFAB_BYTES: u64 = 32 * 1024 * 1024;

domain! { "instances and prefabs";
    list_func_instances: Read, "List instances",
        "The func_instance entities of the map: file, fixup name and style, origin, angles, replace parameters, \
        and whether the file is found.",
        { "limit": limit(), "offset": offset() }, [];
    create_instance: Edit, "Create an instance",
        "Place a func_instance of a VMF file (relative to the map's folder, like \"instances/door.vmf\") with a \
        fixup name, style and replace parameters ($name value).",
        {
            "file": string("Instance file, relative to the map folder."),
            "origin": vec3("Origin."), "angles": vec3("Angles [pitch, yaw, roll]."),
            "targetname": string("Fixup name."),
            "fixup_style": choice("Name fixup (default prefix).", &["prefix", "postfix", "none"]),
            "replaces": keyvalues("Parameters: {\"$color\": \"255 0 0\"}."),
        }, ["file", "origin"];
    set_instance_parameters: Edit, "Edit an instance",
        "Change the file, fixup name, fixup style or the replace parameters (the whole list) of a func_instance.",
        {
            "id": integer("func_instance id.", 1, i64::from(u32::MAX)),
            "file": string("Instance file, relative to the map folder."),
            "targetname": string("Fixup name."),
            "fixup_style": choice("Name fixup.", &["prefix", "postfix", "none"]),
            "replaces": keyvalues("Every parameter: {\"$name\": \"value\"} (replaces the old list)."),
        }, ["id"];
    collapse_instances: Destructive, "Collapse instances",
        "Replace func_instance entities by the contents of their files (fixed up like VBSP), or every instance \
        (all: true). The files load in the background: when they are not loaded yet, call again.",
        { "ids": ids("func_instance ids (default: the selected ones)."), "all": boolean("Every instance of the map.") }, [];
    make_instance: Edit, "Make an instance from objects",
        "Write objects to a new instance file (relative to the map folder) and replace them with a func_instance \
        of it at an origin (default: the center of their bounds, on the grid).",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "file": string("New instance file, relative to the map folder (\"instances/door.vmf\")."),
            "origin": vec3("Origin of the instance."),
            "overwrite": boolean("Replace an existing file (default false)."),
        }, ["file"];
    get_instance_file: Read, "Instance file",
        "What a VMF instance file (relative to the map folder) holds: its parameters (func_instance_parms: name, \
        type, default), entity classes with counts, solids, bounds.",
        { "file": string("Instance file, relative to the map folder.") }, ["file"];
    list_prefabs: Read, "List prefabs",
        "The prefab files of the prefab library (names relative to the library folder), with a name filter.",
        { "filter": string("Name pattern with * and ?."), "limit": limit(), "offset": offset() }, [];
    insert_prefab: Edit, "Insert a prefab",
        "Insert a prefab of the library: at its saved place moved by an offset, at a position (its bounds \
        centered there), or fitted in a box. The &i token gets the next number unless given.",
        {
            "name": string("Prefab file relative to the library (list_prefabs)."),
            "offset": vec3("Offset from where it was saved."),
            "position": vec3("Center of the prefab bounds."),
            "min": vec3("Fit box minimum (with max)."), "max": vec3("Fit box maximum."),
            "token": integer("Value of the &i token.", 0, 1_000_000),
            "group": boolean("Group the inserted objects (default true)."),
        }, ["name"];
    create_prefab: Editor, "Create a prefab",
        "Save objects as a prefab file of the library (written in the background). The objects are selected.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "name": string("Prefab file relative to the library (\"props/bench.vmf\")."),
            "overwrite": boolean("Replace an existing prefab (default false)."),
        }, ["name"];
    sprinkle: Edit, "Sprinkle",
        "Scatter point entities (props, lights...) on the surfaces below an area, like the Sprinkle tool: a list of \
        weighted items, a count, a minimum spacing, random yaw, optional alignment to the surface.",
        {
            "items": {
                "type": "array",
                "minItems": 1,
                "maxItems": 64,
                "items": {
                    "type": "object",
                    "properties": {
                        "classname": { "type": "string" },
                        "keyvalues": { "type": "object", "additionalProperties": { "type": "string" } },
                        "weight": { "type": "number", "minimum": 0.001, "maximum": 1000 },
                    },
                    "required": ["classname"],
                    "additionalProperties": false,
                },
                "description": "Items: classname, keyvalues (model...), weight (default 1).",
            },
            "center": vec3("Area center (circle, with radius)."),
            "radius": number("Area radius.", 1.0, 65536.0),
            "min": vec3("Area box minimum (instead of center and radius)."),
            "max": vec3("Area box maximum."),
            "count": integer("Number of items.", 1, MAX_SPRINKLE as i64),
            "spacing": number("Least distance between two items (default 0).", 0.0, 4096.0),
            "seed": integer("Random seed.", 0, i64::from(u32::MAX)),
            "random_yaw": boolean("Random yaw (default true)."),
            "align": boolean("Turn items to the surface normal (default false)."),
            "max_slope": number("Steepest surface in degrees (default 90).", 0.0, 90.0),
        }, ["items", "count"];
    trace_ray: Read, "Trace a ray",
        "The first solid (and face) or point entity hit by a ray: object, face, point, normal, distance.",
        {
            "origin": vec3("Ray start."),
            "direction": vec3("Ray direction."),
            "max_distance": number("Longest distance (default unlimited).", 1.0, 1_000_000.0),
            "entities": boolean("Hit point entities too (default true)."),
            "hidden": boolean("Hit hidden objects too (default false)."),
        }, ["origin", "direction"];
}

fn is_instance(state: &AppState, id: u32) -> bool {
    state.doc.scene().entity(id).is_some_and(|e| hp_editor::instances::is_instance(&e.entity))
}

fn fixup_value(s: &str) -> &'static str {
    match s {
        "postfix" => "1",
        "none" => "2",
        _ => "0",
    }
}

fn style_name(s: FixupStyle) -> &'static str {
    match s {
        FixupStyle::Prefix => "prefix",
        FixupStyle::Postfix => "postfix",
        FixupStyle::None => "none",
    }
}

fn replaces(a: &Args<'_>) -> R<Option<Vec<(String, String)>>> {
    if a.get("replaces").is_none() {
        return Ok(None);
    }
    let kv = a.keyvalues("replaces")?;
    if kv.len() > 99 {
        return Err("at most 99 replace parameters".to_owned());
    }
    kv.into_iter()
        .enumerate()
        .map(|(i, (k, v))| {
            if !k.starts_with('$') || k.len() < 2 || k.contains(char::is_whitespace) {
                return Err(format!("parameter \"{k}\" must look like $name (no spaces)"));
            }
            Ok((hp_editor::instances::replace_key(i + 1), format!("{k} {v}")))
        })
        .collect::<R<Vec<_>>>()
        .map(Some)
}

fn instance_file(a: &Args<'_>) -> R<Option<String>> {
    a.opt_str("file", 255)?.map(|f| relative_file(f, "vmf")).transpose()
}

fn list_func_instances(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let mut ids: Vec<u32> = state.doc.instance_entities();
    ids.sort_unstable();
    let (ids, total) = page(ids, a)?;
    let main = state.doc.path().map(Path::to_path_buf);
    let list: Vec<Value> = ids
        .into_iter()
        .filter_map(|id| {
            let e = &state.doc.scene().entity(id)?.entity;
            let spec = InstanceSpec::of(e)?;
            let found = hp_editor::instances::candidate_paths(&spec.file, main.as_deref(), None, None)
                .iter()
                .any(|p| p.is_file());
            Some(json!({
                "id": id,
                "file": spec.file,
                "file_found": found,
                "targetname": spec.name,
                "fixup_style": style_name(spec.style),
                "origin": v3(spec.origin),
                "angles": v3(spec.angles),
                "replaces": spec.replaces.iter().map(|(k, v)| json!({ "name": k, "value": v })).collect::<Vec<_>>(),
            }))
        })
        .collect();
    Ok(ToolResult::json(json!({ "total": total, "instances": list, "map_saved": main.is_some() })))
}

fn create_instance(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let file = instance_file(a)?.ok_or("file is required")?;
    let origin = a.vec3("origin")?;
    let mut kv = vec![("file".to_owned(), file.clone())];
    if let Some(angles) = a.opt_vec3("angles")? {
        kv.push(("angles".to_owned(), hp_vmf::fmt_vec3(angles)));
    }
    if let Some(name) = a.opt_str("targetname", MAX_NAME)? {
        super::tools::check_value("targetname", name)?;
        kv.push(("targetname".to_owned(), name.to_owned()));
    }
    if let Some(style) = a.opt_enum("fixup_style", &["prefix", "postfix", "none"])? {
        kv.push(("fixup_style".to_owned(), fixup_value(&style).to_owned()));
    }
    kv.extend(replaces(a)?.unwrap_or_default());
    let id = edit(state, "create_instance", |doc| doc.create_point_entity("func_instance", origin, &kv))?;
    let found = state
        .doc
        .path()
        .is_some_and(|m| hp_editor::instances::candidate_paths(&file, Some(m), None, None).iter().any(|p| p.is_file()));
    let mut out = json!({ "id": id, "file": file, "file_found": found });
    if !found {
        out["warning"] = json!("the file was not found next to the map (it may be created later)");
    }
    Ok(ToolResult::json(out))
}

fn set_instance_parameters(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = a.id("id")?;
    if !is_instance(state, id) {
        return Err(format!("{id} is not a func_instance"));
    }
    let file = instance_file(a)?;
    let name = a.opt_str("targetname", MAX_NAME)?.map(str::to_owned);
    if let Some(n) = &name {
        super::tools::check_value("targetname", n)?;
    }
    let style = a.opt_enum("fixup_style", &["prefix", "postfix", "none"])?;
    let reps = replaces(a)?;
    if file.is_none() && name.is_none() && style.is_none() && reps.is_none() {
        return Err("give file, targetname, fixup_style or replaces".to_owned());
    }
    edit(state, "set_instance_parameters", |doc| {
        doc.edit_entity(id, |e| {
            if let Some(f) = &file {
                e.set("file", f);
            }
            if let Some(n) = &name {
                e.set("targetname", n);
            }
            if let Some(s) = &style {
                e.set("fixup_style", fixup_value(s));
            }
            if let Some(r) = &reps {
                e.props.retain(|(k, _)| !hp_editor::instances::is_replace_key(k));
                for (k, v) in r {
                    e.set(k, v);
                }
            }
        })
    })?;
    Ok(ToolResult::json(json!({ "id": id })))
}

fn collapse_instances(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let all = a.opt_bool("all")?.unwrap_or(false);
    let ids: Vec<u32> = if all {
        if a.get("ids").is_some() {
            return Err("all cannot be combined with ids".to_owned());
        }
        state.doc.instance_entities()
    } else if a.get("ids").is_some() {
        let ids = a.ids("ids")?;
        if let Some(bad) = ids.iter().find(|id| !is_instance(state, **id)) {
            return Err(format!("{bad} is not a func_instance"));
        }
        ids
    } else {
        hp_app::runtime::instances::selected_instances(&state.doc)
    };
    if ids.is_empty() {
        return Err("no func_instance to collapse (give ids, select instances or all: true)".to_owned());
    }
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit; try again".to_owned());
    }
    let cache = InstanceCache::shared();
    let AppState { doc, assets, settings, .. } = state;
    let fgd = assets.fgd();
    let kinds = |c: &str, k: &str| fgd.and_then(|f| key_fixup(f, c, k));
    let skip_hidden = settings.instances.collapse_skips_hidden;
    let main = doc.path().map(Path::to_path_buf);
    let mut source = cache.source(main.as_deref());
    let opts = ExpandOptions { skip_hidden, kinds: Some(&kinds), main_map: main.as_deref(), first_auto_index: 1 };
    let pending =
        doc.expand_instances(&mut source, &opts).iter().filter(|(id, e)| ids.contains(id) && e.is_pending()).count();
    if pending > 0 {
        return Ok(ToolResult::json(json!({
            "collapsed": [],
            "loading": pending,
            "note": "The instance files are loading in the background: call collapse_instances again in a moment.",
        })));
    }
    let report = doc
        .transaction(super::tools::undo_label("collapse_instances"), |doc| {
            doc.collapse_instances(&ids, &mut source, &CollapseOptions { kinds: Some(&kinds), skip_hidden })
        })
        .map_err(|e| e.to_string())?;
    let issues = |l: &[(u32, hp_editor::instances::InstanceIssue)]| -> Vec<Value> {
        l.iter().map(|(id, i)| json!({ "id": id, "issue": format!("{i:?}") })).collect()
    };
    Ok(ToolResult::json(json!({
        "collapsed": report.collapsed,
        "objects": report.objects.len(),
        "skipped": issues(&report.skipped),
        "warnings": issues(&report.warnings),
    })))
}

fn make_instance(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let file = instance_file(a)?.ok_or("file is required")?;
    let dir = map_dir(state)?;
    let path = dir.join(&file);
    let overwrite = a.opt_bool("overwrite")?.unwrap_or(false);
    if path.exists() && !overwrite {
        return Err(format!("{file} exists already (overwrite: true replaces it)"));
    }
    let origin = match a.opt_vec3("origin")? {
        Some(o) => o,
        None => {
            let f = bounds_fn(state);
            let c = state.doc.scene().objects_bounds(&objs, &f).ok_or("the objects have no bounds")?.center();
            let g = grid(state);
            (c / g).round() * g
        }
    };
    let fgd = state.assets.fgd();
    let kinds = |c: &str, k: &str| fgd.and_then(|f| key_fixup(f, c, k));
    let map = state.doc.instance_map(&objs, origin, Some(&kinds)).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("cannot create the folder: {e}"))?;
    }
    let opts = hp_vmf::SaveOptions { backup: false, sync: true };
    hp_vmf::save_with(&map, &path, &opts).map_err(|e| format!("cannot write {file}: {e}"))?;
    let id = edit(state, "make_instance", |doc| doc.replace_with_instance(&objs, &file, origin))?;
    Ok(ToolResult::json(json!({ "id": id, "file": file, "origin": v3(origin) })))
}

fn get_instance_file(state: &mut AppState, a: &Args<'_>) -> R<super::tools::Outcome> {
    let file = instance_file(a)?.ok_or("file is required")?;
    let path = map_dir(state)?.join(&file);
    let meta = std::fs::metadata(&path).map_err(|_| format!("no file {file} in the map folder"))?;
    if meta.len() > MAX_PREFAB_BYTES * 4 {
        return Err(format!("{file} is too large"));
    }
    Ok(super::tools::Worker::spawn("get_instance_file", move || {
        let map = match hp_vmf::load(&path) {
            Ok(m) => m,
            Err(e) => return ToolResult::error(format!("get_instance_file: {file}: {e}")),
        };
        let parms: Vec<Value> = hp_editor::instances::map_parms(&map)
            .into_iter()
            .map(|p| json!({ "name": p.name, "type": p.kind, "default": p.default }))
            .collect();
        let mut classes: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for e in &map.entities {
            *classes.entry(e.classname.to_ascii_lowercase()).or_default() += 1;
        }
        let solids = map.world.solids.len() + map.entities.iter().map(|e| e.solids.len()).sum::<usize>();
        let doc = hp_editor::Document::from_map(map);
        ToolResult::json(json!({
            "file": file,
            "parameters": parms,
            "classes": classes,
            "solids": solids,
            "bounds": super::tools::bounds_json(doc.stats().bounds),
        }))
    }))
}

fn library(state: &AppState) -> PathBuf {
    state.settings.placement.library_dir()
}

fn prefab_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            if out.len() >= MAX_PREFABS {
                return out;
            }
            let p = e.path();
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_dir() && !kind.is_symlink() {
                stack.push(p);
            } else if kind.is_file()
                && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("vmf"))
                && let Ok(rel) = p.strip_prefix(dir)
            {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

fn list_prefabs(state: &mut AppState, a: &Args<'_>) -> R<super::tools::Outcome> {
    let dir = library(state);
    let filter = a.opt_str("filter", MAX_NAME)?.map(str::to_owned);
    let limit = a.opt_u64("limit", 1, hp_mcp::tools::MAX_LIST)?.unwrap_or(100) as usize;
    let offset = a.opt_u64("offset", 0, u64::from(u32::MAX))?.unwrap_or(0) as usize;
    Ok(super::tools::Worker::spawn("list_prefabs", move || {
        let files: Vec<String> = prefab_files(&dir)
            .into_iter()
            .filter(|f| filter.as_deref().is_none_or(|p| hp_editor::glob_match(p, f)))
            .collect();
        let total = files.len();
        let files: Vec<String> = files.into_iter().skip(offset).take(limit).collect();
        ToolResult::json(json!({ "library": dir.display().to_string(), "total": total, "prefabs": files }))
    }))
}

#[derive(Clone, Copy)]
enum Place {
    Offset(DVec3),
    Center(DVec3),
    Fit(hp_geom::Aabb),
}

fn insert_prefab(state: &mut AppState, a: &Args<'_>) -> R<super::tools::Outcome> {
    let name = relative_file(a.str("name", 255)?, "vmf")?;
    let path = library(state).join(&name);
    let meta = std::fs::metadata(&path).map_err(|_| format!("no prefab {name} in the library (list_prefabs)"))?;
    if meta.len() > MAX_PREFAB_BYTES {
        return Err(format!("{name} is larger than {} MB", MAX_PREFAB_BYTES / (1024 * 1024)));
    }
    let place = match (a.opt_vec3("offset")?, a.opt_vec3("position")?, a.opt_vec3("min")?, a.opt_vec3("max")?) {
        (Some(o), None, None, None) => Place::Offset(o),
        (None, Some(p), None, None) => Place::Center(p),
        (None, None, Some(p), Some(q)) => Place::Fit(hp_geom::Aabb::new(p, q)),
        (None, None, None, None) => Place::Offset(DVec3::ZERO),
        _ => return Err("give one of offset, position, or min and max".to_owned()),
    };
    let token = a.opt_u64("token", 0, 1_000_000)?.map(|t| t as u32);
    let group = a.opt_bool("group")?.unwrap_or(true);
    Ok(super::tools::Worker::spawn_then("insert_prefab", move || {
        let prefab = match Prefab::load(&path) {
            Ok(p) => p,
            Err(e) => return (ToolResult::error(format!("insert_prefab: cannot read {name}: {e}")), None),
        };
        let apply: super::tools::ApplyFn = Box::new(move |state: &mut AppState| {
            let placement = match place {
                Place::Offset(o) => PrefabPlacement::Offset(o),
                Place::Center(p) => PrefabPlacement::Offset(p - prefab.bounds().map_or(DVec3::ZERO, |b| b.center())),
                Place::Fit(b) => PrefabPlacement::Fit(b),
            };
            let token = match token {
                Some(t) => Some(t),
                None if prefab.has_token() => {
                    hp_app::runtime::PrefabTokens::take(&mut state.sidecar, &state.doc, &prefab)
                }
                None => None,
            };
            let opts = InsertOptions { token, group };
            Some(match edit(state, "insert_prefab", |doc| doc.insert_prefab(&prefab, placement, &opts)) {
                Ok(r) => ToolResult::json(json!({
                    "objects": r.objects.iter().map(|o| obj_json(*o)).collect::<Vec<_>>(),
                    "group": r.group,
                    "token": r.token,
                })),
                Err(e) => ToolResult::error(format!("insert_prefab: {e}")),
            })
        });
        (ToolResult::error("insert_prefab: not applied"), Some(apply))
    }))
}

fn create_prefab(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let name = relative_file(a.str("name", 255)?, "vmf")?;
    let path = library(state).join(&name);
    let overwrite = a.opt_bool("overwrite")?.unwrap_or(false);
    if path.exists() && !overwrite {
        return Err(format!("the prefab {name} exists already (overwrite: true replaces it)"));
    }
    if state.runtime.placement.create_request.is_some() {
        return Err("another prefab is being written; try again".to_owned());
    }
    state.doc.select(&objs, SelectOp::Replace);
    state.runtime.placement.create_request = Some(hp_app::runtime::CreatePrefabRequest { path, overwrite });
    Ok(ToolResult::json(json!({
        "writing": name,
        "objects": objs.len(),
        "note": "The prefab is written in the background; list_prefabs shows it when done.",
    })))
}

struct Item {
    classname: String,
    keyvalues: Vec<(String, String)>,
    weight: f64,
}

fn sprinkle_items(a: &Args<'_>) -> R<Vec<Item>> {
    let list = a.get("items").and_then(Value::as_array).filter(|l| !l.is_empty() && l.len() <= 64);
    let list = list.ok_or("items must be 1 to 64 objects")?;
    list.iter()
        .map(|v| {
            let ia = super::tools::args(v, &["classname", "keyvalues", "weight"]).map_err(|e| format!("items: {e}"))?;
            let classname = ia.str("classname", MAX_NAME)?.trim().to_owned();
            check_class(&classname)?;
            let keyvalues = ia.keyvalues("keyvalues")?;
            let weight = ia.opt_f64("weight", 0.001, 1000.0)?.unwrap_or(1.0);
            Ok(Item { classname, keyvalues, weight })
        })
        .collect()
}

fn sprinkle(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let items = sprinkle_items(a)?;
    let count = a.opt_u64("count", 1, MAX_SPRINKLE)?.ok_or("count is required")? as usize;
    let (center, radius, area) =
        match (a.opt_vec3("center")?, a.opt_f64("radius", 1.0, 65536.0)?, a.opt_vec3("min")?, a.opt_vec3("max")?) {
            (Some(c), Some(r), None, None) => {
                (c, r, hp_geom::Aabb::new(c - DVec3::new(r, r, 0.0), c + DVec3::new(r, r, 0.0)))
            }
            (None, None, Some(p), Some(q)) => {
                let b = hp_geom::Aabb::new(p, q);
                (b.center(), 0.0, b)
            }
            _ => return Err("give center and radius, or min and max".to_owned()),
        };
    let spacing = a.opt_f64("spacing", 0.0, 4096.0)?.unwrap_or(0.0);
    let seed = a.opt_u64("seed", 0, u64::from(u32::MAX))?.unwrap_or_else(|| state.doc.revision());
    let random_yaw = a.opt_bool("random_yaw")?.unwrap_or(true);
    let align = a.opt_bool("align")?.unwrap_or(false);
    let max_slope = a.opt_f64("max_slope", 0.0, 90.0)?.unwrap_or(90.0);
    let add = scatter(state, &items, count, (center, radius, area), (spacing, seed, random_yaw, align, max_slope))?;
    let ids = edit(state, "sprinkle", |doc| doc.sprinkle(&add, &[]))?;
    Ok(ToolResult::json(json!({ "placed": ids.len(), "ids": ids, "asked": count })))
}

fn scatter(
    state: &AppState,
    items: &[Item],
    count: usize,
    (center, radius, area): (DVec3, f64, hp_geom::Aabb),
    (spacing, seed, random_yaw, align, max_slope): (f64, u64, bool, bool, f64),
) -> R<Vec<hp_editor::prefab::SprinkleItem>> {
    let min_dot = max_slope.to_radians().cos() - 1e-9;
    let f = bounds_fn(state);
    let top = state
        .doc
        .scene()
        .objects_bounds(&state.doc.scene().solids().keys().map(|id| ObjRef::Solid(*id)).collect::<Vec<_>>(), &f)
        .map_or(area.max.z, |b| b.max.z.max(area.max.z))
        + 1.0;
    let total_weight: f64 = items.iter().map(|i| i.weight).sum();
    let mut rng = Rng(seed);
    let mut placed: Vec<DVec3> = Vec::new();
    let mut add = Vec::new();
    let opts = PickOptions { include_entities: false, ..Default::default() };
    let attempts = count * 30;
    for _ in 0..attempts {
        if add.len() >= count {
            break;
        }
        let (x, y) = if radius > 0.0 {
            let (r, t) = (radius * rng.next().sqrt(), rng.next() * std::f64::consts::TAU);
            (center.x + r * t.cos(), center.y + r * t.sin())
        } else {
            (area.min.x + rng.next() * (area.max.x - area.min.x), area.min.y + rng.next() * (area.max.y - area.min.y))
        };
        let ray = hp_geom::Ray::new(DVec3::new(x, y, top), DVec3::NEG_Z);
        let Some(hit) = state.doc.pick_ray(&ray, &opts, &f) else { continue };
        if hit.point.z < area.min.z - 1e-6 && radius == 0.0 {
            continue;
        }
        if hit.normal.z < min_dot {
            continue;
        }
        if spacing > 0.0 && placed.iter().any(|p| p.distance(hit.point) < spacing) {
            continue;
        }
        let mut pick = rng.next() * total_weight;
        let item = items.iter().find(|i| {
            pick -= i.weight;
            pick < 0.0
        });
        let item = item.unwrap_or(&items[items.len() - 1]);
        let yaw = if random_yaw { (rng.next() * 360.0).floor() } else { 0.0 };
        let angles = if align {
            let n = hit.normal;
            let pitch = -n.x.atan2(n.z).to_degrees();
            let roll = n.y.atan2(n.z).to_degrees();
            DVec3::new(pitch, yaw, roll)
        } else {
            DVec3::new(0.0, yaw, 0.0)
        };
        let mut kv = item.keyvalues.clone();
        if !kv.iter().any(|(k, _)| k.eq_ignore_ascii_case("angles")) {
            kv.push(("angles".to_owned(), hp_vmf::fmt_vec3(angles)));
        }
        placed.push(hit.point);
        add.push(hp_editor::prefab::SprinkleItem {
            classname: item.classname.clone(),
            origin: hit.point,
            keyvalues: kv,
        });
    }
    if add.is_empty() {
        return Err("no surface was found below the area (or none flat enough)".to_owned());
    }
    Ok(add)
}

fn trace_ray(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let origin = a.vec3("origin")?;
    let dir = opt_dir(a, "direction")?.ok_or("direction is required")?;
    let opts = PickOptions {
        include_entities: a.opt_bool("entities")?.unwrap_or(true),
        ignore_hidden: !a.opt_bool("hidden")?.unwrap_or(false),
        max_distance: a.opt_f64("max_distance", 1.0, 1_000_000.0)?.unwrap_or(f64::INFINITY),
        ..Default::default()
    };
    let f = bounds_fn(state);
    let ray = hp_geom::Ray::new(origin, dir);
    let hit = state.doc.pick_ray(&ray, &opts, &f);
    Ok(ToolResult::json(match hit {
        None => json!({ "hit": false }),
        Some(h) => json!({
            "hit": true,
            "object": obj_json(h.obj),
            "face": h.face.and_then(|side| match h.obj {
                ObjRef::Solid(id) => Some(json!({ "solid": id, "side": side })),
                ObjRef::Entity(_) => None,
            }),
            "point": v3(h.point),
            "normal": v3(h.normal),
            "distance": (h.t * 1000.0).round() / 1000.0,
        }),
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn instances_without_files() {
        let mut s = hp_app::testing::test_state();
        let r = ok(
            &mut s,
            "create_instance",
            json!({ "file": "instances\\door", "origin": [0, 0, 0], "targetname": "d1", "fixup_style": "postfix", "replaces": { "$color": "255 0 0" } }),
        );
        let id = r["id"].as_u64().unwrap();
        assert_eq!(r["file"], "instances/door.vmf");
        let l = ok(&mut s, "list_func_instances", json!({}));
        assert_eq!(l["total"], 1);
        assert_eq!(l["instances"][0]["fixup_style"], "postfix");
        assert_eq!(l["instances"][0]["replaces"][0]["name"], "$color");
        ok(
            &mut s,
            "set_instance_parameters",
            json!({ "id": id, "replaces": { "$a": "1", "$b": "2" }, "fixup_style": "none" }),
        );
        let l = ok(&mut s, "list_func_instances", json!({}));
        assert_eq!(l["instances"][0]["replaces"].as_array().unwrap().len(), 2);
        err(&mut s, "set_instance_parameters", json!({ "id": 5, "fixup_style": "none" }));
        err(&mut s, "create_instance", json!({ "file": "../evil.vmf", "origin": [0, 0, 0] }));
        err(&mut s, "create_instance", json!({ "file": "C:/evil.vmf", "origin": [0, 0, 0] }));
        err(&mut s, "create_instance", json!({ "file": "a.vmf", "origin": [0, 0, 0], "replaces": { "color": "x" } }));
        err(&mut s, "make_instance", json!({ "solid_ids": [3], "file": "instances/wall.vmf" }));
        err(&mut s, "collapse_instances", json!({ "ids": [4] }));
    }

    #[test]
    fn make_instance_and_collapse_it() {
        let dir = std::env::temp_dir().join(format!("hp_mcp_inst_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = hp_app::testing::test_state();
        s.doc.set_path(Some(dir.join("main.vmf")));
        let r =
            ok(&mut s, "make_instance", json!({ "solid_ids": [3], "file": "instances/wall", "origin": [0, 256, 0] }));
        assert!(dir.join("instances/wall.vmf").is_file());
        let info = ok(&mut s, "get_instance_file", json!({ "file": "instances/wall" }));
        assert_eq!(info["solids"], 1);
        let id = r["id"].as_u64().unwrap();
        assert!(s.doc.scene().solid(3).is_none());
        err(&mut s, "make_instance", json!({ "solid_ids": [2], "file": "instances/wall" }));
        let mut collapsed = false;
        for _ in 0..200 {
            let r = ok(&mut s, "collapse_instances", json!({ "ids": [id] }));
            if !r["collapsed"].as_array().unwrap().is_empty() {
                collapsed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(collapsed);
        assert_eq!(s.doc.undo_label(), Some("MCP: collapse_instances"));
        assert_eq!(s.doc.instance_entities().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prefabs_sprinkle_and_rays() {
        let dir = std::env::temp_dir().join(format!("hp_mcp_prefabs_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("props")).unwrap();
        let mut s = hp_app::testing::test_state();
        s.settings.placement.prefab_library = Some(dir.clone());
        let text = s.doc.to_vmf_string();
        std::fs::write(dir.join("props/room.vmf"), text).unwrap();
        let l = ok(&mut s, "list_prefabs", json!({ "filter": "props/*" }));
        assert_eq!(l["prefabs"], json!(["props/room.vmf"]));
        let n = s.doc.stats().solids;
        let r = ok(&mut s, "insert_prefab", json!({ "name": "props/room", "offset": [0, 0, 1024] }));
        assert!(r["group"].as_u64().is_some());
        assert_eq!(s.doc.stats().solids, n * 2);
        err(&mut s, "insert_prefab", json!({ "name": "props/none" }));
        err(&mut s, "insert_prefab", json!({ "name": "props/room", "offset": [0, 0, 0], "position": [0, 0, 0] }));
        ok(&mut s, "create_prefab", json!({ "solid_ids": [3], "name": "walls/w1" }));
        assert!(s.runtime.placement.create_request.is_some());
        err(&mut s, "create_prefab", json!({ "solid_ids": [3], "name": "props/room" }));

        let r = ok(&mut s, "trace_ray", json!({ "origin": [0, 0, 100], "direction": [0, 0, -1] }));
        assert_eq!(r["hit"], true);
        assert_eq!(r["point"], json!([0.0, 0.0, 0.0]));
        assert_eq!(r["normal"], json!([0.0, 0.0, 1.0]));
        let r = ok(&mut s, "trace_ray", json!({ "origin": [0, 0, 100], "direction": [0, 0, 1], "max_distance": 10 }));
        assert_eq!(r["hit"], false);

        let r = ok(
            &mut s,
            "sprinkle",
            json!({
                "items": [{ "classname": "prop_static", "keyvalues": { "model": "models/props_c17/oildrum001.mdl" }, "weight": 2 }, { "classname": "info_target" }],
                "center": [0, 0, 0], "radius": 200, "count": 10, "spacing": 16, "seed": 3,
            }),
        );
        assert_eq!(r["placed"], 10);
        assert_eq!(s.doc.undo_label(), Some("MCP: sprinkle"));
        err(
            &mut s,
            "sprinkle",
            json!({ "items": [{ "classname": "bad class" }], "center": [0, 0, 0], "radius": 10, "count": 1 }),
        );
        err(
            &mut s,
            "sprinkle",
            json!({ "items": [{ "classname": "info_target" }], "center": [99999, 0, 0], "radius": 10, "count": 1 }),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
