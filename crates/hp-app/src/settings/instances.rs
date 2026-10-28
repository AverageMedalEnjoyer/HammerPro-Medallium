use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InstanceDisplay {
    Hidden,
    #[default]
    Tinted,
    Normal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InstancesSettings {
    pub display: InstanceDisplay,
    pub folder: String,
    pub collapse_skips_hidden: bool,
    pub outline_selected: bool,
    pub show_context: bool,
}

impl Default for InstancesSettings {
    fn default() -> Self {
        Self {
            display: InstanceDisplay::Tinted,
            folder: "instances".to_owned(),
            collapse_skips_hidden: true,
            outline_selected: true,
            show_context: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_load_with_defaults() {
        let s: InstancesSettings = toml::from_str("display = \"Normal\"").expect("parse");
        assert_eq!(s.display, InstanceDisplay::Normal);
        assert_eq!(s.folder, "instances");
        assert!(s.collapse_skips_hidden);
        let round: InstancesSettings = toml::from_str(&toml::to_string(&s).expect("write")).expect("read");
        assert_eq!(round, s);
    }
}
