use ahash::AHashMap;
use glam::{DMat3, DVec3};
use hp_editor::{Document, Id, Owner};
use hp_render::parallax::ParallaxBox;

fn numbers(v: &str) -> Vec<f32> {
    v.trim()
        .trim_start_matches(['[', '{'])
        .trim_end_matches([']', '}'])
        .split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect()
}

pub fn box_of_material(mat: &hp_vmt::Material) -> Option<ParallaxBox> {
    let on = mat.get("$envmapparallax").is_some_and(|v| v.trim().parse::<f32>().is_ok_and(|x| x != 0.0));
    if !on {
        return None;
    }
    let row = |k: &str| -> Option<[f32; 4]> {
        let n = numbers(mat.get(k)?);
        (n.len() == 4 && n.iter().all(|x| x.is_finite())).then(|| [n[0], n[1], n[2], n[3]])
    };
    let origin = numbers(mat.get("$envmaporigin")?);
    if origin.len() != 3 {
        return None;
    }
    Some(ParallaxBox {
        rows: [row("$envmapparallaxobb1")?, row("$envmapparallaxobb2")?, row("$envmapparallaxobb3")?],
        origin: [origin[0], origin[1], origin[2]],
    })
}

fn box_of_points(first_face: &[DVec3], points: &[DVec3], origin: DVec3) -> Option<ParallaxBox> {
    let (corner, a, b) = (*first_face.first()?, *first_face.get(1)?, *first_face.get(3).or(first_face.get(2))?);
    let y = a - corner;
    let z = b - corner;
    let n = y.cross(z).normalize_or_zero();
    let depth = points.iter().map(|p| (*p - corner).dot(n)).fold(0.0f64, |m, d| if d.abs() > m.abs() { d } else { m });
    let x = n * depth;
    let m = DMat3::from_cols(x, y, z);
    if m.determinant().abs() < 1e-9 {
        return None;
    }
    let inv = m.inverse();
    let t = -(inv * corner);
    let row = |i: usize| {
        let r = inv.row(i);
        [r.x as f32, r.y as f32, r.z as f32, t[i] as f32]
    };
    Some(ParallaxBox { rows: [row(0), row(1), row(2)], origin: origin.as_vec3().to_array() })
}

pub fn boxes_of_document(doc: &Document) -> Vec<(Id, ParallaxBox)> {
    let scene = doc.scene();
    let mut out = Vec::new();
    for (id, node) in scene.entities() {
        let e = &node.entity;
        if !e.classname.eq_ignore_ascii_case("env_cubemap") || e.hidden {
            continue;
        }
        let Some(name) = e.get("parallaxobb").map(str::trim).filter(|n| !n.is_empty()) else { continue };
        let Some((obb_id, _)) = scene.entities().iter().find(|(_, n)| {
            n.entity.classname.eq_ignore_ascii_case("parallax_obb")
                && n.entity.get("targetname").is_some_and(|t| t.trim().eq_ignore_ascii_case(name))
        }) else {
            continue;
        };
        let origin: Vec<f64> =
            e.get("origin").unwrap_or("").split_whitespace().filter_map(|x| x.parse().ok()).collect();
        let origin = if origin.len() == 3 {
            DVec3::new(origin[0].trunc(), origin[1].trunc(), origin[2].trunc())
        } else {
            DVec3::ZERO
        };
        let Some(solid) = scene.solids().values().find(|s| s.owner == Owner::Entity(*obb_id)) else { continue };
        let points: Vec<DVec3> = solid.geom.faces.iter().flat_map(|f| f.vertices.iter().copied()).collect();
        if let Some(first) = solid.geom.faces.first()
            && let Some(b) = box_of_points(&first.vertices, &points, origin)
        {
            out.push((*id, b));
        }
    }
    out
}

