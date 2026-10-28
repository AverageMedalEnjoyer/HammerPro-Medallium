use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VpkError {
    #[error("cannot read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("not a VPK directory file (signature {0:#010x})")]
    BadSignature(u32),
    #[error("unsupported VPK version {0}")]
    UnsupportedVersion(u32),
    #[error("corrupt VPK directory: {0}")]
    Corrupt(String),
    #[error("data of {path} lies outside of {}", archive.display())]
    OutOfBounds { path: String, archive: PathBuf },
    #[error("CRC mismatch for {path}: stored {expected:#010x}, computed {actual:#010x}")]
    CrcMismatch { path: String, expected: u32, actual: u32 },
}

impl VpkError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }

    pub(crate) fn corrupt(msg: impl Into<String>) -> Self {
        Self::Corrupt(msg.into())
    }
}
