use hp_app::AppState;
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, R};

const MAX_ID: usize = 260;

domain! { "games";
    list_games: Read, "List the games",
        "The Source games Hammer Pro compiles maps for: those found on this computer first (Steam libraries and \
        sourcemods, with their folders), then the others (maps still compile in their format). Each with its id, \
        engine branch, BSP version, static prop format, limits, FGD files and executable.",
        { "installed_only": boolean("Only the games found on this computer.") }, [];
    get_game: Read, "The game of the map",
        "The game the open map is for (its own one, kept with the map, else the default one of Settings > Game) \
        with its profile: folders, BSP format, limits, FGD files, executable.",
        {}, [];
    set_game: Editor, "Choose the game",
        "Make a game (an id from list_games) the game of the open map (scope \"map\", kept with the map), or the \
        default game of every map (scope \"default\", saved in the settings: set confirm). Its content and entity \
        definitions load when it is installed; the compile follows its BSP format, limits and lighting.",
        {
            "id": string("Game id from list_games (gmod, tf, portal2, mod:<folder>...)."),
            "scope": choice("map: this map only (default); default: every map without a game of its own.", &["map", "default"]),
            "confirm": boolean("The user asked to change the default game (scope default)."),
        }, ["id"];
}

fn profile_json(p: &hp_fs::GameProfile) -> Value {
    let f = p.format();
    json!({
        "id": p.id,
        "name": p.name,
        "app_id": p.app_id,
        "installed": p.installed,
        "branch": p.branch.map(|b| b.name()),
        "bsp_version": p.branch.map(|_| f.version),
        "lump_table": p.branch.map(|_| if f.layout == hp_bsp::LumpLayout::VersionFirst { "version first (Left 4 Dead 2)" } else { "standard" }),
        "static_props": p.branch.map(|_| f.static_props.name()),
        "worldlights_version": p.branch.map(|_| f.worldlights_version),
        "limits": p.branch.map(|_| if f.limits == hp_bsp::limits::LimitSet::Gmod { "Garry's Mod" } else { "Valve" }),
        "lighting": p.lighting.flag(),
        "game_dir": (!p.game_dir.as_os_str().is_empty()).then(|| p.game_dir.display().to_string()),
        "install_dir": (!p.root.as_os_str().is_empty()).then(|| p.root.display().to_string()),
        "executable": p.exe.as_ref().map(|e| e.display().to_string()),
        "fgds": p.fgds.iter().map(|f| f.display().to_string()).collect::<Vec<_>>(),
        "source_mod": p.source_mod,
        "mapbase": p.mapbase,
    })
}

fn list_games(_state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let installed_only = a.opt_bool("installed_only")?.unwrap_or(false);
    let list = hp_fs::game_catalogue();
    let games: Vec<Value> = list.iter().filter(|p| p.installed || !installed_only).map(profile_json).collect();
    Ok(ToolResult::json(json!({ "count": games.len(), "games": games })))
}

fn get_game(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let g = &state.settings.game;
    let profile = g.active_profile();
    Ok(ToolResult::json(json!({
        "active": g.active_game(),
        "default": g.game,
        "map_game": g.map_game,
        "is_gmod": g.is_gmod(),
        "profile": profile.as_ref().map(profile_json),
    })))
}

fn set_game(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = a.str("id", MAX_ID)?.trim().to_owned();
    let for_map = a.opt_enum("scope", &["map", "default"])?.as_deref() != Some("default");
    if !for_map && a.opt_bool("confirm")? != Some(true) {
        return Err(
            "the default game is saved in the settings: set confirm to true when the user asked for it".to_owned()
        );
    }
    let list = hp_fs::game_catalogue();
    let Some(p) = list.iter().find(|p| p.id.eq_ignore_ascii_case(&id)) else {
        return Err(format!("no game {id} (list_games gives the ids)"));
    };
    let p = p.clone();
    let changed = hp_app::games::choose_game(state, &p.id, for_map);
    Ok(ToolResult::json(json!({
        "game": p.id,
        "scope": if for_map { "map" } else { "default" },
        "changed": changed,
        "installed": p.installed,
        "note": if p.installed {
            "Its content and entity definitions are loading (get_fgd_info shows when they are loaded)."
        } else {
            "Not installed: the content mounted now stays; maps compile in its format and limits, without copy nor launch."
        },
    })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn games() {
        let mut s = hp_app::testing::test_state();
        let all = ok(&mut s, "list_games", json!({}));
        let games = all["games"].as_array().unwrap();
        let gmod = games.iter().find(|g| g["id"] == "gmod").expect("Garry's Mod is listed");
        assert_eq!(gmod["bsp_version"], 20);
        assert_eq!(gmod["limits"], "Garry's Mod");
        let l4d2 = games.iter().find(|g| g["id"] == "left4dead2").expect("listed");
        assert_eq!(l4d2["bsp_version"], 21);
        assert_eq!(l4d2["static_props"], "v9");
        assert!(!games.iter().any(|g| g["id"] == "cs2" || g["id"] == "contagion"));
        let installed = ok(&mut s, "list_games", json!({ "installed_only": true }));
        assert!(installed["games"].as_array().unwrap().iter().all(|g| g["installed"] == true));
        let g = ok(&mut s, "get_game", json!({}));
        assert_eq!(g["active"], "gmod");
        assert_eq!(g["is_gmod"], true);
        let r = ok(&mut s, "set_game", json!({ "id": "left4dead2" }));
        assert_eq!(r["scope"], "map");
        assert_eq!(ok(&mut s, "get_game", json!({}))["active"], "left4dead2");
        assert_eq!(s.settings.compile.target.as_deref(), Some("left4dead2"));
        ok(&mut s, "set_game", json!({ "id": "gmod" }));
        assert_eq!(ok(&mut s, "get_game", json!({}))["is_gmod"], true);
        err(&mut s, "set_game", json!({ "id": "tf", "scope": "default" }));
        err(&mut s, "set_game", json!({ "id": "nope" }));
        err(&mut s, "set_game", json!({ "id": "contagion" }));
        ok(&mut s, "set_game", json!({ "id": "tf", "scope": "default", "confirm": true }));
        assert_eq!(s.settings.game.game, "tf");
        assert_eq!(ok(&mut s, "get_game", json!({}))["default"], "tf");
    }
}
