mod lexer;
mod model;
mod named;
mod parser;
mod resolve;
mod text;

use std::path::PathBuf;

pub use model::{
    AutoVisGroup, AutoVisSubGroup, Choice, ClassKind, EntityClass, Fgd, FlagDef, Helper, IoDef, Property, PropertyType,
};
pub use parser::MAX_FGD_FILE;
pub use resolve::{DEFAULT_POINT_BOUNDS, ResolvedClass, ResolvedFgd, StudioHelper};
pub use text::decode_text;

#[derive(Debug, thiserror::Error)]
pub enum FgdError {
    #[error("failed to read FGD file {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
