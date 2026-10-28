use std::collections::{HashMap, HashSet};

use glam::DVec3;
use hp_editor::{Document, EditError, EditResult, Id, ObjRef, SelectOp};
use hp_fgd::{PropertyType, ResolvedFgd};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathClass {
    pub class: String,
    pub next_key: String,
    pub alt_key: Option<String>,
}

const KNOWN: &[(&str, &str, Option<&str>)] = &[
    ("path_corner", "target", None),
    ("path_track", "target", Some("altpath")),
    ("path_corner_crash", "target", None),
    ("keyframe_rope", "NextKey", None),
    ("move_rope", "NextKey", None),
    ("keyframe_track", "NextKey", None),
    ("move_track", "NextKey", None),
    ("move_keyframed", "NextKey", None),
    ("scripted_sequence", "m_iszNextScript", None),
    ("assault_assaultpoint", "nextassaultpoint", None),
];

const NEXT_KEYS: &[&str] = &["target", "NextKey", "m_iszNextScript", "nextassaultpoint"];

pub const MAX_LINKS: usize = 50_000;

pub const MAX_BASE_LEN: usize = 120;

pub fn path_class(fgd: Option<&ResolvedFgd>, class: &str) -> Option<PathClass> {
    let class = class.trim();
    if class.is_empty() {
        return None;
    }
    let known = KNOWN.iter().find(|(c, _, _)| c.eq_ignore_ascii_case(class));
    if let Some(def) = fgd.and_then(|f| f.class(class)) {
        if def.is_solid() {
            return None;
        }
        let kind_chains = matches!(def.kind, hp_fgd::ClassKind::KeyFrame | hp_fgd::ClassKind::Move);
        let next = def.props.iter().find(|p| {
            let named = NEXT_KEYS.iter().any(|k| k.eq_ignore_ascii_case(&p.name));
            let typed = matches!(p.kind, PropertyType::TargetDestination | PropertyType::String);
            let drawn = def.helpers.iter().any(|h| {
                h.is("line")
                    && h.arg(1).is_some_and(|a| a.trim().eq_ignore_ascii_case("targetname"))
                    && h.arg(2).is_some_and(|a| a.trim().eq_ignore_ascii_case(&p.name))
            });
            let says_next = p.display_name.to_ascii_lowercase().contains("next");
            named && typed && (known.is_some() || kind_chains || drawn || says_next)
        });
        if let Some(p) = next {
            let alt = def
                .props
                .iter()
                .find(|q| q.name.eq_ignore_ascii_case("altpath") && q.kind == PropertyType::TargetDestination)
                .map(|q| q.name.clone())
                .or_else(|| known.and_then(|k| k.2).map(str::to_owned));
            return Some(PathClass { class: def.name.clone(), next_key: p.name.clone(), alt_key: alt });
        }
    }
    known.map(|(c, next, alt)| PathClass {
        class: (*c).to_owned(),
        next_key: (*next).to_owned(),
        alt_key: alt.map(str::to_owned),
    })
}

pub fn path_classes(fgd: Option<&ResolvedFgd>) -> Vec<PathClass> {
    let mut out: Vec<PathClass> = match fgd {
        Some(f) if !f.is_empty() => f.point_classes().iter().filter_map(|c| path_class(fgd, c)).collect(),
        _ => KNOWN.iter().filter_map(|(c, _, _)| path_class(None, c)).collect(),
    };
    let rank = |c: &PathClass| match c.class.to_ascii_lowercase().as_str() {
        "path_track" => 0,
        "path_corner" => 1,
        _ => 2,
    };
    out.sort_by(|a, b| {
        rank(a).cmp(&rank(b)).then_with(|| a.class.to_ascii_lowercase().cmp(&b.class.to_ascii_lowercase()))
    });
    out.dedup_by(|a, b| a.class.eq_ignore_ascii_case(&b.class));
    out
}