pub fn boxes_of_pakfile(pak: &hp_bsp::pak::PakArchive<'_>) -> AHashMap<[i32; 3], ParallaxBox> {
    let mut out = AHashMap::new();
    for e in pak.entries() {
        let Some(name) = e.key.strip_prefix("materials/").and_then(|n| n.strip_suffix(".vmt")) else { continue };
        let Some(origin) = hp_editor::limits::world::patched_cubemap_origin(name) else { continue };
        if out.contains_key(&origin) {
            continue;
        }
        let Ok(data) = pak.read(e) else { continue };
        let text = String::from_utf8_lossy(&data);
        if !text.to_ascii_lowercase().contains("$envmapparallax") {
            continue;
        }
        let Ok(patch) = hp_vmt::parse(&text) else { continue };
        let Ok(merged) = hp_vmt::resolve_patch(&patch, &|_| Some("\"LightmappedGeneric\" { }".to_owned())) else {
            continue;
        };
        if let Some(b) = box_of_material(&merged) {
            out.insert(origin, b);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patched_material_keys() {
        let merged = hp_vmt::parse("\"LightmappedGeneric\" { \"$envmap\" \"maps/x/c0_0_64\" \"$envMapParallax\" \"1\" \"$envMapParallaxOBB1\" \"[0.01 0 0 -0.5]\" \"$envMapParallaxOBB2\" \"[0 0.02 0 -0.5]\" \"$envMapParallaxOBB3\" \"[0 0 0.03 0]\" \"$envMapOrigin\" \"[0 0 64]\" }").expect("vmt");
        let b = box_of_material(&merged).expect("a box");
        assert_eq!(b.rows[0], [0.01, 0.0, 0.0, -0.5]);
        assert_eq!(b.rows[2], [0.0, 0.0, 0.03, 0.0]);
        assert_eq!(b.origin, [0.0, 0.0, 64.0]);
        assert!(
            box_of_material(&hp_vmt::parse("\"LightmappedGeneric\" { \"$envmap\" \"env_cubemap\" }").unwrap())
                .is_none()
        );
        let off = hp_vmt::parse("\"LightmappedGeneric\" { \"$envMapParallax\" \"0\" }").unwrap();
        assert!(box_of_material(&off).is_none());
    }

    #[test]
    fn boxes_are_read_from_the_patched_materials_of_a_pakfile() {
        let patch = |extra: &str| {
            format!(
                "\"patch\" {{ \"include\" \"materials/tile/floor.vmt\" \"insert\" {{ \"$envmap\" \"maps/gm_p/c0_0_64\" {extra} }} }}"
            )
            .into_bytes()
        };
        let keys = "\"$envMapParallax\" \"1\" \"$envMapParallaxOBB1\" \"[0.01 0 0 -0.5]\" \"$envMapParallaxOBB2\" \
                    \"[0 0.02 0 -0.5]\" \"$envMapParallaxOBB3\" \"[0 0 0.03 0]\" \"$envMapOrigin\" \"[0 0 64]\"";
        let mut w = hp_bsp::PakWriter::new();
        w.add("materials/maps/gm_p/tile/floor_0_0_64.vmt", patch(keys)).expect("add");
        w.add("materials/maps/gm_p/tile/wall_128_0_64.vmt", patch("")).expect("add");
        w.add("materials/maps/gm_p/tile/blend_wvt_patch.vmt", patch(keys)).expect("add");
        let zip = w.finish(hp_bsp::PakOptions::default()).expect("zip");
        let pak = hp_bsp::PakArchive::parse(zip).expect("pak");
        let boxes = boxes_of_pakfile(&pak);
        assert_eq!(boxes.len(), 1, "{boxes:?}");
        let b = boxes[&[0, 0, 64]];
        assert_eq!(b.rows[1], [0.0, 0.02, 0.0, -0.5]);
        assert_eq!(b.origin, [0.0, 0.0, 64.0]);
    }

    #[test]
    fn a_box_brush_maps_onto_the_unit_cube() {
        let (lo, hi) = (DVec3::new(-64.0, 0.0, 0.0), DVec3::new(192.0, 128.0, 96.0));
        let top = [
            DVec3::new(lo.x, lo.y, hi.z),
            DVec3::new(hi.x, lo.y, hi.z),
            DVec3::new(hi.x, hi.y, hi.z),
            DVec3::new(lo.x, hi.y, hi.z),
        ];
        let mut points = Vec::new();
        for k in 0..8 {
            points.push(DVec3::new(
                if k & 1 == 0 { lo.x } else { hi.x },
                if k & 2 == 0 { lo.y } else { hi.y },
                if k & 4 == 0 { lo.z } else { hi.z },
            ));
        }
        let b = box_of_points(&top, &points, DVec3::new(64.0, 64.0, 48.0)).expect("a box");
        for p in &points {
            let q: Vec<f32> = (0..3)
                .map(|i| {
                    b.rows[i][0] * p.x as f32 + b.rows[i][1] * p.y as f32 + b.rows[i][2] * p.z as f32 + b.rows[i][3]
                })
                .collect();
            assert!(q.iter().all(|c| c.abs() < 1e-4 || (c - 1.0).abs() < 1e-4), "{p} gives {q:?}");
        }
    }
}
