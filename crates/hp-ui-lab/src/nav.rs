use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use crossbeam_channel::Receiver;
use hp_app::AppState;
use hp_app::view::ViewInfo;
use hp_nav::{NavMesh, NavStats, attr};
use hp_render::{Overlay, OverlayLine, OverlayTri};
use hp_ui_viewports::layers::OverlayLayer;

const LIFT: f32 = 2.0;

#[derive(Debug)]
pub struct NavDisplay {
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
    pub mesh: NavMesh,
    pub stats: NavStats,
    pub groups: Vec<u32>,
    pub serial: u64,
}

pub type NavHandle = Arc<RwLock<Option<Arc<NavDisplay>>>>;

pub fn new_handle() -> NavHandle {
    Arc::new(RwLock::new(None))
}

pub fn current(handle: &NavHandle) -> Option<Arc<NavDisplay>> {
    handle.read().ok().and_then(|g| g.clone())
}

pub fn set_current(handle: &NavHandle, display: Option<Arc<NavDisplay>>) {
    if let Ok(mut g) = handle.write() {
        *g = display;
    }
}

pub fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

pub fn find_nav_file(state: &AppState) -> Option<PathBuf> {
    if let Some(p) = &state.runtime.lab.nav_file {
        return Some(p.clone());
    }
    let source = state.doc.path().or(state.runtime.lab.decompiled_from.as_deref())?;
    let stem = source.file_stem()?.to_string_lossy().into_owned();
    let beside = source.with_file_name(format!("{stem}.nav"));
    if beside.is_file() {
        return Some(beside);
    }
    let game = state.assets.fs().and_then(|fs| fs.game_dir().map(Path::to_path_buf))?;
    let in_game = game.join("maps").join(format!("{stem}.nav"));
    in_game.is_file().then_some(in_game)
}

pub fn spawn_points(doc: &hp_editor::Document) -> Vec<[f32; 3]> {
    doc.scene()
        .entities()
        .values()
        .filter(|n| {
            let c = n.entity.classname.to_ascii_lowercase();
            c.starts_with("info_player_") || c == "gmod_player_start"
        })
        .filter_map(|n| n.entity.origin().filter(|o| o.is_finite()))
        .map(|o| [o.x as f32, o.y as f32, o.z as f32])
        .collect()
}

pub fn load(path: &Path, serial: u64, spawns: &[[f32; 3]]) -> Result<NavDisplay, hp_nav::NavError> {
    let mtime = modified(path);
    let mesh = hp_nav::load(path)?;
    let stats = mesh.stats_from(spawns);
    let (groups, _) = mesh.components_from(spawns);
    Ok(NavDisplay { path: path.to_path_buf(), modified: mtime, mesh, stats, groups, serial })
}

pub struct NavJob {
    pub path: PathBuf,
    rx: Receiver<Result<NavDisplay, String>>,
}

impl NavJob {
    pub fn start(path: PathBuf, serial: u64, spawns: Vec<[f32; 3]>, ctx: Option<egui::Context>) -> Option<NavJob> {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let worker_path = path.clone();
        std::thread::Builder::new()
            .name("hp-nav-load".to_owned())
            .spawn(move || {
                let result = load(&worker_path, serial, &spawns).map_err(|e| e.to_string());
                let _ = tx.send(result);
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
            })
            .ok()?;
        Some(NavJob { path, rx })
    }

    pub fn poll(&self) -> Option<Result<NavDisplay, String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(crossbeam_channel::TryRecvError::Empty) => None,
            Err(crossbeam_channel::TryRecvError::Disconnected) => Some(Err("the reader stopped".to_owned())),
        }
    }
}

fn area_color(flags: u32, isolated: bool) -> [f32; 4] {
    if isolated {
        return [0.95, 0.25, 0.85, 1.0];
    }
    if flags & (attr::AVOID | attr::NAV_BLOCKER) != 0 {
        [0.95, 0.3, 0.25, 1.0]
    } else if flags & attr::JUMP != 0 {
        [0.95, 0.85, 0.25, 1.0]
    } else if flags & attr::CROUCH != 0 {
        [0.3, 0.55, 1.0, 1.0]
    } else if flags & attr::STAIRS != 0 {
        [0.3, 0.9, 0.9, 1.0]
    } else {
        [0.3, 0.9, 0.4, 1.0]
    }
}

