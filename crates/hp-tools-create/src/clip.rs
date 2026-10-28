use std::cell::RefCell;

use egui::{Align2, CursorIcon, Key, Pos2, Stroke, Ui, Vec2};
use glam::{DVec2, DVec3};
use hp_app::actions::{Action, key_label, shortcut_label};
use hp_app::state::AppState;
use hp_app::{Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, ViewKind, t, tf};
use hp_editor::{ClipKeep, Id};
use hp_geom::{Aabb, Plane};
use hp_render::Overlay;

use crate::common::{
    COLOR_BACK, COLOR_FRONT, COLOR_PENDING, COLOR_REMOVED, Mouse, Ov, Snap, active_material, consume_plain_key,
    consume_tabs, fmt_len, fmt_num, fresh, paint_handle, paint_label, pick_surface, plane_box_polygon, rgba, round_to,
};

const HANDLE_PICK: f32 = 8.0;

pub fn clip_plane_2d(kind: ViewKind, a: DVec2, b: DVec2) -> Option<Plane> {
    let (r, u, _) = kind.axes_2d()?;
    let d = b - a;
    if !d.is_finite() || d.length_squared() < 1e-12 {
        return None;
    }
    let mut normal = DVec3::ZERO;
    normal[r] = d.y;
    normal[u] = -d.x;
    Plane::from_normal_point(normal, kind.from_2d(a, 0.0))
}

pub fn clip_plane_3_points(kind: ViewKind, a: DVec2, b: DVec2, depth: f64, c: DVec3) -> Option<Plane> {
    let base = clip_plane_2d(kind, a, b)?;
    let (pa, pb) = (kind.from_2d(a, depth), kind.from_2d(b, depth));
    match Plane::from_points(pa, pb, c) {
        Some(p) if p.normal.dot(base.normal) < 0.0 => Some(p.flip()),
        Some(p) => Some(p),
        None => Some(base),
    }
}

pub fn next_keep(keep: ClipKeep, backwards: bool) -> ClipKeep {
    match (keep, backwards) {
        (ClipKeep::Both, false) | (ClipKeep::Back, true) => ClipKeep::Front,
        (ClipKeep::Front, false) | (ClipKeep::Both, true) => ClipKeep::Back,
        (ClipKeep::Back, false) | (ClipKeep::Front, true) => ClipKeep::Both,
    }
}

fn keep_key(keep: ClipKeep) -> &'static str {
    match keep {
        ClipKeep::Front => "create.clip.keep_front",
        ClipKeep::Back => "create.clip.keep_back",
        ClipKeep::Both => "create.clip.keep_both",
    }
}

fn fmt_angle(deg: f64) -> String {
    fmt_num(round_to(deg, 0.1))
}

