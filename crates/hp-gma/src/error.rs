use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GmaError {
    #[error("cannot read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("not a GMA archive")]
    BadMagic,
    #[error("unsupported GMA version {0}")]
    UnsupportedVersion(u8),
    #[error("corrupt GMA archive: {0}")]
    Corrupt(String),
    #[error("cannot decompress legacy GMA: {0}")]
    Decompress(String),
    #[error("data of {path} lies outside of the archive")]
    OutOfBounds { path: String },
    #[error("CRC mismatch for {path}: stored {expected:#010x}, computed {actual:#010x}")]
    CrcMismatch { path: String, expected: u32, actual: u32 },
}

impl GmaError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }

    pub(crate) fn corrupt(msg: impl Into<String>) -> Self {
        Self::Corrupt(msg.into())
    }
}
