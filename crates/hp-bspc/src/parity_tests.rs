use glam::{DVec3, Vec3};

use crate::brush::{BrushSide, MapBrush};
use crate::materials::{contents, surf};
use crate::planes::PlaneSet;

fn cube(planes: &mut PlaneSet, lo: DVec3, hi: DVec3, c: u32) -> MapBrush {
    let mut sides = Vec::new();
    for axis in 0..3 {
        for (dir, d) in [(1.0, hi[axis]), (-1.0, -lo[axis])] {
            let mut n = DVec3::ZERO;
            n[axis] = dir;
            let p = planes.find(n, d).unwrap();
            sides.push(BrushSide {
                plane: p,
                texinfo: 0,
                surf: 0,
                winding: None,
                bevel: false,
                visible: false,
                no_split: false,
                side_id: 0,
                disp: None,
                contents: c,
                fwinding: None,
                smoothing: 0,
                overlays: Vec::new(),
            });
        }
    }
    let mut b = MapBrush { id: 1, entity: 0, contents: c, sides, mins: lo, maxs: hi, surfaceprop: String::new() };
    assert!(b.make_windings(planes));
    b.add_bevels(planes);
    b
}

#[test]
fn parallel_world_tree_matches_one_block_after_the_other() {
    let mut planes = PlaneSet::new();
    let mut brushes = Vec::new();
    let boxes = [
        (DVec3::new(-1500.0, -600.0, -64.0), DVec3::new(1800.0, 1300.0, 0.0), contents::SOLID),
        (DVec3::new(-1500.0, -600.0, 512.0), DVec3::new(1800.0, 1300.0, 576.0), contents::SOLID),
        (DVec3::new(-1564.0, -600.0, 0.0), DVec3::new(-1500.0, 1300.0, 512.0), contents::SOLID),
        (DVec3::new(1800.0, -600.0, 0.0), DVec3::new(1864.0, 1300.0, 512.0), contents::SOLID),
        (DVec3::new(-1500.0, -664.0, 0.0), DVec3::new(1800.0, -600.0, 512.0), contents::SOLID),
        (DVec3::new(-1500.0, 1300.0, 0.0), DVec3::new(1800.0, 1364.0, 512.0), contents::SOLID),
        (DVec3::new(-200.0, -100.0, 0.0), DVec3::new(300.0, 900.0, 200.0), contents::SOLID),
        (DVec3::new(100.0, 300.0, 0.0), DVec3::new(700.0, 500.0, 300.0), contents::SOLID),
        (DVec3::new(900.0, 0.0, 0.0), DVec3::new(1500.0, 1000.0, 96.0), contents::WATER | contents::TRANSLUCENT),
        (DVec3::new(1100.0, 400.0, 0.0), DVec3::new(1116.0, 600.0, 200.0), contents::AREAPORTAL),
    ];
    for (lo, hi, c) in boxes {
        brushes.push(cube(&mut planes, lo, hi, c));
    }
    let ids: Vec<usize> = (0..brushes.len()).collect();
    let contents_of: Vec<u32> = brushes.iter().map(|b| b.contents).collect();
    let vis = crate::brushbsp::Visibility::Windings;
    let (mut pa, mut pb) = (planes.clone(), planes.clone());
    let (mut ca, mut cb) = (contents_of.clone(), contents_of.clone());
    let a = crate::brushbsp::world_tree(&mut pa, &brushes, &mut ca, &ids, vis, 1024);
    let b = crate::brushbsp::world_tree_serial(&mut pb, &brushes, &mut cb, &ids, vis, 1024);
    assert_eq!(pa.len(), pb.len(), "the same planes, in the same order");
    for i in 0..pa.len() {
        assert_eq!(pa.getf(i), pb.getf(i));
    }
    assert_eq!(ca, cb);
    assert_eq!(ca[9] & contents::WATER, contents::WATER, "the areaportal took the water's contents");
    assert_eq!(a.nodes.len(), b.nodes.len());
    assert!(a.nodes.len() > 20);
    for (x, y) in a.nodes.iter().zip(&b.nodes) {
        assert_eq!(
            (x.plane, x.children, x.contents, x.brushes.len()),
            (y.plane, y.children, y.contents, y.brushes.len())
        );
    }
}

#[test]
fn material_paths_and_side_contents() {
    use crate::materials::{material_path, side_contents};
    assert_eq!(material_path("/op_sand"), "op_sand");
    assert_eq!(material_path("//Concrete\\\\Floor//01"), "Concrete/Floor/01");
    assert_eq!(format!("maps/big/{}_wvt_patch", material_path("/OP_blend")), "maps/big/OP_blend_wvt_patch");
    assert_eq!(side_contents(0, 0), contents::SOLID);
    assert_eq!(side_contents(contents::PLAYERCLIP, 0), contents::PLAYERCLIP | contents::DETAIL);
    assert_eq!(side_contents(contents::SOLID, surf::SKIP), 0);
    assert_eq!(side_contents(contents::WATER, 0), contents::WATER);
    assert_eq!(side_contents(contents::LADDER, 0), contents::LADDER | contents::SOLID);
}

