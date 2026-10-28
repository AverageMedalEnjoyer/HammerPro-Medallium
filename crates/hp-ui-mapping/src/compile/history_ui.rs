use std::sync::mpsc::{self, Receiver, TryRecvError};

use egui::{RichText, Ui};
use hp_app::{AppState, t, tf, theme};
use hp_compile::StepKind;
use hp_compile::history::{self, HistoryEntry};

use super::history_file;
use crate::util::fmt_duration_ms;

const SHOWN: usize = 60;

pub(crate) type SlowSteps = Vec<(StepKind, String, u64, u64)>;

#[derive(Default)]
pub(crate) struct HistoryView {
    key: Option<String>,
    rx: Option<Receiver<Vec<HistoryEntry>>>,
    entries: Vec<HistoryEntry>,
    stale: bool,
    pub pending_slow: Option<Receiver<SlowSteps>>,
    pub slow: SlowSteps,
}

fn ago(when: u64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let s = now.saturating_sub(when);
    if s < 60 {
        t!("compile.history.just_now").to_owned()
    } else if s < 3600 {
        tf!("compile.history.min_ago", s / 60)
    } else if s < 86_400 {
        tf!("compile.history.h_ago", s / 3600)
    } else {
        tf!("compile.history.d_ago", s / 86_400)
    }
}

impl HistoryView {
    pub(crate) fn poll(&mut self) {
        let Some(rx) = &self.pending_slow else { return };
        match rx.try_recv() {
            Ok(slow) => {
                self.slow = slow;
                self.pending_slow = None;
                self.stale = true;
            }
            Err(TryRecvError::Disconnected) => self.pending_slow = None,
            Err(TryRecvError::Empty) => {}
        }
    }

    fn load(&mut self, ctx: &egui::Context, state: &AppState) {
        let key = hp_app::sidecar::map_key(state.doc.path());
        if self.key.as_deref() == Some(key.as_str()) && !self.stale {
            return;
        }
        if self.rx.is_some() {
            return;
        }
        self.stale = false;
        let file = history_file(&key);
        self.key = Some(key);
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-compile-history-read".into()).spawn(move || {
            let _ = tx.send(history::load(&file));
            repaint.request_repaint();
        });
        if spawned.is_ok() {
            self.rx = Some(rx);
        }
    }

    pub(crate) fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.load(ui.ctx(), state);
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(e) => {
                    self.entries = e;
                    self.rx = None;
                }
                Err(TryRecvError::Disconnected) => self.rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if !state.settings.compile.history {
            ui.label(RichText::new(t!("compile.history.off")).weak());
        }
        if self.entries.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new(t!("compile.history.empty")).weak());
            });
            return;
        }
        let shown: Vec<&HistoryEntry> = self.entries.iter().rev().take(SHOWN).collect();
        let mut series: Vec<((StepKind, String), Vec<u64>)> = Vec::new();
        for e in shown.iter().rev() {
            for s in e.steps.iter().filter(|s| s.ok && matches!(s.kind, StepKind::Bsp | StepKind::Rad)) {
                let k = (s.kind, s.mode.clone());
                match series.iter_mut().find(|(key, _)| *key == k) {
                    Some((_, v)) => v.push(s.ms),
                    None => series.push((k, vec![s.ms])),
                }
            }
        }
        ui.horizontal_wrapped(|ui| {
            for ((kind, mode), values) in series.iter().filter(|(_, v)| v.len() >= 2) {
                ui.vertical(|ui| {
                    let median = history::median_ms(&self.entries, *kind, mode).unwrap_or(0);
                    let last = values.last().copied().unwrap_or(0);
                    ui.label(
                        RichText::new(tf!(
                            "compile.history.series",
                            super::step_name(*kind),
                            mode,
                            fmt_duration_ms(last),
                            fmt_duration_ms(median)
                        ))
                        .small(),
                    );
                    sparkline(ui, values, median);
                });
                ui.add_space(12.0);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().id_salt("hp_compile_history").auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("hp_compile_history_grid").num_columns(6).striped(true).spacing([12.0, 3.0]).show(
                ui,
                |ui| {
                    for h in [
                        "compile.history.when",
                        "compile.history.preset",
                        "compile.step.bsp",
                        "compile.step.rad",
                        "compile.history.total",
                        "compile.history.portals",
                    ] {
                        let text =
                            if h.starts_with("compile.") { hp_app::i18n::tr(h).to_owned() } else { h.to_owned() };
                        ui.label(RichText::new(text).strong());
                    }
                    ui.end_row();
                    for e in &shown {
                        let (icon, color) = if e.success {
                            ("✔", theme::success())
                        } else if e.leaked {
                            ("💧", theme::error())
                        } else {
                            ("❌", theme::error())
                        };
                        ui.label(RichText::new(format!("{icon} {}", ago(e.when))).color(color));
                        let auto = if e.auto {
                            format!("{} ({})", e.preset, t!("compile.history.auto"))
                        } else {
                            e.preset.clone()
                        };
                        ui.label(auto);
                        for kind in [StepKind::Bsp, StepKind::Rad] {
                            let text: Vec<String> = e
                                .steps
                                .iter()
                                .filter(|s| s.kind == kind)
                                .map(|s| format!("{} {}", fmt_duration_ms(s.ms), s.mode))
                                .collect();
                            ui.label(RichText::new(text.join(", ")).monospace().small());
                        }
                        ui.label(RichText::new(fmt_duration_ms(e.total_ms)).monospace());
                        ui.label(
                            e.portalclusters
                                .map(|c| tf!("compile.history.clusters", c, e.numportals.unwrap_or(0)))
                                .unwrap_or_default(),
                        );
                        ui.end_row();
                    }
                },
            );
        });
    }
}

fn sparkline(ui: &mut Ui, values: &[u64], median: u64) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(160.0, 28.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
    let max = values.iter().copied().max().unwrap_or(1).max(median).max(1) as f32;
    let n = values.len().max(2) - 1;
    let point = |i: usize, v: u64| {
        egui::pos2(
            rect.left() + 2.0 + (rect.width() - 4.0) * i as f32 / n as f32,
            rect.bottom() - 2.0 - (rect.height() - 4.0) * v as f32 / max,
        )
    };
    let my = point(0, median).y;
    painter.line_segment(
        [egui::pos2(rect.left(), my), egui::pos2(rect.right(), my)],
        egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
    );
    let pts: Vec<egui::Pos2> = values.iter().enumerate().map(|(i, v)| point(i, *v)).collect();
    painter.add(egui::Shape::line(pts.clone(), egui::Stroke::new(1.5, theme::accent())));
    if let (Some(&p), Some(&v)) = (pts.last(), values.last()) {
        let slow = median > 0 && v as f64 >= history::SLOW_FACTOR * median as f64;
        painter.circle_filled(p, 2.5, if slow { theme::warning() } else { theme::accent() });
    }
}
