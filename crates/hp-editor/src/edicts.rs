use std::collections::HashMap;

use crate::limits::snapshot::MapSnapshot;
use crate::problems::{Problem, Severity};
use crate::selection::ObjRef;

pub const MAX_EDICTS: usize = 8192;
pub const KEEP_FREE: usize = 2000;

const COMPILED_AWAY: &[&str] = &[
    "func_detail",
    "prop_static",
    "prop_detail",
    "info_overlay",
    "info_overlay_transition",
    "info_overlay_accessor",
    "env_cubemap",
    "func_instance",
    "func_instance_parms",
    "func_instance_origin",
    "func_instance_io_proxy",
    "info_null",
    "func_viscluster",
    "info_no_dynamic_shadow",
    "env_detail_controller",
];

const SERVER_ONLY_PREFIXES: &[&str] = &["logic_", "math_", "filter_"];
const SERVER_ONLY: &[&str] = &["point_template", "env_global", "info_lighting", "game_ragdoll_manager"];

const LIGHTS: &[&str] = &["light", "light_spot", "light_environment", "light_glspot"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdictFree {
    Compiled,
    RemovedAtSpawn,
    ServerOnly,
}

pub fn edict_free(class: &str, named: bool) -> Option<EdictFree> {
    let c = class.to_ascii_lowercase();
    if COMPILED_AWAY.contains(&c.as_str()) {
        return Some(EdictFree::Compiled);
    }
    if (!named && (LIGHTS.contains(&c.as_str()) || c == "infodecal")) || c.starts_with("info_node") {
        return Some(EdictFree::RemovedAtSpawn);
    }
    if SERVER_ONLY.contains(&c.as_str()) || SERVER_ONLY_PREFIXES.iter().any(|p| c.starts_with(p)) {
        return Some(EdictFree::ServerOnly);
    }
    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EdictHint {
    StaticBrushes { count: usize },
    DynamicProps { count: usize },
    NamedLights { count: usize },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EdictReport {
    pub used: usize,
    pub by_class: Vec<(String, usize)>,
    pub hints: Vec<EdictHint>,
}

impl EdictReport {
    pub fn left(&self) -> usize {
        MAX_EDICTS.saturating_sub(self.used)
    }

    pub fn tight(&self) -> bool {
        self.left() < KEEP_FREE
    }
}

const STATIC_BRUSHES: &[&str] = &["func_brush", "func_illusionary", "func_wall", "func_lod"];

pub fn estimate_edicts(snap: &MapSnapshot) -> EdictReport {
    let mut by_class: HashMap<String, usize> = HashMap::new();
    let mut used = 1;
    let mut targeted: std::collections::HashSet<String> = std::collections::HashSet::new();
    for r in &snap.entities {
        for c in &r.entity.connections {
            targeted.insert(c.target.trim().to_ascii_lowercase());
        }
    }
    let (mut static_brushes, mut dynamic_props, mut named_lights) = (0, 0, 0);
    for r in &snap.entities {
        let e = &r.entity;
        if !snap.is_visible(ObjRef::Entity(e.id)) {
            continue;
        }
        let name = e.get("targetname").map(str::trim).filter(|n| !n.is_empty());
        if edict_free(&e.classname, name.is_some()).is_some() {
            continue;
        }
        used += 1;
        *by_class.entry(e.classname.to_ascii_lowercase()).or_default() += 1;
        let class = e.classname.to_ascii_lowercase();
        if STATIC_BRUSHES.contains(&class.as_str()) && name.is_none() {
            static_brushes += 1;
        }
        if class == "prop_dynamic"
            && name.is_none_or(|n| !targeted.contains(&n.to_ascii_lowercase()))
            && e.get("parentname").is_none_or(|p| p.trim().is_empty())
            && e.connections.is_empty()
        {
            dynamic_props += 1;
        }
        if LIGHTS.contains(&class.as_str()) && name.is_some() {
            named_lights += 1;
        }
    }
    let mut by_class: Vec<(String, usize)> = by_class.into_iter().collect();
    by_class.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut hints = Vec::new();
    if static_brushes >= 8 {
        hints.push(EdictHint::StaticBrushes { count: static_brushes });
    }
    if dynamic_props >= 8 {
        hints.push(EdictHint::DynamicProps { count: dynamic_props });
    }
    if named_lights > 0 {
        hints.push(EdictHint::NamedLights { count: named_lights });
    }
    EdictReport { used, by_class, hints }
}

pub fn edict_problems(report: &EdictReport) -> Vec<Problem> {
    if !report.tight() {
        return Vec::new();
    }
    let top: Vec<String> = report.by_class.iter().take(3).map(|(c, n)| format!("{c} {n}")).collect();
    vec![
        Problem::new(
            if report.used > MAX_EDICTS { Severity::Error } else { Severity::Warning },
            "edict_budget",
            format!(
                "{} edicts of {MAX_EDICTS} used, {} left (keep about {KEEP_FREE}): {}",
                report.used,
                report.left(),
                top.join(", ")
            ),
            Vec::new(),
            false,
        )
        .with_args(vec![report.used.to_string(), report.left().to_string(), top.join(", ")]),
    ]
}

#[cfg(test)]
mod tests {
    use glam::DVec3;

    use super::*;
    use crate::Document;

    #[test]
    fn counts_what_spawns() {
        let mut doc = Document::new_empty();
        let kv = |k: &str, v: &str| vec![(k.to_owned(), v.to_owned())];
        doc.create_point_entity("prop_static", DVec3::ZERO, &[]).expect("e");
        doc.create_point_entity("light", DVec3::ZERO, &[]).expect("e");
        doc.create_point_entity("light", DVec3::ZERO, &kv("targetname", "switch")).expect("e");
        doc.create_point_entity("logic_relay", DVec3::ZERO, &[]).expect("e");
        doc.create_point_entity("filter_activator_name", DVec3::ZERO, &[]).expect("e");
        doc.create_point_entity("info_node", DVec3::ZERO, &[]).expect("e");
        for _ in 0..9 {
            doc.create_point_entity("prop_dynamic", DVec3::ZERO, &[]).expect("e");
        }
        doc.create_point_entity("func_areaportal", DVec3::ZERO, &[]).expect("e");
        let report = estimate_edicts(&MapSnapshot::capture(&doc));
        assert_eq!(report.used, 12);
        assert_eq!(report.by_class[0], ("prop_dynamic".to_owned(), 9));
        assert!(report.hints.contains(&EdictHint::DynamicProps { count: 9 }));
        assert!(report.hints.contains(&EdictHint::NamedLights { count: 1 }));
        assert!(!report.tight());
        assert!(edict_problems(&report).is_empty());
        let full = EdictReport { used: 7000, by_class: vec![("prop_physics".into(), 6000)], hints: Vec::new() };
        let p = edict_problems(&full);
        assert_eq!(p[0].code, "edict_budget");
        assert_eq!(p[0].severity, Severity::Warning);
        assert!(p[0].message.starts_with("7000 edicts of 8192 used, 1192 left"));
        assert_eq!(edict_free("LIGHT_SPOT", false), Some(EdictFree::RemovedAtSpawn));
        assert_eq!(edict_free("light_spot", true), None);
        assert_eq!(edict_free("math_counter", false), Some(EdictFree::ServerOnly));
        assert_eq!(edict_free("func_detail", false), Some(EdictFree::Compiled));
    }
}
