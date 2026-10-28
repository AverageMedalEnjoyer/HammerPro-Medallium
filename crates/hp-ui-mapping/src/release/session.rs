use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pass {
    Cubemaps { hdr: bool },
    Nav,
}

pub(crate) fn passes(ldr: bool, hdr: bool, cubemaps: bool, nav: bool) -> Vec<Pass> {
    let mut out = Vec::new();
    if cubemaps {
        if ldr || !hdr {
            out.push(Pass::Cubemaps { hdr: false });
        }
        if hdr {
            out.push(Pass::Cubemaps { hdr: true });
        }
    }
    if nav {
        out.push(Pass::Nav);
    }
    out
}

pub(crate) fn game_exe(tools: &hp_compile::ToolPaths) -> Option<PathBuf> {
    let root = tools.game_dir.parent()?;
    let bin = root.join("bin");
    [bin.join("win64").join("gmod.exe"), bin.join("gmod.exe")]
        .into_iter()
        .find(|p| p.is_file())
        .or_else(|| tools.game_exe.clone())
}

pub(crate) fn timeout(pass: Pass) -> std::time::Duration {
    std::time::Duration::from_secs(match pass {
        Pass::Cubemaps { .. } => 20 * 60,
        Pass::Nav => 45 * 60,
    })
}

pub(crate) fn args(pass: Pass, map: &str, game_folder: &str) -> Vec<String> {
    let mut a: Vec<String> =
        ["-steam", "-game", game_folder, "-novid", "-condebug", "-nojoy"].iter().map(|s| (*s).to_owned()).collect();
    match pass {
        Pass::Cubemaps { hdr } => {
            for s in
                ["+mat_specular", "0", "+mat_hdr_level", if hdr { "2" } else { "0" }, "-buildcubemaps", "+map", map]
            {
                a.push(s.to_owned());
            }
        }
        Pass::Nav => {
            for s in ["+map", map] {
                a.push(s.to_owned());
            }
        }
    }
    a
}

pub(crate) fn nav_helpers() -> Vec<hp_bsp::Entity> {
    let ent = |pairs: &[(&str, &str)]| hp_bsp::Entity {
        pairs: pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
    };
    vec![
        ent(&[
            ("classname", "lua_run"),
            ("targetname", "hammerpro_session"),
            ("Code", "RunConsoleCommand('nav_generate')"),
            ("origin", "0 0 0"),
        ]),
        ent(&[
            ("classname", "logic_auto"),
            ("spawnflags", "1"),
            ("OnMapSpawn", "hammerpro_session,RunCode,,2,-1"),
            ("origin", "0 0 0"),
        ]),
    ]
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Done,
    Failed(String),
}

pub(crate) fn scan(pass: Pass, text: &str) -> Option<Verdict> {
    for line in text.lines() {
        let l = line.to_ascii_lowercase();
        if l.contains("map load failed") || l.contains("couldn't load map") || l.contains("can't find map") {
            return Some(Verdict::Failed(line.trim().to_owned()));
        }
        if pass == Pass::Nav {
            if l.contains("navigation map") && l.contains("saved") {
                return Some(Verdict::Done);
            }
            if l.contains("cannot save navigation map") || l.contains("no walkable seeds") {
                return Some(Verdict::Failed(line.trim().to_owned()));
            }
        }
    }
    None
}

pub(crate) struct LogTail {
    path: PathBuf,
    offset: u64,
    partial: String,
}

impl LogTail {
    pub(crate) fn new(path: &Path) -> LogTail {
        let offset = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        LogTail { path: path.to_path_buf(), offset, partial: String::new() }
    }

    pub(crate) fn poll(&mut self) -> String {
        let Ok(mut f) = File::open(&self.path) else { return String::new() };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.offset {
            self.offset = 0;
        }
        if f.seek(SeekFrom::Start(self.offset)).is_err() {
            return String::new();
        }
        let mut buf = Vec::new();
        let n = f.take(1 << 20).read_to_end(&mut buf).unwrap_or(0);
        self.offset += n as u64;
        self.partial.push_str(&String::from_utf8_lossy(&buf));
        match self.partial.rfind('\n') {
            Some(end) => {
                let rest = self.partial.split_off(end + 1);
                std::mem::replace(&mut self.partial, rest)
            }
            None => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_and_arguments() {
        assert_eq!(
            passes(true, true, true, true),
            vec![Pass::Cubemaps { hdr: false }, Pass::Cubemaps { hdr: true }, Pass::Nav]
        );
        assert_eq!(passes(false, true, true, false), vec![Pass::Cubemaps { hdr: true }]);
        assert_eq!(passes(false, false, true, false), vec![Pass::Cubemaps { hdr: false }]);
        let a = args(Pass::Cubemaps { hdr: true }, "gm_x", "garrysmod");
        assert!(a.windows(2).any(|w| w == ["+mat_hdr_level", "2"]));
        assert!(a.contains(&"-buildcubemaps".to_owned()));
        let n = args(Pass::Nav, "gm_x", "garrysmod");
        assert!(n.windows(2).any(|w| w == ["+map", "gm_x"]));
        assert!(!n.iter().any(|a| a.contains("cheats")));
        let helpers = nav_helpers();
        assert_eq!(helpers[0].classname(), "lua_run");
        assert!(helpers.iter().all(|e| e.pairs.iter().all(|(k, v)| !k.contains('"') && !v.contains('"'))));
    }

    #[test]
    fn log_scanning_and_tail() {
        assert_eq!(scan(Pass::Nav, "x\nNavigation map 'maps/gm_x.nav' saved.\n"), Some(Verdict::Done));
        assert!(matches!(
            scan(Pass::Cubemaps { hdr: false }, "map load failed: gm_x not found\n"),
            Some(Verdict::Failed(_))
        ));
        assert_eq!(scan(Pass::Cubemaps { hdr: false }, "Navigation map 'x' saved."), None);
        let dir = crate::test_scratch("release_tail");
        let log = dir.join("console.log");
        std::fs::write(&log, "old line\n").unwrap();
        let mut tail = LogTail::new(&log);
        assert_eq!(tail.poll(), "");
        std::fs::write(&log, "old line\nnew one\npart").unwrap();
        assert_eq!(tail.poll(), "new one\n");
        std::fs::write(&log, "old line\nnew one\npartial done\n").unwrap();
        assert_eq!(tail.poll(), "partial done\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