#[test]
fn texture_info_offsets_add_like_vbsp() {
    let mut tex = crate::texinfo::TexInfos::new();
    let m = crate::texinfo::SideMapping {
        u_axis: DVec3::new(-1.0, 0.0, 0.0),
        u_shift: 210.0,
        u_scale: 0.1,
        v_axis: DVec3::new(0.0, -0.8944, -0.4472),
        v_shift: -243.0,
        v_scale: 0.1,
        lightmap_scale: 16.0,
    };
    let i = tex.for_side(&m, DVec3::new(11615.0, -12421.0, -12803.1), 0, 0);
    let info = tex.infos[i as usize];
    assert_eq!(info.lightmap[1][3].to_bits(), 0x4483_552e);
    assert_eq!(info.lightmap[1][1].to_bits(), (-0.8944f32 * (1.0 / 16.0)).to_bits());
}

#[test]
fn static_prop_leaves_follow_the_hull() {
    use crate::props::{TreeStep, hull_leaves};
    let step = |n: usize| match n {
        0 => TreeStep::Node { normal: DVec3::X, dist: 0.0, children: [4, 2] },
        4 => TreeStep::Node { normal: DVec3::Y, dist: 100.0, children: [3, 1] },
        3 => TreeStep::Leaf { open: false },
        _ => TreeStep::Leaf { open: true },
    };
    let cube = |lo: DVec3, hi: DVec3| -> Vec<DVec3> {
        (0..8)
            .map(|i| {
                DVec3::new(
                    if i & 1 == 0 { lo.x } else { hi.x },
                    if i & 2 == 0 { lo.y } else { hi.y },
                    if i & 4 == 0 { lo.z } else { hi.z },
                )
            })
            .collect()
    };
    assert_eq!(hull_leaves(0, &step, &[cube(DVec3::new(-10.0, 0.0, 0.0), DVec3::new(10.0, 10.0, 10.0))]), vec![2, 1]);
    assert_eq!(hull_leaves(0, &step, &[cube(DVec3::new(-10.0, 0.0, 0.0), DVec3::new(0.0, 10.0, 10.0))]), vec![2]);
    let tri = vec![
        DVec3::new(1.0, 0.0, 0.0),
        DVec3::new(50.0, 0.0, 0.0),
        DVec3::new(1.0, 150.0, 0.0),
        DVec3::new(1.0, 0.0, 5.0),
    ];
    assert_eq!(hull_leaves(0, &step, &[tri]), vec![1]);
    let rod = vec![DVec3::new(2.0, 2.0, 0.0), DVec3::new(30.0, 90.0, 0.0), DVec3::new(2.0, 2.0, 1.0)];
    assert_eq!(hull_leaves(0, &step, &[rod]), vec![1]);
}

#[test]
fn detail_blockers_hold_their_inside() {
    let b = crate::detail_props::DetailBlocker {
        mins: Vec3::ZERO,
        maxs: Vec3::splat(10.0),
        planes: vec![
            (Vec3::X, 10.0),
            (-Vec3::X, 0.0),
            (Vec3::Y, 10.0),
            (-Vec3::Y, 0.0),
            (Vec3::Z, 10.0),
            (-Vec3::Z, 0.0),
            (Vec3::new(1.0, 1.0, 0.0).normalize(), 10.0 / 2f32.sqrt()),
        ],
    };
    assert!(b.contains(Vec3::new(2.0, 2.0, 5.0)));
    assert!(b.contains(Vec3::new(0.0, 0.0, 0.0)), "on the sides counts");
    assert!(!b.contains(Vec3::new(8.0, 8.0, 5.0)), "cut off by the slanted side");
    assert!(!b.contains(Vec3::new(11.0, 0.0, 0.0)));
}