#[derive(Clone, Debug, PartialEq)]
enum ClipDef {
    None,
    Line { kind: ViewKind, a: DVec2, b: DVec2, depth: f64, c: Option<DVec3> },
    Points { points: Vec<DVec3>, plane: Option<Plane> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    A,
    B,
    C,
    Both,
    Point(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    view: usize,
    target: Target,
    from: DVec2,
    start: (DVec2, DVec2),
}

#[derive(Debug, Default)]
struct Preview {
    key: Option<(u64, [u64; 4])>,
    front_edges: Vec<[DVec3; 2]>,
    back_edges: Vec<[DVec3; 2]>,
    front_tris: Vec<[DVec3; 3]>,
    back_tris: Vec<[DVec3; 3]>,
    crossing: usize,
    bounds: Aabb,
}

pub struct ClipTool {
    pub keep: ClipKeep,
    pub three_point: bool,
    def: ClipDef,
    drag: Option<Drag>,
    hover_handle: Option<(usize, Target, u64)>,
    hover_3d: Option<(DVec3, u64)>,
    last_focus: Option<egui::Id>,
    preview: RefCell<Preview>,
}

impl Default for ClipTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipTool {
    pub fn new() -> Self {
        Self {
            keep: ClipKeep::Both,
            three_point: false,
            def: ClipDef::None,
            drag: None,
            hover_handle: None,
            hover_3d: None,
            last_focus: None,
            preview: RefCell::new(Preview::default()),
        }
    }

    pub fn set_line(&mut self, kind: ViewKind, a: DVec2, b: DVec2) {
        self.def = if kind.is_2d() { ClipDef::Line { kind, a, b, depth: 0.0, c: None } } else { ClipDef::None };
    }

    pub fn set_third_point(&mut self, p: DVec3) {
        if let ClipDef::Line { c, .. } = &mut self.def
            && p.is_finite()
        {
            *c = Some(p);
        }
    }

    pub fn plane(&self) -> Option<Plane> {
        match &self.def {
            ClipDef::None => None,
            ClipDef::Line { kind, a, b, depth, c } => match c {
                Some(c) if self.three_point => clip_plane_3_points(*kind, *a, *b, *depth, *c),
                _ => clip_plane_2d(*kind, *a, *b),
            },
            ClipDef::Points { plane, .. } => *plane,
        }
    }

    pub fn cycle_mode(&mut self, backwards: bool) {
        self.keep = next_keep(self.keep, backwards);
    }

    fn tab_input(&mut self, ctx: &egui::Context) -> bool {
        let focused = ctx.memory(|m| m.focused());
        let had_focus = std::mem::replace(&mut self.last_focus, focused);
        if had_focus.is_some() {
            return false;
        }
        let presses = consume_tabs(ctx);
        if presses.is_empty() {
            return false;
        }
        if let Some(id) = focused {
            ctx.memory_mut(|m| m.surrender_focus(id));
            self.last_focus = None;
            ctx.request_repaint();
        }
        for back in presses {
            self.cycle_mode(back);
        }
        true
    }

    fn mode_keys(state: &AppState) -> String {
        let tab = key_label(Key::Tab);
        match state.keys.shortcut(Action::ToolClip) {
            Some(sc) => format!("{} / {tab}", shortcut_label(&sc)),
            None => tab,
        }
    }

    pub fn clear(&mut self) -> bool {
        let had = self.def != ClipDef::None;
        self.def = ClipDef::None;
        self.drag = None;
        self.hover_handle = None;
        had
    }

    pub fn apply(&mut self, state: &mut AppState) -> bool {
        let Some(plane) = self.plane() else { return false };
        let ids: Vec<Id> = state.doc.selected_solid_ids();
        if ids.is_empty() {
            state.notify(Severity::Warning, t!("create.clip.no_selection"));
            return false;
        }
        let material = state.settings.brush.clip_current_material.then(|| active_material(state));
        match state.doc.clip_with(&ids, &plane, self.keep, material.as_deref()) {
            Ok(pieces) if pieces.is_empty() => {
                state.notify(Severity::Info, t!("create.clip.nothing"));
                false
            }
            Ok(pieces) => {
                let cut = if self.keep == ClipKeep::Both { pieces.len() / 2 } else { pieces.len() };
                state.log(Severity::Info, tf!("create.clip.done", cut));
                true
            }
            Err(e) => {
                state.notify(Severity::Error, tf!("create.clip.failed", e));
                false
            }
        }
    }

    fn line_points(&self) -> Option<(DVec3, DVec3)> {
        match &self.def {
            ClipDef::Line { kind, a, b, depth, .. } => Some((kind.from_2d(*a, *depth), kind.from_2d(*b, *depth))),
            _ => None,
        }
    }

    fn handle_world(&self, view: &ViewInfo, target: Target) -> Option<DVec3> {
        match (&self.def, target) {
            (ClipDef::Line { kind, a, depth, .. }, Target::A) if *kind == view.kind => Some(kind.from_2d(*a, *depth)),
            (ClipDef::Line { kind, b, depth, .. }, Target::B) if *kind == view.kind => Some(kind.from_2d(*b, *depth)),
            (ClipDef::Line { kind, c: Some(c), .. }, Target::C)
                if self.three_point && view.is_2d() && *kind != view.kind =>
            {
                Some(*c)
            }
            (ClipDef::Points { points, .. }, Target::Point(i)) => points.get(i).copied(),
            _ => None,
        }
    }

    fn handle_pos(&self, view: &ViewInfo, target: Target) -> Option<Pos2> {
        let p = view.world_to_screen(self.handle_world(view, target)?)?;
        view.rect.expand(HANDLE_PICK).contains(p).then_some(p)
    }

    fn targets(&self) -> Vec<Target> {
        match &self.def {
            ClipDef::None => Vec::new(),
            ClipDef::Line { .. } => vec![Target::C, Target::B, Target::A],
            ClipDef::Points { points, .. } => (0..points.len()).rev().map(Target::Point).collect(),
        }
    }

    fn handle_at(&self, view: &ViewInfo, pos: Pos2) -> Option<Target> {
        self.targets().into_iter().find(|t| self.handle_pos(view, *t).is_some_and(|h| h.distance(pos) <= HANDLE_PICK))
    }

    fn on_line(&self, view: &ViewInfo, pos: Pos2) -> bool {
        let (Some(a), Some(b)) = (self.handle_pos(view, Target::A), self.handle_pos(view, Target::B)) else {
            return false;
        };
        let d = |p: Pos2| DVec2::new(f64::from(p.x), f64::from(p.y));
        hp_geom::point_segment_distance_2d(d(pos), d(a), d(b)) <= f64::from(HANDLE_PICK)
    }

    fn set_c_2d(&mut self, view: &ViewInfo, p: DVec2) {
        let Some((pa, pb)) = self.line_points() else { return };
        let Some((_, _, d)) = view.kind.axes_2d() else { return };
        if let ClipDef::Line { c, .. } = &mut self.def {
            let depth = c.map_or((pa[d] + pb[d]) * 0.5, |c| c[d]);
            *c = Some(view.kind.from_2d(p, depth));
        }
    }

    fn move_point(&mut self, index: usize, p: DVec3) {
        let ClipDef::Points { points, plane } = &mut self.def else { return };
        let mut moved = points.clone();
        let Some(slot) = moved.get_mut(index) else { return };
        *slot = p;
        if let [a, b, c] = moved[..] {
            let Some(mut next) = Plane::from_points(a, b, c) else { return };
            if plane.is_some_and(|old| old.normal.dot(next.normal) < 0.0) {
                next = next.flip();
            }
            *plane = Some(next);
        }
        *points = moved;
    }

    fn new_line_depth(state: &AppState, kind: ViewKind) -> f64 {
        let Some((_, _, d)) = kind.axes_2d() else { return 0.0 };
        state
            .doc
            .selection_bounds(&|e| state.entity_bounds(e))
            .map(|b| b.center()[d])
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
    }

    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        let snap = Snap::for_selection(state, view, m.modifiers);
        if view.is_2d() { self.mouse_2d(state, view, m, pos, &snap) } else { self.mouse_3d(state, view, m, pos, &snap) }
    }

    fn mouse_2d(&mut self, state: &AppState, view: &ViewInfo, m: &Mouse, pos: Pos2, snap: &Snap) -> ToolFlow {
        if m.hovering() {
            self.hover_handle = self.handle_at(view, pos).map(|t| (view.index, t, state.frame.index));
            return ToolFlow::Ignored;
        }
        let origin = m.origin().unwrap_or(pos);
        let at_origin = snap.v2(view.screen_to_2d(origin));
        let ctrl = m.modifiers.ctrl || m.modifiers.command;
        let shift = m.modifiers.shift;
        let line_elsewhere = matches!(&self.def, ClipDef::Line { kind, .. } if *kind != view.kind);
        let third = self.three_point && line_elsewhere && !shift;
        if m.drag_started {
            let hit = if shift { None } else { self.handle_at(view, origin) };
            let start = match &self.def {
                ClipDef::Line { a, b, .. } => (*a, *b),
                _ => (DVec2::ZERO, DVec2::ZERO),
            };
            let drag = |target| Drag { view: view.index, target, from: view.screen_to_2d(origin), start };
            self.drag = match hit {
                Some(Target::A | Target::B) if ctrl => Some(drag(Target::Both)),
                Some(t) => Some(drag(t)),
                None if ctrl && self.on_line(view, origin) => Some(drag(Target::Both)),
                None if third => {
                    self.set_c_2d(view, at_origin);
                    Some(drag(Target::C))
                }
                None => {
                    let depth = Self::new_line_depth(state, view.kind);
                    self.def = ClipDef::Line { kind: view.kind, a: at_origin, b: at_origin, depth, c: None };
                    Some(drag(Target::B))
                }
            };
        } else if m.clicked && third {
            self.set_c_2d(view, at_origin);
        }
        if (m.dragging || m.drag_stopped)
            && let Some(d) = self.drag
            && d.view == view.index
        {
            let p = snap.v2(view.screen_to_2d(pos));
            match d.target {
                Target::A | Target::B | Target::Both => {
                    if let ClipDef::Line { a, b, .. } = &mut self.def {
                        match d.target {
                            Target::A => *a = p,
                            Target::B => *b = p,
                            _ => {
                                let moved = snap.v2(d.start.0 + (view.screen_to_2d(pos) - d.from));
                                let delta = moved - d.start.0;
                                if delta.is_finite() {
                                    *a = moved;
                                    *b = d.start.1 + delta;
                                }
                            }
                        }
                    }
                }
                Target::C => self.set_c_2d(view, p),
                Target::Point(i) => {
                    if let (Some((r, u, _)), ClipDef::Points { points, .. }) = (view.kind.axes_2d(), &self.def)
                        && let Some(mut q) = points.get(i).copied()
                    {
                        q[r] = p.x;
                        q[u] = p.y;
                        self.move_point(i, q);
                    }
                }
            }
            self.hover_handle = Some((view.index, d.target, state.frame.index));
        }
        if m.drag_stopped {
            self.drag = None;
            if let ClipDef::Line { a, b, .. } = &self.def
                && a.distance(*b) < 1e-6
            {
                self.def = ClipDef::None;
            }
        }
        ToolFlow::Handled
    }

    fn mouse_3d(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse, pos: Pos2, snap: &Snap) -> ToolFlow {
        let point = pick_surface(state, view, pos).map(|h| snap.on_plane(h.point, h.normal));
        if m.hovering() {
            self.hover_3d = point.map(|p| (p, state.frame.index));
            self.hover_handle = self.handle_at(view, pos).map(|t| (view.index, t, state.frame.index));
            return ToolFlow::Ignored;
        }
        let origin = m.origin().unwrap_or(pos);
        if m.drag_started
            && !m.modifiers.shift
            && let Some(Target::Point(i)) = self.handle_at(view, origin)
        {
            self.drag =
                Some(Drag { view: view.index, target: Target::Point(i), from: DVec2::ZERO, start: Default::default() });
            return ToolFlow::Handled;
        }
        if let Some(d) = self.drag
            && d.view == view.index
            && let Target::Point(i) = d.target
        {
            if m.dragging || m.drag_stopped {
                let old = match &self.def {
                    ClipDef::Points { points, .. } => points.get(i).copied(),
                    _ => None,
                };
                let target = point.or_else(|| {
                    let old = old?;
                    let (o, dir) = view.ray(pos);
                    let n = view.cam3d.forward();
                    let denom = dir.dot(n);
                    let t = (old - o).dot(n) / denom;
                    (denom.abs() > 1e-9 && t > 0.0 && t.is_finite()).then(|| o + dir * t)
                });
                if let Some(p) = target.filter(|p| p.is_finite()) {
                    self.move_point(i, p);
                }
                self.hover_handle = Some((view.index, d.target, state.frame.index));
            }
            if m.drag_stopped {
                self.drag = None;
            }
            return ToolFlow::Handled;
        }
        if m.activated()
            && let Some(p) = point
        {
            let mut points = match std::mem::replace(&mut self.def, ClipDef::None) {
                ClipDef::Points { points, plane: None } => points,
                _ => Vec::new(),
            };
            if !points.iter().any(|q| q.distance(p) < 1e-6) {
                points.push(p);
            }
            let mut plane = None;
            if let [a, b, c] = points[..] {
                match Plane::from_points(a, b, c) {
                    Some(pl) => plane = Some(if pl.distance(view.cam3d.pos) < 0.0 { pl.flip() } else { pl }),
                    None => {
                        state.notify(Severity::Warning, t!("create.clip.collinear"));
                        points.clear();
                    }
                }
            }
            self.def = if points.is_empty() { ClipDef::None } else { ClipDef::Points { points, plane } };
        }
        if m.any_button() { ToolFlow::Handled } else { ToolFlow::Ignored }
    }

    fn refresh_preview(&self, state: &AppState) {
        let Ok(mut guard) = self.preview.try_borrow_mut() else { return };
        let pv: &mut Preview = &mut guard;
        let Some(plane) = self.plane() else {
            if pv.key.is_some() {
                *pv = Preview::default();
            }
            return;
        };
        let key = (
            state.doc.revision(),
            [plane.normal.x.to_bits(), plane.normal.y.to_bits(), plane.normal.z.to_bits(), plane.dist.to_bits()],
        );
        if pv.key == Some(key) {
            return;
        }
        *pv = Preview { key: Some(key), bounds: Aabb::EMPTY, ..Preview::default() };
        let mut next = 1u32;
        let mut gen_id = || {
            next = next.wrapping_add(1);
            next
        };
        let mut selection_bounds = Aabb::EMPTY;
        for id in state.doc.selected_solid_ids() {
            let Some(node) = state.doc.scene().solid(id) else { continue };
            selection_bounds = selection_bounds.union(&node.bounds);
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for c in node.bounds.corners() {
                let d = plane.distance(c);
                lo = lo.min(d);
                hi = hi.max(d);
            }
            if hi <= hp_geom::PLANE_EPS || lo >= -hp_geom::PLANE_EPS {
                continue;
            }
            let Ok((Some(front), Some(back))) = hp_geom::clip_solid(&node.solid.sides, &plane, &mut gen_id) else {
                continue;
            };
            pv.crossing += 1;
            for (sides, is_front) in [(front, true), (back, false)] {
                let geom = hp_geom::build_solid(&sides);
                pv.bounds = pv.bounds.union(&geom.aabb);
                let (edges, tris) = if is_front {
                    (&mut pv.front_edges, &mut pv.front_tris)
                } else {
                    (&mut pv.back_edges, &mut pv.back_tris)
                };
                edges.extend(geom.edges());
                for f in &geom.faces {
                    for [i, j, k] in f.triangles() {
                        if let (Some(&a), Some(&b), Some(&c)) =
                            (f.vertices.get(i), f.vertices.get(j), f.vertices.get(k))
                        {
                            tris.push([a, b, c]);
                        }
                    }
                }
            }
        }
        if pv.bounds.is_empty() {
            pv.bounds = selection_bounds;
        }
    }

    pub fn crossing_count(&self, state: &AppState) -> usize {
        self.refresh_preview(state);
        self.preview.try_borrow().map(|p| p.crossing).unwrap_or(0)
    }

    fn plane_region(&self, pv: &Preview) -> Aabb {
        if !pv.bounds.is_empty() {
            return pv.bounds.expand(16.0);
        }
        let mut b = Aabb::EMPTY;
        match &self.def {
            ClipDef::Line { kind, a, b: pb, c, .. } => {
                b.extend(kind.from_2d(*a, -256.0));
                b.extend(kind.from_2d(*pb, 256.0));
                if let Some(c) = c {
                    b.extend(*c);
                }
            }
            ClipDef::Points { points, .. } => {
                for p in points {
                    b.extend(*p);
                }
            }
            ClipDef::None => {}
        }
        if b.is_empty() { b } else { b.expand(64.0) }
    }

    fn colors(&self) -> ([f32; 4], [f32; 4]) {
        match self.keep {
            ClipKeep::Both => (rgba(COLOR_FRONT, 1.0), rgba(COLOR_BACK, 1.0)),
            ClipKeep::Front => (rgba(COLOR_FRONT, 1.0), rgba(COLOR_REMOVED, 0.55)),
            ClipKeep::Back => (rgba(COLOR_REMOVED, 0.55), rgba(COLOR_BACK, 1.0)),
        }
    }

    fn mode_ui(&mut self, ui: &mut Ui, state: &AppState) {
        let tip = tf!("create.clip.mode_tip", Self::mode_keys(state));
        for keep in [ClipKeep::Front, ClipKeep::Back, ClipKeep::Both] {
            let b = egui::Button::selectable(self.keep == keep, t!(keep_key(keep)));
            if ui.add(b).on_hover_text(&tip).clicked() {
                self.keep = keep;
            }
        }
    }

    fn toggle_measurements(state: &mut AppState) {
        state.settings.brush.clip_measurements = !state.settings.brush.clip_measurements;
        state.settings_dirty = true;
    }

    fn active_handle(&self, state: &AppState, view: &ViewInfo) -> Option<Target> {
        let dragged = self.drag.filter(|d| d.view == view.index).map(|d| d.target);
        dragged.or_else(|| {
            self.hover_handle.filter(|(vi, _, frame)| *vi == view.index && fresh(state, *frame)).map(|(_, t, _)| t)
        })
    }

    fn paint_line(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let ClipDef::Line { kind, a, b, .. } = &self.def else { return };
        let (Some(pa), Some(pb)) =
            (view.world_to_screen(kind.from_2d(*a, 0.0)), view.world_to_screen(kind.from_2d(*b, 0.0)))
        else {
            return;
        };
        let d = pb - pa;
        if d.length() > 0.5 {
            let dir = d.normalized();
            let reach = view.rect.size().length() * 2.0;
            painter.line_segment(
                [pa - dir * reach, pb + dir * reach],
                Stroke::new(1.0, COLOR_PENDING.gamma_multiply(0.35)),
            );
            let mid = pa + d * 0.5;
            let n = egui::vec2(-dir.y, dir.x);
            let (front, _) = self.colors();
            let c =
                egui::Color32::from_rgb((front[0] * 255.0) as u8, (front[1] * 255.0) as u8, (front[2] * 255.0) as u8);
            painter.arrow(mid, n * 14.0, Stroke::new(1.5, c));
            if state.settings.brush.clip_measurements {
                let delta = *b - *a;
                let angle = delta.y.atan2(delta.x).to_degrees();
                let text = tf!("create.clip.measure", fmt_len(delta.length()), fmt_angle(angle));
                paint_label(painter, mid - n * 10.0, Align2::CENTER_CENTER, &text, COLOR_PENDING);
            }
        }
        painter.line_segment([pa, pb], Stroke::new(2.0, COLOR_PENDING));
        let active = self.active_handle(state, view);
        let is = |t: Target| active == Some(t) || active == Some(Target::Both) && matches!(t, Target::A | Target::B);
        paint_handle(painter, pa, is(Target::A));
        paint_handle(painter, pb, is(Target::B));
    }

    fn paint_third(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let Some(c) = self.handle_pos(view, Target::C) else { return };
        paint_handle(painter, c, self.active_handle(state, view) == Some(Target::C));
        if state.settings.brush.clip_measurements
            && let ClipDef::Line { kind, a, b, .. } = &self.def
            && let (Some(base), Some(plane)) = (clip_plane_2d(*kind, *a, *b), self.plane())
        {
            let tilt = base.normal.dot(plane.normal).clamp(-1.0, 1.0).acos().to_degrees();
            let text = tf!("create.clip.tilt", fmt_angle(tilt));
            paint_label(painter, c + Vec2::new(10.0, -10.0), Align2::LEFT_BOTTOM, &text, COLOR_PENDING);
        }
    }

    fn paint_points(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let ClipDef::Points { points, .. } = &self.def else { return };
        let active = self.active_handle(state, view);
        for i in 0..points.len() {
            if let Some(p) = self.handle_pos(view, Target::Point(i)) {
                paint_handle(painter, p, active == Some(Target::Point(i)));
            }
        }
        if view.is_2d() || !state.settings.brush.clip_measurements {
            return;
        }
        for (i, p) in points.iter().enumerate() {
            let Some(q) = points.get((i + 1) % points.len()).filter(|_| points.len() > 1) else { continue };
            if points.len() == 2 && i == 1 {
                break;
            }
            if let Some(s) = view.world_to_screen((*p + *q) * 0.5)
                && view.rect.contains(s)
            {
                paint_label(painter, s, Align2::CENTER_CENTER, &fmt_len(p.distance(*q)), COLOR_PENDING);
            }
        }
    }
}

impl Tool for ClipTool {
    fn id(&self) -> ToolId {
        ToolId::Clip
    }

