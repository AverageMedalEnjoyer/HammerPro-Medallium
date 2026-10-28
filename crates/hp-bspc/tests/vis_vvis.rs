mod common;

use common::{Solid, Vmf, block, convex};
use glam::DVec3;
use hp_bspc::vis::{self, VisOptions};

const VBSP: &str = "D:/SteamLibrary/steamapps/common/GarrysMod/bin/win64/vbsp.exe";
const VVIS: [&str; 2] = [
    "D:/SteamLibrary/steamapps/common/GarrysMod/bin/win64/vvis.exe",
    "D:/SteamLibrary/steamapps/common/GarrysMod/bin/vvis.exe",
];
const GAME: &str = "D:/SteamLibrary/steamapps/common/GarrysMod/garrysmod";
const WALL: &str = "dev/dev_measuregeneric01b";

const TRACE: (u32, u32) = (3, 5);
const TRACE_CORRIDOR: (u32, u32) = (0, 7);
const TRACE_MAZE: (u32, u32) = (0, 40);
const TRACE_GENERATED: (u32, u32) = (2, 262);

fn grid_map(plan: &[String], cell: f64, height: f64) -> Vmf {
    let v = DVec3::new;
    let mut m = Vmf::new();
    let rows = plan.len() as f64;
    let cols = plan.iter().map(|r| r.len()).max().unwrap_or(0) as f64;
    m.world.push(block(v(0.0, -rows * cell, -16.0), v(cols * cell, 0.0, 0.0), WALL));
    m.world.push(block(v(0.0, -rows * cell, height), v(cols * cell, 0.0, height + 16.0), WALL));
    for (r, line) in plan.iter().enumerate() {
        let bytes = line.as_bytes();
        let mut c = 0;
        while c < bytes.len() {
            if bytes[c] != b'#' {
                if bytes[c] == b'P' {
                    let center = v((c as f64 + 0.5) * cell, -(r as f64 + 0.5) * cell, 8.0);
                    m.point("info_player_start", center, &[("angles", "0 0 0")]);
                }
                c += 1;
                continue;
            }
            let start = c;
            while c < bytes.len() && bytes[c] == b'#' {
                c += 1;
            }
            let (y0, y1) = (-(r as f64 + 1.0) * cell, -(r as f64) * cell);
            m.world.push(block(v(start as f64 * cell, y0, 0.0), v(c as f64 * cell, y1, height), WALL));
        }
    }
    m
}

fn turned_pillar(center: DVec3, half: f64, angle: f64, height: f64) -> Solid {
    let (s, c) = angle.to_radians().sin_cos();
    let rot = |x: f64, y: f64| DVec3::new(center.x + x * c - y * s, center.y + x * s + y * c, 0.0);
    let corners = [rot(-half, -half), rot(half, -half), rot(half, half), rot(-half, half)];
    let mut planes: Vec<[DVec3; 3]> = Vec::new();
    for i in 0..4 {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        planes.push([a, b, b + DVec3::Z * height]);
    }
    planes.push([DVec3::new(0.0, 0.0, height), DVec3::new(1.0, 0.0, height), DVec3::new(0.0, 1.0, height)]);
    planes.push([DVec3::ZERO, DVec3::new(0.0, 1.0, 0.0), DVec3::new(1.0, 0.0, 0.0)]);
    convex(&planes, DVec3::new(center.x, center.y, height * 0.5), WALL)
}

