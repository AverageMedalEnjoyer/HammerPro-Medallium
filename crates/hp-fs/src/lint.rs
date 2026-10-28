use crate::deps::{DependencyGraph, PackPolicy};
use crate::fs::GameFs;

const GENERIC_FOLDERS: &[&str] = &[
    "custom",
    "textures",
    "texture",
    "mytextures",
    "my_textures",
    "new",
    "test",
    "tests",
    "temp",
    "tmp",
    "misc",
    "other",
    "stuff",
    "map",
    "maps_custom",
    "brick",
    "bricks",
    "wood",
    "metal",
    "concrete",
    "stone",
    "glass",
    "floor",
    "floors",
    "walls",
    "wall",
    "props",
    "prop",
    "decals",
    "sounds",
    "music",
    "ambient",
    "effects",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LintKind {
    GenericPath,
    Space,
    Clash { other: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LintIssue {
    pub vpath: String,
    pub kind: LintKind,
}

pub fn is_generic_path(vpath: &str) -> bool {
    let mut parts = vpath.split('/');
    let Some(root) = parts.next() else { return false };
    if !matches!(root, "materials" | "models" | "sound" | "particles" | "scripts") {
        return false;
    }
    let rest: Vec<&str> = parts.collect();
    match rest.as_slice() {
        [] | [_] => !matches!(root, "particles" | "scripts"),
        ["models", folder, ..] if root == "materials" => GENERIC_FOLDERS.contains(folder),
        [folder, ..] => GENERIC_FOLDERS.contains(folder),
    }
}

impl GameFs {
    pub fn lint_paths(&self, graph: &DependencyGraph, policy: PackPolicy) -> Vec<LintIssue> {
        let snapshot = self.snapshot();
        let mut out = Vec::new();
        for node in graph.shipped(policy).filter(|n| n.generated.is_none()) {
            let v = node.vpath.as_str();
            if v.starts_with("maps/") || v.starts_with("scripts/soundscapes_") {
                continue;
            }
            if v.contains(' ') {
                out.push(LintIssue { vpath: v.to_owned(), kind: LintKind::Space });
            }
            if is_generic_path(v) {
                out.push(LintIssue { vpath: v.to_owned(), kind: LintKind::GenericPath });
            }
            let other = snapshot
                .iter()
                .filter(|s| s.is_enabled() && Some(s.id) != node.source && s.contains(v))
                .map(|s| s.label.clone())
                .next();
            if let Some(other) = other {
                out.push(LintIssue { vpath: v.to_owned(), kind: LintKind::Clash { other } });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deps::{DepRoot, DepsCache, ResolveOptions, RootKind};
    use crate::test_util::{TempDir, vtf};
    use crate::{GameFs, MountOptions};

    #[test]
    fn generic_paths() {
        assert!(is_generic_path("materials/brick.vmt"));
        assert!(is_generic_path("materials/custom/brick.vmt"));
        assert!(is_generic_path("materials/models/props/crate.vmt"));
        assert!(is_generic_path("sound/hum.wav"));
        assert!(!is_generic_path("materials/gm_mymap/brick.vmt"));
        assert!(!is_generic_path("materials/models/gm_mymap/crate.vmt"));
        assert!(!is_generic_path("particles/fire.pcf"));
        assert!(!is_generic_path("lua/autorun/x.lua"));
    }

    #[test]
    fn clashes_with_other_sources() {
        let tmp = TempDir::new("lint");
        tmp.write(
            "gmod/garrysmod/gameinfo.txt",
            b"GameInfo { FileSystem { SearchPaths { game garrysmod game |all_source_engine_paths|sourceengine } } }",
        );
        tmp.write("gmod/garrysmod/materials/custom/a b.vmt", b"\"UnlitGeneric\" { }");
        tmp.write("gmod/garrysmod/addons/other/materials/custom/a b.vmt", b"\"UnlitGeneric\" { }");
        tmp.write("gmod/garrysmod/materials/gm_x/ok.vmt", b"\"UnlitGeneric\" { \"$basetexture\" \"gm_x/ok\" }");
        tmp.write("gmod/garrysmod/materials/gm_x/ok.vtf", &vtf(4, 4, [1, 1, 1, 255]));
        let opts = MountOptions { mount_workshop: false, mount_cfg: false, ..MountOptions::default() };
        let fs = GameFs::for_gmod(&tmp.path().join("gmod"), &opts).unwrap();
        let roots = ["custom/a b", "gm_x/ok"].map(|n| DepRoot {
            kind: RootKind::Material,
            name: n.to_owned(),
            why: String::new(),
        });
        let g = fs.resolve_dependencies(
            &roots,
            &ResolveOptions { map_name: "gm_x", ..ResolveOptions::default() },
            &DepsCache::new(),
        );
        let issues = fs.lint_paths(&g, PackPolicy::default());
        let kinds: Vec<&LintKind> =
            issues.iter().filter(|i| i.vpath == "materials/custom/a b.vmt").map(|i| &i.kind).collect();
        assert!(kinds.contains(&&LintKind::Space));
        assert!(kinds.contains(&&LintKind::GenericPath));
        assert!(kinds.iter().any(|k| matches!(k, LintKind::Clash { .. })));
        assert!(!issues.iter().any(|i| i.vpath.starts_with("materials/gm_x/")));
    }
}
