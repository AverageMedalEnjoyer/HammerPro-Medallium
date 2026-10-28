use ahash::AHashSet;
use glam::DVec3;
use hp_app::runtime::helpers::geometry::light_direction;
use hp_editor::{Document, Id};
use hp_render::SpriteInstance;

use crate::entities::entity_object;
use crate::material::MatKind;
use crate::producer::{DerivedBuild, DerivedProducer, ProducerChanges, ProducerCx};
use crate::util::sprite_material;

pub(crate) const SUN_DISTANCE: f64 = 4096.0;
const SIZE_SCALE: f64 = 12.0;
const DEFAULT_MATERIAL: &str = "sprites/light_glow02_add_noz";

fn is_sun(class: &str) -> bool {
    class.trim().eq_ignore_ascii_case("env_sun")
}

pub(crate) fn sun_direction(doc: &Document, e: &hp_vmf::Entity) -> DVec3 {
    let origin = e.origin().filter(|o| o.is_finite()).unwrap_or(DVec3::ZERO);
    if let Some(target) = e.get("target").map(str::trim).filter(|t| !t.is_empty()) {
        let pos = doc.scene().entities().values().find_map(|n| {
            n.entity.get("targetname").filter(|t| t.trim().eq_ignore_ascii_case(target)).and(n.entity.origin())
        });
        if let Some(d) = pos.and_then(|p| (origin - p).try_normalize()) {
            return d;
        }
    }
    -light_direction(e)
}

#[derive(Default)]
pub(crate) struct SunProducer {
    suns: AHashSet<Id>,
    targets: AHashSet<String>,
    dirty: bool,
    shown: Option<bool>,
}

impl DerivedProducer for SunProducer {
    fn name(&self) -> &'static str {
        "suns"
    }

    fn changed(&mut self, changes: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool {
        let shown = cx.opts.show_helpers;
        if self.shown != Some(shown) || changes.full || changes.entities {
            self.shown = Some(shown);
            self.dirty = true;
            return true;
        }
        let scene = cx.doc.scene();
        let touches = |id: &Id| {
            self.suns.contains(id)
                || scene.entity(*id).is_some_and(|n| {
                    is_sun(&n.entity.classname)
                        || n.entity
                            .get("targetname")
                            .is_some_and(|t| self.targets.contains(&t.trim().to_ascii_lowercase()))
                })
        };
        if changes.doc.entities_changed.iter().any(touches)
            || changes.doc.entities_removed.iter().any(|id| self.suns.contains(id))
        {
            self.dirty = true;
        }
        self.dirty
    }

    fn build(&mut self, cx: &mut ProducerCx<'_>) -> DerivedBuild {
        self.dirty = false;
        self.suns.clear();
        self.targets.clear();
        let mut sprites = Vec::new();
        if self.shown == Some(true) {
            let found: Vec<(Id, String, DVec3, f32, [f32; 4])> = cx
                .doc
                .scene()
                .entities()
                .iter()
                .filter(|(_, n)| is_sun(&n.entity.classname))
                .filter_map(|(id, n)| {
                    let e = &n.entity;
                    let origin = e.origin().filter(|o| o.is_finite())?;
                    if let Some(t) = e.get("target").map(str::trim).filter(|t| !t.is_empty()) {
                        self.targets.insert(t.to_ascii_lowercase());
                    }
                    let material =
                        e.get("material").and_then(sprite_material).unwrap_or_else(|| DEFAULT_MATERIAL.to_owned());
                    let size = e
                        .get("size")
                        .and_then(|s| s.trim().parse::<f64>().ok())
                        .filter(|s| s.is_finite() && *s > 0.0)
                        .unwrap_or(16.0)
                        .min(256.0);
                    let rgb: Vec<f32> = e
                        .get("rendercolor")
                        .unwrap_or("100 80 80")
                        .split_ascii_whitespace()
                        .take(3)
                        .filter_map(|c| c.parse::<f32>().ok())
                        .filter(|c| c.is_finite())
                        .map(|c| (c / 255.0).clamp(0.0, 1.0))
                        .collect();
                    let color = match rgb.as_slice() {
                        [r, g, b] => [*r, *g, *b, 1.0],
                        _ => [1.0; 4],
                    };
                    let pos = origin + sun_direction(cx.doc, e) * SUN_DISTANCE;
                    Some((*id, material, pos, (size * SIZE_SCALE) as f32, color))
                })
                .collect();
            for (id, material, pos, size, color) in found {
                self.suns.insert(id);
                let material = cx.assets.asset_material(&material, MatKind::SpriteAdditive);
                sprites.push(SpriteInstance {
                    pos: pos.as_vec3().to_array(),
                    size,
                    material,
                    color,
                    object: entity_object(id),
                });
            }
        }
        DerivedBuild { sprites: Some(sprites), ..DerivedBuild::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_points_away_from_its_target_or_up_its_pitch() {
        let mut state = hp_app::testing::test_state();
        let sun = state
            .doc
            .create_point_entity("env_sun", DVec3::new(0.0, 0.0, 64.0), &[("pitch".to_owned(), "-90".to_owned())])
            .expect("sun");
        let e = state.doc.scene().entity(sun).expect("sun").entity.clone();
        assert!((sun_direction(&state.doc, &e) - DVec3::Z).length() < 1e-9, "pitch -90 lights down: the sun is up");
        state.doc.edit_entity(sun, |e| e.set("target", "main_light")).expect("target");
        let e = state.doc.scene().entity(sun).expect("sun").entity.clone();
        assert!((sun_direction(&state.doc, &e) - DVec3::NEG_Z).length() < 1e-9);
    }
}
