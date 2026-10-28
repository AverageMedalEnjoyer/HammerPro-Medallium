use std::cell::RefCell;
use std::rc::Rc;

use egui::{Align2, CursorIcon, Key, Pos2, Ui, Vec2};
use glam::DVec3;
use hp_app::actions::key_label;
use hp_app::state::AppState;
use hp_app::tool::ToolOption;
use hp_app::{Action, Severity, Tool, ToolFlow, ToolId, ToolMenuEntry, ViewInfo, ViewInput, t, tf};
use hp_editor::{Id, ObjRef, SelectOp};
use hp_geom::Aabb;
use hp_render::{Overlay, OverlayLine};

use super::chain::{
    PathClass, PathGraph, UNDO_CLOSE, UNDO_LINK, add_point, class_defaults, insert_after, link, name_after, path_class,
    path_classes, remove_points,
};
use crate::common::{
    COLOR_GHOST, COLOR_PENDING, Mouse, Ov, Snap, clean_vec, consume_plain_key, depth_axis, f3, fresh, ground_point,
    paint_label, pick_surface, rgba,
};
use crate::entity::rest_on_surface_snapped;

const COLOR_CHAIN: egui::Color32 = hp_app::theme::VIEW_ACCENT;
const COLOR_BRANCH: egui::Color32 = hp_app::theme::VIEW_SUCCESS;
const GRAB: f32 = 8.0;
const ARROW_PX: f64 = 11.0;
const MAX_LABELS: usize = 300;

#[derive(Clone, Debug, Default, PartialEq)]
struct Live {
    ids: Vec<Id>,
}

#[derive(Clone, Debug, PartialEq)]
struct Hover {
    view: usize,
    frame: u64,
    pos: Option<DVec3>,
    node: Option<Id>,
    label: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum Drag {
    Move { view: usize, ids: Vec<Id>, start: DVec3, applied: DVec3 },
    Insert { view: usize, from: Id, pos: DVec3 },
    Swallow,
}

#[derive(Default)]
pub struct PathTool {
    chain: Option<Live>,
    hover: Option<Hover>,
    drag: Option<Drag>,
    last_point: Option<DVec3>,
    graph: RefCell<Option<(u64, u64, Rc<PathGraph>)>>,
    classes: RefCell<Option<(u64, Rc<Vec<PathClass>>)>>,
    sel_center: Option<(u64, Option<DVec3>)>,
}

fn pixel_size(view: &ViewInfo, p: DVec3) -> f64 {
    let ppp = f64::from(view.pixels_per_point.max(0.1));
    if view.is_2d() {
        1.0 / (view.cam2d.zoom.max(1e-6) * ppp)
    } else {
        let h = f64::from(view.rect.height().max(1.0)) * ppp;
        let half = (view.cam3d.fov_y_deg.to_radians() * 0.5).tan().max(1e-3);
        let depth = (p - view.cam3d.pos).dot(view.cam3d.forward()).max(view.cam3d.near.max(0.01));
        2.0 * half * depth / h
    }
}

fn arrow(view: &ViewInfo, a: DVec3, b: DVec3, color: [f32; 4], out: &mut Vec<OverlayLine>) {
    let mid = (a + b) * 0.5;
    let d = b - a;
    let toward = if view.is_2d() { view.kind.toward_viewer() } else { (view.cam3d.pos - mid).normalize_or_zero() };
    let flat = if view.is_2d() { d - toward * d.dot(toward) } else { d };
    let Some(dir) = flat.try_normalize() else { return };
    let Some(side) = toward.cross(dir).try_normalize() else { return };
    let size = (pixel_size(view, mid) * ARROW_PX).min(d.length() * 0.45);
    if !(size.is_finite() && size > 0.0) {
        return;
    }
    let tip = mid + dir * size * 0.5;
    let back = tip - dir * size;
    for s in [side, -side] {
        out.push(OverlayLine { a: f3(tip), b: f3(back + s * size * 0.5), color });
    }
}

pub fn chosen_class(state: &AppState) -> PathClass {
    let name = state.settings.entities.path.class_name().to_owned();
    path_class(state.assets.fgd(), &name).unwrap_or(PathClass {
        class: name,
        next_key: "target".to_owned(),
        alt_key: None,
    })
}

fn selected_points(state: &AppState, graph: &PathGraph) -> Vec<Id> {
    state
        .doc
        .selection()
        .objects
        .iter()
        .filter_map(|o| match o {
            ObjRef::Entity(id) if graph.contains(*id) => Some(*id),
            _ => None,
        })
        .collect()
}

pub fn insert_after_selection(state: &mut AppState) -> bool {
    let graph = PathGraph::build(&state.doc, state.assets.fgd());
    let sel = selected_points(state, &graph);
    let [node] = sel.as_slice() else {
        state.notify(Severity::Info, t!("path.not_path"));
        return false;
    };
    let base = state.settings.entities.path.base().to_owned();
    let fgd = state.assets.fgd();
    match insert_after(&mut state.doc, fgd, *node, None, &base) {
        Ok(ins) => {
            report_insert(state, &ins);
            true
        }
        Err(e) => {
            state.notify(Severity::Error, tf!("path.failed", e));
            false
        }
    }
}

fn report_insert(state: &mut AppState, ins: &super::chain::Inserted) {
    if ins.renamed.is_empty() {
        state.log(Severity::Info, tf!("path.inserted", &ins.name));
    } else {
        state.log(Severity::Info, tf!("path.inserted_renamed", &ins.name, ins.renamed.len()));
    }
}

pub fn remove_selected(state: &mut AppState, relink: bool) -> bool {
    let objs = state.doc.selected_objects();
    let graph = PathGraph::build(&state.doc, state.assets.fgd());
    if !objs.iter().any(|o| matches!(o, ObjRef::Entity(id) if graph.contains(*id))) {
        state.notify(Severity::Info, t!("path.not_path"));
        return false;
    }
    let fgd = state.assets.fgd();
    match remove_points(&mut state.doc, fgd, &objs, relink) {
        Ok(n) => {
            state.log(Severity::Info, tf!("path.removed", n));
            true
        }
        Err(e) => {
            state.notify(Severity::Error, tf!("path.failed", e));
            false
        }
    }
}

impl PathTool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn drawing(&self) -> &[Id] {
        self.chain.as_ref().map_or(&[], |c| &c.ids)
    }

