use glam::DVec3;

use crate::NumberStyle;

#[inline]
pub(crate) fn parse_f64(s: &str) -> Option<f64> {
    let v: f64 = s.trim().parse().ok()?;
    v.is_finite().then_some(v)
}

pub(crate) fn parse_i32(s: &str) -> Option<i32> {
    let t = s.trim();
    if let Ok(v) = t.parse::<i32>() {
        return Some(v);
    }
    parse_f64(t).map(|f| f as i32)
}

pub(crate) fn parse_u32(s: &str) -> Option<u32> {
    let t = s.trim();
    if let Ok(v) = t.parse::<u32>() {
        return Some(v);
    }
    parse_f64(t).filter(|f| *f >= 0.0).map(|f| f as u32)
}

pub(crate) fn parse_bool(s: &str) -> Option<bool> {
    let t = s.trim();
    match t {
        "1" => Some(true),
        "0" => Some(false),
        _ if t.eq_ignore_ascii_case("true") => Some(true),
        _ if t.eq_ignore_ascii_case("false") => Some(false),
        _ => parse_f64(t).map(|v| v != 0.0),
    }
}

#[inline]
pub(crate) fn num_tokens(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| c.is_ascii_whitespace() || matches!(c, '(' | ')' | '[' | ']')).filter(|t| !t.is_empty())
}

pub(crate) fn parse_fixed<const N: usize>(s: &str) -> Option<[f64; N]> {
    let mut out = [0.0; N];
    let mut it = num_tokens(s);
    for slot in &mut out {
        let v: f64 = it.next()?.parse().ok()?;
        if !v.is_finite() {
            return None;
        }
        *slot = v;
    }
    if it.next().is_some() {
        return None;
    }
    Some(out)
}

#[inline]
pub(crate) fn parse_vec3_strict(s: &str) -> Option<DVec3> {
    parse_fixed::<3>(s).map(DVec3::from_array)
}

pub(crate) fn parse_f32_prefix(s: &str) -> Option<f32> {
    let t = s.trim();
    if let Ok(v) = t.parse::<f32>() {
        return v.is_finite().then_some(v);
    }
    let end = t
        .char_indices()
        .find(|(_, c)| !matches!(c, '0'..='9' | '+' | '-' | '.' | 'e' | 'E'))
        .map_or(t.len(), |(i, _)| i);
    let mut prefix = t.get(..end)?;
    while !prefix.is_empty() {
        if let Ok(v) = prefix.parse::<f32>() {
            return v.is_finite().then_some(v);
        }
        prefix = prefix.get(..prefix.len() - 1)?;
    }
    None
}

