use std::borrow::Cow;
use std::io::Cursor;
use std::sync::LazyLock;

use rayon::prelude::*;
use texpresso::{Algorithm, COLOUR_WEIGHTS_PERCEPTUAL, COLOUR_WEIGHTS_UNIFORM, Params};

use crate::header::{RESOURCE_HIGH_RES, RESOURCE_LOW_RES, SIGNATURE};
use crate::{ImageFormat, RgbaImage, VtfError, flags};

const GAMMA: f32 = 2.2;

pub const MAX_POW2_SIZE: u32 = 4096;

const LOW_RES_MAX: u32 = 16;

const MAX_IMPORT_DIM: u32 = 32_768;

const PARALLEL_PIXELS: usize = 128 * 128;

static GAMMA_TO_LINEAR: LazyLock<[f32; 256]> =
    LazyLock::new(|| std::array::from_fn(|i| (i as f32 / 255.0).powf(GAMMA)));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum EncodeFormat {
    #[default]
    Auto,
    Dxt1,
    Dxt5,
    Bgra8888,
    Bgr888,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum EncodeQuality {
    Fast,
    #[default]
    Normal,
    Best,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeOptions {
    pub format: EncodeFormat,
    pub mipmaps: bool,
    pub flags: u32,
    pub version_minor: u32,
    pub resize_to_pow2: bool,
    pub quality: EncodeQuality,
    pub mip_filter: MipFilter,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            format: EncodeFormat::Auto,
            mipmaps: true,
            flags: 0,
            version_minor: 4,
            resize_to_pow2: true,
            quality: EncodeQuality::Normal,
            mip_filter: MipFilter::FromFlags,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum MipFilter {
    #[default]
    FromFlags,
    Color,
    Data,
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutFormat {
    Dxt1,
    Dxt5,
    Bgra8888,
    Bgr888,
}

impl OutFormat {
    fn image_format(self) -> ImageFormat {
        match self {
            Self::Dxt1 => ImageFormat::Dxt1,
            Self::Dxt5 => ImageFormat::Dxt5,
            Self::Bgra8888 => ImageFormat::Bgra8888,
            Self::Bgr888 => ImageFormat::Bgr888,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    Gamma,
    Linear,
    Normal,
}

fn filter_for(tex_flags: u32) -> Filter {
    if tex_flags & flags::NORMAL != 0 {
        Filter::Normal
    } else if tex_flags & flags::SSBUMP != 0 {
        Filter::Linear
    } else {
        Filter::Gamma
    }
}

#[inline]
fn linear_to_gamma8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0).powf(1.0 / GAMMA) * 255.0 + 0.5) as u8
}

#[inline]
fn unorm8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

struct AxisTaps {
    spans: Vec<(usize, usize)>,
    taps: Vec<(usize, f32)>,
}

impl AxisTaps {
    fn new(src: usize, dst: usize) -> Self {
        let mut spans = Vec::with_capacity(dst);
        let mut taps = Vec::new();
        let scale = src as f64 / dst as f64;
        let last = src.saturating_sub(1);
        for i in 0..dst {
            let start = taps.len();
            if scale >= 1.0 {
                let lo = i as f64 * scale;
                let hi = (i + 1) as f64 * scale;
                let mut j = lo.floor() as usize;
                while (j as f64) < hi && j < src {
                    let overlap = hi.min(j as f64 + 1.0) - lo.max(j as f64);
                    if overlap > 1e-9 {
                        taps.push((j, (overlap / scale) as f32));
                    }
                    j += 1;
                }
            } else {
                let center = ((i as f64 + 0.5) * scale - 0.5).max(0.0);
                let j0 = (center.floor() as usize).min(last);
                let j1 = (j0 + 1).min(last);
                let t = (center - j0 as f64).clamp(0.0, 1.0) as f32;
                taps.push((j0, 1.0 - t));
                if j1 != j0 && t > 0.0 {
                    taps.push((j1, t));
                }
            }
            if taps.len() == start {
                taps.push((i.min(last), 1.0));
            }
            let sum: f32 = taps.get(start..).map_or(0.0, |s| s.iter().map(|t| t.1).sum());
            if sum > 0.0
                && let Some(span) = taps.get_mut(start..)
            {
                for t in span {
                    t.1 /= sum;
                }
            }
            spans.push((start, taps.len()));
        }
        Self { spans, taps }
    }

    fn get(&self, i: usize) -> &[(usize, f32)] {
        self.spans.get(i).and_then(|&(a, b)| self.taps.get(a..b)).unwrap_or(&[])
    }
}

fn resample(src: &[u8], w: usize, h: usize, tw: usize, th: usize, filter: Filter) -> Vec<u8> {
    let px = src.as_chunks::<4>().0;
    let xt = AxisTaps::new(w, tw);
    let yt = AxisTaps::new(h, th);
    let lut = &*GAMMA_TO_LINEAR;
    let mut out = vec![0u8; tw * th * 4];
    let row = |(ty, dst): (usize, &mut [u8])| {
        let ytaps = yt.get(ty);
        for (tx, d) in dst.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let xtaps = xt.get(tx);
            let mut acc = [0f32; 4];
            let mut plain = [0f32; 3];
            for &(sy, wy) in ytaps {
                let line = sy * w;
                for &(sx, wx) in xtaps {
                    let Some(p) = px.get(line + sx) else { continue };
                    let wgt = wx * wy;
                    match filter {
                        Filter::Gamma => {
                            let a = f32::from(p[3]) * (1.0 / 255.0) * wgt;
                            for c in 0..3 {
                                let l = lut[usize::from(p[c])];
                                acc[c] += l * a;
                                plain[c] += l * wgt;
                            }
                            acc[3] += a;
                        }
                        Filter::Linear => {
                            for c in 0..4 {
                                acc[c] += f32::from(p[c]) * wgt;
                            }
                        }
                        Filter::Normal => {
                            for c in 0..3 {
                                acc[c] += (f32::from(p[c]) / 127.5 - 1.0) * wgt;
                            }
                            acc[3] += f32::from(p[3]) * wgt;
                        }
                    }
                }
            }
            *d = match filter {
                Filter::Gamma => {
                    let a = acc[3];
                    let rgb = if a > 1e-6 { [acc[0] / a, acc[1] / a, acc[2] / a] } else { plain };
                    [linear_to_gamma8(rgb[0]), linear_to_gamma8(rgb[1]), linear_to_gamma8(rgb[2]), unorm8(a)]
                }
                Filter::Linear => acc.map(|v| (v + 0.5).clamp(0.0, 255.0) as u8),
                Filter::Normal => {
                    let len = (acc[0] * acc[0] + acc[1] * acc[1] + acc[2] * acc[2]).sqrt();
                    let n = if len > 1e-6 { [acc[0] / len, acc[1] / len, acc[2] / len] } else { [0.0, 0.0, 1.0] };
                    let enc = |v: f32| unorm8(v * 0.5 + 0.5);
                    [enc(n[0]), enc(n[1]), enc(n[2]), (acc[3] + 0.5).clamp(0.0, 255.0) as u8]
                }
            };
        }
    };
    if tw * th >= PARALLEL_PIXELS {
        out.par_chunks_mut(tw * 4).enumerate().for_each(row);
    } else {
        out.chunks_mut(tw * 4).enumerate().for_each(row);
    }
    out
}

fn nearest_pow2(v: u32) -> u32 {
    if v >= MAX_POW2_SIZE {
        return MAX_POW2_SIZE;
    }
    if v <= 1 {
        return 1;
    }
    let up = v.next_power_of_two();
    let down = up >> 1;
    if v - down < up - v { down } else { up }
}

fn reflectivity(pixels: &[u8]) -> [f32; 3] {
    let lut = &*GAMMA_TO_LINEAR;
    let px = pixels.as_chunks::<4>().0;
    if px.is_empty() {
        return [0.0; 3];
    }
    let sum = px
        .par_chunks(16 * 1024)
        .map(|chunk| {
            let mut s = [0f64; 3];
            for p in chunk {
                for c in 0..3 {
                    s[c] += f64::from(lut[usize::from(p[c])]);
                }
            }
            s
        })
        .reduce(|| [0.0; 3], |a, b| [a[0] + b[0], a[1] + b[1], a[2] + b[2]]);
    let n = px.len() as f64;
    sum.map(|s| (s / n) as f32)
}

fn texpresso_params(quality: EncodeQuality, filter: Filter) -> Params {
    Params {
        algorithm: match quality {
            EncodeQuality::Fast => Algorithm::RangeFit,
            EncodeQuality::Normal => Algorithm::ClusterFit,
            EncodeQuality::Best => Algorithm::IterativeClusterFit,
        },
        weights: if filter == Filter::Gamma { COLOUR_WEIGHTS_PERCEPTUAL } else { COLOUR_WEIGHTS_UNIFORM },
        weigh_colour_by_alpha: false,
    }
}

fn compress_level(format: OutFormat, rgba: &[u8], w: usize, h: usize, params: Params) -> Vec<u8> {
    let bc = |f: texpresso::Format| {
        let mut out = vec![0u8; f.compressed_size(w, h)];
        f.compress(rgba, w, h, params, &mut out);
        out
    };
    match format {
        OutFormat::Dxt1 => bc(texpresso::Format::Bc1),
        OutFormat::Dxt5 => bc(texpresso::Format::Bc3),
        OutFormat::Bgra8888 => rgba.as_chunks::<4>().0.iter().flat_map(|&[r, g, b, a]| [b, g, r, a]).collect(),
        OutFormat::Bgr888 => rgba.as_chunks::<4>().0.iter().flat_map(|&[r, g, b, _]| [b, g, r]).collect(),
    }
}

fn check_image(img: &RgbaImage) -> Result<(usize, usize), VtfError> {
    if img.width == 0 || img.height == 0 {
        return Err(VtfError::InvalidDimensions { width: img.width, height: img.height });
    }
    let expected = u64::from(img.width) * u64::from(img.height) * 4;
    if img.pixels.len() as u64 != expected {
        return Err(VtfError::InvalidImage(format!(
            "{}x{} image needs {expected} bytes, got {}",
            img.width,
            img.height,
            img.pixels.len()
        )));
    }
    Ok((img.width as usize, img.height as usize))
}

pub fn encode(img: &RgbaImage, opts: &EncodeOptions) -> Result<Vec<u8>, VtfError> {
    encode_frames(std::slice::from_ref(img), opts)
}

pub fn encode_frames(frames: &[RgbaImage], opts: &EncodeOptions) -> Result<Vec<u8>, VtfError> {
    let first = frames.first().ok_or_else(|| VtfError::InvalidImage("no frames".into()))?;
    if frames.len() > usize::from(u16::MAX) {
        return Err(VtfError::InvalidImage("too many frames".into()));
    }
    if opts.version_minor > 5 {
        return Err(VtfError::UnsupportedVersion(7, opts.version_minor));
    }
    let (sw, sh) = check_image(first)?;
    for f in frames {
        if check_image(f)? != (sw, sh) {
            return Err(VtfError::InvalidImage("all frames must have the same size".into()));
        }
    }

    let (tw, th) = if opts.resize_to_pow2 {
        (nearest_pow2(first.width), nearest_pow2(first.height))
    } else {
        (first.width, first.height)
    };
    if tw > u32::from(u16::MAX) || th > u32::from(u16::MAX) {
        return Err(VtfError::TooLarge);
    }
    let (w, h) = (tw as usize, th as usize);

    let mut tex_flags = opts.flags & !flags::ENVMAP;
    let filter = match opts.mip_filter {
        MipFilter::FromFlags => filter_for(tex_flags),
        MipFilter::Color => Filter::Gamma,
        MipFilter::Data => Filter::Linear,
        MipFilter::Normal => Filter::Normal,
    };
    let opaque = frames.iter().all(RgbaImage::is_opaque);
    let format = match opts.format {
        EncodeFormat::Auto if opaque => OutFormat::Dxt1,
        EncodeFormat::Auto | EncodeFormat::Dxt5 => OutFormat::Dxt5,
        EncodeFormat::Dxt1 => OutFormat::Dxt1,
        EncodeFormat::Bgra8888 => OutFormat::Bgra8888,
        EncodeFormat::Bgr888 => OutFormat::Bgr888,
    };
    if !opaque {
        match format {
            OutFormat::Dxt1 => tex_flags |= flags::ONEBITALPHA,
            OutFormat::Dxt5 | OutFormat::Bgra8888 if tex_flags & flags::ONEBITALPHA == 0 => {
                tex_flags |= flags::EIGHTBITALPHA;
            }
            _ => {}
        }
    }

    let mip_count = if opts.mipmaps { 32 - tw.max(th).leading_zeros() } else { 1 };

    let (mut lw, mut lh) = (tw, th);
    let mut low_mip = 0u32;
    while lw > LOW_RES_MAX || lh > LOW_RES_MAX {
        lw = (lw / 2).max(1);
        lh = (lh / 2).max(1);
        low_mip += 1;
    }

    let params = texpresso_params(opts.quality, filter);
    let mut levels: Vec<Vec<Vec<u8>>> = Vec::with_capacity(frames.len());
    let mut low_src: Option<Vec<u8>> = None;
    let mut base_reflectivity = [0f32; 3];
    for (fi, frame) in frames.iter().enumerate() {
        let mut cur: Cow<'_, [u8]> = if (sw, sh) == (w, h) {
            Cow::Borrowed(&frame.pixels)
        } else {
            Cow::Owned(resample(&frame.pixels, sw, sh, w, h, filter))
        };
        if fi == 0 {
            base_reflectivity = reflectivity(&cur);
        }
        let (mut cw, mut ch) = (w, h);
        let mut out_levels = Vec::with_capacity(mip_count as usize);
        for mip in 0..mip_count {
            if mip > 0 {
                let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
                cur = Cow::Owned(resample(&cur, cw, ch, nw, nh, filter));
                (cw, ch) = (nw, nh);
            }
            if fi == 0 && mip == low_mip {
                low_src = Some(cur.to_vec());
            }
            out_levels.push(compress_level(format, &cur, cw, ch, params));
        }
        levels.push(out_levels);
    }
    let mut low_rgba = match low_src {
        Some(v) => v,
        None => resample(&first.pixels, sw, sh, lw as usize, lh as usize, filter),
    };
    for p in low_rgba.as_chunks_mut::<4>().0 {
        p[3] = 255;
    }
    let low_data = compress_level(
        OutFormat::Dxt1,
        &low_rgba,
        lw as usize,
        lh as usize,
        texpresso_params(EncodeQuality::Fast, filter),
    );

    let hi_size: usize = levels.iter().flatten().map(Vec::len).sum();
    let minor = opts.version_minor;
    let header_size: usize = match minor {
        0 | 1 => 64,
        2 => 80,
        _ => 80 + 2 * 8,
    };
    let low_offset = header_size;
    let hi_offset = low_offset + low_data.len();
    let total = hi_offset + hi_size;
    let hi_offset_u32 = u32::try_from(hi_offset).map_err(|_| VtfError::TooLarge)?;
    u32::try_from(total).map_err(|_| VtfError::TooLarge)?;

    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&SIGNATURE);
    out.extend_from_slice(&7u32.to_le_bytes());
    out.extend_from_slice(&minor.to_le_bytes());
    out.extend_from_slice(&(header_size as u32).to_le_bytes());
    out.extend_from_slice(&(tw as u16).to_le_bytes());
    out.extend_from_slice(&(th as u16).to_le_bytes());
    out.extend_from_slice(&tex_flags.to_le_bytes());
    out.extend_from_slice(&(frames.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    for c in base_reflectivity {
        out.extend_from_slice(&c.to_le_bytes());
    }
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&1.0f32.to_le_bytes());
    out.extend_from_slice(&format.image_format().raw().to_le_bytes());
    out.push(mip_count as u8);
    out.extend_from_slice(&ImageFormat::Dxt1.raw().to_le_bytes());
    out.push(lw as u8);
    out.push(lh as u8);
    if minor >= 2 {
        out.extend_from_slice(&1u16.to_le_bytes());
    }
    if minor >= 3 {
        out.extend_from_slice(&[0; 3]);
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(&RESOURCE_LOW_RES);
        out.push(0);
        out.extend_from_slice(&(low_offset as u32).to_le_bytes());
        out.extend_from_slice(&RESOURCE_HIGH_RES);
        out.push(0);
        out.extend_from_slice(&hi_offset_u32.to_le_bytes());
    }
    out.resize(header_size, 0);
    out.extend_from_slice(&low_data);
    for mip in (0..mip_count as usize).rev() {
        for frame in &levels {
            if let Some(level) = frame.get(mip) {
                out.extend_from_slice(level);
            }
        }
    }
    Ok(out)
}

pub fn load_image_file(bytes: &[u8], ext_hint: &str) -> Result<RgbaImage, VtfError> {
    use image::ImageFormat as F;
    let supported = |f: &F| matches!(f, F::Png | F::Jpeg | F::Tga | F::Bmp);
    let ext = ext_hint.rsplit('.').next().unwrap_or(ext_hint).trim();
    let hinted = F::from_extension(ext).filter(supported);
    let guessed = image::guess_format(bytes).ok().filter(supported);
    let format = match (hinted, guessed) {
        (Some(F::Tga), _) => F::Tga,
        (h, g) => g.or(h).ok_or_else(|| {
            VtfError::ImageDecode(format!("unsupported image type (hint '{ext_hint}'); expected PNG, JPG, TGA or BMP"))
        })?,
    };
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMPORT_DIM);
    limits.max_image_height = Some(MAX_IMPORT_DIM);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|e| VtfError::ImageDecode(e.to_string()))?.into_rgba8();
    let (width, height) = decoded.dimensions();
    if width == 0 || height == 0 {
        return Err(VtfError::InvalidDimensions { width, height });
    }
    Ok(RgbaImage { width, height, pixels: decoded.into_raw() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pow2_rounding() {
        assert_eq!(nearest_pow2(1), 1);
        assert_eq!(nearest_pow2(3), 4);
        assert_eq!(nearest_pow2(300), 256);
        assert_eq!(nearest_pow2(384), 512);
        assert_eq!(nearest_pow2(400), 512);
        assert_eq!(nearest_pow2(512), 512);
        assert_eq!(nearest_pow2(5000), 4096);
    }

    #[test]
    fn taps_sum_to_one() {
        for (s, d) in [(4, 2), (3, 1), (5, 4), (2, 3), (300, 256), (7, 7), (1, 4)] {
            let t = AxisTaps::new(s, d);
            for i in 0..d {
                let sum: f32 = t.get(i).iter().map(|x| x.1).sum();
                assert!((sum - 1.0).abs() < 1e-4, "{s}->{d} at {i}: {sum}");
                assert!(t.get(i).iter().all(|x| x.0 < s));
            }
        }
    }

    #[test]
    fn gamma_mip_of_black_and_white_is_bright() {
        let src = [0, 0, 0, 255, 255, 255, 255, 255];
        let out = resample(&src, 2, 1, 1, 1, Filter::Gamma);
        assert!((185..=187).contains(&out[0]), "{}", out[0]);
        assert_eq!(out[3], 255);
    }

    #[test]
    fn transparent_pixels_do_not_bleed() {
        let src = [255, 0, 0, 255, 0, 0, 0, 0];
        let out = resample(&src, 2, 1, 1, 1, Filter::Gamma);
        assert_eq!(&out[..3], &[255, 0, 0]);
        assert_eq!(out[3], 128);
    }

    #[test]
    fn normal_maps_are_renormalized() {
        let src = [204, 128, 230, 255, 51, 128, 230, 255];
        let out = resample(&src, 2, 1, 1, 1, Filter::Normal);
        assert!(out[0].abs_diff(128) <= 1, "{out:?}");
        assert!(out[2] >= 254, "{out:?}");
    }

    #[test]
    fn reflectivity_is_linear_average() {
        let px = [255, 0, 0, 255, 0, 0, 0, 255];
        let r = reflectivity(&px);
        assert!((r[0] - 0.5).abs() < 1e-6);
        assert_eq!(r[1], 0.0);
    }
}