fn with_alpha(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

fn lift(p: [f32; 3]) -> [f32; 3] {
    [p[0], p[1], p[2] + LIFT]
}

pub fn build_overlay(d: &NavDisplay, is_3d: bool, fill: bool, connections: bool, out: &mut Overlay) {
    let mesh = &d.mesh;
    let finite = |p: &[f32; 3]| p.iter().all(|v| v.is_finite() && v.abs() < 1.0e6);
    for (i, a) in mesh.areas.iter().enumerate() {
        let c = a.corners();
        if !c.iter().all(finite) {
            continue;
        }
        let isolated = d.groups.get(i).is_some_and(|&g| g != 0);
        let color = area_color(a.flags, isolated);
        let c = c.map(lift);
        if is_3d && fill {
            let special = attr::AVOID | attr::NAV_BLOCKER | attr::JUMP | attr::CROUCH | attr::STAIRS;
            let fc = if isolated || a.flags & special != 0 { with_alpha(color, 0.3) } else { [0.2, 0.75, 1.0, 0.16] };
            out.triangles.push(OverlayTri { p: [c[0], c[1], c[2]], color: fc });
            out.triangles.push(OverlayTri { p: [c[0], c[2], c[3]], color: fc });
        }
        let lc = with_alpha(color, if isolated { 1.0 } else { 0.75 });
        for k in 0..4 {
            out.lines.push(OverlayLine { a: c[k], b: c[(k + 1) % 4], color: lc });
        }
    }
    if connections {
        let index = mesh.index_by_id();
        for (i, a) in mesh.areas.iter().enumerate() {
            let from = lift(a.center());
            if !finite(&from) {
                continue;
            }
            for (dir, id) in a.all_connections() {
                let Some(&j) = index.get(&id) else { continue };
                let b = &mesh.areas[j];
                let two_way = b.all_connections().any(|(_, x)| x == a.id);
                if two_way && j < i {
                    continue;
                }
                let edge = lift(a.edge_center(dir));
                let to = lift(b.center());
                if !finite(&edge) || !finite(&to) {
                    continue;
                }
                if two_way {
                    let col = [1.0, 1.0, 1.0, 0.45];
                    out.lines.push(OverlayLine { a: from, b: edge, color: col });
                    out.lines.push(OverlayLine { a: edge, b: to, color: col });
                } else {
                    let col = [1.0, 0.6, 0.15, 0.95];
                    out.lines.push(OverlayLine { a: from, b: edge, color: col });
                    out.lines.push(OverlayLine { a: edge, b: to, color: col });
                    arrow(out, edge, to, col);
                }
            }
        }
    }
    for l in &mesh.ladders {
        if !finite(&l.top) || !finite(&l.bottom) || !l.width.is_finite() {
            continue;
        }
        let half = (l.width * 0.5).clamp(0.0, 512.0);
        let (dx, dy) = if l.dir % 2 == 0 { (half, 0.0) } else { (0.0, half) };
        let col = [0.7, 0.4, 1.0, 1.0];
        let p = |b: [f32; 3], s: f32| [b[0] + dx * s, b[1] + dy * s, b[2]];
        let corners = [p(l.bottom, -1.0), p(l.bottom, 1.0), p(l.top, 1.0), p(l.top, -1.0)];
        for k in 0..4 {
            out.lines.push(OverlayLine { a: corners[k], b: corners[(k + 1) % 4], color: col });
        }
    }
}

fn arrow(out: &mut Overlay, from: [f32; 3], to: [f32; 3], color: [f32; 4]) {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let size = len.min(16.0) * 0.6;
    let mid = [(from[0] + to[0]) * 0.5, (from[1] + to[1]) * 0.5, (from[2] + to[2]) * 0.5];
    let back = [mid[0] - ux * size, mid[1] - uy * size, mid[2]];
    for s in [-1.0f32, 1.0] {
        let side = [back[0] - uy * size * 0.5 * s, back[1] + ux * size * 0.5 * s, back[2]];
        out.lines.push(OverlayLine { a: mid, b: side, color });
    }
}

type OverlayKey = (u64, bool, bool, bool);

pub struct NavLayer {
    handle: NavHandle,
    shown: Option<Arc<NavDisplay>>,
    cache: RefCell<Vec<(OverlayKey, Overlay)>>,
}

impl NavLayer {
    pub fn new(handle: NavHandle) -> Self {
        Self { handle, shown: None, cache: RefCell::new(Vec::new()) }
    }
}

impl OverlayLayer for NavLayer {
    fn name(&self) -> &'static str {
        "lab_nav_mesh"
    }

    fn prepare(&mut self, state: &AppState) {
        self.shown = if state.settings.lab.show_nav_mesh { current(&self.handle) } else { None };
        if self.shown.is_none() {
            self.cache.borrow_mut().clear();
        }
    }

    fn retained_version(&self, state: &AppState, view: &ViewInfo) -> Option<u64> {
        let d = self.shown.as_ref()?;
        let s = &state.settings.lab;
        let is_3d = !view.kind.is_2d();
        Some(
            d.serial.wrapping_mul(16)
                ^ (u64::from(is_3d) | (u64::from(s.nav_fill) << 1) | (u64::from(s.nav_connections) << 2)),
        )
    }

    fn retained(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let Some(d) = self.shown.as_ref() else { return };
        let s = &state.settings.lab;
        let key = (d.serial, !view.kind.is_2d(), s.nav_fill, s.nav_connections);
        let mut cache = self.cache.borrow_mut();
        cache.retain(|(k, _)| k.0 == d.serial);
        if let Some((_, o)) = cache.iter().find(|(k, _)| *k == key) {
            extend(out, o);
            return;
        }
        let mut built = Overlay::default();
        build_overlay(d, key.1, key.2, key.3, &mut built);
        extend(out, &built);
        cache.push((key, built));
    }
}

