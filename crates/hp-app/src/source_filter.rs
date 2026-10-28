use std::collections::HashMap;
use std::path::Path;

use hp_fs::SourceOrigin;

fn fold_char(c: char, out: &mut String) {
    match c {
        'A'..='Z' => out.push(c.to_ascii_lowercase()),
        '\\' => out.push('/'),
        c if c.is_ascii() => out.push(c),
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => out.push('a'),
        'æ' | 'Æ' => out.push_str("ae"),
        'ç' | 'Ç' => out.push('c'),
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => out.push('e'),
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => out.push('i'),
        'ñ' | 'Ñ' => out.push('n'),
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => out.push('o'),
        'œ' | 'Œ' => out.push_str("oe"),
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => out.push('u'),
        'ý' | 'ÿ' | 'Ý' => out.push('y'),
        'ß' => out.push_str("ss"),
        c => out.extend(c.to_lowercase()),
    }
}

pub fn fold_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        fold_char(c, &mut out);
    }
    out
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SourceFilter {
    #[default]
    All,
    Game,
    Addons,
    Workshop,
    Source(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceGroup {
    Game,
    Addons,
    Workshop,
    Custom,
}

impl SourceGroup {
    pub fn of(origin: SourceOrigin) -> SourceGroup {
        match origin {
            SourceOrigin::Custom => SourceGroup::Custom,
            SourceOrigin::Addon => SourceGroup::Addons,
            SourceOrigin::Workshop => SourceGroup::Workshop,
            SourceOrigin::GameLoose
            | SourceOrigin::BaseGame
            | SourceOrigin::Mounted
            | SourceOrigin::Extra
            | SourceOrigin::Download => SourceGroup::Game,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SourceBadge {
    Base,
    Mounted,
    Loose,
    Addon,
    Workshop(Option<u64>),
    Custom,
    Other,
}

impl SourceBadge {
    pub fn of(origin: SourceOrigin, path: &Path) -> SourceBadge {
        match origin {
            SourceOrigin::BaseGame => SourceBadge::Base,
            SourceOrigin::Mounted => SourceBadge::Mounted,
            SourceOrigin::GameLoose => SourceBadge::Loose,
            SourceOrigin::Addon => SourceBadge::Addon,
            SourceOrigin::Workshop => SourceBadge::Workshop(workshop_id(path)),
            SourceOrigin::Custom => SourceBadge::Custom,
            SourceOrigin::Extra | SourceOrigin::Download => SourceBadge::Other,
        }
    }

    pub fn is_stock(&self) -> bool {
        matches!(self, SourceBadge::Base)
    }
}

pub fn workshop_id(path: &Path) -> Option<u64> {
    let parse = |s: &str| {
        (!s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse().ok()).flatten()
    };
    let mut components: Vec<&str> = path.iter().filter_map(|c| c.to_str()).collect();
    if let Some(i) = components.iter().position(|c| *c == "4000")
        && let Some(id) = components.get(i + 1).and_then(|c| parse(c))
    {
        return Some(id);
    }
    let stem = components.pop().and_then(|f| Path::new(f).file_stem().and_then(|s| s.to_str()))?;
    parse(stem).or_else(|| stem.rsplit('_').next().and_then(parse))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMeta {
    pub group: Option<SourceGroup>,
    pub folded_label: String,
    pub badge: Option<SourceBadge>,
}

#[derive(Clone, Debug, Default)]
pub struct SourceTable {
    map: HashMap<u32, SourceMeta>,
}

impl SourceTable {
    pub fn insert(&mut self, id: u32, group: Option<SourceGroup>, label: &str) {
        self.map.insert(id, SourceMeta { group, folded_label: fold_label(label), badge: None });
    }

    pub fn insert_with_badge(&mut self, id: u32, group: Option<SourceGroup>, label: &str, badge: SourceBadge) {
        self.map.insert(id, SourceMeta { group, folded_label: fold_label(label), badge: Some(badge) });
    }

    pub fn of_fs(fs: &hp_fs::GameFs) -> SourceTable {
        let mut t = SourceTable::default();
        for s in fs.sources() {
            t.insert_with_badge(s.id, Some(SourceGroup::of(s.origin)), &s.label, SourceBadge::of(s.origin, &s.path));
        }
        t
    }

    pub fn get(&self, id: u32) -> Option<&SourceMeta> {
        self.map.get(&id)
    }

    pub fn badge(&self, id: u32) -> Option<&SourceBadge> {
        self.map.get(&id).and_then(|m| m.badge.as_ref())
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn allows(&self, filter: SourceFilter, id: u32) -> bool {
        let group = || self.map.get(&id).and_then(|m| m.group);
        match filter {
            SourceFilter::All => true,
            SourceFilter::Source(s) => s == id,
            SourceFilter::Game => group() == Some(SourceGroup::Game),
            SourceFilter::Addons => group() == Some(SourceGroup::Addons),
            SourceFilter::Workshop => group() == Some(SourceGroup::Workshop),
        }
    }

    pub fn is_stock(&self, id: u32) -> bool {
        self.badge(id).is_some_and(SourceBadge::is_stock)
    }

    pub fn passes(&self, filter: SourceFilter, gmod_only: bool, id: u32) -> bool {
        self.allows(filter, id) && (!gmod_only || self.is_stock(id))
    }

    pub fn has_group(&self, group: SourceGroup) -> bool {
        self.map.values().any(|m| m.group == Some(group))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_fold() {
        assert_eq!(fold_label("Custom: Textures Été"), "custom: textures ete");
        assert_eq!(fold_label("Cœur\\X"), "coeur/x");
    }

    #[test]
    fn workshop_ids_from_paths() {
        let p = Path::new("D:/Steam/steamapps/workshop/content/4000/123456789/model_pack.gma");
        assert_eq!(workshop_id(p), Some(123_456_789));
        assert_eq!(workshop_id(Path::new("C:/cache/workshop/987654.gma")), Some(987_654));
        assert_eq!(workshop_id(Path::new("C:/cache/my_addon_555.bin")), Some(555));
        assert_eq!(workshop_id(Path::new("C:/addons/my_addon.gma")), None);
        assert_eq!(workshop_id(Path::new("")), None);
        assert_eq!(workshop_id(Path::new("C:/x/4000/notanumber/a.gma")), None);
        assert_eq!(workshop_id(Path::new("C:/x/123456789012345678901234567890.gma")), None);
    }

    #[test]
    fn badges_and_gmod_only() {
        let mut t = SourceTable::default();
        t.insert_with_badge(1, Some(SourceGroup::Game), "garrysmod.vpk", SourceBadge::Base);
        t.insert_with_badge(2, Some(SourceGroup::Game), "cstrike", SourceBadge::Mounted);
        t.insert_with_badge(3, Some(SourceGroup::Workshop), "Workshop: X", SourceBadge::Workshop(Some(5)));
        t.insert(4, Some(SourceGroup::Custom), "Custom: mine");
        assert!(t.passes(SourceFilter::All, true, 1));
        assert!(!t.passes(SourceFilter::All, true, 2), "mounted games are not stock content");
        assert!(!t.passes(SourceFilter::All, true, 3));
        assert!(!t.passes(SourceFilter::All, true, 4), "unknown badges are not stock");
        assert!(!t.passes(SourceFilter::All, true, 99));
        assert!(t.passes(SourceFilter::Game, false, 2));
        assert!(!t.passes(SourceFilter::Workshop, false, 2));
        assert!(t.passes(SourceFilter::Source(4), false, 4));
        assert!(t.has_group(SourceGroup::Workshop));
        assert!(!t.has_group(SourceGroup::Addons));
        assert_eq!(t.badge(3), Some(&SourceBadge::Workshop(Some(5))));
        assert_eq!(t.len(), 4);
    }
}
