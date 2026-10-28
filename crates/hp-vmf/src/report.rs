use hp_kv::ParseIssues;

use crate::write::{AXIS_KEYS, SIDE_SCALAR_KEYS};
use crate::{Map, Side, TextEncoding};

const MAX_EXAMPLES: usize = 5;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadReport {
    pub syntax: ParseIssues,
    pub encoding: TextEncoding,
    pub unreadable_values: usize,
    pub unreadable_examples: Vec<String>,
    pub missing_ids: usize,
}

impl LoadReport {
    pub(crate) fn new(syntax: ParseIssues, map: &Map) -> Self {
        let mut report = Self { syntax, encoding: map.format.encoding, ..Self::default() };
        let mut note = |what: String| {
            report.unreadable_values += 1;
            if report.unreadable_examples.len() < MAX_EXAMPLES {
                report.unreadable_examples.push(what);
            }
        };
        let entities = std::iter::once(&map.world).chain(map.entities.iter());
        let mut missing_ids = 0;
        for (i, e) in entities.enumerate() {
            if e.id == 0 && i > 0 {
                missing_ids += 1;
            }
            for s in &e.solids {
                if s.id == 0 {
                    missing_ids += 1;
                }
                for side in &s.sides {
                    for (k, v) in unreadable_side_values(side) {
                        note(format!("solid {}, side {}: {k} \"{v}\"", s.id, side.id));
                    }
                }
            }
        }
        report.missing_ids = missing_ids;
        report
    }

    pub fn is_damaged(&self) -> bool {
        self.syntax.is_truncated()
    }

    pub fn is_clean(&self) -> bool {
        !self.is_damaged()
            && self.syntax.stray_closes == 0
            && self.syntax.missing_values == 0
            && self.unreadable_values == 0
            && self.missing_ids == 0
    }

    pub fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        let s = &self.syntax;
        if s.unterminated_string {
            out.push(
                "The file ends inside a quoted string: it is truncated or corrupt and part of the map is missing"
                    .to_owned(),
            );
        }
        if s.unclosed_blocks > 0 {
            out.push(format!(
                "The file ends with {}: it is truncated and part of the map is missing",
                count(s.unclosed_blocks, "unclosed block", "unclosed blocks")
            ));
        }
        if s.stray_closes > 0 {
            out.push(format!(
                "{} ignored",
                count(s.stray_closes, "unexpected closing brace was", "unexpected closing braces were")
            ));
        }
        if s.missing_values > 0 {
            out.push(format!(
                "{} ignored",
                count(s.missing_values, "key without a value was", "keys without a value were")
            ));
        }
        if self.unreadable_values > 0 {
            let mut msg = format!(
                "{} unchanged (the solids using them are invalid)",
                count(
                    self.unreadable_values,
                    "value could not be read and is kept",
                    "values could not be read and are kept"
                )
            );
            if !self.unreadable_examples.is_empty() {
                msg.push_str(": ");
                msg.push_str(&self.unreadable_examples.join("; "));
            }
            out.push(msg);
        }
        if self.missing_ids > 0 {
            out.push(format!(
                "{} a new one",
                count(self.missing_ids, "object had no valid id and got", "objects had no valid id and got")
            ));
        }
        out
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn unreadable_side_values(side: &Side) -> impl Iterator<Item = (&str, &str)> {
    side.extra_props.iter().filter_map(move |(k, v)| {
        let key = k.as_str();
        let unread = if key.eq_ignore_ascii_case("plane") {
            side.plane == Side::UNREADABLE_PLANE
        } else if AXIS_KEYS.iter().any(|a| key.eq_ignore_ascii_case(a)) {
            let axis = if key.eq_ignore_ascii_case("uaxis") { &side.uaxis } else { &side.vaxis };
            axis.axis == glam::DVec3::ZERO
        } else {
            SIDE_SCALAR_KEYS.iter().any(|a| key.eq_ignore_ascii_case(a))
        };
        unread.then_some((key, v.as_str()))
    })
}
