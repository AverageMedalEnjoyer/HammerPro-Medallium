use hp_kv::Block;

use crate::material::set_unique;
use crate::{Material, VmtError, parse};

pub const MAX_INCLUDE_DEPTH: usize = 8;

pub fn normalize_include_path(include: &str) -> String {
    let mut path = String::with_capacity(include.len() + 16);
    let mut prev_slash = false;
    for c in include.trim().chars() {
        let c = if c == '\\' { '/' } else { c.to_ascii_lowercase() };
        if c == '/' && prev_slash {
            continue;
        }
        prev_slash = c == '/';
        path.push(c);
    }
    let mut start = 0usize;
    loop {
        let rest = path.get(start..).unwrap_or("");
        if rest.starts_with("./") {
            start += 2;
        } else if rest.starts_with('/') {
            start += 1;
        } else {
            break;
        }
    }
    path.drain(..start.min(path.len()));
    if !path.starts_with("materials/") {
        path.insert_str(0, "materials/");
    }
    if !path.ends_with(".vmt") {
        path.push_str(".vmt");
    }
    path
}

pub fn resolve_patch(mat: &Material, load: &dyn Fn(&str) -> Option<String>) -> Result<Material, VmtError> {
    let mut visited = Vec::new();
    resolve_inner(mat, load, 0, &mut visited)
}

fn resolve_inner(
    mat: &Material,
    load: &dyn Fn(&str) -> Option<String>,
    depth: usize,
    visited: &mut Vec<String>,
) -> Result<Material, VmtError> {
    if !mat.is_patch() {
        return Ok(mat.clone());
    }
    let include = mat.get("include").map(str::trim).filter(|s| !s.is_empty()).ok_or(VmtError::PatchWithoutInclude)?;
    let path = normalize_include_path(include);
    if visited.contains(&path) {
        return Err(VmtError::IncludeCycle(path));
    }
    if depth >= MAX_INCLUDE_DEPTH {
        return Err(VmtError::IncludeTooDeep { path, max: MAX_INCLUDE_DEPTH });
    }
    let text = load(&path).ok_or_else(|| VmtError::IncludeNotFound(path.clone()))?;
    let base = parse(&text).map_err(|e| VmtError::Include { path: path.clone(), source: Box::new(e) })?;
    visited.push(path);
    let mut resolved = resolve_inner(&base, load, depth + 1, visited)?;
    apply_patch(&mut resolved, mat);
    Ok(resolved)
}

fn apply_patch(target: &mut Material, patch: &Material) {
    for block in patch.other_blocks.iter().filter(|b| b.name.eq_ignore_ascii_case("insert")) {
        insert_into(target, block, false);
    }
    for block in patch.other_blocks.iter().filter(|b| b.name.eq_ignore_ascii_case("replace")) {
        insert_into(target, block, true);
    }
}

fn insert_into(target: &mut Material, src: &Block, only_existing: bool) {
    for (k, v) in &src.props {
        if only_existing && !target.has(k) {
            continue;
        }
        set_unique(&mut target.params, k, v);
    }
    for child in &src.children {
        if child.name.eq_ignore_ascii_case("proxies") {
            match &mut target.proxies {
                Some(proxies) => merge_block(proxies, child, only_existing),
                None if !only_existing => target.proxies = Some(child.clone()),
                None => {}
            }
            continue;
        }
        match target.other_blocks.iter_mut().find(|b| b.name.eq_ignore_ascii_case(&child.name)) {
            Some(existing) => merge_block(existing, child, only_existing),
            None if !only_existing => target.other_blocks.push(child.clone()),
            None => {}
        }
    }
}

