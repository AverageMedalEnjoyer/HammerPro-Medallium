use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hp_app::{AppState, t};
use hp_discord::{Presence, Privacy, Status, Texts, Wanted};

const REFRESH: Duration = Duration::from_millis(500);
const MAX_MAPS: usize = 64;

pub struct DiscordPresence {
    presence: Option<Presence>,
    last_check: Option<Instant>,
    opened: HashMap<String, u64>,
}

impl DiscordPresence {
    pub fn new(active: bool) -> Self {
        Self { presence: active.then(Presence::discord), last_check: None, opened: HashMap::new() }
    }

    pub fn tick(&mut self, state: &AppState) {
        let Some(presence) = self.presence.as_mut() else {
            return;
        };
        if self.last_check.is_some_and(|t| t.elapsed() < REFRESH) {
            return;
        }
        self.last_check = Some(Instant::now());
        let s = &state.settings.discord;
        let Some(id) = hp_discord::application_id(&s.application_id).filter(|_| s.enabled) else {
            presence.set(Wanted::default());
            return;
        };
        let doc = &state.doc;
        let map = (doc.path().is_some() || doc.manifest().is_some() || doc.is_dirty())
            .then(|| crate::files::map_display_name(state));
        let opened_at = map.as_ref().map(|name| {
            if self.opened.len() >= MAX_MAPS && !self.opened.contains_key(name) {
                self.opened.clear();
            }
            let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
            *self.opened.entry(name.clone()).or_insert(now)
        });
        let game_id = state.settings.game.active_game();
        let game_name = hp_fs::known_game(game_id).map_or(game_id, |g| g.name);
        let status = Status {
            map: map.as_deref(),
            opened_at,
            tool: Some(t!(state.tool.label_key())),
            compiling: state.runtime.compile.progress.is_some(),
            game_id,
            game_name,
            version: env!("CARGO_PKG_VERSION"),
            project_url: &s.project_url,
        };
        let privacy = Privacy { show_map: s.show_map, show_elapsed: s.show_elapsed, show_tool: s.show_tool };
        let texts = Texts {
            editing: t!("shell.discord.activity_editing"),
            editing_hidden: t!("shell.discord.activity_editing_hidden"),
            no_map: t!("shell.discord.activity_no_map"),
            compiling: t!("shell.discord.activity_compiling"),
            button: t!("shell.discord.activity_button"),
        };
        let activity = hp_discord::build_activity(&status, privacy, &texts);
        presence.set(Wanted { client_id: id.to_owned(), activity: Some(activity) });
    }
}
