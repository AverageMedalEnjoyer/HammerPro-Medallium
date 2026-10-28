use crate::BcFormat;

pub(crate) type Block = [[u8; 4]; 16];

#[inline]
fn expand5(v: u16) -> u8 {
    let v = (v & 31) as u8;
    (v << 3) | (v >> 2)
}

#[inline]
fn expand6(v: u16) -> u8 {
    let v = (v & 63) as u8;
    (v << 2) | (v >> 4)
}

#[inline]
fn rgb565(c: u16) -> [u16; 3] {
    [u16::from(expand5(c >> 11)), u16::from(expand6(c >> 5)), u16::from(expand5(c))]
}

#[inline]
fn mix(a: [u16; 3], b: [u16; 3], wa: u16, wb: u16, div: u16) -> [u8; 4] {
    let f = |i: usize| ((a[i] * wa + b[i] * wb + div / 2) / div) as u8;
    [f(0), f(1), f(2), 255]
}

#[inline]
pub(crate) fn decode_color(bits: u64, bc1: bool, out: &mut Block) {
    let c0 = bits as u16;
    let c1 = (bits >> 16) as u16;
    let indices = (bits >> 32) as u32;
    let a = rgb565(c0);
    let b = rgb565(c1);
    let p0 = [a[0] as u8, a[1] as u8, a[2] as u8, 255];
    let p1 = [b[0] as u8, b[1] as u8, b[2] as u8, 255];
    let palette = if c0 > c1 || !bc1 {
        [p0, p1, mix(a, b, 2, 1, 3), mix(a, b, 1, 2, 3)]
    } else {
        [p0, p1, mix(a, b, 1, 1, 2), [0, 0, 0, 0]]
    };
    for (i, px) in out.iter_mut().enumerate() {
        *px = palette[((indices >> (2 * i)) & 3) as usize];
    }
}

#[inline]
pub(crate) fn decode_interpolated<const C: usize>(bits: u64, out: &mut Block) {
    let a0 = u32::from(bits as u8);
    let a1 = u32::from((bits >> 8) as u8);
    let mut palette = [0u8; 8];
    palette[0] = a0 as u8;
    palette[1] = a1 as u8;
    if a0 > a1 {
        for i in 1..7u32 {
            palette[i as usize + 1] = (((7 - i) * a0 + i * a1 + 3) / 7) as u8;
        }
    } else {
        for i in 1..5u32 {
            palette[i as usize + 1] = (((5 - i) * a0 + i * a1 + 2) / 5) as u8;
        }
        palette[6] = 0;
        palette[7] = 255;
    }
    let indices = bits >> 16;
    for (i, px) in out.iter_mut().enumerate() {
        px[C] = palette[((indices >> (3 * i)) & 7) as usize];
    }
}

#[inline]
pub(crate) fn decode_explicit_alpha(bits: u64, out: &mut Block) {
    for (i, px) in out.iter_mut().enumerate() {
        px[3] = (((bits >> (4 * i)) & 15) as u8) * 17;
    }
}

#[inline]
fn reconstruct_z(px: &mut [u8; 4]) {
    let x = f32::from(px[0]) / 127.5 - 1.0;
    let y = f32::from(px[1]) / 127.5 - 1.0;
    let z = (1.0 - x * x - y * y).max(0.0).sqrt();
    px[2] = (z * 127.5 + 127.5).round().clamp(0.0, 255.0) as u8;
    px[3] = 255;
}

#[inline]
pub(crate) fn decode_block(format: BcFormat, lo: u64, hi: u64, out: &mut Block) {
    match format {
        BcFormat::Bc1 => decode_color(lo, true, out),
        BcFormat::Bc2 => {
            decode_color(hi, false, out);
            decode_explicit_alpha(lo, out);
        }
        BcFormat::Bc3 => {
            decode_color(hi, false, out);
            decode_interpolated::<3>(lo, out);
        }
        BcFormat::Bc4 => {
            decode_interpolated::<0>(lo, out);
            for px in out.iter_mut() {
                *px = [px[0], px[0], px[0], 255];
            }
        }
        BcFormat::Bc5 => {
            decode_interpolated::<0>(lo, out);
            decode_interpolated::<1>(hi, out);
            for px in out.iter_mut() {
                reconstruct_z(px);
            }
        }
    }
}

