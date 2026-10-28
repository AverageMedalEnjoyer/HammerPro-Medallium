use egui::{PointerButton, Pos2};
use hp_app::ViewInput;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerEvent {
    Hover(Pos2),
    DragStart { origin: Pos2, pos: Pos2 },
    Drag(Pos2),
    DragEnd(Pos2),
    Click(Pos2),
    DoubleClick(Pos2),
    Leave,
}

pub fn events(input: &ViewInput<'_>) -> Vec<PointerEvent> {
    let r = input.response;
    let pos = input.pointer.or_else(|| r.interact_pointer_pos()).or_else(|| r.hover_pos());
    let mut out = Vec::with_capacity(2);
    let Some(pos) = pos else {
        out.push(PointerEvent::Leave);
        return out;
    };
    if input.navigating {
        out.push(PointerEvent::Hover(pos));
        return out;
    }
    let mut gesture = false;
    if r.drag_started_by(PointerButton::Primary) {
        let origin = r.ctx.input(|i| i.pointer.press_origin()).unwrap_or(pos);
        out.push(PointerEvent::DragStart { origin, pos });
        gesture = true;
    } else if r.dragged_by(PointerButton::Primary) {
        out.push(PointerEvent::Drag(pos));
        gesture = true;
    }
    if r.drag_stopped_by(PointerButton::Primary) {
        out.push(PointerEvent::DragEnd(pos));
        gesture = true;
    }
    if r.double_clicked_by(PointerButton::Primary) || r.triple_clicked_by(PointerButton::Primary) {
        out.push(PointerEvent::DoubleClick(pos));
        gesture = true;
    } else if r.clicked_by(PointerButton::Primary) {
        out.push(PointerEvent::Click(pos));
        gesture = true;
    }
    if !gesture {
        out.push(PointerEvent::Hover(pos));
    }
    out
}

pub fn primary_down(input: &ViewInput<'_>) -> bool {
    input.response.ctx.input(|i| i.pointer.primary_down())
}

pub fn ctrl(m: egui::Modifiers) -> bool {
    m.command || m.ctrl
}

#[cfg(test)]
mod tests {
    use egui::{Event, Modifiers, Pos2, RawInput, Rect, Sense, vec2};
    use hp_app::view::{Camera2d, Camera3d, ViewInfo, ViewKind};

    use super::*;

    fn frame(ctx: &egui::Context, time: f64, raw_events: Vec<Event>) -> Vec<PointerEvent> {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
            time: Some(time),
            events: raw_events,
            ..Default::default()
        };
        let mut seen = Vec::new();
        let mut out = ctx.run_ui(input, |ui| {
            let (rect, response) = ui.allocate_exact_size(vec2(800.0, 600.0), Sense::click_and_drag());
            let view = ViewInfo {
                index: 1,
                kind: ViewKind::Top,
                rect,
                cam3d: Camera3d::default(),
                cam2d: Camera2d::default(),
                pixels_per_point: 1.0,
                grid: 16.0,
            };
            let input = ViewInput {
                view: &view,
                response: &response,
                pointer: response.hover_pos(),
                modifiers: Modifiers::NONE,
                navigating: false,
            };
            seen = events(&input);
        });
        out.textures_delta.clear();
        seen
    }

    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
    }

    #[test]
    fn a_quick_third_click_is_still_a_double_click() {
        let ctx = egui::Context::default();
        let (a, b) = (Pos2::new(100.0, 100.0), Pos2::new(300.0, 200.0));
        let mut time = 0.0;
        let mut step = |events: Vec<Event>| {
            time += 0.03;
            frame(&ctx, time, events)
        };
        step(vec![Event::PointerMoved(a)]);
        step(vec![button(a, true)]);
        assert_eq!(step(vec![button(a, false)]), vec![PointerEvent::Click(a)]);
        step(vec![Event::PointerMoved(b)]);
        step(vec![button(b, true)]);
        step(vec![button(b, false)]);
        step(vec![button(b, true)]);
        let last = step(vec![button(b, false)]);
        assert_eq!(last, vec![PointerEvent::DoubleClick(b)], "the second click on the second object");
    }
}
