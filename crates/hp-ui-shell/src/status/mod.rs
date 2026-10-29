use egui::Ui;
use hp_app::AppState;
use hp_app::extend::{StatusContext, StatusSegment, StatusSide};
use hp_app::i18n::TrTable;

mod compile;
mod health;
mod launch;
mod lighting;
mod shell;
mod startup;

pub use shell::{SIZE_LIMIT, StatusCache, fmt_len, fmt_point, fmt_size};
#[cfg(test)]
pub(crate) use startup::late_text;

pub(crate) const TABLES: &[TrTable] =
    &[shell::TR, startup::TR, compile::TR, health::TR, launch::TR, lighting::TR];

fn default_segments() -> Vec<Box<dyn StatusSegment>> {
    let mut out = Vec::new();
    out.extend(shell::segments());
    out.extend(startup::segments());
    out.extend(compile::segments());
    out.extend(health::segments());
    out.extend(launch::segments());
    out.extend(lighting::segments());
    out
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusOutput {
    pub open_settings: Option<&'static str>,
}

pub struct StatusBar {
    segments: Vec<Box<dyn StatusSegment>>,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self { segments: default_segments() }
    }
}

impl std::fmt::Debug for StatusBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatusBar").field("segments", &self.segment_ids()).finish()
    }
}

impl StatusBar {
    pub fn add(&mut self, segment: Box<dyn StatusSegment>) {
        if !self.segments.iter().any(|s| s.id() == segment.id()) {
            self.segments.push(segment);
        }
    }

    pub fn segment_ids(&self) -> Vec<&'static str> {
        self.segments.iter().map(|s| s.id()).collect()
    }

    fn side(&self, side: StatusSide) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.segments.len()).filter(|&i| self.segments[i].side() == side).collect();
        idx.sort_by_key(|&i| self.segments[i].order());
        idx
    }

    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState, tool_hint: Option<String>) -> StatusOutput {
        let mut cx = StatusContext { tool_hint, open_settings: None };
        let right = self.side(StatusSide::Right);
        let left = self.side(StatusSide::Left);
        let segments = &mut self.segments;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for &i in &right {
                    segments[i].ui(ui, state, &mut cx);
                }
                ui.separator();
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    for (n, &i) in left.iter().enumerate() {
                        if n > 0 {
                            ui.separator();
                        }
                        let width = match segments[i].width() {
                            Some(w) => w,
                            None => {
                                let after: f32 = left[n + 1..].iter().filter_map(|&j| segments[j].width()).sum();
                                (ui.available_width() - after).max(80.0)
                            }
                        };
                        ui.allocate_ui_with_layout(
                            egui::vec2(width, ui.available_height()),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_min_width(width);
                                segments[i].ui(ui, state, &mut cx);
                            },
                        );
                    }
                });
            });
        });
        StatusOutput { open_settings: cx.open_settings }
    }
}
