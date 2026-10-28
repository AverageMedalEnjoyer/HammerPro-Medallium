use egui::{IconData, Pos2};

const SIZE: u32 = 64;
const SAMPLES: u32 = 4;

const BACKGROUND: [f32; 3] = [10.0, 10.0, 10.0];
const BORDER: [f32; 3] = [58.0, 58.0, 58.0];
const MARK: [f32; 3] = [255.0, 255.0, 255.0];

fn in_rounded_square(x: f32, y: f32, inset: f32, radius: f32) -> bool {
    let (min, max) = (inset, 1.0 - inset);
    let r = (radius - inset).max(0.0);
    let cx = x.clamp(min + r, max - r);
    let cy = y.clamp(min + r, max - r);
    x >= min && x <= max && y >= min && y <= max && (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

fn in_convex(poly: &[Pos2], p: Pos2) -> bool {
    let mut sign = 0.0f32;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if cross != 0.0 {
            if sign != 0.0 && cross.signum() != sign {
                return false;
            }
            sign = cross.signum();
        }
    }
    true
}

pub fn icon_rgba(size: u32) -> Vec<u8> {
    let polys = hp_app::widgets::hammer_polygons();
    let size = size.max(1);
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    let radius = 0.22;
    let border = 1.2 / size.max(16) as f32;
    let n = (SAMPLES * SAMPLES) as f32;
    for py in 0..size {
        for px in 0..size {
            let (mut cover, mut mark, mut edge) = (0.0f32, 0.0f32, 0.0f32);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (px as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / size as f32;
                    if !in_rounded_square(x, y, 0.0, radius) {
                        continue;
                    }
                    cover += 1.0;
                    if polys.iter().any(|poly| in_convex(poly, Pos2::new(x, y))) {
                        mark += 1.0;
                    } else if !in_rounded_square(x, y, border, radius) {
                        edge += 1.0;
                    }
                }
            }
            let mut rgb = [0.0f32; 3];
            if cover > 0.0 {
                let (m, e) = (mark / cover, edge / cover);
                for i in 0..3 {
                    rgb[i] = MARK[i] * m + BORDER[i] * e + BACKGROUND[i] * (1.0 - m - e).max(0.0);
                }
            }
            let a = cover / n;
            rgba.extend_from_slice(&[
                rgb[0].round().clamp(0.0, 255.0) as u8,
                rgb[1].round().clamp(0.0, 255.0) as u8,
                rgb[2].round().clamp(0.0, 255.0) as u8,
                (a * 255.0).round().clamp(0.0, 255.0) as u8,
            ]);
        }
    }
    rgba
}

pub fn app_icon() -> IconData {
    IconData { rgba: icon_rgba(SIZE), width: SIZE, height: SIZE }
}

#[cfg(test)]
fn ico_bytes(sizes: &[u32]) -> Vec<u8> {
    let pngs: Vec<(u32, Vec<u8>)> =
        sizes.iter().map(|&s| (s, if s >= 256 { png_bytes(s) } else { dib_bytes(s) })).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(pngs.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * pngs.len() as u32;
    for (s, png) in &pngs {
        let side = if *s >= 256 { 0 } else { *s as u8 };
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in &pngs {
        out.extend_from_slice(png);
    }
    out
}

#[cfg(test)]
fn dib_bytes(size: u32) -> Vec<u8> {
    let rgba = icon_rgba(size);
    let mut out = Vec::new();
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&(2 * size as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    let mask_row = size.div_ceil(32) * 4;
    out.extend(std::iter::repeat_n(0u8, (mask_row * size) as usize));
    out
}

#[cfg(test)]
fn png_bytes(size: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_raw(size, size, icon_rgba(size)).expect("icon buffer size");
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).expect("png encoding");
    out.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(rgba: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    #[test]
    fn icon_is_black_and_white() {
        let icon = app_icon();
        assert_eq!(icon.rgba.len(), (SIZE * SIZE * 4) as usize);
        for px in icon.rgba.chunks(4) {
            assert!(px[0] == px[1] && px[1] == px[2], "gray only: {px:?}");
        }
        assert_eq!(pixel(&icon.rgba, SIZE, 0, 0)[3], 0);
        let edge = pixel(&icon.rgba, SIZE, 4, SIZE / 2);
        assert!(edge[3] == 255 && edge[0] < 30, "{edge:?}");
        let white = icon.rgba.chunks(4).filter(|p| p[0] > 240 && p[3] == 255).count();
        let share = white as f32 / (SIZE * SIZE) as f32;
        assert!((0.12..0.4).contains(&share), "mark covers {share}");
    }

    #[test]
    fn tiny_and_large_sizes_do_not_panic() {
        for s in [0, 1, 2, 16, 256] {
            let px = icon_rgba(s);
            assert_eq!(px.len(), (s.max(1) * s.max(1) * 4) as usize);
        }
    }

    #[test]
    fn assets_hold_the_procedural_icon() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let png_path = dir.join("hammer-pro.png");
        let ico_path = dir.join("hammer-pro.ico");
        if std::env::var_os("HP_WRITE_ICON").is_some() {
            std::fs::write(&png_path, png_bytes(256)).expect("write png");
            std::fs::write(&ico_path, ico_bytes(&[16, 24, 32, 48, 64, 128, 256])).expect("write ico");
        }
        let img = image::open(&png_path).expect("assets/hammer-pro.png").to_rgba8();
        assert_eq!(img.dimensions(), (256, 256));
        let fresh = icon_rgba(256);
        let worst = img.as_raw().iter().zip(&fresh).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
        assert!(worst <= 2, "assets/hammer-pro.png is out of date (HP_WRITE_ICON=1 cargo test)");
        let ico = std::fs::read(&ico_path).expect("assets/hammer-pro.ico");
        assert_eq!(&ico[..4], &[0, 0, 1, 0]);
        assert_eq!(u16::from_le_bytes([ico[4], ico[5]]), 7);
    }
}
