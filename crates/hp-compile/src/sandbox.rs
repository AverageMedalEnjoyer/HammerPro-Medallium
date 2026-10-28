use std::sync::atomic::{AtomicBool, Ordering};

static SANDBOXED: AtomicBool = AtomicBool::new(false);

pub fn set_sandboxed(on: bool) {
    SANDBOXED.store(on, Ordering::SeqCst);
}

pub fn is_sandboxed() -> bool {
    SANDBOXED.load(Ordering::SeqCst)
}

pub const REFUSED: &str = "skipped: automated runs never write into the game folder or start the game";
