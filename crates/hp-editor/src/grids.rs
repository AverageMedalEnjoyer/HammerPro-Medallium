use hp_vmf::EditorInfo;

use crate::{Document, EditError, EditResult, Id, ObjRef, Owner};

pub const OBJECT_GRID_KEY: &str = "hammerpro_grid";

pub const GRIDS_BLOCK: &str = "hammerpro_grids";

pub const MAX_GRID_NAME: usize = 48;

const ATTACH_LABEL: &str = "Attach grid";
const DETACH_LABEL: &str = "Detach grid";

pub fn editor_grid(editor: &EditorInfo) -> Option<&str> {
    editor
        .extra_props
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(OBJECT_GRID_KEY))
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

fn set_editor_grid(editor: &mut EditorInfo, grid: Option<&str>) -> bool {
    let before = editor_grid(editor).map(str::to_owned);
    if before.as_deref() == grid {
        return false;
    }
    editor.extra_props.retain(|(k, _)| !k.eq_ignore_ascii_case(OBJECT_GRID_KEY));
    if let Some(name) = grid {
        editor.extra_props.push((OBJECT_GRID_KEY.to_owned(), name.to_owned()));
    }
    true
}

pub fn clean_grid_name(name: &str) -> String {
    let cleaned: String =
        name.chars().filter(|c| !matches!(c, '"' | '{' | '}' | '\r' | '\n' | '\t')).take(MAX_GRID_NAME).collect();
    cleaned.trim().to_owned()
}

impl Document {
    pub fn own_object_grid(&self, obj: ObjRef) -> Option<&str> {
        self.scene.editor_info(obj).and_then(editor_grid)
    }

    pub fn group_grid(&self, group: Id) -> Option<&str> {
        self.scene.groups.get(&group).and_then(|g| editor_grid(&g.group.editor))
    }

    fn group_chain_grid(&self, mut group: Option<Id>) -> Option<&str> {
        for _ in 0..64 {
            let id = group?;
            let node = self.scene.groups.get(&id)?;
            if let Some(name) = editor_grid(&node.group.editor) {
                return Some(name);
            }
            group = node.group.editor.group_id;
        }
        None
    }

    pub fn object_grid(&self, obj: ObjRef) -> Option<&str> {
        let editor = self.scene.editor_info(obj)?;
        if let Some(name) = editor_grid(editor) {
            return Some(name);
        }
        if let Some(name) = self.group_chain_grid(editor.group_id) {
            return Some(name);
        }
        if let ObjRef::Solid(id) = obj
            && let Some(node) = self.scene.solids.get(&id)
            && let Owner::Entity(e) = node.owner
        {
            return self.object_grid(ObjRef::Entity(e));
        }
        None
    }

    pub fn selection_grid(&self, limit: usize) -> Option<&str> {
        let sel = self.selection();
        sel.objects.iter().take(limit).find_map(|o| self.object_grid(*o))
    }

    pub fn grid_user_count(&self, name: &str) -> usize {
        let is = |e: &EditorInfo| editor_grid(e).is_some_and(|g| g.eq_ignore_ascii_case(name));
        self.scene.solids.values().filter(|n| is(&n.solid.editor)).count()
            + self.scene.entities.values().filter(|n| is(&n.entity.editor)).count()
            + self.scene.groups.values().filter(|n| is(&n.group.editor)).count()
    }

    pub fn set_object_grid(&mut self, objs: &[ObjRef], groups: &[Id], grid: Option<&str>) -> EditResult<usize> {
        let name = match grid {
            Some(g) => {
                let n = clean_grid_name(g);
                if n.is_empty() {
                    return Err(EditError::Invalid("a grid needs a name".to_owned()));
                }
                Some(n)
            }
            None => None,
        };
        let objs: Vec<ObjRef> = objs.iter().copied().filter(|o| self.scene.contains(*o)).collect();
        let groups: Vec<Id> = groups.iter().copied().filter(|g| self.scene.groups.contains_key(g)).collect();
        if objs.is_empty() && groups.is_empty() {
            return Err(EditError::Nothing("no objects to attach a grid to"));
        }
        let label = if name.is_some() { ATTACH_LABEL } else { DETACH_LABEL };
        self.tx(label, |doc| Ok(doc.set_object_grid_unchecked(&objs, &groups, name.as_deref())))
    }

