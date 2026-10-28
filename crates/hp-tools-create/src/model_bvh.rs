use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use glam::{DMat3, DVec3, Vec3};

const MAX_TRIANGLES: usize = 1 << 18;
const CAPACITY: usize = 64;
const TRIANGLE_BUDGET: usize = 2 << 20;
const LEAF: usize = 4;

#[derive(Clone, Copy, Debug)]
struct Node {
    min: Vec3,
    max: Vec3,
    first: u32,
    count: u32,
}

#[derive(Debug, Default)]
pub struct TriBvh {
    nodes: Vec<Node>,
    tris: Vec<[Vec3; 3]>,
}

fn ray_box(o: Vec3, inv: Vec3, min: Vec3, max: Vec3, limit: f32) -> Option<f32> {
    let t0 = (min - o) * inv;
    let t1 = (max - o) * inv;
    let near = t0.min(t1).max_element().max(0.0);
    let far = t0.max(t1).min_element().min(limit);
    (near <= far).then_some(near)
}

fn ray_tri(o: Vec3, d: Vec3, t: &[Vec3; 3]) -> Option<f32> {
    let e1 = t[1] - t[0];
    let e2 = t[2] - t[0];
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - t[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let dist = e2.dot(q) * inv;
    (dist > 1e-4).then_some(dist)
}

impl TriBvh {
    pub fn build(mut tris: Vec<[Vec3; 3]>) -> Self {
        tris.retain(|t| t.iter().all(|p| p.is_finite()) && (t[1] - t[0]).cross(t[2] - t[0]).length_squared() > 1e-12);
        tris.truncate(MAX_TRIANGLES);
        if tris.is_empty() {
            return Self::default();
        }
        let centroid = |t: &[Vec3; 3]| (t[0] + t[1] + t[2]) / 3.0;
        let mut nodes = Vec::with_capacity(2 * tris.len() / LEAF + 2);
        nodes.push(Node { min: Vec3::ZERO, max: Vec3::ZERO, first: 0, count: 0 });
        let mut stack = vec![(0usize, 0usize, tris.len())];
        while let Some((node, start, end)) = stack.pop() {
            let Some(slice) = tris.get_mut(start..end) else { continue };
            let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            let (mut cmin, mut cmax) = (min, max);
            for t in slice.iter() {
                for p in t {
                    min = min.min(*p);
                    max = max.max(*p);
                }
                let c = centroid(t);
                cmin = cmin.min(c);
                cmax = cmax.max(c);
            }
            let n = end - start;
            let extent = cmax - cmin;
            if n <= LEAF || extent.max_element() <= 0.0 {
                if let Some(nd) = nodes.get_mut(node) {
                    *nd = Node { min, max, first: start as u32, count: n as u32 };
                }
                continue;
            }
            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            let mid = n / 2;
            slice.select_nth_unstable_by(mid, |a, b| centroid(a)[axis].total_cmp(&centroid(b)[axis]));
            let left = nodes.len();
            nodes.push(Node { min: Vec3::ZERO, max: Vec3::ZERO, first: 0, count: 0 });
            nodes.push(Node { min: Vec3::ZERO, max: Vec3::ZERO, first: 0, count: 0 });
            if let Some(nd) = nodes.get_mut(node) {
                *nd = Node { min, max, first: left as u32, count: 0 };
            }
            stack.push((left, start, start + mid));
            stack.push((left + 1, start + mid, end));
        }
        Self { nodes, tris }
    }

    pub fn len(&self) -> usize {
        self.tris.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    pub fn ray(&self, origin: DVec3, dir: DVec3) -> Option<(f64, DVec3)> {
        if self.nodes.is_empty() || !origin.is_finite() || !dir.is_finite() || dir.length_squared() < 1e-24 {
            return None;
        }
        let (o, d) = (origin.as_vec3(), dir.as_vec3());
        let inv = Vec3::ONE / d;
        let mut best: Option<(f32, usize)> = None;
        let mut stack: Vec<usize> = vec![0];
        while let Some(i) = stack.pop() {
            let Some(n) = self.nodes.get(i) else { continue };
            let limit = best.map_or(f32::INFINITY, |(t, _)| t);
            if ray_box(o, inv, n.min, n.max, limit).is_none() {
                continue;
            }
            if n.count > 0 {
                let start = n.first as usize;
                for k in start..start + n.count as usize {
                    let Some(tri) = self.tris.get(k) else { continue };
                    if let Some(t) = ray_tri(o, d, tri)
                        && best.is_none_or(|(b, _)| t < b)
                    {
                        best = Some((t, k));
                    }
                }
            } else {
                stack.push(n.first as usize);
                stack.push(n.first as usize + 1);
            }
        }
        let (t, k) = best?;
        let tri = self.tris.get(k)?;
        let n = (tri[1] - tri[0]).cross(tri[2] - tri[0]).as_dvec3().try_normalize()?;
        let n = if n.dot(dir) > 0.0 { -n } else { n };
        Some((f64::from(t), n))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelHit {
    pub t: f64,
    pub point: DVec3,
    pub normal: DVec3,
}

pub fn ray_placed(bvh: &TriBvh, origin: DVec3, angles: DVec3, ray_o: DVec3, ray_d: DVec3) -> Option<ModelHit> {
    let m: DMat3 = hp_geom::angle_matrix(angles);
    if !m.is_finite() {
        return None;
    }
    let inv = m.transpose();
    let (t, n_local) = bvh.ray(inv * (ray_o - origin), inv * ray_d)?;
    let normal = (m * n_local).try_normalize()?;
    let point = ray_o + ray_d * t;
    point.is_finite().then_some(ModelHit { t, point, normal })
}

#[derive(Clone, Debug)]
pub enum Surface {
    Loading,
    Ready(Arc<TriBvh>),
    Missing,
}

struct Entry {
    surface: Surface,
    last_used: u64,
}

type Done = (String, Option<TriBvh>);

pub struct ModelSurfaces {
    entries: HashMap<String, Entry>,
    jobs: Option<Sender<(String, hp_fs::GameFs)>>,
    done_tx: Sender<Done>,
    done: Receiver<Done>,
    clock: u64,
    generation: Option<u64>,
    version: u64,
}

impl Default for ModelSurfaces {
    fn default() -> Self {
        let (done_tx, done) = channel();
        Self { entries: HashMap::new(), jobs: None, done_tx, done, clock: 0, generation: None, version: 0 }
    }
}

impl std::fmt::Debug for ModelSurfaces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelSurfaces").field("models", &self.entries.len()).finish_non_exhaustive()
    }
}

fn key(path: &str) -> String {
    path.trim().replace('\\', "/").to_ascii_lowercase()
}

fn load(path: &str, fs: &hp_fs::GameFs) -> Option<TriBvh> {
    let model = hp_mdl::load_from(path, |p| fs.read(p), &hp_mdl::LoadOptions::default()).ok()?;
    let count: usize = model.meshes.iter().map(|m| m.triangle_count()).sum();
    if count == 0 || count > MAX_TRIANGLES {
        return None;
    }
    let mut tris = Vec::with_capacity(count);
    for mesh in &model.meshes {
        for t in mesh.indices.chunks_exact(3) {
            let v = |i: u32| mesh.vertices.get(i as usize).map(|v| Vec3::from_array(v.pos));
            if let (Some(a), Some(b), Some(c)) = (v(t[0]), v(t[1]), v(t[2])) {
                tris.push([a, b, c]);
            }
        }
    }
    let bvh = TriBvh::build(tris);
    (!bvh.is_empty()).then_some(bvh)
}

impl ModelSurfaces {
    pub fn poll(&mut self, generation: u64) {
        if self.generation.is_some_and(|g| g != generation) {
            self.entries.clear();
            self.version = self.version.wrapping_add(1);
        }
        self.generation = Some(generation);
        while let Ok((path, bvh)) = self.done.try_recv() {
            if let Some(e) = self.entries.get_mut(&path) {
                e.surface = match bvh {
                    Some(b) => Surface::Ready(Arc::new(b)),
                    None => Surface::Missing,
                };
                self.version = self.version.wrapping_add(1);
            }
            self.evict();
        }
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn get(&mut self, path: &str, fs: Option<&hp_fs::GameFs>, repaint: Option<&egui::Context>) -> Surface {
        let k = key(path);
        self.clock += 1;
        let clock = self.clock;
        if let Some(e) = self.entries.get_mut(&k) {
            e.last_used = clock;
            return e.surface.clone();
        }
        let Some(fs) = fs else { return Surface::Missing };
        self.evict();
        if self.jobs.is_none() {
            let (tx, rx) = channel::<(String, hp_fs::GameFs)>();
            let done = self.done_tx.clone();
            let repaint = repaint.cloned();
            let spawned = std::thread::Builder::new().name("hp-model-surfaces".to_owned()).spawn(move || {
                while let Ok((path, fs)) = rx.recv() {
                    let bvh = load(&path, &fs);
                    if done.send((path, bvh)).is_err() {
                        return;
                    }
                    if let Some(ctx) = &repaint {
                        ctx.request_repaint();
                    }
                }
            });
            match spawned {
                Ok(_) => self.jobs = Some(tx),
                Err(e) => log::warn!("cannot start the model surface worker: {e}"),
            }
        }
        let sent = self.jobs.as_ref().is_some_and(|tx| tx.send((k.clone(), fs.clone())).is_ok());
        let surface = if sent { Surface::Loading } else { Surface::Missing };
        self.entries.insert(k, Entry { surface: surface.clone(), last_used: clock });
        surface
    }

    pub fn insert(&mut self, path: &str, bvh: TriBvh) {
        self.clock += 1;
        self.entries.insert(key(path), Entry { surface: Surface::Ready(Arc::new(bvh)), last_used: self.clock });
        self.version = self.version.wrapping_add(1);
    }

    fn triangles(&self) -> usize {
        self.entries
            .values()
            .map(|e| match &e.surface {
                Surface::Ready(b) => b.len(),
                _ => 0,
            })
            .sum()
    }

    fn evict(&mut self) {
        while self.entries.len() >= CAPACITY || self.triangles() > TRIANGLE_BUDGET {
            let oldest = self
                .entries
                .iter()
                .filter(|(_, e)| !matches!(e.surface, Surface::Loading))
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => {
                    self.entries.remove(&k);
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn cube(half: f32) -> TriBvh {
        let c = |x: f32, y: f32, z: f32| Vec3::new(x, y, z) * half;
        let corners = [
            c(-1.0, -1.0, -1.0),
            c(1.0, -1.0, -1.0),
            c(-1.0, 1.0, -1.0),
            c(1.0, 1.0, -1.0),
            c(-1.0, -1.0, 1.0),
            c(1.0, -1.0, 1.0),
            c(-1.0, 1.0, 1.0),
            c(1.0, 1.0, 1.0),
        ];
        let faces = [[0, 2, 6, 4], [1, 5, 7, 3], [0, 4, 5, 1], [2, 3, 7, 6], [0, 1, 3, 2], [4, 6, 7, 5]];
        let mut tris = Vec::new();
        for f in faces {
            tris.push([corners[f[0]], corners[f[1]], corners[f[2]]]);
            tris.push([corners[f[0]], corners[f[2]], corners[f[3]]]);
        }
        TriBvh::build(tris)
    }

    #[test]
    fn rays_hit_the_closest_triangle() {
        let b = cube(8.0);
        assert_eq!(b.len(), 12);
        let (t, n) = b.ray(DVec3::new(1.0, 2.0, 100.0), DVec3::NEG_Z).expect("hit");
        assert!((t - 92.0).abs() < 1e-4, "{t}");
        assert!((n - DVec3::Z).length() < 1e-6);
        assert!(b.ray(DVec3::new(20.0, 0.0, 100.0), DVec3::NEG_Z).is_none());
        assert!(b.ray(DVec3::NAN, DVec3::Z).is_none());
        assert!(b.ray(DVec3::ZERO, DVec3::ZERO).is_none());
        let mut tris = Vec::new();
        for i in 0..100 {
            for j in 0..100 {
                let (x, y) = (i as f32, j as f32);
                let z = (x * 0.1).sin();
                tris.push([Vec3::new(x, y, z), Vec3::new(x + 1.0, y, z), Vec3::new(x, y + 1.0, z)]);
                tris.push([Vec3::new(x + 1.0, y, z), Vec3::new(x + 1.0, y + 1.0, z), Vec3::new(x, y + 1.0, z)]);
            }
        }
        let grid = TriBvh::build(tris);
        let (t, _) = grid.ray(DVec3::new(50.5, 50.5, 10.0), DVec3::NEG_Z).expect("grid hit");
        let z = f64::from((50.0f32 * 0.1).sin());
        assert!((10.0 - t - z).abs() < 1e-3);
        assert!(TriBvh::build(vec![[Vec3::ZERO; 3], [Vec3::NAN, Vec3::X, Vec3::Y]]).is_empty());
    }

    #[test]
    fn placed_models_turn_with_their_angles() {
        let tris = vec![
            [Vec3::new(-8.0, -8.0, 4.0), Vec3::new(8.0, -8.0, 4.0), Vec3::new(8.0, 8.0, 4.0)],
            [Vec3::new(-8.0, -8.0, 4.0), Vec3::new(8.0, 8.0, 4.0), Vec3::new(-8.0, 8.0, 4.0)],
        ];
        let b = TriBvh::build(tris);
        let origin = DVec3::new(100.0, 0.0, 10.0);
        let hit = ray_placed(&b, origin, DVec3::ZERO, DVec3::new(101.0, 1.0, 100.0), DVec3::NEG_Z).expect("hit");
        assert!((hit.point.z - 14.0).abs() < 1e-4);
        assert!((hit.normal - DVec3::Z).length() < 1e-6);
        let hit = ray_placed(&b, origin, DVec3::new(90.0, 0.0, 0.0), DVec3::new(200.0, 1.0, 11.0), DVec3::NEG_X)
            .expect("turned");
        assert!((hit.point.x - 104.0).abs() < 1e-3, "{hit:?}");
        assert!((hit.normal - DVec3::X).length() < 1e-4, "{hit:?}");
    }

    #[test]
    fn cache_without_content_and_with_injected_models() {
        let mut s = ModelSurfaces::default();
        assert!(matches!(s.get("models/a.mdl", None, None), Surface::Missing));
        let v = s.version();
        s.insert("Models\\A.mdl", cube(4.0));
        assert_ne!(s.version(), v);
        assert!(matches!(s.get("models/a.mdl", None, None), Surface::Ready(_)));
        s.poll(6);
        assert!(matches!(s.get("models/a.mdl", None, None), Surface::Ready(_)), "first poll keeps it");
        s.poll(7);
        assert!(matches!(s.get("models/a.mdl", None, None), Surface::Missing), "content changed");
        let fs = hp_fs::GameFs::empty();
        assert!(matches!(s.get("models/none.mdl", Some(&fs), None), Surface::Loading));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            s.poll(7);
            if matches!(s.get("models/none.mdl", Some(&fs), None), Surface::Missing) {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn real_models_when_available() {
        let Some(root) = hp_fs::detect_gmod_root() else { return };
        let opts = hp_fs::MountOptions { mount_workshop: false, mount_addons: false, ..Default::default() };
        let Ok(fs) = hp_fs::GameFs::for_gmod(&root, &opts) else { return };
        let path = "models/props_junk/wood_crate001a.mdl";
        if !fs.exists(path) {
            return;
        }
        let bvh = load(path, &fs).expect("crate loads");
        assert!(bvh.len() > 10);
        let (t, n) = bvh.ray(DVec3::new(0.0, 0.0, 500.0), DVec3::NEG_Z).expect("crate top");
        assert!(t > 400.0 && t < 500.0, "{t}");
        assert!(n.z > 0.9, "{n:?}");
    }
}
