use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    Rgba8888,
    Abgr8888,
    Rgb888,
    Bgr888,
    Rgb565,
    I8,
    Ia88,
    P8,
    A8,
    Rgb888Bluescreen,
    Bgr888Bluescreen,
    Argb8888,
    Bgra8888,
    Dxt1,
    Dxt3,
    Dxt5,
    Bgrx8888,
    Bgr565,
    Bgrx5551,
    Bgra4444,
    Dxt1OneBitAlpha,
    Bgra5551,
    Uv88,
    Uvwq8888,
    Rgba16161616F,
    Rgba16161616,
    Uvlx8888,
    R32F,
    Rgb323232F,
    Rgba32323232F,
    NvDst16,
    NvDst24,
    NvIntz,
    NvRawz,
    AtiDst16,
    AtiDst24,
    NvNull,
    Ati2n,
    Ati1n,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BcFormat {
    Bc1,
    Bc2,
    Bc3,
    Bc4,
    Bc5,
}

impl BcFormat {
    #[must_use]
    pub const fn block_bytes(self) -> u32 {
        match self {
            Self::Bc1 | Self::Bc4 => 8,
            Self::Bc2 | Self::Bc3 | Self::Bc5 => 16,
        }
    }
}

const ALL_FORMATS: [ImageFormat; 39] = [
    ImageFormat::Rgba8888,
    ImageFormat::Abgr8888,
    ImageFormat::Rgb888,
    ImageFormat::Bgr888,
    ImageFormat::Rgb565,
    ImageFormat::I8,
    ImageFormat::Ia88,
    ImageFormat::P8,
    ImageFormat::A8,
    ImageFormat::Rgb888Bluescreen,
    ImageFormat::Bgr888Bluescreen,
    ImageFormat::Argb8888,
    ImageFormat::Bgra8888,
    ImageFormat::Dxt1,
    ImageFormat::Dxt3,
    ImageFormat::Dxt5,
    ImageFormat::Bgrx8888,
    ImageFormat::Bgr565,
    ImageFormat::Bgrx5551,
    ImageFormat::Bgra4444,
    ImageFormat::Dxt1OneBitAlpha,
    ImageFormat::Bgra5551,
    ImageFormat::Uv88,
    ImageFormat::Uvwq8888,
    ImageFormat::Rgba16161616F,
    ImageFormat::Rgba16161616,
    ImageFormat::Uvlx8888,
    ImageFormat::R32F,
    ImageFormat::Rgb323232F,
    ImageFormat::Rgba32323232F,
    ImageFormat::NvDst16,
    ImageFormat::NvDst24,
    ImageFormat::NvIntz,
    ImageFormat::NvRawz,
    ImageFormat::AtiDst16,
    ImageFormat::AtiDst24,
    ImageFormat::NvNull,
    ImageFormat::Ati2n,
    ImageFormat::Ati1n,
];

impl ImageFormat {
    pub const ALL: &'static [ImageFormat] = &ALL_FORMATS;

