use glam::DVec3;

use crate::props::{ModelCheck, ModelSource};

pub const MAX_LIT_VERTICES: u32 = 65536;

struct PropEntity<'a> {
    id: u32,
    class: &'a str,
    model: String,
    origin: DVec3,
    entity: &'a hp_vmf::Entity,
}

fn int_key(e: &hp_vmf::Entity, k: &str) -> Option<i64> {
    e.get(k).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).map(|v| v as i64)
}

fn origin_of(e: &hp_vmf::Entity) -> DVec3 {
    let v: Vec<f64> = e.get("origin").unwrap_or("").split_whitespace().filter_map(|x| x.parse().ok()).collect();
    if v.len() == 3 { DVec3::new(v[0], v[1], v[2]) } else { DVec3::ZERO }
}

fn needs_physics(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    c.starts_with("prop_physics") || c == "prop_ragdoll" || c == "prop_sphere"
}

fn reasons(p: &PropEntity<'_>, check: Option<&ModelCheck>, static_error: Option<&str>, outside: bool) -> Vec<String> {
    let mut out = Vec::new();
    let is_static = p.class.eq_ignore_ascii_case("prop_static");
    if p.model.is_empty() {
        out.push("it has no model".to_owned());
        return out;
    }
    if !p.model.to_ascii_lowercase().ends_with(".mdl") {
        out.push("its model is not a .mdl file".to_owned());
    }
    let Some(c) = check else {
        if outside {
            out.push("it touches no open leaf (inside solid or outside the map): it is left out of the map".to_owned());
        }
        return out;
    };
    if !c.exists {
        out.push(if is_static {
            "the model file is missing: the prop is left out of the map".to_owned()
        } else {
            "the model file is missing: the game shows an error model".to_owned()
        });
        return out;
    }
    if !c.loads {
        out.push("the model file is broken (it does not read as a model)".to_owned());
        return out;
    }
    if !c.has_vvd {
        out.push("its vertex file (.vvd) is missing: the model cannot be drawn".to_owned());
    } else if !c.vvd_matches {
        out.push(
            "its vertex file (.vvd) does not match the .mdl (compiled at another time): it may draw broken".to_owned(),
        );
    }
    if !c.has_vtx {
        out.push("its mesh file (.dx90.vtx) is missing: the model cannot be drawn".to_owned());
    } else if !c.vtx_matches {
        out.push(
            "its mesh file (.vtx) does not match the .mdl (compiled at another time): it may draw broken".to_owned(),
        );
    }
    if c.has_phy && !c.phy_matches {
        out.push("its collision model (.phy) does not match the .mdl (compiled at another time): its collisions may be wrong".to_owned());
    }
    if c.has_phy && c.phy_solids == 0 {
        out.push("its collision model (.phy) holds no solid: nothing collides with it".to_owned());
    }
    if needs_physics(p.class) && !c.has_phy {
        out.push(format!(
            "{} needs a collision model (.phy) and this model has none: the game removes the prop",
            p.class
        ));
    }
    if is_static {
        let solid = int_key(p.entity, "solid").unwrap_or(6);
        if solid == 6 && !c.has_phy {
            out.push("its collisions are set to VPhysics but the model has no collision model (.phy): nothing collides with it (use Bounding Box or Not Solid)".to_owned());
        }
        if let Some(e) = static_error {
            let e = e.replace('\n', " ");
            let e = e.split_whitespace().collect::<Vec<_>>().join(" ");
            out.push(format!("the model cannot be a static prop, VBSP deletes it ({e})"));
        }
        if c.vertices > MAX_LIT_VERTICES {
            out.push(format!(
                "its model has {} vertices, more than the {MAX_LIT_VERTICES} the static prop lighting stores: it gets no per vertex lighting",
                c.vertices
            ));
        }
        if outside {
            out.push("it touches no open leaf (inside solid or outside the map): it is left out of the map".to_owned());
        }
    }
    let skin = int_key(p.entity, "skin").unwrap_or(0);
    let skins = i64::from(c.skins.max(1));
    if skin < 0 || skin >= skins {
        out.push(format!(
            "skin {skin} does not exist (the model has {skins} skin{}): skin 0 is drawn",
            if skins == 1 { "" } else { "s" }
        ));
    }
    let body = int_key(p.entity, "body").or_else(|| int_key(p.entity, "bodygroup")).unwrap_or(0);
    if body != 0 {
        let bodies = i64::from(c.bodies.max(1));
        if is_static {
            out.push(format!("body group {body} is set but a static prop always draws body group 0 (use prop_dynamic to show another one)"));
        } else if body < 0 || body >= bodies {
            out.push(format!(
                "body group {body} does not exist (the model has {bodies} combination{}): body group 0 is drawn",
                if bodies == 1 { "" } else { "s" }
            ));
        }
    }
    out
}

