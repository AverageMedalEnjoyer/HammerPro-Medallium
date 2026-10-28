use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

pub(crate) const LUA: &str = include_str!("../../../../../assets/bridge/hammerpro_bridge.lua");

const ADDON: &str = "hammerpro_bridge";
const MAX_JSON: u64 = 4 << 20;
pub(crate) const MAX_PROPS: usize = 4096;
const MAX_COORD: f64 = 65_536.0;

fn addon_dir(root: &Path) -> PathBuf {
    root.join("garrysmod").join("addons").join(ADDON)
}

pub(crate) fn lua_file(root: &Path) -> PathBuf {
    addon_dir(root).join("lua").join("autorun").join("server").join("hammerpro_bridge.lua")
}

fn data_dir(root: &Path) -> PathBuf {
    root.join("garrysmod").join("data").join("hammerpro")
}

pub(crate) fn is_installed(root: &Path) -> bool {
    lua_file(root).is_file()
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if hp_compile::sandbox::is_sandboxed() {
        return Err(std::io::Error::other(hp_compile::sandbox::REFUSED));
    }
    let dir = path.parent().ok_or_else(|| std::io::Error::other("no folder"))?;
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

pub(crate) fn install(root: &Path) -> std::io::Result<()> {
    if !root.join("garrysmod").is_dir() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "garrysmod folder not found"));
    }
    write_atomic(&lua_file(root), LUA.as_bytes())
}

