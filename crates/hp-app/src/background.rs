//! Keeps background work from competing with the rest of the computer, audio in particular.

use std::sync::atomic::{AtomicBool, Ordering};

static ACTIVE: AtomicBool = AtomicBool::new(true);

/// Set by the main window every frame: `true` while Hammer Pro is the window being used.
pub fn set_app_active(active: bool) {
    ACTIVE.store(active, Ordering::Relaxed);
}

/// `false` while Hammer Pro is minimised or another program is in front. Work that only
/// matters while editing (watching files, polling folders) waits until it is `true` again.
pub fn app_active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Runs the calling thread slightly below normal priority so it never pushes aside the
/// audio of other programs or the editor's own window.
pub fn lower_current_thread() {
    #[cfg(windows)]
    // SAFETY: both calls act on the calling thread only.
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

/// Runs the calling thread in Windows' background mode: lowest CPU, disk and memory priority.
/// For threads that only poll or prepare things nobody is waiting for.
pub fn background_current_thread() {
    #[cfg(windows)]
    // SAFETY: both calls act on the calling thread only.
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN};
        SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
    }
}
