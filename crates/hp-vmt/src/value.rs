const KNOWN_EXTENSIONS: &[&str] =
    &["vtf", "vmt", "png", "tga", "jpg", "jpeg", "bmp", "psd", "dds", "hdr", "exr", "webp", "gif"];

pub fn parse_f32(s: &str) -> Option<f32> {
    let t = s.trim();
    if let Ok(v) = t.parse::<f32>() {
        return v.is_finite().then_some(v);
    }
    let end = numeric_prefix_len(t.as_bytes());
    if end == 0 {
        return None;
    }
    t.get(..end)?.parse::<f32>().ok().filter(|v| v.is_finite())
}

fn numeric_prefix_len(b: &[u8]) -> usize {
    let mut i = 0;
    if matches!(b.first(), Some(b'+' | b'-')) {
        i = 1;
    }
    let int_start = i;
    while b.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    let mut digits = i - int_start;
    if b.get(i) == Some(&b'.') {
        let frac_start = i + 1;
        let mut j = frac_start;
        while b.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        }
        digits += j - frac_start;
        i = j;
    }
    if digits == 0 {
        return 0;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(b.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        let exp_start = j;
        while b.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    i
}

pub fn parse_i32(s: &str) -> Option<i32> {
    parse_f32(s).map(|v| v as i32)
}

pub fn parse_bool(s: &str) -> Option<bool> {
    let t = s.trim();
    if t.eq_ignore_ascii_case("true") || t.eq_ignore_ascii_case("yes") {
        Some(true)
    } else if t.eq_ignore_ascii_case("false") || t.eq_ignore_ascii_case("no") {
        Some(false)
    } else {
        parse_f32(t).map(|v| v != 0.0)
    }
}

pub fn parse_vec3(s: &str) -> Option<[f32; 3]> {
    let t = s.trim();
    let (inner, divisor) = if let Some(rest) = t.strip_prefix('[') {
        (rest.strip_suffix(']').unwrap_or(rest), 1.0)
    } else if let Some(rest) = t.strip_prefix('{') {
        (rest.strip_suffix('}').unwrap_or(rest), 255.0)
    } else {
        (t, 1.0)
    };
    let mut out = [0.0f32; 3];
    let mut count = 0usize;
    for part in inner.split_ascii_whitespace() {
        let v = parse_f32(part)? / divisor;
        if let Some(slot) = out.get_mut(count) {
            *slot = v;
        }
        count += 1;
    }
    match count {
        1 => Some([out[0]; 3]),
        n if n >= 3 => Some(out),
        _ => None,
    }
}

pub fn normalize_texture_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut prev_slash = false;
    for c in path.trim().chars() {
        let c = if c == '\\' { '/' } else { c.to_ascii_lowercase() };
        if c == '/' {
            if prev_slash {
                continue;
            }
            prev_slash = true;
        } else {
            prev_slash = false;
        }
        out.push(c);
    }

    let mut start = 0usize;
    loop {
        let rest = out.get(start..).unwrap_or("");
        if rest.starts_with("./") {
            start += 2;
        } else if rest.starts_with('/') {
            start += 1;
        } else {
            break;
        }
    }
    if out.get(start..).is_some_and(|r| r.starts_with("materials/")) {
        start += "materials/".len();
    }

    let mut end = out.len();
    if let Some(rest) = out.get(start..)
        && let Some(dot) = rest.rfind('.')
    {
        let ext = rest.get(dot + 1..).unwrap_or("");
        if !ext.contains('/') && KNOWN_EXTENSIONS.contains(&ext) {
            end = start + dot;
        }
    }
    out.truncate(end);
    out.drain(..start.min(out.len()));
    out
}

pub fn normalize_material_name(name: &str) -> String {
    normalize_texture_path(name)
}

