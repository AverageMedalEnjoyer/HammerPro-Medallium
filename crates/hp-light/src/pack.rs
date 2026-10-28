fn ufloat(v: f32, mantissa: u32) -> u32 {
    let max = (30u32 << mantissa) | ((1 << mantissa) - 1);
    if v.is_nan() || v <= 0.0 {
        return 0;
    }
    if v.is_infinite() {
        return max;
    }
    let bits = v.to_bits();
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mant = bits & 0x7f_ffff;
    let drop = 23 - mantissa;
    if exp <= 0 {
        let shift = (1 - exp) as u32 + drop;
        if shift > 24 {
            return 0;
        }
        let full = mant | 0x80_0000;
        let half = 1u32 << (shift - 1);
        let m = (full + half) >> shift;
        return m.min(max);
    }
    let half = 1u32 << (drop - 1);
    let mut m = (mant + half) >> drop;
    let mut e = exp as u32;
    if m >> mantissa != 0 {
        m = 0;
        e += 1;
    }
    if e >= 31 {
        return max;
    }
    (e << mantissa) | m
}

fn ufloat_decode(v: u32, mantissa: u32) -> f32 {
    let e = (v >> mantissa) & 0x1f;
    let m = v & ((1 << mantissa) - 1);
    let mf = m as f32 / (1u32 << mantissa) as f32;
    match e {
        0 => mf * 2f32.powi(-14),
        31 => f32::INFINITY,
        _ => (1.0 + mf) * 2f32.powi(e as i32 - 15),
    }
}

pub fn pack_rg11b10(rgb: [f32; 3]) -> u32 {
    ufloat(rgb[0], 6) | (ufloat(rgb[1], 6) << 11) | (ufloat(rgb[2], 5) << 22)
}

pub fn unpack_rg11b10(v: u32) -> [f32; 3] {
    [ufloat_decode(v & 0x7ff, 6), ufloat_decode((v >> 11) & 0x7ff, 6), ufloat_decode(v >> 22, 5)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for v in [0.0f32, 1.0, 0.5, 0.25, 2.0, 10.0, 0.001, 0.0001, 3.5, 100.0, 1000.0, 60000.0] {
            let p = pack_rg11b10([v, v, v]);
            let u = unpack_rg11b10(p);
            for (i, c) in u.iter().enumerate() {
                let tol = if i == 2 { 1.0 / 32.0 } else { 1.0 / 64.0 } * v.max(1e-4);
                assert!((c - v).abs() <= tol, "{v} -> {c} (channel {i})");
            }
        }
        assert_eq!(pack_rg11b10([1.0, 0.0, 0.0]) & 0x7ff, 15 << 6);
        assert_eq!(pack_rg11b10([0.0, 1.0, 0.0]) >> 11 & 0x7ff, 15 << 6);
        assert_eq!(pack_rg11b10([0.0, 0.0, 1.0]) >> 22, 15 << 5);
    }

    #[test]
    fn hostile_values() {
        assert_eq!(pack_rg11b10([-1.0, f32::NAN, f32::NEG_INFINITY]), 0);
        let big = unpack_rg11b10(pack_rg11b10([f32::INFINITY, 1e30, 70000.0]));
        assert!(big.iter().all(|c| c.is_finite() && *c > 60000.0), "{big:?}");
        let tiny = unpack_rg11b10(pack_rg11b10([1e-5, 1e-5, 1e-5]));
        assert!(tiny[0] > 0.0 && tiny[0] < 2e-5, "{tiny:?}");
        assert_eq!(unpack_rg11b10(pack_rg11b10([1e-12; 3])), [0.0; 3]);
    }
}
