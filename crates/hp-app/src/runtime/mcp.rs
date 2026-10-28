#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpClientRow {
    pub id: &'static str,
    pub label: &'static str,
    pub status: Option<String>,
    pub ok: bool,
    pub present: bool,
}

#[derive(Debug, Default)]
pub struct McpRuntime {
    pub running: bool,
    pub port: Option<u16>,
    pub clients: usize,
    pub calls: u64,
    pub last_tool: Option<String>,
    pub error: Option<String>,
    pub tools: Vec<(String, String)>,
    pub show_window: bool,
    pub install_running: bool,
    pub install_rows: Vec<McpClientRow>,
}
