use std::borrow::Cow;
use std::path::{Path, PathBuf};

pub fn normalize_path(path: &str) -> Cow<'_, str> {
    hp_vpk::normalize_path(path)
}

pub(crate) fn extension(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rfind('.') {
        Some(dot) => name.get(dot + 1..).unwrap_or(""),
        None => "",
    }
}

pub(crate) fn strip_extension(path: &str) -> &str {
    let ext = extension(path);
    if ext.is_empty() && !path.ends_with('.') {
        return path;
    }
    path.get(..path.len() - ext.len() - 1).unwrap_or(path)
}

const RESERVED_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2",
    "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9", "conin$", "conout$",
];

pub(crate) fn is_safe_component(component: &str) -> bool {
    if component.is_empty() || component == "." || component == ".." || component.len() > 255 {
        return false;
    }
    if component.chars().any(|c| c.is_control() || matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')) {
        return false;
    }
    if component.ends_with('.') || component.ends_with(' ') {
        return false;
    }
    let stem = component.split('.').next().unwrap_or(component).trim_end();
    !RESERVED_NAMES.iter().any(|r| stem.eq_ignore_ascii_case(r))
}

pub(crate) fn safe_join(base: &Path, vpath: &str) -> Option<PathBuf> {
    if vpath.is_empty() || vpath.starts_with('/') {
        return None;
    }
    let mut out = base.to_path_buf();
    for component in vpath.split('/') {
        if !is_safe_component(component) {
            return None;
        }
        out.push(component);
    }
    Some(out)
}

pub(crate) fn join_virtual(prefix: &str, rel: &str) -> String {
    if prefix.is_empty() {
        rel.to_owned()
    } else {
        let mut s = String::with_capacity(prefix.len() + 1 + rel.len());
        s.push_str(prefix);
        s.push('/');
        s.push_str(rel);
        s
    }
}

pub(crate) fn normalize_prefix(prefix: &str) -> String {
    let normalized = normalize_path(prefix.trim());
    let trimmed = normalized.trim_matches('/');
    let trimmed = trimmed.strip_prefix("materials/").unwrap_or(trimmed);
    let trimmed = if trimmed == "materials" { "" } else { trimmed };
    trimmed.trim_matches('/').to_owned()
}

#[inline]
pub(crate) fn under_prefix(path: &str, prefix: &str) -> bool {
    prefix.is_empty()
        || (path.len() > prefix.len() && path.starts_with(prefix) && path.as_bytes().get(prefix.len()) == Some(&b'/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions() {
        assert_eq!(extension("materials/a.b/c.vmt"), "vmt");
        assert_eq!(extension("maps/readme"), "");
        assert_eq!(extension("a.dx90.vtx"), "vtx");
        assert_eq!(strip_extension("materials/a.b/c.vmt"), "materials/a.b/c");
        assert_eq!(strip_extension("maps/readme"), "maps/readme");
        assert_eq!(strip_extension("x."), "x");
    }

    #[test]
    fn safe_join_rejects_escapes() {
        let base = Path::new("base");
        assert_eq!(safe_join(base, "materials/a.vmt"), Some(base.join("materials").join("a.vmt")));
        assert_eq!(safe_join(base, "../evil.vmt"), None);
        assert_eq!(safe_join(base, "materials/../../evil.vmt"), None);
        assert_eq!(safe_join(base, "/abs.vmt"), None);
        assert_eq!(safe_join(base, "c:/abs.vmt"), None);
        assert_eq!(safe_join(base, "materials//a.vmt"), None);
        assert_eq!(safe_join(base, "materials/con.vtf"), None);
        assert_eq!(safe_join(base, "materials/Com1.txt"), None);
        assert_eq!(safe_join(base, "materials/trailing./a"), None);
        assert_eq!(safe_join(base, ""), None);
        assert!(safe_join(base, "materials/console/x.vtf").is_some());
    }

    #[test]
    fn prefixes() {
        assert_eq!(normalize_prefix(" Custom\\Bricks/ "), "custom/bricks");
        assert_eq!(normalize_prefix("/materials/custom/"), "custom");
        assert_eq!(normalize_prefix("materials"), "");
        assert!(under_prefix("materials/a.vmt", "materials"));
        assert!(!under_prefix("materialsx/a.vmt", "materials"));
        assert!(!under_prefix("materials", "materials"));
        assert!(under_prefix("anything", ""));
        assert_eq!(join_virtual("", "a"), "a");
        assert_eq!(join_virtual("p", "a"), "p/a");
    }
}
