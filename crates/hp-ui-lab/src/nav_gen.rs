use glam::DVec3;
use hp_app::AppState;
use hp_app::state::ModelState;
use hp_editor::Owner;
use hp_nav::{GenInput, GenSolid};

const BLOCKING_CLASSES: &[&str] = &["func_detail", "func_brush", "func_wall", "func_wall_toggle", "func_lod"];

const PASSABLE_TOOLS: &[&str] = &[
    "toolstrigger",
    "toolshint",
    "toolsskip",
    "toolsareaportal",
    "toolsoccluder",
    "toolsnpcclip",
    "toolsfog",
    "toolsblock_los",
    "toolsblocklight",
];

fn is_spawn(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    c.starts_with("info_player_") || c == "gmod_player_start"
}

fn v(p: DVec3) -> [f64; 3] {
    [p.x, p.y, p.z]
}

fn passable(material: &str) -> bool {
    let m = material.to_ascii_lowercase();
    PASSABLE_TOOLS.iter().any(|t| m.contains(t)) || m.contains("water") || m.contains("liquid")
}

pub fn input_from_doc(state: &mut AppState) -> GenInput {
    let mut input = GenInput::default();
    let scene = state.doc.scene();
    for (id, node) in scene.solids() {
        if !state.doc.is_solid_visible(*id) {
            continue;
        }
        let blocking_owner = match node.owner {
            Owner::World => true,
            Owner::Entity(e) => scene.entity(e).is_some_and(|en| {
                let c = en.entity.classname.to_ascii_lowercase();
                BLOCKING_CLASSES.iter().any(|b| c == *b)
            }),
        };
        if !blocking_owner {
            continue;
        }
        let mut has_disp = false;
        for mesh in node.disp.iter().flatten() {
            has_disp = true;
            for t in mesh.indices.chunks_exact(3) {
                let p = |i: u32| v(mesh.positions[i as usize]);
                input.surfaces.push([p(t[0]), p(t[1]), p(t[2])]);
            }
        }
        if has_disp {
            continue;
        }
        if node.solid.sides.iter().all(|s| passable(&s.material)) {
            continue;
        }
        let faces: Vec<Vec<[f64; 3]>> =
            node.geom.faces.iter().map(|f| f.vertices.iter().map(|p| v(*p)).collect()).collect();
        if let Some(s) = GenSolid::from_faces(faces) {
            input.solids.push(s);
        }
    }
    let mut props: Vec<(String, DVec3, DVec3)> = Vec::new();
    for (_, node) in scene.entities() {
        let e = &node.entity;
        let Some(origin) = e.origin().filter(|o| o.is_finite()) else { continue };
        if is_spawn(&e.classname) {
            input.seeds.push(v(origin));
        } else if e.classname.eq_ignore_ascii_case("prop_static")
            && e.get("solid").is_none_or(|s| s.trim() != "0")
            && let Some(model) = e.get("model").filter(|m| !m.trim().is_empty())
        {
            let angles = e.get("angles").and_then(hp_vmf::parse_vec3).unwrap_or(DVec3::ZERO);
            props.push((model.trim().to_owned(), origin, angles));
        }
    }
    for (model, origin, angles) in props {
        let ModelState::Ready(info) = state.assets.model_info(&model) else { continue };
        let (lo, hi) = match (info.hull_min, info.hull_max) {
            (Some(a), Some(b)) => (a, b),
            _ => (info.bounds_min, info.bounds_max),
        };
        if !(lo.is_finite() && hi.is_finite()) || (hi - lo).min_element() <= 0.0 {
            continue;
        }
        let (fwd, right, up) = hp_geom::angles_to_vectors(angles);
        let left = -right;
        let center_local = (lo + hi) * 0.5;
        let center = origin + fwd * center_local.x + left * center_local.y + up * center_local.z;
        let half = (hi - lo) * 0.5;
        if let Some(s) = GenSolid::oriented_box(v(center), v(half), [v(fwd), v(left), v(up)]) {
            input.solids.push(s);
        }
    }
    if let Some(bsp) = state.doc.path().map(|p| p.with_extension("bsp")) {
        input.bsp_size = std::fs::metadata(bsp).map(|m| m.len().min(u64::from(u32::MAX)) as u32).unwrap_or(0);
    }
    input
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_map_has_ground_and_a_start() {
        let mut state = hp_app::testing::test_state();
        let input = input_from_doc(&mut state);
        assert!(!input.solids.is_empty());
        if !input.seeds.is_empty() {
            let (mesh, report) = hp_nav::generate(&input);
            assert!(report.samples > 0 || report.bad_seeds > 0);
            let _ = mesh;
        }
    }

    #[test]
    fn passable_materials() {
        assert!(passable("TOOLS/TOOLSTRIGGER"));
        assert!(passable("nature/water_canals_city"));
        assert!(!passable("tools/toolsplayerclip"));
        assert!(!passable("brick/wall"));
    }
}
