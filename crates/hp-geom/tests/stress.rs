use glam::{DMat4, DQuat, DVec3};
use hp_geom::{
    Aabb, GeomError, Plane, Primitive, PrimitiveParams, build_solid, carve, clip_solid, convex_hull, make_primitive,
    merge_convex, rebuild_from_points, texel_uv, transform_sides,
};
use hp_vmf::Side;

struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
    fn vec(&mut self, half: f64) -> DVec3 {
        DVec3::new(self.range(-half, half), self.range(-half, half), self.range(-half, half))
    }
    fn quat(&mut self) -> DQuat {
        DQuat::from_axis_angle(self.vec(1.0).normalize_or(DVec3::Z), self.range(0.0, std::f64::consts::TAU))
    }
}

fn ids() -> impl FnMut() -> u32 {
    let mut n = 10_000;
    move || {
        n += 1;
        n
    }
}

fn block(min: DVec3, max: DVec3, next: &mut dyn FnMut() -> u32) -> Vec<Side> {
    let p = PrimitiveParams { kind: Primitive::Block, bounds: Aabb::new(min, max), ..Default::default() };
    make_primitive(&p, next).expect("block").remove(0)
}

fn volume(sides: &[Side]) -> f64 {
    let g = build_solid(sides);
    assert!(g.valid);
    g.volume()
}

fn intersection_volume(a: &[Side], b: &[Side]) -> f64 {
    let mut all = a.to_vec();
    all.extend_from_slice(b);
    let g = build_solid(&all);
    if g.valid { g.volume() } else { 0.0 }
}

#[test]
fn random_clips_conserve_volume() {
    let mut rng = Rng(0x5eed);
    let mut next = ids();
    for _ in 0..300 {
        let mut sides = block(DVec3::splat(-64.0), DVec3::splat(64.0), &mut next);
        let m = DMat4::from_rotation_translation(rng.quat(), rng.vec(4000.0));
        transform_sides(&mut sides, m, true, false, None).expect("transform");
        let v = volume(&sides);
        let c = build_solid(&sides).aabb.center();
        let plane = Plane::from_normal_point(rng.vec(1.0), c + rng.vec(60.0)).expect("plane");
        let (front, back) = clip_solid(&sides, &plane, &mut next).expect("clip");
        let vf = front.as_deref().map_or(0.0, volume);
        let vb = back.as_deref().map_or(0.0, volume);
        assert!((vf + vb - v).abs() < 1e-6 * v, "{vf} + {vb} != {v}");
    }
}

#[test]
fn random_carves_conserve_volume() {
    let mut rng = Rng(0xca7e);
    let mut next = ids();
    for round in 0..200 {
        let target = block(rng.vec(100.0) - DVec3::splat(80.0), rng.vec(100.0) + DVec3::splat(80.0), &mut next);
        let mut carver = block(DVec3::splat(-40.0), DVec3::splat(40.0), &mut next);
        let m = if round % 2 == 0 {
            DMat4::from_rotation_translation(rng.quat(), rng.vec(120.0))
        } else {
            DMat4::from_translation(rng.vec(120.0).round())
        };
        transform_sides(&mut carver, m, false, false, None).expect("transform");
        let inter = intersection_volume(&target, &carver);
        let pieces = carve(&target, &carver, &mut next).expect("carve");
        let left: f64 = pieces.iter().map(|p| volume(p)).sum();
        let v = volume(&target);
        assert!((left + inter - v).abs() < 1e-6 * v, "round {round}: {left} + {inter} != {v}");
        for (i, a) in pieces.iter().enumerate() {
            assert!(intersection_volume(a, &carver) < 1e-6 * v);
            for b in pieces.iter().skip(i + 1) {
                assert!(intersection_volume(a, b) < 1e-6 * v);
            }
        }
    }
}

#[test]
fn hulls_of_awkward_point_sets() {
    let mut rng = Rng(0xbad);
    let lattice: Vec<DVec3> = (0..5)
        .flat_map(|x| (0..5).flat_map(move |y| (0..5).map(move |z| DVec3::new(x as f64, y as f64, z as f64) * 16.0)))
        .collect();
    let h = convex_hull(&lattice).expect("lattice");
    assert_eq!(h.faces.len(), 6);
    assert_eq!(h.points.len(), 8);
    for _ in 0..50 {
        let mut pts = Vec::new();
        for _ in 0..200 {
            let mut p = rng.vec(512.0).round();
            let k = (rng.next_u64() % 3) as usize;
            p[k] = if rng.unit() < 0.5 { -512.0 } else { 512.0 };
            pts.push(p + rng.vec(1e-5));
        }
        let h = convex_hull(&pts).expect("hull");
        for f in &h.faces {
            for &p in &pts {
                assert!(f.plane.distance(p) < hp_geom::PLANE_EPS);
            }
        }
    }
    for scale in [0.05, 1.0, 1e5] {
        let pts: Vec<DVec3> = (0..100).map(|_| rng.vec(scale)).collect();
        let h = convex_hull(&pts).expect("hull");
        assert!(h.volume() > 0.0);
    }
    assert_eq!(convex_hull(&[]).err(), Some(GeomError::TooFewPoints(0)));
}

