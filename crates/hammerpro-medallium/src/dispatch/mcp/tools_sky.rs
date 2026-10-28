use hp_app::AppState;
use hp_editor::sky;
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, MAX_NAME, R, edit, v3};

const MAX_CAMERAS: usize = 64;

domain! { "skybox";
    list_sky_cameras: Read, "Sky cameras",
        "The sky_camera entities of the map in file order: id, name, origin, scale (and whether the engine can use \
        it), whether the game uses it (the last one of the map does), whether the 3D skybox preview shows it, and \
        its skybox room (bounds and how many solids and entities are inside; null when the room is open).",
        {}, [];
    preview_sky_camera: Editor, "Preview a sky camera",
        "Choose the sky camera whose 3D skybox the 3D views show around the map (without id: the one the game \
        uses), and show or hide the 3D skybox preview.",
        {
            "id": integer("Sky camera entity id (default: the one the game uses).", 1, i64::from(u32::MAX)),
            "show": boolean("Show the 3D skybox preview (default true)."),
        }, [];
    use_sky_camera_in_game: Edit, "Use a sky camera in game",
        "Make this sky camera the one the game uses: it moves after the other entities of the map (the game shows \
        the 3D skybox of the last sky_camera). One undo step.",
        { "id": integer("Sky camera entity id.", 1, i64::from(u32::MAX)) }, ["id"];
    create_skybox_switch: Edit, "Create a skybox switch",
        "Place a skybox switch (point_hammerpro_skybox) that shows the 3D skybox of this sky camera in game when \
        it gets the Activate input (Deactivate shows the map's own skybox again, Toggle switches). The sky camera \
        gets a name when it has none. Wire it with add_connection (for example a trigger's OnStartTouch to \
        <switch name>,Activate). Hammer Pro ships the switch's Lua with the map (development addon of test \
        compiles, Build and save as content). One undo step.",
        {
            "sky_camera": integer("Sky camera entity id.", 1, i64::from(u32::MAX)),
            "name": string("Name of the switch (default: the camera's name with _switch)."),
            "origin": vec3("Position of the switch (default: next to the sky camera)."),
            "per_player": boolean("Per player mode: only the player who fires Activate sees this 3D skybox, drawn on their client (several skyboxes at once). Default false (global, the same for everyone)."),
            "start_active": boolean("Show this skybox when the map starts (global mode only, default false)."),
        }, ["sky_camera"];
}

fn camera_json(state: &AppState, c: &sky::SkyCamera, game: Option<u32>, previewed: Option<u32>) -> Value {
    let room = sky::find_sky_room(&state.doc, c.origin).map(|r| {
        json!({
            "min": v3(r.bounds.min),
            "max": v3(r.bounds.max),
            "solids": r.solids.len(),
            "entities": r.entities.len(),
        })
    });
    json!({
        "id": c.id,
        "name": c.name,
        "origin": v3(c.origin),
        "scale": c.scale,
        "scale_key": c.scale_text,
        "scale_valid": c.scale_valid,
        "in_game": game == Some(c.id),
        "previewed": previewed == Some(c.id),
        "room": room,
    })
}

fn list_sky_cameras(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let cams = sky::sky_cameras(&state.doc);
    let game = sky::game_sky_camera(&state.doc);
    let previewed = hp_app::runtime::render::previewed_sky_camera(state).or(game);
    let list: Vec<Value> = cams.iter().take(MAX_CAMERAS).map(|c| camera_json(state, c, game, previewed)).collect();
    let switches: Vec<Value> = state
        .doc
        .scene()
        .entities()
        .values()
        .filter(|n| sky::is_skybox_switch(&n.entity))
        .take(MAX_CAMERAS)
        .map(|n| {
            let e = &n.entity;
            let target = sky::switch_target(e);
            let camera = cams.iter().find(|c| !target.is_empty() && c.name.eq_ignore_ascii_case(target)).map(|c| c.id);
            json!({
                "id": e.id,
                "name": e.get("targetname").unwrap_or_default(),
                "sky_camera_name": target,
                "sky_camera": camera,
                "per_player": e.get("mode").and_then(|m| m.trim().parse::<i32>().ok()) == Some(1),
                "start_active": e.get("spawnflags").and_then(|f| f.trim().parse::<u32>().ok()).is_some_and(|f| f & 1 != 0),
            })
        })
        .collect();
    Ok(ToolResult::json(json!({
        "total": cams.len(),
        "in_game": game,
        "previewed": previewed,
        "preview_shown": state.settings.render.skybox_3d,
        "sky_cameras": list,
        "skybox_switches": switches,
        "note": "The game shows one 3D skybox at a time: the last sky_camera of the map, until a skybox switch \
            (point_hammerpro_skybox) gets Activate.",
    })))
}

fn sky_camera_id(state: &AppState, a: &Args<'_>) -> R<u32> {
    let id = a.id("id")?;
    match state.doc.scene().entity(id) {
        Some(n) if sky::is_sky_camera(&n.entity) => Ok(id),
        Some(n) => Err(format!("entity {id} is a {}, not a sky_camera", n.entity.classname)),
        None => Err(format!("no entity {id}")),
    }
}

fn preview_sky_camera(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = match a.get("id") {
        Some(_) => Some(sky_camera_id(state, a)?),
        None => None,
    };
    let show = a.opt_bool("show")?.unwrap_or(true);
    state.runtime.render.sky_camera = id;
    if state.settings.render.skybox_3d != show {
        state.settings.render.skybox_3d = show;
        state.settings_dirty = true;
    }
    let game = sky::game_sky_camera(&state.doc);
    Ok(ToolResult::json(json!({ "previewed": id.or(game), "follows_game": id.is_none(), "preview_shown": show })))
}

