use std::collections::HashSet;

use crate::document::Document;
use crate::{EditError, EditResult, Id};

const KNOWN_PATH_CLASSES: &[(&str, &str)] = &[
    ("path_track", "target"),
    ("path_corner", "target"),
    ("path_corner_crash", "target"),
    ("keyframe_rope", "NextKey"),
    ("move_rope", "NextKey"),
    ("keyframe_track", "NextKey"),
];

const MAX_NAME_LEN: usize = 256;

pub(crate) fn next_node_name(name: &str, taken: &dyn Fn(&str) -> bool) -> Option<String> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_NAME_LEN {
        return None;
    }
    let digits = name.bytes().rev().take_while(u8::is_ascii_digit).count();
    let (prefix, number) = name.split_at(name.len() - digits);
    let width = number.len();
    let tail = number.get(number.len().saturating_sub(18)..).unwrap_or("");
    let mut n: u64 = tail.parse().unwrap_or(0);
    for _ in 0..100_000 {
        n = n.checked_add(1)?;
        let candidate = format!("{prefix}{n:0width$}");
        if !taken(&candidate.to_ascii_lowercase()) {
            return Some(candidate);
        }
    }
    None
}

impl Document {
    pub fn known_path_next_key(classname: &str) -> Option<&'static str> {
        let c = classname.trim();
        KNOWN_PATH_CLASSES.iter().find(|(k, _)| k.eq_ignore_ascii_case(c)).map(|(_, key)| *key)
    }

    pub fn link_cloned_path_node(&mut self, src: Id, clone: Id, next_key: &str) -> EditResult<bool> {
        let next_key = next_key.trim();
        if next_key.is_empty() || crate::document::is_reserved_key(next_key) {
            return Err(EditError::Invalid(format!("key '{next_key}' cannot name the next node")));
        }
        let source = self.scene.entities.get(&src).ok_or(EditError::NotFound(crate::ObjRef::Entity(src)))?;
        if !self.scene.entities.contains_key(&clone) || src == clone {
            return Err(EditError::NotFound(crate::ObjRef::Entity(clone)));
        }
        let Some(name) = source.entity.get("targetname").map(str::trim).filter(|n| !n.is_empty()) else {
            return Ok(false);
        };
        let next = source.entity.get(next_key).map(str::to_owned);
        let used: HashSet<String> = self
            .scene
            .entities
            .values()
            .filter(|n| n.entity.id != clone)
            .filter_map(|n| n.entity.get("targetname").map(|t| t.trim().to_ascii_lowercase()))
            .collect();
        let Some(new_name) = next_node_name(name, &|c| used.contains(c)) else {
            return Ok(false);
        };
        self.tx("Duplicate", |doc| {
            doc.update_entity(clone, |e| {
                e.set("targetname", new_name.clone());
                match &next {
                    Some(n) => e.set(next_key, n.clone()),
                    None => {
                        e.remove(next_key);
                    }
                }
            });
            doc.update_entity(src, |e| e.set(next_key, new_name.clone()));
            Ok(true)
        })
    }
}

#[cfg(test)]
mod tests {
    use glam::{DMat4, DVec3};

    use super::*;
    use crate::ObjRef;

    #[test]
    fn names() {
        let none = |_: &str| false;
        assert_eq!(next_node_name("track01", &none).as_deref(), Some("track02"));
        assert_eq!(next_node_name("track09", &none).as_deref(), Some("track10"));
        assert_eq!(next_node_name("track99", &none).as_deref(), Some("track100"));
        assert_eq!(next_node_name("path", &none).as_deref(), Some("path1"));
        assert_eq!(next_node_name("p_7", &|c| c == "p_8").as_deref(), Some("p_9"));
        assert_eq!(next_node_name("  ", &none), None);
        let long = format!("n{}", "9".repeat(40));
        assert!(next_node_name(&long, &none).is_some(), "very long numbers do not panic");
        assert_eq!(Document::known_path_next_key("PATH_TRACK"), Some("target"));
        assert_eq!(Document::known_path_next_key("keyframe_rope"), Some("NextKey"));
        assert_eq!(Document::known_path_next_key("light"), None);
    }

    #[test]
    fn a_cloned_node_is_inserted_after_its_source() {
        let text = r#"world { "id" "1" "classname" "worldspawn" }
entity { "id" "2" "classname" "path_track" "targetname" "t1" "target" "t2" "origin" "0 0 0" }
entity { "id" "3" "classname" "path_track" "targetname" "t2" "origin" "128 0 0" }
"#;
        let map = hp_vmf::parse(text).expect("map");
        let mut doc = Document::from_map(map);
        let before = doc.history_len().0;
        doc.begin("Duplicate");
        let new = doc
            .duplicate_transformed(
                &[ObjRef::Entity(2)],
                DMat4::from_translation(DVec3::new(64.0, 0.0, 0.0)),
                DVec3::ZERO,
                true,
            )
            .expect("clone");
        let ObjRef::Entity(clone) = new[0] else { panic!("entity") };
        assert!(doc.link_cloned_path_node(2, clone, "target").expect("link"));
        doc.commit();
        assert_eq!(doc.history_len().0, before + 1, "one undo step");
        let e = |id| doc.scene().entity(id).expect("entity").entity.clone();
        assert_eq!(e(clone).get("targetname"), Some("t3"));
        assert_eq!(e(clone).get("target"), Some("t2"));
        assert_eq!(e(2).get("target"), Some("t3"));
        assert!(doc.link_cloned_path_node(2, clone, "classname").is_err());
        assert!(doc.link_cloned_path_node(2, 999, "target").is_err());
        assert!(doc.undo());
        assert_eq!(doc.scene().entity(2).expect("entity").entity.get("target"), Some("t2"));
        assert!(doc.scene().entity(clone).is_none(), "the clone is undone with its links");
        doc.update_entity(3, |e| {
            e.remove("targetname");
        });
        assert_eq!(doc.link_cloned_path_node(3, 2, "target").ok(), Some(false));
    }
}
