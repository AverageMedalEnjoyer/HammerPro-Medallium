use std::collections::HashMap;

use hp_vmf::{Entity, Map, Side};

use crate::{Document, Id};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RootKind {
    Material,
    Texture,
    Model,
    Sprite,
    Sound,
    Particle,
    Soundscape,
    Skybox,
    File,
    Instance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootSource {
    Faces { count: usize },
    Key { entity: Id, class: String, key: String },
    Output { entity: Id, class: String, output: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetRoot {
    pub kind: RootKind,
    pub name: String,
    pub source: RootSource,
    pub uses: usize,
}

pub type KeyClassifier<'a> = dyn Fn(&str, &str) -> Option<RootKind> + 'a;

const MATERIAL_KEYS: &[&str] = &[
    "material",
    "ropematerial",
    "smokematerial",
    "detailmaterial",
    "overlaymaterial",
    "spritename",
    "texture",
    "overlayname",
];

const SOUND_KEYS: &[&str] = &[
    "message",
    "startsound",
    "stopsound",
    "movesound",
    "noise1",
    "noise2",
    "locked_sound",
    "unlocked_sound",
    "closesound",
    "startclosesound",
    "soundname",
    "sound",
    "movepingsound",
    "pitchsound",
    "soundopenoverride",
    "soundcloseoverride",
    "soundmoveoverride",
    "soundlockedoverride",
    "soundunlockedoverride",
    "puntsound",
];

const BUTTON_CLASSES: &[&str] = &["func_button", "func_rot_button", "momentary_rot_button"];

const BUTTON_SOUND_KEYS: &[&str] = &["sounds", "locked_sound", "unlocked_sound"];

pub fn button_sound(class: &str, key: &str, value: &str) -> Option<String> {
    let numbered = BUTTON_CLASSES.iter().any(|c| c.eq_ignore_ascii_case(class))
        && BUTTON_SOUND_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key));
    let n: u32 = value.trim().parse().ok().filter(|n| *n > 0)?;
    numbered.then(|| format!("Buttons.snd{n}"))
}

const MODEL_KEYS: &[&str] = &["model", "gibmodel", "shootmodel", "breakmodel", "worldmodel"];

const SOUND_MESSAGE_CLASSES: &[&str] = &["ambient_generic", "ambient_music", "func_rotating"];

fn ext(value: &str) -> &str {
    let file = value.rsplit(['/', '\\']).next().unwrap_or(value);
    file.rfind('.').map(|i| &file[i + 1..]).unwrap_or("")
}

fn kind_by_extension(value: &str) -> Option<RootKind> {
    match ext(value).to_ascii_lowercase().as_str() {
        "mdl" => Some(RootKind::Model),
        "vmt" | "spr" => Some(RootKind::Sprite),
        "wav" | "mp3" | "ogg" => Some(RootKind::Sound),
        "vtf" => Some(RootKind::Texture),
        "raw" | "pcf" => Some(RootKind::File),
        _ => None,
    }
}

pub fn fallback_kind(class: &str, key: &str, value: &str) -> Option<RootKind> {
    let class = class.to_ascii_lowercase();
    let key = key.to_ascii_lowercase();
    let value = value.trim();
    if value.is_empty() || value.starts_with('*') {
        return None;
    }
    match (class.as_str(), key.as_str()) {
        ("worldspawn", "skyname") => return Some(RootKind::Skybox),
        ("worldspawn", "detailvbsp") => return None,
        ("info_particle_system", "effect_name") => return Some(RootKind::Particle),
        (c, "soundscape") if c.starts_with("env_soundscape") => return Some(RootKind::Soundscape),
        ("color_correction", "filename") => return Some(RootKind::File),
        ("func_instance", "file") => return Some(RootKind::Instance),
        ("env_projectedtexture", "texturename") => return Some(RootKind::Texture),
        (c, "message") if !SOUND_MESSAGE_CLASSES.contains(&c) => {
            return kind_by_extension(value).filter(|k| *k == RootKind::Sound);
        }
        _ => {}
    }
    if let Some(kind) = kind_by_extension(value) {
        return Some(kind);
    }
    if MODEL_KEYS.contains(&key.as_str()) {
        return value.to_ascii_lowercase().starts_with("sprites/").then_some(RootKind::Sprite);
    }
    let digits_trimmed = key.trim_end_matches(|c: char| c.is_ascii_digit());
    if MATERIAL_KEYS.contains(&digits_trimmed) && value.parse::<f64>().is_err() {
        return Some(RootKind::Material);
    }
    if SOUND_KEYS.contains(&key.as_str()) && value.parse::<f64>().is_err() {
        return Some(RootKind::Sound);
    }
    None
}

pub fn output_assets(param: &str) -> Vec<(RootKind, String)> {
    param
        .split(|c: char| c.is_whitespace() || c == ':' || c == ',')
        .filter(|t| !t.is_empty())
        .filter_map(|t| kind_by_extension(t).map(|k| (k, t.to_owned())))
        .collect()
}

#[derive(Default)]
struct Collector {
    index: HashMap<(RootKind, String), usize>,
    roots: Vec<AssetRoot>,
}

impl Collector {
    fn push(&mut self, kind: RootKind, name: &str, source: RootSource, uses: usize) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let key = (kind, name.replace('\\', "/").to_ascii_lowercase());
        match self.index.get(&key) {
            Some(&i) => self.roots[i].uses += uses,
            None => {
                self.index.insert(key, self.roots.len());
                self.roots.push(AssetRoot { kind, name: name.to_owned(), source, uses });
            }
        }
    }

    fn sides<'a>(&mut self, sides: impl Iterator<Item = &'a Side>) {
        let mut counts: HashMap<String, (String, usize)> = HashMap::new();
        for side in sides {
            let key = side.material.trim().replace('\\', "/").to_ascii_lowercase();
            counts.entry(key).or_insert_with(|| (side.material.trim().to_owned(), 0)).1 += 1;
        }
        let mut list: Vec<(String, (String, usize))> = counts.into_iter().collect();
        list.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        for (_, (name, count)) in list {
            self.push(RootKind::Material, &name, RootSource::Faces { count }, count);
        }
    }

    fn entity(&mut self, e: &Entity, classify: &KeyClassifier<'_>) {
        let class = e.classname.as_str();
        for (key, value) in &e.props {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            if let Some(script) = button_sound(class, key, value) {
                let source = RootSource::Key { entity: e.id, class: class.to_owned(), key: key.clone() };
                self.push(RootKind::Sound, &script, source, 1);
                continue;
            }
            let kind = match classify(class, key) {
                Some(k) => match kind_by_extension(value) {
                    Some(RootKind::Sprite) if k == RootKind::Model => Some(RootKind::Sprite),
                    _ => Some(k),
                },
                None => fallback_kind(class, key, value),
            };
            let Some(kind) = kind else { continue };
            let source = RootSource::Key { entity: e.id, class: class.to_owned(), key: key.clone() };
            self.push(kind, value, source, 1);
        }
        for c in &e.connections {
            for (kind, name) in output_assets(&c.param) {
                let source = RootSource::Output { entity: e.id, class: class.to_owned(), output: c.output.clone() };
                self.push(kind, &name, source, 1);
            }
        }
    }
}