fn extend(out: &mut Overlay, o: &Overlay) {
    out.lines.extend_from_slice(&o.lines);
    out.lines_on_top.extend_from_slice(&o.lines_on_top);
    out.triangles.extend_from_slice(&o.triangles);
    out.triangles_on_top.extend_from_slice(&o.triangles_on_top);
    out.points.extend_from_slice(&o.points);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> NavDisplay {
        let area = |id: u32, x: f32, links: [Vec<u32>; 4]| hp_nav::NavArea {
            id,
            nw: [x, 0.0, 0.0],
            se: [x + 50.0, 50.0, 0.0],
            connections: links,
            ..Default::default()
        };
        let mesh = NavMesh {
            version: 16,
            areas: vec![
                area(1, 0.0, [vec![], vec![2], vec![], vec![]]),
                area(2, 50.0, [vec![], vec![], vec![], vec![1]]),
                area(3, 100.0, [vec![], vec![], vec![], vec![2]]),
                area(4, 500.0, [vec![], vec![], vec![], vec![]]),
            ],
            ladders: vec![hp_nav::NavLadder { width: 20.0, top: [0.0, 0.0, 100.0], ..Default::default() }],
            ..Default::default()
        };
        let stats = mesh.stats();
        let (groups, _) = hp_nav::area_components(&mesh);
        NavDisplay { path: PathBuf::from("x.nav"), modified: None, mesh, stats, groups, serial: 1 }
    }

    #[test]
    fn overlay_has_fills_outlines_links_and_ladders() {
        let d = sample();
        let mut o = Overlay::default();
        build_overlay(&d, true, true, true, &mut o);
        assert_eq!(o.triangles.len(), 8, "two triangles per area");
        assert_eq!(o.lines.len(), 16 + 2 + 4 + 4);
        assert!(o.lines.iter().any(|l| l.color[0] > 0.9 && l.color[2] > 0.8));
        let mut flat = Overlay::default();
        build_overlay(&d, false, true, false, &mut flat);
        assert!(flat.triangles.is_empty(), "no fill in 2D");
        assert_eq!(flat.lines.len(), 16 + 4);
    }

    #[test]
    fn the_layer_draws_only_when_shown() {
        let handle = new_handle();
        let mut layer = NavLayer::new(handle.clone());
        let mut state = hp_app::testing::test_state();
        let view = ViewInfo {
            index: 0,
            kind: hp_app::ViewKind::Perspective,
            rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0)),
            cam3d: hp_app::view::Camera3d::default(),
            cam2d: hp_app::view::Camera2d { center: glam::DVec2::ZERO, zoom: 1.0 },
            pixels_per_point: 1.0,
            grid: 16.0,
        };
        layer.prepare(&state);
        assert!(layer.retained_version(&state, &view).is_none());
        set_current(&handle, Some(Arc::new(sample())));
        layer.prepare(&state);
        assert!(layer.retained_version(&state, &view).is_none(), "off by default");
        state.settings.lab.show_nav_mesh = true;
        layer.prepare(&state);
        let v1 = layer.retained_version(&state, &view).expect("version");
        let mut out = Overlay::default();
        layer.retained(&state, &view, &mut out);
        assert!(!out.triangles.is_empty());
        state.settings.lab.nav_fill = false;
        assert_ne!(layer.retained_version(&state, &view), Some(v1));
    }

    #[test]
    fn nav_files_are_found_next_to_the_map() {
        let dir = std::env::temp_dir().join(format!("hp_lab_nav_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let mut state = hp_app::testing::test_state();
        assert_eq!(find_nav_file(&state), None, "no path: nothing");
        state.doc.set_path(Some(dir.join("gm_x.vmf")));
        assert_eq!(find_nav_file(&state), None);
        std::fs::write(dir.join("gm_x.nav"), b"x").expect("write");
        assert_eq!(find_nav_file(&state), Some(dir.join("gm_x.nav")));
        state.runtime.lab.nav_file = Some(PathBuf::from("chosen.nav"));
        assert_eq!(find_nav_file(&state), Some(PathBuf::from("chosen.nav")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
