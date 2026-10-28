use std::time::Duration;

use egui::{Align2, Id, Order, RichText};
use hp_app::{AppState, t};

use crate::common::severity_color;

const TOAST_WIDTH: f32 = 320.0;
const FADE_SECS: f32 = 0.6;
const FADE_IN_SECS: f32 = 0.15;

pub fn toast_opacity(age: f32, life: f32) -> f32 {
    if !(age.is_finite() && life.is_finite()) || life <= 0.0 {
        return 0.0;
    }
    let fade_in = (age / FADE_IN_SECS).clamp(0.0, 1.0);
    let remaining = life - age;
    let fade_out = (remaining / FADE_SECS).clamp(0.0, 1.0);
    fade_in.min(fade_out)
}

pub fn next_repaint(ages_and_lives: impl IntoIterator<Item = (f32, f32)>) -> Option<Duration> {
    let mut next: Option<f32> = None;
    for (age, life) in ages_and_lives {
        let fade_start = life - FADE_SECS;
        let wait = if age < FADE_IN_SECS || age >= fade_start { 0.0 } else { fade_start - age };
        next = Some(next.map_or(wait, |n: f32| n.min(wait)));
    }
    next.map(|s| Duration::from_secs_f32(s.max(0.0)))
}

const CONSOLE_LINES_ON_SCREEN: usize = 8;

pub fn repeats_console(state: &AppState, toast: &hp_app::console::Toast) -> bool {
    state.settings.ui.show_console
        && matches!(toast.severity, hp_app::Severity::Info | hp_app::Severity::Success)
        && toast.key.is_none()
        && state.console.lines().rev().take(CONSOLE_LINES_ON_SCREEN).any(|l| l.text == toast.text)
}

pub fn toasts(ctx: &egui::Context, state: &mut AppState) {
    let all = state.toasts.live().to_vec();
    let live: Vec<(usize, hp_app::console::Toast)> =
        all.into_iter().enumerate().filter(|(_, t)| !repeats_console(state, t)).collect();
    if live.is_empty() {
        return;
    }
    let timing: Vec<(f32, f32)> =
        live.iter().map(|(_, t)| (t.created.elapsed().as_secs_f32(), t.duration.as_secs_f32())).collect();
    if let Some(wait) = next_repaint(timing.iter().copied()) {
        if wait.is_zero() {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(wait);
        }
    }
    let mut dismiss = None;
    egui::Area::new(Id::unique("hp_shell_toasts"))
        .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-14.0, -34.0))
        .order(Order::Foreground)
        .interactable(true)
        .show(ctx, |ui| {
            ui.set_width(TOAST_WIDTH);
            ui.spacing_mut().item_spacing.y = 6.0;
            for (i, (queue_index, toast)) in live.iter().enumerate() {
                let (age, life) = timing.get(i).copied().unwrap_or((0.0, 1.0));
                let opacity = toast_opacity(age, life);
                let r = ui
                    .scope(|ui| {
                        ui.multiply_opacity(opacity);
                        let frame = egui::Frame::popup(ui.style()).inner_margin(egui::Margin {
                            left: 14,
                            right: 10,
                            top: 8,
                            bottom: 8,
                        });
                        let inner = frame.show(ui, |ui| {
                            let width = TOAST_WIDTH - 26.0;
                            ui.set_width(width);
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, 0.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    let color = severity_color(ui.visuals(), toast.severity);
                                    ui.label(RichText::new(toast.severity.icon()).size(16.0).color(color));
                                    ui.add(egui::Label::new(RichText::new(&toast.text)).wrap());
                                },
                            );
                        });
                        let rect = inner.response.rect;
                        let strip =
                            egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + 4.0, rect.bottom()));
                        let color = severity_color(ui.visuals(), toast.severity).gamma_multiply(opacity);
                        ui.painter().rect_filled(strip, egui::CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 }, color);
                        ui.interact(rect, Id::unique(("hp_shell_toast", i, toast.created)), egui::Sense::CLICK)
                    })
                    .inner;
                let r = r.on_hover_text(t!("shell.toast.dismiss"));
                if r.clicked() {
                    dismiss = Some(*queue_index);
                }
            }
        });
    if let Some(i) = dismiss {
        state.toasts.dismiss(i);
    }
}

pub(crate) const TR: hp_app::i18n::TrTable = &[("shell.toast.dismiss", "Click to dismiss", "Cliquez pour fermer")];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_curve() {
        assert_eq!(toast_opacity(0.0, 3.0), 0.0);
        assert!((toast_opacity(1.0, 3.0) - 1.0).abs() < 1e-6);
        assert!(toast_opacity(2.7, 3.0) < 1.0 && toast_opacity(2.7, 3.0) > 0.0);
        assert_eq!(toast_opacity(3.5, 3.0), 0.0);
        assert_eq!(toast_opacity(f32::NAN, 3.0), 0.0);
    }

    #[test]
    fn routine_messages_are_not_repeated_over_the_console() {
        let mut state = hp_app::testing::test_state();
        state.notify(hp_app::Severity::Info, "Copied the id 42");
        state.notify(hp_app::Severity::Success, "Opened map.vmf in 12 ms");
        state.notify(hp_app::Severity::Warning, "Nothing to paste");
        state.toasts.push_keyed("speed", hp_app::Severity::Info, "Camera speed 400");
        let live = state.toasts.live().to_vec();
        let shown: Vec<&str> = live.iter().filter(|t| !repeats_console(&state, t)).map(|t| t.text.as_str()).collect();
        assert_eq!(shown, ["Nothing to paste", "Camera speed 400"], "the console shows the routine line");
        state.settings.ui.show_console = false;
        let shown = live.iter().filter(|t| !repeats_console(&state, t)).count();
        assert_eq!(shown, 4, "console hidden: every toast shows");
    }

    #[test]
    fn repaint_schedule() {
        assert_eq!(next_repaint(std::iter::empty()), None);
        let d = next_repaint([(1.0, 3.0)]);
        assert!(d.is_some_and(|d| (d.as_secs_f32() - (3.0 - FADE_SECS - 1.0)).abs() < 1e-3), "{d:?}");
        assert_eq!(next_repaint([(1.0, 3.0), (2.9, 3.0)]), Some(Duration::ZERO));
    }
}
