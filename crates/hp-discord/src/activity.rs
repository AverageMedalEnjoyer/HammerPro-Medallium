use serde_json::{Map, Value, json};

pub const LOGO_KEY: &str = "hammerpro";

const MAX_TEXT: usize = 128;
const MAX_LABEL: usize = 32;
const MAX_URL: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Activity {
    pub details: String,
    pub state: String,
    pub start: Option<u64>,
    pub large_image: String,
    pub large_text: String,
    pub small_image: Option<String>,
    pub small_text: Option<String>,
    pub button: Option<Button>,
}

fn field(text: &str, max: usize) -> String {
    let mut s: String = text.trim().chars().take(max).collect();
    while s.chars().count() < 2 {
        s.push(' ');
    }
    s
}

impl Activity {
    pub fn to_json(&self) -> Value {
        let mut a = Map::new();
        if !self.details.trim().is_empty() {
            a.insert("details".into(), field(&self.details, MAX_TEXT).into());
        }
        if !self.state.trim().is_empty() {
            a.insert("state".into(), field(&self.state, MAX_TEXT).into());
        }
        if let Some(start) = self.start {
            a.insert("timestamps".into(), json!({ "start": start }));
        }
        let mut assets = Map::new();
        if !self.large_image.is_empty() {
            assets.insert("large_image".into(), self.large_image.clone().into());
            if !self.large_text.trim().is_empty() {
                assets.insert("large_text".into(), field(&self.large_text, MAX_TEXT).into());
            }
        }
        if let Some(small) = self.small_image.as_ref().filter(|s| !s.is_empty()) {
            assets.insert("small_image".into(), small.clone().into());
            if let Some(text) = self.small_text.as_ref().filter(|t| !t.trim().is_empty()) {
                assets.insert("small_text".into(), field(text, MAX_TEXT).into());
            }
        }
        if !assets.is_empty() {
            a.insert("assets".into(), Value::Object(assets));
        }
        if let Some(b) = &self.button {
            let label: String = b.label.trim().chars().take(MAX_LABEL).collect();
            a.insert("buttons".into(), json!([{ "label": label, "url": b.url }]));
        }
        Value::Object(a)
    }

    pub fn is_new_map(&self, before: &Activity) -> bool {
        self.details != before.details || self.start != before.start
    }
}

pub fn handshake_json(client_id: &str) -> String {
    json!({ "v": 1, "client_id": client_id }).to_string()
}

pub fn set_activity_json(pid: u32, activity: Option<&Activity>, nonce: &str) -> String {
    let mut args = Map::new();
    args.insert("pid".into(), pid.into());
    if let Some(a) = activity {
        args.insert("activity".into(), a.to_json());
    }
    json!({ "cmd": "SET_ACTIVITY", "args": Value::Object(args), "nonce": nonce }).to_string()
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Status<'a> {
    pub map: Option<&'a str>,
    pub opened_at: Option<u64>,
    pub tool: Option<&'a str>,
    pub compiling: bool,
    pub game_id: &'a str,
    pub game_name: &'a str,
    pub version: &'a str,
    pub project_url: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Privacy {
    pub show_map: bool,
    pub show_elapsed: bool,
    pub show_tool: bool,
}

impl Default for Privacy {
    fn default() -> Self {
        Self { show_map: true, show_elapsed: true, show_tool: true }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Texts<'a> {
    pub editing: &'a str,
    pub editing_hidden: &'a str,
    pub no_map: &'a str,
    pub compiling: &'a str,
    pub button: &'a str,
}

impl Texts<'static> {
    pub const EN: Texts<'static> = Texts {
        editing: "Editing {0}",
        editing_hidden: "Editing a map",
        no_map: "No map open",
        compiling: "Compiling...",
        button: "Get Hammer Pro",
    };
}

pub fn game_image_key(game_id: &str) -> String {
    let id = game_id.trim().to_ascii_lowercase();
    let key = match id.as_str() {
        "gmod" | "garrysmod" => "gmod",
        "hl2" | "episodic" | "ep2" | "lostcoast" | "sdk2013sp" | "sdk2007" => "hl2",
        "hl1" => "hls",
        "hl2mp" | "sdk2013mp" => "hl2dm",
        "tf" | "tf2" => "tf2",
        "cstrike" | "css" => "css",
        "dod" => "dods",
        "portal" => "portal",
        "portal2" => "portal2",
        "left4dead" => "l4d",
        "left4dead2" => "l4d2",
        "csgo" => "csgo",
        other => other,
    };
    key.to_owned()
}

