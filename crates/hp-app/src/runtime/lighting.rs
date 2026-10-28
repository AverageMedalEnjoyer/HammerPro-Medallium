#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BakePhase {
    Preparing,
    Compiling,
    Direct,
    Bounce,
    Finishing,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BakeKind {
    #[default]
    Full,
    AroundCamera,
    Update,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BakeProgress {
    pub kind: BakeKind,
    pub phase: BakePhase,
    pub fraction: f32,
    pub elapsed_s: f32,
    pub faces: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BakeSummary {
    pub kind: BakeKind,
    pub cancelled: bool,
    pub seconds: f32,
    pub faces: usize,
    pub luxels: u64,
    pub density: f32,
    pub atlas_full: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreviewStats {
    pub faces: usize,
    pub baked_faces: usize,
    pub luxels: u64,
    pub triangles: usize,
    pub layers: u32,
    pub gpu_bytes: u64,
    pub world_lights: usize,
    pub static_props: usize,
    pub vertex_lit_props: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BudgetEntry {
    pub solid: u32,
    pub side: usize,
    pub bytes: u64,
    pub scale: f64,
    pub dims: [u32; 2],
    pub displacement: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BudgetSummary {
    pub faces: usize,
    pub luxels: u64,
    pub bytes: u64,
    pub limit: u64,
    pub tiny_faces: usize,
    pub wide_faces: usize,
    pub oversized_displacements: usize,
    pub top: Vec<BudgetEntry>,
    pub revision: u64,
}

impl BudgetSummary {
    pub fn fraction(&self) -> f64 {
        if self.limit == 0 { 0.0 } else { self.bytes as f64 / self.limit as f64 }
    }
}

#[derive(Debug, Default)]
pub struct LightingRuntime {
    pub progress: Option<BakeProgress>,
    pub last: Option<BakeSummary>,
    pub has_bake: bool,
    pub stale: bool,
    pub stats: PreviewStats,
    pub budget: Option<BudgetSummary>,
    pub budget_wanted: bool,
    pub no_lights: bool,
}
