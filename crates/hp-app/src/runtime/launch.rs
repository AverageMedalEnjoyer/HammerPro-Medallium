use std::collections::HashMap;
use std::time::Duration;

use crate::state::AppState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameState {
    #[default]
    Unknown,
    NotRunning,
    Starting,
    Ready,
}

impl GameState {
    pub fn is_running(self) -> bool {
        matches!(self, GameState::Starting | GameState::Ready)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapLoadReport {
    pub map: String,
    pub total: Duration,
    pub phases: Vec<(String, Duration)>,
}

#[derive(Debug, Default)]
pub struct LaunchRuntime {
    pub game: GameState,
    pub pending_map: Option<String>,
    pub last_load: Option<MapLoadReport>,
    pub bridge_installed: Option<bool>,
    pub bridge_request: Option<bool>,
    pub compile_launch_deferred: bool,
    pub compile_launch_map: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContentUse {
    pub workshop: bool,
    pub addons: bool,
}

impl ContentUse {
    pub fn from_graph(graph: &hp_fs::deps::DependencyGraph) -> Self {
        let has = |o: hp_fs::SourceOrigin| graph.nodes.iter().any(|n| n.origin == Some(o));
        Self { workshop: has(hp_fs::SourceOrigin::Workshop), addons: has(hp_fs::SourceOrigin::Addon) }
    }
}

fn material_vpath(name: &str) -> String {
    let name = name.trim().replace('\\', "/").to_ascii_lowercase();
    let name = name.strip_prefix("materials/").unwrap_or(&name);
    let name = name.strip_suffix(".vmt").unwrap_or(name);
    format!("materials/{name}.vmt")
}

pub fn content_use(state: &AppState) -> Option<ContentUse> {
    let fs = state.assets.fs()?;
    let origins: HashMap<u32, hp_fs::SourceOrigin> = fs.sources().into_iter().map(|s| (s.id, s.origin)).collect();
    let mut paths: Vec<String> = state.doc.used_materials().into_iter().map(|(m, _)| material_vpath(&m)).collect();
    for node in state.doc.scene().entities().values() {
        if let Some(model) = node.entity.get("model").filter(|m| m.to_ascii_lowercase().ends_with(".mdl")) {
            paths.push(model.replace('\\', "/").to_ascii_lowercase());
        }
    }
    let mut out = ContentUse::default();
    for path in &paths {
        match fs.which_source(path).and_then(|id| origins.get(&id)) {
            Some(hp_fs::SourceOrigin::Workshop) => out.workshop = true,
            Some(hp_fs::SourceOrigin::Addon) => out.addons = true,
            _ => {}
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_names_become_vmt_paths() {
        assert_eq!(material_vpath("CONCRETE/Floor01"), "materials/concrete/floor01.vmt");
        assert_eq!(material_vpath("materials\\a\\b.vmt"), "materials/a/b.vmt");
    }

    #[test]
    fn graphs_tell_what_comes_from_addons_and_the_workshop() {
        let node = |vpath: &str, origin| hp_fs::deps::DepNode {
            vpath: vpath.to_owned(),
            kind: hp_fs::deps::DepKind::Material,
            source: Some(1),
            origin: Some(origin),
            source_label: String::new(),
            workshop_id: None,
            in_base: false,
            size: 0,
            reasons: Vec::new(),
            reason_count: 0,
            generated: None,
        };
        let mut graph = hp_fs::deps::DependencyGraph {
            nodes: vec![node("materials/a.vmt", hp_fs::SourceOrigin::BaseGame)],
            missing: Vec::new(),
            warnings: Vec::new(),
            generation: 1,
            cancelled: false,
            links: Vec::new(),
        };
        assert_eq!(ContentUse::from_graph(&graph), ContentUse::default());
        graph.nodes.push(node("materials/b.vmt", hp_fs::SourceOrigin::Addon));
        assert_eq!(ContentUse::from_graph(&graph), ContentUse { workshop: false, addons: true });
        graph.nodes.push(node("models/c.mdl", hp_fs::SourceOrigin::Workshop));
        assert_eq!(ContentUse::from_graph(&graph), ContentUse { workshop: true, addons: true });
    }

    #[test]
    fn nothing_is_known_without_game_content() {
        let state = crate::testing::test_state();
        assert_eq!(content_use(&state), None);
    }
}