fn finish(mut c: Collector) -> Vec<AssetRoot> {
    c.roots.sort_by_key(|r| (r.kind, r.name.to_ascii_lowercase()));
    c.roots
}

impl Document {
    pub fn asset_roots(&self, classify: &KeyClassifier<'_>) -> Vec<AssetRoot> {
        let scene = self.scene();
        let mut c = Collector::default();
        c.sides(scene.solids().values().flat_map(|n| n.solid.sides.iter()));
        c.entity(scene.world(), classify);
        for node in scene.entities().values() {
            c.entity(&node.entity, classify);
        }
        finish(c)
    }
}

pub fn map_asset_roots(map: &Map, classify: &KeyClassifier<'_>) -> Vec<AssetRoot> {
    let mut c = Collector::default();
    c.sides(
        map.world.solids.iter().chain(map.entities.iter().flat_map(|e| e.solids.iter())).flat_map(|s| s.sides.iter()),
    );
    c.entity(&map.world, classify);
    for e in &map.entities {
        c.entity(e, classify);
    }
    finish(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VMF: &str = r#"
world
{
	"id" "1"
	"classname" "worldspawn"
	"skyname" "sky_day01_01"
	"detailmaterial" "detail/detailsprites"
	"detailvbsp" "detail.vbsp"
	solid
	{
		"id" "2"
		side { "id" "1" "plane" "(0 0 0) (0 1 0) (1 1 0)" "material" "CUSTOM/Brick" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "2" "plane" "(0 0 8) (1 1 8) (0 1 8)" "material" "custom/brick" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
	}
}
entity
{
	"id" "10"
	"classname" "prop_static"
	"model" "models/props/crate.mdl"
	"origin" "0 0 0"
}
entity
{
	"id" "11"
	"classname" "ambient_generic"
	"message" "ambient/water/drip1.wav"
	"origin" "0 0 0"
}
entity
{
	"id" "12"
	"classname" "info_particle_system"
	"effect_name" "fire_large_01"
	"origin" "0 0 0"
}
entity
{
	"id" "13"
	"classname" "logic_relay"
	"targetname" "relay"
	connections
	{
		"OnTrigger" "door,SetModel,models/props/door2.mdl,0,-1"
		"OnTrigger" "!self,AddOutput,model models/props/crate.mdl,0,-1"
	}
}
entity
{
	"id" "14"
	"classname" "env_soundscape"
	"soundscape" "GenericIndoor"
	"message" "not a sound"
}
entity
{
	"id" "15"
	"classname" "func_door"
	"noise1" "Doors.Move1"
	"speed" "100"
}
entity
{
	"id" "16"
	"classname" "env_sprite"
	"model" "sprites/glow01.vmt"
}
"#;

    #[test]
    fn collects_every_kind_of_root() {
        let doc = Document::from_map(hp_vmf::parse(VMF).unwrap());
        let roots = doc.asset_roots(&|_, _| None);
        let names: Vec<(RootKind, &str, usize)> = roots.iter().map(|r| (r.kind, r.name.as_str(), r.uses)).collect();
        assert_eq!(
            names,
            vec![
                (RootKind::Material, "CUSTOM/Brick", 2),
                (RootKind::Material, "detail/detailsprites", 1),
                (RootKind::Model, "models/props/crate.mdl", 2),
                (RootKind::Model, "models/props/door2.mdl", 1),
                (RootKind::Sprite, "sprites/glow01.vmt", 1),
                (RootKind::Sound, "ambient/water/drip1.wav", 1),
                (RootKind::Sound, "Doors.Move1", 1),
                (RootKind::Particle, "fire_large_01", 1),
                (RootKind::Soundscape, "GenericIndoor", 1),
                (RootKind::Skybox, "sky_day01_01", 1),
            ]
        );
        assert_eq!(roots[0].source, RootSource::Faces { count: 2 });
        assert!(matches!(&roots[3].source, RootSource::Output { output, .. } if output == "OnTrigger"));
        assert_eq!(map_asset_roots(&hp_vmf::parse(VMF).unwrap(), &|_, _| None), roots);
    }

    #[test]
    fn classifier_wins_over_fallback() {
        let doc = Document::from_map(hp_vmf::parse(VMF).unwrap());
        let roots =
            doc.asset_roots(&|class, key| (class == "logic_relay" && key == "targetname").then_some(RootKind::Sound));
        assert!(roots.iter().any(|r| r.kind == RootKind::Sound && r.name == "relay"));
    }

    #[test]
    fn fallback_rules() {
        assert_eq!(fallback_kind("prop_dynamic", "model", "*3"), None);
        assert_eq!(fallback_kind("func_brush", "model", "*3"), None);
        assert_eq!(fallback_kind("game_text", "message", "Hello"), None);
        assert_eq!(fallback_kind("env_screenoverlay", "OverlayName3", "effects/tp_eyefx"), Some(RootKind::Material));
        assert_eq!(fallback_kind("color_correction", "filename", "materials/correction/a.raw"), Some(RootKind::File));
        assert_eq!(fallback_kind("func_door", "speed", "100"), None);
        assert_eq!(fallback_kind("x", "sound", "0.5"), None);
        assert_eq!(fallback_kind("func_rotating", "message", "ambient/fan.wav"), Some(RootKind::Sound));
        assert_eq!(fallback_kind("func_breakable", "material", "0"), None);
        assert_eq!(fallback_kind("func_physbox", "material", "2"), None);
        assert_eq!(fallback_kind("prop_door_rotating", "soundopenoverride", "Doors.Open"), Some(RootKind::Sound));
        assert_eq!(button_sound("func_button", "sounds", "3").as_deref(), Some("Buttons.snd3"));
        assert_eq!(button_sound("FUNC_BUTTON", "Locked_Sound", "10").as_deref(), Some("Buttons.snd10"));
        assert_eq!(button_sound("func_button", "sounds", "0"), None);
        assert_eq!(button_sound("func_door", "sounds", "3"), None);
        assert_eq!(button_sound("func_button", "speed", "3"), None);
        assert_eq!(output_assets("model models/a.mdl"), vec![(RootKind::Model, "models/a.mdl".to_owned())]);
        assert!(output_assets("Kill").is_empty());
    }
}
