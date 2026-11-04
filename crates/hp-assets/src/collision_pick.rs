//! Ray tests against the collision mesh (`.phy`) of model entities.
//!
//! Selection in the 3D view uses this instead of the model's bounding box, which is far
//! less precise (a bounding box also catches the empty space around the model).

use glam::{DVec3, Vec3};
use hp_app::state::CollisionTriangles;

use crate::AssetManager;
use crate::entities::{class_of, model_path, model_scale};
use crate::models::entity_matrix;
use crate::render::load_collision_triangles;
use crate::util::model_key_cow;

/// Load state of the collision mesh of one model.
#[derive(Clone)]
pub(crate) enum CollisionSlot {
    Pending,
    /// `None`: the model has no usable collision mesh.
    Ready(Option<CollisionTriangles>),
}

/// Two sided Moeller-Trumbore. Returns the ray parameter of the hit.
fn ray_triangle(o: DVec3, d: DVec3, tri: &[[f32; 3]; 3]) -> Option<f64> {
    let a = Vec3::from(tri[0]).as_dvec3();
    let b = Vec3::from(tri[1]).as_dvec3();
    let c = Vec3::from(tri[2]).as_dvec3();
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-6 && t.is_finite()).then_some(t)
}

impl AssetManager {
    fn request_collision(&self, key: &str) {
        let Some(fs) = self.fs.clone() else { return };
        {
            let mut map = self.shared.collision.write();
            if map.contains_key(key) {
                return;
            }
            map.insert(key.to_owned(), CollisionSlot::Pending);
        }
        let shared = std::sync::Arc::clone(&self.shared);
        let key = key.to_owned();
        rayon::spawn(move || {
            let tris = load_collision_triangles(&fs, &key);
            shared.collision.write().insert(key, CollisionSlot::Ready(tris));
        });
    }

    /// Ray test against the collision mesh of a model entity.
    ///
    /// * `None`: no collision information (not a model, no `.phy`, or still loading). The caller
    ///   keeps using the bounding box.
    /// * `Some(None)`: the collision mesh is known and the ray misses it.
    /// * `Some(Some(t))`: the ray hits the collision mesh at parameter `t`.
    pub(crate) fn entity_collision_ray_impl(
        &self,
        e: &hp_vmf::Entity,
        origin: DVec3,
        dir: DVec3,
    ) -> Option<Option<f64>> {
        let class = class_of(self.fgd.as_deref(), e);
        let path = model_path(e, class)?;
        let key = model_key_cow(&path).into_owned();
        let slot = self.shared.collision.read().get(&key).cloned();
        let tris = match slot {
            Some(CollisionSlot::Ready(tris)) => tris?,
            Some(CollisionSlot::Pending) => return None,
            None => {
                self.request_collision(&key);
                return None;
            }
        };
        let pos = e.origin().filter(|o| o.is_finite()).unwrap_or(DVec3::ZERO);
        let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
        let m = entity_matrix(pos, angles, model_scale(e));
        if !m.is_finite() || m.determinant().abs() < 1e-18 {
            return None;
        }
        let inv = m.inverse();
        let (lo, ld) = (inv.transform_point3(origin), inv.transform_vector3(dir));
        let mut best: Option<f64> = None;
        for tri in tris.iter() {
            if let Some(t) = ray_triangle(lo, ld, tri)
                && best.is_none_or(|b| t < b)
            {
                best = Some(t);
            }
        }
        Some(best)
    }
}
