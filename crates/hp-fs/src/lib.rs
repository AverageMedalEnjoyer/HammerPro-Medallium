mod assets;
pub mod collect;
mod custom;
pub mod deps;
mod detect;
mod dirindex;
mod error;
mod export;
mod fs;
pub mod games;
mod keywords;
pub mod lint;
mod material;
mod mount;
mod path;
mod progress;
mod source;
mod texture;

#[cfg(test)]
mod test_util;

pub use assets::AssetEntry;
pub use custom::{
    CustomDir, CustomLayout, CustomMode, IMAGE_EXTENSIONS, custom_layout, game_texture_size, is_auto_prefix,
    is_safe_material_name, safe_material_name, suggest_prefix,
};
pub use detect::{GMOD_APP_ID, detect_gmod_root, detect_steam_libraries, detect_workshop_dir};
pub use error::FsError;
pub use export::{ExportFailure, ExportReport, ResizedImage};
pub use fs::{CustomReport, GameFs};
pub use games::{
    GMOD_ID, GameProfile, KNOWN_GAMES, KnownGame, LightingPasses, detect_games, game_catalogue, known_game,
    refresh_games, resolve_game,
};
pub use keywords::MaterialKeywords;
pub use mount::{DeferredMount, MountOptions};
pub use path::normalize_path;
pub use progress::{MountProgress, MountSnapshot, MountStage};
pub use source::{SourceInfo, SourceKind, SourceOrigin};
pub use texture::{CustomImageInfo, LoadedTexture};
