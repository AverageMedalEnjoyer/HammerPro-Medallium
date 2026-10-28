use serde::{Deserialize, Serialize};

pub const DEFAULT_CORDON_MATERIAL: &str = "tools/toolsskybox";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VisibilitySettings {
    pub cordon_material: String,
    pub shade_outside_2d: bool,
    pub shade_opacity: f32,
    pub show_cordon_names: bool,
    pub move_remove_others: bool,
    pub move_hide: bool,
}

impl Default for VisibilitySettings {
    fn default() -> Self {
        Self {
            cordon_material: DEFAULT_CORDON_MATERIAL.to_owned(),
            shade_outside_2d: true,
            shade_opacity: 0.45,
            show_cordon_names: true,
            move_remove_others: true,
            move_hide: false,
        }
    }
}

impl VisibilitySettings {
    pub fn cordon_material(&self) -> &str {
        let m = self.cordon_material.trim();
        if m.is_empty() { DEFAULT_CORDON_MATERIAL } else { m }
    }

    pub fn shade_alpha(&self) -> f32 {
        if self.shade_opacity.is_finite() { self.shade_opacity.clamp(0.0, 1.0) } else { 0.45 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_old_files() {
        let s: VisibilitySettings = toml::from_str("").expect("empty table");
        assert_eq!(s, VisibilitySettings::default());
        assert_eq!(s.cordon_material(), "tools/toolsskybox");
        let s: VisibilitySettings =
            toml::from_str("cordon_material = \"  \"\nshade_opacity = 7.0\nunknown = 1").expect("parse");
        assert_eq!(s.cordon_material(), DEFAULT_CORDON_MATERIAL);
        assert_eq!(s.shade_alpha(), 1.0);
    }
}
