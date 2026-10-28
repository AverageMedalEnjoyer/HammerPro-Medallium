use hp_app::i18n::TrTable;
use hp_app::{Action, Severity, ToolId, t};
use hp_tools_mesh::disp::commands;

use super::Package;
use crate::app::HammerProApp;

pub(crate) struct Mesh;

#[derive(Default)]
pub(crate) struct State {}

const HANDLED: &[Action] = &[
    Action::VertexSplitFace,
    Action::VertexScale,
    Action::DispSubdivide,
    Action::DispInvertAlpha,
    Action::DispNoise,
    Action::ToggleUvLock,
];

impl Package for Mesh {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, _ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::VertexSplitFace | Action::VertexScale => {
                if app.state.tool != ToolId::Vertex && app.has_tool(ToolId::Vertex) {
                    app.switch_tool(ToolId::Vertex);
                }
                if !app.active_tool_action(action) {
                    app.state.notify(Severity::Info, t!("mesh.app.vertex_tool_only"));
                }
            }
            Action::DispSubdivide => {
                commands::subdivide(&mut app.state);
            }
            Action::DispInvertAlpha => {
                commands::invert_alpha(&mut app.state);
            }
            Action::DispNoise => app.state.runtime.mesh.noise_dialog = true,
            Action::ToggleUvLock => {
                let s = &mut app.state.settings.mesh;
                s.uv_lock = !s.uv_lock;
                app.state.settings_dirty = true;
            }
            _ => return false,
        }
        true
    }

    fn dialogs(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        commands::noise_window(ctx, &mut app.state);
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

const TR: TrTable = &[(
    "mesh.app.vertex_tool_only",
    "This command works in the vertex tool.",
    "Cette commande fonctionne dans l'outil sommets.",
)];
