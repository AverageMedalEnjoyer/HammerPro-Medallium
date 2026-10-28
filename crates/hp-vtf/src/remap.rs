use rayon::prelude::*;

use crate::encode::MipFilter;
use crate::{
    DecodeOptions, EncodeFormat, EncodeOptions, EncodeQuality, ImageFormat, MipSelect, RgbaImage, VtfError,
    decode_rgba8, encode_frames, read_info,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TexelKind {
    Color,
    Normal,
    SsBump,
}

pub const MAX_REPEAT: i32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Angle,
    Fraction,
    TooMany,
    SsBump,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexelMap {
    pub m: [[i32; 2]; 2],
    pub t: [f64; 2],
}

impl TexelMap {
    pub const IDENTITY: TexelMap = TexelMap { m: [[1, 0], [0, 1]], t: [0.0, 0.0] };

    pub fn from_affine(m: [[f64; 2]; 2], t: [f64; 2]) -> Result<TexelMap, Refusal> {
        const EPS: f64 = 1e-4;
        if !m.iter().flatten().chain(t.iter()).all(|v| v.is_finite()) {
            return Err(Refusal::Angle);
        }
        let zero = |v: f64| v.abs() < EPS;
        let diagonal = zero(m[0][1]) && zero(m[1][0]) && !zero(m[0][0]) && !zero(m[1][1]);
        let anti = zero(m[0][0]) && zero(m[1][1]) && !zero(m[0][1]) && !zero(m[1][0]);
        if !diagonal && !anti {
            return Err(Refusal::Angle);
        }
        let mut out = [[0i32; 2]; 2];
        for r in 0..2 {
            for c in 0..2 {
                let v = m[r][c];
                if zero(v) {
                    continue;
                }
                let n = v.round();
                if (v - n).abs() > EPS || n == 0.0 {
                    return Err(Refusal::Fraction);
                }
                if n.abs() > f64::from(MAX_REPEAT) {
                    return Err(Refusal::TooMany);
                }
                out[r][c] = n as i32;
            }
        }
        let wrap = |v: f64| {
            let w = v.rem_euclid(1.0);
            if (1.0 - w).abs() < 1e-9 { 0.0 } else { w }
        };
        Ok(TexelMap { m: out, t: [wrap(t[0]), wrap(t[1])] })
    }

    pub fn is_identity(&self) -> bool {
        self.m == TexelMap::IDENTITY.m && self.t[0].abs() < 1e-9 && self.t[1].abs() < 1e-9
    }

    fn swaps(&self) -> bool {
        self.m[0][0] == 0
    }

    fn repeats(&self) -> [usize; 2] {
        let col = |c: usize| (self.m[0][c].unsigned_abs() + self.m[1][c].unsigned_abs()) as usize;
        [col(0).max(1), col(1).max(1)]
    }

    pub fn check(&self, kind: TexelKind) -> Result<(), Refusal> {
        if kind == TexelKind::SsBump && (self.swaps() || self.m[0][0] < 0) {
            return Err(Refusal::SsBump);
        }
        Ok(())
    }

    pub fn output_size(&self, size: [u32; 2]) -> [u32; 2] {
        if self.swaps() { [size[1], size[0]] } else { size }
    }

    fn vector_sources(&self) -> [(usize, f32); 2] {
        let mut out = [(0, 1.0), (1, 1.0)];
        for (c, slot) in out.iter_mut().enumerate() {
            for r in 0..2 {
                let v = self.m[r][c];
                if v != 0 {
                    *slot = (r, if v > 0 { 1.0 } else { -1.0 });
                }
            }
        }
        out
    }
}

type Tap = (usize, f32);

fn taps_at(x: f64, y: f64, w: usize, h: usize, out: &mut Vec<Tap>, weight: f32) {
    let snap = |v: f64| {
        let r = v.round();
        if (v - r).abs() < 1e-6 { r } else { v }
    };
    let (x, y) = (snap(x), snap(y));
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
    let wrap = |v: f64, n: usize| (v as i64).rem_euclid(n as i64) as usize;
    let (xa, xb) = (wrap(x0, w), wrap(x0 + 1.0, w));
    let (ya, yb) = (wrap(y0, h), wrap(y0 + 1.0, h));
    for (xi, yi, wt) in
        [(xa, ya, (1.0 - fx) * (1.0 - fy)), (xb, ya, fx * (1.0 - fy)), (xa, yb, (1.0 - fx) * fy), (xb, yb, fx * fy)]
    {
        if wt > 0.0 {
            out.push((yi * w + xi, wt * weight));
        }
    }
}

pub fn remap_image(img: &RgbaImage, map: &TexelMap, kind: TexelKind) -> Result<RgbaImage, VtfError> {
    let (w, h) = (img.width as usize, img.height as usize);
    if w == 0 || h == 0 || img.pixels.len() != w * h * 4 {
        return Err(VtfError::InvalidDimensions { width: img.width, height: img.height });
    }
    map.check(kind).map_err(|r| VtfError::InvalidImage(format!("cannot remap this texture: {r:?}")))?;
    let [ow, oh] = map.output_size([img.width, img.height]);
    let (nw, nh) = (ow as usize, oh as usize);
    let [k0, k1] = map.repeats();
    let m = map.m.map(|r| r.map(f64::from));
    let src = img.pixels.as_chunks::<4>().0;
    let vec_src = map.vector_sources();
    let swap_gb = kind == TexelKind::SsBump && map.m[1][1] < 0;
    let sub = 1.0 / (k0 * k1) as f32;
    let mut out = vec![0u8; nw * nh * 4];
    out.par_chunks_mut(nw * 4).enumerate().for_each(|(j, row)| {
        let mut taps: Vec<Tap> = Vec::with_capacity(4 * k0 * k1);
        for (i, d) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            taps.clear();
            for b in 0..k1 {
                for a in 0..k0 {
                    let px = (i as f64 + (a as f64 + 0.5) / k0 as f64) / nw as f64;
                    let py = (j as f64 + (b as f64 + 0.5) / k1 as f64) / nh as f64;
                    let qx = m[0][0] * px + m[0][1] * py + map.t[0];
                    let qy = m[1][0] * px + m[1][1] * py + map.t[1];
                    taps_at(qx * w as f64 - 0.5, qy * h as f64 - 0.5, w, h, &mut taps, sub);
                }
            }
            *d = match (kind, taps.as_slice()) {
                (TexelKind::Normal, [(at, _)]) => {
                    let p = src.get(*at).copied().unwrap_or([128, 128, 255, 255]);
                    let comp = |(from, sign): (usize, f32)| {
                        let c = p[from];
                        if sign < 0.0 { 255 - c } else { c }
                    };
                    [comp(vec_src[0]), comp(vec_src[1]), p[2], p[3]]
                }
                (TexelKind::Normal, _) => {
                    let mut acc = [0f32; 4];
                    for &(at, wt) in &taps {
                        let Some(p) = src.get(at) else { continue };
                        let v = |c: usize| f32::from(p[c]) / 127.5 - 1.0;
                        let n = [v(0), v(1)];
                        acc[0] += vec_src[0].1 * n[vec_src[0].0] * wt;
                        acc[1] += vec_src[1].1 * n[vec_src[1].0] * wt;
                        acc[2] += v(2) * wt;
                        acc[3] += f32::from(p[3]) * wt;
                    }
                    let len = (acc[0] * acc[0] + acc[1] * acc[1] + acc[2] * acc[2]).sqrt();
                    let n = if len > 1e-6 { [acc[0] / len, acc[1] / len, acc[2] / len] } else { [0.0, 0.0, 1.0] };
                    let enc = |v: f32| ((v * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    [enc(n[0]), enc(n[1]), enc(n[2]), (acc[3] + 0.5).clamp(0.0, 255.0) as u8]
                }
                (TexelKind::Color | TexelKind::SsBump, [(at, _)]) => src.get(*at).copied().unwrap_or_default(),
                (TexelKind::Color | TexelKind::SsBump, _) => {
                    let mut acc = [0f32; 4];
                    for &(at, wt) in &taps {
                        let Some(p) = src.get(at) else { continue };
                        for c in 0..4 {
                            acc[c] += f32::from(p[c]) * wt;
                        }
                    }
                    acc.map(|v| (v + 0.5).clamp(0.0, 255.0) as u8)
                }
            };
            if swap_gb {
                d.swap(1, 2);
            }
        }
    });
    Ok(RgbaImage { width: ow, height: oh, pixels: out })
}

pub fn remap_format(format: ImageFormat) -> Option<EncodeFormat> {
    use ImageFormat as F;
    Some(match format {
        F::Dxt1 | F::Dxt1OneBitAlpha => EncodeFormat::Dxt1,
        F::Dxt3 | F::Dxt5 => EncodeFormat::Dxt5,
        F::Rgba8888 | F::Abgr8888 | F::Argb8888 | F::Bgra8888 | F::Bgra4444 | F::Bgra5551 | F::Ia88 | F::A8 => {
            EncodeFormat::Bgra8888
        }
        F::Rgb888
        | F::Bgr888
        | F::Bgrx8888
        | F::Rgb565
        | F::Bgr565
        | F::Bgrx5551
        | F::I8
        | F::P8
        | F::Ati2n
        | F::Ati1n => EncodeFormat::Bgr888,
        _ => return None,
    })
}

pub fn remap_vtf(data: &[u8], map: &TexelMap, kind: TexelKind) -> Result<Vec<u8>, VtfError> {
    let info = read_info(data)?;
    if info.is_cubemap() || info.depth > 1 {
        return Err(VtfError::InvalidImage("cubemaps and volume textures cannot be remapped".to_owned()));
    }
    let format = remap_format(info.format).ok_or(VtfError::UnsupportedFormat(info.format))?;
    let frames = (0..info.frames.max(1))
        .map(|frame| {
            let img = decode_rgba8(data, &DecodeOptions { mip: MipSelect::Largest, frame, face: 0 })?;
            remap_image(&img, map, kind)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let opts = EncodeOptions {
        format,
        mipmaps: info.mip_count > 1,
        flags: info.flags,
        version_minor: info.version.1.min(5),
        resize_to_pow2: false,
        quality: EncodeQuality::Normal,
        mip_filter: mip_filter_for(kind),
    };
    encode_frames(&frames, &opts)
}

pub fn mip_filter_for(kind: TexelKind) -> MipFilter {
    match kind {
        TexelKind::Color => MipFilter::FromFlags,
        TexelKind::Normal => MipFilter::Normal,
        TexelKind::SsBump => MipFilter::Data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags;

    fn img(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> RgbaImage {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                px.extend_from_slice(&f(x, y));
            }
        }
        RgbaImage::new(w, h, px).unwrap()
    }

    fn at(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width + x) * 4) as usize;
        [img.pixels[i], img.pixels[i + 1], img.pixels[i + 2], img.pixels[i + 3]]
    }

    fn normals_of(hgt: &[f32], w: u32, h: u32) -> RgbaImage {
        let get = |x: i64, y: i64| hgt[(y.rem_euclid(h as i64) * w as i64 + x.rem_euclid(w as i64)) as usize];
        img(w, h, |x, y| {
            let (x, y) = (i64::from(x), i64::from(y));
            let dx = (get(x + 1, y) - get(x - 1, y)) * 0.5;
            let dy = (get(x, y + 1) - get(x, y - 1)) * 0.5;
            let n = [-dx, -dy, 1.0];
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            let e = |v: f32| ((v / l * 0.5 + 0.5) * 255.0).round() as u8;
            [e(n[0]), e(n[1]), e(n[2]), 255]
        })
    }

    #[test]
    fn maps_from_transforms() {
        let m = TexelMap::from_affine([[0.0, -1.0], [1.0, 0.0]], [1.25, -0.5]).unwrap();
        assert_eq!(m.m, [[0, -1], [1, 0]]);
        assert_eq!(m.t, [0.25, 0.5]);
        assert_eq!(TexelMap::from_affine([[-2.0, 0.0], [0.0, 3.0]], [0.0, 0.0]).unwrap().m, [[-2, 0], [0, 3]]);
        let s = 45f64.to_radians().sin();
        assert_eq!(TexelMap::from_affine([[s, -s], [s, s]], [0.0, 0.0]), Err(Refusal::Angle));
        assert_eq!(TexelMap::from_affine([[0.5, 0.0], [0.0, 1.0]], [0.0, 0.0]), Err(Refusal::Fraction));
        assert_eq!(TexelMap::from_affine([[1.5, 0.0], [0.0, 1.0]], [0.0, 0.0]), Err(Refusal::Fraction));
        assert_eq!(TexelMap::from_affine([[64.0, 0.0], [0.0, 1.0]], [0.0, 0.0]), Err(Refusal::TooMany));
        assert!(TexelMap::from_affine([[1.0, 0.0], [0.0, 1.0]], [2.0, -1.0]).unwrap().is_identity());
        let ss = |m: [[i32; 2]; 2]| TexelMap { m, t: [0.0, 0.0] }.check(TexelKind::SsBump);
        assert_eq!(ss([[1, 0], [0, -1]]), Ok(()));
        assert_eq!(ss([[-1, 0], [0, 1]]), Err(Refusal::SsBump));
        assert_eq!(ss([[0, 1], [1, 0]]), Err(Refusal::SsBump));
    }

    #[test]
    fn whole_texel_shifts_wrap_exactly() {
        let src = img(8, 4, |x, y| [x as u8 * 10, y as u8 * 20, 7, 200]);
        let map = TexelMap::from_affine([[1.0, 0.0], [0.0, 1.0]], [3.0 / 8.0, -1.0 / 4.0]).unwrap();
        let out = remap_image(&src, &map, TexelKind::Color).unwrap();
        assert_eq!((out.width, out.height), (8, 4));
        for y in 0..4 {
            for x in 0..8 {
                assert_eq!(at(&out, x, y), at(&src, (x + 3) % 8, (y + 3) % 4), "{x} {y}");
            }
        }
        let half = TexelMap::from_affine([[1.0, 0.0], [0.0, 1.0]], [0.5 / 8.0, 0.0]).unwrap();
        let out = remap_image(&src, &half, TexelKind::Color).unwrap();
        assert_eq!(at(&out, 0, 0)[0], 5);
        assert_eq!(at(&out, 7, 0)[0], 35, "the last texel blends with the first one (wrap)");
    }

    #[test]
    fn repeats_tile_the_texture_in_the_same_size() {
        let src = img(8, 8, |x, y| [if x < 4 { 0 } else { 200 }, if y < 4 { 0 } else { 100 }, 0, 255]);
        let map = TexelMap::from_affine([[2.0, 0.0], [0.0, 2.0]], [0.0, 0.0]).unwrap();
        let out = remap_image(&src, &map, TexelKind::Color).unwrap();
        assert_eq!((out.width, out.height), (8, 8));
        for (x, y) in [(0, 0), (4, 0), (0, 4), (4, 4)] {
            assert_eq!(at(&out, x, y), [0, 0, 0, 255]);
            assert_eq!(at(&out, x + 3, y + 3), [200, 100, 0, 255]);
        }
    }

    #[test]
    fn quarter_turns_turn_pixels_and_normals() {
        let flat = [128, 128, 255, 255];
        let src = img(4, 4, |x, y| if (x, y) == (1, 0) { [255, 128, 128, 77] } else { flat });
        let map = TexelMap::from_affine([[0.0, 1.0], [-1.0, 0.0]], [0.0, 0.0]).unwrap();
        let out = remap_image(&src, &map, TexelKind::Normal).unwrap();
        assert_eq!(at(&out, 3, 1), [127, 255, 128, 77], "bytes exact, alpha kept");
        assert_eq!(at(&out, 1, 0), [127, 128, 255, 255]);
        let (w, h) = (16u32, 16u32);
        let hgt: Vec<f32> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f32, (i / w) as f32);
                3.0 * (x * 0.39).sin() + 2.0 * (y * 0.785 + x * 0.2).cos() + 0.05 * x * y
            })
            .collect();
        for m in [[[0, 1], [-1, 0]], [[0, -1], [1, 0]], [[-1, 0], [0, -1]], [[-1, 0], [0, 1]], [[1, 0], [0, -1]]] {
            let map = TexelMap { m, t: [0.0, 0.0] };
            let mf = m.map(|r| r.map(f64::from));
            let moved: Vec<f32> = (0..w * h)
                .map(|k| {
                    let (i, j) = (f64::from(k % w), f64::from(k / w));
                    let p = [(i + 0.5) / f64::from(w), (j + 0.5) / f64::from(h)];
                    let q = [mf[0][0] * p[0] + mf[0][1] * p[1], mf[1][0] * p[0] + mf[1][1] * p[1]];
                    let x = (q[0] * f64::from(w) - 0.5).round() as i64;
                    let y = (q[1] * f64::from(h) - 0.5).round() as i64;
                    hgt[(y.rem_euclid(i64::from(h)) * i64::from(w) + x.rem_euclid(i64::from(w))) as usize]
                })
                .collect();
            let want = normals_of(&moved, w, h);
            let got = remap_image(&normals_of(&hgt, w, h), &map, TexelKind::Normal).unwrap();
            for (a, b) in want.pixels.iter().zip(&got.pixels) {
                assert!(a.abs_diff(*b) <= 1, "{m:?}: {a} vs {b}");
            }
        }
    }

    #[test]
    fn ssbump_v_flip_swaps_green_and_blue() {
        let src = img(2, 2, |x, y| [10 + x as u8, 20 + y as u8, 30, 255]);
        let map = TexelMap::from_affine([[1.0, 0.0], [0.0, -1.0]], [0.0, 0.0]).unwrap();
        let out = remap_image(&src, &map, TexelKind::SsBump).unwrap();
        assert_eq!(at(&out, 1, 0), [11, 30, 21, 255]);
        assert!(remap_image(&src, &TexelMap { m: [[0, 1], [1, 0]], t: [0.0, 0.0] }, TexelKind::SsBump).is_err());
    }

    #[test]
    fn non_square_quarter_turn_swaps_sides() {
        let src = img(8, 4, |x, y| [x as u8, y as u8, 0, 255]);
        let map = TexelMap::from_affine([[0.0, 1.0], [1.0, 0.0]], [0.0, 0.0]).unwrap();
        let out = remap_image(&src, &map, TexelKind::Color).unwrap();
        assert_eq!((out.width, out.height), (4, 8));
        assert_eq!(at(&out, 2, 5), [5, 2, 0, 255]);
    }

    #[test]
    fn vtf_keeps_format_flags_and_mips() {
        let src = img(64, 64, |x, y| {
            let n = if x < 32 { [230, 128, 200, 255] } else { [128, 40, 220, 255] };
            [n[0], n[1], n[2], (y * 4) as u8]
        });
        let opts = EncodeOptions {
            format: EncodeFormat::Dxt5,
            flags: flags::NORMAL | flags::TRILINEAR,
            version_minor: 2,
            ..EncodeOptions::default()
        };
        let vtf = crate::encode(&src, &opts).unwrap();
        let map = TexelMap::from_affine([[-1.0, 0.0], [0.0, 1.0]], [0.0, 0.0]).unwrap();
        let out = remap_vtf(&vtf, &map, TexelKind::Normal).unwrap();
        let a = read_info(&vtf).unwrap();
        let b = read_info(&out).unwrap();
        assert_eq!(b.format, ImageFormat::Dxt5);
        assert_eq!((b.width, b.height, b.mip_count, b.version), (a.width, a.height, a.mip_count, a.version));
        assert_eq!(
            b.flags & !(flags::EIGHTBITALPHA | flags::ONEBITALPHA),
            a.flags & !(flags::EIGHTBITALPHA | flags::ONEBITALPHA)
        );
        let dec = decode_rgba8(&out, &DecodeOptions::default()).unwrap();
        let p = at(&dec, 40, 8);
        assert!(p[0] < 40, "{p:?}");
        assert!(p[3].abs_diff(32) <= 8, "alpha follows the pixels: {p:?}");
        assert_eq!(remap_format(ImageFormat::Rgba16161616F), None);
    }
}
