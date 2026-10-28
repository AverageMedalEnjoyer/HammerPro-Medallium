use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;
use crate::tool::ToolId;

action_list! { mesh_actions {
    VertexSplitFace => "vertex_split_face", Tools, ToolsMesh, sc(CTRL, Key::F), Ready;
    VertexScale => "vertex_scale", Tools, ToolsMesh, sc(ALT, Key::E), Ready;
    DispSubdivide => "disp_subdivide", Tools, ToolsMesh, None, Ready;
    DispInvertAlpha => "disp_invert_alpha", Tools, ToolsMesh, None, Ready;
    DispNoise => "disp_noise", Tools, ToolsMesh, None, Ready;
    ToggleUvLock => "toggle_uv_lock", Tools, ToolsMesh, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

fn has_selected_disps(state: &AppState) -> bool {
    let doc = &state.doc;
    let sel = doc.selection();
    let face = sel.faces.iter().any(|f| doc.face_side(*f).is_some_and(|s| s.disp.is_some()));
    face || sel.objects.iter().any(|o| match o {
        hp_editor::ObjRef::Solid(id) => doc.scene().solid(*id).is_some_and(|n| n.has_disp()),
        hp_editor::ObjRef::Entity(id) => {
            doc.scene().entity_solid_ids(*id).iter().any(|s| doc.scene().solid(*s).is_some_and(|n| n.has_disp()))
        }
    })
}

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::VertexSplitFace | Action::VertexScale => Some(state.tool == ToolId::Vertex),
        Action::DispSubdivide | Action::DispInvertAlpha | Action::DispNoise => Some(has_selected_disps(state)),
        _ => None,
    }
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    match action {
        Action::ToggleUvLock => Some(Check::Toggle(state.settings.mesh.uv_lock)),
        _ => None,
    }
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    Some(match action {
        Action::VertexSplitFace => "✂",
        Action::VertexScale => "⛶",
        Action::DispSubdivide => "⊞",
        Action::DispInvertAlpha => "◑",
        Action::DispNoise => "〰",
        Action::ToggleUvLock => "🔒",
        _ => return None,
    })
}

pub(super) const TR: TrTable = &[
    ("action.vertex_split_face", "Split face", "Scinder la face"),
    ("action.vertex_scale", "Scale vertices...", "Mettre les sommets à l'échelle..."),
    ("action.disp_subdivide", "Subdivide displacements", "Subdiviser les displacements"),
    ("action.disp_invert_alpha", "Invert displacement alpha", "Inverser l'alpha des displacements"),
    ("action.disp_noise", "Add noise to displacements...", "Ajouter du bruit aux displacements..."),
    ("action.toggle_uv_lock", "UV lock (vertex tool)", "Verrouillage UV (outil sommets)"),
    ("shell.undo.invert_displacement_alpha", "Invert displacement alpha", "Inversion de l'alpha du displacement"),
    ("shell.undo.displacement_noise", "Displacement noise", "Bruit sur le displacement"),
    ("shell.undo.scale_displacement", "Scale displacement", "Mise à l'échelle du displacement"),
    ("shell.undo.displacement_collision", "Displacement collision", "Collisions du displacement"),
    ("shell.undo.subdivide_displacements", "Subdivide displacements", "Subdivision des displacements"),
    ("shell.undo.scale_vertices", "Scale vertices", "Mise à l'échelle des sommets"),
    ("shell.undo.sculpt_displacement", "Sculpt displacement", "Sculpture du displacement"),
    ("shell.undo.tag_displacement", "Tag displacement", "Marquage du displacement"),
    ("shell.undo.displacement_shadows", "Displacement shadows", "Ombres du displacement"),
    ("shell.undo.displacement_lightmap_scale", "Displacement lightmap scale", "Échelle de lightmap du displacement"),
];
