use std::cell::RefCell;
use std::rc::Rc;

use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, vec2};
use hp_app::testing::test_state;

use super::*;

#[derive(Default)]
struct CountingTool {
    views: Vec<usize>,
    overlays: RefCell<Vec<usize>>,
}

impl Tool for CountingTool {
    fn id(&self) -> ToolId {
        ToolId::default()
    }

    fn view_input(&mut self, _state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        self.views.push(input.view.index);
        ToolFlow::Handled
    }

    fn overlay(&self, _state: &AppState, view: &ViewInfo, _out: &mut Overlay) {
        self.overlays.borrow_mut().push(view.index);
    }
}

#[derive(Default)]
struct Seen {
    calls: Vec<(usize, Rect)>,
}

struct FakeContent(Rc<RefCell<Seen>>);

impl LogicalContent for FakeContent {
    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut AppState,
        index: usize,
        header: &mut dyn FnMut(&mut egui::Ui, &mut AppState),
    ) {
        ui.horizontal(|ui| header(ui, state));
        self.0.borrow_mut().calls.push((index, ui.max_rect()));
        let (_, response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        if response.clicked() {
            state.doc.select_all();
        }
    }
}

struct H {
    ctx: egui::Context,
    vp: Viewports,
    state: AppState,
    tool: CountingTool,
    time: f64,
    shapes: Vec<egui::epaint::ClippedShape>,
}

impl H {
    fn new() -> Self {
        register_translations();
        let mut h = Self {
            ctx: egui::Context::default(),
            vp: Viewports::new(),
            state: test_state(),
            tool: CountingTool::default(),
            time: 0.0,
            shapes: Vec::new(),
        };
        h.frame(vec![]);
        h.frame(vec![]);
        h
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        self.state.frame.dt = 1.0 / 60.0;
        let input = RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))),
            time: Some(self.time),
            events,
            focused: true,
            ..Default::default()
        };
        let out = self.ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| self.vp.ui(ui, &mut self.state, &mut self.tool));
        });
        self.shapes = out.shapes;
    }

    fn click(&mut self, p: Pos2, button: PointerButton) {
        let press = |pressed| Event::PointerButton { pos: p, button, pressed, modifiers: Modifiers::NONE };
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![press(true)]);
        self.frame(vec![press(false)]);
        self.frame(vec![]);
    }

    fn text_at(&self, text: &str) -> Option<Pos2> {
        self.shapes.iter().find_map(|c| match &c.shape {
            egui::Shape::Text(t) if t.galley.text() == text => Some(t.visual_bounding_rect().center()),
            _ => None,
        })
    }
}

#[test]
fn view_type_menu_offers_the_logical_view() {
    let mut h = H::new();
    let top = h.vp.view_rects()[1].expect("a top view").1;
    h.click(top.min + vec2(14.0, 14.0), PointerButton::Primary);
    let label = t!("view.logical");
    let item = h.text_at(label).unwrap_or_else(|| panic!("the view type menu lists \"{label}\""));
    for kind in ViewKind::ALL {
        assert!(h.text_at(t!(kind.label_key())).is_some(), "{kind:?} is listed too");
    }
    h.click(item, PointerButton::Primary);
    assert_eq!(QuadrantView::from_index(h.state.settings.ui.quadrants[1]), QuadrantView::Logical);
    assert!(h.state.settings_dirty, "the choice is saved with the settings");
    h.frame(vec![]);
    assert!(h.vp.view_rects()[1].is_none(), "no projected view in that quadrant");
    let logical = h.vp.logical_rects()[1].expect("the logical view is shown");
    assert!(logical.contains_rect(top) && (logical.size() - top.size()).length() <= 1.5, "{logical:?} {top:?}");
    assert!(h.vp.view_size(ViewKind::Top).is_none(), "the top view is not shown any more");
    h.click(top.min + vec2(14.0, 14.0), PointerButton::Primary);
    let back = h.text_at(t!("view.top")).expect("the logical view's menu lists the top view");
    h.click(back, PointerButton::Primary);
    assert_eq!(QuadrantView::from_index(h.state.settings.ui.quadrants[1]), QuadrantView::View(ViewKind::Top));
    h.frame(vec![]);
    assert_eq!(h.vp.view_rects()[1].map(|v| v.0), Some(ViewKind::Top));
    assert!(h.vp.logical_rects().iter().all(Option::is_none));
}

#[test]
fn logical_quadrant_has_no_projection_and_no_tool_input() {
    let mut h = H::new();
    let seen = Rc::new(RefCell::new(Seen::default()));
    h.vp.set_logical_content(Box::new(FakeContent(seen.clone())));
    h.state.settings.ui.quadrants[2] = QuadrantView::LOGICAL_INDEX;
    h.frame(vec![]);
    let rect = h.vp.logical_rects()[2].expect("the logical view is shown");
    assert!(h.vp.view_rects()[2].is_none());
    assert!(seen.borrow().calls.iter().any(|(i, r)| *i == 2 && rect.contains_rect(*r)), "the content draws it");
    h.tool.overlays.borrow_mut().clear();
    h.frame(vec![]);
    let drawn = h.tool.overlays.borrow().clone();
    assert!(!drawn.is_empty() && !drawn.contains(&2), "overlays for the other views only: {drawn:?}");
    h.tool.views.clear();
    let c = rect.center();
    h.frame(vec![Event::PointerMoved(c)]);
    assert_eq!(h.state.ui.hovered_view, None, "no projected view under the pointer");
    assert_eq!(h.state.ui.hovered_logical, Some(2), "the view type keys and Maximize act on it");
    assert_eq!(h.state.hover_world, None, "no world position in the logical view");
    h.click(c, PointerButton::Primary);
    assert!(!h.state.doc.selected_objects().is_empty(), "the content got the click");
    assert_eq!(h.state.ui.focused_view, Some(2), "the logical view becomes the last view used");
    h.click(c, PointerButton::Secondary);
    h.click(c, PointerButton::Middle);
    h.frame(vec![Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: vec2(0.0, 2.0),
        modifiers: Modifiers::NONE,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
    }]);
    let press = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    h.frame(vec![press(c, true)]);
    for k in 1..6 {
        h.frame(vec![Event::PointerMoved(c + vec2(10.0 * k as f32, 5.0))]);
    }
    h.frame(vec![press(c + vec2(50.0, 25.0), false)]);
    assert!(
        h.tool.views.iter().all(|&v| v != 2),
        "the tool never gets input from the logical view: {:?}",
        h.tool.views
    );
    assert!(h.vp.menu.is_none(), "no viewport context menu in the logical view");
    h.state.ui.maximized_view = Some(2);
    h.frame(vec![]);
    assert_eq!(h.vp.logical_rects()[2].map(|r| r.width() > rect.width()), Some(true));
    assert!(h.vp.view_rects().iter().all(Option::is_none));
    h.frame(vec![Event::Key {
        key: egui::Key::ArrowLeft,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    zoom_all_2d(&mut h.state, 2.0);
    h.frame(vec![]);
}

#[test]
fn logical_view_without_content_shows_a_placeholder() {
    let mut h = H::new();
    h.state.settings.ui.quadrants = [QuadrantView::LOGICAL_INDEX; 4];
    h.frame(vec![]);
    assert!(h.vp.view_rects().iter().all(Option::is_none));
    assert!(h.vp.logical_rects().iter().all(Option::is_some));
    assert!(h.text_at(t!("viewport.logical_none")).is_some());
    assert!(h.text_at(t!("view.logical")).is_some(), "the header names the view");
}