pub fn class_defaults(fgd: Option<&ResolvedFgd>, class: &str) -> Vec<(String, String)> {
    let Some(c) = fgd.and_then(|f| f.class(class)) else { return Vec::new() };
    let mut kv: Vec<(String, String)> = c
        .default_keyvalues()
        .into_iter()
        .filter(|(k, _)| !["origin", "classname", "targetname"].iter().any(|r| k.eq_ignore_ascii_case(r)))
        .collect();
    if c.spawnflags().is_some() && !kv.iter().any(|(k, _)| k.eq_ignore_ascii_case("spawnflags")) {
        kv.push(("spawnflags".to_owned(), c.default_spawnflags().to_string()));
    }
    kv
}

pub fn name_prefix(base: &str) -> String {
    let mut b: String =
        base.trim()
            .chars()
            .take(MAX_BASE_LEN)
            .map(|c| {
                if c.is_whitespace() || c.is_control() || matches!(c, '"' | ',' | '\x1b' | '*' | '!') { '_' } else { c }
            })
            .collect();
    if b.is_empty() {
        b.push_str("path");
    }
    if !b.ends_with(['_', '-', '.']) {
        b.push('_');
    }
    b
}

pub fn split_number(name: &str) -> Option<(&str, u64)> {
    let digits = name.bytes().rev().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 18 {
        return None;
    }
    let (prefix, number) = name.split_at(name.len() - digits);
    Some((prefix, number.parse().ok()?))
}

