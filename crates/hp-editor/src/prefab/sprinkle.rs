use glam::DVec3;
use hp_vmf::{EditorInfo, Entity};

use crate::document::{Document, is_reserved_key};
use crate::ops::{ENTITY_COLOR, clean_vec};
use crate::selection::ObjRef;
use crate::{EditError, EditResult, Id};

pub const MAX_SPRINKLE_ITEMS: usize = 100_000;

#[derive(Clone, Debug, PartialEq)]
pub struct SprinkleItem {
    pub classname: String,
    pub origin: DVec3,
    pub keyvalues: Vec<(String, String)>,
}

fn clean(keyvalues: &[(String, String)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::with_capacity(keyvalues.len() + 1);
    for (k, v) in keyvalues {
        let k = k.trim();
        if k.is_empty() || is_reserved_key(k) || k.eq_ignore_ascii_case("origin") {
            continue;
        }
        match out.iter_mut().find(|(ok, _)| ok.eq_ignore_ascii_case(k)) {
            Some(slot) => slot.1.clone_from(v),
            None => out.push((k.to_owned(), v.clone())),
        }
    }
    out
}

fn valid_class(class: &str) -> bool {
    !class.is_empty() && class.len() <= 256 && !class.chars().any(|c| c.is_whitespace() || matches!(c, '"' | '{' | '}'))
}

impl Document {
    pub fn sprinkle(&mut self, add: &[SprinkleItem], remove: &[Id]) -> EditResult<Vec<Id>> {
        if add.len() > MAX_SPRINKLE_ITEMS {
            return Err(EditError::Invalid("too many entities".to_owned()));
        }
        for item in add {
            if !valid_class(item.classname.trim()) {
                return Err(EditError::Invalid(format!("invalid class name \"{}\"", item.classname)));
            }
            if !item.origin.is_finite() {
                return Err(EditError::Invalid("origin is not finite".to_owned()));
            }
        }
        let removed: Vec<ObjRef> = remove
            .iter()
            .filter(|id| self.scene.entities.contains_key(*id) && !self.scene.is_brush_entity(**id))
            .map(|id| ObjRef::Entity(*id))
            .collect();
        if add.is_empty() && removed.is_empty() {
            return Ok(Vec::new());
        }
        let entities: Vec<Entity> = add
            .iter()
            .map(|item| {
                let mut props = clean(&item.keyvalues);
                props.push(("origin".to_owned(), hp_vmf::fmt_vec3(clean_vec(item.origin))));
                Entity {
                    classname: item.classname.trim().to_owned(),
                    props,
                    editor: EditorInfo { color: ENTITY_COLOR, ..EditorInfo::default() },
                    ..Entity::default()
                }
            })
            .collect();
        self.tx("Sprinkle", |doc| {
            if !removed.is_empty() {
                doc.delete(&removed)?;
            }
            let mut ids = Vec::with_capacity(entities.len());
            for mut e in entities {
                e.id = doc.alloc_object_id();
                ids.push(e.id);
                doc.add_entity(e);
            }
            Ok(ids)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(class: &str, x: f64) -> SprinkleItem {
        SprinkleItem {
            classname: class.to_owned(),
            origin: DVec3::new(x, 0.0, 0.0),
            keyvalues: vec![
                ("model".to_owned(), "models/a.mdl".to_owned()),
                ("origin".to_owned(), "9 9 9".to_owned()),
                ("classname".to_owned(), "nope".to_owned()),
                ("model".to_owned(), "models/b.mdl".to_owned()),
            ],
        }
    }

    #[test]
    fn adds_and_removes_in_one_step() {
        let mut doc = Document::new_empty();
        let ids = doc.sprinkle(&[item("prop_static", 1.0), item("prop_static", 2.0)], &[]).expect("add");
        assert_eq!(ids.len(), 2);
        assert_eq!(doc.undo_label(), Some("Sprinkle"));
        let e = &doc.scene().entity(ids[0]).expect("entity").entity;
        assert_eq!(e.classname, "prop_static");
        assert_eq!(e.get("model"), Some("models/b.mdl"));
        assert_eq!(e.get("origin"), Some("1 0 0"));
        assert!(doc.selected_objects().is_empty(), "the selection does not change");

        doc.set_selection(&[ObjRef::Entity(ids[0])]);
        doc.begin("Sprinkle");
        let more = doc.sprinkle(&[item("prop_static", 3.0)], &[ids[0]]).expect("stroke 1");
        doc.sprinkle(&[], &[ids[1], 999_999]).expect("stroke 2");
        assert!(doc.commit());
        assert_eq!(doc.scene().entities().len(), 1);
        assert!(doc.selected_objects().is_empty());
        assert!(doc.scene().entity(more[0]).is_some());
        assert!(doc.undo());
        assert_eq!(doc.scene().entities().len(), 2);

        let rev = doc.revision();
        assert!(doc.sprinkle(&[], &[]).expect("noop").is_empty());
        assert!(doc.sprinkle(&[item("bad class", 0.0)], &[]).is_err());
        assert!(doc.sprinkle(&[SprinkleItem { origin: DVec3::NAN, ..item("x", 0.0) }], &[]).is_err());
        assert_eq!(doc.revision(), rev);
    }
}