    fn set_object_grid_unchecked(&mut self, objs: &[ObjRef], groups: &[Id], grid: Option<&str>) -> usize {
        let mut count = 0;
        let mut solids = Vec::new();
        for o in objs {
            match *o {
                ObjRef::Solid(id) => {
                    if let Some(n) = self.scene.solids.get(&id) {
                        let mut s = n.solid.clone();
                        if set_editor_grid(&mut s.editor, grid) {
                            solids.push((id, s));
                            count += 1;
                        }
                    }
                }
                ObjRef::Entity(id) => {
                    let changes = self
                        .scene
                        .entities
                        .get(&id)
                        .is_some_and(|n| editor_grid(&n.entity.editor).map(str::to_owned).as_deref() != grid);
                    if changes {
                        self.update_entity(id, |e| {
                            set_editor_grid(&mut e.editor, grid);
                        });
                        count += 1;
                    }
                }
            }
        }
        self.update_solids(solids);
        for id in groups {
            if let Some(node) = self.scene.groups.get(id) {
                let mut g = node.group.clone();
                if set_editor_grid(&mut g.editor, grid) {
                    self.put_group(g);
                    count += 1;
                }
            }
        }
        count
    }

    pub fn rename_object_grids(&mut self, old: &str, new: Option<&str>) -> usize {
        let is = |e: &EditorInfo| editor_grid(e).is_some_and(|g| g.eq_ignore_ascii_case(old));
        let objs: Vec<ObjRef> = self
            .scene
            .solids
            .values()
            .filter(|n| is(&n.solid.editor))
            .map(|n| ObjRef::Solid(n.solid.id))
            .chain(self.scene.entities.values().filter(|n| is(&n.entity.editor)).map(|n| ObjRef::Entity(n.entity.id)))
            .collect();
        let groups: Vec<Id> = self.scene.groups.values().filter(|n| is(&n.group.editor)).map(|n| n.group.id).collect();
        if objs.is_empty() && groups.is_empty() {
            return 0;
        }
        let new = new.map(clean_grid_name).filter(|n| !n.is_empty());
        let label = if new.is_some() { ATTACH_LABEL } else { DETACH_LABEL };
        self.tx(label, |doc| Ok(doc.set_object_grid_unchecked(&objs, &groups, new.as_deref()))).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_geom::PrimitiveParams;

    fn doc_with_box() -> (Document, Id) {
        let mut doc = Document::new_empty();
        let ids = doc.create_primitive(&PrimitiveParams::default(), Owner::World).unwrap();
        (doc, ids[0])
    }

    #[test]
    fn attach_detach_is_one_undo_step_and_survives_the_vmf() {
        let (mut doc, id) = doc_with_box();
        let solid = ObjRef::Solid(id);
        assert_eq!(doc.object_grid(solid), None);
        assert_eq!(doc.set_object_grid(&[solid], &[], Some(" Stairs ")).unwrap(), 1);
        assert_eq!(doc.object_grid(solid), Some("Stairs"));
        assert_eq!(doc.undo_label(), Some(ATTACH_LABEL));
        assert_eq!(doc.grid_user_count("stairs"), 1);
        let text = doc.to_vmf_string();
        assert!(text.contains("\"hammerpro_grid\" \"Stairs\""), "{text}");
        let back = Document::from_map(hp_vmf::parse(&text).unwrap());
        assert_eq!(back.object_grid(solid), Some("Stairs"));
        assert_eq!(doc.set_object_grid(&[solid], &[], Some("Stairs")).unwrap(), 0);
        assert!(doc.undo());
        assert_eq!(doc.object_grid(solid), None);
        assert!(doc.redo());
        assert_eq!(doc.set_object_grid(&[solid], &[], None).unwrap(), 1);
        assert_eq!(doc.undo_label(), Some(DETACH_LABEL));
        assert_eq!(doc.object_grid(solid), None);
        assert!(doc.set_object_grid(&[solid], &[], Some("  ")).is_err());
        assert!(doc.set_object_grid(&[ObjRef::Solid(999_999)], &[], Some("A")).is_err());
    }

    #[test]
    fn groups_and_brush_entities_pass_their_grid_on() {
        let (mut doc, id) = doc_with_box();
        let solid = ObjRef::Solid(id);
        let group = doc.group(&[solid]).unwrap();
        doc.set_object_grid(&[], &[group], Some("Roof")).unwrap();
        assert_eq!(doc.group_grid(group), Some("Roof"));
        assert_eq!(doc.object_grid(solid), Some("Roof"), "from the group");
        doc.set_object_grid(&[solid], &[], Some("Own")).unwrap();
        assert_eq!(doc.object_grid(solid), Some("Own"), "its own grid first");
        doc.select(&[solid], crate::SelectOp::Replace);
        assert_eq!(doc.selection_grid(16), Some("Own"));
        assert_eq!(doc.rename_object_grids("roof", Some("Attic")), 1);
        assert_eq!(doc.group_grid(group), Some("Attic"));
        assert_eq!(doc.rename_object_grids("own", None), 1);
        assert_eq!(doc.object_grid(solid), Some("Attic"));
        assert_eq!(doc.rename_object_grids("nothing", None), 0);
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_grid_name("  a\"b{c}\n "), "abc");
        assert_eq!(clean_grid_name(&"x".repeat(100)).len(), MAX_GRID_NAME);
        assert_eq!(clean_grid_name(" \" "), "");
    }
}
