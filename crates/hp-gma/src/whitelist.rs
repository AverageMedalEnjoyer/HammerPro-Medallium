pub const WHITELIST: &[&str] = &[
    "lua/*.lua",
    "scenes/*.vcd",
    "particles/*.pcf",
    "resource/fonts/*.ttf",
    "scripts/vehicles/*.txt",
    "resource/localization/*/*.properties",
    "maps/*.bsp",
    "maps/*.lmp",
    "maps/*.nav",
    "maps/*.ain",
    "maps/thumb/*.png",
    "sound/*.wav",
    "sound/*.mp3",
    "sound/*.ogg",
    "materials/*.vmt",
    "materials/*.vtf",
    "materials/*.png",
    "materials/*.jpg",
    "materials/*.jpeg",
    "materials/colorcorrection/*.raw",
    "models/*.mdl",
    "models/*.vtx",
    "models/*.phy",
    "models/*.ani",
    "models/*.vvd",
    "gamemodes/*/*.txt",
    "gamemodes/*/*.fgd",
    "gamemodes/*/logo.png",
    "gamemodes/*/icon24.png",
    "gamemodes/*/gamemode/*.lua",
    "gamemodes/*/entities/effects/*.lua",
    "gamemodes/*/entities/weapons/*.lua",
    "gamemodes/*/entities/entities/*.lua",
    "gamemodes/*/backgrounds/*.png",
    "gamemodes/*/backgrounds/*.jpg",
    "gamemodes/*/backgrounds/*.jpeg",
    "gamemodes/*/content/models/*.mdl",
    "gamemodes/*/content/models/*.vtx",
    "gamemodes/*/content/models/*.phy",
    "gamemodes/*/content/models/*.ani",
    "gamemodes/*/content/models/*.vvd",
    "gamemodes/*/content/materials/*.vmt",
    "gamemodes/*/content/materials/*.vtf",
    "gamemodes/*/content/materials/*.png",
    "gamemodes/*/content/materials/*.jpg",
    "gamemodes/*/content/materials/*.jpeg",
    "gamemodes/*/content/materials/colorcorrection/*.raw",
    "gamemodes/*/content/scenes/*.vcd",
    "gamemodes/*/content/particles/*.pcf",
    "gamemodes/*/content/resource/fonts/*.ttf",
    "gamemodes/*/content/scripts/vehicles/*.txt",
    "gamemodes/*/content/resource/localization/*/*.properties",
    "gamemodes/*/content/maps/*.bsp",
    "gamemodes/*/content/maps/*.nav",
    "gamemodes/*/content/maps/*.ain",
    "gamemodes/*/content/maps/thumb/*.png",
    "gamemodes/*/content/sound/*.wav",
    "gamemodes/*/content/sound/*.mp3",
    "gamemodes/*/content/sound/*.ogg",
    "data_static/*.txt",
    "data_static/*.dat",
    "data_static/*.json",
    "data_static/*.xml",
    "data_static/*.csv",
    "data_static/*.dem",
    "data_static/*.vcd",
    "data_static/*.vtf",
    "data_static/*.vmt",
    "data_static/*.png",
    "data_static/*.jpg",
    "data_static/*.jpeg",
    "data_static/*.mp3",
    "data_static/*.wav",
    "data_static/*.ogg",
    "shaders/fxc/*.vcs",
];

pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let (p, t) = (pattern.as_bytes(), text.as_bytes());
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && p[pi] == b'*' {
            star = Some((pi, ti));
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn is_allowed(path: &str) -> bool {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    WHITELIST.iter().any(|w| wildcard_match(w, &lower))
}

pub const BSP_WHITELIST: &[&str] = &[
    "materials/*.vmt",
    "materials/*.vtf",
    "materials/colorcorrection/*.raw",
    "models/*.mdl",
    "models/*.vvd",
    "models/*.vtx",
    "models/*.phy",
    "models/*.ani",
    "sound/*.wav",
    "sound/*.mp3",
    "sound/*.ogg",
    "particles/*.pcf",
    "maps/*.txt",
    "maps/*.nav",
    "maps/*.ain",
    "scripts/soundscapes_*.txt",
    "scripts/vehicles/*.txt",
    "data_static/*.txt",
];

pub fn bsp_allowed(path: &str) -> bool {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    BSP_WHITELIST.iter().any(|w| wildcard_match(w, &lower))
}

pub fn must_go_in_pakfile(path: &str) -> bool {
    !is_allowed(path) && bsp_allowed(path)
}

pub fn is_skipped(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".sw.vtx")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_matches_like_gmad() {
        for ok in [
            "maps/gm_x.bsp",
            "maps/thumb/gm_x.png",
            "materials/a/b/c.vmt",
            "Materials/A.VTF",
            "models/props/x.dx90.vtx",
            "sound/ambient/x.wav",
            "particles/fire.pcf",
            "materials/colorcorrection/x.raw",
            "gamemodes/ttt/content/maps/ttt_x.bsp",
            "lua/autorun/server/x.lua",
        ] {
            assert!(is_allowed(ok), "{ok}");
        }
        for bad in ["scripts/soundscapes_gm_x.txt", "maps/gm_x_particles.txt", "maps/gm_x_level_sounds.txt"] {
            assert!(!is_allowed(bad), "{bad}");
            assert!(must_go_in_pakfile(bad), "{bad}");
        }
        for nowhere in
            ["materials/x.psd", "addon.json", "materials/x.raw", "sound/x.wav.bak", "scripts/game_sounds_x.txt"]
        {
            assert!(!is_allowed(nowhere) && !bsp_allowed(nowhere), "{nowhere}");
        }
        for gma_only in ["maps/thumb/gm_x.png", "scenes/x.vcd", "resource/fonts/x.ttf", "materials/x.png"] {
            assert!(is_allowed(gma_only) && !bsp_allowed(gma_only), "{gma_only}");
        }
        assert!(bsp_allowed("Materials\\Models\\X.VMT"));
        assert!(bsp_allowed("maps/graphs/gm_x.ain"));
        assert!(is_skipped("models/a.sw.vtx"));
        assert!(!is_skipped("models/a.dx90.vtx"));
    }

    #[test]
    fn wildcard_edge_cases() {
        assert!(wildcard_match("*", ""));
        assert!(wildcard_match("a*b*c", "aXXbYYc"));
        assert!(!wildcard_match("a*b*c", "aXXbYY"));
        assert!(wildcard_match("**x", "yyx"));
        assert!(!wildcard_match("", "a"));
        let long = "a".repeat(10_000);
        assert!(!wildcard_match("*a*a*a*a*a*b", &long));
    }
}
