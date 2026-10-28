use std::path::{Path, PathBuf};

use hp_kv::Block;

use crate::{EditorInfo, Entity, Id, Map, Solid, VisGroup};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestEntry {
    pub relative_vmf: String,
    pub internal_id: i64,
    pub is_primary: bool,
    pub is_protected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub entries: Vec<ManifestEntry>,
    rest: Block,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("{0}")]
    Parse(#[from] hp_kv::KvError),
    #[error("not a VMF manifest (no Maps / VMFEntry block)")]
    NotAManifest,
}

fn bool_key(block: &Block, key: &str) -> bool {
    block.get(key).map(|v| v.trim()).is_some_and(|v| v != "0" && !v.is_empty())
}

fn entry_from_block(block: &Block) -> Option<ManifestEntry> {
    let relative_vmf = block.get("RelativeVMF").map(str::to_owned).filter(|p| !p.trim().is_empty())?;
    Some(ManifestEntry {
        relative_vmf,
        internal_id: block.get_parsed("InternalID").unwrap_or(0),
        is_primary: bool_key(block, "IsPrimary"),
        is_protected: bool_key(block, "IsProtected"),
    })
}

fn entry_to_block(entry: &ManifestEntry) -> Block {
    let mut b = Block::new("VMFEntry");
    b.set("IsPrimary", if entry.is_primary { "1" } else { "0" });
    b.set("IsProtected", if entry.is_protected { "1" } else { "0" });
    b.set("InternalID", entry.internal_id.to_string());
    b.set("RelativeVMF", entry.relative_vmf.clone());
    b
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let root = hp_kv::parse(text)?;
        let maps = root.child("Maps").cloned().unwrap_or(root);
        let mut entries = Vec::new();
        let mut rest = Block::new(if maps.name.is_empty() { "Maps".to_owned() } else { maps.name.clone() });
        rest.props = maps.props.clone();
        for child in &maps.children {
            if child.name.eq_ignore_ascii_case("VMFEntry") {
                if let Some(e) = entry_from_block(child) {
                    entries.push(e);
                }
            } else {
                rest.children.push(child.clone());
            }
        }
        if entries.is_empty() {
            return Err(ManifestError::NotAManifest);
        }
        Ok(Self { entries, rest })
    }

    pub fn single(relative_vmf: impl Into<String>) -> Self {
        Self {
            entries: vec![ManifestEntry {
                relative_vmf: relative_vmf.into(),
                internal_id: 1,
                is_primary: true,
                is_protected: false,
            }],
            rest: Block::new("Maps"),
        }
    }

    pub fn primary(&self) -> Option<&ManifestEntry> {
        self.entries.iter().find(|e| e.is_primary).or_else(|| self.entries.first())
    }

    pub fn to_text(&self) -> String {
        let mut maps = self.rest.clone();
        if maps.name.is_empty() {
            maps.name = "Maps".to_owned();
        }
        let kept = std::mem::take(&mut maps.children);
        maps.children = self.entries.iter().map(entry_to_block).collect();
        maps.children.extend(kept);
        let mut root = Block::new(String::new());
        root.children.push(maps);
        hp_kv::write(&root)
    }
}

pub fn submap_dir(manifest_path: &Path) -> PathBuf {
    manifest_path.with_extension("")
}

