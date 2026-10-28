use std::collections::HashMap;

use hp_kv::Block;

const SOUND_CHARS: &[char] = &['*', '#', '@', '>', '<', '^', ')', '(', '}', '$', '?', '&', '~', '+', '%', '`'];

pub fn strip_sound_chars(name: &str) -> Option<String> {
    let t = name.trim();
    if t.starts_with('!') {
        return None;
    }
    let s = t.trim_start_matches(SOUND_CHARS).replace('\\', "/").to_ascii_lowercase();
    let s = s.trim_start_matches('/');
    (!s.is_empty()).then(|| s.to_owned())
}

pub fn is_sound_file(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    [".wav", ".mp3", ".ogg"].iter().any(|e| l.ends_with(e))
}

pub fn sound_path(name: &str) -> Option<String> {
    let s = strip_sound_chars(name)?;
    Some(if s.starts_with("sound/") { s } else { format!("sound/{s}") })
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptEntry {
    pub file: String,
    pub waves: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Soundscape {
    pub file: String,
    pub waves: Vec<String>,
    pub scripts: Vec<String>,
    pub nested: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SoundIndex {
    pub entries: HashMap<String, ScriptEntry>,
}

#[derive(Clone, Debug, Default)]
pub struct SoundscapeIndex {
    pub entries: HashMap<String, Soundscape>,
}

fn collect(block: &Block, key: &str, out: &mut Vec<String>) {
    let mut stack = vec![block];
    while let Some(b) = stack.pop() {
        for (k, v) in &b.props {
            if k.eq_ignore_ascii_case(key) {
                out.push(v.clone());
            }
        }
        stack.extend(b.children.iter());
    }
}

pub fn manifest_files(text: &str, keys: &[&str]) -> Vec<String> {
    let Ok(root) = hp_kv::parse(text) else { return Vec::new() };
    let mut out = Vec::new();
    for block in &root.children {
        for (k, v) in &block.props {
            if keys.iter().any(|key| k.eq_ignore_ascii_case(key)) {
                out.push(v.replace('\\', "/").to_ascii_lowercase());
            }
        }
    }
    out
}

impl SoundIndex {
    pub fn add_file(&mut self, file: &str, text: &str, replace: bool) {
        let Ok(root) = hp_kv::parse(text) else { return };
        for block in &root.children {
            let mut waves = Vec::new();
            collect(block, "wave", &mut waves);
            let entry = ScriptEntry { file: file.to_owned(), waves };
            let key = block.name.to_ascii_lowercase();
            if replace {
                self.entries.insert(key, entry);
            } else {
                self.entries.entry(key).or_insert(entry);
            }
        }
    }
}

impl SoundscapeIndex {
    pub fn add_file(&mut self, file: &str, text: &str, replace: bool) {
        let Ok(root) = hp_kv::parse(text) else { return };
        for block in &root.children {
            let mut s = Soundscape { file: file.to_owned(), ..Soundscape::default() };
            collect(block, "wave", &mut s.waves);
            collect(block, "soundname", &mut s.scripts);
            for child in &block.children {
                if child.name.eq_ignore_ascii_case("playsoundscape") {
                    s.nested.extend(child.get("name").map(str::to_owned));
                }
            }
            let key = block.name.to_ascii_lowercase();
            if replace {
                self.entries.insert(key, s);
            } else {
                self.entries.entry(key).or_insert(s);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(strip_sound_chars("^)weapons\\Pistol.wav").as_deref(), Some("weapons/pistol.wav"));
        assert_eq!(strip_sound_chars("!HG_ALERT"), None);
        assert_eq!(strip_sound_chars("  *#"), None);
        assert_eq!(sound_path("#music/x.mp3").as_deref(), Some("sound/music/x.mp3"));
        assert!(is_sound_file("A.WAV"));
        assert!(!is_sound_file("Doors.Move1"));
    }

    #[test]
    fn scripts_and_soundscapes() {
        let mut s = SoundIndex::default();
        s.add_file(
            "scripts/a.txt",
            r#""Door.Move" { "channel" "CHAN_AUTO" "rndwave" { "wave" "doors/a.wav" "wave" ")doors/b.wav" } }"#,
            false,
        );
        s.add_file("maps/x_level_sounds.txt", r#""Door.Move" { "wave" "custom/door.wav" }"#, true);
        assert_eq!(s.entries["door.move"].waves, ["custom/door.wav"]);
        assert_eq!(s.entries["door.move"].file, "maps/x_level_sounds.txt");

        let mut c = SoundscapeIndex::default();
        c.add_file(
            "scripts/soundscapes_x.txt",
            r#""Cave" { "dsp" "1" "playlooping" { "wave" "ambient/cave.wav" } "playrandom" { "rndwave" { "wave" "a.wav" } }
               "playsoundscape" { "name" "Generic" } "playlooping" { "soundname" "Wind.Loop" } }"#,
            true,
        );
        let cave = &c.entries["cave"];
        assert_eq!(cave.nested, ["Generic"]);
        assert_eq!(cave.scripts, ["Wind.Loop"]);
        assert_eq!(cave.waves.len(), 2);
        assert_eq!(
            manifest_files(r#"soundscapes_manifest { "file" "scripts/A.txt" "x" "y" }"#, &["file"]),
            ["scripts/a.txt"]
        );
    }
}