#[test]
fn lumps_are_laid_out_like_vbsp() {
    use crate::emit::lump;
    let mut l = crate::emit::Lumps::new();
    for (i, n) in [
        (lump::PLANES, 20),
        (lump::ENTITIES, 7),
        (lump::LEAFS, 32),
        (lump::PAKFILE, 5),
        (lump::TEXDATA, 32),
        (lump::OCCLUSION, 12),
    ] {
        l.data.insert(i, (vec![1; n], 0));
    }
    let bsp = l.file(1);
    let info = |i: usize| {
        let o = 8 + i * 16;
        (
            u32::from_le_bytes(bsp[o..o + 4].try_into().unwrap()),
            u32::from_le_bytes(bsp[o + 4..o + 8].try_into().unwrap()),
        )
    };
    assert_eq!(info(lump::PLANES), (1036, 20));
    assert_eq!(info(lump::LEAFS), (1056, 32));
    assert_eq!(info(lump::VERTEXES), (1088, 0));
    assert_eq!(info(lump::TEXDATA), (1088, 32));
    let (ent, _) = info(lump::ENTITIES);
    assert_eq!(info(lump::VISIBILITY).0, ent);
    assert!(info(lump::OCCLUSION).0 > ent);
    let (pak, pak_len) = info(lump::PAKFILE);
    assert!(pak > info(lump::GAME_LUMP).0);
    assert_eq!(bsp.len() as u32, (pak + pak_len).div_ceil(4) * 4);
    assert_eq!(info(lump::FACES_HDR), (0, 0), "no HDR faces: not written");
}

#[test]
fn the_entity_lump_is_built_like_vbsp() {
    let text = "versioninfo\n{\n\t\"mapversion\" \"7\"\n}\nworld\n{\n\t\"id\" \"1\"\n\t\"mapversion\" \"7\"\n\t\"classname\" \"worldspawn\"\n\t\"skyname\" \"sky_day01_01\"\n\t\"detailvbsp\" \"detail.vbsp\"\n}\n\
        entity\n{\n\t\"id\" \"10\"\n\t\"classname\" \"light\"\n\t\"targetname\" \"lamp\"\n\t\"style\" \"5\"\n\t\"origin\" \"0 0 64\"\n\tconnections\n\t{\n\t\t\"OnUser1\" \"x,Kill,,0,-1\"\n\t}\n}\n\
        entity\n{\n\t\"id\" \"11\"\n\t\"classname\" \"info_node_hint\"\n\t\"origin\" \"0 0 8\"\n\t\"TargetNode\" \"-1\"\n}\n\
        entity\n{\n\t\"id\" \"12\"\n\t\"classname\" \"info_node_link\"\n\t\"origin\" \"0 0 8\"\n\t\"EndNode\" \"7\"\n}\n\
        entity\n{\n\t\"id\" \"13\"\n\t\"classname\" \"info_overlay\"\n\t\"origin\" \"0 0 8\"\n}\n\
        entity\n{\n\t\"id\" \"14\"\n\t\"classname\" \"info_overlay\"\n\t\"targetname\" \"decal\"\n\t\"origin\" \"0 0 8\"\n}\n\
        entity\n{\n\t\"id\" \"15\"\n\t\"classname\" \"func_ladder\"\n}\n";
    let map = hp_vmf::parse(text).expect("vmf");
    let entities: Vec<&hp_vmf::Entity> = std::iter::once(&map.world).chain(map.entities.iter()).collect();
    let n = entities.len();
    let mut removed = vec![false; n];
    removed[4] = true;
    removed[5] = true;
    let ladders = {
        let mut v = vec![None; n];
        v[6] = Some((DVec3::new(-1.0, 2.5, 0.0), DVec3::new(15.25, 18.0, 128.0)));
        v
    };
    let keys = crate::compile::EntityKeys {
        removed: &removed,
        model_number: &vec![0; n],
        origin_brush: &vec![None; n],
        ladders: &ladders,
        world_box: Some((DVec3::new(-8.0, -8.0, 0.0), DVec3::new(8.5, 8.0, 64.0))),
        map_version: "7".to_owned(),
        format: hp_bsp::BspFormat::default(),
    };
    let ents = crate::compile::entity_list(&entities, &keys).expect("entities");
    let pairs = |i: usize| ents[i].pairs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" ");
    assert_eq!(
        pairs(0),
        "world_maxs=8 8 64 world_mins=-8 -8 0 vbsp_version=Oct 22 2026 vbsp_exe=gmod detailvbsp=detail.vbsp skyname=sky_day01_01 classname=worldspawn mapversion=7 hammerid=1"
    );
    assert_eq!(
        pairs(1),
        "defaultstyle=5 origin=0 0 64 style=32 targetname=lamp classname=light hammerid=10 OnUser1=x,Kill,,0,-1"
    );
    assert_eq!(pairs(2), "TargetNode=1 origin=0 0 8 classname=info_node_hint hammerid=11");
    assert_eq!(pairs(3), "StartNode=2 EndNode=3 origin=0 0 8 classname=info_node_link hammerid=12");
    assert_eq!(pairs(4), "OverlayID=1 origin=0 0 8 targetname=decal classname=info_overlay_accessor hammerid=14");
    assert_eq!(
        pairs(5),
        "maxs.z=128.00 maxs.y=18.00 maxs.x=15.25 mins.z=0.00 mins.y=2.50 mins.x=-1.00 classname=info_ladder hammerid=15"
    );
}
