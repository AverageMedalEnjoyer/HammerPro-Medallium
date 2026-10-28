use std::io::Cursor;
use std::path::Path;

use image::imageops::FilterType;
use image::{ImageEncoder, RgbaImage};

pub(crate) const THUMB_SIZE: u32 = 256;
pub(crate) const ICON_SIZE: u32 = 512;
const MAX_FILE: u64 = 64 << 20;
const MAX_SIDE: u32 = 16_384;

pub(crate) fn square(img: &RgbaImage, size: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let side = w.min(h).max(1);
    let crop = image::imageops::crop_imm(img, (w - side.min(w)) / 2, (h - side.min(h)) / 2, side.min(w), side.min(h));
    image::imageops::resize(&crop.to_image(), size, size, FilterType::Triangle)
}

pub(crate) fn encode_png(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = Cursor::new(Vec::new());
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

pub(crate) fn encode_jpeg(img: &RgbaImage, quality: u8) -> Result<Vec<u8>, String> {
    let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
    let mut out = Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100))
        .write_image(rgb.as_raw(), rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

pub(crate) fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Option<RgbaImage> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    RgbaImage::from_raw(width, height, rgba)
}

pub(crate) fn load_file(path: &Path) -> Result<RgbaImage, String> {
    let bytes =
        super::files::read_limited(path, MAX_FILE).ok_or_else(|| "unreadable or larger than 64 MiB".to_owned())?;
    decode(&bytes)
}

pub(crate) fn decode(bytes: &[u8]) -> Result<RgbaImage, String> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(512 << 20);
    reader.limits(limits);
    Ok(reader.decode().map_err(|e| e.to_string())?.to_rgba8())
}

pub(crate) fn icon_ok(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8]) && decode(bytes).is_ok_and(|i| i.dimensions() == (ICON_SIZE, ICON_SIZE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnails_and_icons() {
        let img = RgbaImage::from_fn(300, 200, |x, y| image::Rgba([x as u8, y as u8, 7, 255]));
        let t = square(&img, THUMB_SIZE);
        assert_eq!(t.dimensions(), (THUMB_SIZE, THUMB_SIZE));
        let png = encode_png(&t).unwrap();
        assert_eq!(decode(&png).unwrap().dimensions(), (THUMB_SIZE, THUMB_SIZE));
        let jpg = encode_jpeg(&square(&img, ICON_SIZE), 90).unwrap();
        assert!(icon_ok(&jpg));
        assert!(!icon_ok(&png));
        assert!(from_rgba(2, 2, vec![0; 15]).is_none());
        assert!(from_rgba(0, 2, Vec::new()).is_none());
        assert!(decode(b"not an image").is_err());
    }
}
