use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BspError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("not a Source BSP file")]
    NotBsp,
    #[error("unsupported BSP version {0} (supported: 19 to 21)")]
    UnsupportedVersion(i32),
    #[error("corrupt BSP: {0}")]
    Corrupt(String),
    #[error("lump index {0} is out of range")]
    BadLump(usize),
    #[error("cannot decompress: {0}")]
    Decompress(String),
    #[error("cannot compress: {0}")]
    Compress(String),
    #[error("invalid pakfile: {0}")]
    Zip(String),
    #[error("{name}: unsupported compression method {method}")]
    UnsupportedMethod { name: String, method: u16 },
    #[error("{name}: CRC mismatch")]
    CrcMismatch { name: String },
    #[error("invalid path {0:?}")]
    InvalidPath(String),
    #[error("too large: {0}")]
    TooLarge(String),
}

impl BspError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }

    pub(crate) fn corrupt(msg: impl Into<String>) -> Self {
        Self::Corrupt(msg.into())
    }

    pub(crate) fn zip(msg: impl Into<String>) -> Self {
        Self::Zip(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, BspError>;