const LISTED: usize = 20;

pub fn report(entities: &[&hp_vmf::Entity], models: &dyn ModelSource, outside: &[u32]) -> Vec<String> {
    use rayon::prelude::*;
    let props: Vec<PropEntity<'_>> = entities
        .iter()
        .filter(|e| e.classname.to_ascii_lowercase().starts_with("prop_") && e.get("model").is_some())
        .map(|e| PropEntity {
            id: e.id,
            class: e.classname.as_str(),
            model: e.get("model").unwrap_or("").trim().replace('\\', "/"),
            origin: origin_of(e),
            entity: e,
        })
        .collect();
    let mut out = vec!["---- Prop debug (-propdebug) ----".to_owned()];
    let mut keys: Vec<String> = props.iter().map(|p| p.model.to_ascii_lowercase()).filter(|m| !m.is_empty()).collect();
    keys.sort();
    keys.dedup();
    let found: Vec<(Option<ModelCheck>, Option<String>)> = keys
        .par_iter()
        .map(|k| {
            let name = props.iter().find(|p| p.model.eq_ignore_ascii_case(k)).map_or(k.as_str(), |p| p.model.as_str());
            let check = models.model_check(name);
            let static_error = check.as_ref().filter(|c| c.loads).and_then(|_| models.static_prop_error(name));
            (check, static_error)
        })
        .collect();
    let of = |m: &str| keys.binary_search(&m.to_ascii_lowercase()).ok().map(|i| &found[i]);
    if found.iter().any(|(c, _)| c.is_none()) {
        out.push("The game files are not mounted: the models themselves cannot be checked".to_owned());
    }
    let outside: ahash::AHashSet<u32> = outside.iter().copied().collect();
    let mut count = 0usize;
    let mut bad_models: Vec<&str> = Vec::new();
    let mut groups: Vec<((String, String), Vec<&PropEntity<'_>>)> = Vec::new();
    let mut group_of: ahash::AHashMap<(String, String), usize> = ahash::AHashMap::new();
    for p in &props {
        let (check, static_error) = of(&p.model).map_or((None, None), |(c, e)| (c.as_ref(), e.as_deref()));
        let is_static = p.class.eq_ignore_ascii_case("prop_static");
        let left_out = is_static && outside.contains(&p.id);
        let list = reasons(p, check, static_error.filter(|_| is_static), left_out);
        if list.is_empty() {
            continue;
        }
        count += 1;
        if !bad_models.iter().any(|m| m.eq_ignore_ascii_case(&p.model)) {
            bad_models.push(&p.model);
        }
        let model = if p.model.is_empty() { "(no model)".to_owned() } else { p.model.clone() };
        for r in list {
            let key = (model.to_ascii_lowercase(), r.clone());
            let k = *group_of.entry(key).or_insert_with(|| {
                groups.push(((model.clone(), r), Vec::new()));
                groups.len() - 1
            });
            groups[k].1.push(p);
        }
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.1.len()));
    for ((model, reason), list) in &groups {
        out.push(format!("{model}: {reason} ({} prop{})", list.len(), if list.len() == 1 { "" } else { "s" }));
        for p in list.iter().take(LISTED) {
            out.push(format!("    {} {} at ({:.0} {:.0} {:.0})", p.class, p.id, p.origin.x, p.origin.y, p.origin.z));
        }
        if list.len() > LISTED {
            out.push(format!("    and {} more", list.len() - LISTED));
        }
    }
    if count == 0 {
        out.push(format!("No problem found in the {} props", props.len()));
    } else {
        out.push(format!(
            "{count} of the {} props have problems ({} model{})",
            props.len(),
            bad_models.len(),
            if bad_models.len() == 1 { "" } else { "s" }
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Models;

    impl ModelSource for Models {
        fn bounds(&self, _path: &str) -> Option<(DVec3, DVec3)> {
            None
        }

        fn static_prop_error(&self, path: &str) -> Option<String> {
            path.contains("dynamic").then(|| {
                format!("Error! To use model \"{path}\"\n      with prop_static, it must be compiled with $staticprop!")
            })
        }

        fn model_check(&self, path: &str) -> Option<ModelCheck> {
            let good = ModelCheck {
                exists: true,
                loads: true,
                has_vvd: true,
                vvd_matches: true,
                has_vtx: true,
                vtx_matches: true,
                has_phy: true,
                phy_matches: true,
                phy_solids: 1,
                skins: 2,
                bodies: 1,
                vertices: 100,
            };
            Some(match path {
                p if p.contains("missing") => ModelCheck::default(),
                p if p.contains("broken") => ModelCheck { exists: true, ..ModelCheck::default() },
                p if p.contains("nophy") => ModelCheck { has_phy: false, phy_solids: 0, ..good },
                p if p.contains("novvd") => ModelCheck { has_vvd: false, ..good },
                p if p.contains("huge") => ModelCheck { vertices: 70_000, ..good },
                _ => good,
            })
        }
    }

    fn entity(id: u32, class: &str, model: &str, extra: &[(&str, &str)]) -> hp_vmf::Entity {
        let mut props = vec![("model".to_owned(), model.to_owned()), ("origin".to_owned(), "10 20 30".to_owned())];
        props.extend(extra.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())));
        hp_vmf::Entity { id, classname: class.to_owned(), props, ..Default::default() }
    }

    #[test]
    fn each_problem_is_named_with_the_entity_model_and_position() {
        let ents = [
            entity(1, "prop_static", "models/ok.mdl", &[]),
            entity(2, "prop_static", "models/missing.mdl", &[]),
            entity(3, "prop_static", "models/broken.mdl", &[]),
            entity(4, "prop_static", "models/nophy.mdl", &[]),
            entity(5, "prop_static", "models/nophy.mdl", &[("solid", "0")]),
            entity(6, "prop_physics", "models/nophy.mdl", &[]),
            entity(7, "prop_static", "models/ok.mdl", &[("skin", "5")]),
            entity(8, "prop_static", "models/ok.mdl", &[("body", "1")]),
            entity(9, "prop_dynamic", "models/ok.mdl", &[("body", "3")]),
            entity(10, "prop_static", "models/dynamic.mdl", &[]),
            entity(11, "prop_static", "models/huge.mdl", &[]),
            entity(12, "prop_static", "models/ok.mdl", &[]),
            entity(13, "prop_dynamic", "models/novvd.mdl", &[]),
        ];
        let refs: Vec<&hp_vmf::Entity> = ents.iter().collect();
        let lines = report(&refs, &Models, &[12]);
        let has = |id: u32, what: &str| has_problem(&lines, id, what);
        assert!(!lines.iter().any(|l| l.contains(" 1 at")), "{lines:#?}");
        assert!(has(2, "model file is missing"), "{lines:#?}");
        assert!(lines.iter().any(|l| l == "    prop_static 2 at (10 20 30)"));
        assert!(has(2, "models/missing.mdl"));
        assert!(has(3, "broken"));
        assert!(has(4, "VPhysics"));
        assert!(!lines.iter().any(|l| l.contains(" 5 at")), "not solid: no collision needed");
        assert!(has(6, "removes the prop"));
        assert!(has(7, "skin 5 does not exist"));
        assert!(has(8, "always draws body group 0"));
        assert!(has(9, "body group 3 does not exist"));
        assert!(has(10, "cannot be a static prop"));
        assert!(has(11, "70000 vertices"));
        assert!(has(12, "no open leaf"));
        assert!(has(13, ".vvd"));
        assert!(
            lines.last().is_some_and(|l| l.starts_with("11 of the 13 props have problems (7 models)")),
            "{lines:#?}"
        );
    }

    fn has_problem(lines: &[String], id: u32, what: &str) -> bool {
        lines.iter().enumerate().any(|(k, l)| {
            !l.starts_with(' ')
                && l.contains(what)
                && lines[k + 1..].iter().take_while(|x| x.starts_with(' ')).any(|x| x.contains(&format!(" {id} at")))
        })
    }

    #[test]
    fn without_game_files_only_the_map_is_checked() {
        let ents = [entity(1, "prop_static", "models/x.mdl", &[]), entity(2, "prop_static", "", &[])];
        let refs: Vec<&hp_vmf::Entity> = ents.iter().collect();
        let lines = report(&refs, &crate::props::NoModels, &[1]);
        assert!(lines.iter().any(|l| l.contains("not mounted")));
        assert!(has_problem(&lines, 1, "no open leaf"), "{lines:#?}");
        assert!(has_problem(&lines, 2, "no model"), "{lines:#?}");
    }

    #[test]
    fn many_props_with_one_problem_are_counted() {
        let ents: Vec<hp_vmf::Entity> = (1..=30).map(|i| entity(i, "prop_static", "models/missing.mdl", &[])).collect();
        let refs: Vec<&hp_vmf::Entity> = ents.iter().collect();
        let lines = report(&refs, &Models, &[]);
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("models/missing.mdl: the model file is missing") && l.ends_with("(30 props)")),
            "{lines:#?}"
        );
        assert_eq!(lines.iter().filter(|l| l.starts_with("    prop_static")).count(), LISTED);
        assert!(lines.iter().any(|l| l == "    and 10 more"));
    }
}
