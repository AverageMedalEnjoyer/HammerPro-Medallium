use std::cell::Cell;

#[derive(Debug, Default)]
pub struct VisibilityRuntime {
    pub edit_cordon: Option<usize>,
    pub move_to_visgroup_open: bool,
    pub focus_box: Option<hp_geom::Aabb>,
    pub hidden_cache: Cell<Option<(u64, bool)>>,
}
