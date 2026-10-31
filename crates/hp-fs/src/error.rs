use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FsError {
    #[error("cannot access {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("not a game installation with addons: {}", .0.display())]
    NotAGameDir(PathBuf),
    #[error("not a folder: {}", .0.display())]
    NotADirectory(PathBuf),
    #[error("no materials, models, sound or particles folder in {}", .0.display())]
    NoContentFolders(PathBuf),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unsafe or invalid path: {0}")]
    InvalidPath(String),
    #[error(transparent)]
    Vpk(#[from] hp_vpk::VpkError),
    #[error(transparent)]
    Gma(#[from] hp_gma::GmaError),
    #[error(transparent)]
    Vtf(#[from] hp_vtf::VtfError),
    #[error(transparent)]
    Vmt(#[from] hp_vmt::VmtError),
}

impl FsError {
    pub(crate) fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io { path: path.as_ref().to_path_buf(), source }
    }
}