    pub fn graph(&self, state: &AppState) -> Rc<PathGraph> {
        let key = (state.doc.revision(), state.assets.generation());
        let mut cache = self.graph.borrow_mut();
        if let Some((r, g, graph)) = cache.as_ref()
            && (*r, *g) == key
        {
            return Rc::clone(graph);
        }
        let graph = Rc::new(PathGraph::build(&state.doc, state.assets.fgd()));
        *cache = Some((key.0, key.1, Rc::clone(&graph)));
        graph
    }

    fn classes(&self, state: &AppState) -> Rc<Vec<PathClass>> {
        let generation = state.assets.generation();
        let mut cache = self.classes.borrow_mut();
        if let Some((g, list)) = cache.as_ref()
            && *g == generation
        {
            return Rc::clone(list);
        }
        let list = Rc::new(path_classes(state.assets.fgd()));
        *cache = Some((generation, Rc::clone(&list)));
        list
    }

    fn prune(&mut self, state: &AppState) {
        if let Some(c) = &mut self.chain {
            c.ids.retain(|id| state.doc.scene().entity(*id).is_some());
            if c.ids.is_empty() {
                self.chain = None;
            }
        }
    }

    fn depth(&mut self, state: &AppState, axis: usize, graph: &PathGraph) -> f64 {
        let chain_last =
            self.chain.as_ref().and_then(|c| c.ids.last()).and_then(|id| graph.node(*id)).map(|n| n.origin);
        let selected = selected_points(state, graph).first().and_then(|id| graph.node(*id)).map(|n| n.origin);
        if let Some(p) = chain_last.or(selected) {
            return p[axis];
        }
        let rev = state.doc.revision();
        let center = match self.sel_center {
            Some((r, c)) if r == rev => c,
            _ => {
                let c = state.doc.selection_center(&|e| state.entity_bounds(e)).filter(|c| c.is_finite());
                self.sel_center = Some((rev, c));
                c
            }
        };
        center.or(self.last_point).map_or(0.0, |c| crate::common::clean(c[axis]))
    }

