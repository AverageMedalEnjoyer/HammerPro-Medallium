use hp_vmf::VisGroup;

use crate::selection::ObjRef;
use crate::visibility::find_visgroup;
use crate::{Document, EditError, EditResult, Id};

const QUICK_HIDE_PREFIX: &str = "_FromQuickHide";
const MAX_VISGROUP_NAME: usize = 128;

fn siblings_mut(groups: &mut Vec<VisGroup>, id: Id) -> Option<(&mut Vec<VisGroup>, usize)> {
    if let Some(pos) = groups.iter().position(|g| g.id == id) {
        return Some((groups, pos));
    }
    groups.iter_mut().find_map(|g| siblings_mut(&mut g.children, id))
}

fn sibling_position(groups: &[VisGroup], id: Id) -> Option<(usize, usize)> {
    if let Some(pos) = groups.iter().position(|g| g.id == id) {
        return Some((pos, groups.len()));
    }
    groups.iter().find_map(|g| sibling_position(&g.children, id))
}

fn name_used(groups: &[VisGroup], name: &str) -> bool {
    let mut stack: Vec<&VisGroup> = groups.iter().collect();
    while let Some(g) = stack.pop() {
        if g.name.eq_ignore_ascii_case(name) {
            return true;
        }
        stack.extend(g.children.iter());
    }
    false
}

fn clean_visgroup_name(name: &str) -> String {
    let s: String = name.chars().filter(|c| !c.is_control() && *c != '"').take(MAX_VISGROUP_NAME).collect();
    s.trim().to_owned()
}

impl Document {
    pub fn set_visgroups(&mut self, objs: &[ObjRef], ids: &[Id]) -> EditResult<usize> {
        let mut unique: Vec<Id> = Vec::with_capacity(ids.len());
        for id in ids {
            if self.visgroup(*id).is_none() {
                return Err(EditError::VisgroupNotFound(*id));
            }
            if !unique.contains(id) {
                unique.push(*id);
            }
        }
        self.tx("Set visgroups", |doc| Ok(doc.replace_visgroups_unchecked(objs, &unique)))
    }

    pub fn move_to_visgroup(
        &mut self,
        objs: &[ObjRef],
        target: Option<Id>,
        new_name: &str,
        remove_others: bool,
        hide: bool,
    ) -> EditResult<Id> {
        let objs: Vec<ObjRef> = objs.iter().copied().filter(|o| self.scene.contains(*o)).collect();
        if objs.is_empty() {
            return Err(EditError::Nothing("no objects to move"));
        }
        let name = clean_visgroup_name(new_name);
        match target {
            Some(id) if self.visgroup(id).is_none() => return Err(EditError::VisgroupNotFound(id)),
            None if name.is_empty() => return Err(EditError::Invalid("a visgroup needs a name".to_owned())),
            _ => {}
        }
        self.tx("Move to visgroup", |doc| {
            let id = match target {
                Some(id) => id,
                None => doc.create_visgroup(&name, None, &[])?,
            };
            if remove_others {
                doc.replace_visgroups_unchecked(&objs, &[id]);
            } else {
                doc.add_to_visgroup_unchecked(id, &objs);
            }
            if hide {
                doc.set_visgroup_visible(id, false)?;
                doc.deselect(&objs);
            }
            Ok(id)
        })
    }

    pub fn hidden_to_visgroup(&mut self) -> EditResult<Option<(Id, usize)>> {
        let mut objs: Vec<ObjRef> =
            self.scene.solids.values().filter(|n| n.solid.hidden).map(|n| ObjRef::Solid(n.solid.id)).collect();
        objs.extend(self.scene.entities.values().filter(|n| n.entity.hidden).map(|n| ObjRef::Entity(n.entity.id)));
        if objs.is_empty() {
            return Ok(None);
        }
        let tree = &self.scene.visgroups;
        let name = (1..)
            .map(|n: u64| format!("{QUICK_HIDE_PREFIX}({n})"))
            .find(|n| !name_used(tree, n))
            .unwrap_or_else(|| QUICK_HIDE_PREFIX.to_owned());
        let count = objs.len();
        self.tx("Visgroup from hidden objects", |doc| {
            let id = doc.create_visgroup(&name, None, &objs)?;
            doc.set_visgroup_visible(id, false)?;
            doc.set_hidden(&objs, false);
            Ok(Some((id, count)))
        })
    }

