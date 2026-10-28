use std::collections::HashSet;

use hp_vmf::{CONNECTION_ESC, Connection, Entity, Id};

fn is_compile_key(key: &str, value: &str) -> bool {
    key.eq_ignore_ascii_case("hammerid") || (key.eq_ignore_ascii_case("model") && value.starts_with('*'))
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(crate) fn is_output(key: &str, value: &str, known_outputs: Option<&HashSet<String>>) -> bool {
    if key.is_empty() || key.eq_ignore_ascii_case("classname") {
        return false;
    }
    if let Some(known) = known_outputs
        && known.contains(&key.to_ascii_lowercase())
    {
        return value.matches([',', CONNECTION_ESC]).count() >= 4;
    }
    let sep = if value.contains(CONNECTION_ESC) { CONNECTION_ESC } else { ',' };
    let fields: Vec<&str> = value.split(sep).collect();
    if fields.len() < 5 {
        return false;
    }
    let n = fields.len();
    let delay_ok = fields[n - 2].trim().parse::<f32>().is_ok();
    let times_ok = fields[n - 1].trim().parse::<i32>().is_ok();
    if !delay_ok || !times_ok || !is_identifier(fields[1].trim()) {
        return false;
    }
    if sep == CONNECTION_ESC {
        return true;
    }
    let lower = key.to_ascii_lowercase();
    let output_like = lower.starts_with("on") || lower.starts_with("out");
    let target = fields[0].trim();
    output_like || (!target.is_empty() && target.parse::<f64>().is_err())
}

pub(crate) fn convert(
    src: &hp_bsp::Entity,
    fallback_id: Id,
    used: &mut HashSet<Id>,
    known_outputs: Option<&HashSet<String>>,
) -> Entity {
    let mut e = Entity { classname: src.classname().to_owned(), ..Entity::default() };
    let hammer_id = src.get("hammerid").and_then(|v| v.trim().parse::<Id>().ok()).filter(|&id| id > 1);
    e.id = match hammer_id {
        Some(id) if used.insert(id) => id,
        _ => {
            used.insert(fallback_id);
            fallback_id
        }
    };
    for (k, v) in &src.pairs {
        if k.eq_ignore_ascii_case("classname") || is_compile_key(k, v) {
            continue;
        }
        if is_output(k, v, known_outputs) {
            e.connections.push(Connection::parse(k, v));
        } else {
            e.props.push((k.clone(), v.clone()));
        }
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(pairs: &[(&str, &str)]) -> hp_bsp::Entity {
        hp_bsp::Entity { pairs: pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect() }
    }

    #[test]
    fn outputs_become_connections() {
        let src = ent(&[
            ("classname", "logic_relay"),
            ("targetname", "relay"),
            ("hammerid", "1234"),
            ("OnTrigger", "door,Open,,0,-1"),
            ("OnTrigger", "lamp\u{1b}TurnOn\u{1b}\u{1b}1.5\u{1b}1"),
            ("PressedMoving", "relay2,Trigger,,0.25,-1"),
            ("rendercolor", "255,255,255,0,1"),
            ("spawnflags", "1"),
            ("model", "*3"),
        ]);
        let mut used = HashSet::new();
        let e = convert(&src, 50, &mut used, None);
        assert_eq!(e.id, 1234);
        assert_eq!(e.classname, "logic_relay");
        assert_eq!(e.connections.len(), 3);
        assert_eq!(e.connections[1].input, "TurnOn");
        assert_eq!(e.connections[1].delay, 1.5);
        assert_eq!(e.connections[2].output, "PressedMoving");
        assert!(e.get("hammerid").is_none());
        assert_eq!(e.get("rendercolor"), Some("255,255,255,0,1"));
        assert_eq!(e.get("spawnflags"), Some("1"));
        assert!(e.get("model").is_none(), "brush model keys are dropped");
        let prop = convert(&ent(&[("classname", "prop_dynamic"), ("model", "models/a.mdl")]), 60, &mut used, None);
        assert_eq!(prop.get("model"), Some("models/a.mdl"));
        let e2 = convert(&src, 51, &mut used, None);
        assert_eq!(e2.id, 51);
    }

    #[test]
    fn known_outputs_decide_first() {
        let known: HashSet<String> = ["onfoo".to_owned()].into_iter().collect();
        assert!(is_output("OnFoo", "1,2,3,4,5", Some(&known)));
        assert!(!is_output("color", "1,2,3,4,5", Some(&known)));
        assert!(!is_output("OnFoo", "a,b", Some(&known)));
    }
}