    fn local_bounds(state: &AppState) -> Aabb {
        let class = state.settings.entities.path.class_name();
        state
            .assets
            .fgd()
            .and_then(|f| f.class(class))
            .and_then(|c| c.size())
            .map(|(a, b)| Aabb::new(DVec3::from_array(a), DVec3::from_array(b)))
            .filter(|b| b.min.is_finite() && b.max.is_finite())
            .unwrap_or_else(|| Aabb::new(DVec3::splat(-8.0), DVec3::splat(8.0)))
    }

    fn place(&mut self, state: &AppState, view: &ViewInfo, pos: Pos2, snap: &Snap, graph: &PathGraph) -> Option<DVec3> {
        if view.is_2d() {
            let p2 = snap.v2(view.screen_to_2d(pos));
            let depth = self.depth(state, depth_axis(view.kind), graph);
            return Some(clean_vec(view.kind.from_2d(p2, depth))).filter(|p| p.is_finite());
        }
        let offset = state.settings.entities.path.offset();
        let local = Self::local_bounds(state);
        let (point, normal, snap) = match pick_surface(state, view, pos) {
            Some(hit) if snap.is_on() => {
                let surface = Some(hp_editor::ObjRef::Solid(hit.solid));
                (hit.point, hit.normal, Snap::on_object(state, view, egui::Modifiers::NONE, surface))
            }
            Some(hit) => (hit.point, hit.normal, *snap),
            None => (ground_point(view, pos)?, DVec3::Z, *snap),
        };
        let p = rest_on_surface_snapped(point, normal, &local, glam::DQuat::IDENTITY, &snap) + normal * offset;
        Some(clean_vec(p)).filter(|p| p.is_finite())
    }

    fn node_at(graph: &PathGraph, view: &ViewInfo, pos: Pos2) -> Option<Id> {
        graph
            .nodes
            .iter()
            .filter(|n| n.visible)
            .filter_map(|n| view.world_to_screen(n.origin).map(|s| (n.id, s.distance(pos))))
            .filter(|(_, d)| *d <= GRAB)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }

    pub fn add(&mut self, state: &mut AppState, p: DVec3) -> Option<Id> {
        self.prune(state);
        let class = chosen_class(state);
        let base = state.settings.entities.path.base().to_owned();
        let fgd = state.assets.fgd();
        let kv = class_defaults(fgd, &class.class);
        let prev = self.chain.as_ref().and_then(|c| c.ids.last().copied());
        match add_point(&mut state.doc, fgd, &class, &kv, prev, &base, p) {
            Ok((id, _)) => {
                self.chain.get_or_insert_with(Live::default).ids.push(id);
                self.last_point = Some(p);
                Some(id)
            }
            Err(e) => {
                state.notify(Severity::Error, tf!("path.failed", e));
                None
            }
        }
    }

    pub fn end(&mut self, state: &mut AppState) -> bool {
        self.prune(state);
        let Some(c) = self.chain.take() else { return false };
        let n = c.ids.len();
        if state.settings.entities.path.close_loop && n >= 2 {
            self.close(state, &c);
        } else {
            state.log(Severity::Info, tf!("path.ended", n));
        }
        true
    }

    fn close(&mut self, state: &mut AppState, c: &Live) {
        let (Some(first), Some(last)) = (c.ids.first().copied(), c.ids.last().copied()) else { return };
        if first == last {
            return;
        }
        let base = state.settings.entities.path.base().to_owned();
        let fgd = state.assets.fgd();
        match link(&mut state.doc, fgd, last, first, &base, UNDO_CLOSE) {
            Ok(_) => state.log(Severity::Info, tf!("path.ended_loop", c.ids.len())),
            Err(e) => state.notify(Severity::Error, tf!("path.failed", e)),
        }
    }

    pub fn close_now(&mut self, state: &mut AppState) -> bool {
        self.prune(state);
        let Some(c) = self.chain.take() else { return false };
        if c.ids.len() < 2 {
            self.chain = Some(c);
            return false;
        }
        self.close(state, &c);
        true
    }

