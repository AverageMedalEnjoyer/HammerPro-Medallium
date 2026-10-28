use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileKind {
    Mdl,
    Vvd,
    Vtx,
    Phy,
}

impl fmt::Display for FileKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            FileKind::Mdl => "MDL",
            FileKind::Vvd => "VVD",
            FileKind::Vtx => "VTX",
            FileKind::Phy => "PHY",
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MdlError {
    #[error("{file}: bad magic {found:?} (expected {expected:?})")]
    BadMagic { file: FileKind, expected: &'static str, found: String },

    #[error("{file}: unsupported version {version}")]
    UnsupportedVersion { file: FileKind, version: i32 },

    #[error("{file}: {what} out of bounds (offset {offset}, size {size}, file length {file_len})")]
    OutOfBounds { file: FileKind, what: &'static str, offset: i64, size: u64, file_len: usize },

    #[error("{file}: invalid {what}: {value}")]
    InvalidValue { file: FileKind, what: &'static str, value: i64 },

    #[error("{file}: checksum {found:#010x} does not match the MDL checksum {expected:#010x}")]
    ChecksumMismatch { file: FileKind, expected: u32, found: u32 },

    #[error("{file}: cannot read {path}")]
    MissingFile { file: FileKind, path: String },

    #[error("{file}: {what} mismatch (MDL has {mdl}, {file} has {other})")]
    StructureMismatch { file: FileKind, what: &'static str, mdl: i64, other: i64 },
}

impl MdlError {
    pub fn file(&self) -> FileKind {
        match self {
            MdlError::BadMagic { file, .. }
            | MdlError::UnsupportedVersion { file, .. }
            | MdlError::OutOfBounds { file, .. }
            | MdlError::InvalidValue { file, .. }
            | MdlError::ChecksumMismatch { file, .. }
            | MdlError::MissingFile { file, .. }
            | MdlError::StructureMismatch { file, .. } => *file,
        }
    }
}
