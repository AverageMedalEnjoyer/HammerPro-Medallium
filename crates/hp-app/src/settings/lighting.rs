use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightingQuality {
    #[default]
    Draft,
    Normal,
    Final,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LightingSettings {
    pub auto_bake: bool,
    pub threads: u32,
    pub max_luxels_millions: u32,
    pub tiny_scale: u32,
    pub budget_styles: u32,
    pub show_budget: bool,
    pub quality: LightingQuality,
}

impl Default for LightingSettings {
    fn default() -> Self {
        Self {
            auto_bake: true,
            threads: 0,
            max_luxels_millions: 16,
            tiny_scale: 4,
            budget_styles: 1,
            show_budget: true,
            quality: LightingQuality::Draft,
        }
    }
}

impl LightingSettings {
    pub const MAX_THREADS: u32 = 256;
    pub const LUXELS_RANGE: std::ops::RangeInclusive<u32> = 1..=64;

    pub fn sanitized(&self) -> Self {
        Self {
            auto_bake: self.auto_bake,
            threads: self.threads.min(Self::MAX_THREADS),
            max_luxels_millions: self.max_luxels_millions.clamp(*Self::LUXELS_RANGE.start(), *Self::LUXELS_RANGE.end()),
            tiny_scale: self.tiny_scale.clamp(1, 64),
            budget_styles: self.budget_styles.clamp(1, 4),
            show_budget: self.show_budget,
            quality: self.quality,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_ranges() {
        let d = LightingSettings::default();
        assert_eq!(d.sanitized(), d);
        let bad = LightingSettings { threads: 9999, max_luxels_millions: 0, tiny_scale: 0, budget_styles: 9, ..d };
        let s = bad.sanitized();
        assert_eq!((s.threads, s.max_luxels_millions, s.tiny_scale, s.budget_styles), (256, 1, 1, 4));
        let parsed: LightingSettings = toml::from_str("").unwrap();
        assert_eq!(parsed, d);
    }
}
