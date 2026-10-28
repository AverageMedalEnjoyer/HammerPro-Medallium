use crate::scene::Owner;
use crate::{Document, Id, ObjRef};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DescKind {
    Solid,
    EntitySolid,
    BrushEntity,
    PointEntity,
    Group,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Description {
    pub kind: DescKind,
    pub id: Id,
    pub classname: String,
    pub targetname: String,
    pub faces: usize,
    pub solids: usize,
    pub group_size: usize,
}

impl Document {
    pub fn describe(&self, obj: ObjRef) -> Option<Description> {
        let scene = &self.scene;
        match obj {
            ObjRef::Solid(id) => {
                let node = scene.solids.get(&id)?;
                let (kind, classname, targetname) = match node.owner {
                    Owner::Entity(e) => {
                        let ent = scene.entities.get(&e).map(|n| &n.entity);
                        let class = ent.map(|e| e.classname.clone()).unwrap_or_default();
                        let name = ent.and_then(|e| e.get("targetname")).unwrap_or_default().to_owned();
                        (DescKind::EntitySolid, class, name)
                    }
                    Owner::World => (DescKind::Solid, String::new(), String::new()),
                };
                Some(Description {
                    kind,
                    id,
                    classname,
                    targetname,
                    faces: node.solid.sides.len(),
                    solids: 1,
                    group_size: 0,
                })
            }
            ObjRef::Entity(id) => {
                let node = scene.entities.get(&id)?;
                let solids = scene.entity_solid_ids(id);
                let faces = solids.iter().filter_map(|s| scene.solids.get(s)).map(|n| n.solid.sides.len()).sum();
                Some(Description {
                    kind: if solids.is_empty() { DescKind::PointEntity } else { DescKind::BrushEntity },
                    id,
                    classname: node.entity.classname.clone(),
                    targetname: node.entity.get("targetname").unwrap_or_default().to_owned(),
                    faces,
                    solids: solids.len(),
                    group_size: 0,
                })
            }
        }
    }

    pub fn describe_group(&self, group: Id) -> Option<Description> {
        self.scene.groups.get(&group)?;
        Some(Description {
            kind: DescKind::Group,
            id: group,
            classname: String::new(),
            targetname: String::new(),
            faces: 0,
            solids: 0,
            group_size: self.scene.group_members(group).len(),
        })
    }

    pub fn describe_selection(&self) -> Option<Description> {
        let objects = &self.selection().objects;
        let first = *objects.first()?;
        if objects.len() == 1 {
            return self.describe(first);
        }
        let root = self.scene.group_root(self.scene.object_group(first)?);
        let same_root =
            objects.iter().all(|o| self.scene.object_group(*o).is_some_and(|g| self.scene.group_root(g) == root));
        if !same_root {
            return None;
        }
        let desc = self.describe_group(root)?;
        (desc.group_size == objects.len()).then_some(desc)
    }

    pub fn redo_labels(&self) -> Vec<&str> {
        self.history.redo.iter().rev().map(|e| e.label.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec3;

    use super::*;
    use crate::tests::{cube, doc_with_cube};
    use crate::{Owner, SelectOp};

    #[test]
    fn describes_solids_and_entities() {
        let (mut doc, solid) = doc_with_cube();
        let d = doc.describe(ObjRef::Solid(solid)).expect("solid");
        assert_eq!((d.kind, d.id, d.faces, d.solids), (DescKind::Solid, solid, 6, 1));
        assert!(d.classname.is_empty() && d.targetname.is_empty());
        let light = doc
            .create_point_entity("light", DVec3::new(0.0, 0.0, 128.0), &[("targetname".into(), "lamp".into())])
            .expect("light");
        let d = doc.describe(ObjRef::Entity(light)).expect("light");
        assert_eq!((d.kind, d.classname.as_str(), d.targetname.as_str()), (DescKind::PointEntity, "light", "lamp"));
        assert_eq!((d.faces, d.solids), (0, 0));
        let other = doc.create_solid(cube(DVec3::new(128.0, 0.0, 0.0), 32.0), Owner::World).expect("cube");
        let door = doc.tie_to_entity(&[ObjRef::Solid(solid), ObjRef::Solid(other)], "func_door", &[]).expect("tie");
        let d = doc.describe(ObjRef::Entity(door)).expect("door");
        assert_eq!((d.kind, d.faces, d.solids), (DescKind::BrushEntity, 12, 2));
        let d = doc.describe(ObjRef::Solid(other)).expect("member");
        assert_eq!((d.kind, d.classname.as_str()), (DescKind::EntitySolid, "func_door"));
        assert!(doc.describe(ObjRef::Solid(99_999)).is_none());
        assert!(doc.describe(ObjRef::Entity(99_999)).is_none());
    }

    #[test]
    fn redo_labels_next_first() {
        let (mut doc, a) = doc_with_cube();
        doc.translate(&[ObjRef::Solid(a)], DVec3::X).expect("move");
        doc.create_solid(cube(DVec3::new(128.0, 0.0, 0.0), 32.0), Owner::World).expect("cube");
        assert!(doc.redo_labels().is_empty());
        let undo = doc.undo_labels().into_iter().map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(undo.len(), 2);
        assert!(doc.undo() && doc.undo());
        let redo: Vec<String> = doc.redo_labels().into_iter().map(str::to_owned).collect();
        assert_eq!(redo, undo, "the next redo is the oldest undone step");
    }

    #[test]
    fn describes_one_object_or_one_group() {
        let (mut doc, a) = doc_with_cube();
        assert!(doc.describe_selection().is_none(), "nothing selected");
        let b = doc.create_solid(cube(DVec3::new(128.0, 0.0, 0.0), 32.0), Owner::World).expect("cube");
        let c = doc.create_solid(cube(DVec3::new(256.0, 0.0, 0.0), 32.0), Owner::World).expect("cube");
        doc.select(&[ObjRef::Solid(a)], SelectOp::Replace);
        assert_eq!(doc.describe_selection().map(|d| d.id), Some(a));
        doc.select(&[ObjRef::Solid(a), ObjRef::Solid(b)], SelectOp::Replace);
        assert!(doc.describe_selection().is_none());
        let group = doc.group(&[ObjRef::Solid(a), ObjRef::Solid(b)]).expect("group");
        doc.select(&[ObjRef::Solid(a), ObjRef::Solid(b)], SelectOp::Replace);
        let d = doc.describe_selection().expect("group");
        assert_eq!((d.kind, d.id, d.group_size), (DescKind::Group, group, 2));
        doc.select(&[ObjRef::Solid(a), ObjRef::Solid(b), ObjRef::Solid(c)], SelectOp::Replace);
        assert!(doc.describe_selection().is_none());
        let outer = doc.group(&[ObjRef::Solid(a), ObjRef::Solid(b), ObjRef::Solid(c)]).expect("outer");
        doc.select(&[ObjRef::Solid(a), ObjRef::Solid(b), ObjRef::Solid(c)], SelectOp::Replace);
        let d = doc.describe_selection().expect("outer group");
        assert_eq!((d.id, d.group_size), (outer, 3));
        assert!(doc.describe_group(12_345).is_none());
    }
}