pub fn taken_names(doc: &Document) -> HashSet<String> {
    doc.scene()
        .entities()
        .values()
        .filter_map(|n| n.entity.get("targetname"))
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

fn next_free(taken: &HashSet<String>, prefix: &str, after: u64) -> u64 {
    let mut n = after.saturating_add(1);
    while taken.contains(&format!("{prefix}{n}").to_ascii_lowercase()) && n < u64::MAX {
        n += 1;
    }
    n
}

pub fn chain_names(doc: &Document, base: &str, count: usize) -> Vec<String> {
    let prefix = name_prefix(base);
    let taken = taken_names(doc);
    let mut n = 0;
    (0..count)
        .map(|_| {
            n = next_free(&taken, &prefix, n);
            format!("{prefix}{n}")
        })
        .collect()
}

pub fn name_after(doc: &Document, previous: Option<&str>, base: &str) -> String {
    let taken = taken_names(doc);
    if let Some((prefix, n)) = previous.and_then(split_number)
        && !prefix.is_empty()
    {
        return format!("{prefix}{}", next_free(&taken, prefix, n));
    }
    let prefix = name_prefix(base);
    format!("{prefix}{}", next_free(&taken, &prefix, 0))
}

#[derive(Clone, Debug, PartialEq)]
pub struct PathNode {
    pub id: Id,
    pub name: String,
    pub class: String,
    pub origin: DVec3,
    pub next_key: String,
    pub next_name: String,
    pub next: Option<Id>,
    pub alt_key: Option<String>,
    pub alt: Option<Id>,
    pub visible: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathGraph {
    pub nodes: Vec<PathNode>,
    index: HashMap<Id, usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    pub nodes: Vec<Id>,
    pub closed: bool,
}

impl PathGraph {
    pub fn build(doc: &Document, fgd: Option<&ResolvedFgd>) -> Self {
        let scene = doc.scene();
        let mut classes: HashMap<String, Option<PathClass>> = HashMap::new();
        let mut ids: Vec<Id> = scene.entities().keys().copied().collect();
        ids.sort_unstable();
        let mut nodes = Vec::new();
        for id in ids {
            let Some(node) = scene.entity(id) else { continue };
            let e = &node.entity;
            let lower = e.classname.trim().to_ascii_lowercase();
            let class = classes.entry(lower).or_insert_with(|| path_class(fgd, &e.classname)).clone();
            let Some(class) = class else { continue };
            if scene.is_brush_entity(id) {
                continue;
            }
            let Some(origin) = e.origin().filter(|o| o.is_finite()) else { continue };
            nodes.push(PathNode {
                id,
                name: e.get("targetname").unwrap_or_default().trim().to_owned(),
                class: e.classname.clone(),
                origin,
                next_name: e.get(&class.next_key).unwrap_or_default().trim().to_owned(),
                next_key: class.next_key,
                next: None,
                alt: None,
                alt_key: class.alt_key,
                visible: doc.is_entity_visible(id),
            });
        }
        let mut by_name: HashMap<String, Id> = HashMap::new();
        for n in &nodes {
            if !n.name.is_empty() {
                by_name.entry(n.name.to_ascii_lowercase()).or_insert(n.id);
            }
        }
        let mut links = 0usize;
        let lookup = |v: &str| (!v.is_empty()).then(|| by_name.get(&v.to_ascii_lowercase()).copied()).flatten();
        for node in &mut nodes {
            if links >= MAX_LINKS {
                break;
            }
            let next = lookup(&node.next_name);
            let alt = node
                .alt_key
                .as_ref()
                .and_then(|k| scene.entity(node.id).and_then(|n| n.entity.get(k)).map(|v| v.trim().to_owned()))
                .and_then(|v| lookup(&v));
            links += usize::from(next.is_some()) + usize::from(alt.is_some());
            node.next = next;
            node.alt = alt;
        }
        let index = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        Self { nodes, index }
    }

    pub fn node(&self, id: Id) -> Option<&PathNode> {
        self.index.get(&id).and_then(|i| self.nodes.get(*i))
    }

    pub fn contains(&self, id: Id) -> bool {
        self.index.contains_key(&id)
    }

    pub fn previous(&self, id: Id) -> Vec<Id> {
        self.nodes.iter().filter(|n| n.next == Some(id)).map(|n| n.id).collect()
    }

    pub fn component(&self, seeds: &[Id]) -> HashSet<Id> {
        let mut adj: HashMap<Id, Vec<Id>> = HashMap::new();
        for n in &self.nodes {
            for other in [n.next, n.alt].into_iter().flatten() {
                adj.entry(n.id).or_default().push(other);
                adj.entry(other).or_default().push(n.id);
            }
        }
        let mut seen: HashSet<Id> = HashSet::new();
        let mut stack: Vec<Id> = seeds.iter().copied().filter(|id| self.contains(*id)).collect();
        while let Some(id) = stack.pop() {
            if seen.insert(id) {
                stack.extend(adj.get(&id).into_iter().flatten().copied().filter(|o| !seen.contains(o)));
            }
        }
        seen
    }

    pub fn walk(&self, id: Id) -> (Vec<Id>, bool) {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut cur = Some(id);
        while let Some(c) = cur {
            if !self.contains(c) {
                break;
            }
            if !seen.insert(c) || out.len() >= MAX_LINKS {
                return (out, true);
            }
            out.push(c);
            cur = self.node(c).and_then(|n| n.next);
        }
        (out, false)
    }

    pub fn chain_start(&self, id: Id) -> Id {
        let mut cur = id;
        let mut seen = HashSet::new();
        seen.insert(id);
        loop {
            let prev = self.previous(cur);
            match prev.first() {
                Some(p) if seen.insert(*p) => cur = *p,
                Some(_) => return id,
                None => return cur,
            }
        }
    }

    pub fn chains(&self) -> Vec<Chain> {
        let mut has_prev: HashSet<Id> = HashSet::new();
        for n in &self.nodes {
            if let Some(next) = n.next {
                has_prev.insert(next);
            }
        }
        let mut done: HashSet<Id> = HashSet::new();
        let mut out = Vec::new();
        let starts: Vec<Id> = self.nodes.iter().filter(|n| !has_prev.contains(&n.id)).map(|n| n.id).collect();
        for start in starts.into_iter().chain(self.nodes.iter().map(|n| n.id)) {
            if done.contains(&start) {
                continue;
            }
            let (walked, closed) = self.walk(start);
            let mut nodes = Vec::new();
            for id in walked {
                if !done.insert(id) {
                    break;
                }
                nodes.push(id);
            }
            if !nodes.is_empty() {
                out.push(Chain { nodes, closed });
            }
        }
        out
    }
}

pub const UNDO_ADD: &str = "Add path point";
pub const UNDO_CREATE: &str = "Create path";
pub const UNDO_CLOSE: &str = "Close path loop";
pub const UNDO_LINK: &str = "Link path point";
pub const UNDO_INSERT: &str = "Insert path point";
pub const UNDO_REMOVE: &str = "Remove path point";

#[derive(Clone, Debug, PartialEq)]
pub struct ChainSpec<'a> {
    pub class: &'a PathClass,
    pub base: &'a str,
    pub points: &'a [DVec3],
    pub closed: bool,
    pub keyvalues: &'a [(String, String)],
}

fn set(kv: &mut Vec<(String, String)>, key: &str, value: &str) {
    match kv.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
        Some(slot) => value.clone_into(&mut slot.1),
        None => kv.push((key.to_owned(), value.to_owned())),
    }
}