fn rooms() -> Vmf {
    let plan: Vec<String> = [
        "###############",
        "#...#...#.....#",
        "#.......#.....#",
        "#...#...#.....#",
        "##.####.###.###",
        "#.....#.......#",
        "#.###.#.#####.#",
        "#.#.......#...#",
        "#.#.###.#.#.###",
        "#...#P..#.....#",
        "###############",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let mut m = grid_map(&plan, 128.0, 128.0);
    m.world.push(turned_pillar(DVec3::new(11.5 * 128.0, -2.5 * 128.0, 0.0), 40.0, 30.0, 128.0));
    m
}

fn corridor() -> Vmf {
    let n = 16;
    let plan: Vec<String> = vec![
        "#".repeat(4 * n + 2),
        format!("#{}#", ".###".repeat(n)),
        format!("#P{}#", ".".repeat(4 * n - 1)),
        format!("#{}#", "##.#".repeat(n)),
        "#".repeat(4 * n + 2),
    ];
    grid_map(&plan, 64.0, 128.0)
}

fn maze(n: usize, seed: u64) -> Vmf {
    let mut state = seed;
    let mut rnd = move |k: usize| {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((state >> 33) % k as u64) as usize
    };
    let size = 2 * n + 1;
    let mut g = vec![vec![b'#'; size]; size];
    let mut seen = vec![false; n * n];
    let mut stack = vec![(0usize, 0usize)];
    seen[0] = true;
    g[1][1] = b'.';
    while let Some(&(x, y)) = stack.last() {
        let mut next = Vec::new();
        for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x as i64 + dx, y as i64 + dy);
            if nx >= 0 && ny >= 0 && (nx as usize) < n && (ny as usize) < n && !seen[ny as usize * n + nx as usize] {
                next.push((nx as usize, ny as usize));
            }
        }
        if next.is_empty() {
            stack.pop();
            continue;
        }
        let (nx, ny) = next[rnd(next.len())];
        seen[ny * n + nx] = true;
        g[2 * ny + 1][2 * nx + 1] = b'.';
        g[y + ny + 1][x + nx + 1] = b'.';
        stack.push((nx, ny));
    }
    for _ in 0..n * 2 {
        let (r, c) = (1 + rnd(size - 2), 1 + rnd(size - 2));
        if (r + c) % 2 == 1 {
            g[r][c] = b'.';
        }
    }
    g[1][1] = b'P';
    let plan: Vec<String> = g.into_iter().map(|r| String::from_utf8(r).unwrap()).collect();
    grid_map(&plan, 96.0, 128.0)
}

fn maps() -> Vec<(&'static str, Vmf)> {
    vec![
        ("vvis_rooms", rooms()),
        ("vvis_corridor", corridor()),
        ("vvis_maze", maze(9, 5)),
        ("vvis_generated", common::maps::generated(7)),
    ]
}

struct Run {
    name: &'static str,
    map: &'static str,
    args: &'static [&'static str],
}

const RUNS: &[Run] = &[
    Run { name: "vvis_rooms_full", map: "vvis_rooms", args: &[] },
    Run { name: "vvis_rooms_fast", map: "vvis_rooms", args: &["-fast"] },
    Run { name: "vvis_rooms_r300", map: "vvis_rooms", args: &["-radius_override", "300"] },
    Run { name: "vvis_rooms_r300_fast", map: "vvis_rooms", args: &["-fast", "-radius_override", "300"] },
    Run { name: "vvis_corridor_full", map: "vvis_corridor", args: &[] },
    Run { name: "vvis_corridor_fast", map: "vvis_corridor", args: &["-fast"] },
    Run { name: "vvis_corridor_r512", map: "vvis_corridor", args: &["-radius_override", "512"] },
    Run { name: "vvis_corridor_r512_fast", map: "vvis_corridor", args: &["-fast", "-radius_override", "512"] },
    Run { name: "vvis_corridor_nosort", map: "vvis_corridor", args: &["-nosort"] },
    Run { name: "vvis_maze_full", map: "vvis_maze", args: &[] },
    Run { name: "vvis_maze_fast", map: "vvis_maze", args: &["-fast"] },
    Run { name: "vvis_maze_r512", map: "vvis_maze", args: &["-radius_override", "512"] },
    Run { name: "vvis_generated_full", map: "vvis_generated", args: &[] },
    Run { name: "vvis_generated_fast", map: "vvis_generated", args: &["-fast"] },
    Run { name: "vvis_generated_r400", map: "vvis_generated", args: &["-radius_override", "400"] },
    Run { name: "vvis_generated_r400_fast", map: "vvis_generated", args: &["-fast", "-radius_override", "400"] },
];

type Trace = (&'static str, &'static str, (u32, u32), &'static [&'static str]);