    fn link_to(&mut self, state: &mut AppState, node: Id) {
        let Some(c) = self.chain.take() else { return };
        let Some(last) = c.ids.last().copied() else { return };
        if last == node {
            self.chain = Some(c);
            return;
        }
        let label = if c.ids.first() == Some(&node) { UNDO_CLOSE } else { UNDO_LINK };
        let base = state.settings.entities.path.base().to_owned();
        let fgd = state.assets.fgd();
        match link(&mut state.doc, fgd, last, node, &base, label) {
            Ok(_) if label == UNDO_CLOSE => state.log(Severity::Info, tf!("path.ended_loop", c.ids.len())),
            Ok(_) => state.log(Severity::Info, tf!("path.ended", c.ids.len())),
            Err(e) => state.notify(Severity::Error, tf!("path.failed", e)),
        }
    }

    fn insert(&mut self, state: &mut AppState, from: Id, pos: DVec3) {
        let graph = self.graph(state);
        let was_end = graph.node(from).is_some_and(|n| n.next.is_none());
        let start = graph.chain_start(from);
        let (walked, _) = graph.walk(start);
        let base = state.settings.entities.path.base().to_owned();
        let fgd = state.assets.fgd();
        match insert_after(&mut state.doc, fgd, from, Some(pos), &base) {
            Ok(ins) => {
                report_insert(state, &ins);
                self.last_point = Some(pos);
                if was_end {
                    let mut ids = walked;
                    ids.push(ins.id);
                    self.chain = Some(Live { ids });
                }
            }
            Err(e) => state.notify(Severity::Error, tf!("path.failed", e)),
        }
    }