fn check_point(p: DVec3) -> EditResult<()> {
    if p.is_finite() && p.abs().max_element() <= 131_072.0 {
        Ok(())
    } else {
        Err(EditError::Invalid("path point outside the world".to_owned()))
    }
}

pub fn create_chain(doc: &mut Document, spec: &ChainSpec<'_>) -> EditResult<Vec<(Id, String)>> {
    if spec.points.is_empty() {
        return Err(EditError::Nothing("a path needs points"));
    }
    if spec.points.len() > 4096 {
        return Err(EditError::Invalid("a path holds at most 4096 points".to_owned()));
    }
    for p in spec.points {
        check_point(*p)?;
    }
    let names = chain_names(doc, spec.base, spec.points.len());
    let n = names.len();
    doc.transaction(UNDO_CREATE, |doc| {
        let mut out = Vec::with_capacity(n);
        for (i, (p, name)) in spec.points.iter().zip(&names).enumerate() {
            let mut kv = spec.keyvalues.to_vec();
            set(&mut kv, "targetname", name);
            let next = if i + 1 < n {
                Some(names[i + 1].as_str())
            } else if spec.closed && n >= 2 {
                Some(names[0].as_str())
            } else {
                None
            };
            match next {
                Some(next) => set(&mut kv, &spec.class.next_key, next),
                None => kv.retain(|(k, _)| !k.eq_ignore_ascii_case(&spec.class.next_key)),
            }
            let id = doc.create_point_entity(&spec.class.class, *p, &kv)?;
            out.push((id, name.clone()));
        }
        let objs: Vec<ObjRef> = out.iter().map(|(id, _)| ObjRef::Entity(*id)).collect();
        doc.select(&objs, SelectOp::Replace);
        Ok(out)
    })
}

pub fn add_point(
    doc: &mut Document,
    fgd: Option<&ResolvedFgd>,
    class: &PathClass,
    keyvalues: &[(String, String)],
    previous: Option<Id>,
    base: &str,
    pos: DVec3,
) -> EditResult<(Id, String)> {
    check_point(pos)?;
    let prev_name = match previous {
        Some(p) => {
            let e = doc.scene().entity(p).ok_or(EditError::NotFound(ObjRef::Entity(p)))?;
            Some(e.entity.get("targetname").unwrap_or_default().trim().to_owned()).filter(|n| !n.is_empty())
        }
        None => None,
    };
    let name = name_after(doc, prev_name.as_deref(), base);
    doc.transaction(UNDO_ADD, |doc| {
        if let Some(p) = previous {
            let pname = match &prev_name {
                Some(n) => n.clone(),
                None => {
                    let n = name_after(doc, None, base);
                    doc.edit_entity(p, |e| e.set("targetname", n.clone()))?;
                    n
                }
            };
            let next_key = previous_next_key(doc, fgd, p, class);
            let mut kv = keyvalues.to_vec();
            kv.retain(|(k, _)| !k.eq_ignore_ascii_case(&class.next_key));
            let new_name =
                if pname.eq_ignore_ascii_case(&name) { name_after(doc, Some(&pname), base) } else { name.clone() };
            set(&mut kv, "targetname", &new_name);
            let id = doc.create_point_entity(&class.class, pos, &kv)?;
            doc.edit_entity(p, |e| e.set(&next_key, new_name.clone()))?;
            return Ok((id, new_name));
        }
        let mut kv = keyvalues.to_vec();
        kv.retain(|(k, _)| !k.eq_ignore_ascii_case(&class.next_key));
        set(&mut kv, "targetname", &name);
        let id = doc.create_point_entity(&class.class, pos, &kv)?;
        Ok((id, name.clone()))
    })
}

