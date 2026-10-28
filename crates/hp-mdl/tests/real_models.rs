mod common;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::{Report, check_model, open_gma, open_vpk, read_model};
use hp_mdl::{BodyGroupSelect, LoadOptions, Model, Pose, load};

const GAME: &str = r"D:\SteamLibrary\steamapps\common\GarrysMod";
const WORKSHOP: &str = r"D:\SteamLibrary\steamapps\workshop\content\4000";

fn gmod() -> PathBuf {
    Path::new(GAME).join("garrysmod")
}

fn scratch_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let target = exe.ancestors().nth(3).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("target"));
    target.join("scratch")
}

fn collect_mdls(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_mdls(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("mdl")) {
            out.push(path);
        }
    }
}

fn loose_models() -> Option<Vec<String>> {
    let models = gmod().join("models");
    if !models.is_dir() {
        eprintln!("skipping: {} not found", models.display());
        return None;
    }
    let mut files = Vec::new();
    collect_mdls(&models, &mut files);
    if let Ok(addons) = std::fs::read_dir(gmod().join("addons")) {
        for addon in addons.flatten() {
            collect_mdls(&addon.path().join("models"), &mut files);
        }
    }
    let mut files: Vec<String> = files.iter().filter_map(|p| p.to_str().map(str::to_string)).collect();
    files.sort();
    Some(files)
}

fn read_disk(path: &str) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

#[test]
fn load_all_loose_models() {
    let Some(files) = loose_models() else { return };
    let mut report = Report::default();
    for path in &files {
        report.add(path, read_model(path, read_disk));
    }
    eprintln!("{}", report.summary("loose models"));
    assert!(report.failures.is_empty(), "{} models failed", report.failures.len());
}

#[test]
fn load_all_vpk_models() {
    let vpks = [
        "sourceengine/hl2_misc_dir.vpk",
        "sourceengine/content_hl2_dir.vpk",
        "sourceengine/content_cstrike_dir.vpk",
        "garrysmod/garrysmod_dir.vpk",
        "garrysmod/fallbacks_dir.vpk",
    ];
    let mut any = false;
    let mut total = Report::default();
    for vpk in vpks {
        let path = Path::new(GAME).join(vpk);
        let Some(archive) = open_vpk(&path) else {
            eprintln!("skipping {}: not found or unreadable", path.display());
            continue;
        };
        any = true;
        let mut report = Report::default();
        for mdl in archive.files_with_ext(".mdl") {
            let files = read_model(&mdl, |p| archive.read(p));
            report.add(&format!("{vpk}:{mdl}"), files);
        }
        eprintln!("{}", report.summary(vpk));
        total.failures.append(&mut report.failures);
    }
    if !any {
        eprintln!("skipping: no game VPK found");
    }
    assert!(total.failures.is_empty(), "{} models failed", total.failures.len());
}

#[derive(Default)]
struct PoseStats {
    static_props: usize,
    posed: usize,
    bind_would_fit: usize,
    not_posed: usize,
    failures: Vec<String>,
}

fn fits(bounds: ([f32; 3], [f32; 3]), min: [f32; 3], max: [f32; 3]) -> bool {
    let (bmin, bmax) = bounds;
    (0..3).all(|k| {
        let tol = 1.0 + 0.05 * (bmax[k] - bmin[k]).max(max[k] - min[k]);
        bmin[k] >= min[k] - tol && bmax[k] <= max[k] + tol
    })
}

