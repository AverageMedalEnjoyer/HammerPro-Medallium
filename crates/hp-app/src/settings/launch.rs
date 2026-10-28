use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchTiming {
    #[default]
    AtEnd,
    AtStart,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchSettings {
    pub timing: LaunchTiming,
    pub reuse_running: bool,
    pub skip_unused_content: bool,
    pub load_report: bool,
    pub spawn_at_camera: bool,
    pub args_version: u32,
}

impl Default for LaunchSettings {
    fn default() -> Self {
        Self {
            timing: LaunchTiming::AtEnd,
            reuse_running: true,
            skip_unused_content: false,
            load_report: false,
            spawn_at_camera: false,
            args_version: 0,
        }
    }
}
