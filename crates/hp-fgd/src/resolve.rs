use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use crate::model::{
    ClassKind, EntityClass, Fgd, FlagDef, Helper, IoDef, MAX_HELPERS_PER_CLASS, Property, PropertyType,
};
use crate::named::{HelperList, NamedList, lower, singleton_group};

pub const DEFAULT_POINT_BOUNDS: ([f64; 3], [f64; 3]) = ([-8.0; 3], [8.0; 3]);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioHelper {
    pub model: Option<String>,
    pub is_prop: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedClass {
    pub kind: ClassKind,
    pub name: String,
    pub description: String,
    pub bases: Vec<String>,
    pub helpers: Vec<Helper>,
    pub props: Vec<Property>,
    pub inputs: Vec<IoDef>,
    pub outputs: Vec<IoDef>,
}

const MAX_INHERITED_PER_CLASS: usize = 16_384;

const MAX_INHERITED_TOTAL: usize = 1 << 20;

fn merge_flags(dst: &mut Vec<FlagDef>, old: Vec<FlagDef>) {
    let present: HashSet<u32> = dst.iter().map(|f| f.value).collect();
    let before = dst.len();
    dst.extend(old.into_iter().filter(|f| !present.contains(&f.value)));
    if dst.len() != before {
        dst.sort_by_key(|f| f.value);
    }
}

fn merge_prop(slot: &mut Property, new: &Property, keep_default: bool) {
    let old = std::mem::replace(slot, new.clone());
    if slot.display_name.is_empty() {
        slot.display_name = old.display_name;
    }
    if slot.description.is_empty() {
        slot.description = old.description;
    }
    if slot.default.is_none() && keep_default {
        slot.default = old.default;
    }
    if keep_default {
        slot.report |= old.report;
    }
    if slot.kind == PropertyType::Flags && old.kind == PropertyType::Flags {
        merge_flags(&mut slot.flags, old.flags);
    } else if slot.choices.is_empty() && slot.kind == old.kind {
        slot.choices = old.choices;
    }
}

fn add_prop(list: &mut NamedList<Property>, new: &Property, keep_default: bool) {
    match list.get_mut(&new.name) {
        Some(slot) => merge_prop(slot, new, keep_default),
        None => list.push(new.clone()),
    }
}

fn add_io(list: &mut NamedList<IoDef>, new: &IoDef) {
    match list.get_mut(&new.name) {
        Some(slot) => {
            let old_description = std::mem::take(&mut slot.description);
            *slot = new.clone();
            if slot.description.is_empty() {
                slot.description = old_description;
            }
        }
        None => list.push(new.clone()),
    }
}

fn apply_extensions<'e>(
    target: &mut EntityClass,
    exts: impl Iterator<Item = &'e EntityClass>,
    warnings: &mut Vec<String>,
) {
    let mut props = NamedList::new(std::mem::take(&mut target.props));
    let mut inputs = NamedList::new(std::mem::take(&mut target.inputs));
    let mut outputs = NamedList::new(std::mem::take(&mut target.outputs));
    let mut truncated = false;
    for ext in exts {
        let own = ext.helpers.iter().filter(|h| !h.is("base"));
        let names: HashSet<String> = own.clone().map(|h| h.name.to_ascii_lowercase()).collect();
        let groups: Vec<&str> = own.filter_map(|h| singleton_group(&h.name)).collect();
        if !names.is_empty() {
            target.helpers.retain(|h| {
                h.is("base")
                    || !(names.contains(lower(&h.name).as_ref())
                        || singleton_group(&h.name).is_some_and(|g| groups.contains(&g)))
            });
        }
        for helper in &ext.helpers {
            if target.helpers.iter().any(|h| h.same_as(helper)) {
                continue;
            }
            if target.helpers.len() >= MAX_HELPERS_PER_CLASS {
                truncated = true;
                break;
            }
            target.helpers.push(helper.clone());
        }
        for prop in &ext.props {
            add_prop(&mut props, prop, true);
        }
        for io in &ext.inputs {
            add_io(&mut inputs, io);
        }
        for io in &ext.outputs {
            add_io(&mut outputs, io);
        }
        if !ext.description.trim().is_empty() {
            target.description.clone_from(&ext.description);
        }
    }
    if truncated {
        warnings.push(format!("class {}: too many helpers from extension classes, extra ones ignored", target.name));
    }
    target.props = props.into_vec();
    target.inputs = inputs.into_vec();
    target.outputs = outputs.into_vec();
}

