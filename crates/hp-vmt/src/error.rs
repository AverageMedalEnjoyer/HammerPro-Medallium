#[derive(Debug, thiserror::Error)]
pub enum VmtError {
    #[error(transparent)]
    Kv(#[from] hp_kv::KvError),
    #[error("no shader block found in material")]
    NoShader,
    #[error("patch material has no include key")]
    PatchWithoutInclude,
    #[error("patch include not found: {0}")]
    IncludeNotFound(String),
    #[error("patch include chain deeper than {max} levels at {path}")]
    IncludeTooDeep { path: String, max: usize },
    #[error("patch include cycle detected at {0}")]
    IncludeCycle(String),
    #[error("failed to parse included material {path}: {source}")]
    Include { path: String, source: Box<VmtError> },
    #[error("invalid parameter: {0}")]
    InvalidParam(String),
}
