use hp_app::AppState;
use hp_editor::{Document, ObjRef};
use hp_mcp::protocol::ToolResult;
use hp_tools_create::path::{self as paths, ChainSpec, PathGraph};
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, MAX_NAME, R, check_class, undo_label, v3};

const MAX_POINTS: usize = 4096;
const MAX_CHAINS: usize = 500;

domain! { "paths";
    list_paths: Read, "Path chains",
        "The chains of path entities of the map (path_track, path_corner, path_corner_crash, keyframe_rope, \
        scripted_sequence... any class whose next key names the next point): for each chain its points in walking \
        order (id, name, class, origin, next key and value, branch), whether it is closed into a loop, and the \
        classes the path tool can place with their next key. A chain starts at a point nothing links to (a loop \
        at its lowest id).",
        { "limit": integer("Most chains listed (default 100).", 1, MAX_CHAINS as i64) }, [];
    create_path: Edit, "Create a path",
        "Create a chain of path entities through a list of points, like clicking them with the path tool: each \
        point is named from the base name (\"path\" gives path_1, path_2..., numbers already used in the map are \
        skipped) and names the next one in its next key (target for path_track and path_corner, NextKey for rope \
        keyframes...). With loop the last point links back to the first (a closed track for a func_tracktrain). \
        The points get the FGD defaults of the class, then keyvalues. One undo step; the new points become the \
        selection. Point a func_tracktrain's target at the first name to make it follow the path.",
        {
            "points": { "type": "array", "items": vec3("Point (world units)."), "minItems": 1, "maxItems": MAX_POINTS, "description": "Positions in path order." },
            "class": string("Class of the points (default: the path tool's class, path_corner unless changed)."),
            "base_name": string("Base name of the points (default: the path tool's, \"path\")."),
            "loop": boolean("Link the last point to the first (default false)."),
            "keyvalues": keyvalues("Keyvalues of every point (speed, wait, spawnflags...); the name and next key are set by the tool."),
        }, ["points"];
    insert_path_point: Edit, "Insert a path point",
        "Insert a point after a path point: it takes that point's next point, which now links to it (the path \
        tool's Shift+drag). Default position: halfway to the next point (one segment further at the end of a \
        chain). When the points after it are numbered in sequence (path_3, path_4... after path_2) the new point \
        takes the next number and they shift by one, with every key and output naming them. One undo step.",
        {
            "after": integer("Id of the path point the new one follows.", 1, i64::from(u32::MAX)),
            "position": vec3("Position of the new point (default: halfway to the next point)."),
        }, ["after"];
    remove_path_points: Edit, "Remove path points",
        "Delete path points. With relink (default true) the point before each removed one links to the point after \
        it (and a train starting on it starts on the next one), so the chain stays connected; at the end of a \
        chain the link is cleared. One undo step.",
        {
            "ids": ids("Path point entity ids (default: the selection)."),
            "relink": boolean("Link the neighbours of each removed point (default true)."),
        }, [];
}

fn path_edit<T>(
    state: &mut AppState,
    tool: &str,
    f: impl FnOnce(&mut Document, Option<&hp_fgd::ResolvedFgd>) -> hp_editor::EditResult<T>,
) -> R<T> {
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    let fgd = state.assets.fgd();
    state.doc.transaction(undo_label(tool), |doc| f(doc, fgd)).map_err(|e| e.to_string())
}

fn point_json(n: &paths::PathNode) -> Value {
    json!({
        "id": n.id,
        "name": n.name,
        "class": n.class,
        "origin": v3(n.origin),
        "next_key": n.next_key,
        "next": if n.next_name.is_empty() { Value::Null } else { json!(n.next_name) },
        "next_id": n.next,
        "branch_id": n.alt,
    })
}

fn list_paths(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let limit = a.opt_u64("limit", 1, MAX_CHAINS as u64)?.unwrap_or(100) as usize;
    let graph = PathGraph::build(&state.doc, state.assets.fgd());
    let chains = graph.chains();
    let list: Vec<Value> = chains
        .iter()
        .take(limit)
        .map(|c| {
            let points: Vec<Value> = c.nodes.iter().filter_map(|id| graph.node(*id)).map(point_json).collect();
            json!({ "closed": c.closed, "count": c.nodes.len(), "points": points })
        })
        .collect();
    let classes: Vec<Value> = paths::path_classes(state.assets.fgd())
        .into_iter()
        .map(|c| json!({ "class": c.class, "next_key": c.next_key, "branch_key": c.alt_key }))
        .collect();
    Ok(ToolResult::json(json!({
        "total": chains.len(),
        "points": graph.nodes.len(),
        "chains": list,
        "classes": classes,
        "tool_class": state.settings.entities.path.class_name(),
        "tool_base_name": state.settings.entities.path.base(),
    })))
}

fn points_arg(a: &Args<'_>) -> R<Vec<glam::DVec3>> {
    let list = a.get("points").and_then(Value::as_array).filter(|l| (1..=MAX_POINTS).contains(&l.len()));
    let list = list.ok_or_else(|| format!("points must be 1 to {MAX_POINTS} [x, y, z] points"))?;
    list.iter()
        .map(|v| {
            let c: Vec<f64> = v
                .as_array()
                .filter(|x| x.len() == 3)
                .into_iter()
                .flatten()
                .filter_map(Value::as_f64)
                .filter(|f| f.is_finite() && f.abs() <= 131_072.0)
                .collect();
            match c[..] {
                [x, y, z] => Ok(glam::DVec3::new(x, y, z)),
                _ => Err("points must contain [x, y, z] points with numbers from -131072 to 131072".to_owned()),
            }
        })
        .collect()
}