fn picker_order(a: &str, b: &str) -> std::cmp::Ordering {
    a.bytes().map(|c| c.to_ascii_lowercase()).cmp(b.bytes().map(|c| c.to_ascii_lowercase())).then_with(|| a.cmp(b))
}

#[derive(Clone, Copy, Default)]
struct Slot {
    base: Option<usize>,
    other: Option<usize>,
}

struct Index {
    slots: HashMap<String, Slot>,
}

impl Index {
    fn build(classes: &[Cow<'_, EntityClass>]) -> (Self, Vec<bool>) {
        let mut slots: HashMap<String, Slot> = HashMap::with_capacity(classes.len());
        let mut live = vec![true; classes.len()];
        for (i, class) in classes.iter().enumerate() {
            let slot = slots.entry(class.name.trim().to_ascii_lowercase()).or_default();
            let field = if class.kind.is_base() { &mut slot.base } else { &mut slot.other };
            if let Some(old) = field.replace(i)
                && let Some(flag) = live.get_mut(old)
            {
                *flag = false;
            }
        }
        (Self { slots }, live)
    }

    fn slot(&self, name: &str) -> Option<Slot> {
        self.slots.get(lower(name.trim()).as_ref()).copied()
    }

    fn pick(
        classes: &[Cow<'_, EntityClass>],
        name: &str,
        candidates: [Option<usize>; 2],
        exclude: Option<usize>,
    ) -> Option<usize> {
        let mut first = None;
        for c in candidates.into_iter().flatten().filter(|&c| Some(c) != exclude) {
            if classes.get(c).is_some_and(|class| class.name == name) {
                return Some(c);
            }
            first = first.or(Some(c));
        }
        first
    }

    fn lookup_base(&self, classes: &[Cow<'_, EntityClass>], name: &str, from: usize) -> Option<usize> {
        let slot = self.slot(name)?;
        Self::pick(classes, name.trim(), [slot.base, slot.other], Some(from))
    }

    fn lookup_target(&self, classes: &[Cow<'_, EntityClass>], name: &str) -> Option<usize> {
        let slot = self.slot(name)?;
        Self::pick(classes, name.trim(), [slot.other, slot.base], None)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    New,
    Active,
    Done,
}

fn item_count(class: &ResolvedClass) -> usize {
    class.props.len() + class.inputs.len() + class.outputs.len() + class.helpers.len() + class.bases.len()
}

fn flatten_one(
    i: usize,
    classes: &[Cow<'_, EntityClass>],
    index: &Index,
    flat: &[Option<ResolvedClass>],
    warnings: &mut Vec<String>,
    budget: &mut usize,
) -> Option<ResolvedClass> {
    let class = classes.get(i)?;
    let mut bases: Vec<String> = Vec::new();
    let mut seen_bases: HashSet<String> = HashSet::new();
    let mut helpers = HelperList::default();
    let mut props = NamedList::new(Vec::new());
    let mut inputs = NamedList::new(Vec::new());
    let mut outputs = NamedList::new(Vec::new());
    let mut inherited = 0usize;

    for base_name in class.bases() {
        let Some(b) = index.lookup_base(classes, base_name, i) else {
            warnings.push(format!("class {}: unknown base class {base_name}", class.name));
            continue;
        };
        let Some(base) = flat.get(b).and_then(Option::as_ref) else {
            warnings.push(format!("class {}: inheritance cycle through base {base_name}", class.name));
            continue;
        };
        if seen_bases.contains(lower(&base.name).as_ref()) {
            continue;
        }
        let cost = item_count(base) + 1;
        if inherited + cost > MAX_INHERITED_PER_CLASS || cost > *budget {
            warnings.push(format!("class {}: base {base_name} skipped, inheritance too large", class.name));
            continue;
        }
        inherited += cost;
        *budget -= cost;

        for name in std::iter::once(&base.name).chain(&base.bases) {
            if seen_bases.insert(name.to_ascii_lowercase()) {
                bases.push(name.clone());
            }
        }
        if helpers.len() == 0 {
            helpers = HelperList::new(base.helpers.clone());
        } else {
            base.helpers.iter().for_each(|h| helpers.add(h));
        }
        if props.is_empty() {
            props = NamedList::new(base.props.clone());
        } else {
            base.props.iter().for_each(|p| add_prop(&mut props, p, false));
        }
        if inputs.is_empty() {
            inputs = NamedList::new(base.inputs.clone());
        } else {
            base.inputs.iter().for_each(|d| add_io(&mut inputs, d));
        }
        if outputs.is_empty() {
            outputs = NamedList::new(base.outputs.clone());
        } else {
            base.outputs.iter().for_each(|d| add_io(&mut outputs, d));
        }
    }

    for helper in class.helpers.iter().filter(|h| !h.is("base")) {
        helpers.add(helper);
    }
    for prop in &class.props {
        add_prop(&mut props, prop, false);
    }
    for io in &class.inputs {
        add_io(&mut inputs, io);
    }
    for io in &class.outputs {
        add_io(&mut outputs, io);
    }
    Some(ResolvedClass {
        kind: class.kind.clone(),
        name: class.name.clone(),
        description: class.description.clone(),
        bases,
        helpers: helpers.into_vec(),
        props: props.into_vec(),
        inputs: inputs.into_vec(),
        outputs: outputs.into_vec(),
    })
}

pub(crate) fn resolve(fgd: &Fgd) -> ResolvedFgd {
    let mut warnings = Vec::new();
    let mut classes: Vec<Cow<'_, EntityClass>> =
        fgd.classes.iter().filter(|c| !c.kind.is_extension()).map(Cow::Borrowed).collect();
    let (index, live) = Index::build(&classes);

    let mut extensions: Vec<(usize, &EntityClass)> = Vec::new();
    for ext in fgd.classes.iter().filter(|c| c.kind.is_extension()) {
        match index.lookup_target(&classes, &ext.name) {
            Some(target) => extensions.push((target, ext)),
            None if ext.kind == ClassKind::Extend => {}
            None => {
                warnings.push(format!("@{} {}: no class with this name to merge into", ext.kind.directive(), ext.name))
            }
        }
    }
    extensions.sort_by_key(|&(target, _)| target);
    for group in extensions.chunk_by(|a, b| a.0 == b.0) {
        if let Some(target) = group.first().and_then(|&(t, _)| classes.get_mut(t)) {
            apply_extensions(target.to_mut(), group.iter().map(|&(_, ext)| ext), &mut warnings);
        }
    }

    let n = classes.len();
    let mut marks = vec![Mark::New; n];
    let mut flat: Vec<Option<ResolvedClass>> = vec![None; n];
    let mut stack: Vec<(usize, bool)> = Vec::new();
    let mut budget = MAX_INHERITED_TOTAL;
    for root in (0..n).filter(|&i| live.get(i).copied().unwrap_or(false)) {
        if marks.get(root) != Some(&Mark::New) {
            continue;
        }
        stack.push((root, false));
        while let Some((i, expanded)) = stack.pop() {
            if expanded {
                let resolved = flatten_one(i, &classes, &index, &flat, &mut warnings, &mut budget);
                if let (Some(slot), Some(mark)) = (flat.get_mut(i), marks.get_mut(i)) {
                    *slot = resolved;
                    *mark = Mark::Done;
                }
                continue;
            }
            match marks.get_mut(i) {
                Some(mark @ Mark::New) => *mark = Mark::Active,
                _ => continue,
            }
            stack.push((i, true));
            if let Some(class) = classes.get(i) {
                for base in class.bases() {
                    if let Some(b) = index.lookup_base(&classes, base, i)
                        && marks.get(b) == Some(&Mark::New)
                    {
                        stack.push((b, false));
                    }
                }
            }
        }
    }

    let mut map: HashMap<String, ResolvedClass> = HashMap::with_capacity(index.slots.len());
    for (key, slot) in index.slots {
        if let Some(resolved) = slot.other.or(slot.base).and_then(|i| flat.get_mut(i)?.take()) {
            map.insert(key, resolved);
        }
    }

    let sorted = |pred: fn(&ResolvedClass) -> bool| {
        let mut names: Vec<String> = map.values().filter(|c| pred(c)).map(|c| c.name.clone()).collect();
        names.sort_unstable_by(|a, b| picker_order(a, b));
        names
    };
    let point_names = sorted(ResolvedClass::is_point);
    let solid_names = sorted(ResolvedClass::is_solid);

    ResolvedFgd {
        classes: map,
        point_names,
        solid_names,
        map_size: fgd.map_size,
        material_exclusions: fgd.material_exclusions.clone(),
        auto_visgroups: fgd.auto_visgroups.clone(),
        warnings,
    }
}

impl Fgd {
    pub fn resolve(&self) -> ResolvedFgd {
        resolve(self)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ResolvedFgd {
    classes: HashMap<String, ResolvedClass>,
    point_names: Vec<String>,
    solid_names: Vec<String>,
    pub map_size: Option<(i32, i32)>,
    pub material_exclusions: Vec<String>,
    pub auto_visgroups: Vec<crate::AutoVisGroup>,
    pub warnings: Vec<String>,
}

impl ResolvedFgd {
    pub fn remove_class(&mut self, name: &str) -> bool {
        let key = lower(name.trim()).into_owned();
        let Some(class) = self.classes.remove(&key) else { return false };
        let names = if class.is_solid() { &mut self.solid_names } else { &mut self.point_names };
        names.retain(|n| !n.eq_ignore_ascii_case(name.trim()));
        true
    }

    pub fn class(&self, name: &str) -> Option<&ResolvedClass> {
        self.classes.get(lower(name.trim()).as_ref())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.class(name).is_some()
    }

    pub fn classes(&self) -> &HashMap<String, ResolvedClass> {
        &self.classes
    }

    pub fn iter(&self) -> impl Iterator<Item = &ResolvedClass> + '_ {
        self.classes.values()
    }

    pub fn len(&self) -> usize {
        self.classes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }

    pub fn point_classes(&self) -> &[String] {
        &self.point_names
    }

    pub fn solid_classes(&self) -> &[String] {
        &self.solid_names
    }

    pub fn class_names_where(&self, pred: impl Fn(&ResolvedClass) -> bool) -> Vec<&str> {
        let mut names: Vec<&str> = self.classes.values().filter(|c| pred(c)).map(|c| c.name.as_str()).collect();
        names.sort_unstable_by(|a, b| picker_order(a, b));
        names
    }

    pub fn all_inputs(&self) -> Vec<&IoDef> {
        self.all_io(|c| &c.inputs)
    }

    pub fn all_outputs(&self) -> Vec<&IoDef> {
        self.all_io(|c| &c.outputs)
    }

    fn all_io<'s>(&'s self, list: impl Fn(&'s ResolvedClass) -> &'s Vec<IoDef>) -> Vec<&'s IoDef> {
        let mut all: Vec<&IoDef> = self.classes.values().filter(|c| !c.is_base()).flat_map(list).collect();
        all.sort_unstable_by(|a, b| picker_order(&a.name, &b.name));
        all.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
        all
    }
}

impl ResolvedClass {
    pub fn is_point(&self) -> bool {
        self.kind.is_point()
    }

    pub fn is_solid(&self) -> bool {
        self.kind.is_solid()
    }

    pub fn is_base(&self) -> bool {
        self.kind.is_base()
    }

    pub fn derives_from(&self, name: &str) -> bool {
        self.bases.iter().any(|b| b.eq_ignore_ascii_case(name))
    }

    pub fn has_helper(&self, name: &str) -> bool {
        self.helpers.iter().any(|h| h.is(name))
    }

    pub fn helper(&self, name: &str) -> Option<&Helper> {
        self.helpers.iter().rev().find(|h| h.is(name))
    }

    pub fn helpers_named<'s>(&'s self, name: &'s str) -> impl Iterator<Item = &'s Helper> + 's {
        self.helpers.iter().filter(move |h| h.is(name))
    }

    pub fn size(&self) -> Option<([f64; 3], [f64; 3])> {
        let numbers = self.helper("size")?.numbers()?;
        match *numbers.as_slice() {
            [x, y, z] => {
                let half = [x.abs() / 2.0, y.abs() / 2.0, z.abs() / 2.0];
                Some((half.map(|v| -v), half))
            }
            [a, b, c, d, e, f] => Some(([a.min(d), b.min(e), c.min(f)], [a.max(d), b.max(e), c.max(f)])),
            _ => None,
        }
    }

    pub fn bounds_or_default(&self) -> ([f64; 3], [f64; 3]) {
        self.size().unwrap_or(DEFAULT_POINT_BOUNDS)
    }

    pub fn color(&self) -> Option<[u8; 3]> {
        let numbers = self.helper("color")?.numbers()?;
        match *numbers.as_slice() {
            [r, g, b, ..] => Some([r, g, b].map(|v| v.round().clamp(0.0, 255.0) as u8)),
            _ => None,
        }
    }

    pub fn studio(&self) -> Option<StudioHelper> {
        let helper = self.helpers.iter().rev().find(|h| singleton_group(&h.name) == Some("studio"))?;
        Some(StudioHelper { model: helper.first_arg().map(str::to_owned), is_prop: !helper.is("studio") })
    }

    pub fn iconsprite(&self) -> Option<&str> {
        self.helper("iconsprite")?.first_arg()
    }

    pub fn sprite(&self) -> Option<&str> {
        self.helper("sprite")?.first_arg()
    }

    pub fn sphere_props(&self) -> Vec<&str> {
        self.helpers_named("sphere").map(|h| h.first_arg().unwrap_or("radius")).collect()
    }

    pub fn property(&self, name: &str) -> Option<&Property> {
        self.props.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }

    pub fn input(&self, name: &str) -> Option<&IoDef> {
        self.inputs.iter().find(|d| d.name.eq_ignore_ascii_case(name))
    }

    pub fn output(&self, name: &str) -> Option<&IoDef> {
        self.outputs.iter().find(|d| d.name.eq_ignore_ascii_case(name))
    }

    pub fn spawnflags(&self) -> Option<&Property> {
        self.property("spawnflags")
    }

    pub fn default_spawnflags(&self) -> u32 {
        self.spawnflags().map_or(0, Property::default_flags)
    }

    pub fn default_keyvalues(&self) -> Vec<(String, String)> {
        self.props
            .iter()
            .filter_map(|p| {
                let value = p.default_value()?;
                (!value.is_empty()).then(|| (p.name.clone(), value))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve_src(src: &str) -> ResolvedFgd {
        let fgd = Fgd::parse(src, None);
        assert!(fgd.warnings.is_empty(), "parse warnings: {:#?}", fgd.warnings);
        fgd.resolve()
    }

    const DOORS: &str = r#"
        @BaseClass = Targetname
        [
            targetname(target_source) : "Name" : : "The name."
            input Kill(void) : "Removes this entity."
        ]
        @BaseClass size(-8 -8 -8, 8 8 8) color(0 0 255) = Sized []
        @BaseClass base(Targetname) = Door
        [
            speed(integer) : "Speed" : 100
            spawnflags(flags) =
            [
                1 : "Starts Open" : 0
                1024 : "Touch Opens" : 1
            ]
            input Open(void) : "Open the door."
            input Close(void) : "Close the door."
            output OnOpen(void) : "Fired when opening."
        ]
        @SolidClass base(Door) = func_door : "A door."
        [
            movedir(angle) : "Move Direction" : "0 0 0"
        ]
        @SolidClass base(Door, Sized) = func_door_rotating : "A rotating door."
        [
            spawnflags(flags) =
            [
                2 : "Reverse Dir" : 0
                1024 : "Touch Opens (rotating)" : 0
                16 : "One-way" : 1
            ]
            speed(integer) : "Rotation speed" : 200
            input Open(void)
        ]
    "#;

    #[test]
    fn inheritance_flattens_in_order() {
        let r = resolve_src(DOORS);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let door = r.class("FUNC_DOOR").unwrap();
        let names: Vec<_> = door.props.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["targetname", "speed", "spawnflags", "movedir"]);
        assert_eq!(door.bases, ["Door", "Targetname"]);
        assert!(door.derives_from("targetname"));
        assert!(door.input("open").is_some() && door.input("Close").is_some());
        assert!(door.input("Kill").is_some());
        assert!(door.output("OnOpen").is_some());
        assert_eq!(door.default_spawnflags(), 1024);
        assert!(door.is_solid() && !door.is_point());
    }

    #[test]
    fn child_overrides_in_place_and_flags_merge() {
        let r = resolve_src(DOORS);
        let rot = r.class("func_door_rotating").unwrap();
        let names: Vec<_> = rot.props.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["targetname", "speed", "spawnflags"]);
        let speed = rot.property("speed").unwrap();
        assert_eq!(speed.display_name, "Rotation speed");
        assert_eq!(speed.default.as_deref(), Some("200"));
        let flags: Vec<_> = rot.spawnflags().unwrap().flags.iter().map(|f| (f.value, f.default_on)).collect();
        assert_eq!(flags, [(1, false), (2, false), (16, true), (1024, false)]);
        assert_eq!(rot.default_spawnflags(), 16);
        assert_eq!(rot.input("Open").unwrap().description, "Open the door.");
        assert_eq!(rot.size(), Some(([-8.0; 3], [8.0; 3])));
        assert_eq!(rot.color(), Some([0, 0, 255]));
        assert_eq!(rot.bases, ["Door", "Targetname", "Sized"]);
    }

    #[test]
    fn pickers_exclude_base_classes() {
        let r = resolve_src(DOORS);
        assert_eq!(r.solid_classes(), ["func_door", "func_door_rotating"]);
        assert!(r.point_classes().is_empty());
        assert!(r.class("Door").unwrap().is_base());
        assert_eq!(r.len(), 5);
        assert_eq!(r.class_names_where(ResolvedClass::is_base), ["Door", "Sized", "Targetname"]);
        let inputs: Vec<_> = r.all_inputs().iter().map(|d| d.name.as_str()).collect();
        assert_eq!(inputs, ["Close", "Kill", "Open"]);
        assert_eq!(r.all_outputs().len(), 1);
    }

    #[test]
    fn default_keyvalues_for_new_entities() {
        let r = resolve_src(DOORS);
        let door = r.class("func_door").unwrap();
        assert_eq!(
            door.default_keyvalues(),
            [
                ("speed".to_owned(), "100".to_owned()),
                ("spawnflags".to_owned(), "1024".to_owned()),
                ("movedir".to_owned(), "0 0 0".to_owned()),
            ]
        );
    }

    #[test]
    fn display_helpers() {
        let src = r#"
            @BaseClass studio() sphere(fademindist) = Model []
            @PointClass base(Model) studioprop() sphere(fademaxdist) sphere() color(300 -5 12.6) size(32 32 16) = prop_x []
            @PointClass iconsprite("editor/light.vmt") light() lightcone() = light_x []
            @PointClass iconsprite() sprite() = sprite_x []
            @PointClass studio("models/editor/playerstart.mdl") = start_x []
            @PointClass lightprop("models/editor/spot.mdl") = spot_x []
            @PointClass size(bad) color(1 2) = broken_x []
        "#;
        let r = resolve_src(src);
        let prop = r.class("prop_x").unwrap();
        assert_eq!(prop.studio(), Some(StudioHelper { model: None, is_prop: true }));
        assert_eq!(prop.helpers_named("studio").count(), 0, "studioprop replaces inherited studio");
        assert_eq!(prop.sphere_props(), ["fademindist", "fademaxdist", "radius"]);
        assert_eq!(prop.color(), Some([255, 0, 13]));
        assert_eq!(prop.size(), Some(([-16.0, -16.0, -8.0], [16.0, 16.0, 8.0])));

        let light = r.class("light_x").unwrap();
        assert_eq!(light.iconsprite(), Some("editor/light.vmt"));
        assert!(light.has_helper("light") && light.has_helper("LIGHTCONE"));
        assert_eq!(light.studio(), None);
        assert_eq!(light.size(), None);
        assert_eq!(light.bounds_or_default(), DEFAULT_POINT_BOUNDS);

        let sprite = r.class("sprite_x").unwrap();
        assert_eq!(sprite.iconsprite(), None);
        assert!(sprite.has_helper("iconsprite"));
        assert_eq!(sprite.sprite(), None);

        let start = r.class("start_x").unwrap();
        let studio = start.studio().unwrap();
        assert_eq!(studio.model.as_deref(), Some("models/editor/playerstart.mdl"));
        assert!(!studio.is_prop);

        let spot = r.class("spot_x").unwrap();
        assert_eq!(spot.studio().unwrap().model.as_deref(), Some("models/editor/spot.mdl"));

        let broken = r.class("broken_x").unwrap();
        assert_eq!(broken.size(), None);
        assert_eq!(broken.color(), None);
    }

    #[test]
    fn base_and_point_class_with_same_name() {
        let src = r#"
            @BaseClass = Light [ _light(color255) : "Brightness" : "255 255 255 200" ]
            @PointClass base(Light) iconsprite("editor/light.vmt") = light [ _distance(integer) : "Distance" : 0 ]
        "#;
        let r = resolve_src(src);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let light = r.class("light").unwrap();
        assert!(light.is_point());
        assert_eq!(light.props.len(), 2);
        assert_eq!(r.point_classes(), ["light"]);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn unknown_base_and_cycles_warn() {
        let src = r#"
            @BaseClass base(B) = A [ a(string) ]
            @BaseClass base(A) = B [ b(string) ]
            @BaseClass base(Self) = Self [ s(string) ]
            @PointClass base(A, Missing) = p [ own(string) ]
        "#;
        let r = resolve_src(src);
        let p = r.class("p").unwrap();
        assert!(p.property("own").is_some());
        assert!(p.property("a").is_some());
        assert!(r.warnings.iter().any(|w| w.contains("unknown base class Missing")), "{:?}", r.warnings);
        assert!(r.warnings.iter().any(|w| w.contains("cycle")), "{:?}", r.warnings);
        assert!(r.warnings.iter().any(|w| w.contains("Self")), "{:?}", r.warnings);
        assert!(r.class("self").unwrap().property("s").is_some());
    }

    #[test]
    fn deep_inheritance_chain_does_not_overflow() {
        let mut src = String::from("@BaseClass = C0 [ p0(string) ]\n");
        for i in 1..1000 {
            src.push_str(&format!("@BaseClass base(C{}) = C{i} []\n", i - 1));
        }
        src.push_str("@PointClass base(C999) = leaf []\n");
        let r = resolve_src(&src);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings.first());
        let leaf = r.class("leaf").unwrap();
        assert_eq!(leaf.props.len(), 1);
        assert_eq!(leaf.bases.len(), 1000);
    }

    #[test]
    fn hostile_inheritance_is_bounded() {
        let mut src = String::from("@BaseClass = C0 [ p0(string) ]\n");
        for i in 1..6000 {
            src.push_str(&format!("@BaseClass base(C{}) = C{i} [ p{i}(string) q{i}(string) ]\n", i - 1));
        }
        let start = std::time::Instant::now();
        let r = resolve_src(&src);
        let total: usize = r.iter().map(item_count).sum();
        assert!(total <= MAX_INHERITED_TOTAL + 3 * 6000 * 3, "{total}");
        assert!(r.warnings.iter().any(|w| w.contains("inheritance too large")));
        assert_eq!(r.len(), 6000);
        assert!(r.class("C5999").unwrap().property("q5999").is_some());
        assert!(start.elapsed() < std::time::Duration::from_secs(20));
    }

    #[test]
    fn many_props_and_extensions_stay_fast() {
        let mut src = String::from("@PointClass = big [\n");
        for i in 0..10_000 {
            src.push_str(&format!("k{i}(integer) : \"K\" : {i}\n"));
        }
        src.push_str("]\n");
        for i in 0..4_000 {
            src.push_str(&format!("@ExtendClass = big [ e{i}(string) k{i}(string) : \"Ext\" ]\n"));
        }
        src.push_str("@PointClass base(big) = child [ k1(float) ]\n");
        let start = std::time::Instant::now();
        let r = resolve_src(&src);
        let big = r.class("big").unwrap();
        assert_eq!(big.props.len(), 14_000);
        assert_eq!(big.property("k10").unwrap().kind, PropertyType::String);
        assert_eq!(big.property("k10").unwrap().default.as_deref(), Some("10"));
        let child = r.class("child").unwrap();
        assert_eq!(child.props.len(), 14_000);
        assert_eq!(child.props[1].kind, PropertyType::Float);
        assert!(start.elapsed() < std::time::Duration::from_secs(20));
    }

    #[test]
    fn later_definition_replaces_earlier() {
        let src = r#"
            @PointClass = thing : "old" [ a(string) ]
            @PointClass = Thing : "new" [ b(string) ]
        "#;
        let fgd = Fgd::parse(src, None);
        assert_eq!(fgd.classes.len(), 1);
        assert_eq!(fgd.class("THING").unwrap().description, "new");
        let r = fgd.resolve();
        let thing = r.class("thing").unwrap();
        assert_eq!(thing.description, "new");
        assert!(thing.property("a").is_none());
    }

    #[test]
    fn extend_class_hammer_plus_plus_rules() {
        let src = r#"
            @BaseClass = Targetname [ targetname(target_source) : "Name" ]
            @PointClass base(Targetname) light() iconsprite("editor/light.vmt") sphere(_zero_percent_distance) = light : "A light."
            [
                effect_name(*particlesystem) : "Particle" : "fire" : "The effect."
                spawnflags(flags) = [ 1 : "Initially dark" : 0 ]
            ]
            @PointClass base(Targetname) lightprop("models/editor/spot.mdl") lightcone() = light_spot []

            @ExtendClass light(point) = light : ""
            [
                effect_name(string) : "Effect Name"
                spawnflags(flags) = [ 2 : "Extra" : 1 ]
                input Pulse(void) : "Pulse."
            ]
            @ExtendClass light(spot) lightprop("models/editor/spot_color.mdl") = light_spot : "" []
            @ExtendClass direction(movedir) = func_missing : "" []
        "#;
        let r = resolve_src(src);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let light = r.class("light").unwrap();
        assert_eq!(light.description, "A light.");
        let lights: Vec<_> = light.helpers_named("light").map(|h| h.args.clone()).collect();
        assert_eq!(lights, [vec!["point".to_owned()]]);
        assert_eq!(light.iconsprite(), Some("editor/light.vmt"));
        let effect = light.property("effect_name").unwrap();
        assert_eq!(effect.kind, PropertyType::String);
        assert_eq!(effect.display_name, "Effect Name");
        assert_eq!(effect.default.as_deref(), Some("fire"));
        assert_eq!(effect.description, "The effect.");
        assert!(effect.report);
        assert_eq!(light.default_spawnflags(), 2);
        assert_eq!(light.spawnflags().unwrap().flags.len(), 2);
        assert!(light.input("Pulse").is_some());

        let spot = r.class("light_spot").unwrap();
        assert_eq!(spot.studio().unwrap().model.as_deref(), Some("models/editor/spot_color.mdl"));
        assert_eq!(spot.helpers_named("lightprop").count(), 1);
        assert!(spot.has_helper("lightcone"));
        assert!(r.class("func_missing").is_none());
    }

    #[test]
    fn override_class_adds_base_and_warns_when_missing() {
        let src = r#"
            @BaseClass = Extra [ extra(integer) : "Extra" : 7 ]
            @PointClass = thing : "Thing" [ a(string) ]
            @OverrideClass base(Extra) = thing : "Better thing" [ b(string) ]
            @OverrideClass = nothing [ ]
        "#;
        let r = resolve_src(src);
        let thing = r.class("thing").unwrap();
        assert_eq!(thing.description, "Better thing");
        let names: Vec<_> = thing.props.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["extra", "a", "b"]);
        assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
    }

    #[test]
    fn extension_of_base_class_reaches_children() {
        let src = r#"
            @BaseClass = Targetname [ targetname(target_source) : "Name" ]
            @PointClass base(Targetname) = info_target []
            @ExtendClass = Targetname [ vscripts(scriptlist) : "Entity Scripts" ]
        "#;
        let r = resolve_src(src);
        assert!(r.class("info_target").unwrap().property("vscripts").is_some());
    }

    #[test]
    fn merge_prop_rules() {
        let mut base = Property::new("spawnflags", PropertyType::Flags);
        base.display_name = "Flags".into();
        base.flags = vec![FlagDef { value: 4, display: "Four".into(), default_on: true }];
        let mut child = Property::new("SpawnFlags", PropertyType::Flags);
        child.flags = vec![FlagDef { value: 1, display: "One".into(), default_on: false }];
        let mut list = NamedList::new(vec![base]);
        add_prop(&mut list, &child, false);
        let list = list.into_vec();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].display_name, "Flags");
        assert_eq!(list[0].default_flags(), 4);
        assert_eq!(list[0].flags.iter().map(|f| f.value).collect::<Vec<_>>(), [1, 4]);

        let mut choices = Property::new("mode", PropertyType::Choices);
        choices.choices = vec![crate::Choice { value: "0".into(), display: "A".into() }];
        choices.default = Some("0".into());
        let mut redefined = Property::new("mode", PropertyType::Choices);
        redefined.display_name = "Mode".into();
        let mut list = NamedList::new(vec![choices]);
        add_prop(&mut list, &redefined, false);
        let list = list.into_vec();
        assert_eq!(list[0].choices.len(), 1, "empty redefinition keeps the choices");
        assert_eq!(list[0].default, None, "inheritance takes the child's default");
    }

    #[test]
    fn types_are_send_and_sync() {
        fn check<T: Send + Sync>() {}
        check::<Fgd>();
        check::<ResolvedFgd>();
        check::<crate::FgdError>();
    }

    #[test]
    fn picker_order_is_case_insensitive() {
        let mut names = vec!["b", "A", "a", "C"];
        names.sort_by(|a, b| picker_order(a, b));
        assert_eq!(names, ["A", "a", "b", "C"]);
    }
}