pub(crate) fn decode_block_row(format: BcFormat, src: &[u8], width: usize, out: &mut [u8]) {
    let row_bytes = width * 4;
    if row_bytes == 0 {
        return;
    }
    let rows = (out.len() / row_bytes).min(4);
    let mut px: Block = [[0; 4]; 16];
    let mut emit = |bx: usize, px: &Block| {
        let x0 = bx * 4;
        let cols = width.saturating_sub(x0).min(4);
        let flat = px.as_flattened();
        for py in 0..rows {
            let dst_start = (py * width + x0) * 4;
            if let (Some(dst), Some(src)) =
                (out.get_mut(dst_start..dst_start + cols * 4), flat.get(py * 16..py * 16 + cols * 4))
            {
                dst.copy_from_slice(src);
            }
        }
    };
    if format.block_bytes() == 8 {
        for (bx, block) in src.as_chunks::<8>().0.iter().enumerate() {
            decode_block(format, u64::from_le_bytes(*block), 0, &mut px);
            emit(bx, &px);
        }
    } else {
        for (bx, block) in src.as_chunks::<16>().0.iter().enumerate() {
            let v = u128::from_le_bytes(*block);
            decode_block(format, v as u64, (v >> 64) as u64, &mut px);
            emit(bx, &px);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bc1_four_color_mode() {
        let mut bits = 0xF800u64 | (0x001Fu64 << 16);
        let idx: u32 = 0b11_10_01_00_11_10_01_00_11_10_01_00_11_10_01_00;
        bits |= u64::from(idx) << 32;
        let mut out = [[0; 4]; 16];
        decode_color(bits, true, &mut out);
        assert_eq!(out[0], [255, 0, 0, 255]);
        assert_eq!(out[1], [0, 0, 255, 255]);
        assert_eq!(out[2], [170, 0, 85, 255]);
        assert_eq!(out[3], [85, 0, 170, 255]);
    }

    #[test]
    fn bc1_three_color_mode_has_transparency() {
        let bits = 0x001Fu64 | (0xF800u64 << 16) | (0xFFFF_FFFFu64 << 32);
        let mut out = [[1; 4]; 16];
        decode_color(bits, true, &mut out);
        assert!(out.iter().all(|p| *p == [0, 0, 0, 0]));
        decode_color(bits, false, &mut out);
        assert!(out.iter().all(|p| p[3] == 255));
    }

    #[test]
    fn bc3_alpha_palettes() {
        let mut out = [[0; 4]; 16];
        let mut bits = 255u64;
        for i in 0..16 {
            bits |= 1u64 << (16 + 3 * i);
        }
        decode_interpolated::<3>(bits, &mut out);
        assert!(out.iter().all(|p| p[3] == 0));
        let bits = (10u64 << 8) | (7u64 << 16) | (6u64 << 19);
        decode_interpolated::<3>(bits, &mut out);
        assert_eq!(out[0][3], 255);
        assert_eq!(out[1][3], 0);
        assert_eq!(out[2][3], 0);
    }

    #[test]
    fn bc2_alpha() {
        let mut out = [[0; 4]; 16];
        decode_explicit_alpha(0xF0F0_F0F0_F0F0_F0F0, &mut out);
        assert_eq!(out[0][3], 0);
        assert_eq!(out[1][3], 255);
    }

    #[test]
    fn row_clips_partial_blocks() {
        let block = (0xF800u64 | (0xF800u64 << 16)).to_le_bytes();
        let mut out = vec![9u8; 2 * 2 * 4];
        decode_block_row(BcFormat::Bc1, &block, 2, &mut out);
        assert_eq!(out, [255, 0, 0, 255].repeat(4));
    }
}
