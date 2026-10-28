use std::borrow::Cow;
use std::collections::HashSet;
use std::path::Path;

use glam::DVec3;
use hp_geom::Aabb;

use crate::clipboard::SideLayout;
use crate::document::Document;
use crate::ops::Fragment;
use crate::selection::{ObjRef, SelectOp};
use crate::{EditError, EditResult, Id};

mod sprinkle;

pub use sprinkle::{MAX_SPRINKLE_ITEMS, SprinkleItem};

pub const TOKEN: &str = "&i";
pub const MAX_FILE_BYTES: u64 = 256 << 20;
pub const MAX_PREVIEW_EDGES: usize = 16_384;
pub const POINT_HALF: f64 = 8.0;
const MAX_TOKEN_TRIES: u32 = 100_000;

#[derive(Clone, Debug)]
pub struct Prefab {
    frag: Fragment,
    bounds: Option<Aabb>,
    edges: Vec<[DVec3; 2]>,
    token_names: Vec<String>,
    has_token: bool,
    solids: usize,
    entities: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PrefabPlacement {
    Offset(DVec3),
    Fit(Aabb),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InsertOptions {
    pub token: Option<u32>,
    pub group: bool,
}

impl Default for InsertOptions {
    fn default() -> Self {
        Self { token: None, group: true }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InsertedPrefab {
    pub objects: Vec<ObjRef>,
    pub group: Option<Id>,
    pub token: Option<u32>,
}

pub fn replace_token(text: &str, n: u32) -> Cow<'_, str> {
    if !has_token(text) {
        return Cow::Borrowed(text);
    }
    let number = n.to_string();
    let mut out = String::with_capacity(text.len() + 8);
    let mut rest = text;
    while let Some(i) = find_token(rest) {
        out.push_str(&rest[..i]);
        out.push_str(&number);
        rest = &rest[i + TOKEN.len()..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

pub fn has_token(text: &str) -> bool {
    find_token(text).is_some()
}

fn find_token(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    bytes.windows(2).position(|w| w[0] == b'&' && (w[1] == b'i' || w[1] == b'I'))
}

fn apply_token(frag: &mut Fragment, n: u32) {
    let fix = |s: &mut String| {
        if let Cow::Owned(new) = replace_token(s, n) {
            *s = new;
        }
    };
    for e in &mut frag.entities {
        for (_, v) in &mut e.props {
            fix(v);
        }
        for c in &mut e.connections {
            fix(&mut c.target);
            fix(&mut c.param);
        }
    }
}

fn push_solid_edges(geom: &hp_geom::SolidGeom, seen: &mut HashSet<[i64; 6]>, out: &mut Vec<[DVec3; 2]>) {
    let key = |p: DVec3| [(p.x * 64.0).round() as i64, (p.y * 64.0).round() as i64, (p.z * 64.0).round() as i64];
    for f in &geom.faces {
        let n = f.vertices.len();
        for i in 0..n {
            if out.len() >= MAX_PREVIEW_EDGES {
                return;
            }
            let (Some(&a), Some(&b)) = (f.vertices.get(i), f.vertices.get((i + 1) % n)) else { continue };
            let (ka, kb) = (key(a), key(b));
            let k = if ka <= kb {
                [ka[0], ka[1], ka[2], kb[0], kb[1], kb[2]]
            } else {
                [kb[0], kb[1], kb[2], ka[0], ka[1], ka[2]]
            };
            if seen.insert(k) {
                out.push([a, b]);
            }
        }
    }
}

fn push_box_edges(b: &Aabb, out: &mut Vec<[DVec3; 2]>) {
    const EDGES: [(usize, usize); 12] =
        [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];
    let c = b.corners();
    for (i, j) in EDGES {
        if out.len() >= MAX_PREVIEW_EDGES {
            return;
        }
        out.push([c[i], c[j]]);
    }
}

impl Prefab {
    pub fn parse(text: &str) -> EditResult<Self> {
        let parsed = hp_vmf::parse_fragment_full(text)?;
        let frag = Fragment {
            solids: parsed.solids,
            entities: parsed.entities,
            groups: parsed.groups,
            visgroups: parsed.visgroups,
        };
        if frag.is_empty() {
            return Err(EditError::Nothing("the prefab has no map objects"));
        }
        let mut bounds = Aabb::EMPTY;
        let mut edges = Vec::new();
        let mut seen = HashSet::new();
        let solids = frag.solids.iter().chain(frag.entities.iter().flat_map(|e| e.solids.iter()));
        let mut solid_count = 0;
        for s in solids {
            solid_count += 1;
            let g = hp_geom::build_solid(&s.sides);
            if g.aabb.is_empty() || !g.aabb.min.is_finite() || !g.aabb.max.is_finite() {
                continue;
            }
            bounds = bounds.union(&g.aabb);
            push_solid_edges(&g, &mut seen, &mut edges);
        }
        let mut token_names = Vec::new();
        let mut any_token = false;
        for e in &frag.entities {
            if e.solids.is_empty()
                && let Some(o) = e.origin().filter(|o| o.is_finite())
            {
                let b = Aabb::new(o - DVec3::splat(POINT_HALF), o + DVec3::splat(POINT_HALF));
                bounds = bounds.union(&b);
                push_box_edges(&b, &mut edges);
            }
            if let Some(name) = e.get("targetname").filter(|n| has_token(n)) {
                token_names.push(name.trim().to_ascii_lowercase());
            }
            any_token |= e.props.iter().any(|(_, v)| has_token(v))
                || e.connections.iter().any(|c| has_token(&c.target) || has_token(&c.param));
        }
        token_names.sort_unstable();
        token_names.dedup();
        let bounds = (!bounds.is_empty() && bounds.min.is_finite() && bounds.max.is_finite()).then_some(bounds);
        let entities = frag.entities.len();
        Ok(Self { frag, bounds, edges, token_names, has_token: any_token, solids: solid_count, entities })
    }

    pub fn load(path: &Path) -> EditResult<Self> {
        let meta = std::fs::metadata(path).map_err(|e| EditError::Invalid(format!("{}: {e}", path.display())))?;
        if !meta.is_file() {
            return Err(EditError::Invalid(format!("{} is not a file", path.display())));
        }
        if meta.len() > MAX_FILE_BYTES {
            return Err(EditError::Invalid(format!("{} is too large ({} bytes)", path.display(), meta.len())));
        }
        let bytes = std::fs::read(path).map_err(|e| EditError::Invalid(format!("{}: {e}", path.display())))?;
        let (text, _) = hp_kv::decode(&bytes);
        Self::parse(&text)
    }

    pub fn bounds(&self) -> Option<Aabb> {
        self.bounds
    }

    pub fn preview_edges(&self) -> &[[DVec3; 2]] {
        &self.edges
    }

    pub fn has_token(&self) -> bool {
        self.has_token
    }

    pub fn solid_count(&self) -> usize {
        self.solids
    }

    pub fn entity_count(&self) -> usize {
        self.entities
    }
}

pub fn fit_box(from: &Aabb, to: &Aabb) -> Aabb {
    let (fs, ts, tc) = (from.size(), to.size(), to.center());
    let mut min = to.min;
    let mut max = to.max;
    for k in 0..3 {
        if !(fs[k] > 1e-6 && ts[k] > 1e-6) {
            min[k] = tc[k] - fs[k] * 0.5;
            max[k] = tc[k] + fs[k] * 0.5;
        }
    }
    Aabb { min, max }
}

impl Document {
    pub fn prefab_token(&self, prefab: &Prefab, start: u32) -> u32 {
        let start = start.max(1);
        if prefab.token_names.is_empty() {
            return start;
        }
        let used: HashSet<String> = self
            .scene
            .entities
            .values()
            .filter_map(|n| n.entity.get("targetname"))
            .filter(|n| !n.is_empty())
            .map(|n| n.trim().to_ascii_lowercase())
            .collect();
        let end = start.saturating_add(MAX_TOKEN_TRIES);
        (start..end)
            .find(|&n| prefab.token_names.iter().all(|name| !used.contains(replace_token(name, n).as_ref())))
            .unwrap_or(start)
    }

    pub fn insert_prefab(
        &mut self,
        prefab: &Prefab,
        placement: PrefabPlacement,
        opts: &InsertOptions,
    ) -> EditResult<InsertedPrefab> {
        match placement {
            PrefabPlacement::Offset(d) if !d.is_finite() => {
                return Err(EditError::Invalid("offset is not finite".to_owned()));
            }
            PrefabPlacement::Fit(b) if b.is_empty() || !b.min.is_finite() || !b.max.is_finite() => {
                return Err(EditError::Invalid("invalid target box".to_owned()));
            }
            _ => {}
        }
        let mut frag = prefab.frag.clone();
        let token = opts.token.filter(|_| prefab.has_token);
        if let Some(n) = token {
            apply_token(&mut frag, n);
        }
        let layout = SideLayout::of(&frag);
        self.tx("Insert prefab", |doc| {
            let objs = doc.instantiate(frag, false)?;
            if let Some(layout) = &layout {
                doc.remap_side_lists(layout, &objs);
            }
            match (placement, prefab.bounds) {
                (PrefabPlacement::Offset(d), _) => {
                    if d != DVec3::ZERO {
                        doc.translate(&objs, d)?;
                    }
                }
                (PrefabPlacement::Fit(to), Some(from)) => {
                    let target = fit_box(&from, &to);
                    doc.scale_bounds(&objs, &from, &target)?;
                }
                (PrefabPlacement::Fit(to), None) => {
                    let c = doc.selection_bounds_of(&objs).map_or(DVec3::ZERO, |b| b.center());
                    let d = to.center() - c;
                    if d.is_finite() && d != DVec3::ZERO {
                        doc.translate(&objs, d)?;
                    }
                }
            }
            doc.attach_pasted_overlays(&objs);
            let group = if opts.group { doc.group_prefab(&objs)? } else { None };
            doc.select(&objs, SelectOp::Replace);
            Ok(InsertedPrefab { objects: objs, group, token })
        })
    }

    fn group_prefab(&mut self, objs: &[ObjRef]) -> EditResult<Option<Id>> {
        if objs.len() < 2 {
            return Ok(None);
        }
        let roots: HashSet<Option<Id>> =
            objs.iter().map(|o| self.scene.object_group(*o).map(|g| self.scene.group_root(g))).collect();
        if roots.len() == 1
            && let Some(Some(root)) = roots.into_iter().next()
        {
            return Ok(Some(root));
        }
        self.group(objs).map(Some)
    }

    fn selection_bounds_of(&self, objs: &[ObjRef]) -> Option<Aabb> {
        let mut b = Aabb::EMPTY;
        for o in objs {
            match *o {
                ObjRef::Solid(id) => {
                    if let Some(sb) = self.scene.solid_bounds(id) {
                        b = b.union(&sb);
                    }
                }
                ObjRef::Entity(id) => {
                    if let Some(sb) = self.scene.entity_solid_bounds(id) {
                        b = b.union(&sb);
                    } else if let Some(o) = self.scene.entity(id).and_then(|n| n.entity.origin()) {
                        b.extend(o);
                    }
                }
            }
        }
        (!b.is_empty() && b.min.is_finite() && b.max.is_finite()).then_some(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) const PREFAB: &str = r#"versioninfo
{
	"prefab" "1"
}
world
{
	"id" "1"
	"classname" "worldspawn"
	solid
	{
		"id" "2"
		side { "id" "1" "plane" "(0 64 64) (64 64 64) (64 0 64)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
		side { "id" "2" "plane" "(0 0 0) (64 0 0) (64 64 0)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
		side { "id" "3" "plane" "(0 64 64) (0 0 64) (0 0 0)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
		side { "id" "4" "plane" "(64 64 0) (64 0 0) (64 0 64)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
		side { "id" "5" "plane" "(64 64 64) (0 64 64) (0 64 0)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
		side { "id" "6" "plane" "(64 0 0) (0 0 0) (0 0 64)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
	}
}
entity
{
	"id" "3"
	"classname" "light"
	"targetname" "lamp_&i"
	"origin" "32 32 96"
	connections
	{
		"OnUser1" "door_&i,Open,&i,0,-1"
		"OnUser2" "outside,Toggle,,0,-1"
	}
}
entity
{
	"id" "4"
	"classname" "prop_dynamic"
	"targetname" "door_&I"
	"parentname" "lamp_&i,attach"
	"origin" "32 0 0"
}
"#;

    fn doc() -> Document {
        Document::new_empty()
    }

    fn names(doc: &Document, objs: &[ObjRef]) -> Vec<String> {
        let mut out: Vec<String> = objs
            .iter()
            .filter_map(|o| match o {
                ObjRef::Entity(id) => {
                    doc.scene().entity(*id).and_then(|n| n.entity.get("targetname")).map(str::to_owned)
                }
                ObjRef::Solid(_) => None,
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn tokens_are_replaced_everywhere_and_unique() {
        assert_eq!(replace_token("a_&i_&I", 7), "a_7_7");
        assert_eq!(replace_token("plain", 7), "plain");
        assert!(matches!(replace_token("plain", 7), Cow::Borrowed(_)));
        assert_eq!(replace_token("&", 3), "&");
        assert_eq!(replace_token("é&ié", 3), "é3é");

        let prefab = Prefab::parse(PREFAB).expect("prefab");
        assert!(prefab.has_token());
        assert_eq!(prefab.solid_count(), 1);
        assert_eq!(prefab.entity_count(), 2);
        let mut doc = doc();
        let n = doc.prefab_token(&prefab, 1);
        assert_eq!(n, 1);
        let opts = InsertOptions { token: Some(n), group: true };
        let first = doc.insert_prefab(&prefab, PrefabPlacement::Offset(DVec3::ZERO), &opts).expect("insert");
        assert_eq!(first.token, Some(1));
        assert_eq!(names(&doc, &first.objects), ["door_1", "lamp_1"]);
        let light = first
            .objects
            .iter()
            .find_map(|o| match o {
                ObjRef::Entity(id) if doc.scene().entity(*id).is_some_and(|n| n.entity.classname == "light") => {
                    doc.scene().entity(*id)
                }
                _ => None,
            })
            .expect("light");
        assert_eq!(light.entity.connections[0].target, "door_1");
        assert_eq!(light.entity.connections[0].param, "1");
        assert_eq!(light.entity.connections[1].target, "outside", "names outside the prefab stay");
        let door = doc.entities_named("door_1");
        assert_eq!(doc.scene().entity(door[0]).and_then(|n| n.entity.get("parentname")), Some("lamp_1,attach"));
        assert_eq!(doc.undo_label(), Some("Insert prefab"));
        let group = first.group.expect("grouped");
        assert!(doc.scene().group(group).is_some());
        assert_eq!(doc.selected_objects().len(), 3);

        let n = doc.prefab_token(&prefab, 1);
        assert_eq!(n, 2);
        let second = doc.insert_prefab(
            &prefab,
            PrefabPlacement::Offset(DVec3::X * 128.0),
            &InsertOptions { token: Some(n), group: true },
        );
        assert_eq!(names(&doc, &second.expect("second").objects), ["door_2", "lamp_2"]);
        assert!(doc.undo());
        assert!(doc.undo());
        assert!(doc.entities_named("lamp_1").is_empty());
    }

    #[test]
    fn placements_offset_and_fit() {
        let prefab = Prefab::parse(PREFAB).expect("prefab");
        let b = prefab.bounds().expect("bounds");
        assert_eq!(b.min, DVec3::new(0.0, -8.0, -8.0));
        assert_eq!(b.max, DVec3::new(64.0, 64.0, 104.0));
        assert!(prefab.preview_edges().len() >= 12 + 24);

        let mut doc = doc();
        let opts = InsertOptions { token: Some(5), group: false };
        let out = doc.insert_prefab(&prefab, PrefabPlacement::Offset(DVec3::new(256.0, 0.0, 0.0)), &opts).expect("ok");
        assert_eq!(out.group, None);
        let solid = out.objects.iter().find_map(|o| if let ObjRef::Solid(id) = o { Some(*id) } else { None });
        let sb = doc.scene().solid_bounds(solid.expect("solid")).expect("bounds");
        assert_eq!(sb.min, DVec3::new(256.0, 0.0, 0.0));

        let to = Aabb::new(DVec3::new(-64.0, -8.0, 500.0), DVec3::new(64.0, 64.0, 500.0));
        let out = doc.insert_prefab(&prefab, PrefabPlacement::Fit(to), &opts).expect("fit");
        let solid = out.objects.iter().find_map(|o| if let ObjRef::Solid(id) = o { Some(*id) } else { None });
        let sb = doc.scene().solid_bounds(solid.expect("solid")).expect("bounds");
        assert!((sb.size().x - 128.0).abs() < 1e-6, "{sb:?}");
        assert!((sb.size().z - 64.0).abs() < 1e-6, "{sb:?}");
        assert!((b.center().z - 48.0).abs() < 1e-9);

        let before = doc.revision();
        assert!(doc.insert_prefab(&prefab, PrefabPlacement::Offset(DVec3::NAN), &opts).is_err());
        assert!(doc.insert_prefab(&prefab, PrefabPlacement::Fit(Aabb::EMPTY), &opts).is_err());
        assert_eq!(doc.revision(), before);
    }

    #[test]
    fn grouped_prefabs_keep_their_group() {
        let grouped = PREFAB
            .replace("\"id\" \"2\"\n", "\"id\" \"2\"\n\t\teditor { \"color\" \"0 1 2\" \"groupid\" \"9\" }\n")
            .replace(
                "\"classname\" \"worldspawn\"\n",
                "\"classname\" \"worldspawn\"\n\tgroup { \"id\" \"9\" editor { \"color\" \"0 1 2\" } }\n",
            );
        let grouped = grouped
            .replace(
                "\"origin\" \"32 32 96\"",
                "\"origin\" \"32 32 96\"\n\teditor { \"color\" \"1 1 1\" \"groupid\" \"9\" }",
            )
            .replace(
                "\"origin\" \"32 0 0\"",
                "\"origin\" \"32 0 0\"\n\teditor { \"color\" \"1 1 1\" \"groupid\" \"9\" }",
            );
        let prefab = Prefab::parse(&grouped).expect("prefab");
        let mut doc = doc();
        let groups = doc.scene().groups().len();
        let out =
            doc.insert_prefab(&prefab, PrefabPlacement::Offset(DVec3::ZERO), &InsertOptions::default()).expect("ok");
        assert!(out.group.is_some());
        assert_eq!(doc.scene().groups().len(), groups + 1, "the saved group is reused, not wrapped");
        let single = Prefab::parse("entity { \"classname\" \"info_target\" \"origin\" \"1 2 3\" }").expect("single");
        let out =
            doc.insert_prefab(&single, PrefabPlacement::Offset(DVec3::ZERO), &InsertOptions::default()).expect("ok");
        assert_eq!(out.group, None);
        assert!(!single.has_token());
        assert_eq!(doc.prefab_token(&single, 0), 1);
    }

    #[test]
    fn malformed_prefabs_never_panic() {
        assert!(Prefab::parse("").is_err());
        assert!(Prefab::parse("versioninfo { }").is_err());
        assert!(
            Prefab::parse("solid { side { \"plane\" \"(nan nan nan) (1 1 1) (2 2 2)\" } }")
                .is_ok_and(|p| p.bounds().is_none())
        );
        assert!(Prefab::parse("{{{{{{{{").is_err());
        let huge = "entity { \"classname\" \"x\" \"origin\" \"1e308 1e308 1e308\" }";
        let p = Prefab::parse(huge).expect("parses");
        assert!(p.bounds().is_none_or(|b| b.min.is_finite()));
        let missing = std::env::temp_dir().join("hp_editor_no_such_prefab.vmf");
        assert!(Prefab::load(&missing).is_err());
        let fit = fit_box(&Aabb::new(DVec3::ZERO, DVec3::ZERO), &Aabb::new(DVec3::ZERO, DVec3::ONE));
        assert_eq!(fit.min, DVec3::splat(0.5));
    }
}