    fn activate(&mut self, _state: &mut AppState) {
        self.last_focus = None;
    }

    fn reactivate(&mut self, _state: &mut AppState) {
        self.cycle_mode(false);
    }

    fn deactivate(&mut self, _state: &mut AppState) {
        self.clear();
        self.hover_3d = None;
        self.last_focus = None;
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if input.navigating {
            return ToolFlow::Ignored;
        }
        let m = Mouse::from_input(input);
        self.handle_mouse(state, input.view, &m)
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if self.tab_input(ctx) {
            return true;
        }
        if self.plane().is_some() && consume_plain_key(ctx, Key::Enter) {
            self.apply(state);
            return true;
        }
        if self.def != ClipDef::None && consume_plain_key(ctx, Key::Escape) {
            return self.clear();
        }
        if consume_plain_key(ctx, Key::C) {
            self.three_point = !self.three_point;
            return true;
        }
        if consume_plain_key(ctx, Key::O) {
            Self::toggle_measurements(state);
            return true;
        }
        false
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        self.refresh_preview(state);
        let mut ov = Ov::new(out, view);
        let Ok(pv) = self.preview.try_borrow() else { return };
        let (front, back) = self.colors();
        for (edges, color) in [(&pv.front_edges, front), (&pv.back_edges, back)] {
            for e in edges {
                ov.xray_line(e[0], e[1], color);
            }
        }
        if !view.is_2d() {
            for (tris, color) in [(&pv.front_tris, front), (&pv.back_tris, back)] {
                let fill = [color[0], color[1], color[2], 0.12];
                for t in tris {
                    ov.tri(t[0], t[1], t[2], fill);
                }
            }
            if let Some(plane) = self.plane() {
                let poly = plane_box_polygon(&plane, &self.plane_region(&pv));
                ov.polygon_fill(&poly, rgba(COLOR_PENDING, 0.16));
                ov.polygon_outline(&poly, rgba(COLOR_PENDING, 0.9), true);
            }
        }
        if let ClipDef::Line { kind, c, .. } = &self.def
            && *kind != view.kind
            && (self.three_point || !view.is_2d())
            && let Some((pa, pb)) = self.line_points()
        {
            let color = rgba(COLOR_PENDING, 1.0);
            ov.xray_line(pa, pb, color);
            ov.point(pa, color, 7.0);
            ov.point(pb, color, 7.0);
            if self.three_point
                && let Some(c) = c
            {
                let mid = (pa + pb) * 0.5;
                ov.xray_line(mid, *c, rgba(COLOR_PENDING, 0.6));
                ov.point(*c, color, 8.0);
            }
        }
        if let ClipDef::Points { points, plane } = &self.def {
            for (i, p) in points.iter().enumerate() {
                ov.point(*p, rgba(COLOR_PENDING, 1.0), 8.0);
                if let Some(q) = points.get(i + 1) {
                    ov.line(*p, *q, rgba(COLOR_PENDING, 1.0));
                }
            }
            if plane.is_none()
                && !view.is_2d()
                && self.drag.is_none()
                && let Some((h, frame)) = self.hover_3d
                && fresh(state, frame)
            {
                if let Some(last) = points.last() {
                    ov.line(*last, h, rgba(COLOR_PENDING, 0.6));
                }
                if let [a, b] = points[..]
                    && let Some(pl) = Plane::from_points(a, b, h)
                {
                    let poly = plane_box_polygon(&pl, &Aabb::from_points([a, b, h]).expand(64.0));
                    ov.polygon_outline(&poly, rgba(COLOR_PENDING, 0.5), true);
                }
            }
        }
        if !view.is_2d()
            && self.drag.is_none()
            && matches!(self.def, ClipDef::None | ClipDef::Points { plane: None, .. })
            && let Some((h, frame)) = self.hover_3d
            && fresh(state, frame)
        {
            ov.point(h, rgba(COLOR_PENDING, 0.8), 6.0);
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        match &self.def {
            ClipDef::Line { kind, .. } if *kind == view.kind => self.paint_line(state, view, &painter),
            ClipDef::Line { .. } => self.paint_third(state, view, &painter),
            ClipDef::Points { .. } => self.paint_points(state, view, &painter),
            ClipDef::None => {}
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal_wrapped(|ui| {
            self.mode_ui(ui, state);
            ui.separator();
            let ready = self.plane().is_some();
            let cut = if ready { self.crossing_count(state) } else { 0 };
            if ui
                .add_enabled(ready && cut > 0, egui::Button::new(t!("create.clip.apply")))
                .on_hover_text(tf!("create.clip.apply_tip", key_label(Key::Enter)))
                .clicked()
            {
                self.apply(state);
            }
            if ui.add_enabled(self.def != ClipDef::None, egui::Button::new(t!("common.clear"))).clicked() {
                self.clear();
            }
            ui.separator();
            if ui
                .add(egui::Button::selectable(self.three_point, t!("create.clip.three_point")))
                .on_hover_text(tf!("create.clip.three_point_tip", key_label(Key::C)))
                .clicked()
            {
                self.three_point = !self.three_point;
            }
            let mut measure = state.settings.brush.clip_measurements;
            if ui
                .checkbox(&mut measure, t!("create.clip.measurements"))
                .on_hover_text(tf!("create.clip.measurements_tip", key_label(Key::O)))
                .changed()
            {
                Self::toggle_measurements(state);
            }
            let mut current = state.settings.brush.clip_current_material;
            if ui
                .checkbox(&mut current, t!("create.clip.cap_material"))
                .on_hover_text(t!("create.clip.cap_material_tip"))
                .changed()
            {
                state.settings.brush.clip_current_material = current;
                state.settings_dirty = true;
            }
            if ready {
                ui.separator();
                ui.label(tf!("create.clip.crossing", cut));
            }
        });
    }

    fn cursor(&self, _state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<CursorIcon> {
        if self.drag.is_some_and(|d| d.view == view.index) {
            return Some(CursorIcon::Grabbing);
        }
        if self.handle_at(view, pos).is_some() {
            return Some(CursorIcon::Grab);
        }
        Some(CursorIcon::Crosshair)
    }

    fn status_hint(&self, state: &AppState) -> Option<String> {
        let escape = key_label(Key::Escape);
        let c = key_label(Key::C);
        Some(match &self.def {
            ClipDef::Points { points, plane: None } => {
                tf!("create.clip.hint_points", 3usize.saturating_sub(points.len()), escape)
            }
            ClipDef::Line { c: None, .. } if self.three_point => tf!("create.clip.hint_third", c, escape),
            _ if self.plane().is_some() => tf!(
                "create.clip.hint_ready",
                t!(keep_key(self.keep)),
                key_label(Key::Enter),
                Self::mode_keys(state),
                escape
            ),
            _ => tf!("create.clip.hint_idle", Self::mode_keys(state), c),
        })
    }

    fn cancel(&mut self, _state: &mut AppState) -> bool {
        self.clear()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::{info_2d, info_3d, settle};
    use egui::Modifiers;
    use hp_editor::ObjRef;

    #[test]
    fn plane_from_two_points_in_each_view() {
        let p = clip_plane_2d(ViewKind::Top, DVec2::new(0.0, 16.0), DVec2::new(64.0, 16.0)).expect("plane");
        assert!((p.normal - DVec3::NEG_Y).length() < 1e-12);
        assert!((p.dist + 16.0).abs() < 1e-12);
        let p = clip_plane_2d(ViewKind::Front, DVec2::new(32.0, 0.0), DVec2::new(32.0, 64.0)).expect("plane");
        assert!((p.normal - DVec3::Y).length() < 1e-12);
        assert!((p.dist - 32.0).abs() < 1e-12);
        let a = DVec2::new(0.0, 0.0);
        let b = DVec2::new(64.0, 64.0);
        let p = clip_plane_2d(ViewKind::Side, a, b).expect("plane");
        assert!(p.normal.y.abs() < 1e-12);
        for q in [ViewKind::Side.from_2d(a, 0.0), ViewKind::Side.from_2d(b, 123.0)] {
            assert!(p.distance(q).abs() < 1e-9);
        }
        assert!(clip_plane_2d(ViewKind::Top, a, a).is_none());
        assert!(clip_plane_2d(ViewKind::Perspective, a, b).is_none());
    }

    #[test]
    fn plane_from_three_points() {
        let a = DVec2::new(0.0, 64.0);
        let b = DVec2::new(0.0, -64.0);
        let base = clip_plane_2d(ViewKind::Top, a, b).expect("base");
        let p = clip_plane_3_points(ViewKind::Top, a, b, 0.0, DVec3::new(64.0, 0.0, 64.0)).expect("plane");
        assert!(p.normal.dot(base.normal) > 0.0, "same front side");
        for q in [DVec3::new(0.0, 64.0, 0.0), DVec3::new(0.0, -64.0, 0.0), DVec3::new(64.0, 0.0, 64.0)] {
            assert!(p.distance(q).abs() < 1e-9);
        }
        assert!((p.normal.dot(base.normal) - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9);
        let flat = clip_plane_3_points(ViewKind::Top, a, b, 0.0, DVec3::new(0.0, 0.0, 0.0)).expect("plane");
        assert!(flat.approx_eq(&base, 1e-9, 1e-9));
        assert!(clip_plane_3_points(ViewKind::Top, a, a, 0.0, DVec3::X).is_none());
    }

    #[test]
    fn mode_cycling() {
        assert_eq!(next_keep(ClipKeep::Both, false), ClipKeep::Front);
        assert_eq!(next_keep(ClipKeep::Front, false), ClipKeep::Back);
        assert_eq!(next_keep(ClipKeep::Back, false), ClipKeep::Both);
        for k in [ClipKeep::Both, ClipKeep::Front, ClipKeep::Back] {
            assert_eq!(next_keep(next_keep(k, false), true), k);
        }
    }

    fn clip_floor(keep: ClipKeep) -> (AppState, Vec<f64>) {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(2)]);
        let mut tool = ClipTool::new();
        tool.keep = keep;
        let view = info_2d(ViewKind::Top, 0.5);
        let a = view.world_to_screen(DVec3::new(3.0, 300.0, 0.0)).expect("a");
        let b = view.world_to_screen(DVec3::new(-2.0, -300.0, 0.0)).expect("b");
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(a, a));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(a, b));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(a, b));
        let plane = tool.plane().expect("plane");
        assert!((plane.normal - DVec3::NEG_X).length() < 1e-12, "{plane:?}");
        assert_eq!(tool.crossing_count(&state), 1);
        let (undo, _) = state.doc.history_len();
        assert!(tool.apply(&mut state));
        assert_eq!(state.doc.history_len().0, undo + 1, "one undo step");
        let mut volumes: Vec<f64> = state
            .doc
            .selected_solid_ids()
            .iter()
            .filter_map(|id| state.doc.scene().solid(*id))
            .map(|n| n.geom.volume())
            .collect();
        volumes.sort_by(f64::total_cmp);
        (state, volumes)
    }

