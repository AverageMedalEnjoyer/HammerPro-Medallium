use std::collections::VecDeque;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use crossbeam_channel::{Receiver, Sender};
use hp_app::Severity;

const RECENT_LINES: usize = 200;

fn recent() -> &'static Mutex<VecDeque<String>> {
    static RECENT: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
    RECENT.get_or_init(|| Mutex::new(VecDeque::with_capacity(RECENT_LINES)))
}

pub fn recent_lines() -> Vec<String> {
    match recent().lock() {
        Ok(lines) => lines.iter().cloned().collect(),
        Err(poisoned) => poisoned.into_inner().iter().cloned().collect(),
    }
}

fn remember(line: &str) {
    let mut lines = match recent().lock() {
        Ok(l) => l,
        Err(poisoned) => poisoned.into_inner(),
    };
    if lines.len() >= RECENT_LINES {
        lines.pop_front();
    }
    lines.push_back(line.to_owned());
}

struct Logger {
    tx: Sender<(Severity, String)>,
    file: Option<Mutex<std::fs::File>>,
}

fn is_ours(target: &str) -> bool {
    target.starts_with("hp") || target.starts_with("hammerpro")
}

fn to_console(target: &str, level: log::Level) -> bool {
    is_ours(target) && level <= log::Level::Warn
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= if is_ours(metadata.target()) { log::Level::Info } else { log::Level::Warn }
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let text = format!("{}", record.args());
        let line = format!("[{}] {}: {}", record.level(), record.target(), text);
        eprintln!("{line}");
        remember(&line);
        if let Some(file) = &self.file
            && let Ok(mut f) = file.lock()
        {
            let _ = writeln!(f, "{line}");
        }
        if to_console(record.target(), record.level()) {
            let severity = if record.level() == log::Level::Error { Severity::Error } else { Severity::Warning };
            let _ = self.tx.try_send((severity, text));
        }
    }

    fn flush(&self) {
        if let Some(file) = &self.file
            && let Ok(mut f) = file.lock()
        {
            let _ = f.flush();
        }
    }
}

fn open_log_file() -> Option<std::fs::File> {
    let dir = hp_app::settings::Settings::log_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("hammerpro.log");
    if path.exists() {
        let _ = std::fs::rename(&path, dir.join("hammerpro.old.log"));
    }
    std::fs::File::create(path).ok()
}

pub fn init(to_file: bool) -> Receiver<(Severity, String)> {
    let (tx, rx) = crossbeam_channel::bounded(4096);
    let file = if to_file { open_log_file().map(Mutex::new) } else { None };
    if log::set_boxed_logger(Box::new(Logger { tx, file })).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_warnings_reach_the_console() {
        assert!(!to_console("hp_fs::fs", log::Level::Info), "English diagnostics stay out of the console");
        assert!(to_console("hp_fs::fs", log::Level::Warn));
        assert!(to_console("hammerpro::files", log::Level::Error));
        assert!(!to_console("wgpu_core", log::Level::Error));
    }

    #[test]
    fn recent_lines_are_bounded() {
        for i in 0..(RECENT_LINES + 10) {
            remember(&format!("line {i}"));
        }
        let lines = recent_lines();
        assert!(lines.len() <= RECENT_LINES);
        assert!(lines.last().is_some_and(|l| l.starts_with("line ")));
    }
}
