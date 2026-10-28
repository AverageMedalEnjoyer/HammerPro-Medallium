use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscordSettings {
    pub enabled: bool,
    pub show_map: bool,
    pub show_elapsed: bool,
    pub show_tool: bool,
    pub application_id: String,
    pub project_url: String,
}

impl Default for DiscordSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            show_map: true,
            show_elapsed: true,
            show_tool: true,
            application_id: String::new(),
            project_url: String::new(),
        }
    }
}
