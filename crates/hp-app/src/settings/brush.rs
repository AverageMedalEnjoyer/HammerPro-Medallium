use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrushSettings {
    pub reorient_primitives: bool,
    pub clip_measurements: bool,
    pub clip_current_material: bool,
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self { reorient_primitives: true, clip_measurements: true, clip_current_material: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_load_with_their_defaults() {
        let s: BrushSettings = toml::from_str("clip_measurements = false").expect("parse");
        assert_eq!(s, BrushSettings { clip_measurements: false, ..BrushSettings::default() });
        assert!(s.reorient_primitives);
        let back: BrushSettings = toml::from_str(&toml::to_string(&s).expect("write")).expect("read");
        assert_eq!(back, s);
    }
}
