use super::*;

fn golden(name: &str) -> Vec<u8> {
    let text = include_str!("testdata/vphysics_golden.txt");
    let hex = text.lines().find_map(|l| l.strip_prefix(name).map(str::trim)).expect("golden entry");
    (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).expect("hex")).collect()
}

fn box_planes(mn: [f32; 3], mx: [f32; 3]) -> Vec<[f32; 4]> {
    let mn = mn.map(|v| 0.0 - v);
    vec![
        [1.0, 0.0, 0.0, mx[0]],
        [-1.0, 0.0, 0.0, mn[0]],
        [0.0, 1.0, 0.0, mx[1]],
        [0.0, -1.0, 0.0, mn[1]],
        [0.0, 0.0, 1.0, mx[2]],
        [0.0, 0.0, -1.0, mn[2]],
    ]
}

const WEDGE: [[f32; 4]; 6] = [
    [1.0, 0.0, 0.0, 96.0],
    [-1.0, 0.0, 0.0, 32.0],
    [0.0, 1.0, 0.0, 64.0],
    [0.0, -1.0, 0.0, 0.0],
    [0.0, 0.0, -1.0, 16.0],
    [0.707_106_77, 0.0, 0.707_106_77, 60.0],
];

fn first_diff(a: &[u8], b: &[u8]) -> Option<usize> {
    (0..a.len().max(b.len())).find(|&i| a.get(i) != b.get(i))
}

#[test]
fn convex_from_planes_matches_vphysics() {
    let got = convex(&WEDGE).expect("wedge");
    let want = golden("WEDGE");
    assert_eq!(first_diff(&got, &want), None);
}

#[test]
fn entity_collide_matches_vphysics() {
    let boxes = [
        ([0.0, 0.0, 0.0], [64.0, 32.0, 16.0]),
        ([64.0, 0.0, 0.0], [80.0, 32.0, 48.0]),
        ([-8.0, -8.0, -16.0], [72.0, 40.0, 0.0]),
    ];
    let mut ledges: Vec<Vec<u8>> = Vec::new();
    for (i, (mn, mx)) in boxes.iter().enumerate() {
        let mut l = convex(&box_planes(*mn, *mx)).expect("box");
        set_game_data(&mut l, 10 + i as i32);
        ledges.push(l);
    }
    let mut l = convex(&WEDGE).expect("wedge");
    set_game_data(&mut l, 20);
    ledges.push(l);
    let params = surface::ConvertParams {
        build_outer_hull: true,
        build_drag_axis_areas: true,
        drag_area_epsilon: 4.0,
        fast_inertia: false,
    };
    let got = surface::convex_to_collide(&ledges, &params, &mut hull).expect("collide");
    let want = golden("ENTITY");
    assert_eq!(first_diff(&got, &want), None);
}

#[test]
fn polysoup_matches_vphysics() {
    let p = |x: f32, y: f32, z: f32| [x, y, z];
    let tris = vec![
        ([p(0.0, 0.0, 0.0), p(128.0, 0.0, 0.0), p(0.0, 128.0, 8.0)], 3),
        ([p(128.0, 0.0, 0.0), p(128.0, 128.0, 16.0), p(0.0, 128.0, 8.0)], 3),
        ([p(0.0, 128.0, 8.0), p(128.0, 128.0, 16.0), p(0.0, 256.0, 0.0)], 5),
        ([p(128.0, 128.0, 16.0), p(128.0, 256.0, 4.0), p(0.0, 256.0, 0.0)], 5),
    ];
    let got = surface::polysoup_to_collide(&tris, &mut hull).expect("soup");
    let want = golden("SOUP");
    assert_eq!(first_diff(&got, &want), None);
}