fn check_pose_bounds(name: &str, f: &common::ModelFiles, stats: &mut PoseStats) {
    let Ok(model) = load(&f.mdl, &f.vvd, &f.vtx, &LoadOptions::default()) else { return };
    if model.meshes.is_empty() {
        return;
    }
    if model.is_static_prop {
        stats.static_props += 1;
        if !fits(model.bounds, model.hull_min, model.hull_max) {
            stats.failures.push(format!(
                "{name}: static prop outside its hull: {:?} vs {:?} {:?}",
                model.bounds, model.hull_min, model.hull_max
            ));
        }
        return;
    }
    let Some(seq) = model.posed_sequence.and_then(|s| model.sequences.get(s)) else {
        stats.not_posed += 1;
        return;
    };
    stats.posed += 1;
    if !fits(model.bounds, seq.bbox_min, seq.bbox_max) {
        stats.failures.push(format!("{name}: posed bounds {:?} outside sequence box", model.bounds));
    }
    let bind = LoadOptions { pose: Pose::Bind, ..LoadOptions::default() };
    if load(&f.mdl, &f.vvd, &f.vtx, &bind).is_ok_and(|m| fits(m.bounds, seq.bbox_min, seq.bbox_max)) {
        stats.bind_would_fit += 1;
    }
}

impl PoseStats {
    fn summary(&self) -> String {
        format!(
            "static props checked {}, posed dynamic models {} (raw bind pose would fit only {}), not posed {}, failures {}",
            self.static_props,
            self.posed,
            self.bind_would_fit,
            self.not_posed,
            self.failures.len()
        )
    }
}

#[test]
fn poses_match_stored_bounding_boxes() {
    let mut stats = PoseStats::default();
    let mut found = false;
    for vpk in ["sourceengine/hl2_misc_dir.vpk", "sourceengine/content_hl2_dir.vpk", "garrysmod/garrysmod_dir.vpk"] {
        let Some(archive) = open_vpk(&Path::new(GAME).join(vpk)) else { continue };
        found = true;
        for mdl in archive.files_with_ext(".mdl") {
            let Ok(f) = read_model(&mdl, |p| archive.read(p)) else { continue };
            check_pose_bounds(&format!("{vpk}:{mdl}"), &f, &mut stats);
        }
    }
    if !found {
        eprintln!("skipping: no game VPK found");
        return;
    }
    eprintln!("{}", stats.summary());
    for f in &stats.failures {
        eprintln!("FAIL {f}");
    }
    assert!(stats.failures.is_empty());
}

#[derive(Default)]
struct AnimStats {
    sequences: usize,
    frames: usize,
    moving: usize,
    failures: Vec<String>,
    best: Vec<(f32, String, String, usize, f32, bool)>,
}

