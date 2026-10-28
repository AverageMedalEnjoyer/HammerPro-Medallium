use std::cell::Cell;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadTimings {
    pub bytes: u64,
    pub read: Duration,
    pub decode: Duration,
    pub scan: Duration,
    pub build: Duration,
}

impl LoadTimings {
    pub fn total(&self) -> Duration {
        self.read + self.decode + self.scan + self.build
    }
}

thread_local! {
    static LAST: Cell<LoadTimings> = const { Cell::new(LoadTimings {
        bytes: 0,
        read: Duration::ZERO,
        decode: Duration::ZERO,
        scan: Duration::ZERO,
        build: Duration::ZERO,
    }) };
}

pub(crate) fn reset() {
    LAST.with(|c| c.set(LoadTimings::default()));
}

pub(crate) fn record(f: impl FnOnce(&mut LoadTimings)) {
    LAST.with(|c| {
        let mut t = c.get();
        f(&mut t);
        c.set(t);
    });
}

pub(crate) fn last() -> LoadTimings {
    LAST.with(Cell::get)
}

pub(crate) fn read_file(path: &Path) -> std::io::Result<Vec<u8>> {
    std::fs::read(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_report_their_phases_on_their_thread() {
        let dir = std::env::temp_dir().join(format!("hp_vmf_timings_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.vmf");
        let text = crate::write(&crate::Map::new_empty());
        std::fs::write(&path, &text).unwrap();
        assert_eq!(crate::last_load_timings(), LoadTimings::default());
        crate::load(&path).unwrap();
        let t = crate::last_load_timings();
        assert_eq!(t.bytes, text.len() as u64);
        assert!(t.total() >= t.read);
        let other = std::thread::spawn(crate::last_load_timings).join().unwrap();
        assert_eq!(other, LoadTimings::default());
        assert!(crate::load(&dir.join("missing.vmf")).is_err());
        assert_eq!(crate::last_load_timings(), LoadTimings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
