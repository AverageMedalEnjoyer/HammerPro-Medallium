use std::borrow::Cow;

#[inline]
fn char_len(s: &[u8], i: usize) -> usize {
    match s.get(i) {
        Some(&b) if b >= 0xF0 => 4,
        Some(&b) if b >= 0xE0 => 3,
        Some(&b) if b >= 0xC0 => 2,
        _ => 1,
    }
}

fn match_captures(pattern: &str, text: &str) -> Option<Vec<(usize, usize)>> {
    let p = pattern.as_bytes();
    let t = text.as_bytes();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    let mut caps: Vec<(usize, usize)> = Vec::new();
    while ti < t.len() {
        match p.get(pi) {
            Some(b'*') => {
                caps.push((ti, ti));
                star = Some((pi, ti));
                pi += 1;
            }
            Some(b'?') => {
                pi += 1;
                ti += char_len(t, ti);
            }
            Some(&pc) if t.get(ti).is_some_and(|&tc| tc.eq_ignore_ascii_case(&pc)) => {
                pi += 1;
                ti += 1;
            }
            _ => {
                let (spi, sti) = star?;
                let next = sti + char_len(t, sti);
                star = Some((spi, next));
                if let Some(last) = caps.last_mut() {
                    last.1 = next;
                }
                pi = spi + 1;
                ti = next;
            }
        }
    }
    while p.get(pi) == Some(&b'*') {
        caps.push((ti, ti));
        pi += 1;
    }
    (pi == p.len()).then_some(caps)
}

pub fn glob_match(pattern: &str, text: &str) -> bool {
    if !pattern.contains(['*', '?']) {
        return pattern.eq_ignore_ascii_case(text);
    }
    match_captures(pattern, text).is_some()
}

pub fn glob_replace(pattern: &str, replacement: &str, text: &str) -> Option<String> {
    let caps = if pattern.contains(['*', '?']) {
        match_captures(pattern, text)?
    } else if pattern.eq_ignore_ascii_case(text) {
        Vec::new()
    } else {
        return None;
    };
    if !replacement.contains('*') {
        return Some(replacement.to_owned());
    }
    let mut out = String::with_capacity(replacement.len() + text.len());
    let mut caps = caps.into_iter();
    for (i, part) in replacement.split('*').enumerate() {
        if i > 0
            && let Some((a, b)) = caps.next()
        {
            out.push_str(text.get(a..b).unwrap_or(""));
        }
        out.push_str(part);
    }
    Some(out)
}

pub(crate) fn normalize_material(name: &str) -> Cow<'_, str> {
    let mut s: Cow<'_, str> =
        if name.contains('\\') { Cow::Owned(name.replace('\\', "/")) } else { Cow::Borrowed(name) };
    let trimmed = s.trim();
    if trimmed.len() != s.len() {
        s = Cow::Owned(trimmed.to_owned());
    }
    let lower_prefix = s.get(..10).is_some_and(|p| p.eq_ignore_ascii_case("materials/"));
    if lower_prefix {
        s = Cow::Owned(s.get(10..).unwrap_or("").to_owned());
    }
    let n = s.len();
    if n >= 4 && s.get(n - 4..).is_some_and(|e| e.eq_ignore_ascii_case(".vmt")) {
        s = Cow::Owned(s.get(..n - 4).unwrap_or("").to_owned());
    }
    s
}

pub(crate) fn material_match(pattern: &str, material: &str) -> bool {
    glob_match(&normalize_material(pattern), &normalize_material(material))
}

pub(crate) fn same_material(a: &str, b: &str) -> bool {
    normalize_material(a).eq_ignore_ascii_case(&normalize_material(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching() {
        assert!(glob_match("DEV/*", "dev/dev_measuregeneric01"));
        assert!(glob_match("*generic*", "DEV/DEV_MEASUREGENERIC01"));
        assert!(glob_match("tools/tools?rigger", "TOOLS/TOOLSTRIGGER"));
        assert!(!glob_match("tools/*", "dev/floor"));
        assert!(glob_match("*", ""));
        assert!(glob_match("a*b*c", "aXXbYYc"));
        assert!(!glob_match("a*b*c", "aXXbYY"));
        assert!(glob_match("exact", "EXACT"));
        assert!(!glob_match("exact", "exactly"));
        assert!(glob_match("?\u{e9}t\u{e9}", "\u{e9}\u{e9}t\u{e9}"));
        assert!(glob_match("**a", "bba"));
    }

    #[test]
    fn replacing() {
        assert_eq!(glob_replace("dev/*", "tools/*", "DEV/Floor01").as_deref(), Some("tools/Floor01"));
        assert_eq!(glob_replace("*/wall*", "*/brick*", "concrete/wall01a").as_deref(), Some("concrete/brick01a"));
        assert_eq!(glob_replace("dev/*", "tools/toolsnodraw", "dev/x").as_deref(), Some("tools/toolsnodraw"));
        assert_eq!(glob_replace("dev/*", "x", "tools/x"), None);
        assert_eq!(glob_replace("Same", "other", "same").as_deref(), Some("other"));
    }

    #[test]
    fn materials() {
        assert_eq!(normalize_material("materials\\Dev\\Floor.vmt"), "Dev/Floor");
        assert!(material_match("dev/floor", "MATERIALS/DEV/FLOOR.VMT"));
        assert!(same_material("tools/toolsnodraw", "TOOLS\\TOOLSNODRAW"));
        assert!(!same_material("tools/toolsnodraw", "tools/toolsclip"));
    }
}
