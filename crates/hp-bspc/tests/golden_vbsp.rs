mod common;

use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, Mutex, OnceLock};

use common::lumps::Lumps;
use hp_bspc::emit::lump;

const VBSP: [&str; 2] = [
    "D:/SteamLibrary/steamapps/common/GarrysMod/bin/win64/vbsp.exe",
    "D:/SteamLibrary/steamapps/common/GarrysMod/bin/vbsp.exe",
];
const GAME: &str = "D:/SteamLibrary/steamapps/common/GarrysMod/garrysmod";

#[test]
#[ignore = "writes the fixtures with Valve's vbsp.exe; run by hand"]
fn regenerate_valve_fixtures() {
    let vbsp = VBSP.iter().find(|p| std::path::Path::new(p).is_file()).expect("Garry's Mod's vbsp.exe");
    let fixtures = common::fixtures();
    std::fs::create_dir_all(&fixtures).unwrap();
    let scratch = std::env::temp_dir().join("hp_bspc_golden_scratch");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    for (name, map) in common::maps::golden() {
        let text = map.text();
        std::fs::write(fixtures.join(format!("{name}.vmf")), &text).unwrap();
        let vmf = scratch.join(format!("{name}.vmf"));
        std::fs::write(&vmf, &text).unwrap();
        let out = std::process::Command::new(vbsp).arg("-game").arg(GAME).arg(&vmf).output().expect("vbsp runs");
        let log = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{name}: vbsp failed\n{log}");
        let leaks = name.contains("leak");
        assert_eq!(log.contains("**** leaked ****"), leaks, "{name}: leak\n{log}");
        for ext in if leaks { &["bsp", "lin"][..] } else { &["bsp", "prt"][..] } {
            std::fs::copy(scratch.join(format!("{name}.{ext}")), fixtures.join(format!("{name}.{ext}")))
                .unwrap_or_else(|e| panic!("{name}.{ext}: {e}\n{log}"));
        }
    }
    std::fs::remove_dir_all(&scratch).unwrap();
}

struct Pair {
    ours: Lumps,
    valve: Lumps,
    ours_bytes: Vec<u8>,
    valve_bytes: Vec<u8>,
    ours_prt: String,
    valve_prt: Option<String>,
    ours_lin: Option<String>,
    valve_lin: Option<String>,
    lit: bool,
}

fn pair(map: &str) -> Arc<Pair> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Pair>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(p) = cache.lock().unwrap().get(map) {
        return p.clone();
    }
    let dir = common::fixtures();
    let text = std::fs::read_to_string(dir.join(format!("{map}.vmf"))).expect("the fixture VMF");
    let opts = hp_bspc::CompileOptions { map_name: Some(map.to_owned()), ..Default::default() };
    let out = common::compile_with(&text, &common::GameMats, &opts);
    let valve_bytes = std::fs::read(dir.join(format!("{map}.bsp"))).expect("the fixture BSP");
    let p = Arc::new(Pair {
        ours: Lumps::from_bytes(&out.bsp),
        valve: Lumps::from_bytes(&valve_bytes),
        ours_bytes: out.bsp,
        valve_bytes,
        ours_prt: out.prt,
        valve_prt: std::fs::read_to_string(dir.join(format!("{map}.prt"))).ok(),
        ours_lin: out.leak.as_ref().map(|path| {
            path.iter()
                .map(|p| {
                    format!(
                        "{:.6} {:.6} {:.6}
",
                        p.x, p.y, p.z
                    )
                })
                .collect()
        }),
        valve_lin: std::fs::read_to_string(dir.join(format!("{map}.lin"))).ok(),
        lit: map == "hammerpro_test_areaportal",
    });
    cache.lock().unwrap().insert(map.to_owned(), p.clone());
    p
}