const TRACES: &[Trace] = &[
    ("vvis_rooms_trace", "vvis_rooms", TRACE, &[]),
    ("vvis_corridor_trace_r512", "vvis_corridor", TRACE_CORRIDOR, &["-radius_override", "512"]),
    ("vvis_maze_trace", "vvis_maze", TRACE_MAZE, &[]),
    ("vvis_generated_trace", "vvis_generated", TRACE_GENERATED, &[]),
];

fn options(args: &[&str]) -> VisOptions {
    let mut o = VisOptions::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match *a {
            "-fast" => o.fast = true,
            "-nosort" => o.sort = false,
            "-radius_override" => o.radius_override = it.next().and_then(|r| r.parse().ok()),
            other => panic!("unknown option {other}"),
        }
    }
    o
}

fn bsp_lump(bsp: &[u8], l: usize) -> Vec<u8> {
    let at = 8 + 16 * l;
    let ofs = i32::from_le_bytes(bsp[at..at + 4].try_into().unwrap()) as usize;
    let len = i32::from_le_bytes(bsp[at + 4..at + 8].try_into().unwrap()) as usize;
    bsp[ofs..ofs + len].to_vec()
}

fn run_vvis(exe: &str, scratch: &std::path::Path, folder: &str, map: &str, args: &[&str]) -> (Vec<u8>, Option<String>) {
    let dir = scratch.join(folder);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for ext in ["bsp", "prt"] {
        std::fs::copy(scratch.join(format!("{map}.{ext}")), dir.join(format!("{map}.{ext}"))).unwrap();
    }
    let out = std::process::Command::new(exe)
        .args(args)
        .arg("-game")
        .arg(GAME)
        .arg(dir.join(format!("{map}.bsp")))
        .output()
        .expect("vvis runs");
    let log = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{folder}: vvis failed\n{log}");
    let bsp = std::fs::read(dir.join(format!("{map}.bsp"))).unwrap();
    let lin = std::fs::read_to_string(dir.join(format!("{map}.lin"))).ok();
    (bsp, lin)
}

#[test]
#[ignore = "writes the fixtures with Valve's vbsp.exe and vvis.exe; run by hand"]
fn regenerate_vvis_fixtures() {
    let fixtures = common::fixtures();
    let scratch = std::env::temp_dir().join("hp_vvis_scratch");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    for (name, map) in maps() {
        let text = map.text();
        std::fs::write(fixtures.join(format!("{name}.vmf")), &text).unwrap();
        let vmf = scratch.join(format!("{name}.vmf"));
        std::fs::write(&vmf, &text).unwrap();
        let out = std::process::Command::new(VBSP).arg("-game").arg(GAME).arg(&vmf).output().expect("vbsp runs");
        let log = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success() && !log.contains("leaked"), "{name}: vbsp failed\n{log}");
        std::fs::copy(scratch.join(format!("{name}.prt")), fixtures.join(format!("{name}.prt"))).unwrap();
    }
    for run in RUNS {
        let mut lumps = Vec::new();
        for (k, exe) in VVIS.iter().enumerate() {
            let (bsp, _) = run_vvis(exe, &scratch, &format!("{}_{k}", run.name), run.map, run.args);
            lumps.push(bsp_lump(&bsp, 4));
        }
        if lumps[0] != lumps[1] {
            eprintln!("{}: the 32 bit VVIS differs from the 64 bit one", run.name);
        }
        std::fs::write(fixtures.join(format!("{}.vis", run.name)), &lumps[0]).unwrap();
    }
    for (name, map, (a, b), extra) in TRACES {
        let (a, b) = (a.to_string(), b.to_string());
        let mut args: Vec<&str> = vec!["-threads", "1", "-trace", &a, &b];
        args.extend_from_slice(extra);
        let mut lins = Vec::new();
        for (k, exe) in VVIS.iter().enumerate() {
            let (_, lin) = run_vvis(exe, &scratch, &format!("{name}_{k}"), map, &args);
            lins.push(lin.unwrap_or_else(|| panic!("{name}: no trace")));
        }
        if lins[0] != lins[1] {
            eprintln!("{name}: the 32 bit VVIS differs from the 64 bit one");
        }
        std::fs::write(fixtures.join(format!("{name}.lin")), &lins[0]).unwrap();
    }
    std::fs::remove_dir_all(&scratch).unwrap();
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(common::fixtures().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn portals(map: &str) -> (usize, Vec<vis::VisPortal>) {
    let text = String::from_utf8(fixture(&format!("{map}.prt"))).unwrap();
    vis::parse_prt(&text).expect("a valid portal file")
}

fn decode(lump: &[u8], c: usize, kind: usize) -> Vec<bool> {
    let n = i32::from_le_bytes(lump[0..4].try_into().unwrap()) as usize;
    let at = 4 + 8 * c + 4 * kind;
    let mut i = u32::from_le_bytes(lump[at..at + 4].try_into().unwrap()) as usize;
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        let b = lump[i];
        i += 1;
        if b == 0 {
            let run = lump[i] as usize;
            i += 1;
            out.extend(std::iter::repeat_n(false, 8 * run));
        } else {
            out.extend((0..8).map(|k| b & (1 << k) != 0));
        }
    }
    out.truncate(n);
    out
}

