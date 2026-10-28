use crate::ImageFormat;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VtfError {
    #[error("not a VTF file (bad signature)")]
    BadSignature,
    #[error("unsupported VTF version {0}.{1}")]
    UnsupportedVersion(u32, u32),
    #[error("VTF data truncated: {needed} bytes needed, {available} available")]
    Truncated { needed: u64, available: u64 },
    #[error("invalid texture dimensions {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },
    #[error("unknown VTF image format id {0}")]
    UnknownFormat(u32),
    #[error("VTF image format {0} cannot be decoded")]
    UnsupportedFormat(ImageFormat),
    #[error("mip level {index} out of range (texture has {count})")]
    MipOutOfRange { index: u32, count: u32 },
    #[error("frame {index} out of range (texture has {count})")]
    FrameOutOfRange { index: u32, count: u32 },
    #[error("face {index} out of range (texture has {count})")]
    FaceOutOfRange { index: u32, count: u32 },
    #[error("corrupt VTF header: {0}")]
    Corrupt(&'static str),
    #[error("texture too large")]
    TooLarge,
    #[error("invalid input image: {0}")]
    InvalidImage(String),
    #[error("could not decode image file: {0}")]
    ImageDecode(String),
}
