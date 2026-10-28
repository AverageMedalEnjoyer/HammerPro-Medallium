use hp_geom::Aabb;

#[derive(Debug, Default)]
pub struct BrushRuntime {
    pub last_created: Option<Aabb>,
}