    fn finish_move(&mut self, state: &mut AppState) {
        if matches!(self.drag, Some(Drag::Move { .. })) && state.doc.in_transaction() {
            state.doc.commit();
        }
    }

    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        sync_options(state);
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        self.prune(state);
        let snap = Snap::new(state, view, m.modifiers);
        let graph = self.graph(state);
        if m.hovering() {
            let node = Self::node_at(&graph, view, pos);
            let place = self.place(state, view, pos, &snap, &graph);
            let label = (node.is_none() && place.is_some()).then(|| {
                let prev = self
                    .chain
                    .as_ref()
                    .and_then(|c| c.ids.last())
                    .and_then(|id| graph.node(*id))
                    .map(|n| n.name.clone());
                name_after(&state.doc, prev.as_deref().filter(|n| !n.is_empty()), state.settings.entities.path.base())
            });
            self.hover = Some(Hover { view: view.index, frame: state.frame.index, pos: place, node, label });
            return ToolFlow::Ignored;
        }
        let origin = m.origin().unwrap_or(pos);
        if m.drag_started {
            self.finish_move(state);
            self.drag = None;
            if let Some(node) = Self::node_at(&graph, view, origin)
                && let Some(n) = graph.node(node)
            {
                if m.modifiers.shift {
                    self.drag = Some(Drag::Insert { view: view.index, from: node, pos: n.origin });
                } else if !state.doc.in_transaction() {
                    let selected = selected_points(state, &graph);
                    let ids = if selected.contains(&node) {
                        selected
                    } else {
                        state.doc.select(&[ObjRef::Entity(node)], SelectOp::Replace);
                        vec![node]
                    };
                    state.doc.begin("Move");
                    self.drag = Some(Drag::Move { view: view.index, ids, start: n.origin, applied: DVec3::ZERO });
                }
            }
        }
        if m.dragging || m.drag_stopped {
            match self.drag.clone() {
                Some(Drag::Move { view: vi, ids, start, applied }) if vi == view.index => {
                    let target = if view.is_2d() {
                        let p2 = snap.v2(view.screen_to_2d(pos));
                        Some(view.kind.from_2d(p2, start[depth_axis(view.kind)]))
                    } else {
                        self.place(state, view, pos, &snap, &graph)
                    };
                    if let Some(target) = target.filter(|t| t.is_finite()) {
                        let delta = clean_vec(target - start);
                        let step = delta - applied;
                        let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Entity(*id)).collect();
                        if step.length_squared() > 1e-12
                            && state.doc.translate(&objs, step).is_ok()
                            && let Some(Drag::Move { applied, .. }) = &mut self.drag
                        {
                            *applied = delta;
                        }
                    }
                }
                Some(Drag::Insert { view: vi, from, .. }) if vi == view.index => {
                    let target = if view.is_2d() {
                        let depth = graph.node(from).map_or(0.0, |n| n.origin[depth_axis(view.kind)]);
                        Some(clean_vec(view.kind.from_2d(snap.v2(view.screen_to_2d(pos)), depth)))
                    } else {
                        self.place(state, view, pos, &snap, &graph)
                    };
                    if let Some(t) = target.filter(|t| t.is_finite())
                        && let Some(Drag::Insert { pos: at, .. }) = &mut self.drag
                    {
                        *at = t;
                    }
                }
                _ => {}
            }
        }
        if m.activated() && self.drag.is_none() {
            if m.double_clicked && self.chain.is_some() {
                self.end(state);
            } else if let Some(node) = Self::node_at(&graph, view, origin) {
                if self.chain.is_some() {
                    self.link_to(state, node);
                } else {
                    let op = if m.modifiers.command || m.modifiers.ctrl { SelectOp::Toggle } else { SelectOp::Replace };
                    state.doc.select(&[ObjRef::Entity(node)], op);
                }
            } else if let Some(p) = self.place(state, view, origin, &snap, &graph) {
                self.add(state, p);
            }
            if m.drag_started {
                self.drag = Some(Drag::Swallow);
            }
        }
        if m.drag_stopped {
            match self.drag.take() {
                Some(Drag::Move { .. }) => {
                    if state.doc.in_transaction() {
                        state.doc.commit();
                    }
                }
                Some(Drag::Insert { from, pos, .. }) => {
                    let moved = graph.node(from).is_none_or(|n| n.origin.distance(pos) > 1e-6);
                    if moved {
                        self.insert(state, from, pos);
                    }
                }
                _ => {}
            }
        }
        ToolFlow::Handled
    }

    fn hover_in(&self, state: &AppState, view: &ViewInfo) -> Option<&Hover> {
        self.hover.as_ref().filter(|h| h.view == view.index && fresh(state, h.frame) && self.drag.is_none())
    }

    fn highlighted(&self, state: &AppState, graph: &PathGraph) -> std::collections::HashSet<Id> {
        let mut seeds = selected_points(state, graph);
        seeds.extend(self.drawing().iter().copied());
        graph.component(&seeds)
    }

    const CTX_INSERT: u32 = 0;
    const CTX_REMOVE: u32 = 1;
    const CTX_END: u32 = 2;
    const CTX_CLOSE: u32 = 3;
}

impl Tool for PathTool {
    fn id(&self) -> ToolId {
        ToolId::Path
    }

    fn activate(&mut self, state: &mut AppState) {
        sync_options(state);
    }

    fn deactivate(&mut self, state: &mut AppState) {
        self.finish_move(state);
        self.drag = None;
        self.end(state);
        self.hover = None;
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if input.navigating {
            return ToolFlow::Ignored;
        }
        let m = Mouse::from_input(input);
        self.handle_mouse(state, input.view, &m)
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if self.drag.is_some() && consume_plain_key(ctx, Key::Escape) {
            self.cancel(state);
            return true;
        }
        if self.chain.is_some() && (consume_plain_key(ctx, Key::Enter) || consume_plain_key(ctx, Key::Escape)) {
            return self.end(state);
        }
        false
    }

    fn action(&mut self, state: &mut AppState, action: Action) -> bool {
        if action != Action::Delete || !state.settings.entities.path.relink_on_delete {
            return false;
        }
        let graph = self.graph(state);
        if selected_points(state, &graph).is_empty() {
            return false;
        }
        self.finish_move(state);
        self.drag = None;
        remove_selected(state, true);
        self.prune(state);
        true
    }

