mod common;

use std::path::Path;

use common::open_vpk;
use hp_mdl::{LoadOptions, load_from, parse_phy, parse_phy_for_model, phy_path};

const GAME: &str = r"D:\SteamLibrary\steamapps\common\GarrysMod";

#[test]
fn every_vpk_collision_model_parses() {
    let mut any = false;
    for vpk in ["sourceengine/hl2_misc_dir.vpk", "garrysmod/garrysmod_dir.vpk"] {
        let path = Path::new(GAME).join(vpk);
        let Some(archive) = open_vpk(&path) else {
            eprintln!("skipping {}: not found", path.display());
            continue;
        };
        any = true;
        let (mut ok, mut failed, mut checked, mut outside) = (0usize, Vec::new(), 0usize, Vec::new());
        let (mut solids, mut hulls, mut no_pieces) = (0usize, 0usize, Vec::new());
        for phy in archive.files_with_ext(".phy") {
            let Some(data) = archive.read(&phy) else { continue };
            let mdl = phy.trim_end_matches(".phy").to_owned() + ".mdl";
            let parsed = match archive.read(&mdl) {
                Some(mdl_data) => parse_phy_for_model(&data, &mdl_data, hp_mdl::Pose::FirstSequence),
                None => parse_phy(&data),
            };
            match parsed {
                Ok(m) => {
                    ok += 1;
                    for s in &m.solids {
                        solids += 1;
                        let pieces = s.piece_triangles().count();
                        hulls += usize::from(pieces < s.triangles.len());
                        if s.ledges.is_empty() {
                            no_pieces.push(phy.clone());
                        }
                    }
                    if m.solids.len() == 1
                        && let (Some((lo, hi)), Ok(model)) =
                            (m.bounds(), load_from(&mdl, |p| archive.read(p), &LoadOptions::default()))
                    {
                        checked += 1;
                        let (mlo, mhi) = model.bounds;
                        let size = (0..3).map(|i| mhi[i] - mlo[i]).fold(1.0f32, f32::max);
                        let slack = size * 0.5 + 8.0;
                        let inside = (0..3).all(|i| lo[i] >= mlo[i] - slack && hi[i] <= mhi[i] + slack);
                        if !inside {
                            outside.push(format!("{mdl}: hull {lo:?} {hi:?}, model {mlo:?} {mhi:?}"));
                        }
                    }
                }
                Err(e) => failed.push(format!("{phy}: {e}")),
            }
        }
        eprintln!("{vpk}: {ok} parsed, {} failed, {checked} hulls checked, {} outside", failed.len(), outside.len());
        eprintln!("  {solids} solids, {hulls} with hull ledges, {} without pieces", no_pieces.len());
        for n in no_pieces.iter().take(10) {
            eprintln!("  no pieces: {n}");
        }
        for f in failed.iter().take(10) {
            eprintln!("  {f}");
        }
        for o in outside.iter().take(10) {
            eprintln!("  {o}");
        }
        assert!(failed.len() * 100 <= ok, "more than 1% of the collision models fail");
        assert!(outside.len() * 50 <= checked.max(1), "more than 2% of the hulls are off: {}", outside.len());
        assert!(no_pieces.is_empty(), "solids without convex pieces: {no_pieces:?}");
        assert_eq!(phy_path("models/a/b.mdl"), "models/a/b.phy");
    }
    if !any {
        eprintln!("skipping: no game VPK found");
    }
}
