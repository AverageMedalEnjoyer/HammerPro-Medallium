use std::collections::{BTreeMap, HashMap};

use hp_vmf::{Entity, Id, Map, Solid};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ObjectKind {
    Entity,
    Solid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Change {
    OnlyHere,
    OnlyThere,
    Changed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Detail {
    Key { key: String, here: Option<String>, there: Option<String> },
    Class { here: String, there: String },
    Outputs { here: usize, there: usize },
    Solids { here: usize, there: usize },
    Geometry,
    Materials { here: String, there: String },
    Alignment,
    Displacement,
    Editor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectDiff {
    pub kind: ObjectKind,
    pub id: Id,
    pub change: Change,
    pub class: String,
    pub owner: Option<Id>,
    pub details: Vec<Detail>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapDiff {
    pub objects: Vec<ObjectDiff>,
}

impl MapDiff {
    pub fn count(&self, change: Change) -> usize {
        self.objects.iter().filter(|o| o.change == change).count()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

fn keys(e: &Entity) -> BTreeMap<String, &str> {
    e.props.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.as_str())).collect()
}

fn entity_details(here: &Entity, there: &Entity) -> Vec<Detail> {
    let mut out = Vec::new();
    if !here.classname.eq_ignore_ascii_case(&there.classname) {
        out.push(Detail::Class { here: here.classname.clone(), there: there.classname.clone() });
    }
    let (a, b) = (keys(here), keys(there));
    for (k, v) in &a {
        match b.get(k) {
            Some(w) if w == v => {}
            other => out.push(Detail::Key {
                key: k.clone(),
                here: Some((*v).to_owned()),
                there: other.map(|w| (*w).to_owned()),
            }),
        }
    }
    for (k, w) in &b {
        if !a.contains_key(k) {
            out.push(Detail::Key { key: k.clone(), here: None, there: Some((*w).to_owned()) });
        }
    }
    if here.connections != there.connections {
        out.push(Detail::Outputs { here: here.connections.len(), there: there.connections.len() });
    }
    let ids = |e: &Entity| {
        let mut v: Vec<Id> = e.solids.iter().map(|s| s.id).collect();
        v.sort_unstable();
        v
    };
    if ids(here) != ids(there) {
        out.push(Detail::Solids { here: here.solids.len(), there: there.solids.len() });
    }
    if here.editor != there.editor {
        out.push(Detail::Editor);
    }
    out
}

fn solid_details(here: &Solid, there: &Solid) -> Vec<Detail> {
    let mut out = Vec::new();
    let same_planes = here.sides.len() == there.sides.len()
        && here.sides.iter().zip(&there.sides).all(|(a, b)| a.plane == b.plane && a.vertices_plus == b.vertices_plus);
    if !same_planes {
        out.push(Detail::Geometry);
    }
    if let Some((a, b)) =
        here.sides.iter().zip(&there.sides).find(|(a, b)| !a.material.eq_ignore_ascii_case(&b.material))
    {
        out.push(Detail::Materials { here: a.material.clone(), there: b.material.clone() });
    }
    let aligned = here.sides.iter().zip(&there.sides).all(|(a, b)| {
        a.uaxis == b.uaxis && a.vaxis == b.vaxis && a.rotation == b.rotation && a.lightmap_scale == b.lightmap_scale
    });
    if same_planes && !aligned {
        out.push(Detail::Alignment);
    }
    if here.sides.iter().zip(&there.sides).any(|(a, b)| a.disp != b.disp) {
        out.push(Detail::Displacement);
    }
    if out.is_empty() && here != there {
        out.push(Detail::Editor);
    }
    out
}

fn solids(map: &Map) -> HashMap<Id, (&Solid, Option<Id>, &str)> {
    let mut out = HashMap::new();
    for s in &map.world.solids {
        out.entry(s.id).or_insert((s, None, map.world.classname.as_str()));
    }
    for e in &map.entities {
        for s in &e.solids {
            out.entry(s.id).or_insert((s, Some(e.id), e.classname.as_str()));
        }
    }
    out
}

pub fn diff_maps(here: &Map, there: &Map) -> MapDiff {
    let mut objects = Vec::new();
    let mut world_here = here.world.clone();
    let mut world_there = there.world.clone();
    world_here.solids.clear();
    world_there.solids.clear();
    world_here.editor = Default::default();
    world_there.editor = Default::default();
    let world = entity_details(&world_here, &world_there);
    if !world.is_empty() {
        objects.push(ObjectDiff {
            kind: ObjectKind::Entity,
            id: here.world.id,
            change: Change::Changed,
            class: here.world.classname.clone(),
            owner: None,
            details: world,
        });
    }
    let a: HashMap<Id, &Entity> = here.entities.iter().map(|e| (e.id, e)).collect();
    let b: HashMap<Id, &Entity> = there.entities.iter().map(|e| (e.id, e)).collect();
    for (id, e) in &a {
        let entry = |change, details| ObjectDiff {
            kind: ObjectKind::Entity,
            id: *id,
            change,
            class: e.classname.clone(),
            owner: None,
            details,
        };
        match b.get(id) {
            None => objects.push(entry(Change::OnlyHere, Vec::new())),
            Some(other) => {
                let d = entity_details(e, other);
                if !d.is_empty() {
                    objects.push(entry(Change::Changed, d));
                }
            }
        }
    }
    for (id, e) in &b {
        if !a.contains_key(id) {
            objects.push(ObjectDiff {
                kind: ObjectKind::Entity,
                id: *id,
                change: Change::OnlyThere,
                class: e.classname.clone(),
                owner: None,
                details: Vec::new(),
            });
        }
    }
    let (sa, sb) = (solids(here), solids(there));
    for (id, (s, owner, class)) in &sa {
        let entry = |change, details| ObjectDiff {
            kind: ObjectKind::Solid,
            id: *id,
            change,
            class: (*class).to_owned(),
            owner: *owner,
            details,
        };
        match sb.get(id) {
            None => objects.push(entry(Change::OnlyHere, Vec::new())),
            Some((other, _, _)) => {
                if s != other {
                    let d = solid_details(s, other);
                    objects.push(entry(Change::Changed, d));
                }
            }
        }
    }
    for (id, (_, owner, class)) in &sb {
        if !sa.contains_key(id) {
            objects.push(ObjectDiff {
                kind: ObjectKind::Solid,
                id: *id,
                change: Change::OnlyThere,
                class: (*class).to_owned(),
                owner: *owner,
                details: Vec::new(),
            });
        }
    }
    objects.sort_by_key(|o| (o.kind, o.change, o.id));
    MapDiff { objects }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"world
{
	"id" "1"
	"classname" "worldspawn"
	"skyname" "sky_day01_01"
	solid
	{
		"id" "2"
		side { "id" "1" "plane" "(0 64 64) (64 64 64) (64 0 64)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "2" "plane" "(0 0 0) (64 0 0) (64 64 0)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "3" "plane" "(0 64 64) (0 0 64) (0 0 0)" "material" "DEV/A" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "4" "plane" "(64 64 0) (64 0 0) (64 0 64)" "material" "DEV/A" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "5" "plane" "(64 64 64) (0 64 64) (0 64 0)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "6" "plane" "(64 0 0) (0 0 0) (0 0 64)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
	}
}
entity
{
	"id" "10"
	"classname" "light"
	"targetname" "lamp"
	"origin" "0 0 96"
}
entity
{
	"id" "11"
	"classname" "info_target"
	"origin" "32 0 0"
}
"#;

    fn map(text: &str) -> Map {
        hp_vmf::parse(text).expect("parses")
    }

    #[test]
    fn identical_maps_have_no_difference() {
        let d = diff_maps(&map(BASE), &map(BASE));
        assert!(d.is_empty(), "{d:#?}");
    }

    #[test]
    fn keys_objects_and_faces() {
        let mut here = map(BASE);
        let there = map(BASE);
        let lamp = here.entities.iter_mut().find(|e| e.id == 10).expect("lamp");
        lamp.set("origin", "0 0 128");
        lamp.set("_light", "255 0 0 200");
        here.entities.retain(|e| e.id != 11);
        here.entities.push(Entity { id: 12, classname: "info_null".into(), ..Default::default() });
        here.world.solids[0].sides[0].material = "DEV/B".into();
        here.world.set("skyname", "sky_night01");
        let d = diff_maps(&here, &there);
        assert_eq!(d.count(Change::OnlyHere), 1);
        assert_eq!(d.count(Change::OnlyThere), 1);
        assert_eq!(d.count(Change::Changed), 3, "{d:#?}");
        let lamp = d.objects.iter().find(|o| o.id == 10).expect("lamp diff");
        assert!(lamp.details.contains(&Detail::Key {
            key: "origin".into(),
            here: Some("0 0 128".into()),
            there: Some("0 0 96".into())
        }));
        assert!(lamp.details.iter().any(|x| matches!(x, Detail::Key { key, there: None, .. } if key == "_light")));
        let solid = d.objects.iter().find(|o| o.kind == ObjectKind::Solid).expect("solid diff");
        assert_eq!(solid.details, vec![Detail::Materials { here: "DEV/B".into(), there: "DEV/A".into() }]);
        assert!(d.objects.iter().any(|o| o.id == 1 && o.class == "worldspawn"));
        assert_eq!(d.objects.last().map(|o| o.kind), Some(ObjectKind::Solid));
    }

    #[test]
    fn moved_brushes_are_geometry() {
        let mut here = map(BASE);
        for s in &mut here.world.solids[0].sides {
            for p in &mut s.plane {
                p.x += 16.0;
            }
        }
        let d = diff_maps(&here, &map(BASE));
        assert_eq!(d.objects.len(), 1);
        assert_eq!(d.objects[0].details, vec![Detail::Geometry]);
    }
}
