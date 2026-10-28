use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use hp_editor::{Document, Id};

pub const MAX_RESOLVED: usize = 256;

fn lower(s: &str) -> Cow<'_, str> {
    if s.bytes().any(|b| b.is_ascii_uppercase()) { Cow::Owned(s.to_ascii_lowercase()) } else { Cow::Borrowed(s) }
}

fn push(map: &mut HashMap<Box<str>, Vec<Id>>, key: &str, id: Id) {
    let key = lower(key);
    match map.get_mut(key.as_ref()) {
        Some(list) => list.push(id),
        None => {
            map.insert(key.into_owned().into_boxed_str(), vec![id]);
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct NameIndex {
    revision: Option<u64>,
    fingerprint: u64,
    by_name: HashMap<Box<str>, Vec<Id>>,
    by_class: HashMap<Box<str>, Vec<Id>>,
}

fn fingerprint(doc: &Document) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for (id, node) in doc.scene().entities() {
        id.hash(&mut h);
        node.entity.classname.hash(&mut h);
        node.entity.get("targetname").unwrap_or("").hash(&mut h);
    }
    h.finish()
}

impl NameIndex {
    pub fn build(doc: &Document) -> Self {
        let mut out = NameIndex { revision: Some(doc.revision()), fingerprint: fingerprint(doc), ..Default::default() };
        for (id, node) in doc.scene().entities() {
            let e = &node.entity;
            push(&mut out.by_class, e.classname.trim(), *id);
            if let Some(name) = e.get("targetname").map(str::trim).filter(|n| !n.is_empty()) {
                push(&mut out.by_name, name, *id);
            }
        }
        out
    }

    pub fn refresh(&mut self, doc: &Document) -> bool {
        if self.revision == Some(doc.revision()) {
            return false;
        }
        let fp = fingerprint(doc);
        if self.revision.is_some() && fp == self.fingerprint {
            self.revision = Some(doc.revision());
            return false;
        }
        *self = Self::build(doc);
        true
    }

    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    pub fn named(&self, name: &str) -> &[Id] {
        self.by_name.get(lower(name.trim()).as_ref()).map_or(&[], Vec::as_slice)
    }

    pub fn of_class(&self, class: &str) -> &[Id] {
        self.by_class.get(lower(class.trim()).as_ref()).map_or(&[], Vec::as_slice)
    }

    pub fn resolve_name(&self, target: &str, out: &mut Vec<Id>) {
        let t = target.trim();
        if t.is_empty() || t.starts_with('!') {
            return;
        }
        let key = lower(t);
        if let Some(prefix) = key.strip_suffix('*') {
            let mut keys: Vec<&Box<str>> = self.by_name.keys().filter(|k| k.starts_with(prefix)).collect();
            keys.sort_unstable();
            for k in keys {
                for id in &self.by_name[k] {
                    if out.len() >= MAX_RESOLVED {
                        return;
                    }
                    out.push(*id);
                }
            }
            return;
        }
        if let Some(ids) = self.by_name.get(key.as_ref()) {
            out.extend(ids.iter().take(MAX_RESOLVED.saturating_sub(out.len())));
        }
    }

    pub fn resolve(&self, target: &str, out: &mut Vec<Id>) {
        let before = out.len();
        self.resolve_name(target, out);
        if out.len() == before && !target.trim().starts_with('!') {
            let ids = self.of_class(target);
            out.extend(ids.iter().take(MAX_RESOLVED.saturating_sub(out.len())));
        }
    }

    pub fn is_special(target: &str) -> bool {
        target.trim().starts_with('!')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_names_wildcards_and_classes() {
        let mut state = crate::testing::test_state();
        let mut names = NameIndex::build(&state.doc);
        assert_eq!(names.named("MAIN_LIGHT"), &[5]);
        let mut out = Vec::new();
        names.resolve_name("main_*", &mut out);
        assert_eq!(out, vec![5]);
        out.clear();
        names.resolve("light", &mut out);
        assert_eq!(out, vec![5], "class names resolve when no entity has that name");
        out.clear();
        names.resolve("!activator", &mut out);
        assert!(out.is_empty());
        names.resolve("nobody", &mut out);
        assert!(out.is_empty());
        assert!(NameIndex::is_special(" !self"));
        let fp = names.fingerprint();
        state.doc.translate(&[hp_editor::ObjRef::Solid(2)], glam::DVec3::X).expect("move");
        assert!(!names.refresh(&state.doc));
        assert_eq!(names.fingerprint(), fp);
        state.doc.edit_entity(5, |e| e.set("targetname", "lamp")).expect("rename");
        assert!(names.refresh(&state.doc));
        assert!(names.named("main_light").is_empty());
        assert_eq!(names.named("lamp"), &[5]);
        assert!(!names.refresh(&state.doc));
    }
}