fn diff<T: PartialEq + Debug>(what: &str, ours: &[T], valve: &[T]) -> Vec<String> {
    let mut out = Vec::new();
    if ours.len() != valve.len() {
        out.push(format!("{what}: {} records here, {} in Valve's", ours.len(), valve.len()));
    }
    let differing: Vec<usize> = (0..ours.len().min(valve.len())).filter(|&i| ours[i] != valve[i]).collect();
    if !differing.is_empty() {
        out.push(format!("{what}: {} records differ, the first:", differing.len()));
        for &i in differing.iter().take(4) {
            out.push(format!("  [{i}] ours  {:?}\n  [{i}] valve {:?}", ours[i], valve[i]));
        }
    }
    out
}

fn diff_raw(p: &Pair, l: usize) -> Vec<String> {
    let (a, b) = (p.ours.raw(l), p.valve.raw(l));
    if a == b {
        return Vec::new();
    }
    let first = a.iter().zip(b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
    vec![format!("lump {l}: {} bytes here, {} in Valve's, first difference at byte {first}", a.len(), b.len())]
}

fn finish(map: &str, group: &str, problems: Vec<String>) {
    assert!(problems.is_empty(), "{map}, {group}: our compile differs from Valve's VBSP\n{}", problems.join("\n"));
}

fn unlit(mut faces: Vec<common::lumps::Face>) -> Vec<common::lumps::Face> {
    for f in &mut faces {
        f.styles = [0; 4];
        f.light_ofs = 0;
    }
    faces
}

fn canonical_dispinfo(raw: &[u8]) -> Vec<Vec<u8>> {
    raw.chunks_exact(176)
        .map(|x| {
            let mut x = x.to_vec();
            x[38] = 0;
            x[39] = 0;
            for s in 0..8 {
                let o = 48 + 6 * s;
                if x[o] == 0xff && x[o + 1] == 0xff {
                    x[o + 2..o + 6].fill(0);
                }
                x[o + 5] = 0;
            }
            for k in 0..4 {
                let o = 96 + 10 * k;
                let n = usize::from(x[o + 8]).min(4);
                x[o + 2 * n..o + 8].fill(0);
                x[o + 9] = 0;
            }
            x
        })
        .collect()
}

type GameLumpEntry = (u32, u16, u16, i64, i32);

fn game_lump(bytes: &[u8], lumps: &Lumps) -> (Vec<GameLumpEntry>, Vec<u8>) {
    let at = i32::from_le_bytes(bytes[8 + 16 * lump::GAME_LUMP..][..4].try_into().unwrap()) as i64;
    let d = lumps.raw(lump::GAME_LUMP);
    if d.len() < 4 {
        return (Vec::new(), d.to_vec());
    }
    let n = i32::from_le_bytes(d[0..4].try_into().unwrap()) as usize;
    let dir = (0..n)
        .map(|i| {
            let o = 4 + 16 * i;
            let r = |k: usize| i32::from_le_bytes(d[o + k..o + k + 4].try_into().unwrap());
            let flags = u16::from_le_bytes(d[o + 4..o + 6].try_into().unwrap());
            let version = u16::from_le_bytes(d[o + 6..o + 8].try_into().unwrap());
            (r(0) as u32, flags, version, i64::from(r(8)) - at, r(12))
        })
        .collect();
    (dir, d[(4 + 16 * n).min(d.len())..].to_vec())
}

fn portal_set(prt: &str) -> Vec<(u32, u32, Vec<[i64; 3]>)> {
    let mut out: Vec<(u32, u32, Vec<[i64; 3]>)> = prt
        .lines()
        .skip(3)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let head: Vec<&str> = l.split('(').next().unwrap_or("").split_whitespace().collect();
            let n: f64 = 1000.0;
            let mut pts: Vec<[i64; 3]> = l
                .split('(')
                .skip(1)
                .map(|p| {
                    let v: Vec<f64> = p
                        .trim_end()
                        .trim_end_matches(')')
                        .split_whitespace()
                        .map(|x| x.parse().expect("a number"))
                        .collect();
                    [0, 1, 2].map(|k| (v[k] * n).round() as i64)
                })
                .collect();
            let first = (0..pts.len()).min_by_key(|&i| pts[i]).unwrap_or(0);
            pts.rotate_left(first);
            (head[1].parse().unwrap(), head[2].parse().unwrap(), pts)
        })
        .collect();
    out.sort();
    out
}