fn create_path(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let points = points_arg(a)?;
    let class_name = match a.opt_str("class", MAX_NAME)? {
        Some(c) => {
            check_class(c.trim())?;
            c.trim().to_owned()
        }
        None => state.settings.entities.path.class_name().to_owned(),
    };
    let class = paths::path_class(state.assets.fgd(), &class_name).ok_or_else(|| {
        let known: Vec<String> = paths::path_classes(state.assets.fgd()).into_iter().map(|c| c.class).collect();
        format!("{class_name} does not link to a next point (path classes: {})", known.join(", "))
    })?;
    let base = match a.opt_str("base_name", 120)? {
        Some(b) if !b.trim().is_empty() => b.trim().to_owned(),
        Some(_) => return Err("base_name is empty".to_owned()),
        None => state.settings.entities.path.base().to_owned(),
    };
    let closed = a.opt_bool("loop")?.unwrap_or(false);
    let mut kv = paths::class_defaults(state.assets.fgd(), &class.class);
    for (k, v) in a.keyvalues("keyvalues")? {
        if ["targetname", "origin", "classname"].iter().any(|r| k.eq_ignore_ascii_case(r))
            || k.eq_ignore_ascii_case(&class.next_key)
        {
            return Err(format!("keyvalues: {k} is set by the tool"));
        }
        match kv.iter_mut().find(|(x, _)| x.eq_ignore_ascii_case(&k)) {
            Some(slot) => slot.1 = v,
            None => kv.push((k, v)),
        }
    }
    let spec = ChainSpec { class: &class, base: &base, points: &points, closed, keyvalues: &kv };
    let created = path_edit(state, "create_path", |doc, _| paths::create_chain(doc, &spec))?;
    let names: Vec<&str> = created.iter().map(|(_, n)| n.as_str()).collect();
    Ok(ToolResult::json(json!({
        "ids": created.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        "names": names,
        "class": class.class,
        "next_key": class.next_key,
        "closed": closed && created.len() >= 2,
        "first": names.first(),
    })))
}

fn insert_path_point(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let after = entity_id(&state.doc, a, "after")?;
    let position = a.opt_vec3("position")?;
    let base = state.settings.entities.path.base().to_owned();
    let ins = path_edit(state, "insert_path_point", |doc, fgd| paths::insert_after(doc, fgd, after, position, &base))?;
    let renamed: Vec<Value> =
        ins.renamed.iter().map(|(id, old, new)| json!({ "id": id, "old": old, "new": new })).collect();
    Ok(ToolResult::json(json!({ "id": ins.id, "name": ins.name, "origin": v3(ins.origin), "renamed": renamed })))
}

fn remove_path_points(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let ids = target_entities(state, a, "ids")?;
    let relink = a.opt_bool("relink")?.unwrap_or(true);
    let graph = PathGraph::build(&state.doc, state.assets.fgd());
    if let Some(id) = ids.iter().find(|id| !graph.contains(**id)) {
        return Err(format!("entity {id} is not a path point"));
    }
    let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Entity(*id)).collect();
    let removed = path_edit(state, "remove_path_points", |doc, fgd| paths::remove_points(doc, fgd, &objs, relink))?;
    Ok(ToolResult::json(json!({ "removed": removed, "relinked": relink })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn create_insert_remove_and_list() {
        let mut s = hp_app::testing::test_state();
        let c = ok(
            &mut s,
            "create_path",
            json!({ "points": [[0, 0, 8], [256, 0, 8], [256, 256, 8], [0, 256, 8]], "class": "path_track",
                    "base_name": "loop", "loop": true, "keyvalues": { "speed": "100" } }),
        );
        assert_eq!(c["names"], json!(["loop_1", "loop_2", "loop_3", "loop_4"]));
        assert_eq!(c["closed"], true);
        assert_eq!(s.doc.undo_label(), Some("MCP: create_path"));
        let l = ok(&mut s, "list_paths", json!({}));
        assert_eq!(l["total"], 1);
        assert_eq!(l["chains"][0]["closed"], true);
        assert_eq!(l["chains"][0]["points"][3]["next"], "loop_1");
        assert_eq!(l["chains"][0]["points"][0]["class"], "path_track");
        let first = c["ids"][0].as_u64().unwrap();
        let e = &s.doc.scene().entity(first as u32).unwrap().entity;
        assert_eq!(e.get("speed"), Some("100"));
        let i = ok(&mut s, "insert_path_point", json!({ "after": first }));
        assert_eq!(i["name"], "loop_2");
        assert_eq!(i["origin"], json!([128.0, 0.0, 8.0]));
        assert_eq!(i["renamed"].as_array().unwrap().len(), 3);
        assert_eq!(s.doc.undo_label(), Some("MCP: insert_path_point"));
        let l = ok(&mut s, "list_paths", json!({}));
        assert_eq!(l["chains"][0]["count"], 5);
        let r = ok(&mut s, "remove_path_points", json!({ "ids": [i["id"]] }));
        assert_eq!(r["removed"], 1);
        let l = ok(&mut s, "list_paths", json!({}));
        assert_eq!(l["chains"][0]["points"][0]["next"], "loop_3");
        assert_eq!(l["chains"][0]["closed"], true);
        err(&mut s, "create_path", json!({ "points": [[0, 0, 0]], "class": "light" }));
        err(&mut s, "create_path", json!({ "points": [[0, 0, 0]], "keyvalues": { "target": "x" } }));
        err(&mut s, "create_path", json!({ "points": [] }));
        err(&mut s, "insert_path_point", json!({ "after": 999_999 }));
        let floor = s.doc.scene().solids().keys().next().copied().unwrap();
        err(&mut s, "remove_path_points", json!({ "ids": [floor] }));
    }
}
