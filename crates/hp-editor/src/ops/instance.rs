use glam::DVec3;
use hp_vmf::{EditorInfo, Entity, Group, Map, Solid};

use crate::clipboard::SideLayout;
use crate::document::Document;
use crate::ids::{IdSet, id_set};
use crate::instances::{
    CollapseOptions, CollapseReport, ExpandOptions, Expansion, Fixup, INSTANCE_CLASS, InstanceIssue, InstanceSource,
    KeyFixupFn, expand, is_instance,
};
use crate::ops::Fragment;
use crate::selection::{ObjRef, SelectOp};
use crate::{EditError, EditResult, Id};

fn used_groups(solids: &[Solid], entities: &[Entity], groups: &[Group]) -> Vec<Group> {
    let mut used: IdSet = id_set(groups.len());
    let mut todo: Vec<Id> = solids
        .iter()
        .chain(entities.iter().flat_map(|e| e.solids.iter()))
        .filter_map(|s| s.editor.group_id)
        .chain(entities.iter().filter_map(|e| e.editor.group_id))
        .collect();
    while let Some(g) = todo.pop() {
        if !used.insert(g) {
            continue;
        }
        if let Some(parent) = groups.iter().find(|x| x.id == g).and_then(|x| x.editor.group_id) {
            todo.push(parent);
        }
    }
    groups.iter().filter(|g| used.contains(&g.id)).cloned().collect()
}

fn take_visgroups(e: &mut EditorInfo, visgroups: &[Id]) {
    e.visgroup_ids = visgroups.to_vec();
    e.visgroup_shown = true;
    e.visgroup_auto_shown = true;
}

impl Document {
    pub fn instance_entities(&self) -> Vec<Id> {
        let mut list: Vec<(u64, Id)> =
            self.scene.entities.values().filter(|n| is_instance(&n.entity)).map(|n| (n.order(), n.entity.id)).collect();
        list.sort_unstable();
        list.into_iter().map(|(_, id)| id).collect()
    }

    pub fn expand_instances(&self, source: &mut dyn InstanceSource, opts: &ExpandOptions<'_>) -> Vec<(Id, Expansion)> {
        let ids = self.instance_entities();
        let roots: Vec<&Entity> = ids.iter().filter_map(|id| self.scene.entities.get(id).map(|n| &n.entity)).collect();
        let main = opts.main_map.or(self.path());
        let opts = ExpandOptions { main_map: main, ..*opts };
        let exps = expand(&roots, source, &opts);
        roots.iter().map(|e| e.id).zip(exps).collect()
    }

    pub fn collapse_instances(
        &mut self,
        ids: &[Id],
        source: &mut dyn InstanceSource,
        opts: &CollapseOptions<'_>,
    ) -> EditResult<CollapseReport> {
        let wanted: IdSet = ids
            .iter()
            .copied()
            .filter(|id| self.scene.entities.get(id).is_some_and(|n| is_instance(&n.entity)))
            .collect();
        if wanted.is_empty() {
            return Err(EditError::Nothing("no instance to collapse"));
        }
        let expand_opts = ExpandOptions {
            skip_hidden: opts.skip_hidden,
            kinds: opts.kinds,
            main_map: self.path(),
            first_auto_index: 1,
        };
        let all = self.expand_instances(source, &expand_opts);
        let mut report = CollapseReport::default();
        let mut work: Vec<(Id, Expansion)> = Vec::new();
        for (id, exp) in all {
            if !wanted.contains(&id) {
                continue;
            }
            if exp.is_pending() {
                return Err(EditError::Invalid("instance files are still loading".to_owned()));
            }
            if !exp.is_placed() {
                let issue = exp.issues.first().cloned().unwrap_or(InstanceIssue::NoFile);
                report.skipped.push((id, issue));
                continue;
            }
            if let Some(issue) = exp.issues.first() {
                report.warnings.push((id, issue.clone()));
            }
            work.push((id, exp));
        }
        if work.is_empty() {
            return Ok(report);
        }
        let kinds = opts.kinds;
        let skip_hidden = opts.skip_hidden;
        self.tx("Collapse instances", |doc| {
            let mut created: Vec<ObjRef> = Vec::new();
            for (id, exp) in &work {
                let visgroups: Vec<Id> =
                    doc.scene.entities.get(id).map(|n| n.entity.editor.visgroup_ids.clone()).unwrap_or_default();
                for piece in &exp.pieces {
                    let solids = piece.world_solids(skip_hidden);
                    let entities = piece.entities(kinds, skip_hidden);
                    created.extend(doc.insert_instance_objects(solids, entities, piece.groups(), &visgroups)?);
                }
                if !exp.unresolved.is_empty() {
                    created.extend(doc.insert_instance_objects(Vec::new(), exp.unresolved.clone(), &[], &visgroups)?);
                }
                doc.delete(&[ObjRef::Entity(*id)])?;
                report.collapsed.push(*id);
            }
            created.retain(|o| doc.scene.contains(*o));
            doc.select(&created, SelectOp::Replace);
            report.objects = created;
            Ok(())
        })?;
        Ok(report)
    }