const GROUPED: &[usize] = &[
    lump::ENTITIES,
    lump::PLANES,
    lump::TEXDATA,
    lump::VERTEXES,
    lump::NODES,
    lump::TEXINFO,
    lump::FACES,
    lump::LEAFS,
    lump::FACEIDS,
    lump::EDGES,
    lump::SURFEDGES,
    lump::MODELS,
    lump::LEAFFACES,
    lump::LEAFBRUSHES,
    lump::BRUSHES,
    lump::BRUSHSIDES,
    lump::AREAS,
    lump::AREAPORTALS,
    lump::DISPINFO,
    lump::ORIGINALFACES,
    lump::PHYSDISP,
    lump::PHYSCOLLIDE,
    lump::VERTNORMALS,
    lump::VERTNORMALINDICES,
    lump::DISP_VERTS,
    lump::DISP_LIGHTMAP_SAMPLE_POSITIONS,
    lump::GAME_LUMP,
    lump::LEAFWATERDATA,
    lump::PRIMITIVES,
    38,
    lump::PRIMINDICES,
    lump::PAKFILE,
    lump::CLIPPORTALVERTS,
    lump::TEXDATA_STRING_DATA,
    lump::TEXDATA_STRING_TABLE,
    lump::LEAFMINDISTTOWATER,
    lump::DISP_TRIS,
];

