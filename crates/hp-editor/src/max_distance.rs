use hp_vmf::Entity;

use crate::document::Document;
use crate::{EditResult, Id};

pub const MAX_DISTANCE: f64 = 1_000_000.0;
const FADE_START: f64 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DistanceKind {
    LightCutoff,
    LightLength,
    ViewFade,
    GlowFade,
    BrushFade,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistanceRule {
    pub kind: DistanceKind,
    pub key: &'static str,
    pub default: f64,
}

const MODEL_CLASSES: &[&str] = &[
    "prop_dynamic",
    "prop_dynamic_override",
    "prop_dynamic_ornament",
    "prop_physics",
    "prop_physics_override",
    "prop_physics_multiplayer",
    "prop_ragdoll",
    "prop_door_rotating",
];

pub fn rule_for(classname: &str, has_key: &dyn Fn(&str) -> Option<bool>) -> Option<DistanceRule> {
    let c = classname.trim().to_ascii_lowercase();
    let rule = |kind, key, default| Some(DistanceRule { kind, key, default });
    match c.as_str() {
        "light" | "light_spot" => rule(DistanceKind::LightCutoff, "_distance", 0.0),
        "light_dynamic" => rule(DistanceKind::LightLength, "distance", 120.0),
        "point_spotlight" => rule(DistanceKind::LightLength, "SpotlightLength", 500.0),
        "env_lightglow" => rule(DistanceKind::GlowFade, "OuterMaxDist", 0.0),
        "func_lod" => rule(DistanceKind::BrushFade, "DisappearMaxDist", 2800.0),
        "prop_static" | "info_overlay" | "env_detail_controller" => rule(DistanceKind::ViewFade, "fademaxdist", 0.0),
        "prop_detail" | "prop_detail_sprite" | "shadow_control" => None,
        _ => match has_key("fademaxdist") {
            Some(true) => rule(DistanceKind::ViewFade, "fademaxdist", 0.0),
            Some(false) => None,
            None => MODEL_CLASSES.contains(&c.as_str()).then_some(DistanceRule {
                kind: DistanceKind::ViewFade,
                key: "fademaxdist",
                default: 0.0,
            }),
        },
    }
}

fn num(e: &Entity, key: &str) -> Option<f64> {
    e.get(key)
        .and_then(|v| v.split_ascii_whitespace().next())
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

fn fmt(v: f64) -> String {
    format!("{}", v.round().clamp(-MAX_DISTANCE, MAX_DISTANCE) as i64)
}

pub fn current(e: &Entity, rule: &DistanceRule) -> f64 {
    let v = num(e, rule.key).unwrap_or(rule.default);
    match rule.kind {
        DistanceKind::GlowFade => {
            let max = num(e, "MaxDist").unwrap_or(2000.0);
            if v > max { v } else { 0.0 }
        }
        _ => v.max(0.0),
    }
}

pub fn edits(e: &Entity, rule: &DistanceRule, d: f64) -> Vec<(&'static str, String)> {
    let d = if d.is_finite() { d.round().clamp(0.0, MAX_DISTANCE) } else { 0.0 };
    let mut out: Vec<(&'static str, String)> = Vec::new();
    let set = |key: &'static str, v: f64, out: &mut Vec<(&'static str, String)>| {
        let v = fmt(v);
        if e.get(key).map(str::trim) != Some(v.as_str()) {
            out.push((key, v));
        }
    };
    let start = |key: &str, default: f64| {
        let s = num(e, key).unwrap_or(default);
        if s >= 0.0 && s < d { s } else { (d * FADE_START).round() }
    };
    match rule.kind {
        DistanceKind::LightCutoff => set(rule.key, d, &mut out),
        DistanceKind::LightLength => {
            let now = num(e, rule.key).filter(|v| *v > 0.0).unwrap_or(rule.default);
            if d > 0.0 && now > d {
                set(rule.key, d, &mut out);
            }
        }
        DistanceKind::ViewFade => {
            if d > 0.0 {
                set("fademindist", start("fademindist", -1.0), &mut out);
            }
            set("fademaxdist", d, &mut out);
        }
        DistanceKind::GlowFade => {
            if d > 0.0 {
                let max = num(e, "MaxDist").unwrap_or(2000.0);
                if max >= d {
                    let new_max = (d * FADE_START).round();
                    set("MaxDist", new_max, &mut out);
                    if num(e, "MinDist").unwrap_or(500.0) >= new_max {
                        set("MinDist", (new_max * 0.25).round(), &mut out);
                    }
                }
            }
            set(rule.key, d, &mut out);
        }
        DistanceKind::BrushFade => {
            if d > 0.0 {
                set("DisappearMinDist", start("DisappearMinDist", 2000.0), &mut out);
                set(rule.key, d, &mut out);
                if e.get("DisappearDist").is_some() {
                    set("DisappearDist", d, &mut out);
                }
            }
        }
    }
    out
}

pub fn is_distance_key(classname: &str, key: &str) -> Option<DistanceKind> {
    let rule = rule_for(classname, &|k| Some(k.eq_ignore_ascii_case(key)))?;
    rule.key.eq_ignore_ascii_case(key).then_some(rule.kind)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaxDistanceReport {
    pub changed: Vec<Id>,
    pub unchanged: Vec<Id>,
    pub skipped: Vec<Id>,
    pub skipped_classes: Vec<String>,
}

impl Document {
    pub fn set_max_distance(
        &mut self,
        ids: &[Id],
        d: f64,
        label: &str,
        has_key: &dyn Fn(&str, &str) -> Option<bool>,
    ) -> EditResult<MaxDistanceReport> {
        self.check_entities(ids)?;
        let mut report = MaxDistanceReport::default();
        let mut plan: Vec<(Id, Vec<(&'static str, String)>)> = Vec::new();
        for &id in ids {
            let Some(node) = self.scene().entity(id) else { continue };
            let e = &node.entity;
            let class = e.classname.clone();
            match rule_for(&class, &|k| has_key(&class, k)) {
                None => {
                    report.skipped.push(id);
                    let lower = class.trim().to_ascii_lowercase();
                    if !report.skipped_classes.contains(&lower) {
                        report.skipped_classes.push(lower);
                    }
                }
                Some(rule) => {
                    let keys = edits(e, &rule, d);
                    if keys.is_empty() {
                        report.unchanged.push(id);
                    } else {
                        report.changed.push(id);
                        plan.push((id, keys));
                    }
                }
            }
        }
        report.skipped_classes.sort();
        if plan.is_empty() {
            return Ok(report);
        }
        self.begin(label);
        for (id, keys) in plan {
            if let Err(err) = self.edit_entity(id, |e| {
                for (k, v) in keys {
                    e.set(k, v);
                }
            }) {
                self.cancel();
                return Err(err);
            }
        }
        self.commit();
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(class: &str, keys: &[(&str, &str)]) -> Entity {
        let mut e = Entity { classname: class.to_owned(), ..Default::default() };
        for (k, v) in keys {
            e.set(k, *v);
        }
        e
    }

    fn no_fgd(_: &str) -> Option<bool> {
        None
    }

    #[test]
    fn rules_per_class() {
        let r = |c: &str| rule_for(c, &no_fgd).map(|r| (r.kind, r.key));
        assert_eq!(r("light"), Some((DistanceKind::LightCutoff, "_distance")));
        assert_eq!(r("Light_Spot"), Some((DistanceKind::LightCutoff, "_distance")));
        assert_eq!(r("light_dynamic"), Some((DistanceKind::LightLength, "distance")));
        assert_eq!(r("point_spotlight"), Some((DistanceKind::LightLength, "SpotlightLength")));
        assert_eq!(r("prop_static"), Some((DistanceKind::ViewFade, "fademaxdist")));
        assert_eq!(r("prop_door_rotating"), Some((DistanceKind::ViewFade, "fademaxdist")));
        assert_eq!(r("info_overlay"), Some((DistanceKind::ViewFade, "fademaxdist")));
        assert_eq!(r("env_lightglow"), Some((DistanceKind::GlowFade, "OuterMaxDist")));
        assert_eq!(r("func_lod"), Some((DistanceKind::BrushFade, "DisappearMaxDist")));
        for none in [
            "logic_auto",
            "trigger_multiple",
            "func_brush",
            "env_sprite",
            "infodecal",
            "light_environment",
            "prop_detail",
        ] {
            assert_eq!(r(none), None, "{none}");
        }
        let fgd = |k: &str| Some(k == "fademaxdist");
        assert_eq!(rule_for("item_healthkit", &fgd).map(|r| r.key), Some("fademaxdist"));
        assert_eq!(rule_for("item_healthkit", &no_fgd), None);
        assert_eq!(rule_for("prop_physics", &|_| Some(false)), None);
        assert_eq!(is_distance_key("light_spot", "_DISTANCE"), Some(DistanceKind::LightCutoff));
        assert_eq!(is_distance_key("prop_physics", "fademaxdist"), Some(DistanceKind::ViewFade));
        assert_eq!(is_distance_key("prop_physics", "fademindist"), None);
    }

    #[test]
    fn keys_per_kind() {
        let rule = |c: &str| rule_for(c, &no_fgd).unwrap();
        let light = ent("light", &[]);
        assert_eq!(edits(&light, &rule("light"), 256.4), [("_distance", "256".to_owned())]);
        assert_eq!(edits(&ent("light", &[("_distance", "256")]), &rule("light"), 256.0), []);
        assert_eq!(edits(&ent("light_dynamic", &[]), &rule("light_dynamic"), 500.0), []);
        assert_eq!(
            edits(&ent("light_dynamic", &[("distance", "800")]), &rule("light_dynamic"), 500.0),
            [("distance", "500".to_owned())]
        );
        assert_eq!(
            edits(&ent("point_spotlight", &[]), &rule("point_spotlight"), 300.0),
            [("SpotlightLength", "300".to_owned())]
        );
        assert_eq!(edits(&ent("point_spotlight", &[]), &rule("point_spotlight"), 0.0), []);
        let prop = ent("prop_static", &[("fademindist", "-1"), ("fademaxdist", "0")]);
        assert_eq!(
            edits(&prop, &rule("prop_static"), 1000.0),
            [("fademindist", "800".to_owned()), ("fademaxdist", "1000".to_owned())]
        );
        let kept = ent("prop_dynamic", &[("fademindist", "300")]);
        assert_eq!(edits(&kept, &rule("prop_dynamic"), 1000.0), [("fademaxdist", "1000".to_owned())]);
        assert_eq!(
            edits(&ent("prop_static", &[("fademaxdist", "900")]), &rule("prop_static"), 0.0),
            [("fademaxdist", "0".to_owned())]
        );
        let glow = ent("env_lightglow", &[("MinDist", "500"), ("MaxDist", "2000")]);
        assert_eq!(edits(&glow, &rule("env_lightglow"), 3000.0), [("OuterMaxDist", "3000".to_owned())]);
        assert_eq!(
            edits(&glow, &rule("env_lightglow"), 500.0),
            [("MaxDist", "400".to_owned()), ("MinDist", "100".to_owned()), ("OuterMaxDist", "500".to_owned())]
        );
        let lod = ent("func_lod", &[("DisappearMinDist", "2000"), ("DisappearMaxDist", "2800")]);
        assert_eq!(
            edits(&lod, &rule("func_lod"), 1000.0),
            [("DisappearMinDist", "800".to_owned()), ("DisappearMaxDist", "1000".to_owned())]
        );
        assert_eq!(edits(&lod, &rule("func_lod"), 0.0), []);
        assert_eq!(edits(&light, &rule("light"), f64::NAN), [("_distance", "0".to_owned())]);
        assert_eq!(current(&ent("light", &[("_distance", "300")]), &rule("light")), 300.0);
        assert_eq!(current(&glow, &rule("env_lightglow")), 0.0);
        assert_eq!(current(&ent("light_dynamic", &[]), &rule("light_dynamic")), 120.0);
    }

    #[test]
    fn one_undo_step_and_skips() {
        let mut text = String::from("world { \"id\" \"1\" \"classname\" \"worldspawn\" }\n");
        for (id, class) in [(2, "light"), (3, "prop_static"), (4, "logic_auto"), (5, "trigger_once")] {
            text += &format!("entity {{ \"id\" \"{id}\" \"classname\" \"{class}\" \"origin\" \"0 0 0\" }}\n");
        }
        let text = text.as_str();
        let mut doc = Document::from_map(hp_vmf::parse(text).expect("vmf"));
        let steps = doc.history_len().0;
        let r = doc.set_max_distance(&[2, 3, 4, 5], 512.0, "Set maximum distance", &|_, _| None).unwrap();
        assert_eq!((r.changed.len(), r.unchanged.len(), r.skipped.len()), (2, 0, 2));
        assert_eq!(r.skipped_classes, ["logic_auto", "trigger_once"]);
        assert_eq!(doc.history_len().0, steps + 1);
        assert_eq!(doc.undo_labels().last().copied(), Some("Set maximum distance"));
        assert_eq!(doc.scene().entity(2).unwrap().entity.get("_distance"), Some("512"));
        assert_eq!(doc.scene().entity(3).unwrap().entity.get("fademaxdist"), Some("512"));
        let again = doc.set_max_distance(&[2, 3], 512.0, "Set maximum distance", &|_, _| None).unwrap();
        assert_eq!((again.changed.len(), again.unchanged.len()), (0, 2));
        assert_eq!(doc.history_len().0, steps + 1);
        assert!(doc.set_max_distance(&[999_999], 1.0, "x", &|_, _| None).is_err());
    }
}
