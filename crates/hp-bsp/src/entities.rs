#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entity {
    pub pairs: Vec<(String, String)>,
}

impl Entity {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.pairs.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }

    pub fn classname(&self) -> &str {
        self.get("classname").unwrap_or("")
    }

    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        match self.pairs.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            Some((_, v)) => *v = value,
            None => self.pairs.push((key.to_owned(), value)),
        }
    }

    pub fn origin(&self) -> Option<[f64; 3]> {
        let mut it = self.get("origin")?.split_ascii_whitespace().map(|s| s.parse::<f64>().ok());
        let p = [it.next()??, it.next()??, it.next()??];
        p.iter().all(|v| v.is_finite()).then_some(p)
    }
}

pub fn parse_entities(text: &str) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut cur: Option<Entity> = None;
    let mut key: Option<String> = None;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                cur = Some(Entity::default());
                key = None;
                i += 1;
            }
            b'}' => {
                if let Some(e) = cur.take() {
                    out.push(e);
                }
                key = None;
                i += 1;
            }
            b'"' => {
                let start = i + 1;
                let Some(len) = bytes[start..].iter().position(|&b| b == b'"') else { break };
                let s = &text[start..start + len];
                i = start + len + 1;
                if let Some(e) = cur.as_mut() {
                    match key.take() {
                        None => key = Some(s.to_owned()),
                        Some(k) => e.pairs.push((k, s.to_owned())),
                    }
                }
            }
            _ => i += 1,
        }
    }
    out
}

pub fn write_entities(entities: &[Entity]) -> Vec<u8> {
    let mut out = String::with_capacity(entities.len() * 128);
    for e in entities {
        out.push_str("{\n");
        for (k, v) in &e.pairs {
            out.push('"');
            out.extend(k.chars().filter(|&c| c != '"'));
            out.push_str("\" \"");
            out.extend(v.chars().filter(|&c| c != '"'));
            out.push_str("\"\n");
        }
        out.push_str("}\n");
    }
    let mut bytes = out.into_bytes();
    bytes.push(0);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_order_and_duplicates() {
        let text = "{\n\"world_maxs\" \"1 2 3\"\n\"classname\" \"worldspawn\"\n}\n{\n\"classname\" \"logic_relay\"\n\"OnTrigger\" \"a,Kill,,0,-1\"\n\"OnTrigger\" \"b,Kill,,0,-1\"\n\"origin\" \"1 -2.5 3\"\n}\n";
        let ents = parse_entities(text);
        assert_eq!(ents.len(), 2);
        assert_eq!(ents[0].classname(), "worldspawn");
        assert_eq!(ents[1].pairs.len(), 4);
        assert_eq!(ents[1].origin(), Some([1.0, -2.5, 3.0]));
        let written = write_entities(&ents);
        assert_eq!(written.last(), Some(&0));
        assert_eq!(&written[..written.len() - 1], text.as_bytes());
    }

    #[test]
    fn garbage_never_panics() {
        for t in ["{\"a", "}}}{", "{\"a\" \"b\"", "\"x\" \"y\"", "{ \"k\" }", "{\"é\" \"ü\"}"] {
            let _ = parse_entities(t);
        }
        let mut e = Entity::default();
        e.set("Origin", "0 0 0");
        e.set("origin", "1 1 1");
        assert_eq!(e.pairs.len(), 1);
        assert_eq!(e.get("ORIGIN"), Some("1 1 1"));
        e.set("origin", "nan 0 0");
        assert_eq!(e.origin(), None);
    }
}