fn check(map: &str, group: &str) {
    let p = pair(map);
    let (o, v) = (&p.ours, &p.valve);
    let mut problems = Vec::new();
    match group {
        "planes" => problems.extend(diff("planes", &o.planes(), &v.planes())),
        "tree" => {
            problems.extend(diff("nodes", &o.nodes(), &v.nodes()));
            problems.extend(diff("leaves", &o.leaves(), &v.leaves()));
            problems.extend(diff("leaf faces", &o.leaf_faces(), &v.leaf_faces()));
            problems.extend(diff("leaf brushes", &o.leaf_brushes(), &v.leaf_brushes()));
        }
        "faces" => {
            if p.lit {
                problems.extend(diff("faces", &unlit(o.faces()), &unlit(v.faces())));
                problems.extend(diff("original faces", &unlit(o.original_faces()), &unlit(v.original_faces())));
            } else {
                problems.extend(diff("faces", &o.faces(), &v.faces()));
                problems.extend(diff("original faces", &o.original_faces(), &v.original_faces()));
            }
            problems.extend(diff("vertices", &o.vertices(), &v.vertices()));
            problems.extend(diff("edges", &o.edges(), &v.edges()));
            problems.extend(diff("surface edges", &o.surfedges(), &v.surfedges()));
            for l in [lump::FACEIDS, lump::PRIMITIVES, 38, lump::PRIMINDICES] {
                problems.extend(diff_raw(&p, l));
            }
            if !p.lit {
                problems.extend(diff_raw(&p, lump::VERTNORMALS));
                problems.extend(diff_raw(&p, lump::VERTNORMALINDICES));
            }
        }
        "textures" => {
            problems.extend(diff("texture infos", &o.texinfos(), &v.texinfos()));
            problems.extend(diff("texture data", &o.texdatas(), &v.texdatas()));
            problems.extend(diff("texture names", &o.texdata_names(), &v.texdata_names()));
            problems.extend(diff_raw(&p, lump::TEXDATA_STRING_DATA));
            problems.extend(diff_raw(&p, lump::TEXDATA_STRING_TABLE));
        }
        "brushes" => {
            problems.extend(diff("brushes", &o.brushes(), &v.brushes()));
            problems.extend(diff("brush sides", &o.brush_sides(), &v.brush_sides()));
        }
        "areas" => {
            problems.extend(diff("areas", &o.areas(), &v.areas()));
            problems.extend(diff("areaportals", &o.area_portals(), &v.area_portals()));
            problems.extend(diff("clip portal vertices", &o.clip_portal_verts(), &v.clip_portal_verts()));
        }
        "models" => problems.extend(diff("models", &o.models(), &v.models())),
        "water" => {
            problems.extend(diff("leaf water data", &o.leaf_water(), &v.leaf_water()));
            if !p.lit {
                problems.extend(diff(
                    "leaf distances to water",
                    &o.leaf_min_dist_to_water(),
                    &v.leaf_min_dist_to_water(),
                ));
            }
        }
        "displacements" => {
            problems.extend(diff(
                "dispinfo",
                &canonical_dispinfo(o.raw(lump::DISPINFO)),
                &canonical_dispinfo(v.raw(lump::DISPINFO)),
            ));
            for l in [lump::DISP_VERTS, lump::DISP_TRIS, lump::PHYSDISP, lump::DISP_LIGHTMAP_SAMPLE_POSITIONS] {
                problems.extend(diff_raw(&p, l));
            }
        }
        "physics" => problems.extend(diff_raw(&p, lump::PHYSCOLLIDE)),
        "entities" => {
            let strip = |ents: Vec<Vec<(String, String)>>| -> Vec<Vec<(String, String)>> {
                ents.into_iter()
                    .map(|e| {
                        e.into_iter()
                            .map(|(k, val)| if k == "vbsp_exe" { (k, "gmod".to_owned()) } else { (k, val) })
                            .collect()
                    })
                    .collect()
            };
            problems.extend(diff("entities", &strip(o.entities()), &strip(v.entities())));
        }
        "game_lump" => {
            let (od, odata) = game_lump(&p.ours_bytes, o);
            let (vd, vdata) = game_lump(&p.valve_bytes, v);
            problems.extend(diff("game lump directory (offsets from the lump)", &od, &vd));
            if odata != vdata {
                problems.push(format!("game lump data: {} bytes here, {} in Valve's", odata.len(), vdata.len()));
            }
        }
        "other_lumps" => {
            for l in (0..64).filter(|l| !GROUPED.contains(l)) {
                problems.extend(diff_raw(&p, l));
            }
        }
        "header" => {
            let ob = hp_bsp::Bsp::from_vec(p.ours_bytes.clone()).unwrap();
            let vb = hp_bsp::Bsp::from_vec(p.valve_bytes.clone()).unwrap();
            problems.extend(diff("bsp version", &[ob.version()], &[vb.version()]));
            problems.extend(diff("map revision", &[ob.map_revision()], &[vb.map_revision()]));
            let versions = |b: &hp_bsp::Bsp, l: &Lumps| -> Vec<(usize, u32, bool)> {
                (0..64)
                    .filter(|&i| !l.raw(i).is_empty() && i != lump::PAKFILE)
                    .map(|i| (i, b.lump_info(i).unwrap().version, b.is_compressed(i)))
                    .collect()
            };
            problems.extend(diff("lump (index, version, compressed)", &versions(&ob, o), &versions(&vb, v)));
        }
        "portals" => {
            let Some(valve_prt) = &p.valve_prt else { panic!("{map}: no portal file of Valve's") };
            let sorted = |s: &str| {
                let lines: Vec<String> = s.lines().map(|l| l.trim_end().to_owned()).collect();
                let mut body = lines.get(3..).unwrap_or_default().to_vec();
                body.sort();
                (lines.get(..3).unwrap_or_default().to_vec(), body)
            };
            let (oh, ob) = sorted(&p.ours_prt);
            let (vh, vb) = sorted(valve_prt);
            problems.extend(diff("portal file header", &oh, &vh));
            problems.extend(diff("portal lines (sorted)", &ob, &vb));
        }
        "portal_geometry" => {
            let Some(valve_prt) = &p.valve_prt else { panic!("{map}: no portal file of Valve's") };
            problems.extend(diff(
                "portals (rounded, from their lowest point, sorted)",
                &portal_set(&p.ours_prt),
                &portal_set(valve_prt),
            ));
        }
        "portal_file" => {
            let Some(valve_prt) = &p.valve_prt else { panic!("{map}: no portal file of Valve's") };
            let lines = |s: &str| s.lines().map(|l| l.trim_end().to_owned()).collect::<Vec<_>>();
            problems.extend(diff("portal file lines", &lines(&p.ours_prt), &lines(valve_prt)));
        }
        "leak_file" => {
            let Some(valve_lin) = &p.valve_lin else { panic!("{map}: no leak pointfile of Valve's") };
            let Some(ours_lin) = &p.ours_lin else { panic!("{map}: our compile does not leak") };
            let lines = |s: &str| s.lines().map(|l| l.trim_end().to_owned()).collect::<Vec<_>>();
            problems.extend(diff("leak pointfile lines", &lines(ours_lin), &lines(valve_lin)));
        }
        _ => panic!("unknown group {group}"),
    }
    finish(map, group, problems);
}