fn check_animation(name: &str, f: &common::ModelFiles, ani: Option<&[u8]>, stats: &mut AnimStats) {
    let opts = LoadOptions { pose: Pose::Bind, bone_weights: true, ..LoadOptions::default() };
    let Ok(model) = load(&f.mdl, &f.vvd, &f.vtx, &opts) else { return };
    if model.is_static_prop || model.meshes.is_empty() || model.bones.is_empty() {
        return;
    }
    let pose_all = |m: &[hp_mdl::SkinMatrix]| -> Vec<[f32; 3]> {
        model
            .meshes
            .iter()
            .flat_map(|mesh| mesh.vertices.iter().zip(&mesh.bone_weights))
            .map(|(v, w)| hp_mdl::pose_vertex(m, w, v.pos, v.normal).0)
            .collect()
    };
    let bounds = |pts: &[[f32; 3]]| -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for p in pts {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        (lo, hi)
    };
    for (s, seq) in model.sequences.iter().enumerate().take(6) {
        let Some(clip) = hp_mdl::animation_clip(&f.mdl, s) else { continue };
        let Some(first) = hp_mdl::skin_matrices(&f.mdl, &model.bones, s, 0, 0.0) else { continue };
        stats.sequences += 1;
        let p0 = pose_all(&first);
        let baked = LoadOptions { pose: Pose::Sequence(s), ..LoadOptions::default() };
        if let Ok(b) = load(&f.mdl, &f.vvd, &f.vtx, &baked)
            && b.posed_sequence == Some(s)
        {
            let bp: Vec<[f32; 3]> = b.meshes.iter().flat_map(|m| m.vertices.iter().map(|v| v.pos)).collect();
            if bp.len() == p0.len() {
                let worst = bp
                    .iter()
                    .zip(&p0)
                    .map(|(a, c)| (0..3).map(|k| (a[k] - c[k]).abs()).fold(0.0, f32::max))
                    .fold(0.0, f32::max);
                if worst > 0.05 {
                    stats
                        .failures
                        .push(format!("{name} seq {}: frame 0 differs from the baked pose by {worst}", seq.name));
                }
            }
        }
        let last = clip.frames.saturating_sub(1);
        let mut travel = 0.0f32;
        for frame in [last / 4, last / 2, (3 * last) / 4, last] {
            let Some(m) = hp_mdl::skin_matrices_with(&f.mdl, ani, &model.bones, s, frame, 0.5) else {
                stats.failures.push(format!("{name} seq {}: frame {frame} did not decode", seq.name));
                continue;
            };
            stats.frames += 1;
            let p = pose_all(&m);
            let (lo, hi) = bounds(&p);
            let clamp = |v: [f32; 3]| v.map(|c| c.clamp(-16384.0, 16384.0));
            let hidden = seq.flags & 0x0400 != 0;
            if !hidden && !fits((clamp(lo), clamp(hi)), seq.bbox_min, seq.bbox_max) {
                stats.failures.push(format!(
                    "{name} seq {} frame {frame}/{}: {:?} outside the sequence box {:?} {:?}",
                    seq.name,
                    clip.frames,
                    bounds(&p),
                    seq.bbox_min,
                    seq.bbox_max
                ));
            }
            let d = p
                .iter()
                .zip(&p0)
                .map(|(a, c)| (0..3).map(|k| (a[k] - c[k]).abs()).fold(0.0, f32::max))
                .fold(0.0, f32::max);
            travel = travel.max(d);
        }
        if travel > 1.0 {
            stats.moving += 1;
            stats.best.push((travel, name.to_owned(), seq.name.clone(), clip.frames, clip.fps, clip.looping));
        }
    }
}

#[test]
fn animation_frames_stay_in_sequence_bounds() {
    let mut stats = AnimStats::default();
    let mut found = false;
    for vpk in ["sourceengine/content_hl2_dir.vpk", "garrysmod/garrysmod_dir.vpk"] {
        let Some(archive) = open_vpk(&Path::new(GAME).join(vpk)) else { continue };
        found = true;
        for mdl in archive.files_with_ext(".mdl") {
            let Ok(f) = read_model(&mdl, |p| archive.read(p)) else { continue };
            let ani = hp_mdl::animation_block_path(&f.mdl).and_then(|p| archive.read(&p));
            check_animation(&mdl, &f, ani.as_deref(), &mut stats);
        }
    }
    if !found {
        eprintln!("skipping: no game VPK found");
        return;
    }
    stats.best.sort_by(|a, b| b.0.total_cmp(&a.0));
    eprintln!(
        "sequences played {}, frames checked {}, sequences that move {}, failures {}",
        stats.sequences,
        stats.frames,
        stats.moving,
        stats.failures.len()
    );
    for (t, m, s, n, fps, l) in stats.best.iter().take(15) {
        eprintln!("  moves {t:7.1}  {m}  [{s}] {n} frames @ {fps} fps looping={l}");
    }
    for f in stats.failures.iter() {
        eprintln!("FAIL {f}");
    }
    assert!(stats.moving > 50, "decoding later frames should move many models");
    assert!(stats.failures.is_empty(), "{} failures", stats.failures.len());
}

