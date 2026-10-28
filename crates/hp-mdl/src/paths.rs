pub fn sibling_paths(mdl_path: &str) -> (String, Vec<String>) {
    let base = mdl_path
        .len()
        .checked_sub(4)
        .filter(|&i| mdl_path.get(i..).is_some_and(|ext| ext.eq_ignore_ascii_case(".mdl")))
        .and_then(|i| mdl_path.get(..i))
        .unwrap_or(mdl_path);
    let vtx = [".dx90.vtx", ".dx80.vtx", ".sw.vtx", ".vtx"].iter().map(|ext| format!("{base}{ext}")).collect();
    (format!("{base}.vvd"), vtx)
}

pub fn normalize_material_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_slash = true;
    for ch in name.chars() {
        let c = if ch == '\\' { '/' } else { ch.to_ascii_lowercase() };
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
    while out.ends_with('/') {
        out.pop();
    }
    if out.starts_with("materials/") {
        out.drain(.."materials/".len());
    }
    let file_start = out.rfind('/').map_or(0, |i| i + 1);
    if let Some(dot) = out.get(file_start..).and_then(|file| file.rfind('.')).filter(|&dot| dot > 0) {
        out.truncate(file_start + dot);
    }
    out
}

pub(crate) fn compose_material(dir: &str, texture: &str) -> String {
    let texture = texture.trim_start_matches(['/', '\\']);
    let dir = dir.trim_start_matches(['/', '\\']);
    let mut joined = String::with_capacity(dir.len() + texture.len() + 1);
    joined.push_str(dir);
    if !dir.is_empty() && !dir.ends_with(['/', '\\']) {
        joined.push('/');
    }
    joined.push_str(texture);
    normalize_material_name(&joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn siblings() {
        let (vvd, vtx) = sibling_paths("C:\\gmod\\models\\Foo.MDL");
        assert_eq!(vvd, "C:\\gmod\\models\\Foo.vvd");
        assert_eq!(
            vtx,
            [
                "C:\\gmod\\models\\Foo.dx90.vtx",
                "C:\\gmod\\models\\Foo.dx80.vtx",
                "C:\\gmod\\models\\Foo.sw.vtx",
                "C:\\gmod\\models\\Foo.vtx"
            ]
        );
        let (vvd, _) = sibling_paths("noext");
        assert_eq!(vvd, "noext.vvd");
        let (vvd, _) = sibling_paths("é.mdl");
        assert_eq!(vvd, "é.vvd");
        let (vvd, _) = sibling_paths("ab");
        assert_eq!(vvd, "ab.vvd");
    }

    #[test]
    fn normalize() {
        assert_eq!(normalize_material_name("Materials/Models/Foo.vmt"), "models/foo");
        assert_eq!(normalize_material_name("models\\foo\\"), "models/foo");
        assert_eq!(normalize_material_name("a.b/c"), "a.b/c");
        assert_eq!(normalize_material_name("dir/.hidden"), "dir/.hidden");
        assert_eq!(normalize_material_name(""), "");
    }

    #[test]
    fn compose() {
        assert_eq!(compose_material("models\\props_c17\\", "Chair01a"), "models/props_c17/chair01a");
        assert_eq!(compose_material("/models/x", "\\tex"), "models/x/tex");
        assert_eq!(compose_material("", "models/full/path"), "models/full/path");
    }
}