    #[test]
    fn three_modes_on_the_test_map() {
        let half = 256.0 * 512.0 * 16.0;
        let (mut state, v) = clip_floor(ClipKeep::Both);
        assert_eq!(v.len(), 2);
        assert!((v[0] - half).abs() < 1e-6 && (v[1] - half).abs() < 1e-6);
        assert!(state.doc.undo());
        assert_eq!(state.doc.scene().solid(2).map(|n| n.geom.volume()), Some(2.0 * half));

        let (state, v) = clip_floor(ClipKeep::Front);
        assert_eq!(v.len(), 1);
        let b = state.doc.scene().solid(2).expect("floor").bounds;
        assert_eq!((b.min.x, b.max.x), (-256.0, 0.0), "front (-X) kept");
        assert!((v[0] - half).abs() < 1e-6);

        let (state, _) = clip_floor(ClipKeep::Back);
        let b = state.doc.scene().solid(2).expect("floor").bounds;
        assert_eq!((b.min.x, b.max.x), (0.0, 256.0), "back (+X) kept");
    }

    #[test]
    fn nothing_to_clip_and_no_selection() {
        let mut state = settle();
        state.doc.clear_selection();
        let mut tool = ClipTool::new();
        tool.set_line(ViewKind::Top, DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0));
        assert!(!tool.apply(&mut state));
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        tool.set_line(ViewKind::Top, DVec2::new(0.0, 1000.0), DVec2::new(10.0, 1000.0));
        assert_eq!(tool.crossing_count(&state), 0);
        assert!(!tool.apply(&mut state));
        assert!(state.console.len() >= 2);
    }

