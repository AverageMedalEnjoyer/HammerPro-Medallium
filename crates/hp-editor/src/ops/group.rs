use hp_vmf::{EditorInfo, Group, Solid};

use super::solid_color;
use crate::document::Document;
use crate::ids::{IdSet, id_set};
use crate::scene::Owner;
use crate::selection::{ObjRef, SelectOp};
use crate::{EditError, EditResult, Id};

impl Document {
    fn units_of(&self, objs: &[ObjRef]) -> Vec<ObjRef> {
        let mut out = indexmap::IndexSet::<ObjRef, crate::IdBuildHasher>::default();
        for o in objs {
            if self.scene.contains(*o) {
                out.insert(match *o {
                    ObjRef::Solid(id) => self.scene.object_of_solid(id),
                    e => e,
                });
            }
        }
        out.into_iter().collect()
    }

    fn set_group_of(&mut self, obj: ObjRef, group: Option<Id>) {
        match obj {
            ObjRef::Solid(id) => {
                if let Some(n) = self.scene.solids.get(&id)
                    && n.solid.editor.group_id != group
                {
                    let mut s: Solid = n.solid.clone();
                    s.editor.group_id = group;
                    self.update_solid(id, s);
                }
            }
            ObjRef::Entity(id) => {
                self.update_entity(id, |e| e.editor.group_id = group);
            }
        }
    }

    pub fn group(&mut self, objs: &[ObjRef]) -> EditResult<Id> {
        let units = self.units_of(objs);
        if units.is_empty() {
            return Err(EditError::Nothing("nothing to group"));
        }
        let unit_set: indexmap::IndexSet<ObjRef, crate::IdBuildHasher> = units.iter().copied().collect();
        let mut nested_roots = id_set(0);
        let mut direct: Vec<ObjRef> = Vec::new();
        let mut old_groups = id_set(0);
        let index = self.scene.group_index();
        for u in &units {
            match self.scene.object_group(*u) {
                Some(g) => {
                    old_groups.insert(g);
                    let root = self.scene.group_root(g);
                    if nested_roots.contains(&root) {
                        continue;
                    }
                    let members = index.subtree_members(root);
                    if !members.is_empty() && members.iter().all(|m| unit_set.contains(m)) {
                        nested_roots.insert(root);
                    } else {
                        direct.push(*u);
                    }
                }
                None => direct.push(*u),
            }
        }
        self.tx("Group", |doc| {
            let id = doc.alloc_object_id();
            doc.put_group(Group { id, editor: EditorInfo { color: solid_color(id), ..EditorInfo::default() } });
            for root in &nested_roots {
                if let Some(n) = doc.scene.groups.get(root) {
                    let mut g = n.group.clone();
                    g.editor.group_id = Some(id);
                    doc.put_group(g);
                }
            }
            for u in &direct {
                doc.set_group_of(*u, Some(id));
            }
            let candidates: IdSet = old_groups.into_iter().filter(|g| !nested_roots.contains(g)).collect();
            doc.cleanup_empty_groups(candidates);
            doc.select(&units, SelectOp::Replace);
            Ok(id)
        })
    }

    pub fn ungroup(&mut self, objs: &[ObjRef]) -> EditResult<usize> {
        let units = self.units_of(objs);
        let mut roots = id_set(0);
        for u in &units {
            if let Some(g) = self.scene.object_group(*u) {
                roots.insert(self.scene.group_root(g));
            }
        }
        if roots.is_empty() {
            return Ok(0);
        }
        let count = roots.len();
        self.tx("Ungroup", |doc| {
            let in_roots = |g: Option<Id>| g.is_some_and(|g| roots.contains(&g));
            let solids: Vec<ObjRef> = doc
                .scene
                .solids
                .values()
                .filter(|n| n.owner == Owner::World && in_roots(n.solid.editor.group_id))
                .map(|n| ObjRef::Solid(n.solid.id))
                .collect();
            let entities: Vec<ObjRef> = doc
                .scene
                .entities
                .values()
                .filter(|n| in_roots(n.entity.editor.group_id))
                .map(|n| ObjRef::Entity(n.entity.id))
                .collect();
            let children: Vec<Group> = doc
                .scene
                .groups
                .values()
                .filter(|n| !roots.contains(&n.group.id) && in_roots(n.group.editor.group_id))
                .map(|n| {
                    let mut g = n.group.clone();
                    g.editor.group_id = None;
                    g
                })
                .collect();
            for o in solids.iter().chain(entities.iter()) {
                doc.set_group_of(*o, None);
            }
            for g in children {
                doc.put_group(g);
            }
            let roots: Vec<Id> = roots.iter().copied().collect();
            doc.remove_groups(&roots);
            Ok(count)
        })
    }
}
