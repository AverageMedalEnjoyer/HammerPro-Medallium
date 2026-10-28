use glam::DVec3;
use hp_geom::{Aabb, Primitive, PrimitiveParams};

use super::checks::MaterialFacts;
use crate::scene::Owner;
use crate::{Document, EditError, EditResult, Id, ObjRef};

const MAX_GROW: f64 = 64.0;
const STEP: f64 = 1.0;
pub const AREAPORTAL_MATERIAL: &str = "TOOLS/TOOLSAREAPORTAL";

fn in_world(doc: &Document, solids: &[Id], p: DVec3) -> bool {
    solids.iter().any(|id| {
        doc.scene
            .solids
            .get(id)
            .is_some_and(|n| n.bounds.contains_point(p) && n.geom.faces.iter().all(|f| f.plane.distance(p) < -0.01))
    })
}

pub fn fit_opening(doc: &Document, door: Aabb) -> Option<Aabb> {
    if door.is_empty() || !door.min.is_finite() || !door.max.is_finite() {
        return None;
    }
    let size = door.size();
    if size.min_element() <= 0.0 || size.max_element() > 4096.0 {
        return None;
    }
    let (k, u) = if size.x <= size.y { (0, 1) } else { (1, 0) };
    let center = door.center()[k];
    let half = (size[k] * 0.25).clamp(0.5, 4.0);
    let reach = door.expand(MAX_GROW + STEP);
    let solids: Vec<Id> = doc
        .scene
        .solids
        .values()
        .filter(|n| n.owner == Owner::World && n.geom.valid && !n.has_disp() && n.bounds.intersects(&reach))
        .filter(|n| n.solid.sides.iter().all(|s| MaterialFacts::guess(&s.material).seals()))
        .map(|n| n.solid.id)
        .collect();
    let (mut u0, mut u1, mut z0, mut z1) = (door.min[u], door.max[u], door.min.z, door.max.z);
    let point = |uu: f64, zz: f64| {
        let mut p = DVec3::ZERO;
        p[k] = center;
        p[u] = uu;
        p.z = zz;
        p
    };
    let sealed_u = |uu: f64, z0: f64, z1: f64| {
        (0..3).all(|i| in_world(doc, &solids, point(uu, z0 + (z1 - z0) * (f64::from(i) + 0.5) / 3.0)))
    };
    let sealed_z = |zz: f64, u0: f64, u1: f64| {
        (0..3).all(|i| in_world(doc, &solids, point(u0 + (u1 - u0) * (f64::from(i) + 0.5) / 3.0, zz)))
    };
    let grow = |start: f64, dir: f64, sealed: &dyn Fn(f64) -> bool| {
        let mut d = 0.0;
        while d <= MAX_GROW {
            let at = start + dir * d;
            if sealed(at) {
                return at;
            }
            d += STEP;
        }
        start
    };
    u0 = grow(u0, -1.0, &|x| sealed_u(x, z0, z1));
    u1 = grow(u1, 1.0, &|x| sealed_u(x, z0, z1));
    z0 = grow(z0, -1.0, &|x| sealed_z(x, u0, u1));
    z1 = grow(z1, 1.0, &|x| sealed_z(x, u0, u1));
    let mut min = DVec3::ZERO;
    let mut max = DVec3::ZERO;
    min[k] = center - half;
    max[k] = center + half;
    min[u] = u0;
    max[u] = u1;
    min.z = z0;
    max.z = z1;
    Some(Aabb::new(min, max))
}

fn starts_open(e: &hp_vmf::Entity) -> bool {
    let num = |k: &str| e.get(k).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
    num("spawnpos") != 0 || (e.classname.to_ascii_lowercase().starts_with("func_door") && num("spawnflags") & 1 != 0)
}