fn merge_block(dst: &mut Block, src: &Block, only_existing: bool) {
    for (k, v) in &src.props {
        if only_existing && dst.get(k).is_none() {
            continue;
        }
        set_unique(&mut dst.props, k, v);
    }
    for child in &src.children {
        match dst.child_mut(&child.name) {
            Some(existing) => merge_block(existing, child, only_existing),
            None if !only_existing => dst.children.push(child.clone()),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn loader(files: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = files.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        move |p: &str| map.get(p).cloned()
    }

    const BASE: &str = r#"
"LightmappedGeneric"
{
	"$basetexture" "concrete/floor01"
	"$surfaceprop" "concrete"
	"$envmap" "env_cubemap"
	"Proxies"
	{
		"Sine"
		{
			"sinemin" "0"
			"resultvar" "$alpha"
		}
	}
}
"#;

    #[test]
    fn include_path_normalization() {
        assert_eq!(normalize_include_path("Materials\\Concrete\\Floor01.VMT"), "materials/concrete/floor01.vmt");
        assert_eq!(normalize_include_path("concrete/floor01"), "materials/concrete/floor01.vmt");
        assert_eq!(normalize_include_path(" /materials//a.vmt "), "materials/a.vmt");
        assert_eq!(normalize_include_path("./materials/a.vmt"), "materials/a.vmt");
    }

    #[test]
    fn insert_and_replace() {
        let patch = parse(
            r#"patch
            {
                include "materials/CONCRETE/floor01.vmt"
                insert
                {
                    "$detail" "detail/noise"
                    "Proxies" { "Sine" { "sinemax" "1" } "TextureScroll" { "texturescrollvar" "$x" } }
                }
                replace
                {
                    "$envmap" "maps/test/c0_0_0"
                    "$notthere" "1"
                    "$SurfaceProp" "metal"
                }
            }"#,
        )
        .unwrap();
        let load = loader(&[("materials/concrete/floor01.vmt", BASE)]);
        let m = resolve_patch(&patch, &load).unwrap();
        assert_eq!(m.shader, "LightmappedGeneric");
        assert_eq!(m.get("$envmap"), Some("maps/test/c0_0_0"));
        assert_eq!(m.get("$surfaceprop"), Some("metal"));
        assert_eq!(m.get("$detail"), Some("detail/noise"));
        assert!(!m.has("$notthere"));
        assert!(m.params.iter().any(|(k, _)| k == "$surfaceprop"));
        let proxies = m.proxies.as_ref().unwrap();
        let sine = proxies.child("sine").unwrap();
        assert_eq!(sine.get("sinemin"), Some("0"));
        assert_eq!(sine.get("sinemax"), Some("1"));
        assert!(proxies.child("texturescroll").is_some());
        assert!(!m.is_patch());
    }

    #[test]
    fn chained_patches_outer_wins() {
        let inner = r#"patch { include "materials/concrete/floor01.vmt" insert { "$detail" "a" "$color" "[1 0 0]" } }"#;
        let outer = parse(r#"patch { include "materials/inner.vmt" insert { "$detail" "b" } }"#).unwrap();
        let load = loader(&[("materials/concrete/floor01.vmt", BASE), ("materials/inner.vmt", inner)]);
        let m = resolve_patch(&outer, &load).unwrap();
        assert_eq!(m.get("$detail"), Some("b"));
        assert_eq!(m.get("$color"), Some("[1 0 0]"));
        assert_eq!(m.get("$basetexture"), Some("concrete/floor01"));
    }

    #[test]
    fn replace_proxies_only_existing() {
        let base = "X { $a 1 }";
        let patch =
            parse(r#"patch { include "materials/x.vmt" replace { Proxies { Sine { } } Fallback { k v } } }"#).unwrap();
        let load = loader(&[("materials/x.vmt", base)]);
        let m = resolve_patch(&patch, &load).unwrap();
        assert!(m.proxies.is_none());
        assert!(m.other_blocks.is_empty());
        let patch = parse(r#"patch { include "materials/x.vmt" insert { Fallback { k v } } }"#).unwrap();
        let m = resolve_patch(&patch, &load).unwrap();
        assert_eq!(m.block("fallback").unwrap().get("k"), Some("v"));
    }

    #[test]
    fn fallback_sections_need_their_own_keys() {
        let base =
            r#""LightmappedGeneric" { "$basetexture" "a/b" "LightmappedGeneric_DX9" { "$bumpmap" "a/b_normal" } }"#;
        let load = loader(&[("materials/x.vmt", base)]);
        let top = parse(
            r#"patch { include "materials/x.vmt" insert { "$bumpmap" "v/baked" } replace { "$bumpmap" "v/baked" } }"#,
        )
        .unwrap();
        let m = resolve_patch(&top, &load).unwrap();
        assert_eq!(m.block("lightmappedgeneric_dx9").unwrap().get("$bumpmap"), Some("a/b_normal"));
        assert_eq!(m.resolve_fallbacks(crate::DEFAULT_DX_LEVEL, false).get("$bumpmap"), Some("a/b_normal"));
        let nested = parse(
            r#"patch { include "materials/x.vmt" insert { "$bumpmap" "v/baked" "LightmappedGeneric_DX9" { "$bumpmap" "v/baked" } } }"#,
        )
        .unwrap();
        let m = resolve_patch(&nested, &load).unwrap();
        assert_eq!(m.resolve_fallbacks(crate::DEFAULT_DX_LEVEL, false).get("$bumpmap"), Some("v/baked"));
    }

    #[test]
    fn non_patch_is_cloned() {
        let m = parse(BASE).unwrap();
        let load = loader(&[]);
        assert_eq!(resolve_patch(&m, &load).unwrap(), m);
    }

    #[test]
    fn errors() {
        let load = loader(&[
            ("materials/a.vmt", r#"patch { include "materials/b.vmt" }"#),
            ("materials/b.vmt", r#"patch { include "materials/a.vmt" }"#),
            ("materials/bad.vmt", "// nothing here"),
        ]);
        let missing = parse(r#"patch { insert { $a 1 } }"#).unwrap();
        assert!(matches!(resolve_patch(&missing, &load), Err(VmtError::PatchWithoutInclude)));
        let empty = parse(r#"patch { include "  " }"#).unwrap();
        assert!(matches!(resolve_patch(&empty, &load), Err(VmtError::PatchWithoutInclude)));
        let not_found = parse(r#"patch { include "materials/nope.vmt" }"#).unwrap();
        assert!(matches!(
            resolve_patch(&not_found, &load),
            Err(VmtError::IncludeNotFound(p)) if p == "materials/nope.vmt"
        ));
        let cycle = parse(r#"patch { include "materials/a.vmt" }"#).unwrap();
        assert!(matches!(resolve_patch(&cycle, &load), Err(VmtError::IncludeCycle(_))));
        let bad = parse(r#"patch { include "materials/bad.vmt" }"#).unwrap();
        let err = resolve_patch(&bad, &load).unwrap_err();
        assert!(matches!(&err, VmtError::Include { path, .. } if path == "materials/bad.vmt"));
        assert!(err.to_string().contains("materials/bad.vmt"));
    }

    #[test]
    fn depth_limit() {
        let mut files: Vec<(String, String)> = (1..=MAX_INCLUDE_DEPTH)
            .map(|i| (format!("materials/p{i}.vmt"), format!("patch {{ include \"materials/p{}.vmt\" }}", i + 1)))
            .collect();
        files.push((format!("materials/p{}.vmt", MAX_INCLUDE_DEPTH + 1), BASE.to_owned()));
        let map: HashMap<String, String> = files.into_iter().collect();
        let load = move |p: &str| map.get(p).cloned();

        let ok = parse("patch { include \"materials/p2.vmt\" }").unwrap();
        let resolved = resolve_patch(&ok, &load).unwrap();
        assert_eq!(resolved.get("$basetexture"), Some("concrete/floor01"));
        let too_deep = parse("patch { include \"materials/p1.vmt\" }").unwrap();
        assert!(matches!(
            resolve_patch(&too_deep, &load),
            Err(VmtError::IncludeTooDeep { max: MAX_INCLUDE_DEPTH, .. })
        ));
    }
}