pub(crate) fn remove(root: &Path) -> std::io::Result<()> {
    let file = lua_file(root);
    match std::fs::remove_file(&file) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let top = addon_dir(root);
    let mut dir = file.parent().map(Path::to_path_buf);
    while let Some(d) = dir {
        if !d.starts_with(&top) || std::fs::remove_dir(&d).is_err() {
            break;
        }
        if d == top {
            break;
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(crate) struct Command {
    pub seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setpos: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setang: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dump_props: bool,
}

pub(crate) fn next_seq() -> u64 {
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    u64::try_from(now.as_millis()).unwrap_or(u64::MAX)
}

pub(crate) fn send(root: &Path, cmd: &Command) -> std::io::Result<()> {
    let json = serde_json::to_vec(cmd).map_err(std::io::Error::other)?;
    write_atomic(&data_dir(root).join("cmd.json"), &json)
}

fn read_small(path: &Path) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    (meta.len() <= MAX_JSON).then(|| std::fs::read(path).ok()).flatten()
}

pub(crate) fn props_time(root: &Path) -> Option<SystemTime> {
    std::fs::metadata(data_dir(root).join("props.json")).ok()?.modified().ok()
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GameProp {
    pub class: String,
    pub model: String,
    pub skin: u32,
    pub origin: [f64; 3],
    pub angles: [f64; 3],
}

#[derive(Deserialize)]
struct RawProps {
    #[serde(default)]
    props: Vec<RawProp>,
}

#[derive(Deserialize)]
struct RawProp {
    #[serde(default)]
    class: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    skin: f64,
    #[serde(default)]
    origin: Vec<f64>,
    #[serde(default)]
    angles: Vec<f64>,
}

fn safe_model(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.starts_with("models/")
        && lower.ends_with(".mdl")
        && path.len() <= 256
        && !path.split('/').any(|part| part.is_empty() || part == "." || part == "..")
        && path.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.' | ' '))
}

fn vec3(v: &[f64], limit: f64) -> Option<[f64; 3]> {
    let [x, y, z] = <[f64; 3]>::try_from(v).ok()?;
    [x, y, z].iter().all(|c| c.is_finite() && c.abs() <= limit).then_some([x, y, z])
}

pub(crate) fn parse_props(bytes: &[u8]) -> Option<Vec<GameProp>> {
    let raw: RawProps = serde_json::from_slice(bytes).ok()?;
    let mut out = Vec::new();
    for p in raw.props.into_iter().take(MAX_PROPS) {
        let class_ok = matches!(p.class.as_str(), "prop_physics" | "prop_dynamic" | "prop_physics_multiplayer");
        let (Some(origin), Some(angles)) = (vec3(&p.origin, MAX_COORD), vec3(&p.angles, 360_000.0)) else {
            continue;
        };
        if !class_ok || !safe_model(&p.model) || !p.skin.is_finite() || !(0.0..=1024.0).contains(&p.skin) {
            continue;
        }
        out.push(GameProp { class: p.class, model: p.model, skin: p.skin as u32, origin, angles });
    }
    Some(out)
}

pub(crate) fn read_props(root: &Path, after: Option<SystemTime>) -> Option<Vec<GameProp>> {
    let time = props_time(root)?;
    if after.is_some_and(|a| time <= a) {
        return None;
    }
    parse_props(&read_small(&data_dir(root).join("props.json"))?)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub(crate) struct Ack {
    pub map: String,
    pub spawned: bool,
    pub lua_ms: Option<u64>,
}

pub(crate) fn ack_time(root: &Path) -> Option<SystemTime> {
    std::fs::metadata(data_dir(root).join("ack.json")).ok()?.modified().ok()
}

pub(crate) fn read_ack(root: &Path, after: Option<SystemTime>) -> Option<Ack> {
    let time = ack_time(root)?;
    if after.is_some_and(|a| time <= a) {
        return None;
    }
    let mut ack: Ack = serde_json::from_slice(&read_small(&data_dir(root).join("ack.json"))?).ok()?;
    ack.lua_ms = ack.lua_ms.filter(|ms| *ms < 3_600_000);
    Some(ack)
}

pub(crate) const PROPS_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) fn props_to_map(props: &[GameProp]) -> hp_vmf::Map {
    let mut map = hp_vmf::Map::new_empty();
    let fmt = |v: [f64; 3]| format!("{} {} {}", round(v[0]), round(v[1]), round(v[2]));
    for (i, p) in props.iter().enumerate() {
        let classname = if p.class == "prop_physics" { "prop_physics" } else { "prop_static" };
        let mut props = vec![
            ("origin".to_owned(), fmt(p.origin)),
            ("angles".to_owned(), fmt(p.angles)),
            ("model".to_owned(), p.model.clone()),
        ];
        if p.skin != 0 {
            props.push(("skin".to_owned(), p.skin.to_string()));
        }
        map.entities.push(hp_vmf::Entity {
            id: u32::try_from(i + 2).unwrap_or(u32::MAX),
            classname: classname.to_owned(),
            props,
            ..hp_vmf::Entity::default()
        });
    }
    map
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hammerpro_bridge_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("garrysmod")).expect("scratch dir");
        dir
    }

    #[test]
    fn the_addon_installs_and_removes_only_its_files() {
        let root = scratch("install");
        assert!(!is_installed(&root));
        install(&root).expect("install");
        assert!(is_installed(&root));
        assert_eq!(std::fs::read_to_string(lua_file(&root)).expect("read"), LUA);
        install(&root).expect("again");
        let user = addon_dir(&root).join("lua").join("mine.lua");
        std::fs::write(&user, "print(1)").expect("write");
        remove(&root).expect("remove");
        assert!(!is_installed(&root));
        assert!(user.is_file(), "the user's file stays");
        std::fs::remove_file(&user).expect("cleanup");
        install(&root).expect("install");
        remove(&root).expect("remove");
        assert!(!addon_dir(&root).exists(), "empty folders go");
        remove(&root).expect("removing twice is fine");
        assert!(install(&root.join("nowhere")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_lua_has_no_long_dashes_and_only_listens_on_listen_servers() {
        assert!(!LUA.contains('\u{2014}') && !LUA.contains('\u{2013}'));
        assert!(LUA.contains("game.IsDedicated()"));
    }

    #[test]
    fn commands_are_written_atomically_as_json() {
        let root = scratch("cmd");
        let cmd = Command {
            seq: 5,
            map: Some("gm_test".to_owned()),
            setpos: Some([1.0, 2.0, 3.0]),
            setang: Some([0.0, 90.0, 0.0]),
            dump_props: false,
        };
        send(&root, &cmd).expect("send");
        let text = std::fs::read_to_string(data_dir(&root).join("cmd.json")).expect("read");
        assert_eq!(text, r#"{"seq":5,"map":"gm_test","setpos":[1.0,2.0,3.0],"setang":[0.0,90.0,0.0]}"#);
        send(&root, &Command { seq: 6, dump_props: true, ..Command::default() }).expect("send");
        let text = std::fs::read_to_string(data_dir(&root).join("cmd.json")).expect("read");
        assert_eq!(text, r#"{"seq":6,"dump_props":true}"#);
        assert!(next_seq() > 1_600_000_000_000);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn props_from_the_game_are_checked() {
        let json = br#"{"map":"gm_x","props":[
            {"class":"prop_physics","model":"models/props_c17/oildrum001.mdl","skin":1,"origin":[1,2,3],"angles":[0,90,0]},
            {"class":"prop_dynamic","model":"models/a.mdl","skin":0,"origin":[4,5,6],"angles":[0,0,0]},
            {"class":"prop_physics","model":"models/../../cfg/autoexec.mdl","origin":[0,0,0],"angles":[0,0,0]},
            {"class":"npc_combine","model":"models/a.mdl","origin":[0,0,0],"angles":[0,0,0]},
            {"class":"prop_physics","model":"models/a.mdl","origin":[1e30,0,0],"angles":[0,0,0]},
            {"class":"prop_physics","model":"models/a.mdl","origin":[0,0],"angles":[0,0,0]}
        ]}"#;
        let props = parse_props(json).expect("parses");
        assert_eq!(props.len(), 2, "{props:?}");
        assert_eq!(props[0].skin, 1);
        assert!(parse_props(b"not json").is_none());
        assert_eq!(parse_props(b"{}").map(|p| p.len()), Some(0));
        let map = props_to_map(&props);
        assert_eq!(map.entities.len(), 2);
        assert_eq!(map.entities[0].classname, "prop_physics");
        assert_eq!(map.entities[1].classname, "prop_static");
        assert_eq!(map.entities[0].get("skin"), Some("1"));
        assert_eq!(map.entities[1].get("origin"), Some("4 5 6"));
        let text = hp_vmf::write(&map);
        assert_eq!(hp_vmf::parse(&text).expect("parses").entities.len(), 2);
    }

    #[test]
    fn acks_are_read_only_when_newer_and_checked() {
        let root = scratch("ack");
        assert!(read_ack(&root, None).is_none());
        let path = data_dir(&root).join("ack.json");
        std::fs::create_dir_all(path.parent().expect("dir")).expect("dir");
        std::fs::write(&path, br#"{"seq":"5","map":"gm_x","placed":false,"spawned":true,"lua_ms":2500,"time":1}"#)
            .expect("write");
        let ack = read_ack(&root, None).expect("ack");
        assert_eq!(ack, Ack { map: "gm_x".to_owned(), spawned: true, lua_ms: Some(2500) });
        assert!(read_ack(&root, ack_time(&root)).is_none());
        std::fs::write(&path, br#"{"map":"gm_x","spawned":true,"lua_ms":99999999999}"#).expect("write");
        assert_eq!(read_ack(&root, None).expect("ack").lua_ms, None, "nonsense timings are dropped");
        std::fs::write(&path, b"garbage").expect("write");
        assert!(read_ack(&root, None).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn props_are_read_only_when_newer() {
        let root = scratch("props");
        assert!(read_props(&root, None).is_none());
        let path = data_dir(&root).join("props.json");
        std::fs::create_dir_all(path.parent().expect("dir")).expect("dir");
        std::fs::write(&path, br#"{"props":[]}"#).expect("write");
        assert_eq!(read_props(&root, None).map(|p| p.len()), Some(0));
        let t = props_time(&root);
        assert!(read_props(&root, t).is_none(), "not newer than the request");
        let _ = std::fs::remove_dir_all(&root);
    }
}