    fn insert_instance_objects(
        &mut self,
        mut solids: Vec<Solid>,
        mut entities: Vec<Entity>,
        groups: &[Group],
        visgroups: &[Id],
    ) -> EditResult<Vec<ObjRef>> {
        for s in &mut solids {
            take_visgroups(&mut s.editor, visgroups);
        }
        for e in &mut entities {
            take_visgroups(&mut e.editor, visgroups);
            for s in &mut e.solids {
                take_visgroups(&mut s.editor, visgroups);
            }
        }
        let mut groups = used_groups(&solids, &entities, groups);
        for g in &mut groups {
            take_visgroups(&mut g.editor, visgroups);
        }
        let frag = Fragment { solids, entities, groups, visgroups: Vec::new() };
        if frag.is_empty() {
            return Ok(Vec::new());
        }
        let layout = SideLayout::of(&frag);
        let objs = self.instantiate(frag, false)?;
        if let Some(layout) = &layout {
            self.remap_side_lists(layout, &objs);
        }
        Ok(objs)
    }

    pub fn instance_map(&self, objs: &[ObjRef], origin: DVec3, kinds: Option<KeyFixupFn<'_>>) -> EditResult<Map> {
        if !origin.is_finite() {
            return Err(EditError::Invalid("origin is not finite".to_owned()));
        }
        let frag = self.extract_fragment(objs);
        if frag.is_empty() {
            return Err(EditError::Nothing("nothing selected"));
        }
        let fix = Fixup::translation(-origin);
        let clear = |e: &mut EditorInfo| {
            e.visgroup_ids.clear();
            e.visgroup_shown = true;
            e.visgroup_auto_shown = true;
        };
        let mut map = Map::new_empty();
        for mut s in frag.solids {
            fix.fix_solid(&mut s)?;
            clear(&mut s.editor);
            map.world.solids.push(s);
        }
        for mut e in frag.entities {
            for s in &mut e.solids {
                fix.fix_solid(s)?;
                clear(&mut s.editor);
            }
            fix.fix_entity(&mut e, kinds);
            clear(&mut e.editor);
            map.entities.push(e);
        }
        for mut g in frag.groups {
            clear(&mut g.editor);
            map.world.groups.push(g);
        }
        Ok(map)
    }