pub fn submap_path(manifest_path: &Path, relative_vmf: &str) -> PathBuf {
    let mut path = submap_dir(manifest_path);
    for part in relative_vmf.split(['\\', '/']).filter(|p| !p.is_empty() && *p != ".") {
        path.push(part);
    }
    path
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmapLayout {
    pub internal_id: i64,
    pub relative_vmf: String,
    pub id_start: Id,
    pub id_end: Id,
    pub is_primary: bool,
    pub is_protected: bool,
}

pub struct Combined {
    pub map: Map,
    pub layout: Vec<SubmapLayout>,
    pub frames: Vec<Map>,
}

fn shift_id(id: Id, off: Id) -> Id {
    if id == 0 { 0 } else { id.saturating_add(off) }
}

fn offset_editor(ed: &mut EditorInfo, off: Id) {
    if let Some(g) = ed.group_id.as_mut() {
        *g = shift_id(*g, off);
    }
    for v in &mut ed.visgroup_ids {
        *v = shift_id(*v, off);
    }
}

fn offset_solid(s: &mut Solid, off: Id) {
    s.id = shift_id(s.id, off);
    for side in &mut s.sides {
        side.id = shift_id(side.id, off);
    }
    offset_editor(&mut s.editor, off);
}

fn offset_entity(e: &mut Entity, off: Id) {
    e.id = shift_id(e.id, off);
    offset_editor(&mut e.editor, off);
    for s in &mut e.solids {
        offset_solid(s, off);
    }
    for g in &mut e.groups {
        g.id = shift_id(g.id, off);
        offset_editor(&mut g.editor, off);
    }
}

fn offset_visgroup(vg: &mut VisGroup, off: Id) {
    vg.id = shift_id(vg.id, off);
    for c in &mut vg.children {
        offset_visgroup(c, off);
    }
}

fn offset_map(m: &mut Map, off: Id) {
    offset_entity(&mut m.world, off);
    for e in &mut m.entities {
        offset_entity(e, off);
    }
    for vg in &mut m.visgroups {
        offset_visgroup(vg, off);
    }
}

fn visgroup_max(vg: &VisGroup, mx: &mut Id) {
    *mx = (*mx).max(vg.id);
    for c in &vg.children {
        visgroup_max(c, mx);
    }
}

fn entity_max(e: &Entity, mx: &mut Id) {
    *mx = (*mx).max(e.id);
    for s in &e.solids {
        *mx = (*mx).max(s.id);
        for side in &s.sides {
            *mx = (*mx).max(side.id);
        }
    }
    for g in &e.groups {
        *mx = (*mx).max(g.id);
    }
}

fn max_id(m: &Map) -> Id {
    let mut mx = 0;
    entity_max(&m.world, &mut mx);
    for e in &m.entities {
        entity_max(e, &mut mx);
    }
    for vg in &m.visgroups {
        visgroup_max(vg, &mut mx);
    }
    mx
}

fn frame_of(m: &Map) -> Map {
    let mut f = m.clone();
    f.world.solids.clear();
    f.world.groups.clear();
    f.entities.clear();
    f.visgroups.clear();
    f
}

pub fn combine(submaps: &[(ManifestEntry, Map)]) -> Combined {
    let primary_idx = submaps.iter().position(|(e, _)| e.is_primary).unwrap_or(0);
    let mut out = submaps.get(primary_idx).map(|(_, m)| m.clone()).unwrap_or_default();
    out.world.solids.clear();
    out.world.groups.clear();
    out.entities.clear();
    out.visgroups.clear();

    let mut layout = Vec::with_capacity(submaps.len());
    let mut frames = Vec::with_capacity(submaps.len());
    let mut base: Id = 0;
    for (entry, map) in submaps {
        frames.push(frame_of(map));
        let top = max_id(map);
        let mut shifted = map.clone();
        offset_map(&mut shifted, base);
        out.world.solids.extend(shifted.world.solids);
        out.world.groups.extend(shifted.world.groups);
        out.entities.extend(shifted.entities);
        out.visgroups.extend(shifted.visgroups);
        layout.push(SubmapLayout {
            internal_id: entry.internal_id,
            relative_vmf: entry.relative_vmf.clone(),
            id_start: base.saturating_add(1),
            id_end: base.saturating_add(top).saturating_add(1),
            is_primary: entry.is_primary,
            is_protected: entry.is_protected,
        });
        base = base.saturating_add(top);
    }
    Combined { map: out, layout, frames }
}

fn visgroup_ids(m: &Map) -> std::collections::HashSet<Id> {
    fn walk(vg: &VisGroup, set: &mut std::collections::HashSet<Id>) {
        set.insert(vg.id);
        for c in &vg.children {
            walk(c, set);
        }
    }
    let mut set = std::collections::HashSet::new();
    for vg in &m.visgroups {
        walk(vg, &mut set);
    }
    set
}

fn sanitize_editor(ed: &mut EditorInfo, groups: &std::collections::HashSet<Id>, vgs: &std::collections::HashSet<Id>) {
    if ed.group_id.is_some_and(|g| !groups.contains(&g)) {
        ed.group_id = None;
    }
    ed.visgroup_ids.retain(|v| vgs.contains(v));
}

pub fn split(combined: &Map, layout: &[SubmapLayout], frames: &[Map], active: usize) -> Vec<Map> {
    let primary = layout.iter().position(|l| l.is_primary).unwrap_or(0).min(frames.len().saturating_sub(1));
    let active = match layout.get(active) {
        Some(l) if !l.is_protected => active,
        _ => primary,
    };
    let route = |id: Id| -> usize { layout.iter().position(|l| id >= l.id_start && id < l.id_end).unwrap_or(active) };
    let mut outs: Vec<Map> = frames.to_vec();
    if outs.is_empty() {
        return outs;
    }
    for s in &combined.world.solids {
        let i = route(s.id);
        outs[i].world.solids.push(s.clone());
    }
    for g in &combined.world.groups {
        let i = route(g.id);
        outs[i].world.groups.push(g.clone());
    }
    for e in &combined.entities {
        let i = route(e.id);
        outs[i].entities.push(e.clone());
    }
    for vg in &combined.visgroups {
        let i = route(vg.id);
        outs[i].visgroups.push(vg.clone());
    }
    outs[primary].world.props = combined.world.props.clone();
    outs[primary].world.connections = combined.world.connections.clone();
    outs[primary].world.editor = combined.world.editor.clone();
    outs[primary].version_info = combined.version_info.clone();
    outs[primary].view_settings = combined.view_settings.clone();
    outs[primary].cordons = combined.cordons.clone();
    outs[primary].cameras = combined.cameras.clone();
    for out in &mut outs {
        let vgs = visgroup_ids(out);
        let groups: std::collections::HashSet<Id> = out.world.groups.iter().map(|g| g.id).collect();
        sanitize_editor(&mut out.world.editor, &groups, &vgs);
        for s in &mut out.world.solids {
            sanitize_editor(&mut s.editor, &groups, &vgs);
        }
        for e in &mut out.entities {
            sanitize_editor(&mut e.editor, &groups, &vgs);
            for s in &mut e.solids {
                sanitize_editor(&mut s.editor, &groups, &vgs);
            }
        }
    }
    outs
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\"Maps\"\n{\n\t\"VMFEntry\"\n\t{\n\t\t\"IsPrimary\" \"1\"\n\t\t\"IsProtected\" \"0\"\n\t\t\"InternalID\" \"1\"\n\t\t\"RelativeVMF\" \"example.vmf\"\n\t}\n\t\"VMFEntry\"\n\t{\n\t\t\"IsPrimary\" \"0\"\n\t\t\"IsProtected\" \"0\"\n\t\t\"InternalID\" \"2\"\n\t\t\"RelativeVMF\" \"rooms\\\\east.vmf\"\n\t}\n\t\"CordonManager\"\n\t{\n\t\t\"active\" \"0\"\n\t}\n}\n";

    #[test]
    fn parses_entries_and_primary() {
        let m = Manifest::parse(SAMPLE).expect("parse");
        assert_eq!(m.entries.len(), 2);
        assert_eq!(m.entries[0].relative_vmf, "example.vmf");
        assert!(m.entries[0].is_primary);
        assert_eq!(m.entries[1].internal_id, 2);
        assert!(!m.entries[1].is_primary);
        assert_eq!(m.primary().unwrap().relative_vmf, "example.vmf");
    }

    #[test]
    fn round_trips_entries_and_keeps_cordons() {
        let m = Manifest::parse(SAMPLE).expect("parse");
        let text = m.to_text();
        assert!(text.contains("CordonManager"), "{text}");
        let again = Manifest::parse(&text).expect("reparse");
        assert_eq!(again.entries, m.entries);
        assert!(again.rest.child("CordonManager").is_some());
    }

    #[test]
    fn rejects_non_manifests() {
        assert!(matches!(Manifest::parse("\"World\" { }").unwrap_err(), ManifestError::NotAManifest));
        assert!(Manifest::parse("not { valid").is_err());
    }

    #[test]
    fn resolves_submap_paths() {
        let mani = Path::new("/maps/city.vmm");
        assert_eq!(submap_dir(mani), Path::new("/maps/city"));
        assert_eq!(submap_path(mani, "example.vmf"), Path::new("/maps/city/example.vmf"));
        assert_eq!(submap_path(mani, "rooms\\east.vmf"), Path::new("/maps/city/rooms/east.vmf"));
    }

    #[test]
    fn single_manifest_has_one_primary() {
        let m = Manifest::single("main.vmf");
        assert_eq!(m.entries.len(), 1);
        assert!(m.primary().unwrap().is_primary);
        assert!(m.to_text().contains("main.vmf"));
    }

    fn solid(id: Id, visgroup: Option<Id>) -> Solid {
        let mut s = Solid { id, ..Default::default() };
        if let Some(v) = visgroup {
            s.editor.visgroup_ids.push(v);
        }
        s
    }

    fn world_map(solids: Vec<Solid>, entities: Vec<(Id, &str)>, visgroups: Vec<Id>) -> Map {
        let mut m = Map::default();
        m.world.classname = "worldspawn".to_owned();
        m.world.solids = solids;
        m.entities =
            entities.into_iter().map(|(id, c)| Entity { id, classname: c.to_owned(), ..Default::default() }).collect();
        m.visgroups =
            visgroups.into_iter().map(|id| VisGroup { id, name: format!("vg{id}"), ..Default::default() }).collect();
        m
    }

    fn entry(id: i64, vmf: &str, primary: bool) -> ManifestEntry {
        ManifestEntry { relative_vmf: vmf.to_owned(), internal_id: id, is_primary: primary, is_protected: false }
    }

    #[test]
    fn combine_shifts_ids_into_disjoint_ranges() {
        let a = world_map(vec![solid(1, Some(10))], vec![(20, "info_player_start")], vec![10]);
        let b = world_map(vec![solid(1, None)], vec![(5, "light")], vec![]);
        let submaps = vec![(entry(1, "a.vmf", true), a), (entry(2, "b.vmf", false), b)];
        let c = combine(&submaps);

        assert_eq!(c.map.world.solids.len(), 2);
        assert_eq!(c.map.entities.len(), 2);
        let mut ids: Vec<Id> = c.map.world.solids.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![1, 21], "{ids:?}");
        assert_eq!(c.layout[0].id_start, 1);
        assert_eq!(c.layout[1].id_start, 21);
        assert_eq!(c.map.world.solids.iter().find(|s| s.id == 1).unwrap().editor.visgroup_ids, vec![10]);
    }

    #[test]
    fn split_routes_objects_back_and_new_ones_to_primary() {
        let a = world_map(vec![solid(1, Some(10))], vec![(20, "info_player_start")], vec![10]);
        let b = world_map(vec![solid(1, None)], vec![(5, "light")], vec![]);
        let submaps = vec![(entry(1, "a.vmf", true), a), (entry(2, "b.vmf", false), b)];
        let mut c = combine(&submaps);
        c.map.entities.push(Entity { id: 9999, classname: "prop_static".to_owned(), ..Default::default() });

        let outs = split(&c.map, &c.layout, &c.frames, 0);
        assert_eq!(outs.len(), 2);
        assert_eq!(outs[0].world.solids.len(), 1);
        assert_eq!(outs[0].visgroups.len(), 1);
        assert_eq!(outs[0].entities.len(), 2, "the new entity went to the primary submap");
        assert!(outs[0].entities.iter().any(|e| e.classname == "prop_static"));
        assert_eq!(outs[0].world.solids[0].editor.visgroup_ids, vec![10]);
        assert_eq!(outs[1].world.solids.len(), 1);
        assert_eq!(outs[1].entities.len(), 1);
        assert!(outs[1].visgroups.is_empty());
    }

    #[test]
    fn active_submap_receives_new_objects() {
        let a = world_map(vec![solid(1, None)], vec![], vec![]);
        let b = world_map(vec![solid(1, None)], vec![], vec![]);
        let submaps = vec![(entry(1, "a.vmf", true), a), (entry(2, "b.vmf", false), b)];
        let mut c = combine(&submaps);
        c.map.entities.push(Entity { id: 9999, classname: "prop_static".to_owned(), ..Default::default() });
        let outs = split(&c.map, &c.layout, &c.frames, 1);
        assert!(outs[1].entities.iter().any(|e| e.classname == "prop_static"));
        assert!(outs[0].entities.is_empty());
    }

    #[test]
    fn protected_active_falls_back_to_primary() {
        let a = world_map(vec![solid(1, None)], vec![], vec![]);
        let b = world_map(vec![solid(1, None)], vec![], vec![]);
        let submaps =
            vec![(entry(1, "a.vmf", true), a), (ManifestEntry { is_protected: true, ..entry(2, "b.vmf", false) }, b)];
        let mut c = combine(&submaps);
        assert!(c.layout[1].is_protected);
        c.map.entities.push(Entity { id: 9999, classname: "prop_static".to_owned(), ..Default::default() });
        let outs = split(&c.map, &c.layout, &c.frames, 1);
        assert!(outs[0].entities.iter().any(|e| e.classname == "prop_static"));
        assert!(outs[1].entities.is_empty());
    }

    #[test]
    fn split_drops_dangling_visgroup_references() {
        let a = world_map(vec![solid(1, None)], vec![], vec![10]);
        let b = world_map(vec![solid(1, None)], vec![], vec![]);
        let submaps = vec![(entry(1, "a.vmf", true), a), (entry(2, "b.vmf", false), b)];
        let mut c = combine(&submaps);
        let b_id = c.layout[1].id_start;
        let b_solid = c.map.world.solids.iter_mut().find(|s| s.id == b_id).unwrap();
        b_solid.editor.visgroup_ids.push(10);

        let outs = split(&c.map, &c.layout, &c.frames, 0);
        assert!(outs[1].world.solids[0].editor.visgroup_ids.is_empty());
        assert_eq!(outs[0].world.solids[0].editor.visgroup_ids, Vec::<Id>::new());
    }
}
