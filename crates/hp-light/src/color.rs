pub const LUT_SIZE: usize = 32;
pub const RAW_BYTES: usize = LUT_SIZE * LUT_SIZE * LUT_SIZE * 3;

pub fn is_raw_lut(bytes: &[u8]) -> bool {
    bytes.len() == RAW_BYTES
}

pub fn blend_luts(luts: &[(&[u8], f32)]) -> Option<Vec<u8>> {
    let valid: Vec<(&[u8], f32)> = luts
        .iter()
        .filter(|(b, w)| is_raw_lut(b) && w.is_finite() && *w > 0.0)
        .map(|(b, w)| (*b, w.clamp(0.0, 1.0)))
        .collect();
    if valid.is_empty() {
        return None;
    }
    let total: f32 = valid.iter().map(|(_, w)| *w).sum();
    let scale = if total > 1.0 { 1.0 / total } else { 1.0 };
    let identity = 1.0 - total.min(1.0);
    let n = LUT_SIZE * LUT_SIZE * LUT_SIZE;
    let mut out = Vec::with_capacity(n * 4);
    let step = 255.0 / (LUT_SIZE - 1) as f32;
    for i in 0..n {
        let (r, g, b) = (i % LUT_SIZE, (i / LUT_SIZE) % LUT_SIZE, i / (LUT_SIZE * LUT_SIZE));
        let mut c = [r as f32 * step * identity, g as f32 * step * identity, b as f32 * step * identity];
        for (bytes, w) in &valid {
            let w = w * scale;
            for (k, v) in c.iter_mut().enumerate() {
                *v += f32::from(bytes.get(i * 3 + k).copied().unwrap_or(0)) * w;
            }
        }
        out.extend(c.map(|v| v.round().clamp(0.0, 255.0) as u8));
        out.push(255);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_raw() -> Vec<u8> {
        let step = 255.0 / (LUT_SIZE - 1) as f32;
        let mut v = Vec::with_capacity(RAW_BYTES);
        for i in 0..LUT_SIZE * LUT_SIZE * LUT_SIZE {
            let (r, g, b) = (i % LUT_SIZE, (i / LUT_SIZE) % LUT_SIZE, i / (LUT_SIZE * LUT_SIZE));
            v.extend([r, g, b].map(|c| (c as f32 * step).round() as u8));
        }
        v
    }

    #[test]
    fn blending() {
        let id = identity_raw();
        let red = vec![255u8; RAW_BYTES]
            .iter()
            .enumerate()
            .map(|(i, _)| if i % 3 == 0 { 255 } else { 0 })
            .collect::<Vec<u8>>();
        assert!(blend_luts(&[]).is_none());
        assert!(blend_luts(&[(&id[..10], 1.0)]).is_none(), "wrong size");
        assert!(blend_luts(&[(&id, 0.0)]).is_none(), "no weight");
        let full = blend_luts(&[(&red, 1.0)]).unwrap();
        assert_eq!(full.len(), LUT_SIZE * LUT_SIZE * LUT_SIZE * 4);
        assert_eq!(&full[..4], &[255, 0, 0, 255]);
        let half = blend_luts(&[(&red, 0.5)]).unwrap();
        let last = half.len() - 4;
        assert_eq!(&half[last..], &[255, 128, 128, 255]);
        let both = blend_luts(&[(&red, 1.0), (&id, 1.0)]).unwrap();
        assert_eq!(&both[last..], &[255, 128, 128, 255]);
    }
}
