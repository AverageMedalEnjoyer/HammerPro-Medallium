use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use crate::model::{Helper, IoDef, Property};

const INDEX_THRESHOLD: usize = 128;

pub(crate) fn lower(name: &str) -> Cow<'_, str> {
    if name.bytes().any(|b| b.is_ascii_uppercase()) {
        Cow::Owned(name.to_ascii_lowercase())
    } else {
        Cow::Borrowed(name)
    }
}

pub(crate) trait Named {
    fn name(&self) -> &str;
}

impl Named for Property {
    fn name(&self) -> &str {
        &self.name
    }
}

impl Named for IoDef {
    fn name(&self) -> &str {
        &self.name
    }
}

pub(crate) struct NamedList<T> {
    items: Vec<T>,
    index: Option<HashMap<String, usize>>,
}

impl<T: Named> NamedList<T> {
    pub(crate) fn new(items: Vec<T>) -> Self {
        let mut list = Self { items, index: None };
        list.maybe_index();
        list
    }

    fn maybe_index(&mut self) {
        if self.index.is_none() && self.items.len() > INDEX_THRESHOLD {
            let mut map = HashMap::with_capacity(self.items.len() * 2);
            for (i, item) in self.items.iter().enumerate() {
                map.entry(item.name().to_ascii_lowercase()).or_insert(i);
            }
            self.index = Some(map);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub(crate) fn get_mut(&mut self, name: &str) -> Option<&mut T> {
        match &self.index {
            Some(map) => {
                let i = *map.get(lower(name).as_ref())?;
                self.items.get_mut(i)
            }
            None => self.items.iter_mut().find(|item| item.name().eq_ignore_ascii_case(name)),
        }
    }

    pub(crate) fn push(&mut self, item: T) {
        if let Some(map) = &mut self.index {
            map.entry(item.name().to_ascii_lowercase()).or_insert(self.items.len());
        }
        self.items.push(item);
        self.maybe_index();
    }

    pub(crate) fn upsert(&mut self, item: T) {
        match self.get_mut(item.name()) {
            Some(slot) => *slot = item,
            None => self.push(item),
        }
    }

    pub(crate) fn into_vec(self) -> Vec<T> {
        self.items
    }
}

pub(crate) fn singleton_group(name: &str) -> Option<&'static str> {
    const MODEL: [&str; 3] = ["studio", "studioprop", "lightprop"];
    const SINGLE: [&str; 14] = [
        "size",
        "color",
        "iconsprite",
        "sprite",
        "lightcone",
        "halfgridsnap",
        "keyframe",
        "animator",
        "decal",
        "overlay",
        "overlay_transition",
        "instance",
        "quadbounds",
        "worldtext",
    ];
    if MODEL.iter().any(|m| name.eq_ignore_ascii_case(m)) {
        return Some("studio");
    }
    SINGLE.iter().find(|s| name.eq_ignore_ascii_case(s)).copied()
}

fn helper_key(helper: &Helper) -> (String, Vec<String>) {
    (helper.name.to_ascii_lowercase(), helper.args.clone())
}

#[derive(Default)]
pub(crate) struct HelperList {
    items: Vec<Helper>,
    groups: Vec<(&'static str, usize)>,
    exact: Option<HashSet<(String, Vec<String>)>>,
}

impl HelperList {
    pub(crate) fn new(items: Vec<Helper>) -> Self {
        let mut list = Self::default();
        for helper in items {
            list.push_unique(helper);
        }
        list
    }

    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }

    fn contains(&self, helper: &Helper) -> bool {
        match &self.exact {
            Some(set) => set.contains(&helper_key(helper)),
            None => self.items.iter().any(|h| h.same_as(helper)),
        }
    }

    fn maybe_index(&mut self) {
        if self.exact.is_none() && self.items.len() > INDEX_THRESHOLD {
            self.exact = Some(self.items.iter().map(helper_key).collect());
        }
    }

    pub(crate) fn push_unique(&mut self, helper: Helper) {
        if self.contains(&helper) {
            return;
        }
        if let Some(group) = singleton_group(&helper.name)
            && !self.groups.iter().any(|(g, _)| *g == group)
        {
            self.groups.push((group, self.items.len()));
        }
        if let Some(set) = &mut self.exact {
            set.insert(helper_key(&helper));
        }
        self.items.push(helper);
        self.maybe_index();
    }

    pub(crate) fn add(&mut self, helper: &Helper) {
        if self.contains(helper) {
            return;
        }
        let slot = singleton_group(&helper.name)
            .and_then(|group| self.groups.iter().find(|(g, _)| *g == group))
            .map(|&(_, i)| i);
        match slot.and_then(|i| self.items.get_mut(i)) {
            Some(old) => {
                if let Some(set) = &mut self.exact {
                    set.remove(&helper_key(old));
                    set.insert(helper_key(helper));
                }
                *old = helper.clone();
            }
            None => self.push_unique(helper.clone()),
        }
    }

    pub(crate) fn into_vec(self) -> Vec<Helper> {
        self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PropertyType;

    #[test]
    fn named_list_small_and_indexed() {
        for count in [5usize, 200] {
            let mut list = NamedList::new(Vec::new());
            for i in 0..count {
                list.push(Property::new(format!("Key{i}"), PropertyType::String));
            }
            assert!(list.get_mut("KEY3").is_some());
            assert!(list.get_mut("missing").is_none());
            let mut replacement = Property::new("key4", PropertyType::Integer);
            replacement.display_name = "Four".into();
            list.upsert(replacement);
            list.upsert(Property::new("brand_new", PropertyType::Float));
            let items = list.into_vec();
            assert_eq!(items.len(), count + 1);
            assert_eq!(items[4].kind, PropertyType::Integer);
            assert_eq!(items.last().map(|p| p.name.as_str()), Some("brand_new"));
        }
    }

    #[test]
    fn helper_list_rules_small_and_indexed() {
        for filler in [0usize, 100] {
            let mut list = HelperList::default();
            for i in 0..filler {
                list.add(&Helper::new("sphere", vec![format!("r{i}")]));
            }
            list.add(&Helper::new("studio", vec![]));
            list.add(&Helper::new("size", vec!["-8 -8 -8".into(), "8 8 8".into()]));
            list.add(&Helper::new("StudioProp", vec![]));
            list.add(&Helper::new("size", vec!["-8 -8 -8".into(), "8 8 8".into()]));
            list.add(&Helper::new("sphere", vec!["r0".into()]));
            list.add(&Helper::new("size", vec!["16 16 16".into()]));
            let items = list.into_vec();
            let tail: Vec<_> = items.iter().skip(filler).map(|h| (h.name.as_str(), h.args.len())).collect();
            let expect_sphere = if filler == 0 { vec![("sphere", 1)] } else { vec![] };
            let mut expected = vec![("StudioProp", 0), ("size", 1)];
            expected.extend(expect_sphere);
            assert_eq!(tail, expected);
        }
    }
}