    fn context_entries(&self, state: &AppState) -> Vec<ToolMenuEntry> {
        let graph = self.graph(state);
        let sel = selected_points(state, &graph);
        let drawing = self.drawing().len();
        vec![
            ToolMenuEntry { label: t!("path.ctx_insert").to_owned(), id: Self::CTX_INSERT, enabled: sel.len() == 1 },
            ToolMenuEntry { label: t!("path.ctx_remove").to_owned(), id: Self::CTX_REMOVE, enabled: !sel.is_empty() },
            ToolMenuEntry { label: t!("path.ctx_end").to_owned(), id: Self::CTX_END, enabled: drawing > 0 },
            ToolMenuEntry { label: t!("path.ctx_close").to_owned(), id: Self::CTX_CLOSE, enabled: drawing >= 2 },
        ]
    }

    fn context_command(&mut self, state: &mut AppState, id: u32) {
        match id {
            Self::CTX_INSERT => {
                insert_after_selection(state);
            }
            Self::CTX_REMOVE => {
                remove_selected(state, true);
                self.prune(state);
            }
            Self::CTX_END => {
                self.end(state);
            }
            Self::CTX_CLOSE => {
                self.close_now(state);
            }
            _ => {}
        }
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let graph = self.graph(state);
        let hl = self.highlighted(state, &graph);
        let mut arrows: Vec<OverlayLine> = Vec::new();
        let mut arrows_hl: Vec<OverlayLine> = Vec::new();
        {
            let mut ov = Ov::new(out, view);
            for n in graph.nodes.iter().filter(|n| n.visible) {
                let on = hl.contains(&n.id);
                let color = if on { rgba(COLOR_PENDING, 1.0) } else { rgba(COLOR_CHAIN, 0.9) };
                ov.point(n.origin, color, if on { 8.0 } else { 6.0 });
                for (other, branch) in [(n.next, false), (n.alt, true)] {
                    let Some(o) = other.and_then(|x| graph.node(x)) else { continue };
                    let c = match (branch, on) {
                        (true, _) => rgba(COLOR_BRANCH, 0.9),
                        (false, true) => rgba(COLOR_PENDING, 1.0),
                        (false, false) => rgba(COLOR_CHAIN, 0.9),
                    };
                    if on {
                        ov.xray_line(n.origin, o.origin, c);
                        arrow(view, n.origin, o.origin, c, &mut arrows_hl);
                    } else {
                        ov.line(n.origin, o.origin, c);
                        arrow(view, n.origin, o.origin, c, &mut arrows);
                    }
                }
            }
            if let Some(h) = self.hover_in(state, view) {
                if let Some(node) = h.node.and_then(|id| graph.node(id)) {
                    ov.point(node.origin, rgba(COLOR_GHOST, 1.0), 11.0);
                    if let Some(last) = self.drawing().last().and_then(|id| graph.node(*id))
                        && last.id != node.id
                    {
                        ov.xray_line(last.origin, node.origin, rgba(COLOR_GHOST, 0.8));
                    }
                } else if let Some(p) = h.pos {
                    ov.point(p, rgba(COLOR_GHOST, 1.0), 7.0);
                    if let Some(last) = self.drawing().last().and_then(|id| graph.node(*id)) {
                        ov.xray_line(last.origin, p, rgba(COLOR_GHOST, 0.8));
                        arrow(view, last.origin, p, rgba(COLOR_GHOST, 0.8), &mut arrows_hl);
                        let first = self.drawing().first().and_then(|id| graph.node(*id));
                        if state.settings.entities.path.close_loop
                            && let Some(f) = first.filter(|f| f.id != last.id)
                        {
                            ov.xray_line(p, f.origin, rgba(COLOR_GHOST, 0.35));
                        }
                    }
                }
            }
            if let Some(Drag::Insert { from, pos, .. }) = &self.drag
                && let Some(n) = graph.node(*from)
            {
                let c = rgba(COLOR_GHOST, 1.0);
                ov.point(*pos, c, 8.0);
                ov.xray_line(n.origin, *pos, c);
                arrow(view, n.origin, *pos, c, &mut arrows_hl);
                if let Some(next) = n.next.and_then(|x| graph.node(x)) {
                    ov.xray_line(*pos, next.origin, c);
                    arrow(view, *pos, next.origin, c, &mut arrows_hl);
                }
            }
        }
        if view.is_2d() {
            out.lines_on_top.extend(arrows);
        } else {
            out.lines.extend(arrows);
        }
        out.lines_on_top.extend(arrows_hl);
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        let graph = self.graph(state);
        let hl = self.highlighted(state, &graph);
        let names = !view.is_2d() || !state.settings.helpers.show_entity_names;
        let mut drawn = if names { 0 } else { MAX_LABELS };
        for n in graph.nodes.iter().filter(|n| n.visible && hl.contains(&n.id) && !n.name.is_empty()) {
            if drawn >= MAX_LABELS {
                break;
            }
            if let Some(s) = view.world_to_screen(n.origin)
                && view.rect.contains(s)
            {
                paint_label(&painter, s + Vec2::new(8.0, -8.0), Align2::LEFT_BOTTOM, &n.name, COLOR_PENDING);
                drawn += 1;
            }
        }
        if let Some(h) = self.hover_in(state, view)
            && let (Some(p), Some(label)) = (h.pos, &h.label)
            && let Some(s) = view.world_to_screen(p)
        {
            paint_label(&painter, s + Vec2::new(10.0, 10.0), Align2::LEFT_TOP, label, COLOR_GHOST);
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        sync_options(state);
        let classes = self.classes(state);
        let graph = self.graph(state);
        ui.horizontal_wrapped(|ui| {
            let current = chosen_class(state);
            let tip = tf!("path.class_tip", &current.next_key);
            ui.label(t!("path.class")).on_hover_text(&tip);
            let mut picked = None;
            egui::ComboBox::from_id_salt("path_tool_class")
                .selected_text(&current.class)
                .width(150.0)
                .show_ui(ui, |ui| {
                    for c in classes.iter() {
                        if ui.selectable_label(c.class.eq_ignore_ascii_case(&current.class), &c.class).clicked() {
                            picked = Some(c.class.clone());
                        }
                    }
                })
                .response
                .on_hover_text(&tip);
            if let Some(c) = picked {
                state.settings.entities.path.class = c;
                state.settings_dirty = true;
            }
            ui.separator();
            let base_tip = tf!("path.base_tip", state.settings.entities.path.base());
            ui.label(t!("path.base")).on_hover_text(&base_tip);
            let mut base = state.settings.entities.path.base_name.clone();
            let r = ui
                .add(egui::TextEdit::singleline(&mut base).desired_width(90.0).hint_text("path"))
                .on_hover_text(&base_tip);
            if r.changed() {
                base.truncate(super::chain::MAX_BASE_LEN);
                state.settings.entities.path.base_name = base;
                state.settings_dirty = true;
            }
            ui.separator();
            let s = &mut state.settings.entities.path;
            let mut changed =
                ui.checkbox(&mut s.close_loop, t!("path.loop")).on_hover_text(t!("path.loop_tip")).changed();
            changed |=
                ui.checkbox(&mut s.relink_on_delete, t!("path.relink")).on_hover_text(t!("path.relink_tip")).changed();
            ui.label(t!("path.offset")).on_hover_text(t!("path.offset_tip"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.surface_offset).speed(1.0).range(-1024.0..=1024.0).max_decimals(2))
                .on_hover_text(t!("path.offset_tip"))
                .changed();
            if changed {
                state.settings_dirty = true;
            }
            ui.separator();
            let drawing = self.drawing().len();
            let sel = selected_points(state, &graph);
            let end_tip = tf!("path.end_tip", key_label(Key::Enter), key_label(Key::Escape));
            if ui.add_enabled(drawing > 0, egui::Button::new(t!("path.end"))).on_hover_text(end_tip).clicked() {
                self.end(state);
            }
            if ui
                .add_enabled(drawing >= 2, egui::Button::new(t!("path.close_now")))
                .on_hover_text(t!("path.close_now_tip"))
                .clicked()
            {
                self.close_now(state);
            }
            if ui
                .add_enabled(sel.len() == 1, egui::Button::new(t!("path.insert")))
                .on_hover_text(t!("path.insert_tip"))
                .clicked()
            {
                insert_after_selection(state);
            }
            if ui
                .add_enabled(!sel.is_empty(), egui::Button::new(t!("path.remove")))
                .on_hover_text(t!("path.remove_tip"))
                .clicked()
            {
                remove_selected(state, true);
                self.prune(state);
            }
            ui.separator();
            if drawing > 0 {
                ui.label(tf!("path.count", drawing));
            } else {
                ui.label(tf!("path.chains", graph.chains().len()));
            }
        });
    }

    fn cursor(&self, state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<CursorIcon> {
        match self.drag {
            Some(Drag::Move { .. }) => return Some(CursorIcon::Grabbing),
            Some(Drag::Insert { .. }) => return Some(CursorIcon::Copy),
            _ => {}
        }
        let graph = self.graph(state);
        if Self::node_at(&graph, view, pos).is_some() {
            return Some(if self.chain.is_some() { CursorIcon::Alias } else { CursorIcon::Grab });
        }
        Some(CursorIcon::Crosshair)
    }

    fn status_hint(&self, _state: &AppState) -> Option<String> {
        Some(match &self.chain {
            Some(c) => tf!("path.hint_drawing", c.ids.len(), key_label(Key::Enter), key_label(Key::Escape)),
            None => t!("path.hint_idle").to_owned(),
        })
    }

    fn cancel(&mut self, state: &mut AppState) -> bool {
        match self.drag.take() {
            Some(Drag::Move { .. }) => {
                if state.doc.in_transaction() {
                    state.doc.cancel();
                }
                true
            }
            Some(_) => true,
            None => self.end(state),
        }
    }

    fn set_option(&mut self, state: &mut AppState, key: &str, value: &str) -> bool {
        let v = value.trim();
        let flag = || match v.to_ascii_lowercase().as_str() {
            "on" | "true" | "1" | "yes" => Some(true),
            "off" | "false" | "0" | "no" => Some(false),
            _ => None,
        };
        let s = &mut state.settings.entities.path;
        match key {
            "class" if !v.is_empty() && v.len() <= 128 && !v.contains(char::is_whitespace) => {
                v.clone_into(&mut s.class)
            }
            "base_name" if v.len() <= super::chain::MAX_BASE_LEN => v.clone_into(&mut s.base_name),
            "loop" => match flag() {
                Some(b) => s.close_loop = b,
                None => return false,
            },
            "relink" => match flag() {
                Some(b) => s.relink_on_delete = b,
                None => return false,
            },
            "offset" => match v.parse::<f64>() {
                Ok(x) if x.is_finite() && x.abs() <= 1024.0 => s.surface_offset = x,
                _ => return false,
            },
            _ => return false,
        }
        state.settings_dirty = true;
        sync_options(state);
        true
    }

    fn option_values(&self) -> Vec<ToolOption> {
        let s = OPTION_SNAPSHOT.with(|s| s.borrow().clone());
        vec![
            ToolOption::new(
                "class",
                s.class_name(),
                "Class of the new points: path_corner, path_track, path_corner_crash, keyframe_rope...",
            ),
            ToolOption::new(
                "base_name",
                s.base(),
                "Base name: \"path\" names the points path_1, path_2... (unique in the map).",
            ),
            ToolOption::new(
                "loop",
                if s.close_loop { "on" } else { "off" },
                "on/off: ending a path links its last point to the first.",
            ),
            ToolOption::new(
                "relink",
                if s.relink_on_delete { "on" } else { "off" },
                "on/off: Delete in this tool links the neighbours of removed points.",
            ),
            ToolOption::new(
                "offset",
                crate::common::fmt_num(s.offset()),
                "Height above the surface of points placed in the 3D view (-1024 to 1024).",
            ),
        ]
    }
}

thread_local! {
    static OPTION_SNAPSHOT: RefCell<hp_app::settings::PathToolSettings> = RefCell::new(Default::default());
}

pub fn sync_options(state: &AppState) {
    OPTION_SNAPSHOT.with(|s| {
        let mut s = s.borrow_mut();
        if *s != state.settings.entities.path {
            *s = state.settings.entities.path.clone();
        }
    });
}