fn previous_next_key(doc: &Document, fgd: Option<&ResolvedFgd>, id: Id, class: &PathClass) -> String {
    let own = doc.scene().entity(id).map(|n| n.entity.classname.clone()).unwrap_or_default();
    if own.eq_ignore_ascii_case(&class.class) {
        return class.next_key.clone();
    }
    path_class(fgd, &own).map_or_else(|| class.next_key.clone(), |c| c.next_key)
}

pub fn link(
    doc: &mut Document,
    fgd: Option<&ResolvedFgd>,
    from: Id,
    to: Id,
    base: &str,
    label: &str,
) -> EditResult<bool> {
    let scene = doc.scene();
    let src = scene.entity(from).ok_or(EditError::NotFound(ObjRef::Entity(from)))?;
    let dst = scene.entity(to).ok_or(EditError::NotFound(ObjRef::Entity(to)))?;
    let class = path_class(fgd, &src.entity.classname)
        .ok_or_else(|| EditError::Invalid(format!("{} does not link to a next point", src.entity.classname)))?;
    let to_name = dst.entity.get("targetname").map(str::trim).filter(|n| !n.is_empty()).map(str::to_owned);
    if let Some(n) = &to_name
        && src.entity.get(&class.next_key).is_some_and(|v| v.trim().eq_ignore_ascii_case(n))
    {
        return Ok(false);
    }
    doc.transaction(label, |doc| {
        let name = match to_name {
            Some(n) => n,
            None => {
                let n = name_after(doc, None, base);
                doc.edit_entity(to, |e| e.set("targetname", n.clone()))?;
                n
            }
        };
        doc.edit_entity(from, |e| e.set(&class.next_key, name))?;
        Ok(true)
    })
}

fn reference_keys(fgd: Option<&ResolvedFgd>, e: &hp_vmf::Entity) -> Vec<String> {
    const USUAL: &[&str] =
        &["target", "altpath", "NextKey", "m_iszNextScript", "nextassaultpoint", "parentname", "pathcorner"];
    let def = fgd.and_then(|f| f.class(&e.classname));
    e.props
        .iter()
        .map(|(k, _)| k)
        .filter(|k| match def.and_then(|d| d.property(k)) {
            Some(p) => matches!(p.kind, PropertyType::TargetDestination | PropertyType::TargetNameOrClass),
            None => USUAL.iter().any(|u| u.eq_ignore_ascii_case(k)),
        })
        .cloned()
        .collect()
}