pub(crate) fn needs_quotes(value: &str) -> bool {
    value.is_empty()
        || value.starts_with('[')
        || value.contains("//")
        || value.chars().any(|c| c.is_whitespace() || matches!(c, '"' | '{' | '}'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats() {
        assert_eq!(parse_f32("1.5"), Some(1.5));
        assert_eq!(parse_f32(" .5 "), Some(0.5));
        assert_eq!(parse_f32("5."), Some(5.0));
        assert_eq!(parse_f32("-2e2"), Some(-200.0));
        assert_eq!(parse_f32("1.25f"), Some(1.25));
        assert_eq!(parse_f32("3e"), Some(3.0));
        assert_eq!(parse_f32("abc"), None);
        assert_eq!(parse_f32(""), None);
        assert_eq!(parse_f32("."), None);
        assert_eq!(parse_f32("-"), None);
        assert_eq!(parse_f32("inf"), None);
        assert_eq!(parse_f32("NaN"), None);
        assert_eq!(parse_f32("1e999"), None);
    }

    #[test]
    fn ints_and_bools() {
        assert_eq!(parse_i32("2"), Some(2));
        assert_eq!(parse_i32("2.9"), Some(2));
        assert_eq!(parse_i32("-7x"), Some(-7));
        assert_eq!(parse_i32("1e20"), Some(i32::MAX));
        assert_eq!(parse_bool("1"), Some(true));
        assert_eq!(parse_bool("0"), Some(false));
        assert_eq!(parse_bool("TRUE"), Some(true));
        assert_eq!(parse_bool(" false "), Some(false));
        assert_eq!(parse_bool("yes"), Some(true));
        assert_eq!(parse_bool("0.0"), Some(false));
        assert_eq!(parse_bool("2"), Some(true));
        assert_eq!(parse_bool("maybe"), None);
    }

    #[test]
    fn vectors() {
        assert_eq!(parse_vec3("[1 0.5 .25]"), Some([1.0, 0.5, 0.25]));
        assert_eq!(parse_vec3("{255 0 51}"), Some([1.0, 0.0, 0.2]));
        assert_eq!(parse_vec3("[ 1 2 3 ]"), Some([1.0, 2.0, 3.0]));
        assert_eq!(parse_vec3("1 2 3"), Some([1.0, 2.0, 3.0]));
        assert_eq!(parse_vec3("[0.5]"), Some([0.5; 3]));
        assert_eq!(parse_vec3("2"), Some([2.0; 3]));
        assert_eq!(parse_vec3("[1 2 3 4]"), Some([1.0, 2.0, 3.0]));
        assert_eq!(parse_vec3("[1 2"), None);
        assert_eq!(parse_vec3("[1 x 3]"), None);
        assert_eq!(parse_vec3(""), None);
        assert_eq!(parse_vec3("[]"), None);
    }

    #[test]
    fn paths() {
        assert_eq!(normalize_texture_path("Tools/toolsNodraw"), "tools/toolsnodraw");
        assert_eq!(normalize_texture_path(" materials\\Concrete//Floor01.VTF "), "concrete/floor01");
        assert_eq!(normalize_texture_path("/materials/a/b.vmt"), "a/b");
        assert_eq!(normalize_texture_path("./a/b.png"), "a/b");
        assert_eq!(normalize_texture_path("a/b.c/d"), "a/b.c/d");
        assert_eq!(normalize_texture_path("a/b.version2"), "a/b.version2");
        assert_eq!(normalize_texture_path("maps/x/materials/y"), "maps/x/materials/y");
        assert_eq!(normalize_texture_path(""), "");
        assert_eq!(normalize_texture_path("materials/"), "");
        assert_eq!(normalize_texture_path("D\u{e9}cor/X"), "d\u{e9}cor/x");
        assert_eq!(normalize_material_name("Materials/Tools/ToolsClip.vmt"), "tools/toolsclip");
    }

    #[test]
    fn quoting() {
        assert!(needs_quotes(""));
        assert!(needs_quotes("a b"));
        assert!(needs_quotes("[1 1 1]"));
        assert!(needs_quotes("{"));
        assert!(needs_quotes("a//b"));
        assert!(!needs_quotes("concrete/floor"));
        assert!(!needs_quotes("0.5"));
    }
}
