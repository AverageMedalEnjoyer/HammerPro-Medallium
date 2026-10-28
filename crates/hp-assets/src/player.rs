use glam::DVec3;
use hp_app::state::PlayerModelPose;
use hp_mdl::{AnimRef, AnimSet, BoneWeight, ModelVertex};
use hp_render::{MaterialId, ModelId, ModelInstance, ModelPart, Renderer};

use crate::AssetManager;
use crate::material::MatKind;
use crate::models::entity_matrix;
use crate::preview::skin_part;

pub(crate) const PLAYER_OBJECT: u32 = u32::MAX - 64;

const FALLBACK_SEQUENCES: &[&str] = &["idle_all_01", "idle_all_02", "idle", "ragdoll"];

struct Part {
    material: MaterialId,
    bind: Vec<ModelVertex>,
    weights: Vec<BoneWeight>,
    indices: Vec<u32>,
}

pub(crate) struct PlayerAnim {
    path: String,
    set: AnimSet,
    parts: Vec<Part>,
    posed: Option<(AnimRef, usize, u32)>,
    model: Option<ModelId>,
}

#[derive(Default)]
pub(crate) struct PlayerModel {
    pub pose: Option<PlayerModelPose>,
    anim: Option<PlayerAnim>,
    failed: Option<String>,
}

fn load(am: &mut AssetManager, path: &str) -> Option<PlayerAnim> {
    let (model, mdl) = {
        let fs = am.fs.as_ref()?;
        let opts =
            hp_mdl::LoadOptions { pose: hp_mdl::Pose::Bind, bone_weights: true, ..hp_mdl::LoadOptions::default() };
        let model = hp_mdl::load_from(path, |p| fs.read(p), &opts).ok()?;
        (model, fs.read(path)?)
    };
    let set = {
        let fs = am.fs.as_ref()?;
        AnimSet::load(&mdl, |p| fs.read(p))?
    };
    let mut parts = Vec::with_capacity(model.meshes.len());
    for mesh in &model.meshes {
        if mesh.indices.is_empty() || mesh.bone_weights.len() != mesh.vertices.len() {
            continue;
        }
        let candidates = model.material_candidates(mesh.skin_ref, 0);
        let name = {
            let fs = am.fs.as_ref()?;
            candidates
                .iter()
                .find(|c| fs.exists(&format!("materials/{c}.vmt")))
                .or_else(|| candidates.first())
                .cloned()
                .unwrap_or_default()
        };
        let material = am.asset_material(&name, MatKind::Model);
        parts.push(Part {
            material,
            bind: mesh.vertices.clone(),
            weights: mesh.bone_weights.clone(),
            indices: mesh.indices.clone(),
        });
    }
    (!parts.is_empty()).then(|| PlayerAnim { path: path.to_owned(), set, parts, posed: None, model: None })
}

fn sequence(set: &AnimSet, pose: &PlayerModelPose) -> Option<AnimRef> {
    let params = [("move_x", pose.move_x), ("move_y", pose.move_y)];
    set.sequence(&pose.sequence, &params).or_else(|| FALLBACK_SEQUENCES.iter().find_map(|s| set.sequence(s, &params)))
}

impl AssetManager {
    pub(crate) fn set_player_model_impl(&mut self, pose: Option<PlayerModelPose>) {
        if self.player.pose != pose {
            self.player.pose = pose;
            self.scene.instances_dirty = true;
        }
    }

    pub(crate) fn player_instance(&mut self, renderer: &mut Renderer) -> Option<ModelInstance> {
        let Some(pose) = self.player.pose.clone() else {
            if let Some(id) = self.player.anim.as_mut().and_then(|a| a.model.take()) {
                renderer.destroy_model(id);
            }
            if let Some(a) = self.player.anim.as_mut() {
                a.posed = None;
            }
            return None;
        };
        if self.player.anim.as_ref().is_none_or(|a| a.path != pose.model)
            && self.player.failed.as_deref() != Some(pose.model.as_str())
        {
            if let Some(id) = self.player.anim.take().and_then(|a| a.model) {
                renderer.destroy_model(id);
            }
            self.player.anim = load(self, &pose.model);
            if self.player.anim.is_none() {
                self.player.failed = Some(pose.model.clone());
            }
        }
        let transform = entity_matrix(pose.feet, DVec3::new(0.0, pose.yaw, 0.0), 1.0).as_mat4().to_cols_array_2d();
        renderer.set_object_flags(PLAYER_OBJECT, hp_render::ObjectFlags::NO_PROBE);
        let instance =
            |model| ModelInstance { model, transform, object: PLAYER_OBJECT, tint: [1.0; 4], fade: [0.0; 2] };
        let animated = self.player.anim.as_mut().filter(|a| a.path == pose.model).and_then(|a| {
            let anim = sequence(&a.set, &pose)?;
            let (frame, frac) = anim.clip.frame_at(pose.time);
            let key = (anim, frame, frac.to_bits());
            if a.posed != Some(key) || a.model.is_none() {
                let matrices = a.set.skin(&anim, frame, frac)?;
                let parts: Vec<ModelPart> = a
                    .parts
                    .iter()
                    .map(|p| ModelPart {
                        material: p.material,
                        vertices: skin_part(&matrices, &p.bind, &p.weights),
                        indices: p.indices.clone(),
                        tangents: Vec::new(),
                    })
                    .collect();
                if let Some(id) = a.model.take() {
                    renderer.destroy_model(id);
                }
                a.model = Some(renderer.create_model(&parts));
                a.posed = Some(key);
            }
            a.model
        });
        match animated {
            Some(model) => Some(instance(model)),
            None => match self.render_model(renderer, &pose.model, 0) {
                Some(model) => Some(instance(model)),
                None => {
                    self.scene.instances_dirty = true;
                    None
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_model_plays_included_animations() {
        let root = std::path::Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod");
        if !root.join("garrysmod").is_dir() {
            eprintln!("Garry's Mod not found: test skipped");
            return;
        }
        let fs = hp_fs::GameFs::for_gmod(root, &hp_fs::MountOptions::default()).expect("mount");
        let path = "models/player/kleiner.mdl";
        let mdl = fs.read(path).expect("model");
        let set = AnimSet::load(&mdl, |p| fs.read(p)).expect("animations");
        for s in ["idle_all_01", "walk_all", "run_all_01", "cidle_all", "cwalk_all", "jump_slam", "swimming_all"] {
            assert!(set.has(s), "{s}");
        }
        let walk = set.sequence("walk_all", &[("move_x", 1.0), ("move_y", 0.0)]).expect("walk");
        let back = set.sequence("walk_all", &[("move_x", -1.0), ("move_y", 0.0)]).expect("walk back");
        assert!(walk.clip.frames > 1 && walk.clip.looping, "{:?}", walk.clip);
        let a = set.skin(&walk, 0, 0.0).expect("frame 0");
        let b = set.skin(&walk, walk.clip.frames / 2, 0.0).expect("middle frame");
        let c = set.skin(&back, 0, 0.0).expect("back frame 0");
        let differ = |x: &[hp_mdl::SkinMatrix], y: &[hp_mdl::SkinMatrix]| {
            x.iter().flatten().flatten().zip(y.iter().flatten().flatten()).any(|(p, q)| (p - q).abs() > 0.05)
        };
        assert_eq!(a.len(), set.bones().len());
        assert!(differ(&a, &b), "the walk moves");
        assert!(differ(&a, &c), "forward and backward walks differ");
    }
}
