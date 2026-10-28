use crate::error::{FileKind, MdlError};
use crate::mdl::{
    FLAG_OBSOLETE, H_NUM_LOCAL_SEQ, SEQDESC_SIZE, parse_cd_textures, parse_header, parse_include_models, parse_skins,
    parse_textures,
};
use crate::paths::compose_material;
use crate::reader::Reader;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MdlDependencies {
    pub textures: Vec<String>,
    pub material_dirs: Vec<String>,
    pub include_models: Vec<String>,
    pub skin_families: Vec<Vec<u16>>,
    pub flags: u32,
    pub sounds: Vec<String>,
}

impl MdlDependencies {
    pub fn used_textures(&self) -> Vec<usize> {
        let mut out: Vec<usize> = if self.skin_families.is_empty() {
            (0..self.textures.len()).collect()
        } else {
            self.skin_families.iter().flatten().map(|&i| usize::from(i)).filter(|&i| i < self.textures.len()).collect()
        };
        out.sort_unstable();
        out.dedup();
        out
    }

    pub fn material_candidates(&self) -> Vec<Vec<String>> {
        if self.flags & FLAG_OBSOLETE != 0 {
            return vec![vec!["models/obsolete/obsolete".to_owned()]];
        }
        self.used_textures()
            .into_iter()
            .map(|t| {
                let texture = &self.textures[t];
                let mut names: Vec<String> = Vec::with_capacity(self.material_dirs.len().max(1));
                if self.material_dirs.is_empty() {
                    names.push(compose_material("", texture));
                }
                for dir in &self.material_dirs {
                    let name = compose_material(dir, texture);
                    if !name.is_empty() && !names.contains(&name) {
                        names.push(name);
                    }
                }
                names
            })
            .filter(|n| !n.is_empty())
            .collect()
    }
}

const MAX_GIBS: usize = 256;