    pub fn replace_with_instance(&mut self, objs: &[ObjRef], file: &str, origin: DVec3) -> EditResult<Id> {
        let file = file.trim();
        if file.is_empty() || file.contains('"') {
            return Err(EditError::Invalid("invalid instance file name".to_owned()));
        }
        if !origin.is_finite() {
            return Err(EditError::Invalid("origin is not finite".to_owned()));
        }
        let objs: Vec<ObjRef> = objs.iter().copied().filter(|o| self.scene.contains(*o)).collect();
        if objs.is_empty() {
            return Err(EditError::Nothing("nothing selected"));
        }
        let keys = [
            ("angles".to_owned(), "0 0 0".to_owned()),
            ("file".to_owned(), file.to_owned()),
            ("fixup_style".to_owned(), "0".to_owned()),
        ];
        self.tx("Create instance", |doc| {
            doc.delete(&objs)?;
            doc.create_point_entity(INSTANCE_CLASS, origin, &keys)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use super::*;
    use crate::instances::{LoadRequest, Loaded};
    use hp_vmf::Connection;

    fn child() -> Arc<Map> {
        let text = r#"
world
{
	"id" "1"
	"classname" "worldspawn"
	solid
	{
		"id" "2"
		side { "id" "1" "plane" "(0 16 16) (16 16 16) (16 0 16)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "2" "plane" "(0 0 0) (16 0 0) (16 16 0)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "3" "plane" "(16 16 16) (0 16 16) (0 16 0)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "4" "plane" "(16 0 0) (0 0 0) (0 0 16)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "5" "plane" "(16 16 0) (16 0 0) (16 0 16)" "material" "DEV/A" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "6" "plane" "(0 16 16) (0 0 16) (0 0 0)" "material" "DEV/A" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		editor { "color" "0 255 0" "visgroupid" "7" "groupid" "40" "visgroupshown" "1" "visgroupautoshown" "1" }
	}
	group { "id" "40" editor { "color" "1 2 3" "visgroupshown" "1" "visgroupautoshown" "1" } }
	group { "id" "41" editor { "color" "1 2 3" "visgroupshown" "1" "visgroupautoshown" "1" } }
}
entity
{
	"id" "3"
	"classname" "info_overlay"
	"origin" "8 8 16"
	"BasisOrigin" "8 8 16"
	"BasisNormal" "0 0 1"
	"sides" "1"
	"targetname" "decal"
}
entity
{
	"id" "4"
	"classname" "func_instance"
	"file" "nested.vmf"
	"origin" "0 0 32"
	"targetname" "sub"
}
entity
{
	"id" "5"
	"classname" "func_instance_parms"
	"parm1" "$x string"
}
"#;
        Arc::new(hp_vmf::parse(text).expect("child"))
    }

    fn nested() -> Arc<Map> {
        let mut m = Map::new_empty();
        let mut relay = Entity { id: 9, classname: "logic_relay".into(), ..Entity::default() };
        relay.set("targetname", "relay");
        relay.set("origin", "0 0 0");
        relay.connections.push(Connection {
            output: "OnTrigger".into(),
            target: "decal".into(),
            input: "Kill".into(),
            ..Connection::default()
        });
        m.entities.push(relay);
        Arc::new(m)
    }

    fn source() -> impl FnMut(&LoadRequest<'_>) -> Loaded {
        let (c, n) = (child(), nested());
        move |req: &LoadRequest<'_>| match req.file {
            "room.vmf" => Loaded::Ready { path: Arc::from(Path::new("/m/room.vmf")), map: Arc::clone(&c) },
            "nested.vmf" => Loaded::Ready { path: Arc::from(Path::new("/m/nested.vmf")), map: Arc::clone(&n) },
            "later.vmf" => Loaded::Pending,
            _ => Loaded::Missing,
        }
    }

    fn doc_with_instances() -> (Document, Id, Id) {
        let mut doc = Document::new_empty();
        doc.visgroups_mut().0.push(hp_vmf::VisGroup { name: "Inst".into(), id: 3, ..hp_vmf::VisGroup::default() });
        let a = doc
            .create_point_entity(
                "func_instance",
                DVec3::new(100.0, 0.0, 0.0),
                &[("file".into(), "room.vmf".into()), ("angles".into(), "0 90 0".into())],
            )
            .unwrap();
        let b =
            doc.create_point_entity("func_instance", DVec3::ZERO, &[("file".into(), "missing.vmf".into())]).unwrap();
        doc.edit_entity(a, |e| e.editor.visgroup_ids = vec![3]).unwrap();
        (doc, a, b)
    }

    #[test]
    fn collapse_replaces_instances_in_one_step() {
        let (mut doc, a, b) = doc_with_instances();
        let mut src = source();
        let before = doc.undo_labels().len();
        let report = doc.collapse_instances(&[a, b], &mut src, &CollapseOptions::default()).unwrap();
        assert_eq!(report.collapsed, vec![a]);
        assert_eq!(report.skipped, vec![(b, InstanceIssue::Missing { file: "missing.vmf".into(), depth: 0 })]);
        assert_eq!(doc.undo_labels().len(), before + 1);
        assert_eq!(doc.undo_label(), Some("Collapse instances"));
        assert!(doc.scene().entity(a).is_none());
        assert!(doc.scene().entity(b).is_some());
        let solid = doc.scene().solids().values().next().unwrap();
        assert!((solid.bounds.min - DVec3::new(84.0, 0.0, 0.0)).length() < 1e-6, "{:?}", solid.bounds);
        assert_eq!(solid.solid.editor.visgroup_ids, vec![3]);
        assert_eq!(doc.scene().groups().len(), 1);
        let names: Vec<String> =
            doc.scene().entities().values().filter_map(|n| n.entity.get("targetname").map(str::to_owned)).collect();
        assert!(names.contains(&"InstanceAuto1-decal".to_owned()), "{names:?}");
        assert!(names.contains(&"InstanceAuto1-sub-relay".to_owned()), "{names:?}");
        let relay = doc.scene().entities().values().find(|n| n.entity.classname == "logic_relay").unwrap();
        assert_eq!(relay.entity.connections[0].target, "InstanceAuto1-sub-decal");
        assert_eq!(relay.entity.get("origin"), Some("100 0 32"));
        let overlay = doc.scene().entities().values().find(|n| n.entity.classname == "info_overlay").unwrap();
        let side_id: Id = overlay.entity.get("sides").unwrap().parse().unwrap();
        assert!(solid.solid.sides.iter().any(|s| s.id == side_id));
        assert!(!doc.scene().entities().values().any(|n| n.entity.classname == "func_instance_parms"));
        assert_eq!(report.objects.len(), doc.selected_objects().len());
        assert!(doc.undo());
        assert!(doc.scene().entity(a).is_some());
        assert!(doc.scene().solids().is_empty());
    }

    #[test]
    fn collapse_waits_for_loading_files() {
        let (mut doc, a, _) = doc_with_instances();
        doc.edit_entity(a, |e| e.set("file", "later.vmf")).unwrap();
        let mut src = source();
        assert!(doc.collapse_instances(&[a], &mut src, &CollapseOptions::default()).is_err());
        assert!(doc.collapse_instances(&[], &mut src, &CollapseOptions::default()).is_err());
    }

    #[test]
    fn create_instance_from_selection() {
        let mut doc = Document::new_empty();
        let ids = doc.create_primitive(&hp_geom::PrimitiveParams::default(), crate::Owner::World).unwrap();
        let light = doc.create_point_entity("light", DVec3::new(10.0, 20.0, 30.0), &[]).unwrap();
        let objs = vec![ObjRef::Solid(ids[0]), ObjRef::Entity(light)];
        let origin = DVec3::new(0.0, 0.0, 30.0);
        let map = doc.instance_map(&objs, origin, None).unwrap();
        assert_eq!(map.world.solids.len(), 1);
        assert_eq!(map.entities[0].get("origin"), Some("10 20 0"));
        let text = hp_vmf::write(&map);
        assert!(hp_vmf::parse(&text).is_ok());
        let id = doc.replace_with_instance(&objs, "instances/x.vmf", origin).unwrap();
        assert_eq!(doc.undo_label(), Some("Create instance"));
        assert!(doc.scene().solids().is_empty());
        let e = &doc.scene().entity(id).unwrap().entity;
        assert_eq!(e.classname, "func_instance");
        assert_eq!(e.get("file"), Some("instances/x.vmf"));
        assert_eq!(e.get("origin"), Some("0 0 30"));
        assert_eq!(doc.selected_objects(), vec![ObjRef::Entity(id)]);
        assert!(doc.undo());
        assert_eq!(doc.scene().solids().len(), 1);
        assert!(doc.replace_with_instance(&[], "x.vmf", origin).is_err());
        assert!(doc.replace_with_instance(&objs, " ", origin).is_err());
        assert!(doc.instance_map(&objs, DVec3::NAN, None).is_err());
    }
}
