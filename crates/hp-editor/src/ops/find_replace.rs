use hp_vmf::{CONNECTION_ESC, Connection, Entity};

use crate::document::is_reserved_key;
use crate::{Document, EditError, EditResult, Id};

const LABEL_REPLACE_TEXT: &str = "Replace text";

pub const MAX_TEXT_LEN: usize = 4096;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextQuery {
    pub find: String,
    pub whole_word: bool,
    pub case_sensitive: bool,
    pub key_names: bool,
    pub only: Option<Vec<Id>>,
    pub visible_only: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConnPart {
    Output,
    Target,
    Input,
    Param,
}

impl ConnPart {
    fn get(self, c: &Connection) -> &str {
        match self {
            ConnPart::Output => &c.output,
            ConnPart::Target => &c.target,
            ConnPart::Input => &c.input,
            ConnPart::Param => &c.param,
        }
    }

    fn get_mut(self, c: &mut Connection) -> &mut String {
        match self {
            ConnPart::Output => &mut c.output,
            ConnPart::Target => &mut c.target,
            ConnPart::Input => &mut c.input,
            ConnPart::Param => &mut c.param,
        }
    }

    const ALL: [ConnPart; 4] = [ConnPart::Output, ConnPart::Target, ConnPart::Input, ConnPart::Param];
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TextField {
    Value(String),
    KeyName(String),
    Connection { index: usize, part: ConnPart },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextMatch {
    pub entity: Id,
    pub world: bool,
    pub field: TextField,
    pub text: String,
    pub ranges: Vec<(usize, usize)>,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub fn find_ranges(text: &str, find: &str, case_sensitive: bool, whole_word: bool) -> Vec<(usize, usize)> {
    if find.is_empty() || find.len() > text.len() {
        return Vec::new();
    }
    let (hay, needle) = (text.as_bytes(), find.as_bytes());
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(start) = find_from(hay, needle, from, case_sensitive) {
        let end = start + needle.len();
        let ok = !whole_word || {
            let before = text[..start].chars().next_back();
            let after = text[end..].chars().next();
            !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
        };
        if ok {
            out.push((start, end));
            from = end;
        } else {
            from = start + text[start..].chars().next().map_or(1, char::len_utf8);
        }
        if from >= text.len() {
            break;
        }
    }
    out
}

fn find_from(hay: &[u8], needle: &[u8], from: usize, case_sensitive: bool) -> Option<usize> {
    let last = hay.len().checked_sub(needle.len())?;
    let first = *needle.first()?;
    (from..=last).find(|&i| {
        let window = &hay[i..i + needle.len()];
        if case_sensitive {
            hay[i] == first && window == needle
        } else {
            hay[i].eq_ignore_ascii_case(&first) && window.eq_ignore_ascii_case(needle)
        }
    })
}

fn replace_ranges(text: &str, ranges: &[(usize, usize)], with: &str) -> String {
    let mut out = String::with_capacity(text.len() + ranges.len() * with.len());
    let mut last = 0;
    for &(a, b) in ranges {
        out.push_str(&text[last..a]);
        out.push_str(with);
        last = b;
    }
    out.push_str(&text[last..]);
    out
}

fn entity_matches(e: &Entity, world: bool, q: &TextQuery, out: &mut Vec<TextMatch>) {
    let mut push = |field: TextField, text: &str| {
        let ranges = find_ranges(text, &q.find, q.case_sensitive, q.whole_word);
        if !ranges.is_empty() {
            out.push(TextMatch { entity: e.id, world, field, text: text.to_owned(), ranges });
        }
    };
    push(TextField::Value("classname".to_owned()), &e.classname);
    for (k, v) in &e.props {
        if q.key_names {
            push(TextField::KeyName(k.clone()), k);
        }
        push(TextField::Value(k.clone()), v);
    }
    for (index, c) in e.connections.iter().enumerate() {
        for part in ConnPart::ALL {
            push(TextField::Connection { index, part }, part.get(c));
        }
    }
}

fn checked_replacement(field: &TextField, text: &str, ranges: &[(usize, usize)], with: &str) -> Option<String> {
    let new = replace_ranges(text, ranges, with);
    if new == text || new.len() > MAX_TEXT_LEN {
        return None;
    }
    match field {
        TextField::Value(k) if k.eq_ignore_ascii_case("classname") => (!new.trim().is_empty()).then_some(new),
        TextField::KeyName(_) => (!new.trim().is_empty() && !is_reserved_key(new.trim())).then_some(new),
        _ => Some(new),
    }
}

fn apply_field(e: &mut Entity, field: &TextField, new: &str) -> bool {
    match field {
        TextField::Value(k) if k.eq_ignore_ascii_case("classname") => {
            let new = new.trim();
            if e.classname == new {
                return false;
            }
            new.clone_into(&mut e.classname);
            true
        }
        TextField::Value(k) => {
            let Some(slot) = e.props.iter_mut().find(|(pk, _)| pk == k) else { return false };
            if slot.1 == new {
                return false;
            }
            new.clone_into(&mut slot.1);
            true
        }
        TextField::KeyName(k) => {
            let new = new.trim();
            let Some(pos) = e.props.iter().position(|(pk, _)| pk == k) else { return false };
            let value = e.props.remove(pos).1;
            e.props.retain(|(pk, _)| !pk.eq_ignore_ascii_case(new));
            let at = pos.min(e.props.len());
            e.props.insert(at, (new.to_owned(), value));
            true
        }
        TextField::Connection { index, part } => {
            let Some(c) = e.connections.get_mut(*index) else { return false };
            let slot = part.get_mut(c);
            if slot == new {
                return false;
            }
            new.clone_into(slot);
            if new.contains(',') && c.separator == ',' {
                c.separator = CONNECTION_ESC;
            }
            true
        }
    }
}

impl Document {
    pub fn find_text(&self, q: &TextQuery) -> Vec<TextMatch> {
        let mut out = Vec::new();
        if q.find.is_empty() || q.find.len() > MAX_TEXT_LEN {
            return out;
        }
        let world = &self.scene.world;
        if q.only.is_none() || q.only.as_ref().is_some_and(|ids| ids.contains(&world.id)) {
            entity_matches(world, true, q, &mut out);
        }
        let only: Option<crate::IdSet> = q.only.as_ref().map(|ids| ids.iter().copied().collect());
        for n in self.scene.entities.values() {
            let id = n.entity.id;
            if only.as_ref().is_some_and(|set| !set.contains(&id)) {
                continue;
            }
            if q.visible_only && !self.is_entity_visible(id) {
                continue;
            }
            entity_matches(&n.entity, false, q, &mut out);
        }
        out
    }

    pub fn replace_text(&mut self, q: &TextQuery, matches: &[TextMatch], with: &str) -> EditResult<usize> {
        if with.contains('"') {
            return Err(EditError::Invalid("a key value cannot contain a double quote".to_owned()));
        }
        if with.contains(CONNECTION_ESC) || with.len() > MAX_TEXT_LEN {
            return Err(EditError::Invalid("invalid replacement text".to_owned()));
        }
        if q.find.is_empty() {
            return Ok(0);
        }
        let world_id = self.scene.world.id;
        let mut plan: Vec<(Id, bool, TextField, String)> = Vec::new();
        for m in matches {
            let entity = if m.world {
                (m.entity == world_id).then_some(&self.scene.world)
            } else {
                self.scene.entities.get(&m.entity).map(|n| &n.entity)
            };
            let Some(e) = entity else { continue };
            let current: Option<&str> = match &m.field {
                TextField::Value(k) if k.eq_ignore_ascii_case("classname") => Some(&e.classname),
                TextField::Value(k) => e.props.iter().find(|(pk, _)| pk == k).map(|(_, v)| v.as_str()),
                TextField::KeyName(k) => e.props.iter().find(|(pk, _)| pk == k).map(|(pk, _)| pk.as_str()),
                TextField::Connection { index, part } => e.connections.get(*index).map(|c| part.get(c)),
            };
            let Some(current) = current else { continue };
            let ranges = find_ranges(current, &q.find, q.case_sensitive, q.whole_word);
            if ranges.is_empty() {
                continue;
            }
            if let Some(new) = checked_replacement(&m.field, current, &ranges, with)
                && !plan.iter().any(|(id, w, f, _)| *id == m.entity && *w == m.world && *f == m.field)
            {
                plan.push((m.entity, m.world, m.field.clone(), new));
            }
        }
        if plan.is_empty() {
            return Ok(0);
        }
        self.tx(LABEL_REPLACE_TEXT, |doc| {
            let mut changed = 0;
            for (id, world, field, new) in &plan {
                let done = if *world {
                    let mut e = doc.scene.world.clone();
                    let ok = apply_field(&mut e, field, new);
                    if ok {
                        *doc.world_mut() = e;
                    }
                    ok
                } else {
                    let mut ok = false;
                    doc.update_entity(*id, |e| ok = apply_field(e, field, new));
                    ok
                };
                if done {
                    changed += 1;
                }
            }
            Ok(changed)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Document {
        let vmf = r#"
world
{
	"id" "1"
	"classname" "worldspawn"
	"skyname" "sky_day01_01"
	"message" "Door test"
}
entity
{
	"id" "2"
	"classname" "func_door"
	"targetname" "door_main"
	"speed" "100"
	connections
	{
		"OnOpen" "door_light,TurnOn,,0,-1"
		"OnClose" "door_light,TurnOff,door,0.5,1"
	}
}
entity
{
	"id" "3"
	"classname" "light"
	"targetname" "door_light"
	"origin" "0 0 0"
}
entity
{
	"id" "4"
	"classname" "info_target"
	"targetname" "doorway"
	"origin" "0 0 0"
}
"#;
        Document::from_map(hp_vmf::parse(vmf).expect("test map"))
    }

    fn q(find: &str) -> TextQuery {
        TextQuery { find: find.to_owned(), ..TextQuery::default() }
    }

    #[test]
    fn ranges_whole_word_and_case() {
        assert_eq!(find_ranges("door_main door", "door", false, false), [(0, 4), (10, 14)]);
        assert_eq!(find_ranges("door_main door", "door", false, true), [(10, 14)]);
        assert_eq!(find_ranges("Door DOOR", "door", true, false), []);
        assert_eq!(find_ranges("Door DOOR", "door", false, false), [(0, 4), (5, 9)]);
        assert_eq!(find_ranges("aaaa", "aa", false, false), [(0, 2), (2, 4)]);
        assert_eq!(find_ranges("été door", "door", false, true), [(6, 10)], "byte offsets after accents");
        assert_eq!(find_ranges("x", "", false, false), []);
        assert_eq!(find_ranges("", "x", false, false), []);
        assert_eq!(find_ranges("doordoor door", "door", false, true), [(9, 13)]);
    }

    #[test]
    fn big_maps_search_fast() {
        let mut vmf = String::from(
            "world
{
	\"id\" \"1\"
	\"classname\" \"worldspawn\"
}
",
        );
        for i in 0..20_000 {
            vmf.push_str(&format!(
                "entity
{{
	\"id\" \"{}\"
	\"classname\" \"prop_static\"
	\"model\" \"models/props_c17/oildrum001.mdl\"
	\"targetname\" \"drum_{i}\"
	\"origin\" \"{i} 0 0\"
}}
",
                i + 2
            ));
        }
        let d = Document::from_map(hp_vmf::parse(&vmf).expect("big map"));
        let start = std::time::Instant::now();
        let m = d.find_text(&TextQuery { find: "DRUM_1999".to_owned(), whole_word: true, ..TextQuery::default() });
        let elapsed = start.elapsed();
        assert_eq!(m.len(), 1, "{m:?}");
        assert_eq!(d.find_text(&q("DRUM_1999")).len(), 11);
        assert!(elapsed.as_millis() < 1500, "{elapsed:?}");
    }

    #[test]
    fn finds_values_classes_and_connections() {
        let d = doc();
        let all = d.find_text(&q("door"));
        let fields: Vec<(Id, &TextField)> = all.iter().map(|m| (m.entity, &m.field)).collect();
        assert!(fields.contains(&(1, &TextField::Value("message".into()))), "worldspawn keys");
        assert!(fields.contains(&(2, &TextField::Value("classname".into()))), "class names");
        assert!(fields.contains(&(2, &TextField::Connection { index: 0, part: ConnPart::Target })));
        assert!(fields.contains(&(2, &TextField::Connection { index: 1, part: ConnPart::Param })));
        assert!(fields.contains(&(4, &TextField::Value("targetname".into()))));
        assert!(!all.iter().any(|m| matches!(m.field, TextField::KeyName(_))), "key names only when asked");
        let whole = d.find_text(&TextQuery { whole_word: true, ..q("door") });
        assert_eq!(whole.len(), 2, "message 'Door test' and the parameter 'door': {whole:?}");
        let keys = d.find_text(&TextQuery { key_names: true, ..q("speed") });
        assert!(keys.iter().any(|m| m.field == TextField::KeyName("speed".into())));
        let only = d.find_text(&TextQuery { only: Some(vec![3]), ..q("door") });
        assert!(only.iter().all(|m| m.entity == 3));
        assert!(d.find_text(&q("")).is_empty());
    }

    #[test]
    fn replace_all_is_one_step_and_undoes() {
        let mut d = doc();
        let query = q("door_light");
        let matches = d.find_text(&query);
        assert_eq!(matches.len(), 3);
        let n = d.replace_text(&query, &matches, "lamp").expect("replace");
        assert_eq!(n, 3);
        assert_eq!(d.history_len().0, 1);
        assert_eq!(d.undo_label(), Some(LABEL_REPLACE_TEXT));
        let e = &d.scene().entity(2).expect("door").entity;
        assert_eq!(e.connections[0].target, "lamp");
        assert_eq!(d.scene().entity(3).expect("light").entity.get("targetname"), Some("lamp"));
        assert!(d.undo());
        assert_eq!(d.scene().entity(3).expect("light").entity.get("targetname"), Some("door_light"));
    }

    #[test]
    fn world_keys_and_case_preserving_rest() {
        let mut d = doc();
        let query = q("DOOR");
        let matches: Vec<TextMatch> = d.find_text(&query).into_iter().filter(|m| m.world).collect();
        assert_eq!(d.replace_text(&query, &matches, "Gate").expect("world"), 1);
        assert_eq!(d.scene().world().get("message"), Some("Gate test"));
    }

    #[test]
    fn invalid_replacements_are_refused_or_skipped() {
        let mut d = doc();
        let query = q("func_door");
        let matches = d.find_text(&query);
        assert!(d.replace_text(&query, &matches, "a\"b").is_err());
        assert_eq!(d.replace_text(&query, &matches, "  ").expect("skip"), 0);
        assert_eq!(d.history_len().0, 0);
        let keys = TextQuery { key_names: true, ..q("speed") };
        let m = d.find_text(&keys);
        assert_eq!(d.replace_text(&keys, &m, "classname").expect("skip"), 0);
        assert_eq!(d.replace_text(&keys, &m, "movespeed").expect("rename"), 1);
        assert_eq!(d.scene().entity(2).expect("door").entity.get("movespeed"), Some("100"));
    }

    #[test]
    fn commas_switch_the_connection_separator() {
        let mut d = doc();
        let query = q("TurnOn");
        let m = d.find_text(&query);
        d.replace_text(&query, &m, "Fire,User1").expect("replace");
        let c = &d.scene().entity(2).expect("door").entity.connections[0];
        assert_eq!(c.input, "Fire,User1");
        assert_eq!(c.separator, CONNECTION_ESC);
        let back = Connection::parse(&c.output, &c.to_value_string());
        assert_eq!(back.input, "Fire,User1");
    }

    #[test]
    fn stale_matches_are_skipped() {
        let mut d = doc();
        let query = q("door_main");
        let m = d.find_text(&query);
        d.set_entity_key(&[2], "targetname", "renamed").expect("rename");
        assert_eq!(d.replace_text(&query, &m, "x").expect("stale"), 0);
    }

    #[test]
    fn hidden_entities_are_skipped_when_asked() {
        let mut d = doc();
        d.set_hidden(&[crate::ObjRef::Entity(4)], true);
        let visible = d.find_text(&TextQuery { visible_only: true, ..q("doorway") });
        assert!(visible.is_empty());
        assert_eq!(d.find_text(&q("doorway")).len(), 1);
    }
}