pub fn gib_models(phy: &[u8]) -> Vec<String> {
    let text_start = || -> Option<usize> {
        let int = |at: usize| -> Option<usize> {
            let b = phy.get(at..at + 4)?;
            usize::try_from(i32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok()
        };
        let mut pos = int(0)?;
        let count = int(8)?.min(4096);
        for _ in 0..count {
            pos = pos.checked_add(4)?.checked_add(int(pos)?)?;
        }
        (pos <= phy.len()).then_some(pos)
    };
    let text = String::from_utf8_lossy(&phy[text_start().unwrap_or(0)..]);
    let mut out: Vec<String> = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;
    let mut key: Option<String> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let token = match c {
            '{' => {
                stack.push(pending.take().or_else(|| key.take()).unwrap_or_default().to_ascii_lowercase());
                key = None;
                continue;
            }
            '}' => {
                stack.pop();
                (pending, key) = (None, None);
                continue;
            }
            '"' => {
                let mut end = text.len();
                for (j, d) in chars.by_ref() {
                    if d == '"' {
                        end = j;
                        break;
                    }
                }
                text[i + 1..end.max(i + 1)].to_owned()
            }
            c if c.is_whitespace() || c.is_control() => continue,
            _ => {
                let mut end = text.len();
                while let Some(&(j, d)) = chars.peek() {
                    if d.is_whitespace() || d.is_control() || matches!(d, '{' | '}' | '"') {
                        end = j;
                        break;
                    }
                    chars.next();
                }
                text[i..end].to_owned()
            }
        };
        match key.take() {
            Some(k) => {
                if k.eq_ignore_ascii_case("model")
                    && stack.last().is_some_and(|b| b == "break")
                    && out.len() < MAX_GIBS
                    && let Some(p) = include_path(&token)
                {
                    out.push(p);
                }
                pending = None;
            }
            None => {
                key = Some(token.clone());
                pending = Some(token);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

pub(crate) fn include_path(name: &str) -> Option<String> {
    let mut p = name.trim().replace('\\', "/").to_ascii_lowercase();
    while p.starts_with('/') {
        p.remove(0);
    }
    if p.is_empty() {
        return None;
    }
    if !p.starts_with("models/") {
        p.insert_str(0, "models/");
    }
    if !p.ends_with(".mdl") {
        p.push_str(".mdl");
    }
    Some(p)
}

pub fn read_dependencies(mdl: &[u8]) -> Result<MdlDependencies, MdlError> {
    let info = parse_header(mdl)?;
    let r = Reader::new(mdl, FileKind::Mdl);
    let textures = parse_textures(&r)?;
    let material_dirs = parse_cd_textures(&r)?;
    let skin_families = parse_skins(&r).unwrap_or_default();
    let mut include_models: Vec<String> = parse_include_models(&r)?.iter().filter_map(|n| include_path(n)).collect();
    include_models.dedup();
    let sounds = sound_events(&r);
    Ok(MdlDependencies { textures, material_dirs, include_models, skin_families, flags: info.flags, sounds })
}

const EVENT_SIZE: usize = 80;
const MAX_EVENTS: usize = 8192;
const SOUND_EVENTS: &[i32] = &[1004, 1008, 5004];
const SOUND_EVENT_NAMES: &[&str] = &["AE_CL_PLAYSOUND", "AE_SV_PLAYSOUND"];

fn sound_events(r: &Reader<'_>) -> Vec<String> {
    let mut out = Vec::new();
    let read = |out: &mut Vec<String>| -> crate::reader::Result<()> {
        let (start, n) =
            r.rel_table(0, r.i32(H_NUM_LOCAL_SEQ + 4)?, r.i32(H_NUM_LOCAL_SEQ)?, SEQDESC_SIZE, "sequence table")?;
        let mut seen = 0;
        for i in 0..n {
            let s = start + i * SEQDESC_SIZE;
            let (events, count) = r.rel_table(s, r.i32(s + 28)?, r.i32(s + 24)?, EVENT_SIZE, "event table")?;
            for j in 0..count {
                seen += 1;
                if seen > MAX_EVENTS {
                    return Ok(());
                }
                let e = events + j * EVENT_SIZE;
                let id = r.i32(e + 4)?;
                let named = match r.i32(e + 76)? {
                    0 => false,
                    rel => r
                        .rel(e, rel, "event name")
                        .and_then(|at| r.cstr(at, "event name"))
                        .is_ok_and(|name| SOUND_EVENT_NAMES.iter().any(|n| n.eq_ignore_ascii_case(name.trim()))),
                };
                if !named && !SOUND_EVENTS.contains(&id) {
                    continue;
                }
                let options = r.fixed_str(e + 12, 64, "event options")?;
                if let Some(sound) = options.split_whitespace().next().filter(|s| s.len() > 1) {
                    out.push(sound.to_owned());
                }
            }
        }
        Ok(())
    };
    let _ = read(&mut out);
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(b: &mut Vec<u8>, at: usize, bytes: &[u8]) {
        if b.len() < at + bytes.len() {
            b.resize(at + bytes.len(), 0);
        }
        b[at..at + bytes.len()].copy_from_slice(bytes);
    }

    fn i32_at(b: &mut Vec<u8>, at: usize, v: i32) {
        put(b, at, &v.to_le_bytes());
    }

    fn sample() -> Vec<u8> {
        let mut b = Vec::new();
        put(&mut b, 0, b"IDST");
        i32_at(&mut b, 4, 48);
        put(&mut b, 12, b"test/box.mdl\0");
        put(&mut b, 408, &[0; 600]);
        i32_at(&mut b, 204, 2);
        i32_at(&mut b, 208, 500);
        i32_at(&mut b, 500, 300);
        put(&mut b, 800, b"Wood\0");
        i32_at(&mut b, 564, 250);
        put(&mut b, 814, b"wood_burnt\0");
        i32_at(&mut b, 212, 2);
        i32_at(&mut b, 216, 640);
        i32_at(&mut b, 640, 840);
        i32_at(&mut b, 644, 860);
        put(&mut b, 840, b"models\\props\\\0");
        put(&mut b, 860, b"\0");
        i32_at(&mut b, 220, 1);
        i32_at(&mut b, 224, 2);
        i32_at(&mut b, 228, 700);
        put(&mut b, 700, &0u16.to_le_bytes());
        put(&mut b, 702, &1u16.to_le_bytes());
        i32_at(&mut b, 336, 1);
        i32_at(&mut b, 340, 720);
        i32_at(&mut b, 724, 160);
        put(&mut b, 880, b"Humans\\Male_Shared.mdl\0");
        i32_at(&mut b, 188, 1);
        i32_at(&mut b, 192, 1000);
        i32_at(&mut b, 1000, -1000);
        i32_at(&mut b, 1024, 3);
        i32_at(&mut b, 1028, 300);
        i32_at(&mut b, 1304, 1004);
        put(&mut b, 1312, b"Crate.Open\0");
        i32_at(&mut b, 1384, 6004);
        put(&mut b, 1392, b"Foot.Step\0");
        put(&mut b, 1472, b"props/creak.wav 1\0");
        i32_at(&mut b, 1536, 176);
        put(&mut b, 1636, b"AE_CL_PLAYSOUND\0");
        b
    }

    #[test]
    fn reads_every_family_folder_and_include() {
        let deps = read_dependencies(&sample()).unwrap();
        assert_eq!(deps.textures, ["Wood", "wood_burnt"]);
        assert_eq!(deps.used_textures(), [0, 1]);
        assert_eq!(
            deps.material_candidates(),
            vec![
                vec!["models/props/wood".to_owned(), "wood".to_owned()],
                vec!["models/props/wood_burnt".to_owned(), "wood_burnt".to_owned()],
            ]
        );
        assert_eq!(deps.include_models, ["models/humans/male_shared.mdl"]);
        assert_eq!(deps.sounds, ["Crate.Open", "props/creak.wav"]);
    }

    #[test]
    fn hostile_tables_fail_cleanly() {
        let good = sample();
        for i in (0..good.len()).step_by(2) {
            let mut bad = good.clone();
            bad[i] ^= 0x7F;
            let _ = read_dependencies(&bad).map(|d| d.material_candidates());
        }
        let mut bad = good.clone();
        i32_at(&mut bad, 336, 100_000);
        assert!(read_dependencies(&bad).is_err());
        assert!(read_dependencies(b"IDST").is_err());
    }

    fn phy(text: &str) -> Vec<u8> {
        let mut b = Vec::new();
        i32_at(&mut b, 0, 16);
        i32_at(&mut b, 8, 1);
        i32_at(&mut b, 16, 8);
        put(&mut b, 20, b"{break {");
        put(&mut b, 28, text.as_bytes());
        b
    }

    #[test]
    fn gibs_of_the_break_blocks() {
        let text = "solid {
\"index\" \"0\"
\"name\" \"model\"
}
break {
\"model\" \"props_junk\\Crate_Gib01\"
\"health\" \"1\"
}
break {
\"model\" \"models/props_junk/crate_gib02.mdl\"
}
break { \"model\" \"props_junk/crate_gib01\" }
editparams {
\"model\" \"not_a_gib\"
\"totalmass\" \"50\"
}
";
        assert_eq!(gib_models(&phy(text)), ["models/props_junk/crate_gib01.mdl", "models/props_junk/crate_gib02.mdl"]);
        assert!(gib_models(&phy("solid { \"index\" \"0\" }")).is_empty());
        assert!(gib_models(b"").is_empty());
        assert!(gib_models(&[0xFF; 64]).is_empty());
        let mut bad = phy(text);
        i32_at(&mut bad, 16, 1_000_000);
        assert_eq!(gib_models(&bad).len(), 2);
    }
}
