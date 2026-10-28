use egui::{Align, Color32, Layout, Rect, Sense, Stroke, StrokeKind, UiBuilder, vec2};
use hp_app::view::QuadrantView;
use hp_app::{Action, AppState, t};

use crate::paint;

pub trait LogicalContent {
    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut AppState,
        index: usize,
        header: &mut dyn FnMut(&mut egui::Ui, &mut AppState),
    );
}

pub(crate) fn view_ui(
    ui: &mut egui::Ui,
    state: &mut AppState,
    content: Option<&mut (dyn LogicalContent + 'static)>,
    index: usize,
    rect: Rect,
    maximized: bool,
) {
    if ui.rect_contains_pointer(rect) {
        state.ui.hovered_logical = Some(index);
        if ui.input(|i| i.pointer.any_pressed()) {
            state.ui.focused_view = Some(index);
        }
    }
    ui.painter().rect_filled(rect, 0, ui.visuals().panel_fill);
    let inner = rect.shrink(4.0);
    let mut child = ui.new_child(
        UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)).id_salt(("hp_logical_view", index)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    let mut header = |ui: &mut egui::Ui, state: &mut AppState| header_ui(ui, state, index, maximized);
    match content {
        Some(content) => content.ui(&mut child, state, index, &mut header),
        None => {
            child.horizontal(|ui| header(ui, state));
            paint::placeholder(
                &child.painter_at(inner),
                Rect::from_min_max(inner.min + vec2(0.0, 28.0), inner.max),
                child.visuals().extreme_bg_color,
                t!(QuadrantView::Logical.label_key()),
                t!("viewport.logical_none"),
            );
        }
    }
    let focused = state.ui.focused_view == Some(index);
    let border = if focused { hp_app::theme::ACCENT.gamma_multiply(0.55) } else { Color32::from_white_alpha(14) };
    ui.painter().rect_stroke(rect, 0, Stroke::new(1.0, border), StrokeKind::Inside);
}

fn header_ui(ui: &mut egui::Ui, state: &mut AppState, index: usize, maximized: bool) {
    egui::Frame::new().fill(paint::PILL_BG).corner_radius(4).inner_margin(egui::Margin::symmetric(3, 1)).show(
        ui,
        |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let focused = state.ui.focused_view == Some(index);
            let label = crate::view_label(ui, t!(QuadrantView::Logical.label_key()), focused)
                .on_hover_text(t!("viewport.change_view"));
            if label.double_clicked() {
                crate::toggle_maximize(state, index);
                egui::Popup::close_all(ui.ctx());
            }
            egui::Popup::menu(&label).show(|ui| {
                ui.set_min_width(170.0);
                view_type_menu(ui, state, index, QuadrantView::Logical);
            });
            maximize_button(ui, state, index, maximized);
        },
    );
}

pub(crate) fn view_type_menu(ui: &mut egui::Ui, state: &mut AppState, index: usize, current: QuadrantView) {
    ui.label(egui::RichText::new(t!("viewport.view_type")).weak().small());
    for view in QuadrantView::ALL {
        if ui.add(egui::Button::selectable(view == current, t!(view.label_key()))).clicked()
            && let Some(q) = state.settings.ui.quadrants.get_mut(index)
        {
            *q = view.index();
            state.settings_dirty = true;
        }
    }
}

pub(crate) fn maximize_button(ui: &mut egui::Ui, state: &mut AppState, index: usize, maximized: bool) {
    let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::CLICK);
    let hovered = resp.hovered();
    if hovered {
        ui.painter().rect_filled(r, 3, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    let color = if hovered { Color32::WHITE } else { paint::TEXT_WEAK };
    paint::maximize_icon(ui.painter(), r, maximized, color);
    let tip = if maximized {
        t!("viewport.restore").to_owned()
    } else {
        match state.keys.shortcut(Action::MaximizeView) {
            Some(sc) => format!("{} ({})", Action::MaximizeView.label(), hp_app::actions::shortcut_label(&sc)),
            None => Action::MaximizeView.label().to_owned(),
        }
    };
    if resp.on_hover_text(tip).clicked() {
        crate::toggle_maximize(state, index);
    }
}
