use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use parking_lot::Mutex;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MountStage {
    #[default]
    Planning,
    Opening,
    Indexing,
}

impl MountStage {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => MountStage::Opening,
            2 => MountStage::Indexing,
            _ => MountStage::Planning,
        }
    }

    fn to_u8(self) -> u8 {
        match self {
            MountStage::Planning => 0,
            MountStage::Opening => 1,
            MountStage::Indexing => 2,
        }
    }
}

#[derive(Debug, Default)]
pub struct MountProgress {
    stage: AtomicU8,
    done: AtomicUsize,
    total: AtomicUsize,
    current: Mutex<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MountSnapshot {
    pub stage: MountStage,
    pub done: usize,
    pub total: usize,
    pub current: String,
}

impl MountSnapshot {
    pub fn fraction(&self) -> Option<f32> {
        match self.stage {
            MountStage::Planning => None,
            MountStage::Indexing => Some(1.0),
            MountStage::Opening if self.total == 0 => None,
            MountStage::Opening => Some((self.done.min(self.total) as f32 / self.total as f32).clamp(0.0, 1.0)),
        }
    }
}

impl MountProgress {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_stage(&self, stage: MountStage) {
        self.stage.store(stage.to_u8(), Ordering::Release);
    }

    pub fn begin_opening(&self, total: usize) {
        self.done.store(0, Ordering::Release);
        self.total.store(total, Ordering::Release);
        self.current.lock().clear();
        self.set_stage(MountStage::Opening);
    }

    pub fn started(&self, label: &str) {
        let mut cur = self.current.lock();
        cur.clear();
        cur.push_str(label);
    }

    pub fn finished_one(&self) {
        self.done.fetch_add(1, Ordering::AcqRel);
    }

    pub fn snapshot(&self) -> MountSnapshot {
        MountSnapshot {
            stage: MountStage::from_u8(self.stage.load(Ordering::Acquire)),
            done: self.done.load(Ordering::Acquire),
            total: self.total.load(Ordering::Acquire),
            current: self.current.lock().clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_moves_through_the_steps() {
        let p = MountProgress::new();
        assert_eq!(p.snapshot().stage, MountStage::Planning);
        assert_eq!(p.snapshot().fraction(), None);
        p.begin_opening(4);
        p.started("hl2_textures.vpk");
        p.finished_one();
        let s = p.snapshot();
        assert_eq!((s.stage, s.done, s.total), (MountStage::Opening, 1, 4));
        assert_eq!(s.current, "hl2_textures.vpk");
        assert_eq!(s.fraction(), Some(0.25));
        p.set_stage(MountStage::Indexing);
        assert_eq!(p.snapshot().fraction(), Some(1.0));
        p.begin_opening(0);
        assert_eq!(p.snapshot().fraction(), None, "nothing to open: unknown");
    }
}