pub fn build_activity(status: &Status, privacy: Privacy, texts: &Texts) -> Activity {
    let details = match status.map {
        Some(map) if privacy.show_map => texts.editing.replace("{0}", map),
        Some(_) => texts.editing_hidden.to_owned(),
        None => texts.no_map.to_owned(),
    };
    let state = if status.compiling {
        texts.compiling.to_owned()
    } else {
        match status.tool.filter(|t| privacy.show_tool && status.map.is_some() && !t.is_empty()) {
            Some(tool) => tool.to_owned(),
            None => status.game_name.to_owned(),
        }
    };
    let url = status.project_url.trim();
    let button = (url.len() <= MAX_URL && (url.starts_with("https://") || url.starts_with("http://")))
        .then(|| Button { label: texts.button.to_owned(), url: url.to_owned() });
    let small = (!status.game_id.trim().is_empty()).then(|| game_image_key(status.game_id));
    Activity {
        details,
        state,
        start: if privacy.show_elapsed && status.map.is_some() { status.opened_at } else { None },
        large_image: LOGO_KEY.to_owned(),
        large_text: format!("Hammer Pro {}", status.version),
        small_text: small.as_ref().map(|_| status.game_name.to_owned()).filter(|n| !n.is_empty()),
        small_image: small,
        button,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status() -> Status<'static> {
        Status {
            map: Some("gm_city.vmf"),
            opened_at: Some(1_700_000_000),
            tool: Some("Block"),
            compiling: false,
            game_id: "gmod",
            game_name: "Garry's Mod",
            version: "1.0.0",
            project_url: "",
        }
    }

    #[test]
    fn full_activity_json() {
        let a = build_activity(&status(), Privacy::default(), &Texts::EN);
        let v: Value = serde_json::from_str(&set_activity_json(42, Some(&a), "7")).unwrap();
        assert_eq!(v["cmd"], "SET_ACTIVITY");
        assert_eq!(v["nonce"], "7");
        assert_eq!(v["args"]["pid"], 42);
        let act = &v["args"]["activity"];
        assert_eq!(act["details"], "Editing gm_city.vmf");
        assert_eq!(act["state"], "Block");
        assert_eq!(act["timestamps"]["start"], 1_700_000_000u64);
        assert_eq!(act["assets"]["large_image"], "hammerpro");
        assert_eq!(act["assets"]["large_text"], "Hammer Pro 1.0.0");
        assert_eq!(act["assets"]["small_image"], "gmod");
        assert_eq!(act["assets"]["small_text"], "Garry's Mod");
        assert!(act.get("buttons").is_none());
    }

    #[test]
    fn privacy_options_off() {
        let p = Privacy { show_map: false, show_elapsed: false, show_tool: false };
        let act = build_activity(&status(), p, &Texts::EN).to_json();
        assert_eq!(act["details"], "Editing a map");
        assert_eq!(act["state"], "Garry's Mod");
        assert!(act.get("timestamps").is_none());
        assert!(!act.to_string().contains("gm_city"));
    }

    #[test]
    fn no_map_compiling_and_button() {
        let s = Status {
            map: None,
            project_url: "https://example.org/hammerpro",
            game_id: "tf",
            game_name: "Team Fortress 2",
            ..status()
        };
        let act = build_activity(&s, Privacy::default(), &Texts::EN).to_json();
        assert_eq!(act["details"], "No map open");
        assert_eq!(act["state"], "Team Fortress 2");
        assert!(act.get("timestamps").is_none());
        assert_eq!(act["assets"]["small_image"], "tf2");
        assert_eq!(act["buttons"][0]["label"], "Get Hammer Pro");
        assert_eq!(act["buttons"][0]["url"], "https://example.org/hammerpro");
        let s = Status { compiling: true, project_url: "javascript:x", ..status() };
        let c = build_activity(&s, Privacy::default(), &Texts::EN);
        assert_eq!(c.state, "Compiling...");
        assert!(c.button.is_none());
    }

    #[test]
    fn handshake_and_clear() {
        let v: Value = serde_json::from_str(&handshake_json("123")).unwrap();
        assert_eq!(v, json!({ "v": 1, "client_id": "123" }));
        let v: Value = serde_json::from_str(&set_activity_json(1, None, "2")).unwrap();
        assert!(v["args"].get("activity").is_none());
    }

    #[test]
    fn discord_field_limits() {
        let a = Activity { details: "x".into(), state: "y".repeat(300), ..Activity::default() }.to_json();
        assert_eq!(a["details"], "x ");
        assert_eq!(a["state"].as_str().unwrap().chars().count(), 128);
    }
}