pub(crate) fn parse_i32_prefix(s: &str) -> Option<i32> {
    let t = s.trim();
    if let Some(v) = parse_i32(t) {
        return Some(v);
    }
    let bytes = t.as_bytes();
    let sign_len = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let digits = bytes.iter().skip(sign_len).take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let head = t.get(..sign_len + digits)?;
    head.parse::<i64>().ok().map(|v| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
}

#[inline]
pub(crate) fn push_i64(out: &mut String, v: i64) {
    let mut buf = [0u8; 20];
    let mut n = v.unsigned_abs();
    let mut len = 0;
    for slot in buf.iter_mut().rev() {
        *slot = b'0' + (n % 10) as u8;
        n /= 10;
        len += 1;
        if n == 0 {
            break;
        }
    }
    if v < 0 {
        out.push('-');
    }
    if let Some(Ok(digits)) = buf.get(buf.len() - len..).map(std::str::from_utf8) {
        out.push_str(digits);
    }
}

#[inline]
pub(crate) fn push_f64(out: &mut String, v: f64) {
    if !v.is_finite() {
        out.push('0');
    } else if v.fract() == 0.0 && v.abs() < 1e15 {
        push_i64(out, v as i64);
    } else {
        hp_kv::fmt_num(out, v);
    }
}

#[inline]
pub(crate) fn push_f64_styled(out: &mut String, v: f64, style: NumberStyle) {
    if let NumberStyle::Exponent(digits) = style
        && v.is_finite()
        && v != 0.0
        && (v.abs() < 1e-4 || v.abs() >= 1e6)
    {
        use std::fmt::Write as _;
        let mut s = String::with_capacity(24);
        let _ = write!(s, "{v:e}");
        if let Some((mantissa, exp)) = s.split_once('e')
            && let Ok(x) = exp.parse::<i32>()
            && !(-4..6).contains(&x)
        {
            out.push_str(mantissa);
            out.push('e');
            out.push(if x < 0 { '-' } else { '+' });
            let mag = x.unsigned_abs();
            let len = mag.checked_ilog10().map_or(1, |l| l as usize + 1);
            for _ in len..usize::from(digits) {
                out.push('0');
            }
            push_i64(out, i64::from(mag));
            return;
        }
    }
    push_f64(out, v);
}

pub(crate) fn push_f32(out: &mut String, v: f32) {
    use std::fmt::Write as _;
    if !v.is_finite() || v == 0.0 {
        out.push('0');
    } else if v.fract() == 0.0 && v.abs() < 1e7 {
        push_i64(out, v as i64);
    } else {
        let _ = write!(out, "{v}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv_fmt(v: f64) -> String {
        let mut s = String::new();
        hp_kv::fmt_num(&mut s, v);
        s
    }

    fn fast_fmt(v: f64) -> String {
        let mut s = String::new();
        push_f64(&mut s, v);
        s
    }

    #[test]
    fn push_f64_matches_kv_fmt() {
        let values = [
            0.0,
            -0.0,
            1.0,
            -1.0,
            16.0,
            -1024.0,
            0.25,
            -0.5,
            1e14,
            -999_999_999_999_999.0,
            1e15,
            1e16,
            1e300,
            123.456,
            0.1 + 0.2,
            f64::MIN_POSITIVE,
            4_096.000_001,
            -65536.0,
        ];
        for v in values {
            assert_eq!(fast_fmt(v), kv_fmt(v), "value {v}");
        }
        let mut x: i64 = -2_147_483_648;
        while x < 2_147_483_647 {
            assert_eq!(fast_fmt(x as f64), kv_fmt(x as f64));
            x += 7_919_993;
        }
        assert_eq!(fast_fmt(f64::NAN), "0");
        assert_eq!(fast_fmt(f64::INFINITY), "0");
    }

    #[test]
    fn hammer_exponent_style() {
        let fmt = |v: f64, d: u8| {
            let mut s = String::new();
            push_f64_styled(&mut s, v, NumberStyle::Exponent(d));
            s
        };
        assert_eq!(fmt(-1.19209e-7, 3), "-1.19209e-007");
        assert_eq!(fmt(-1.19209e-7, 2), "-1.19209e-07");
        assert_eq!(fmt(1.5e-5, 3), "1.5e-005");
        assert_eq!(fmt(2e6, 3), "2e+006");
        assert_eq!(fmt(1.23457e6, 3), "1.23457e+006");
        assert_eq!(fmt(1e-120, 3), "1e-120");
        assert_eq!(fmt(0.0001, 3), "0.0001");
        assert_eq!(fmt(999_999.0, 3), "999999");
        assert_eq!(fmt(12.3457, 3), "12.3457");
        assert_eq!(fmt(0.0, 3), "0");
        assert_eq!(fmt(-0.0, 3), "0");
        for v in [-1.19209e-7, 3.4e-5, 7.25e8, -5e-300] {
            assert_eq!(fmt(v, 3).parse::<f64>().ok(), Some(v));
        }
        let mut s = String::new();
        push_f64_styled(&mut s, -1.19209e-7, NumberStyle::Plain);
        assert_eq!(s, "-0.000000119209");
    }

    #[test]
    fn push_i64_extremes() {
        for v in [0, 1, -1, 9, 10, -10, i64::MAX, i64::MIN, 1_234_567_890] {
            let mut s = String::new();
            push_i64(&mut s, v);
            assert_eq!(s, v.to_string());
        }
    }

    #[test]
    fn push_f32_shortest() {
        for (v, want) in [(0.0f32, "0"), (-0.0, "0"), (1.9, "1.9"), (0.1, "0.1"), (2.0, "2"), (-1.5, "-1.5")] {
            let mut s = String::new();
            push_f32(&mut s, v);
            assert_eq!(s, want);
            assert_eq!(s.parse::<f32>().ok(), Some(if v == 0.0 { 0.0 } else { v }));
        }
        let mut s = String::new();
        push_f32(&mut s, f32::NAN);
        assert_eq!(s, "0");
    }

    #[test]
    fn lenient_integers() {
        assert_eq!(parse_i32(" 16 "), Some(16));
        assert_eq!(parse_i32("16.7"), Some(16));
        assert_eq!(parse_i32("-3.9"), Some(-3));
        assert_eq!(parse_i32("1e20"), Some(i32::MAX));
        assert_eq!(parse_i32("abc"), None);
        assert_eq!(parse_u32("-1"), None);
        assert_eq!(parse_u32("4294967295"), Some(u32::MAX));
        assert_eq!(parse_u32("12.0"), Some(12));
        assert_eq!(parse_i32_prefix("-1"), Some(-1));
        assert_eq!(parse_i32_prefix("3x"), Some(3));
        assert_eq!(parse_i32_prefix("99999999999"), Some(i32::MAX));
        assert_eq!(parse_i32_prefix("x"), None);
    }

    #[test]
    fn lenient_floats() {
        assert_eq!(parse_f64("nan"), None);
        assert_eq!(parse_f64("inf"), None);
        assert_eq!(parse_f64(" 0.25 "), Some(0.25));
        assert_eq!(parse_f32_prefix("0.5s"), Some(0.5));
        assert_eq!(parse_f32_prefix("1e"), Some(1.0));
        assert_eq!(parse_f32_prefix("--"), None);
        assert_eq!(parse_f32_prefix(""), None);
        assert_eq!(parse_f32_prefix("é1"), None);
    }

    #[test]
    fn booleans() {
        assert_eq!(parse_bool("1"), Some(true));
        assert_eq!(parse_bool("0"), Some(false));
        assert_eq!(parse_bool("TRUE"), Some(true));
        assert_eq!(parse_bool("2"), Some(true));
        assert_eq!(parse_bool("0.0"), Some(false));
        assert_eq!(parse_bool("yes"), None);
    }

    #[test]
    fn fixed_numbers() {
        assert_eq!(parse_fixed::<9>("(0 0 0) (1 0 0) (1 1 0)"), Some([0., 0., 0., 1., 0., 0., 1., 1., 0.]));
        assert_eq!(parse_fixed::<5>("[1 0 0 -16.5] 0.25"), Some([1., 0., 0., -16.5, 0.25]));
        assert_eq!(parse_fixed::<3>("(1 2)"), None);
        assert_eq!(parse_fixed::<3>("1 2 3 4"), None);
        assert_eq!(parse_fixed::<3>("1 nan 3"), None);
        assert_eq!(parse_vec3_strict("[1 2 3]"), Some(DVec3::new(1., 2., 3.)));
    }
}