#[test]
#[ignore]
fn load_all_workshop_models() {
    let Ok(items) = std::fs::read_dir(WORKSHOP) else {
        eprintln!("skipping: {WORKSHOP} not found");
        return;
    };
    let mut report = Report::default();
    let mut poses = PoseStats::default();
    let mut addons = 0;
    for item in items.flatten() {
        let Ok(files) = std::fs::read_dir(item.path()) else { continue };
        for file in files.flatten() {
            let path = file.path();
            if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gma")) {
                continue;
            }
            let Some(archive) = open_gma(&path) else {
                eprintln!("unreadable addon {}", path.display());
                continue;
            };
            addons += 1;
            for mdl in archive.files_with_ext(".mdl") {
                let name = format!("{}:{mdl}", item.file_name().to_string_lossy());
                let files = read_model(&mdl, |p| archive.read(p));
                if let Ok(f) = &files {
                    check_pose_bounds(&name, f, &mut poses);
                }
                report.add(&name, files);
            }
        }
    }
    eprintln!("{}", report.summary(&format!("workshop ({addons} addons)")));
    eprintln!("poses: {}", poses.summary());
    let (stale_hulls, dynamic): (Vec<&String>, Vec<&String>) =
        poses.failures.iter().partition(|f| f.contains("static prop outside its hull"));
    eprintln!("static props with a stale hull: {}", stale_hulls.len());
    for f in &dynamic {
        eprintln!("POSE {f}");
    }
    assert!(report.failures.is_empty(), "{} models failed", report.failures.len());
    assert!(dynamic.is_empty(), "{} posed models outside their sequence box", dynamic.len());
}

#[test]
fn all_lods_and_body_groups() {
    let Some(files) = loose_models() else { return };
    let mut failures = Vec::new();
    let mut multi_lod = 0;
    for path in &files {
        let Ok(f) = read_model(path, read_disk) else { continue };
        let Ok(base) = load(&f.mdl, &f.vvd, &f.vtx, &LoadOptions::default()) else { continue };
        if base.num_lods > 1 {
            multi_lod += 1;
        }
        for lod in 0..=base.num_lods + 2 {
            for body_groups in [BodyGroupSelect::Default, BodyGroupSelect::All, BodyGroupSelect::Body(u32::MAX)] {
                let opts =
                    LoadOptions { lod, body_groups, tangents: true, bone_weights: true, ..LoadOptions::default() };
                match load(&f.mdl, &f.vvd, &f.vtx, &opts) {
                    Ok(model) => {
                        if let Err(e) = check_model(&model) {
                            failures.push(format!("{path} lod {lod}: {e}"));
                        }
                        for mesh in &model.meshes {
                            if mesh.bone_weights.len() != mesh.vertices.len()
                                || !(mesh.tangents.is_empty() || mesh.tangents.len() == mesh.vertices.len())
                            {
                                failures.push(format!("{path} lod {lod}: attribute length"));
                            }
                        }
                        if model.lod >= model.num_lods || model.lod > lod {
                            failures.push(format!("{path}: lod {} not clamped", model.lod));
                        }
                    }
                    Err(e) => failures.push(format!("{path} lod {lod}: {e}")),
                }
            }
        }
    }
    eprintln!("{multi_lod} of {} models have several LODs", files.len());
    for f in &failures {
        eprintln!("FAIL {f}");
    }
    assert!(failures.is_empty());
}

#[test]
fn timing() {
    let Some(files) = loose_models() else { return };
    let mut sets = Vec::new();
    for path in &files {
        if let Ok(f) = read_model(path, read_disk) {
            sets.push((path.clone(), f));
        }
    }
    let mut times: Vec<(Duration, usize, String)> = Vec::new();
    for (path, f) in &sets {
        let mut best = Duration::MAX;
        let mut tris = 0;
        for _ in 0..5 {
            let t = Instant::now();
            let Ok(model) = load(&f.mdl, &f.vvd, &f.vtx, &LoadOptions::default()) else { break };
            best = best.min(t.elapsed());
            tris = model.triangle_count();
        }
        if best != Duration::MAX {
            times.push((best, tris, path.clone()));
        }
    }
    if times.is_empty() {
        return;
    }
    times.sort();
    let pct = |p: usize| times[(times.len() - 1) * p / 100].0;
    let heavy: Vec<_> = times.iter().filter(|t| t.1 >= 20_000).collect();
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{} models, median {:?}, p90 {:?}, p99 {:?}, max {:?}",
        times.len(),
        pct(50),
        pct(90),
        pct(99),
        pct(100)
    );
    if let Some((d, tris, name)) = times.last() {
        let _ = writeln!(s, "slowest {d:?} ({tris} tris) {name}");
    }
    if let Some((d, tris, name)) = times.iter().max_by_key(|t| t.1) {
        let _ = writeln!(s, "largest {d:?} ({tris} tris) {name}");
    }
    let _ = writeln!(s, "{} models with >= 20k tris, max {:?}", heavy.len(), heavy.iter().map(|t| t.0).max());
    eprintln!("{s}");
    assert!(pct(50) < Duration::from_millis(5));
    assert!(pct(100) < Duration::from_millis(200));
}