    #[must_use]
    pub fn from_raw(raw: u32) -> Option<Self> {
        usize::try_from(raw).ok().and_then(|i| ALL_FORMATS.get(i)).copied()
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self as u32
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rgba8888 => "RGBA8888",
            Self::Abgr8888 => "ABGR8888",
            Self::Rgb888 => "RGB888",
            Self::Bgr888 => "BGR888",
            Self::Rgb565 => "RGB565",
            Self::I8 => "I8",
            Self::Ia88 => "IA88",
            Self::P8 => "P8",
            Self::A8 => "A8",
            Self::Rgb888Bluescreen => "RGB888_BLUESCREEN",
            Self::Bgr888Bluescreen => "BGR888_BLUESCREEN",
            Self::Argb8888 => "ARGB8888",
            Self::Bgra8888 => "BGRA8888",
            Self::Dxt1 => "DXT1",
            Self::Dxt3 => "DXT3",
            Self::Dxt5 => "DXT5",
            Self::Bgrx8888 => "BGRX8888",
            Self::Bgr565 => "BGR565",
            Self::Bgrx5551 => "BGRX5551",
            Self::Bgra4444 => "BGRA4444",
            Self::Dxt1OneBitAlpha => "DXT1_ONEBITALPHA",
            Self::Bgra5551 => "BGRA5551",
            Self::Uv88 => "UV88",
            Self::Uvwq8888 => "UVWQ8888",
            Self::Rgba16161616F => "RGBA16161616F",
            Self::Rgba16161616 => "RGBA16161616",
            Self::Uvlx8888 => "UVLX8888",
            Self::R32F => "R32F",
            Self::Rgb323232F => "RGB323232F",
            Self::Rgba32323232F => "RGBA32323232F",
            Self::NvDst16 => "NV_DST16",
            Self::NvDst24 => "NV_DST24",
            Self::NvIntz => "NV_INTZ",
            Self::NvRawz => "NV_RAWZ",
            Self::AtiDst16 => "ATI_DST16",
            Self::AtiDst24 => "ATI_DST24",
            Self::NvNull => "NV_NULL",
            Self::Ati2n => "ATI2N",
            Self::Ati1n => "ATI1N",
        }
    }

    #[must_use]
    pub const fn bc_format(self) -> Option<BcFormat> {
        match self {
            Self::Dxt1 | Self::Dxt1OneBitAlpha => Some(BcFormat::Bc1),
            Self::Dxt3 => Some(BcFormat::Bc2),
            Self::Dxt5 => Some(BcFormat::Bc3),
            Self::Ati1n => Some(BcFormat::Bc4),
            Self::Ati2n => Some(BcFormat::Bc5),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_dxt(self) -> bool {
        matches!(self.bc_format(), Some(BcFormat::Bc1 | BcFormat::Bc2 | BcFormat::Bc3))
    }

    #[must_use]
    pub const fn is_block_compressed(self) -> bool {
        self.bc_format().is_some()
    }

    #[must_use]
    pub const fn bytes_per_pixel(self) -> Option<u32> {
        Some(match self {
            Self::I8 | Self::P8 | Self::A8 => 1,
            Self::Rgb565
            | Self::Ia88
            | Self::Bgr565
            | Self::Bgrx5551
            | Self::Bgra4444
            | Self::Bgra5551
            | Self::Uv88
            | Self::NvDst16
            | Self::AtiDst16 => 2,
            Self::Rgb888 | Self::Bgr888 | Self::Rgb888Bluescreen | Self::Bgr888Bluescreen => 3,
            Self::Rgba8888
            | Self::Abgr8888
            | Self::Argb8888
            | Self::Bgra8888
            | Self::Bgrx8888
            | Self::Uvwq8888
            | Self::Uvlx8888
            | Self::R32F
            | Self::NvDst24
            | Self::NvIntz
            | Self::NvRawz
            | Self::AtiDst24
            | Self::NvNull => 4,
            Self::Rgba16161616F | Self::Rgba16161616 => 8,
            Self::Rgb323232F => 12,
            Self::Rgba32323232F => 16,
            Self::Dxt1 | Self::Dxt3 | Self::Dxt5 | Self::Dxt1OneBitAlpha | Self::Ati2n | Self::Ati1n => return None,
        })
    }

    #[must_use]
    pub const fn has_alpha(self) -> bool {
        matches!(
            self,
            Self::Rgba8888
                | Self::Abgr8888
                | Self::Ia88
                | Self::A8
                | Self::Rgb888Bluescreen
                | Self::Bgr888Bluescreen
                | Self::Argb8888
                | Self::Bgra8888
                | Self::Dxt1OneBitAlpha
                | Self::Dxt3
                | Self::Dxt5
                | Self::Bgra4444
                | Self::Bgra5551
                | Self::Uvwq8888
                | Self::Rgba16161616F
                | Self::Rgba16161616
                | Self::Rgba32323232F
        )
    }

    #[must_use]
    pub const fn is_decodable(self) -> bool {
        !matches!(
            self,
            Self::NvDst16
                | Self::NvDst24
                | Self::NvIntz
                | Self::NvRawz
                | Self::AtiDst16
                | Self::AtiDst24
                | Self::NvNull
        )
    }

    #[must_use]
    pub fn image_size(self, width: u32, height: u32) -> Option<u64> {
        let (w, h) = (u64::from(width), u64::from(height));
        match self.bc_format() {
            Some(bc) => w.div_ceil(4).checked_mul(h.div_ceil(4))?.checked_mul(u64::from(bc.block_bytes())),
            None => w.checked_mul(h)?.checked_mul(u64::from(self.bytes_per_pixel().unwrap_or(4))),
        }
    }
}

impl fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_values_round_trip() {
        for (i, f) in ImageFormat::ALL.iter().enumerate() {
            assert_eq!(f.raw() as usize, i);
            assert_eq!(ImageFormat::from_raw(f.raw()), Some(*f));
        }
        assert_eq!(ImageFormat::from_raw(39), None);
        assert_eq!(ImageFormat::from_raw(u32::MAX), None);
        assert_eq!(ImageFormat::Dxt5.raw(), 15);
        assert_eq!(ImageFormat::Ati2n.raw(), 37);
    }

    #[test]
    fn block_sizes_round_up() {
        assert_eq!(ImageFormat::Dxt1.image_size(1, 1), Some(8));
        assert_eq!(ImageFormat::Dxt5.image_size(2, 1), Some(16));
        assert_eq!(ImageFormat::Dxt1.image_size(5, 4), Some(16));
        assert_eq!(ImageFormat::Dxt5.image_size(16, 16), Some(256));
        assert_eq!(ImageFormat::Bgr888.image_size(3, 2), Some(18));
        assert_eq!(ImageFormat::Rgba16161616F.image_size(2, 2), Some(32));
    }

    #[test]
    fn bc_mapping() {
        assert_eq!(ImageFormat::Dxt1.bc_format(), Some(BcFormat::Bc1));
        assert_eq!(ImageFormat::Dxt1OneBitAlpha.bc_format(), Some(BcFormat::Bc1));
        assert_eq!(ImageFormat::Dxt3.bc_format(), Some(BcFormat::Bc2));
        assert_eq!(ImageFormat::Dxt5.bc_format(), Some(BcFormat::Bc3));
        assert!(ImageFormat::Dxt5.is_dxt());
        assert!(!ImageFormat::Ati2n.is_dxt());
        assert_eq!(ImageFormat::Bgra8888.bc_format(), None);
    }
}