    pub fn can_reorder_visgroup(&self, id: Id, delta: i32) -> bool {
        let Some((pos, len)) = sibling_position(&self.scene.visgroups, id) else { return false };
        let target = pos as i64 + i64::from(delta);
        delta != 0 && target >= 0 && target < len as i64
    }

    pub fn reorder_visgroup(&mut self, id: Id, delta: i32) -> EditResult<bool> {
        if find_visgroup(&self.scene.visgroups, id).is_none() {
            return Err(EditError::VisgroupNotFound(id));
        }
        let Some((pos, len)) = sibling_position(&self.scene.visgroups, id) else { return Ok(false) };
        let max = len.saturating_sub(1) as i64;
        let target = (pos as i64 + i64::from(delta)).clamp(0, max) as usize;
        if target == pos {
            return Ok(false);
        }
        self.tx("Reorder visgroup", |doc| {
            let (tree, _) = doc.visgroups_mut();
            if let Some((list, at)) = siblings_mut(tree, id) {
                let g = list.remove(at);
                let to = target.min(list.len());
                list.insert(to, g);
            }
            Ok(true)
        })
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec3;

    use super::*;
    use crate::tests::{check_undo_redo, cube};
    use crate::{Owner, SelectOp};

    fn doc() -> (Document, Id, Id, Id) {
        let mut doc = Document::new_empty();
        let a = doc.create_solid(cube(DVec3::ZERO, 16.0), Owner::World).expect("a");
        let b = doc.create_solid(cube(DVec3::new(32.0, 0.0, 0.0), 16.0), Owner::World).expect("b");
        let e = doc.create_brush_entity("func_wall", &[], vec![cube(DVec3::new(64.0, 0.0, 0.0), 16.0)]).expect("e");
        doc.clear_selection();
        doc.clear_history();
        (doc, a, b, e)
    }

    fn names(doc: &Document) -> Vec<String> {
        doc.scene().visgroups().iter().map(|g| g.name.clone()).collect()
    }

    #[test]
    fn set_visgroups_replaces_memberships() {
        let (mut doc, a, b, e) = doc();
        let v1 = doc.create_visgroup("one", None, &[ObjRef::Solid(a)]).expect("v1");
        let v2 = doc.create_visgroup("two", None, &[]).expect("v2");
        let objs = [ObjRef::Solid(a), ObjRef::Solid(b), ObjRef::Entity(e)];
        let n = check_undo_redo(&mut doc, |d| d.set_visgroups(&objs, &[v2, v2])).expect("set");
        assert_eq!(n, 3);
        assert_eq!(doc.visgroup_members(v1), Vec::new());
        assert_eq!(doc.visgroup_members(v2).len(), 3);
        assert_eq!(doc.undo_label(), Some("Set visgroups"));
        doc.set_visgroup_visible(v1, false).expect("hide");
        doc.set_visgroups(&[ObjRef::Solid(b)], &[v1]).expect("set");
        assert!(!doc.is_solid_visible(b));
        assert!(matches!(doc.set_visgroups(&objs, &[999]), Err(EditError::VisgroupNotFound(999))));
        doc.set_visgroups(&objs, &[]).expect("clear");
        assert!(doc.visgroup_members(v2).is_empty() && doc.is_solid_visible(b));
    }

    #[test]
    fn move_to_a_new_or_existing_visgroup() {
        let (mut doc, a, b, _) = doc();
        let old = doc.create_visgroup("old", None, &[ObjRef::Solid(a), ObjRef::Solid(b)]).expect("old");
        doc.select(&[ObjRef::Solid(a)], SelectOp::Replace);
        doc.clear_history();
        let id = check_undo_redo(&mut doc, |d| d.move_to_visgroup(&[ObjRef::Solid(a)], None, " Walls ", true, true))
            .expect("move");
        assert_eq!(doc.visgroup(id).map(|g| g.name.as_str()), Some("Walls"));
        assert_eq!(doc.visgroup_members(old), vec![ObjRef::Solid(b)], "removed from the other visgroups");
        assert!(!doc.is_solid_visible(a), "the visgroup is hidden");
        assert!(doc.selection().is_empty(), "hidden objects are deselected");
        assert_eq!(doc.history_len().0, 1, "one undo step");
        doc.move_to_visgroup(&[ObjRef::Solid(b)], Some(id), "", false, false).expect("move");
        let mut members = doc.visgroup_members(old);
        members.extend(doc.visgroup_members(id));
        assert_eq!(members.iter().filter(|o| **o == ObjRef::Solid(b)).count(), 2);
        assert!(!doc.is_solid_visible(b), "b joined a hidden visgroup");
        let before = doc.revision();
        assert!(doc.move_to_visgroup(&[], Some(id), "", true, false).is_err());
        assert!(doc.move_to_visgroup(&[ObjRef::Solid(a)], Some(4242), "", true, false).is_err());
        assert!(doc.move_to_visgroup(&[ObjRef::Solid(a)], None, " \n ", true, false).is_err());
        assert!(doc.move_to_visgroup(&[ObjRef::Solid(77_777)], None, "x", true, false).is_err());
        assert_eq!(doc.revision(), before);
    }

    #[test]
    fn hidden_objects_become_a_hidden_visgroup() {
        let (mut doc, a, b, e) = doc();
        assert_eq!(doc.hidden_to_visgroup().expect("none"), None);
        doc.set_hidden(&[ObjRef::Solid(a), ObjRef::Entity(e)], true);
        doc.create_visgroup("_FromQuickHide(1)", None, &[]).expect("taken");
        doc.clear_history();
        let (id, n) = check_undo_redo(&mut doc, |d| d.hidden_to_visgroup()).expect("convert").expect("some");
        assert_eq!(n, 2);
        assert_eq!(doc.visgroup(id).map(|g| g.name.as_str()), Some("_FromQuickHide(2)"));
        assert_eq!(doc.history_len().0, 1, "one undo step");
        assert!(!doc.scene().solid(a).expect("a").solid.hidden, "the hidden flag is cleared");
        assert!(!doc.is_solid_visible(a) && !doc.is_entity_visible(e), "still hidden, by the visgroup");
        assert!(doc.is_solid_visible(b));
        doc.set_visgroup_visible(id, true).expect("show");
        assert!(doc.is_solid_visible(a) && doc.is_entity_visible(e));
    }

    #[test]
    fn reorder_siblings() {
        let (mut doc, ..) = doc();
        let a = doc.create_visgroup("a", None, &[]).expect("a");
        let b = doc.create_visgroup("b", None, &[]).expect("b");
        let c = doc.create_visgroup("c", None, &[]).expect("c");
        let child1 = doc.create_visgroup("c1", Some(c), &[]).expect("c1");
        let child2 = doc.create_visgroup("c2", Some(c), &[]).expect("c2");
        doc.clear_history();
        assert!(!doc.can_reorder_visgroup(a, -1) && doc.can_reorder_visgroup(a, 1));
        assert!(!doc.can_reorder_visgroup(c, 1) && !doc.can_reorder_visgroup(c, 0));
        assert!(check_undo_redo(&mut doc, |d| d.reorder_visgroup(c, -1)).expect("up"));
        assert_eq!(names(&doc), vec!["a", "c", "b"]);
        assert!(doc.reorder_visgroup(a, 5).expect("down"));
        assert_eq!(names(&doc), vec!["c", "b", "a"]);
        assert!(!doc.reorder_visgroup(a, 1).expect("bottom"));
        assert!(doc.reorder_visgroup(child2, -1).expect("child"));
        let kids: Vec<Id> = doc.visgroup(c).expect("c").children.iter().map(|g| g.id).collect();
        assert_eq!(kids, vec![child2, child1]);
        assert_eq!(doc.visgroup_parent(child2).expect("parent"), Some(c));
        assert!(doc.reorder_visgroup(b, i32::MIN).expect("far"));
        assert_eq!(names(&doc)[0], "b");
        assert!(matches!(doc.reorder_visgroup(999, 1), Err(EditError::VisgroupNotFound(999))));
    }
}