fn write_obj(model: &Model, path: &Path) -> std::io::Result<()> {
    let mut out = String::new();
    let _ = writeln!(out, "# {} (lod {})", model.name, model.lod);
    let mut base = 1usize;
    for (i, mesh) in model.meshes.iter().enumerate() {
        let material = model.material_candidates(mesh.skin_ref, 0).into_iter().next().unwrap_or_default();
        let _ = writeln!(out, "o mesh{i}\nusemtl {material}");
        for v in &mesh.vertices {
            let _ = writeln!(out, "v {} {} {}", v.pos[0], v.pos[1], v.pos[2]);
            let _ = writeln!(out, "vn {} {} {}", v.normal[0], v.normal[1], v.normal[2]);
            let _ = writeln!(out, "vt {} {}", v.uv[0], 1.0 - v.uv[1]);
        }
        for t in mesh.indices.chunks_exact(3) {
            let (a, b, c) = (t[0] as usize + base, t[1] as usize + base, t[2] as usize + base);
            let _ = writeln!(out, "f {a}/{a}/{a} {b}/{b}/{b} {c}/{c}/{c}");
        }
        base += mesh.vertices.len();
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, out)
}

#[test]
fn castle_painting_obj_export() {
    let path = gmod().join("models").join("castle_painting.mdl");
    let Some(f) = path.to_str().and_then(|p| read_model(p, read_disk).ok()) else {
        eprintln!("skipping: {} not found", path.display());
        return;
    };
    let t = Instant::now();
    let model = load(&f.mdl, &f.vvd, &f.vtx, &LoadOptions::default()).expect("castle_painting loads");
    let elapsed = t.elapsed();
    check_model(&model).expect("sane model");
    assert!(model.triangle_count() > 0);
    assert!(!model.materials.is_empty() && !model.material_dirs.is_empty());

    let (min, max) = model.bounds;
    for k in 0..3 {
        assert!(min[k] <= max[k]);
        assert!(
            min[k] >= model.hull_min[k] - 1.0 && max[k] <= model.hull_max[k] + 1.0,
            "bounds {:?} vs hull {:?} {:?}",
            model.bounds,
            model.hull_min,
            model.hull_max
        );
    }
    for mesh in &model.meshes {
        for v in &mesh.vertices {
            let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!((len - 1.0).abs() < 0.01, "normal length {len}");
            assert!(v.uv.iter().all(|c| c.is_finite()));
        }
    }
    let candidates = model.material_candidates(model.meshes[0].skin_ref, 0);
    assert!(!candidates.is_empty());
    assert!(candidates.iter().all(|c| !c.contains('\\') && c == &c.to_lowercase() && !c.ends_with(".vmt")));

    let out = scratch_dir().join("castle_painting.obj");
    write_obj(&model, &out).expect("write obj");
    eprintln!(
        "castle_painting: {} tris, {} verts, bounds {:?}, hull {:?} {:?}, materials {:?}, loaded in {elapsed:?}, obj at {}",
        model.triangle_count(),
        model.vertex_count(),
        model.bounds,
        model.hull_min,
        model.hull_max,
        candidates,
        out.display()
    );
}
