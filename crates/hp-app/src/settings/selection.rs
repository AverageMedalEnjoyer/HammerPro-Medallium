use serde::{Deserialize, Serialize};

const HANDLE_SIZE_RANGE: std::ops::RangeInclusive<f32> = 4.0..=16.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectionSettings {
    pub clone_keeps_visgroups: bool,
    pub rotation_snap_deg: f64,
    pub free_rotation_with_alt: bool,
    pub handle_size: f32,
    pub shift_arrow_clones: bool,
    pub gizmo_local: bool,
    pub click_to_zoom: bool,
    pub stretch_handles: bool,
}

impl Default for SelectionSettings {
    fn default() -> Self {
        Self {
            clone_keeps_visgroups: true,
            rotation_snap_deg: 15.0,
            free_rotation_with_alt: false,
            handle_size: 7.0,
            shift_arrow_clones: true,
            gizmo_local: false,
            click_to_zoom: false,
            stretch_handles: true,
        }
    }
}

impl SelectionSettings {
    pub fn rotation_step(&self) -> f64 {
        if self.rotation_snap_deg.is_finite() { self.rotation_snap_deg.clamp(1.0, 180.0) } else { 15.0 }
    }

    pub fn handle_points(&self) -> f32 {
        if self.handle_size.is_finite() {
            self.handle_size.clamp(*HANDLE_SIZE_RANGE.start(), *HANDLE_SIZE_RANGE.end())
        } else {
            7.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_load_with_their_defaults() {
        let s: SelectionSettings = toml::from_str("rotation_snap_deg = 5.0").expect("parse");
        assert_eq!(s, SelectionSettings { rotation_snap_deg: 5.0, ..SelectionSettings::default() });
        assert!(s.clone_keeps_visgroups && s.shift_arrow_clones && s.stretch_handles);
        let back: SelectionSettings = toml::from_str(&toml::to_string(&s).expect("write")).expect("read");
        assert_eq!(back, s);
        let bad =
            SelectionSettings { rotation_snap_deg: f64::NAN, handle_size: 1000.0, ..SelectionSettings::default() };
        assert_eq!(bad.rotation_step(), 15.0);
        assert_eq!(bad.handle_points(), 16.0);
    }
}
