#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DragPayload {
    Material(String),
    Model { path: String, skin: u32 },
    EntityClass(String),
    Prefab(std::path::PathBuf),
}
