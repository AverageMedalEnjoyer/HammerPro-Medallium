use serde_json::{Map, Value};

use crate::whitelist::wildcard_match;

pub const ADDON_TYPES: &[&str] =
    &["gamemode", "map", "weapon", "vehicle", "npc", "entity", "tool", "effects", "model", "servercontent"];

pub const ADDON_TAGS: &[&str] =
    &["fun", "roleplay", "scenic", "movie", "realism", "cartoon", "water", "comic", "build"];

pub const MAX_TAGS: usize = 2;

pub const MAX_TITLE: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddonProblem {
    EmptyTitle,
    TitleNotAscii,
    TitleTooLong,
    BadType(String),
    TooManyTags,
    BadTag(String),
    DuplicateTag(String),
}

impl std::fmt::Display for AddonProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTitle => f.write_str("the title is empty"),
            Self::TitleNotAscii => f.write_str("the title must be plain ASCII"),
            Self::TitleTooLong => write!(f, "the title is longer than {MAX_TITLE} characters"),
            Self::BadType(t) => write!(f, "unknown type {t:?}"),
            Self::TooManyTags => write!(f, "at most {MAX_TAGS} tags"),
            Self::BadTag(t) => write!(f, "unknown tag {t:?}"),
            Self::DuplicateTag(t) => write!(f, "tag {t:?} is listed twice"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddonJson {
    pub title: String,
    pub kind: String,
    pub tags: Vec<String>,
    pub ignore: Vec<String>,
    pub description: String,
}

impl Default for AddonJson {
    fn default() -> Self {
        AddonJson {
            title: String::new(),
            kind: "map".to_owned(),
            tags: Vec::new(),
            ignore: Vec::new(),
            description: String::new(),
        }
    }
}

impl AddonJson {
    pub fn problems(&self) -> Vec<AddonProblem> {
        let mut out = Vec::new();
        let title = self.title.trim();
        if title.is_empty() {
            out.push(AddonProblem::EmptyTitle);
        } else if !title.chars().all(|c| c.is_ascii() && !c.is_ascii_control()) {
            out.push(AddonProblem::TitleNotAscii);
        }
        if title.len() > MAX_TITLE {
            out.push(AddonProblem::TitleTooLong);
        }
        if !ADDON_TYPES.contains(&self.kind.as_str()) {
            out.push(AddonProblem::BadType(self.kind.clone()));
        }
        if self.tags.len() > MAX_TAGS {
            out.push(AddonProblem::TooManyTags);
        }
        for (i, tag) in self.tags.iter().enumerate() {
            if !ADDON_TAGS.contains(&tag.as_str()) {
                out.push(AddonProblem::BadTag(tag.clone()));
            } else if self.tags[..i].contains(tag) {
                out.push(AddonProblem::DuplicateTag(tag.clone()));
            }
        }
        out
    }

    pub fn is_valid(&self) -> bool {
        self.problems().is_empty()
    }

    pub fn is_ignored(&self, path: &str) -> bool {
        let lower = path.replace('\\', "/").to_ascii_lowercase();
        self.ignore.iter().any(|w| wildcard_match(&w.replace('\\', "/").to_ascii_lowercase(), &lower))
    }

    pub fn parse(text: &str) -> Result<AddonJson, String> {
        let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
        let Value::Object(map) = value else {
            return Err("addon.json must hold an object".to_owned());
        };
        let text_of = |k: &str| map.get(k).and_then(Value::as_str).unwrap_or_default().to_owned();
        let list = |k: &str| -> Vec<String> {
            map.get(k)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).take(256).map(str::to_owned).collect())
                .unwrap_or_default()
        };
        Ok(AddonJson {
            title: text_of("title"),
            kind: map.get("type").and_then(Value::as_str).unwrap_or("map").to_ascii_lowercase(),
            tags: list("tags").into_iter().map(|t| t.to_ascii_lowercase()).collect(),
            ignore: list("ignore"),
            description: text_of("description"),
        })
    }

    pub fn to_json(&self) -> String {
        let mut map = Map::new();
        map.insert("title".into(), Value::String(self.title.trim().to_owned()));
        map.insert("type".into(), Value::String(self.kind.clone()));
        map.insert("tags".into(), Value::Array(self.tags.iter().cloned().map(Value::String).collect()));
        map.insert("ignore".into(), Value::Array(self.ignore.iter().cloned().map(Value::String).collect()));
        if !self.description.is_empty() {
            map.insert("description".into(), Value::String(self.description.clone()));
        }
        let mut text = serde_json::to_string_pretty(&Value::Object(map)).unwrap_or_default();
        text.push('\n');
        text
    }

    pub fn gma_description(&self) -> String {
        let mut map = Map::new();
        map.insert("description".into(), Value::String(self.description.clone()));
        map.insert("type".into(), Value::String(self.kind.clone()));
        map.insert("tags".into(), Value::Array(self.tags.iter().cloned().map(Value::String).collect()));
        serde_json::to_string_pretty(&Value::Object(map)).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_like_gmad() {
        let ok =
            AddonJson { title: "My map".into(), tags: vec!["build".into(), "scenic".into()], ..AddonJson::default() };
        assert!(ok.is_valid());
        let bad = AddonJson {
            title: "Ma carte é".into(),
            kind: "maps".into(),
            tags: vec!["build".into(), "build".into(), "pvp".into()],
            ..AddonJson::default()
        };
        assert_eq!(
            bad.problems(),
            vec![
                AddonProblem::TitleNotAscii,
                AddonProblem::BadType("maps".into()),
                AddonProblem::TooManyTags,
                AddonProblem::DuplicateTag("build".into()),
                AddonProblem::BadTag("pvp".into()),
            ]
        );
        assert_eq!(AddonJson::default().problems(), vec![AddonProblem::EmptyTitle]);
    }

    #[test]
    fn json_round_trip_and_ignore() {
        let a = AddonJson {
            title: "Test".into(),
            tags: vec!["fun".into()],
            ignore: vec!["*.psd".into(), "maps/*.vmf".into()],
            description: "hi".into(),
            ..AddonJson::default()
        };
        let back = AddonJson::parse(&a.to_json()).unwrap();
        assert_eq!(back, a);
        assert!(a.is_ignored("Materials/X.PSD"));
        assert!(a.is_ignored("maps\\gm_x.vmf"));
        assert!(!a.is_ignored("maps/gm_x.bsp"));
        assert!(AddonJson::parse("[1]").is_err());
        assert!(AddonJson::parse("{").is_err());
        let d: Value = serde_json::from_str(&a.gma_description()).unwrap();
        assert_eq!(d["type"], "map");
    }
}