macro_rules! golden_map {
    ($map:ident: $($(#[$attr:meta])* $group:ident),* $(,)?) => {
        mod $map {
            $(
                #[test]
                $(#[$attr])*
                fn $group() {
                    super::check(stringify!($map), stringify!($group));
                }
            )*
        }
    };
}

golden_map!(
    golden_chop:
    planes,
    tree,
    faces,
    textures,
    brushes,
    areas,
    models,
    water,
    displacements,
    physics,
    entities,
    game_lump,
    other_lumps,
    header,
    portals,
    portal_geometry,
    portal_file
);
golden_map!(
    golden_tjunc:
    planes,
    tree,
    faces,
    textures,
    brushes,
    areas,
    models,
    water,
    displacements,
    physics,
    entities,
    game_lump,
    other_lumps,
    header,
    portals,
    portal_geometry,
    portal_file
);
golden_map!(
    golden_detail:
    planes,
    tree,
    faces,
    textures,
    brushes,
    areas,
    models,
    water,
    displacements,
    physics,
    entities,
    game_lump,
    other_lumps,
    header,
    portals,
    portal_geometry,
    portal_file
);
golden_map!(
    golden_water:
    planes,
    tree,
    faces,
    textures,
    brushes,
    areas,
    models,
    water,
    displacements,
    physics,
    entities,
    game_lump,
    other_lumps,
    header,
    portals,
    portal_geometry,
    portal_file
);
golden_map!(
    golden_areaportal:
    planes,
    tree,
    faces,
    textures,
    brushes,
    areas,
    models,
    water,
    displacements,
    physics,
    entities,
    game_lump,
    other_lumps,
    header,
    portals,
    portal_geometry,
    portal_file
);
golden_map!(
    golden_brush_entity:
    planes,
    tree,
    faces,
    textures,
    brushes,
    areas,
    models,
    water,
    displacements,
    physics,
    entities,
    game_lump,
    other_lumps,
    header,
    portals,
    portal_geometry,
    portal_file
);
golden_map!(
    golden_origin_brush:
    planes,
    tree,
    faces,
    textures,
    brushes,
    areas,
    models,
    water,
    displacements,
    physics,
    entities,
    game_lump,
    other_lumps,
    header,
    portals,
    portal_geometry,
    portal_file
);
golden_map!(
    golden_displacement:
    planes,
    tree,
    faces,
    textures,
    brushes,
    areas,
    models,
    water,
    displacements,
    physics,
    entities,
    game_lump,
    other_lumps,
    header,
    portals,
    portal_geometry,
    portal_file
);
golden_map!(
    golden_slanted:
    planes,
    tree,
    faces,
    textures,
    brushes,
    areas,
    models,
    water,
    displacements,
    physics,
    entities,
    game_lump,
    other_lumps,
    header,
    portals,
    portal_geometry,
    portal_file
);
golden_map!(
    hammerpro_test_areaportal:
    planes,
    tree,
    faces,
    textures,
    brushes,
    areas,
    models,
    water,
    displacements,
    physics,
    entities,
    portals,
    portal_geometry,
    portal_file
);
golden_map!(
    golden_leak:
    planes,
    tree,
    faces,
    textures,
    brushes,
    areas,
    models,
    water,
    displacements,
    physics,
    entities,
    game_lump,
    other_lumps,
    header,
    leak_file
);