fn rows_of(rows: &[Vec<u8>], n: usize) -> Vec<Vec<bool>> {
    rows.iter().map(|r| (0..n).map(|b| r[b >> 3] & (1 << (b & 7)) != 0).collect()).collect()
}

fn row_diff(what: &str, ours: &[Vec<bool>], valve: &[Vec<bool>]) -> Vec<String> {
    let mut out = Vec::new();
    for (c, (o, v)) in ours.iter().zip(valve).enumerate() {
        if o != v {
            let extra: Vec<usize> = (0..o.len()).filter(|b| o[*b] && !v[*b]).collect();
            let missing: Vec<usize> = (0..o.len()).filter(|b| !o[*b] && v[*b]).collect();
            out.push(format!("{what} cluster {c}: ours sees {extra:?} more, {missing:?} less"));
        }
    }
    out
}

const NOT_EXACT: &[&str] = &["vvis_maze_full", "vvis_maze_r512", "vvis_generated_full", "vvis_generated_r400"];

const NO_SIGHT: &[(&str, &[(usize, usize)])] = &[
    ("vvis_generated_full", &[(16, 237), (77, 141), (133, 163), (141, 77), (163, 133), (237, 16)]),
    (
        "vvis_generated_r400",
        &[
            (16, 201),
            (70, 128),
            (71, 107),
            (107, 71),
            (128, 70),
            (133, 163),
            (136, 183),
            (137, 183),
            (163, 133),
            (183, 136),
            (183, 137),
            (201, 16),
        ],
    ),
];

fn bits(rows: &[Vec<u8>], a: usize, b: usize) -> bool {
    rows[a][b >> 3] & (1 << (b & 7)) != 0
}

fn check_run(name: &str) {
    let run = RUNS.iter().find(|r| r.name == name).expect("a run");
    let (n, prt) = portals(run.map);
    let valve = fixture(&format!("{name}.vis"));
    let opts = VisOptions { vvis_flow: true, ..options(run.args) };
    let pvs = vis::compute_pvs(n, &prt, &opts, &mut |_| {});
    let valve_pvs: Vec<Vec<bool>> = (0..n).map(|c| decode(&valve, c, 0)).collect();
    if !opts.fast {
        let bounded = vis::compute_pvs(n, &prt, &VisOptions { vvis_flow: false, ..opts }, &mut |_| {});
        let allowed = NO_SIGHT.iter().find(|(r, _)| *r == name).map_or(&[][..], |(_, p)| p);
        for (a, row) in valve_pvs.iter().enumerate() {
            for (b, v) in row.iter().enumerate() {
                if *v && !bits(&bounded, a, b) {
                    assert!(allowed.contains(&(a, b)), "{name}: the bounded flow hides cluster {b} from cluster {a}");
                }
            }
        }
    }
    let pas = vis::compute_pas(&pvs);
    let lump = vis::vis_lump(&pvs, &pas);
    let exact = !NOT_EXACT.contains(&name);
    let mut problems = Vec::new();
    for kind in 0..2 {
        let valve_rows: Vec<Vec<bool>> = (0..n).map(|c| decode(&valve, c, kind)).collect();
        let ours = rows_of(if kind == 0 { &pvs } else { &pas }, n);
        if exact {
            problems.extend(row_diff(["PVS", "PAS"][kind], &ours, &valve_rows));
        } else {
            for (a, (o, v)) in ours.iter().zip(&valve_rows).enumerate() {
                let hidden: Vec<usize> = (0..n).filter(|b| v[*b] && !o[*b]).collect();
                if !hidden.is_empty() {
                    problems.push(format!("{} cluster {a}: ours hides {hidden:?}", ["PVS", "PAS"][kind]));
                }
            }
        }
    }
    if exact && problems.is_empty() && lump != valve {
        let first = lump.iter().zip(&valve).position(|(a, b)| a != b).unwrap_or(lump.len().min(valve.len()));
        problems.push(format!(
            "lump bytes: ours {} bytes, Valve's {}, first difference at {first}",
            lump.len(),
            valve.len()
        ));
    }
    let seen: usize = pvs.iter().flat_map(|r| r.iter()).map(|b| b.count_ones() as usize).sum();
    let valve_seen: usize = valve_pvs.iter().flatten().filter(|v| **v).count();
    eprintln!(
        "{name}: {n} clusters, {} portals, {seen} visible pairs (VVIS {valve_seen}), lump {} bytes",
        prt.len(),
        lump.len()
    );
    assert!(
        problems.is_empty(),
        "{name} differs from VVIS:
{}",
        problems.join(
            "
"
        )
    );
}