#[test]
fn random_vertex_edits_rebuild_valid_solids() {
    let mut rng = Rng(0x7e57);
    let mut next = ids();
    for _ in 0..200 {
        let sides = block(DVec3::splat(-64.0), DVec3::splat(64.0), &mut next);
        let g = build_solid(&sides);
        let (mut verts, _) = hp_geom::unique_vertices(&g);
        let i = (rng.next_u64() % verts.len() as u64) as usize;
        verts[i] = hp_geom::snap_vec3(verts[i] + rng.vec(48.0), 8.0);
        match rebuild_from_points(&verts, &sides, &mut next) {
            Ok(new_sides) => {
                let ng = build_solid(&new_sides);
                assert!(ng.valid);
                let hull = convex_hull(&verts).expect("hull");
                for p in &hull.points {
                    assert!(ng.vertices().any(|v| v.distance(*p) < 0.01), "lost vertex {p:?}");
                }
                for s in &new_sides {
                    if let Some(orig) = sides.iter().find(|o| o.id == s.id) {
                        assert_eq!(texel_uv(s, DVec3::new(3.0, 5.0, 7.0)), texel_uv(orig, DVec3::new(3.0, 5.0, 7.0)));
                    }
                }
            }
            Err(e) => panic!("rebuild failed: {e}"),
        }
    }
}

#[test]
fn merge_random_boxes() {
    let mut rng = Rng(0x3e6);
    let mut next = ids();
    for _ in 0..100 {
        let a =
            block(rng.vec(200.0).round() - DVec3::splat(40.0), rng.vec(200.0).round() + DVec3::splat(240.0), &mut next);
        let b =
            block(rng.vec(200.0).round() - DVec3::splat(40.0), rng.vec(200.0).round() + DVec3::splat(240.0), &mut next);
        let m = merge_convex(&[a.clone(), b.clone()], &mut next).expect("merge");
        let vm = volume(&m);
        assert!(vm + 1e-6 >= volume(&a).max(volume(&b)));
        assert!((intersection_volume(&m, &a) - volume(&a)).abs() < 1e-6 * vm);
        assert!((intersection_volume(&m, &b) - volume(&b)).abs() < 1e-6 * vm);
    }
}

#[test]
#[ignore]
fn bench_operations() {
    let mut rng = Rng(42);
    let mut next = ids();
    let pts: Vec<DVec3> = (0..10_000).map(|_| rng.vec(4096.0)).collect();
    let t = std::time::Instant::now();
    let h = convex_hull(&pts).expect("hull");
    let hull_ms = t.elapsed().as_secs_f64() * 1000.0;

    let target = block(DVec3::splat(-128.0), DVec3::splat(128.0), &mut next);
    let carvers: Vec<Vec<Side>> = (0..1000)
        .map(|_| {
            let mut c = block(DVec3::splat(-48.0), DVec3::splat(48.0), &mut next);
            let m = DMat4::from_rotation_translation(rng.quat(), rng.vec(100.0));
            transform_sides(&mut c, m, false, false, None).expect("transform");
            c
        })
        .collect();
    let t = std::time::Instant::now();
    let mut pieces = 0usize;
    for c in &carvers {
        pieces += carve(&target, c, &mut next).expect("carve").len();
    }
    let carve_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = std::time::Instant::now();
    let mut solids = 0usize;
    for kind in Primitive::ALL {
        for _ in 0..100 {
            let p = PrimitiveParams {
                kind,
                sides: 16,
                bounds: Aabb::new(DVec3::splat(-128.0), DVec3::splat(128.0)),
                ..Default::default()
            };
            solids += make_primitive(&p, &mut next).expect("primitive").len();
        }
    }
    let prim_ms = t.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "convex_hull 10k points: {hull_ms:.2} ms ({} faces); 1000 carves (rotated boxes): {carve_ms:.2} ms ({pieces} pieces); 700 primitives (16 sides): {prim_ms:.2} ms ({solids} solids)",
        h.faces.len()
    );
}