impl Document {
    pub fn seal_opening(&mut self, door: Id, portal: Aabb) -> EditResult<Id> {
        let Some(d) = self.scene.entities.get(&door) else {
            return Err(EditError::NotFound(ObjRef::Entity(door)));
        };
        if portal.is_empty() || portal.size().min_element() <= 0.0 || !portal.min.is_finite() || !portal.max.is_finite()
        {
            return Err(EditError::Invalid("empty areaportal box".to_owned()));
        }
        let open = starts_open(&d.entity);
        let existing = d.entity.get("targetname").map(str::trim).filter(|n| !n.is_empty()).map(str::to_owned);
        let name = match existing {
            Some(n) => n,
            None => {
                let taken = |n: &str| {
                    self.scene
                        .entities
                        .values()
                        .any(|o| o.entity.get("targetname").is_some_and(|t| t.trim().eq_ignore_ascii_case(n)))
                };
                let mut n = format!("door_{door}");
                let mut i = 2;
                while taken(&n) {
                    n = format!("door_{door}_{i}");
                    i += 1;
                }
                n
            }
        };
        let params = PrimitiveParams {
            kind: Primitive::Block,
            bounds: portal,
            material: AREAPORTAL_MATERIAL.to_owned(),
            ..PrimitiveParams::default()
        };
        self.tx("Seal opening", |doc| {
            if doc
                .scene
                .entities
                .get(&door)
                .is_some_and(|n| n.entity.get("targetname").is_none_or(|t| t.trim().is_empty()))
            {
                doc.update_entity(door, |e| e.set("targetname", name.clone()));
            }
            let ids = doc.create_primitive(&params, Owner::World)?;
            let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Solid(*id)).collect();
            let kv = vec![
                ("target".to_owned(), name.clone()),
                ("StartOpen".to_owned(), if open { "1" } else { "0" }.to_owned()),
                ("PortalVersion".to_owned(), "1".to_owned()),
            ];
            doc.tie_to_entity(&objs, "func_areaportal", &kv)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::box_sides;

    fn doorway() -> (Document, Id) {
        let mut doc = Document::new_empty();
        let wall = |doc: &mut Document, a: DVec3, b: DVec3| {
            doc.create_solid(box_sides(a, b, "BRICK/WALL"), Owner::World).expect("wall");
        };
        wall(&mut doc, DVec3::new(-256.0, -8.0, 0.0), DVec3::new(-32.0, 8.0, 128.0));
        wall(&mut doc, DVec3::new(32.0, -8.0, 0.0), DVec3::new(256.0, 8.0, 128.0));
        wall(&mut doc, DVec3::new(-32.0, -8.0, 112.0), DVec3::new(32.0, 8.0, 128.0));
        wall(&mut doc, DVec3::new(-256.0, -256.0, -16.0), DVec3::new(256.0, 256.0, 0.0));
        let d = doc
            .create_solid(
                box_sides(DVec3::new(-30.0, -2.0, 1.0), DVec3::new(30.0, 2.0, 110.0), "WOOD/DOOR"),
                Owner::World,
            )
            .expect("door");
        let kv = vec![("spawnpos".to_owned(), "1".to_owned())];
        let door = doc.tie_to_entity(&[ObjRef::Solid(d)], "func_door", &kv).expect("tie");
        (doc, door)
    }

    #[test]
    fn fits_the_doorway_and_links_the_door() {
        let (mut doc, door) = doorway();
        let bounds = doc.scene().entity_solid_bounds(door).expect("bounds");
        let fit = fit_opening(&doc, bounds).expect("fit");
        assert!(fit.min.x <= -32.0 && fit.min.x > -40.0, "{fit:?}");
        assert!(fit.max.x >= 32.0 && fit.max.x < 40.0, "{fit:?}");
        assert!(fit.max.z >= 112.0 && fit.min.z <= 0.0, "{fit:?}");
        assert!(fit.size().y <= 4.0 && fit.center().y.abs() < 1e-9);
        let portal = doc.seal_opening(door, fit).expect("seal");
        let p = doc.scene().entity(portal).expect("portal");
        assert_eq!(p.entity.classname, "func_areaportal");
        assert_eq!(p.entity.get("target"), Some("door_".to_owned() + &door.to_string()).as_deref());
        assert_eq!(p.entity.get("StartOpen"), Some("1"), "the door starts open");
        let d = doc.scene().entity(door).expect("door");
        assert_eq!(d.entity.get("targetname"), Some(format!("door_{door}").as_str()));
        assert_eq!(doc.undo_label(), Some("Seal opening"));
        assert!(doc.undo());
        assert!(doc.scene().entity(portal).is_none());
        assert!(doc.scene().entity(door).and_then(|n| n.entity.get("targetname")).is_none());
    }

    #[test]
    fn rejects_bad_input() {
        let (mut doc, door) = doorway();
        assert!(fit_opening(&doc, Aabb::EMPTY).is_none());
        assert!(doc.seal_opening(9999, Aabb::new(DVec3::ZERO, DVec3::ONE)).is_err());
        assert!(doc.seal_opening(door, Aabb::new(DVec3::ZERO, DVec3::ZERO)).is_err());
    }
}
