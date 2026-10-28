use glam::DVec3;
use hp_editor::prop_export::collect_prop;
use hp_editor::{Document, FaceRef, Id, Owner};
use hp_vmf::{Side, TexAxis};

use super::*;

struct Game(std::path::PathBuf);

impl Game {
    fn new(name: &str) -> Game {
        let dir = std::env::temp_dir().join(format!("hp_propper_bake_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("materials")).unwrap();
        Game(dir)
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    fn texture(&self, name: &str, rgba: [u8; 4]) {
        let img = hp_vtf::RgbaImage { width: 64, height: 64, pixels: rgba.repeat(64 * 64) };
        let opts = hp_vtf::EncodeOptions { format: hp_vtf::EncodeFormat::Bgra8888, ..Default::default() };
        self.write(&format!("materials/{name}.vtf"), &hp_vtf::encode(&img, &opts).unwrap());
    }

    fn fs(&self) -> GameFs {
        let fs = GameFs::empty();
        fs.add_game_dir(&self.0).unwrap();
        fs
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn box_sides(min: DVec3, max: DVec3, material: &str) -> Vec<Side> {
    let (x0, y0, z0) = (min.x, min.y, min.z);
    let (x1, y1, z1) = (max.x, max.y, max.z);
    let v = DVec3::new;
    let floor = (DVec3::X, DVec3::NEG_Y);
    let wall_y = (DVec3::X, DVec3::NEG_Z);
    let wall_x = (DVec3::Y, DVec3::NEG_Z);
    [
        ([v(x0, y1, z1), v(x1, y1, z1), v(x1, y0, z1)], floor),
        ([v(x0, y0, z0), v(x1, y0, z0), v(x1, y1, z0)], floor),
        ([v(x1, y1, z1), v(x0, y1, z1), v(x0, y1, z0)], wall_y),
        ([v(x1, y0, z0), v(x0, y0, z0), v(x0, y0, z1)], wall_y),
        ([v(x1, y1, z0), v(x1, y0, z0), v(x1, y0, z1)], wall_x),
        ([v(x0, y1, z1), v(x0, y0, z1), v(x0, y0, z0)], wall_x),
    ]
    .iter()
    .map(|(plane, (u, w))| Side {
        plane: *plane,
        material: material.to_owned(),
        uaxis: TexAxis { axis: *u, shift: 0.0, scale: 0.25 },
        vaxis: TexAxis { axis: *w, shift: 0.0, scale: 0.25 },
        ..Default::default()
    })
    .collect()
}

fn ramp(doc: &mut Document, material: &str) -> Id {
    let b =
        doc.create_solid(box_sides(DVec3::ZERO, DVec3::new(128.0, 128.0, 32.0), material), Owner::World).expect("box");
    let face = FaceRef { solid: b, side: 0 };
    doc.create_displacements(&[face], 2).expect("disp");
    let mut info = (**doc.scene().solid(b).unwrap().solid.sides[0].disp.as_ref().unwrap()).clone();
    for (i, a) in info.alphas.iter_mut().enumerate() {
        *a = (i % 5) as f64 / 4.0 * 255.0;
    }
    doc.set_disp_data("ramp", vec![(face, info)]).expect("alphas");
    b
}

fn texel(baked: &Baked, geo: &PropGeometry, s: f64, t: f64) -> [u8; 4] {
    let vtf = &baked.files.iter().find(|(p, _)| p.ends_with("blend.vtf")).unwrap().1;
    let img = hp_vtf::decode_rgba8(vtf, &hp_vtf::DecodeOptions::default()).unwrap();
    let tri = geo.tris.iter().find(|x| x.disp.is_some()).unwrap();
    let d = tri.disp.unwrap();
    let (g0, g1, g2) = (d.grid[0], d.grid[1], d.grid[2]);
    let solve = |k: usize| {
        let (a, b, c) = (tri.uv[0][k], tri.uv[1][k], tri.uv[2][k]);
        let m = [[g1[0] - g0[0], g2[0] - g0[0]], [g1[1] - g0[1], g2[1] - g0[1]]];
        let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
        let (ds, dt) = (s - g0[0], t - g0[1]);
        let x = (ds * m[1][1] - dt * m[0][1]) / det;
        let y = (m[0][0] * dt - m[1][0] * ds) / det;
        a + (b - a) * x + (c - a) * y
    };
    let (u, v) = (solve(0), solve(1));
    let (x, y) = ((u * f64::from(img.width)) as usize, (v * f64::from(img.height)) as usize);
    let i = (y * img.width as usize + x) * 4;
    [img.pixels[i], img.pixels[i + 1], img.pixels[i + 2], img.pixels[i + 3]]
}

#[test]
fn a_blend_displacement_bakes_its_mix() {
    let game = Game::new("mix");
    game.texture("t/red", [255, 0, 0, 255]);
    game.texture("t/blue", [0, 0, 255, 255]);
    game.write(
        "materials/t/blend.vmt",
        b"WorldVertexTransition { $basetexture t/red $basetexture2 t/blue $surfaceprop dirt $surfaceprop2 gravel }",
    );
    game.write("materials/t/plain.vmt", b"LightmappedGeneric { $basetexture t/red $surfaceprop concrete }");
    let fs = game.fs();
    let mut doc = Document::new_empty();
    let blend = ramp(&mut doc, "t/blend");
    let wall = doc
        .create_solid(box_sides(DVec3::new(200.0, 0.0, 0.0), DVec3::new(264.0, 64.0, 64.0), "t/plain"), Owner::World)
        .expect("wall");
    let mut geo = collect_prop(&doc, &[blend, wall], &|_| (64, 64)).expect("geometry");
    assert_eq!(dominant_surface_prop(&fs, &geo.tris).as_deref(), Some("concrete"));
    let baked = bake_blends(&fs, &mut geo, "models/hammerpro/test", "dirt").expect("baked");
    assert_eq!(baked.material, "models/hammerpro/test/blend");
    assert_eq!(baked.faces, 1);
    assert!(!baked.bump);
    assert!(baked.size.0 >= 512 && baked.size.0 <= MAX_ATLAS, "{:?}", baked.size);
    let names: Vec<&str> = baked.files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(names, ["materials/models/hammerpro/test/blend.vtf", "materials/models/hammerpro/test/blend.vmt"]);
    let vmt = String::from_utf8(baked.files[1].1.clone()).unwrap();
    assert!(vmt.starts_with("\"VertexLitGeneric\"") && vmt.contains("\"$surfaceprop\" \"dirt\""), "{vmt}");
    assert!(geo.tris.iter().filter(|t| t.disp.is_some()).all(|t| t.material == baked.material));
    assert!(geo.tris.iter().filter(|t| t.disp.is_none()).all(|t| t.material == "t/plain"));
    let start = texel(&baked, &geo, 0.02, 0.5);
    let end = texel(&baked, &geo, 0.98, 0.5);
    let mid = texel(&baked, &geo, 0.5, 0.5);
    assert!(start[0] > 230 && start[2] < 40, "{start:?}");
    assert!(end[2] > 230 && end[0] < 40, "{end:?}");
    assert!(mid[0] > 120 && mid[2] > 120, "mixed in linear light: {mid:?}");
}

#[test]
fn modulation_and_bump_maps_are_baked() {
    let game = Game::new("bump");
    game.texture("t/red", [255, 0, 0, 255]);
    game.texture("t/blue", [0, 0, 255, 255]);
    game.texture("t/flat_n", [128, 128, 255, 255]);
    game.texture("t/tilt_n", [218, 128, 218, 255]);
    game.texture("t/mod", [0, 128, 0, 255]);
    game.write(
        "materials/t/blend.vmt",
        b"WorldVertexTransition { $basetexture t/red $basetexture2 t/blue $bumpmap t/flat_n $bumpmap2 t/tilt_n \
          $blendmodulatetexture t/mod }",
    );
    let fs = game.fs();
    let mut doc = Document::new_empty();
    let b = ramp(&mut doc, "t/blend");
    let mut geo = collect_prop(&doc, &[b], &|_| (64, 64)).expect("geometry");
    let baked = bake_blends(&fs, &mut geo, "models/hammerpro/test2", "").expect("baked");
    assert!(baked.bump);
    assert!(baked.files.iter().any(|(p, _)| p.ends_with("blend_normal.vtf")));
    let vmt = String::from_utf8(baked.files.iter().find(|(p, _)| p.ends_with(".vmt")).unwrap().1.clone()).unwrap();
    assert!(vmt.contains("\"$bumpmap\" \"models/hammerpro/test2/blend_normal\""), "{vmt}");
    let a = texel(&baked, &geo, 0.4, 0.5);
    let c = texel(&baked, &geo, 0.6, 0.5);
    assert!(a[0] > 230 && a[2] < 30, "{a:?}");
    assert!(c[2] > 230 && c[0] < 30, "{c:?}");
    let normal = &baked.files.iter().find(|(p, _)| p.ends_with("blend_normal.vtf")).unwrap().1;
    let img = hp_vtf::decode_rgba8(normal, &hp_vtf::DecodeOptions::default()).unwrap();
    let tilt: Vec<i32> =
        img.pixels.chunks_exact(4).map(|p| (i32::from(p[0]) - 128).abs().max((i32::from(p[1]) - 128).abs())).collect();
    assert!(tilt.iter().any(|&t| t > 70), "a tilted part");
    assert!(tilt.iter().any(|&t| t < 8), "a flat part");
}

#[test]
fn plain_materials_are_not_baked() {
    let game = Game::new("plain");
    game.texture("t/red", [255, 0, 0, 255]);
    game.write("materials/t/plain.vmt", b"LightmappedGeneric { $basetexture t/red }");
    let fs = game.fs();
    let mut doc = Document::new_empty();
    let b = ramp(&mut doc, "t/plain");
    let mut geo = collect_prop(&doc, &[b], &|_| (64, 64)).expect("geometry");
    let before = geo.clone();
    assert!(bake_blends(&fs, &mut geo, "models/hammerpro/x", "").is_none());
    assert_eq!(geo, before);
    game.write("materials/t/broken.vmt", b"WorldVertexTransition { $basetexture t/red $basetexture2 t/none }");
    let fs = game.fs();
    let mut doc = Document::new_empty();
    let b = ramp(&mut doc, "t/broken");
    let mut geo = collect_prop(&doc, &[b], &|_| (64, 64)).expect("geometry");
    assert!(bake_blends(&fs, &mut geo, "models/hammerpro/x", "").is_none());
}

#[test]
fn cells_pack_inside_the_atlas() {
    let mut cells: Vec<Cell> =
        (0..40).map(|i| Cell { face: i, want: (2048.0, 1024.0), at: (0, 0), size: (0, 0) }).collect();
    let (w, h) = layout(&mut cells);
    assert!(w <= MAX_ATLAS && h <= MAX_ATLAS);
    for (i, a) in cells.iter().enumerate() {
        assert!(a.at.0 + a.size.0 + PAD <= w && a.at.1 + a.size.1 + PAD <= h, "{a:?} in {w} x {h}");
        for b in &cells[i + 1..] {
            let apart = a.at.0 + a.size.0 + PAD <= b.at.0 - PAD.min(b.at.0)
                || b.at.0 + b.size.0 + PAD <= a.at.0 - PAD.min(a.at.0)
                || a.at.1 + a.size.1 + PAD <= b.at.1 - PAD.min(b.at.1)
                || b.at.1 + b.size.1 + PAD <= a.at.1 - PAD.min(a.at.1);
            assert!(apart, "{a:?} overlaps {b:?}");
        }
    }
}

#[test]
fn the_detail_texture_is_baked_in() {
    let game = Game::new("detail");
    game.texture("t/gray", [128, 128, 128, 255]);
    game.texture("t/gray2", [128, 128, 128, 255]);
    game.texture("t/dark", [64, 64, 64, 255]);
    game.write(
        "materials/t/blend.vmt",
        b"WorldVertexTransition { $basetexture t/gray $basetexture2 t/gray2 $detail t/dark $detailscale 0.5 \
          $detailblendfactor 1 }",
    );
    game.write(
        "materials/t/half.vmt",
        b"WorldVertexTransition { $basetexture t/gray $basetexture2 t/gray2 $detail t/dark $detailblendfactor 0 }",
    );
    let fs = game.fs();
    let bake = |material: &str| {
        let mut doc = Document::new_empty();
        let b = ramp(&mut doc, material);
        let mut geo = collect_prop(&doc, &[b], &|_| (64, 64)).expect("geometry");
        let baked = bake_blends(&fs, &mut geo, "models/hammerpro/d", "").expect("baked");
        texel(&baked, &geo, 0.5, 0.5)
    };
    let dark = bake("t/blend");
    let plain = bake("t/half");
    assert!((i32::from(plain[0]) - 128).abs() <= 3, "factor 0 changes nothing: {plain:?}");
    assert!(dark[0] < 105 && dark[0] > 80, "{dark:?}");
}
