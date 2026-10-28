use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReleaseSidecar {
    pub title: String,
    pub tags: Vec<String>,
    pub ignore: Vec<String>,
    pub description: String,
    pub output_dir: Option<PathBuf>,
    pub workshop_id: Option<u64>,
    pub change_note: String,
    pub nav_fingerprint: Option<u64>,
    pub cubemaps_fingerprint: Option<u64>,
    pub last: Option<LastRelease>,
    pub protect_salt: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LastRelease {
    pub time: u64,
    pub files: usize,
    pub bytes: u64,
    pub output: PathBuf,
}

impl hp_app::sidecar::SidecarSection for ReleaseSidecar {
    const NAME: &'static str = "content";
}
