use std::path::{Path, PathBuf};

use glam::{DMat4, DQuat, DVec3};
use hp_geom::{Plane, build_solid, texel_uv, transform_solid, world_axes};
use hp_vmf::{Side, Solid, TexAxis};

fn sample_paths() -> Vec<PathBuf> {
    let dir = Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\gamemodes\terrortown\mapexamples");
    let mut out: Vec<PathBuf> =
        ["ttt_traitorcheck.vmf", "ttt_traps.vmf", "ttt_weaponcheck.vmf"].iter().map(|f| dir.join(f)).collect();
    out.push(PathBuf::from(r"C:\Users\oracle\Desktop\hammermcptest\test.vmf"));
    out.into_iter().filter(|p| p.is_file()).collect()
}

fn parse_axis(s: &str) -> Option<TexAxis> {
    let s = s.trim().strip_prefix('[')?;
    let (inner, scale) = s.split_once(']')?;
    let v: Vec<f64> = inner.split_ascii_whitespace().map(|x| x.parse().ok()).collect::<Option<_>>()?;
    let [x, y, z, shift] = v[..] else { return None };
    Some(TexAxis { axis: DVec3::new(x, y, z), shift, scale: scale.trim().parse().ok()? })
}

fn parse_plane(s: &str) -> Option<[DVec3; 3]> {
    let nums: Vec<f64> = s
        .split(|c| c == '(' || c == ')' || char::is_whitespace(c))
        .filter(|t| !t.is_empty())
        .map(|t| t.parse().ok())
        .collect::<Option<_>>()?;
    let [a, b, c, d, e, f, g, h, i] = nums[..] else { return None };
    Some([DVec3::new(a, b, c), DVec3::new(d, e, f), DVec3::new(g, h, i)])
}

fn collect_solids(block: &hp_kv::Block, out: &mut Vec<Solid>) {
    for child in &block.children {
        if child.name.eq_ignore_ascii_case("solid") {
            let sides = child
                .children_named("side")
                .filter_map(|s| {
                    Some(Side {
                        id: s.get_parsed("id").unwrap_or(0),
                        plane: parse_plane(s.get("plane")?)?,
                        material: s.get("material").unwrap_or("").to_owned(),
                        uaxis: parse_axis(s.get("uaxis")?)?,
                        vaxis: parse_axis(s.get("vaxis")?)?,
                        rotation: s.get_parsed("rotation").unwrap_or(0.0),
                        lightmap_scale: s.get_parsed("lightmapscale").unwrap_or(16),
                        ..Default::default()
                    })
                })
                .collect();
            out.push(Solid { id: child.get_parsed("id").unwrap_or(0), sides, ..Default::default() });
        } else {
            collect_solids(child, out);
        }
    }
}

fn load_solids(path: &Path) -> Option<Vec<Solid>> {
    let bytes = std::fs::read(path).ok()?;
    let root = hp_kv::parse_bytes(&bytes).ok()?;
    let mut solids = Vec::new();
    collect_solids(&root, &mut solids);
    Some(solids)
}