#[test]
fn lump_records_and_text() {
    let side =
        |plane: [f32; 4]| BrushSide { plane, bevel: false, visible: true, surfaceprop: Some("concrete".to_owned()) };
    let solid = |brush: i32, mn: [f32; 3], mx: [f32; 3]| ConvexSolid {
        sides: box_planes(mn, mx).into_iter().map(side).collect(),
        brush,
    };
    let world = PhysModel {
        model_index: 0,
        solids: vec![solid(0, [0.0; 3], [64.0; 3]), solid(1, [64.0, 0.0, 0.0], [128.0, 64.0, 8.0])],
        ..Default::default()
    };
    let entity = PhysModel {
        model_index: 1,
        solids: vec![solid(2, [0.0; 3], [32.0, 32.0, 32.0])],
        size: [32.0; 3],
        faces: vec![("wood".to_owned(), 1024.0)],
    };
    let lump = physcollide_lump(&[entity, world]);
    let rd = |o: usize| i32::from_le_bytes([lump[o], lump[o + 1], lump[o + 2], lump[o + 3]]);
    assert_eq!(rd(0), 0);
    assert_eq!(rd(12), 1);
    let size = rd(16) as usize;
    let solid0 = &lump[20..20 + size];
    assert_eq!(&solid0[0..4], b"VPHY");
    let text_at = 16 + rd(4) as usize;
    let text = std::str::from_utf8(&lump[text_at..text_at + rd(8) as usize - 1]).expect("text");
    assert_eq!(
        text,
        "staticsolid {\n\"index\" \"0\"\n\"contents\" \"33570819\"\n}\nmaterialtable {\n\"concrete\" \"1\"\n}\n"
    );
    let next = text_at + rd(8) as usize;
    assert_eq!(rd(next), 1);
    let at = next + 16 + rd(next + 4) as usize;
    let text = std::str::from_utf8(&lump[at..at + rd(next + 8) as usize - 1]).expect("text");
    assert!(text.starts_with("solid {\n\"index\" \"0\"\n\"mass\" \"341.730865\"\n"), "{text}");
    assert!(text.ends_with("\"surfaceprop\" \"wood\"\n\"volume\" \"29790.996094\"\n}\n"), "{text}");
    let end = at + rd(next + 8) as usize;
    assert_eq!(&lump[end..], &[255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn surface_props_parse_like_vphysics() {
    let mut p = SurfaceProps::default();
    p.parse(
        b"\"default\" { \"density\" \"2000\" }
\"a\" { \"density\" \"100\" \"thickness\" \"0.5\" }
\"b\" { \"base\" \"a\" }
\"c\" { \"thickness\" \"1\" }
\"A\" { \"density\" \"300\" }
\"e\" { \"density\" \"10\" \"base\" \"a\" }
",
    );
    p.parse(
        b"\"b\" { \"density\" \"50\" }
\"d\" { \"base\" \"b\" }
\"default\" { \"density\" \"7\" }
\"F\" { \"friction\" \"1\" }
",
    );
    let want = [
        ("default", 7.0, 0.0),
        ("a", 300.0, 0.5),
        ("b", 50.0, 0.5),
        ("c", 2000.0, 1.0),
        ("e", 300.0, 0.5),
        ("$MATERIAL_INDEX_SHADOW", 2000.0, 0.0),
        ("d", 50.0, 0.5),
        ("f", 7.0, 0.0),
    ];
    let got: Vec<(&str, f32, f32)> = p.entries.iter().map(|(n, d, t)| (n.as_str(), *d, *t)).collect();
    assert_eq!(got, want);
    assert_eq!(p.find(""), None);
    assert_eq!(p.find("NoSuchProp"), Some(0));
    assert_eq!(p.name(p.find("B")), "b");
    assert_eq!(p.name(None), "default");
}

#[test]
fn hostile_input_is_dropped() {
    let nan = BrushSide { plane: [f32::NAN, 0.0, 0.0, 1.0], ..Default::default() };
    let inside_out = ConvexSolid {
        sides: box_planes([64.0; 3], [0.0; 3])
            .into_iter()
            .map(|plane| BrushSide { plane, ..Default::default() })
            .collect(),
        brush: 1,
    };
    let models = [
        PhysModel {
            model_index: 0,
            solids: vec![ConvexSolid { sides: vec![nan; 6], brush: 0 }, inside_out.clone()],
            ..Default::default()
        },
        PhysModel { model_index: 2, solids: vec![inside_out, ConvexSolid::default()], ..Default::default() },
    ];
    let lump = physcollide_lump(&models);
    assert_eq!(lump.len(), 16, "only the end record");
}

const VALVE_MAPS: [&str; 3] = [
    "C:/Users/oracle/Desktop/gmodmapping/test_maps/.valve_ref/hammerpro_test_areaportal.bsp",
    "C:/Users/oracle/AppData/Local/Temp/hp_cmp/vb/hammerpro_test_portes.bsp",
    "C:/Users/oracle/AppData/Local/Temp/hp_cmp/vb/hp_bspc_construct.bsp",
];

struct ValveBsp {
    lumps: Vec<Vec<u8>>,
}

impl ValveBsp {
    fn open(path: &str) -> Option<ValveBsp> {
        let bsp = hp_bsp::Bsp::open(path).ok()?;
        Some(ValveBsp { lumps: (0..64).map(|i| bsp.lump(i).map(|l| l.into_owned()).unwrap_or_default()).collect() })
    }
    fn i32(&self, l: usize, o: usize) -> i32 {
        let b = &self.lumps[l];
        i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    }
    fn f32(&self, l: usize, o: usize) -> f32 {
        f32::from_bits(self.i32(l, o) as u32)
    }
    fn u16(&self, l: usize, o: usize) -> u16 {
        u16::from_le_bytes([self.lumps[l][o], self.lumps[l][o + 1]])
    }
    fn model_brushes(&self, model: usize, mask: i32) -> Vec<usize> {
        let head = self.i32(14, model * 48 + 36);
        let mut used = vec![false; self.lumps[18].len() / 12];
        let mut stack = vec![head];
        while let Some(n) = stack.pop() {
            if n < 0 {
                let leaf = (-1 - n) as usize * 32;
                let first = self.u16(10, leaf + 24) as usize;
                let count = self.u16(10, leaf + 26) as usize;
                for k in first..first + count {
                    let b = self.u16(17, 2 * k) as usize;
                    if self.i32(18, b * 12 + 8) & mask != 0 {
                        used[b] = true;
                    }
                }
            } else {
                stack.push(self.i32(5, n as usize * 32 + 4));
                stack.push(self.i32(5, n as usize * 32 + 8));
            }
        }
        (0..used.len()).filter(|&b| used[b]).collect()
    }
    fn solid(&self, b: usize, names: &dyn Fn(i32) -> Option<String>) -> ConvexSolid {
        let first = self.i32(18, b * 12) as usize;
        let count = self.i32(18, b * 12 + 4) as usize;
        let sides = (first..first + count)
            .map(|s| {
                let plane = self.u16(19, s * 8) as usize;
                let texinfo = self.u16(19, s * 8 + 2) as i16;
                let p = [0, 4, 8, 12].map(|k| self.f32(1, plane * 20 + k));
                let texdata = if texinfo < 0 { -1 } else { self.i32(6, texinfo as usize * 72 + 68) };
                BrushSide {
                    plane: p,
                    bevel: self.lumps[19][s * 8 + 6] != 0,
                    visible: true,
                    surfaceprop: if texinfo < 0 { None } else { names(texdata) },
                }
            })
            .collect();
        ConvexSolid { sides, brush: b as i32 }
    }
    fn records(&self) -> Vec<(i32, Vec<Vec<u8>>, String)> {
        let d = &self.lumps[29];
        let rd = |o: usize| i32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
        let mut out = Vec::new();
        let mut o = 0;
        while o + 16 <= d.len() && rd(o) >= 0 {
            let (data, keys, count) = (rd(o + 4) as usize, rd(o + 8) as usize, rd(o + 12) as usize);
            let mut p = o + 16;
            let mut solids = Vec::new();
            for _ in 0..count {
                let s = rd(p) as usize;
                solids.push(d[p + 4..p + 4 + s].to_vec());
                p += 4 + s;
            }
            out.push((
                rd(o),
                solids,
                String::from_utf8_lossy(&d[o + 16 + data..o + 16 + data + keys - 1]).into_owned(),
            ));
            o += 16 + data + keys;
        }
        out
    }
}

fn materials_of(collide: &[u8]) -> Vec<u8> {
    let s = &collide[28..];
    let mut out = Vec::new();
    for lo in surface::surface_ledges(s) {
        let n = i16::from_le_bytes([s[lo + 12], s[lo + 13]]).max(0) as usize;
        out.extend((0..n).map(|t| s[lo + 16 * (t + 1) + 3] & 0x7f));
    }
    out
}

#[test]
fn valve_maps_rebuilt_from_their_brushes() {
    for path in VALVE_MAPS {
        let Some(bsp) = ValveBsp::open(path) else { continue };
        let recs = bsp.records();
        let world = &recs[0];
        let table: Vec<String> = world
            .2
            .split("materialtable {\n")
            .nth(1)
            .map(|t| {
                t.lines().take_while(|l| *l != "}").map(|l| l.split('"').nth(1).unwrap_or("").to_owned()).collect()
            })
            .unwrap_or_default();
        let contents: Vec<(usize, i32)> = world
            .2
            .split("staticsolid {")
            .skip(1)
            .enumerate()
            .filter_map(|(k, b)| {
                b.split('}')
                    .next()?
                    .split("\"contents\" \"")
                    .nth(1)
                    .and_then(|c| c.split('"').next())
                    .and_then(|c| c.parse().ok())
                    .map(|c| (k, c))
            })
            .collect();
        let mut identical = 0;
        for &(k, c) in &contents {
            let ids = bsp.model_brushes(0, c);
            let td = |t: i32| Some(format!("td{t}"));
            let solids: Vec<ConvexSolid> = ids.iter().map(|&b| bsp.solid(b, &td)).collect();
            let collide = surface::convex_to_collide(&convexes(&solids), &surface::ConvertParams::default(), &mut hull)
                .expect("collide");
            let picks = world_triangle_sides(&collide, &solids);
            let theirs = materials_of(&world.1[k]);
            assert_eq!(picks.len(), theirs.len(), "{path} static {k}: triangle count");
            let mut name_of: ahash::AHashMap<String, String> = ahash::AHashMap::new();
            for ((_, side), m) in picks.iter().zip(&theirs) {
                if *m > 0 {
                    let name = table[*m as usize - 1].clone();
                    let old = name_of.insert(side.surfaceprop.clone().unwrap_or_default(), name.clone());
                    assert!(old.is_none_or(|o| o == name), "{path} static {k}: a texture data with two materials");
                }
            }
            let names = |t: i32| name_of.get(&format!("td{t}")).cloned();
            let solids: Vec<ConvexSolid> = ids.iter().map(|&b| bsp.solid(b, &names)).collect();
            let mut collide =
                surface::convex_to_collide(&convexes(&solids), &surface::ConvertParams::default(), &mut hull)
                    .expect("collide");
            let props = SurfaceProps::standard();
            let mut t: Vec<Option<usize>> =
                if k == 0 { Vec::new() } else { table.iter().map(|n| props.find(n)).collect() };
            world_materials(&mut collide, &solids, &props, &mut t);
            let diff = first_diff(&collide, &world.1[k]);
            eprintln!(
                "{path}: static solid {k} (contents {c}): {} brushes, {} bytes, first difference {diff:?}",
                ids.len(),
                collide.len()
            );
            if diff.is_none() {
                identical += 1;
            }
            if k == 0 {
                let names: Vec<&str> = t.iter().map(|p| props.name(*p)).collect();
                assert_eq!(names[..], table[..t.len()], "{path}: material table order");
            }
        }
        assert_eq!(identical, contents.len(), "{path}: world static solids");
        let fluids = world.2.matches("fluid {").count();
        let soups: Vec<usize> = (0..world.1.len() - fluids).filter(|k| !contents.iter().any(|(c, _)| c == k)).collect();
        for &k in &soups {
            let c = &world.1[k];
            let s = &c[28..];
            let rd = |o: usize| u32::from_le_bytes([s[o], s[o + 1], s[o + 2], s[o + 3]]);
            let mut ledges = surface::surface_ledges(s);
            ledges.sort_unstable();
            let tris: Vec<([[f32; 3]; 3], i32)> = ledges
                .iter()
                .map(|&lo| {
                    let po = (lo as i64 + i64::from(rd(lo) as i32)) as usize;
                    let p = [0, 1, 2].map(|e| {
                        let i = (rd(lo + 20 + 4 * e) & 0xffff) as usize;
                        [0, 4, 8].map(|q| f32::from_bits(rd(po + 16 * i + q)))
                    });
                    (p, ((rd(lo + 16) >> 24) & 0x7f) as i32)
                })
                .collect();
            let ours = surface::polysoup_ivp_to_collide(&tris, &mut hull).expect("soup");
            assert_eq!(first_diff(&ours, c), None, "{path}: soup {k}");
        }
        eprintln!("{path}: {} terrain soups identical", soups.len());
        let mut same = 0;
        for (model, solids, _) in recs.iter().filter(|r| r.0 > 0) {
            let ids = bsp.model_brushes(*model as usize, 0x0203_403b);
            let m = *model as usize * 48;
            let size = [0, 4, 8].map(|k| bsp.f32(14, m + 12 + k) - bsp.f32(14, m + k));
            let pm = PhysModel {
                model_index: *model,
                solids: ids.iter().map(|&b| bsp.solid(b, &|_| None)).collect(),
                size,
                faces: Vec::new(),
            };
            let (blobs, _) = entity_record(&pm, &SurfaceProps::standard()).expect("entity");
            if blobs.first() == solids.first() {
                same += 1;
                continue;
            }
            let vs = &solids[0][28..];
            let valve: Vec<(i32, Vec<[u32; 3]>)> = surface::surface_ledges(vs)
                .into_iter()
                .map(|lo| (i32::from_le_bytes([vs[lo + 4], vs[lo + 5], vs[lo + 6], vs[lo + 7]]), ledge_points(vs, lo)))
                .collect();
            for sol in &pm.solids {
                let want = &valve.iter().find(|(b, _)| *b == sol.brush).expect("brush in Valve's entity").1;
                let ns: Vec<usize> = (0..sol.sides.len()).filter(|&i| !sol.sides[i].bevel).collect();
                let found = (0..1u32 << ns.len().min(10)).any(|mask| {
                    let mut t = sol.clone();
                    for (j, &i) in ns.iter().enumerate() {
                        t.sides[i].visible = mask & (1 << j) != 0;
                    }
                    shrunk_convex(&t).is_some_and(|l| ledge_points(&l, 0) == *want)
                });
                assert!(found, "{path}: model {model} brush {}", sol.brush);
            }
        }
        eprintln!("{path}: brush entities identical with every side visible: {same} / {}", recs.len() - 1);
    }
}

fn ledge_points(s: &[u8], lo: usize) -> Vec<[u32; 3]> {
    let rd = |o: usize| u32::from_le_bytes([s[o], s[o + 1], s[o + 2], s[o + 3]]);
    let po = (lo as i64 + i64::from(rd(lo) as i32)) as usize;
    let n = (rd(lo + 12) as u16 as i16).max(0) as usize;
    let mut v: Vec<[u32; 3]> = (0..n)
        .flat_map(|t| (0..3).map(move |e| (t, e)))
        .map(|(t, e)| {
            let i = (rd(lo + 16 * (t + 1) + 4 + 4 * e) & 0xffff) as usize;
            [rd(po + 16 * i), rd(po + 16 * i + 4), rd(po + 16 * i + 8)]
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}
