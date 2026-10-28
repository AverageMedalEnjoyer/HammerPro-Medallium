use glam::DVec3;
use hp_vmf::{EditorInfo, Entity};

use crate::{Document, EditError, EditResult, Id, ObjRef, SelectOp};

pub const UNDO_GALLERY: &str = "Insert entity gallery";

pub const MAX_GALLERY: usize = 4096;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GalleryEntry {
    pub classname: String,
    pub keyvalues: Vec<(String, String)>,
}

pub fn gallery_positions(n: usize, center: DVec3, spacing: f64) -> Vec<DVec3> {
    if n == 0 {
        return Vec::new();
    }
    let cols = (n as f64).sqrt().ceil().max(1.0) as usize;
    let rows = n.div_ceil(cols);
    let width = (cols - 1) as f64 * spacing;
    let height = (rows - 1) as f64 * spacing;
    (0..n)
        .map(|i| {
            let (r, c) = (i / cols, i % cols);
            DVec3::new(
                center.x - width * 0.5 + c as f64 * spacing,
                center.y + height * 0.5 - r as f64 * spacing,
                center.z,
            )
        })
        .collect()
}

impl Document {
    pub fn insert_entity_gallery(
        &mut self,
        entries: &[GalleryEntry],
        center: DVec3,
        spacing: f64,
    ) -> EditResult<Vec<Id>> {
        let entries: Vec<&GalleryEntry> = entries.iter().filter(|e| !e.classname.trim().is_empty()).collect();
        if entries.is_empty() {
            return Err(EditError::Nothing("no entity class to place"));
        }
        if entries.len() > MAX_GALLERY {
            return Err(EditError::Invalid(format!("more than {MAX_GALLERY} classes")));
        }
        if !center.is_finite() || !spacing.is_finite() || spacing <= 0.0 {
            return Err(EditError::Invalid("bad gallery placement".to_owned()));
        }
        let positions = gallery_positions(entries.len(), center, spacing);
        self.tx(UNDO_GALLERY, |doc| {
            let mut ids = Vec::with_capacity(entries.len());
            for (entry, pos) in entries.iter().zip(positions) {
                let id = doc.alloc_object_id();
                let mut props: Vec<(String, String)> = entry
                    .keyvalues
                    .iter()
                    .filter(|(k, _)| {
                        !["id", "classname", "origin"].iter().any(|r| k.eq_ignore_ascii_case(r)) && !k.trim().is_empty()
                    })
                    .cloned()
                    .collect();
                props.push(("origin".to_owned(), hp_vmf::fmt_vec3(pos)));
                doc.add_entity(Entity {
                    id,
                    classname: entry.classname.trim().to_owned(),
                    props,
                    editor: EditorInfo { color: crate::ops::ENTITY_COLOR, ..EditorInfo::default() },
                    ..Entity::default()
                });
                ids.push(id);
            }
            let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Entity(*id)).collect();
            doc.select(&objs, SelectOp::Replace);
            Ok(ids)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_centered_and_square() {
        let p = gallery_positions(5, DVec3::new(100.0, 0.0, 16.0), 64.0);
        assert_eq!(p.len(), 5);
        assert_eq!(p[0], DVec3::new(36.0, 32.0, 16.0));
        assert_eq!(p[2], DVec3::new(164.0, 32.0, 16.0));
        assert_eq!(p[3], DVec3::new(36.0, -32.0, 16.0));
        assert!(gallery_positions(0, DVec3::ZERO, 64.0).is_empty());
    }

    #[test]
    fn the_gallery_is_one_undo_step() {
        let mut doc = Document::new_empty();
        let entries = vec![
            GalleryEntry { classname: "light".into(), keyvalues: vec![("_light".into(), "255 255 255 200".into())] },
            GalleryEntry {
                classname: "info_target".into(),
                keyvalues: vec![("origin".into(), "9 9 9".into()), ("id".into(), "5".into())],
            },
            GalleryEntry { classname: "  ".into(), keyvalues: Vec::new() },
        ];
        let ids = doc.insert_entity_gallery(&entries, DVec3::ZERO, 128.0).expect("placed");
        assert_eq!(ids.len(), 2);
        assert_eq!(doc.undo_label(), Some(UNDO_GALLERY));
        assert_eq!(doc.selected_objects().len(), 2);
        let light = &doc.scene().entity(ids[0]).expect("light").entity;
        assert_eq!(light.get("_light"), Some("255 255 255 200"));
        let target = &doc.scene().entity(ids[1]).expect("target").entity;
        assert_ne!(target.get("origin"), Some("9 9 9"), "the grid gives the origin");
        assert!(doc.undo());
        assert!(doc.scene().entity(ids[0]).is_none());
        assert!(doc.insert_entity_gallery(&[], DVec3::ZERO, 128.0).is_err());
        assert!(doc.insert_entity_gallery(&entries, DVec3::NAN, 128.0).is_err());
    }
}
