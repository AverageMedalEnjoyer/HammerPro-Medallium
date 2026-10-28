use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::FgdError;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fgd {
    pub classes: Vec<EntityClass>,
    pub map_size: Option<(i32, i32)>,
    pub material_exclusions: Vec<String>,
    pub auto_visgroups: Vec<AutoVisGroup>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ClassKind {
    Base,
    Point,
    Solid,
    Npc,
    KeyFrame,
    Move,
    Filter,
    Override,
    Extend,
    Other(String),
}

impl ClassKind {
    pub fn from_directive(name: &str) -> Option<Self> {
        const KINDS: [(&str, ClassKind); 9] = [
            ("baseclass", ClassKind::Base),
            ("pointclass", ClassKind::Point),
            ("solidclass", ClassKind::Solid),
            ("npcclass", ClassKind::Npc),
            ("keyframeclass", ClassKind::KeyFrame),
            ("moveclass", ClassKind::Move),
            ("filterclass", ClassKind::Filter),
            ("overrideclass", ClassKind::Override),
            ("extendclass", ClassKind::Extend),
        ];
        if let Some((_, kind)) = KINDS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            return Some(kind.clone());
        }
        let is_class =
            name.len() > 5 && name.get(name.len() - 5..).is_some_and(|tail| tail.eq_ignore_ascii_case("class"));
        is_class.then(|| ClassKind::Other(name.to_owned()))
    }

    pub fn directive(&self) -> &str {
        match self {
            ClassKind::Base => "BaseClass",
            ClassKind::Point => "PointClass",
            ClassKind::Solid => "SolidClass",
            ClassKind::Npc => "NPCClass",
            ClassKind::KeyFrame => "KeyFrameClass",
            ClassKind::Move => "MoveClass",
            ClassKind::Filter => "FilterClass",
            ClassKind::Override => "OverrideClass",
            ClassKind::Extend => "ExtendClass",
            ClassKind::Other(name) => name,
        }
    }

    pub fn is_base(&self) -> bool {
        matches!(self, ClassKind::Base)
    }

    pub fn is_point(&self) -> bool {
        matches!(
            self,
            ClassKind::Point
                | ClassKind::Npc
                | ClassKind::KeyFrame
                | ClassKind::Move
                | ClassKind::Filter
                | ClassKind::Other(_)
        )
    }

    pub fn is_solid(&self) -> bool {
        matches!(self, ClassKind::Solid)
    }

    pub fn is_extension(&self) -> bool {
        matches!(self, ClassKind::Override | ClassKind::Extend)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityClass {
    pub kind: ClassKind,
    pub name: String,
    pub description: String,
    pub helpers: Vec<Helper>,
    pub props: Vec<Property>,
    pub inputs: Vec<IoDef>,
    pub outputs: Vec<IoDef>,
}

impl EntityClass {
    pub fn new(kind: ClassKind, name: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            description: String::new(),
            helpers: Vec::new(),
            props: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
        }
    }

    pub fn bases(&self) -> impl Iterator<Item = &str> + '_ {
        self.helpers
            .iter()
            .filter(|h| h.is("base"))
            .flat_map(|h| h.args.iter())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
    }

    pub fn property(&self, name: &str) -> Option<&Property> {
        self.props.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Helper {
    pub name: String,
    pub args: Vec<String>,
}

impl Helper {
    pub fn new(name: impl Into<String>, args: Vec<String>) -> Self {
        Self { name: name.into(), args }
    }

    pub fn is(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }

    pub fn arg(&self, index: usize) -> Option<&str> {
        self.args.get(index).map(|s| s.trim())
    }

    pub fn first_arg(&self) -> Option<&str> {
        self.arg(0).filter(|s| !s.is_empty())
    }

    pub fn numbers(&self) -> Option<Vec<f64>> {
        self.args.iter().flat_map(|a| a.split_whitespace()).map(|t| t.parse::<f64>().ok()).collect()
    }

    pub fn same_as(&self, other: &Helper) -> bool {
        self.is(&other.name) && self.args == other.args
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub name: String,
    pub kind: PropertyType,
    pub display_name: String,
    pub default: Option<String>,
    pub description: String,
    pub readonly: bool,
    pub report: bool,
    pub choices: Vec<Choice>,
    pub flags: Vec<FlagDef>,
}

impl Property {
    pub fn new(name: impl Into<String>, kind: PropertyType) -> Self {
        Self {
            name: name.into(),
            kind,
            display_name: String::new(),
            default: None,
            description: String::new(),
            readonly: false,
            report: false,
            choices: Vec::new(),
            flags: Vec::new(),
        }
    }

    pub fn label(&self) -> &str {
        if self.display_name.is_empty() { &self.name } else { &self.display_name }
    }

    pub fn default_flags(&self) -> u32 {
        self.flags.iter().filter(|f| f.default_on).fold(0, |acc, f| acc | f.value)
    }

    pub fn choice_display(&self, value: &str) -> Option<&str> {
        let value = value.trim();
        let number = value.parse::<f64>().ok();
        self.choices
            .iter()
            .find(|c| {
                let cv = c.value.trim();
                match (number, cv.parse::<f64>().ok()) {
                    (Some(a), Some(b)) => a == b,
                    _ => cv.eq_ignore_ascii_case(value),
                }
            })
            .map(|c| c.display.as_str())
    }

    pub fn default_value(&self) -> Option<String> {
        if self.kind == PropertyType::Flags {
            return Some(self.default_flags().to_string());
        }
        self.default.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub value: String,
    pub display: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlagDef {
    pub value: u32,
    pub display: String,
    pub default_on: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IoDef {
    pub name: String,
    pub arg_type: PropertyType,
    pub description: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutoVisGroup {
    pub name: String,
    pub groups: Vec<AutoVisSubGroup>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutoVisSubGroup {
    pub name: String,
    pub classes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PropertyType {
    String,
    Integer,
    Float,
    Boolean,
    Choices,
    Flags,
    Color255,
    Color1,
    Studio,
    Sprite,
    Sound,
    Decal,
    Material,
    Scene,
    TargetSource,
    TargetDestination,
    TargetNameOrClass,
    Origin,
    Angle,
    AngleNegativePitch,
    VecLine,
    Vector,
    Axis,
    SideList,
    NpcClass,
    FilterClass,
    NodeDest,
    NodeId,
    PointEntityClass,
    InstanceFile,
    InstanceVariable,
    InstanceParm,
    ParticleSystem,
    Script,
    ScriptList,
    Void,
    Other(String),
}

const TYPE_NAMES: [(&str, PropertyType); 37] = [
    ("string", PropertyType::String),
    ("integer", PropertyType::Integer),
    ("float", PropertyType::Float),
    ("boolean", PropertyType::Boolean),
    ("choices", PropertyType::Choices),
    ("flags", PropertyType::Flags),
    ("color255", PropertyType::Color255),
    ("color1", PropertyType::Color1),
    ("studio", PropertyType::Studio),
    ("sprite", PropertyType::Sprite),
    ("sound", PropertyType::Sound),
    ("decal", PropertyType::Decal),
    ("material", PropertyType::Material),
    ("scene", PropertyType::Scene),
    ("target_source", PropertyType::TargetSource),
    ("target_destination", PropertyType::TargetDestination),
    ("target_name_or_class", PropertyType::TargetNameOrClass),
    ("origin", PropertyType::Origin),
    ("angle", PropertyType::Angle),
    ("angle_negative_pitch", PropertyType::AngleNegativePitch),
    ("vecline", PropertyType::VecLine),
    ("vector", PropertyType::Vector),
    ("axis", PropertyType::Axis),
    ("sidelist", PropertyType::SideList),
    ("npcclass", PropertyType::NpcClass),
    ("filterclass", PropertyType::FilterClass),
    ("node_dest", PropertyType::NodeDest),
    ("node_id", PropertyType::NodeId),
    ("pointentityclass", PropertyType::PointEntityClass),
    ("instance_file", PropertyType::InstanceFile),
    ("instance_variable", PropertyType::InstanceVariable),
    ("instance_parm", PropertyType::InstanceParm),
    ("particlesystem", PropertyType::ParticleSystem),
    ("script", PropertyType::Script),
    ("scriptlist", PropertyType::ScriptList),
    ("void", PropertyType::Void),
    ("bool", PropertyType::Boolean),
];

impl PropertyType {
    pub fn from_name(name: &str) -> Self {
        TYPE_NAMES
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, t)| t.clone())
            .unwrap_or_else(|| PropertyType::Other(name.to_owned()))
    }

    pub fn name(&self) -> &str {
        if let PropertyType::Other(name) = self {
            return name;
        }
        TYPE_NAMES.iter().find(|(_, t)| t == self).map(|(n, _)| *n).unwrap_or("")
    }

    pub fn is_entity_reference(&self) -> bool {
        matches!(
            self,
            PropertyType::TargetDestination
                | PropertyType::TargetNameOrClass
                | PropertyType::FilterClass
                | PropertyType::NodeDest
        )
    }

    pub fn is_asset(&self) -> bool {
        matches!(
            self,
            PropertyType::Studio
                | PropertyType::Sprite
                | PropertyType::Sound
                | PropertyType::Decal
                | PropertyType::Material
                | PropertyType::Scene
                | PropertyType::InstanceFile
                | PropertyType::ParticleSystem
                | PropertyType::Script
                | PropertyType::ScriptList
        )
    }
}

impl std::fmt::Display for PropertyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl Fgd {
    pub fn parse(src: &str, base_dir: Option<&Path>) -> Fgd {
        let mut loader = crate::parser::Loader::new(Vec::new());
        loader.parse_source(src, "<input>", base_dir, 0);
        loader.finish()
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Fgd, FgdError> {
        Self::load_many([path])
    }

    pub fn load_many<I, P>(paths: I) -> Result<Fgd, FgdError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        Self::load_with_include_dirs(paths, std::iter::empty::<&Path>())
    }

    pub fn load_with_include_dirs<I, P, D, Q>(paths: I, include_dirs: D) -> Result<Fgd, FgdError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
        D: IntoIterator<Item = Q>,
        Q: AsRef<Path>,
    {
        let dirs = include_dirs.into_iter().map(|d| d.as_ref().to_path_buf()).collect();
        let mut loader = crate::parser::Loader::new(dirs);
        for path in paths {
            loader.load_top_level(path.as_ref())?;
        }
        Ok(loader.finish())
    }

    pub fn merge(&mut self, other: Fgd) {
        self.classes.extend(other.classes);
        dedupe_classes(&mut self.classes);
        if other.map_size.is_some() {
            self.map_size = other.map_size;
        }
        let mut meta = MetaIndex::of(self);
        for dir in other.material_exclusions {
            meta.add_exclusion(&mut self.material_exclusions, dir);
        }
        for group in other.auto_visgroups {
            meta.add_auto_visgroup(&mut self.auto_visgroups, group);
        }
        self.warnings.extend(other.warnings);
    }

    pub fn class(&self, name: &str) -> Option<&EntityClass> {
        let mut found: Option<&EntityClass> = None;
        for class in self.classes.iter().filter(|c| !c.kind.is_extension()) {
            if class.name.eq_ignore_ascii_case(name) && (found.is_none() || found.is_some_and(|f| f.kind.is_base())) {
                found = Some(class);
            }
        }
        found
    }
}

pub(crate) fn dedupe_classes(classes: &mut Vec<EntityClass>) {
    let mut seen: HashMap<(String, bool), usize> = HashMap::with_capacity(classes.len());
    let mut out: Vec<EntityClass> = Vec::with_capacity(classes.len());
    for class in classes.drain(..) {
        if class.kind.is_extension() {
            out.push(class);
            continue;
        }
        let key = (class.name.trim().to_ascii_lowercase(), class.kind.is_base());
        match seen.get(&key).and_then(|&i| out.get_mut(i)) {
            Some(slot) => *slot = class,
            None => {
                seen.insert(key, out.len());
                out.push(class);
            }
        }
    }
    *classes = out;
}

pub(crate) const MAX_HELPERS_PER_CLASS: usize = 256;

#[derive(Default)]
pub(crate) struct MetaIndex {
    exclusions: HashSet<String>,
    groups: HashMap<String, usize>,
    subgroups: HashMap<(usize, String), usize>,
    members: HashSet<(usize, usize, String)>,
}

impl MetaIndex {
    pub(crate) fn of(fgd: &Fgd) -> Self {
        let mut index = Self::default();
        for dir in &fgd.material_exclusions {
            index.exclusions.insert(dir.to_ascii_lowercase());
        }
        for (gi, group) in fgd.auto_visgroups.iter().enumerate() {
            index.groups.entry(group.name.to_ascii_lowercase()).or_insert(gi);
            for (si, sub) in group.groups.iter().enumerate() {
                index.subgroups.entry((gi, sub.name.to_ascii_lowercase())).or_insert(si);
                for class in &sub.classes {
                    index.members.insert((gi, si, class.to_ascii_lowercase()));
                }
            }
        }
        index
    }

    pub(crate) fn add_exclusion(&mut self, list: &mut Vec<String>, dir: String) {
        if self.exclusions.insert(dir.to_ascii_lowercase()) {
            list.push(dir);
        }
    }

    pub(crate) fn add_auto_visgroup(&mut self, list: &mut Vec<AutoVisGroup>, group: AutoVisGroup) {
        let AutoVisGroup { name, groups } = group;
        let gi = *self.groups.entry(name.to_ascii_lowercase()).or_insert_with(|| {
            list.push(AutoVisGroup { name, groups: Vec::new() });
            list.len() - 1
        });
        let Some(target) = list.get_mut(gi) else { return };
        for sub in groups {
            let AutoVisSubGroup { name, classes } = sub;
            let si = *self.subgroups.entry((gi, name.to_ascii_lowercase())).or_insert_with(|| {
                target.groups.push(AutoVisSubGroup { name, classes: Vec::new() });
                target.groups.len() - 1
            });
            let Some(dst) = target.groups.get_mut(si) else { continue };
            for class in classes {
                if self.members.insert((gi, si, class.to_ascii_lowercase())) {
                    dst.classes.push(class);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_kind_from_directive() {
        assert_eq!(ClassKind::from_directive("PointClass"), Some(ClassKind::Point));
        assert_eq!(ClassKind::from_directive("npcclass"), Some(ClassKind::Npc));
        assert_eq!(ClassKind::from_directive("EXTENDCLASS"), Some(ClassKind::Extend));
        assert_eq!(ClassKind::from_directive("PathClass"), Some(ClassKind::Other("PathClass".into())));
        assert_eq!(ClassKind::from_directive("class"), None);
        assert_eq!(ClassKind::from_directive("include"), None);
        assert_eq!(ClassKind::from_directive("\u{e9}class"), Some(ClassKind::Other("\u{e9}class".into())));
        assert!(ClassKind::Npc.is_point());
        assert!(!ClassKind::Base.is_point());
        assert!(ClassKind::Solid.is_solid());
    }

    #[test]
    fn property_type_names_round_trip() {
        for (name, ty) in TYPE_NAMES.iter().take(36) {
            assert_eq!(PropertyType::from_name(name), *ty);
            assert_eq!(ty.name(), *name);
        }
        assert_eq!(PropertyType::from_name("Bool"), PropertyType::Boolean);
        assert_eq!(PropertyType::from_name("FLAGS"), PropertyType::Flags);
        assert_eq!(PropertyType::from_name("ehandle"), PropertyType::Other("ehandle".into()));
        assert_eq!(PropertyType::Other("ehandle".into()).to_string(), "ehandle");
        assert!(PropertyType::TargetDestination.is_entity_reference());
        assert!(PropertyType::Studio.is_asset());
    }

    #[test]
    fn helper_numbers() {
        let h = Helper::new("size", vec!["-16 -16 0".into(), "16 16 72".into()]);
        assert_eq!(h.numbers(), Some(vec![-16.0, -16.0, 0.0, 16.0, 16.0, 72.0]));
        let bad = Helper::new("size", vec!["a".into()]);
        assert_eq!(bad.numbers(), None);
    }

    #[test]
    fn choice_display_matches_numbers_and_text() {
        let mut p = Property::new("mode", PropertyType::Choices);
        p.choices = vec![
            Choice { value: "0".into(), display: "Off".into() },
            Choice { value: "Nothing".into(), display: "None".into() },
        ];
        assert_eq!(p.choice_display("0.0"), Some("Off"));
        assert_eq!(p.choice_display("nothing"), Some("None"));
        assert_eq!(p.choice_display("2"), None);
    }

    #[test]
    fn dedupe_keeps_position_and_namespaces() {
        let mut classes = vec![
            EntityClass::new(ClassKind::Base, "Light"),
            EntityClass::new(ClassKind::Point, "a"),
            EntityClass::new(ClassKind::Point, "light"),
            EntityClass::new(ClassKind::Extend, "a"),
            EntityClass::new(ClassKind::Solid, "A"),
        ];
        dedupe_classes(&mut classes);
        let names: Vec<_> = classes.iter().map(|c| (c.kind.clone(), c.name.as_str())).collect();
        assert_eq!(
            names,
            vec![
                (ClassKind::Base, "Light"),
                (ClassKind::Solid, "A"),
                (ClassKind::Point, "light"),
                (ClassKind::Extend, "a"),
            ]
        );
    }
}