fn rename_node(doc: &mut Document, fgd: Option<&ResolvedFgd>, id: Id, new: &str) -> EditResult<()> {
    let old = doc
        .scene()
        .entity(id)
        .and_then(|n| n.entity.get("targetname"))
        .map(|s| s.trim().to_owned())
        .unwrap_or_default();
    doc.edit_entity(id, |e| e.set("targetname", new))?;
    if old.is_empty() || old.eq_ignore_ascii_case(new) {
        return Ok(());
    }
    let mut ids: Vec<Id> = doc.scene().entities().keys().copied().collect();
    ids.sort_unstable();
    for other in ids {
        let Some(e) = doc.scene().entity(other).map(|n| &n.entity) else { continue };
        let keys: Vec<String> = reference_keys(fgd, e)
            .into_iter()
            .filter(|k| e.get(k).is_some_and(|v| v.trim().eq_ignore_ascii_case(&old)))
            .collect();
        let outputs = e
            .connections
            .iter()
            .any(|c| c.target.eq_ignore_ascii_case(&old) || c.param.trim().eq_ignore_ascii_case(&old));
        if keys.is_empty() && !outputs {
            continue;
        }
        doc.edit_entity(other, |e| {
            for k in &keys {
                e.set(k, new);
            }
            for c in &mut e.connections {
                if c.target.eq_ignore_ascii_case(&old) {
                    new.clone_into(&mut c.target);
                }
                if c.param.trim().eq_ignore_ascii_case(&old) {
                    new.clone_into(&mut c.param);
                }
            }
        })?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Inserted {
    pub id: Id,
    pub name: String,
    pub origin: DVec3,
    pub renamed: Vec<(Id, String, String)>,
}

pub fn insert_position(graph: &PathGraph, node: Id) -> Option<DVec3> {
    let n = graph.node(node)?;
    if let Some(next) = n.next.and_then(|x| graph.node(x)) {
        return Some((n.origin + next.origin) * 0.5);
    }
    let prev = graph.previous(node).first().and_then(|p| graph.node(*p)).map(|p| p.origin);
    let step = prev.map(|p| n.origin - p).filter(|d| d.length_squared() > 1e-6).unwrap_or(DVec3::new(64.0, 0.0, 0.0));
    Some(n.origin + step)
}

pub fn insert_after(
    doc: &mut Document,
    fgd: Option<&ResolvedFgd>,
    node: Id,
    pos: Option<DVec3>,
    base: &str,
) -> EditResult<Inserted> {
    let graph = PathGraph::build(doc, fgd);
    let n = graph.node(node).ok_or_else(|| EditError::Invalid(format!("entity {node} is not a path point")))?.clone();
    let pos = pos.or_else(|| insert_position(&graph, node)).unwrap_or(n.origin);
    check_point(pos)?;
    let class = path_class(fgd, &n.class).ok_or_else(|| EditError::Invalid(format!("{} does not chain", n.class)))?;
    let taken = taken_names(doc);
    let mut renames: Vec<(Id, String, String)> = Vec::new();
    let own_name = n.name.is_empty().then(|| name_after(doc, None, base));
    let mut taken = taken;
    taken.extend(own_name.iter().map(|o| o.to_ascii_lowercase()));
    let name = own_name.clone().unwrap_or_else(|| n.name.clone());
    let new_name = match split_number(&name).filter(|(p, _)| !p.is_empty()) {
        Some((prefix, k)) => {
            let mut run: Vec<(Id, u64)> = Vec::new();
            let mut expected = k.saturating_add(1);
            let mut cur = n.next;
            while let Some(c) = cur {
                if c == node || run.iter().any(|(r, _)| *r == c) {
                    break;
                }
                let Some(cn) = graph.node(c) else { break };
                match split_number(&cn.name) {
                    Some((p, num)) if p.eq_ignore_ascii_case(prefix) && num == expected => {
                        run.push((c, num));
                        expected = expected.saturating_add(1);
                        cur = cn.next;
                    }
                    _ => break,
                }
            }
            let after_run = format!("{prefix}{expected}");
            if run.is_empty() {
                format!("{prefix}{}", next_free(&taken, prefix, k))
            } else if !taken.contains(&after_run.to_ascii_lowercase()) {
                for (id, num) in run.iter().rev() {
                    let old = graph.node(*id).map(|x| x.name.clone()).unwrap_or_default();
                    renames.push((*id, old, format!("{prefix}{}", num + 1)));
                }
                format!("{prefix}{}", k + 1)
            } else {
                let max = taken
                    .iter()
                    .filter_map(|t| split_number(t).filter(|(p, _)| p.eq_ignore_ascii_case(prefix)).map(|(_, x)| x))
                    .max()
                    .unwrap_or(k);
                format!("{prefix}{}", next_free(&taken, prefix, max))
            }
        }
        None => {
            let prefix = name_prefix(&name);
            format!("{prefix}{}", next_free(&taken, &prefix, 0))
        }
    };
    let mut kv = class_defaults(fgd, &class.class);
    doc.transaction(UNDO_INSERT, |doc| {
        if let Some(own) = &own_name {
            doc.edit_entity(node, |e| e.set("targetname", own.clone()))?;
        }
        for (id, _, new) in &renames {
            rename_node(doc, fgd, *id, new)?;
        }
        let next_value = doc
            .scene()
            .entity(node)
            .and_then(|e| e.entity.get(&class.next_key))
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty());
        set(&mut kv, "targetname", &new_name);
        match &next_value {
            Some(v) => set(&mut kv, &class.next_key, v),
            None => kv.retain(|(k, _)| !k.eq_ignore_ascii_case(&class.next_key)),
        }
        let id = doc.create_point_entity(&n.class, pos, &kv)?;
        doc.edit_entity(node, |e| e.set(&class.next_key, new_name.clone()))?;
        doc.select(&[ObjRef::Entity(id)], SelectOp::Replace);
        Ok(Inserted { id, name: new_name.clone(), origin: pos, renamed: renames.clone() })
    })
}

pub fn remove_points(
    doc: &mut Document,
    fgd: Option<&ResolvedFgd>,
    objs: &[ObjRef],
    relink: bool,
) -> EditResult<usize> {
    let existing: Vec<ObjRef> = objs
        .iter()
        .copied()
        .filter(|o| match o {
            ObjRef::Entity(id) => doc.scene().entity(*id).is_some(),
            ObjRef::Solid(id) => doc.scene().solid(*id).is_some(),
        })
        .collect();
    if existing.is_empty() {
        return Err(EditError::Nothing("no path point to remove"));
    }
    doc.transaction(UNDO_REMOVE, |doc| {
        if relink {
            for o in &existing {
                let ObjRef::Entity(id) = *o else { continue };
                relink_around(doc, fgd, id)?;
            }
        }
        doc.delete(&existing)
    })
}

fn relink_around(doc: &mut Document, fgd: Option<&ResolvedFgd>, id: Id) -> EditResult<()> {
    let graph = PathGraph::build(doc, fgd);
    let Some(n) = graph.node(id).cloned() else { return Ok(()) };
    if n.name.is_empty() {
        return Ok(());
    }
    let next = Some(n.next_name.clone()).filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case(&n.name));
    let mut ids: Vec<Id> = doc.scene().entities().keys().copied().filter(|o| *o != id).collect();
    ids.sort_unstable();
    for other in ids {
        let Some(e) = doc.scene().entity(other).map(|x| &x.entity) else { continue };
        let mut keys: Vec<String> = vec!["target".to_owned()];
        if let Some(c) = path_class(fgd, &e.classname) {
            keys.push(c.next_key);
            keys.extend(c.alt_key);
        }
        let keys: Vec<String> =
            keys.into_iter().filter(|k| e.get(k).is_some_and(|v| v.trim().eq_ignore_ascii_case(&n.name))).collect();
        if keys.is_empty() {
            continue;
        }
        let own = e.get("targetname").unwrap_or_default().trim().to_owned();
        let value = next.clone().filter(|v| !v.eq_ignore_ascii_case(&own));
        doc.edit_entity(other, |e| {
            for k in &keys {
                match &value {
                    Some(v) => e.set(k, v.clone()),
                    None => {
                        e.remove(k);
                    }
                }
            }
        })?;
    }
    Ok(())
}
