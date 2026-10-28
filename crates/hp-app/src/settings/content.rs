use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReleaseMode {
    #[default]
    WorkshopAddon,
    SelfContained,
    ServerKit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContentSettings {
    pub mode: ReleaseMode,
    pub compile_first: bool,
    pub compress_pakfile: bool,
    pub lzma_level: u32,
    pub pack_workshop: bool,
    pub pack_mounted: bool,
    pub keep_cubemaps: bool,
    pub game_session: bool,
    pub write_gma: bool,
    pub open_folder: bool,
    pub protect: bool,
    pub protect_watermark: String,
    pub protect_strip_original_faces: bool,
    pub protect_lock: bool,
    pub protect_lock_key: String,
    pub protect_lock_block: bool,
    pub protect_randomize: bool,
    pub show_map_assets: bool,
    pub collect_folder: String,
}

impl Default for ContentSettings {
    fn default() -> Self {
        Self {
            mode: ReleaseMode::WorkshopAddon,
            compile_first: true,
            compress_pakfile: false,
            lzma_level: 6,
            pack_workshop: true,
            pack_mounted: false,
            keep_cubemaps: true,
            game_session: false,
            write_gma: true,
            open_folder: false,
            protect: false,
            protect_watermark: String::new(),
            protect_strip_original_faces: true,
            protect_lock: false,
            protect_lock_key: String::new(),
            protect_lock_block: false,
            protect_randomize: true,
            show_map_assets: false,
            collect_folder: String::new(),
        }
    }
}