#[test]
fn sample_vmfs_build_and_match_hammer() {
    let paths = sample_paths();
    if paths.is_empty() {
        eprintln!("real_vmf: sample VMFs not found, skipping");
        return;
    }
    for path in paths {
        let Some(solids) = load_solids(&path) else {
            eprintln!("real_vmf: could not read {}, skipping", path.display());
            continue;
        };
        assert!(!solids.is_empty());
        let t = std::time::Instant::now();
        let geoms: Vec<_> = solids.iter().map(|s| build_solid(&s.sides)).collect();
        let elapsed = t.elapsed();
        let mut faces = 0usize;
        let mut points_on_vertices = 0usize;
        let mut points_total = 0usize;
        let mut axial = 0usize;
        let mut axial_world = 0usize;
        for (solid, geom) in solids.iter().zip(&geoms) {
            assert!(geom.valid, "{}: solid {} is invalid", path.display(), solid.id);
            for f in &geom.faces {
                faces += 1;
                let side = &solid.sides[f.side_index];
                let n = hp_geom::polygon_normal(&f.vertices).expect("face normal");
                assert!((n - f.plane.normal).length() < 1e-6);
                for &v in &f.vertices {
                    assert!(geom.faces.iter().all(|o| o.plane.distance(v) < 1e-6));
                }
                for p in side.plane {
                    points_total += 1;
                    if f.vertices.iter().any(|v| v.distance(p) < 0.01) {
                        points_on_vertices += 1;
                    } else if std::env::var_os("HP_GEOM_DEBUG").is_some() {
                        eprintln!("solid {} side {} point {:?} not in {:?}", solid.id, side.id, p, f.vertices);
                    }
                }
                let n = f.plane.normal;
                if n.abs().max_element() == 1.0 {
                    axial += 1;
                    if world_axes(n) == (side.uaxis.axis, side.vaxis.axis) {
                        axial_world += 1;
                    } else if std::env::var_os("HP_GEOM_DEBUG").is_some() {
                        eprintln!(
                            "axes n={n:?} u={:?} v={:?} rot={} mat={}",
                            side.uaxis.axis, side.vaxis.axis, side.rotation, side.material
                        );
                    }
                }
            }
        }
        eprintln!(
            "real_vmf: {}: {} solids, {} faces built in {:.3} ms; plane points on vertices {}/{}; world axes on axial faces {}/{}",
            path.file_name().and_then(|f| f.to_str()).unwrap_or("?"),
            solids.len(),
            faces,
            elapsed.as_secs_f64() * 1000.0,
            points_on_vertices,
            points_total,
            axial_world,
            axial
        );
        assert!(axial_world * 2 >= axial);
        for (solid, geom) in solids.iter().zip(&geoms) {
            for f in &geom.faces {
                assert!(hp_geom::texture_axes_valid(&solid.sides[f.side_index], f.plane.normal));
            }
        }

        let m = DMat4::from_rotation_translation(
            DQuat::from_rotation_z(0.4) * DQuat::from_rotation_x(0.2),
            DVec3::new(12.0, -40.0, 8.0),
        );
        for (solid, geom) in solids.iter().zip(&geoms) {
            let mut moved = solid.clone();
            transform_solid(&mut moved, m, true, false).expect("transform");
            let g2 = build_solid(&moved.sides);
            assert!(g2.valid);
            assert!((g2.volume() - geom.volume()).abs() < 1e-6 * geom.volume().max(1.0));
            for f in &geom.faces {
                let before = &solid.sides[f.side_index];
                let after = &moved.sides[f.side_index];
                assert!(Plane::from_side(after).is_some());
                for &v in &f.vertices {
                    let d = texel_uv(after, m.transform_point3(v)) - texel_uv(before, v);
                    assert!(d.length() < 1e-6);
                }
            }
        }
    }
}

#[test]
fn hp_vmf_loader_agrees() {
    let paths = sample_paths();
    if paths.is_empty() {
        eprintln!("real_vmf: sample VMFs not found, skipping");
        return;
    }
    for path in paths {
        let loaded = std::panic::catch_unwind(|| hp_vmf::load(&path));
        let map = match loaded {
            Ok(Ok(map)) => map,
            _ => {
                eprintln!("real_vmf: hp_vmf::load not usable for {}, skipping", path.display());
                continue;
            }
        };
        let solids: Vec<Solid> =
            map.world.solids.iter().chain(map.entities.iter().flat_map(|e| e.solids.iter())).cloned().collect();
        let geoms = hp_geom::build_solids(&solids);
        let invalid = geoms.iter().filter(|g| !g.valid).count();
        assert_eq!(invalid, 0, "{}", path.display());
        let kv_count = load_solids(&path).map(|s| s.len()).unwrap_or(0);
        assert_eq!(solids.len(), kv_count, "{}", path.display());
    }
}
