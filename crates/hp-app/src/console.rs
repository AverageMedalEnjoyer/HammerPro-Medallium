use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Success,
    Warning,
    Error,
}

impl Severity {
    pub fn color(self) -> egui::Color32 {
        match self {
            Severity::Info => crate::theme::info(),
            Severity::Success => crate::theme::success(),
            Severity::Warning => crate::theme::warning(),
            Severity::Error => crate::theme::error(),
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Severity::Info => "ℹ",
            Severity::Success => "✔",
            Severity::Warning => "⚠",
            Severity::Error => "❌",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConsoleLine {
    pub severity: Severity,
    pub text: String,
    pub time: f64,
}

#[derive(Debug)]
pub struct Console {
    lines: VecDeque<ConsoleLine>,
    capacity: usize,
    start: Instant,
    pub revision: u64,
}

impl Default for Console {
    fn default() -> Self {
        Self { lines: VecDeque::with_capacity(1024), capacity: 5000, start: Instant::now(), revision: 0 }
    }
}

impl Console {
    pub fn push(&mut self, severity: Severity, text: impl Into<String>) {
        if self.lines.len() >= self.capacity {
            self.lines.pop_front();
        }
        let time = self.start.elapsed().as_secs_f64();
        self.lines.push_back(ConsoleLine { severity, text: text.into(), time });
        self.revision += 1;
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.push(Severity::Info, text);
    }
    pub fn success(&mut self, text: impl Into<String>) {
        self.push(Severity::Success, text);
    }
    pub fn warn(&mut self, text: impl Into<String>) {
        self.push(Severity::Warning, text);
    }
    pub fn error(&mut self, text: impl Into<String>) {
        self.push(Severity::Error, text);
    }

    pub fn lines(&self) -> impl ExactSizeIterator<Item = &ConsoleLine> + DoubleEndedIterator {
        self.lines.iter()
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.revision += 1;
    }
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub severity: Severity,
    pub text: String,
    pub created: Instant,
    pub duration: Duration,
    pub key: Option<&'static str>,
}

#[derive(Debug, Default)]
pub struct Toasts {
    items: Vec<Toast>,
}

impl Toasts {
    pub fn push(&mut self, severity: Severity, text: impl Into<String>) {
        let duration = match severity {
            Severity::Error => Duration::from_secs(8),
            Severity::Warning => Duration::from_secs(6),
            _ => Duration::from_secs(3),
        };
        self.items.push(Toast { severity, text: text.into(), created: Instant::now(), duration, key: None });
        if self.items.len() > 6 {
            self.items.remove(0);
        }
    }

    pub fn push_keyed(&mut self, key: &'static str, severity: Severity, text: impl Into<String>) {
        self.items.retain(|t| t.key != Some(key));
        self.push(severity, text);
        if let Some(t) = self.items.last_mut() {
            t.key = Some(key);
            t.duration = Duration::from_millis(1500);
        }
    }

    pub fn live(&mut self) -> &[Toast] {
        let now = Instant::now();
        self.items.retain(|t| now.duration_since(t.created) < t.duration);
        &self.items
    }

    pub fn dismiss(&mut self, index: usize) {
        if index < self.items.len() {
            self.items.remove(index);
        }
    }
}