#[test]
fn rooms_full() {
    check_run("vvis_rooms_full");
}

#[test]
fn rooms_fast() {
    check_run("vvis_rooms_fast");
}

#[test]
fn rooms_radius_300() {
    check_run("vvis_rooms_r300");
}

#[test]
fn rooms_radius_300_fast() {
    check_run("vvis_rooms_r300_fast");
}

#[test]
fn corridor_full() {
    check_run("vvis_corridor_full");
}

#[test]
fn corridor_fast() {
    check_run("vvis_corridor_fast");
}

#[test]
fn corridor_radius_512() {
    check_run("vvis_corridor_r512");
}

#[test]
fn corridor_radius_512_fast() {
    check_run("vvis_corridor_r512_fast");
}

#[test]
fn corridor_nosort() {
    check_run("vvis_corridor_nosort");
}

#[test]
fn maze_runs() {
    for name in ["vvis_maze_full", "vvis_maze_fast", "vvis_maze_r512"] {
        check_run(name);
    }
}

#[test]
fn generated_runs() {
    for name in ["vvis_generated_full", "vvis_generated_fast", "vvis_generated_r400", "vvis_generated_r400_fast"] {
        check_run(name);
    }
}

#[test]
fn fast_differs_from_full_on_the_rooms() {
    let full = fixture("vvis_rooms_full.vis");
    let fast = fixture("vvis_rooms_fast.vis");
    let n = portals("vvis_rooms").0;
    let differ = (0..n).filter(|c| decode(&full, *c, 0) != decode(&fast, *c, 0)).count();
    assert!(differ * 4 >= n, "{differ} of {n} rows differ");
    let r = fixture("vvis_corridor_r512.vis");
    let full = fixture("vvis_corridor_full.vis");
    let n = portals("vvis_corridor").0;
    assert!((0..n).any(|c| decode(&full, c, 0) != decode(&r, c, 0)), "the radius culls");
}

#[test]
fn traces() {
    for (name, map, (a, b), extra) in TRACES {
        let (n, prt) = portals(map);
        let mut opts = options(extra);
        opts.trace = Some((*a, *b));
        let trace = vis::compute_trace(n, &prt, &opts, None).expect("valid clusters").expect("a trace");
        let valve = String::from_utf8(fixture(&format!("{name}.lin"))).unwrap();
        let ours = trace.text();
        if let Some(i) = ours.lines().zip(valve.lines()).position(|(a, b)| a != b) {
            panic!("{name}: line {i}: ours {:?}, VVIS {:?}", ours.lines().nth(i), valve.lines().nth(i));
        }
        assert_eq!(ours, valve, "{name}");
        assert_eq!(vis::trace_file_name("C:/maps/x.bsp"), "C:/maps/x.lin");
    }
}