fn use_sky_camera_in_game(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = sky_camera_id(state, a)?;
    let changed = edit(state, "use_sky_camera_in_game", |doc| doc.make_sky_camera_active(id))?;
    Ok(ToolResult::json(json!({ "in_game": id, "changed": changed })))
}

fn create_skybox_switch(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if !state.settings.game.is_gmod() {
        return Err(format!(
            "the skybox switch is a Lua entity of Garry's Mod; this map is for {}",
            state.settings.game.active_game()
        ));
    }
    let id = a.id("sky_camera")?;
    let cam = match state.doc.scene().entity(id) {
        Some(n) => {
            sky::sky_camera_of(&n.entity).ok_or_else(|| format!("entity {id} is not a sky_camera with a position"))?
        }
        None => return Err(format!("no entity {id}")),
    };
    let name = a.opt_str("name", MAX_NAME)?.map(str::trim).filter(|n| !n.is_empty()).map(str::to_owned);
    if name.as_deref().is_some_and(|n| n.chars().any(|c| c.is_control() || c == '"' || c == ',')) {
        return Err("name holds a forbidden character".to_owned());
    }
    let origin = a.opt_vec3("origin")?.unwrap_or(cam.origin + glam::DVec3::new(0.0, 0.0, 32.0));
    let per_player = a.opt_bool("per_player")?.unwrap_or(false);
    let start = a.opt_bool("start_active")?.unwrap_or(false);
    let cam_name = if cam.name.is_empty() { format!("sky_camera_{id}") } else { cam.name.clone() };
    let switch_name = name.unwrap_or_else(|| format!("{cam_name}_switch"));
    let (n1, n2) = (cam_name.clone(), switch_name.clone());
    let created = edit(state, "create_skybox_switch", move |doc| {
        if cam.name.is_empty() {
            doc.set_entity_key(&[id], "targetname", &n1)?;
        }
        let mut keys = vec![("targetname".to_owned(), n2), ("skycamera".to_owned(), n1)];
        if per_player {
            keys.push(("mode".to_owned(), "1".to_owned()));
        }
        if start && !per_player {
            keys.push(("spawnflags".to_owned(), "1".to_owned()));
        }
        doc.create_point_entity(sky::SWITCH_CLASS, origin, &keys)
    })?;
    Ok(ToolResult::json(json!({
        "switch": created,
        "name": switch_name,
        "sky_camera_name": cam_name,
        "mode": if per_player { "per_player" } else { "global" },
        "inputs": ["Activate", "Deactivate", "Toggle"],
        "outputs": ["OnActivated", "OnDeactivated"],
    })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn several_sky_cameras() {
        let mut s = hp_app::testing::test_state();
        let kv = |k: &str, v: &str| (k.to_owned(), v.to_owned());
        let a = s.doc.create_point_entity("sky_camera", glam::DVec3::new(0.0, 0.0, 4000.0), &[kv("targetname", "day")]);
        let b = s.doc.create_point_entity("sky_camera", glam::DVec3::new(0.0, 0.0, 6000.0), &[kv("scale", "0")]);
        let (a, b) = (a.expect("day"), b.expect("night"));
        let l = ok(&mut s, "list_sky_cameras", json!({}));
        assert_eq!(l["total"], 2);
        assert_eq!(l["in_game"], b);
        assert_eq!(l["sky_cameras"][0]["name"], "day");
        assert_eq!(l["sky_cameras"][1]["scale_valid"], false);
        ok(&mut s, "preview_sky_camera", json!({ "id": a }));
        assert_eq!(s.runtime.render.sky_camera, Some(a));
        assert!(s.settings.render.skybox_3d);
        let u = ok(&mut s, "use_sky_camera_in_game", json!({ "id": a }));
        assert_eq!(u["changed"], true);
        assert_eq!(hp_editor::sky::game_sky_camera(&s.doc), Some(a));
        assert_eq!(s.doc.undo_label(), Some("MCP: use_sky_camera_in_game"));
        assert_eq!(ok(&mut s, "use_sky_camera_in_game", json!({ "id": a }))["changed"], false);
        let floor = s.doc.scene().solids().keys().next().copied().expect("a solid");
        err(&mut s, "use_sky_camera_in_game", json!({ "id": 999_999 }));
        err(&mut s, "preview_sky_camera", json!({ "id": floor }));
        let sw = ok(&mut s, "create_skybox_switch", json!({ "sky_camera": b, "start_active": true }));
        assert_eq!(sw["sky_camera_name"], format!("sky_camera_{b}"));
        assert_eq!(s.doc.undo_label(), Some("MCP: create_skybox_switch"));
        let l = ok(&mut s, "list_sky_cameras", json!({}));
        assert_eq!(l["skybox_switches"][0]["sky_camera"], b);
        assert_eq!(l["skybox_switches"][0]["start_active"], true);
        assert_eq!(l["skybox_switches"][0]["per_player"], false);
        let pp = ok(&mut s, "create_skybox_switch", json!({ "sky_camera": a, "per_player": true, "name": "zone_a" }));
        assert_eq!(pp["mode"], "per_player");
        let e =
            s.doc.scene().entities().values().find(|n| n.entity.get("targetname") == Some("zone_a")).expect("switch");
        assert_eq!(e.entity.get("mode"), Some("1"));
        err(&mut s, "create_skybox_switch", json!({ "sky_camera": b, "name": "bad,name" }));
        err(&mut s, "create_skybox_switch", json!({ "sky_camera": 999_999 }));
        ok(&mut s, "preview_sky_camera", json!({ "show": false }));
        assert_eq!(s.runtime.render.sky_camera, None);
        assert!(!s.settings.render.skybox_3d);
    }
}
