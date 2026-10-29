use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomDirSetting {
    pub path: PathBuf,
    pub prefix: String,
    pub enabled: bool,
}

impl Default for CustomDirSetting {
    fn default() -> Self {
        Self { path: PathBuf::new(), prefix: String::new(), enabled: true }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameSettings {
    pub game: String,
    #[serde(skip)]
    pub map_game: Option<String>,
    pub gmod_root: Option<PathBuf>,
    pub mount_addons: bool,
    pub mount_workshop: bool,
    pub mount_cfg: bool,
    #[serde(rename = "mount_server_content")]
    pub mount_downloads: bool,
    pub extra_game_dirs: Vec<PathBuf>,
    pub custom_texture_dirs: Vec<CustomDirSetting>,
    pub extra_fgds: Vec<PathBuf>,
    pub fgd_override: Option<PathBuf>,
    pub map_dir: Option<PathBuf>,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            game: String::new(),
            map_game: None,
            gmod_root: None,
            mount_addons: true,
            mount_workshop: false,
            mount_cfg: true,
            mount_downloads: true,
            extra_game_dirs: Vec::new(),
            custom_texture_dirs: Vec::new(),
            extra_fgds: Vec::new(),
            fgd_override: None,
            map_dir: None,
        }
    }
}

impl GameSettings {
    pub fn active_game(&self) -> &str {
        self.map_game.as_deref().unwrap_or(&self.game).trim()
    }

    pub fn is_gmod(&self) -> bool {
        self.active_profile().is_some_and(|p| p.is_gmod())
    }

    pub fn active_profile(&self) -> Option<hp_fs::GameProfile> {
        profile_of(self, self.active_game())
    }
}

pub fn profile_of(_g: &GameSettings, id: &str) -> Option<hp_fs::GameProfile> {
    hp_fs::resolve_game(id)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MapGame {
    pub id: String,
}

impl crate::sidecar::SidecarSection for MapGame {
    const NAME: &'static str = "game";
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StartupSettings {}