    #[test]
    fn dragging_a_clip_point() {
        let mut state = settle();
        let mut tool = ClipTool::new();
        tool.set_line(ViewKind::Front, DVec2::new(0.0, 0.0), DVec2::new(0.0, 64.0));
        let view = info_2d(ViewKind::Front, 1.0);
        let b = view.world_to_screen(DVec3::new(0.0, 0.0, 64.0)).expect("b");
        let to = view.world_to_screen(DVec3::new(0.0, 64.0, 64.0)).expect("to");
        tool.handle_mouse(&mut state, &view, &Mouse::hover(b));
        assert_eq!(tool.cursor(&state, &view, b), Some(CursorIcon::Grab));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(b, b));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(b, to));
        assert_eq!(
            tool.def,
            ClipDef::Line { kind: ViewKind::Front, a: DVec2::ZERO, b: DVec2::new(64.0, 64.0), depth: 0.0, c: None }
        );
        assert!(tool.cancel(&mut state));
        assert!(tool.plane().is_none());
    }

    #[test]
    fn ctrl_drag_moves_both_points_and_shift_starts_over() {
        let mut state = settle();
        let mut tool = ClipTool::new();
        tool.set_line(ViewKind::Top, DVec2::new(0.0, 0.0), DVec2::new(0.0, 64.0));
        let view = info_2d(ViewKind::Top, 1.0);
        let ctrl = Modifiers { ctrl: true, ..Modifiers::NONE };
        let from = view.world_to_screen(DVec3::new(0.0, 32.0, 0.0)).expect("from");
        let to = view.world_to_screen(DVec3::new(37.0, 40.0, 0.0)).expect("to");
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(from, from).with_modifiers(ctrl));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(from, to).with_modifiers(ctrl));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(from, to).with_modifiers(ctrl));
        let ClipDef::Line { a, b, .. } = tool.def.clone() else { panic!("line") };
        assert_eq!((a, b), (DVec2::new(32.0, 16.0), DVec2::new(32.0, 80.0)), "moved by a snapped offset");
        let shift = Modifiers { shift: true, ..Modifiers::NONE };
        let h = view.world_to_screen(DVec3::new(32.0, 16.0, 0.0)).expect("h");
        let end = view.world_to_screen(DVec3::new(96.0, 16.0, 0.0)).expect("end");
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(h, h).with_modifiers(shift));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(h, end).with_modifiers(shift));
        let ClipDef::Line { a, b, .. } = tool.def.clone() else { panic!("line") };
        assert_eq!((a, b), (DVec2::new(32.0, 16.0), DVec2::new(96.0, 16.0)));
    }

    #[test]
    fn three_point_mode_tilts_the_plane() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        let mut tool = ClipTool::new();
        tool.three_point = true;
        let top = info_2d(ViewKind::Top, 0.5);
        let a = top.world_to_screen(DVec3::new(0.0, 200.0, 0.0)).expect("a");
        let b = top.world_to_screen(DVec3::new(0.0, -200.0, 0.0)).expect("b");
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(a, a));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_end(a, b));
        let base = tool.plane().expect("plane");
        assert!(tool.status_hint(&state).is_some_and(|h| !h.is_empty()));
        let ClipDef::Line { depth, .. } = tool.def.clone() else { panic!("line") };
        let side = info_2d(ViewKind::Side, 1.0);
        let c = side.world_to_screen(DVec3::new(64.0, 0.0, depth + 64.0)).expect("c");
        tool.handle_mouse(&mut state, &side, &Mouse::click(c));
        let tilted = tool.plane().expect("plane");
        assert!((tilted.normal.dot(base.normal) - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9);
        assert!(tilted.normal.dot(base.normal) > 0.0);
        let c2 = side.world_to_screen(DVec3::new(128.0, 0.0, depth + 64.0)).expect("c2");
        tool.handle_mouse(&mut state, &side, &Mouse::drag_start(c, c));
        tool.handle_mouse(&mut state, &side, &Mouse::drag_end(c, c2));
        let more = tool.plane().expect("plane");
        assert!(more.normal.dot(base.normal) < tilted.normal.dot(base.normal));
        assert!(more.distance(DVec3::new(128.0, 0.0, depth + 64.0)).abs() < 1e-6);
        tool.three_point = false;
        assert!(tool.plane().expect("plane").approx_eq(&base, 1e-9, 1e-9));
    }

    #[test]
    fn three_points_in_3d() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        let mut tool = ClipTool::new();
        let view = info_3d(DVec3::new(0.0, 0.0, 64.0), 90.0, 0.0);
        let at = |p: DVec3| view.world_to_screen(p).expect("screen");
        tool.handle_mouse(&mut state, &view, &Mouse::click(at(DVec3::new(-64.0, 256.0, 32.0))));
        tool.handle_mouse(&mut state, &view, &Mouse::click(at(DVec3::new(64.0, 256.0, 96.0))));
        assert!(tool.plane().is_none());
        assert!(tool.status_hint(&state).is_some());
        tool.handle_mouse(&mut state, &view, &Mouse::click(at(DVec3::new(0.0, 128.0, 0.0))));
        let plane = tool.plane().expect("plane");
        assert!(plane.distance(view.cam3d.pos) > 0.0, "front faces the camera");
        assert_eq!(tool.crossing_count(&state), 1);
        let from = at(DVec3::new(0.0, 128.0, 0.0));
        let to = at(DVec3::new(0.0, 160.0, 0.0));
        tool.handle_mouse(&mut state, &view, &Mouse::hover(from));
        assert_eq!(tool.cursor(&state, &view, from), Some(CursorIcon::Grab));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(from, from));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(from, to));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(from, to));
        let moved = tool.plane().expect("plane");
        assert!(!moved.approx_eq(&plane, 1e-6, 1e-3), "the plane moved");
        assert!(moved.normal.dot(plane.normal) > 0.0);
        assert!(moved.distance(DVec3::new(0.0, 160.0, 0.0)).abs() < 1e-6);
        let top = info_2d(ViewKind::Top, 1.0);
        let p = top.world_to_screen(DVec3::new(0.0, 160.0, 0.0)).expect("p");
        let q = top.world_to_screen(DVec3::new(0.0, 192.0, 0.0)).expect("q");
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(p, p));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_end(p, q));
        assert!(tool.plane().expect("plane").distance(DVec3::new(0.0, 192.0, 0.0)).abs() < 1e-6);
    }

    #[test]
    fn current_material_on_the_cut_faces() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(2)]);
        state.current_material = "TOOLS/TOOLSNODRAW".to_owned();
        state.settings.brush.clip_current_material = true;
        let mut tool = ClipTool::new();
        tool.keep = ClipKeep::Back;
        tool.set_line(ViewKind::Top, DVec2::new(0.0, 300.0), DVec2::new(0.0, -300.0));
        assert!(tool.apply(&mut state));
        let sides = &state.doc.scene().solid(2).expect("floor").solid.sides;
        assert_eq!(sides.iter().filter(|s| s.material == "TOOLS/TOOLSNODRAW").count(), 1, "only the cut face");
    }

    #[test]
    fn keys_toggle_three_points_and_measurements() {
        let mut state = settle();
        let mut tool = ClipTool::new();
        assert!(state.settings.brush.clip_measurements);
        let press = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let input = egui::RawInput { events: vec![press(Key::C), press(Key::O)], ..Default::default() };
        let ctx = egui::Context::default();
        let mut handled = (false, false);
        let mut out = ctx.run_ui(input, |ui| {
            handled.0 = tool.key_input(&mut state, ui.ctx());
            handled.1 = tool.key_input(&mut state, ui.ctx());
        });
        out.textures_delta.clear();
        assert_eq!(handled, (true, true));
        assert!(tool.three_point);
        assert!(!state.settings.brush.clip_measurements);
        assert!(state.settings_dirty);
        tool.set_line(ViewKind::Top, DVec2::new(0.0, 0.0), DVec2::new(64.0, 64.0));
        tool.set_third_point(DVec3::new(0.0, 64.0, 64.0));
        state.settings.brush.clip_measurements = true;
        hp_app::testing::run_ctx(&mut state, |ctx, state| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
                tool.paint(state, &info_2d(kind, 1.0), &painter);
            }
        });
        assert_eq!(fmt_angle(45.04), "45");
        assert_eq!(fmt_angle(-26.56), "-26.6");
    }
}
