use std::collections::HashSet;

use hp_geom::Aabb;

use crate::document::Document;
use crate::glob::normalize_material;
use crate::scene::Owner;

#[derive(Clone, Debug, PartialEq)]
pub struct MapStats {
    pub solids: usize,
    pub world_solids: usize,
    pub faces: usize,
    pub entities: usize,
    pub point_entities: usize,
    pub brush_entities: usize,
    pub displacements: usize,
    pub unique_materials: usize,
    pub groups: usize,
    pub visgroups: usize,
    pub invalid_solids: usize,
    pub bounds: Option<Aabb>,
}

impl Document {
    pub fn stats(&self) -> MapStats {
        let scene = &self.scene;
        let mut faces = 0;
        let mut displacements = 0;
        let mut world_solids = 0;
        let mut invalid = 0;
        let mut bounds = Aabb::EMPTY;
        let mut invalid_bounds = Aabb::EMPTY;
        let mut materials: HashSet<String> = HashSet::new();
        for n in scene.solids.values() {
            faces += n.solid.sides.len();
            if n.owner == Owner::World {
                world_solids += 1;
            }
            if !n.geom.valid {
                invalid += 1;
            }
            if !n.bounds.is_empty() && n.bounds.min.is_finite() && n.bounds.max.is_finite() {
                if n.geom.valid {
                    bounds = bounds.union(&n.bounds);
                } else {
                    invalid_bounds = invalid_bounds.union(&n.bounds);
                }
            }
            for s in &n.solid.sides {
                if s.disp.is_some() {
                    displacements += 1;
                }
                if !materials.contains(s.material.as_str()) {
                    materials.insert(s.material.clone());
                }
            }
        }
        if bounds.is_empty() {
            bounds = invalid_bounds;
        }
        let unique: HashSet<String> = materials.iter().map(|m| normalize_material(m).to_ascii_lowercase()).collect();
        let brush_entities = scene.entities.keys().filter(|id| scene.is_brush_entity(**id)).count();
        let mut visgroups = 0;
        let mut stack: Vec<&hp_vmf::VisGroup> = scene.visgroups.iter().collect();
        while let Some(v) = stack.pop() {
            visgroups += 1;
            stack.extend(v.children.iter());
        }
        MapStats {
            solids: scene.solids.len(),
            world_solids,
            faces,
            entities: scene.entities.len(),
            point_entities: scene.entities.len() - brush_entities,
            brush_entities,
            displacements,
            unique_materials: unique.len(),
            groups: scene.groups.len(),
            visgroups,
            invalid_solids: invalid,
            bounds: (!bounds.is_empty()).then_some(bounds),
        }
    }
}
