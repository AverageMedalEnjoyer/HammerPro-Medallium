#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use egui::{Modifiers, Pos2};
use glam::DVec3;
use hp_app::state::AppState;
use hp_app::{Action, Tool, ToolFlow, ViewInfo, ViewKind};
use hp_editor::{Id, ObjRef, SelectOp};
use hp_render::Overlay;

use super::chain::*;
use super::tool::{PathTool, sync_options};
use crate::common::Mouse;
use crate::tests_support::{fgd_state_from, info_2d, info_3d_looking_down};

const PATH_FGD: &str = r#"
@BaseClass = Targetname [ targetname(target_source) : "Name" : : "The name." ]
@BaseClass = KeyFrame [ NextKey(target_destination) : "Next KeyFrame" ]
@PointClass base(Targetname) size(16 16 16) line(255 255 255, targetname, target) = path_corner : "Generic path point"
[
    target(target_destination) : "Next stop target"
    wait(integer) : "Wait here (secs)" : 0
]
@PointClass base(Targetname) size(16 16 16) = path_track : "Track node"
[
    target(target_destination) : "Next Stop Target"
    altpath(target_destination) : "Branch Path"
    speed(float) : "New Train Speed" : 0
]
@PointClass base(Targetname) size(16 16 16) = path_corner_crash : "Crash path"
[
    target(target_destination) : "Next stop target"
]
@KeyFrameClass base(Targetname, KeyFrame) = keyframe_rope : "Rope keyframe" []
@PointClass base(Targetname) line(255 255 255, targetname, LaserTarget) = env_laser : "Laser"
[
    LaserTarget(target_destination) : "Target of Laser"
]
@PointClass base(Targetname) = info_target : "Target" []
@SolidClass base(Targetname) = func_tracktrain : "Train"
[
    target(target_destination) : "First Stop Target"
]
"#;

fn state() -> AppState {
    let mut s = fgd_state_from(PATH_FGD);
    s.settings.entities.path.class = "path_track".to_owned();
    sync_options(&s);
    s
}

fn fgd(s: &AppState) -> Option<&hp_fgd::ResolvedFgd> {
    s.assets.fgd()
}

fn ent(s: &AppState, id: Id) -> &hp_vmf::Entity {
    &s.doc.scene().entity(id).expect("entity").entity
}

fn named(s: &AppState, name: &str) -> Id {
    let ids = s.doc.entities_named(name);
    assert_eq!(ids.len(), 1, "{name}: {ids:?}");
    ids[0]
}

fn next_of(s: &AppState, name: &str) -> Option<String> {
    ent(s, named(s, name)).get("target").map(str::to_owned)
}

fn track(s: &AppState) -> PathClass {
    path_class(fgd(s), "path_track").expect("path_track chains")
}

fn chain(s: &mut AppState, n: usize, closed: bool) -> Vec<(Id, String)> {
    let class = track(s);
    let points: Vec<DVec3> = (0..n).map(|i| DVec3::new(i as f64 * 128.0, 0.0, 8.0)).collect();
    let spec = ChainSpec { class: &class, base: "path", points: &points, closed, keyvalues: &[] };
    create_chain(&mut s.doc, &spec).expect("chain")
}

fn top() -> ViewInfo {
    info_2d(ViewKind::Top, 1.0)
}

fn at(view: &ViewInfo, x: f64, y: f64) -> Pos2 {
    view.world_to_screen(DVec3::new(x, y, 0.0)).expect("screen")
}

fn click(tool: &mut PathTool, s: &mut AppState, view: &ViewInfo, p: Pos2) -> ToolFlow {
    s.frame.index += 1;
    tool.handle_mouse(s, view, &Mouse::click(p))
}

fn drag(tool: &mut PathTool, s: &mut AppState, view: &ViewInfo, from: Pos2, to: Pos2, mods: Modifiers) {
    for m in [
        Mouse::drag_start(from, from).with_modifiers(mods),
        Mouse::drag(from, to).with_modifiers(mods),
        Mouse::drag_end(from, to).with_modifiers(mods),
    ] {
        s.frame.index += 1;
        tool.handle_mouse(s, view, &m);
    }
}

#[test]
fn classes_come_from_the_definitions() {
    let s = state();
    let f = fgd(&s);
    let t = path_class(f, "path_track").unwrap();
    assert_eq!((t.next_key.as_str(), t.alt_key.as_deref()), ("target", Some("altpath")));
    assert_eq!(path_class(f, "PATH_CORNER").unwrap().next_key, "target");
    assert_eq!(path_class(f, "path_corner_crash").unwrap().next_key, "target", "described as the next stop");
    assert_eq!(path_class(f, "keyframe_rope").unwrap().next_key, "NextKey");
    assert!(path_class(f, "env_laser").is_none(), "a line to a target is not a chain");
    assert!(path_class(f, "info_target").is_none());
    assert!(path_class(f, "func_tracktrain").is_none(), "brush entities do not chain");
    assert_eq!(path_class(None, "scripted_sequence").unwrap().next_key, "m_iszNextScript");
    assert_eq!(path_class(None, "path_track").unwrap().alt_key.as_deref(), Some("altpath"));
    assert!(path_class(None, "light").is_none());
    let names: Vec<String> = path_classes(f).into_iter().map(|c| c.class).collect();
    assert_eq!(names[..2], ["path_track".to_owned(), "path_corner".to_owned()]);
    assert!(names.contains(&"keyframe_rope".to_owned()) && !names.contains(&"env_laser".to_owned()));
    assert!(!path_classes(None).is_empty());
}

#[test]
fn names_follow_the_base_and_stay_unique() {
    assert_eq!(name_prefix("path"), "path_");
    assert_eq!(name_prefix("track-"), "track-");
    assert_eq!(name_prefix("  "), "path_");
    assert_eq!(name_prefix("my \"path\",x"), "my__path__x_");
    assert_eq!(split_number("path_12"), Some(("path_", 12)));
    assert_eq!(split_number("path"), None);
    let mut s = state();
    let a = chain(&mut s, 3, false);
    let names: Vec<&str> = a.iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(names, ["path_1", "path_2", "path_3"]);
    let b = chain(&mut s, 2, false);
    assert_eq!(b[0].1, "path_4");
    assert_eq!(chain_names(&s.doc, "other", 2), ["other_1", "other_2"]);
    assert_eq!(name_after(&s.doc, Some("path_2"), "x"), "path_6", "first free number after 2");
}

#[test]
fn create_links_points_and_is_one_undo_step() {
    let mut s = state();
    let before = s.doc.history_len().0;
    let c = chain(&mut s, 4, false);
    assert_eq!(s.doc.history_len().0, before + 1);
    assert_eq!(s.doc.undo_label(), Some(UNDO_CREATE));
    assert_eq!(next_of(&s, "path_1").as_deref(), Some("path_2"));
    assert_eq!(next_of(&s, "path_3").as_deref(), Some("path_4"));
    assert_eq!(next_of(&s, "path_4"), None, "an open chain ends");
    assert_eq!(s.doc.selection().objects.len(), 4, "the new chain is selected");
    let g = PathGraph::build(&s.doc, fgd(&s));
    let chains = g.chains();
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].nodes, c.iter().map(|(id, _)| *id).collect::<Vec<_>>());
    assert!(!chains[0].closed);
    s.doc.undo();
    assert!(s.doc.entities_named("path_1").is_empty(), "undo removes the whole chain");
}

#[test]
fn a_loop_links_the_last_point_to_the_first() {
    let mut s = state();
    let c = chain(&mut s, 3, true);
    assert_eq!(next_of(&s, "path_3").as_deref(), Some("path_1"));
    let g = PathGraph::build(&s.doc, fgd(&s));
    let chains = g.chains();
    assert_eq!(chains.len(), 1);
    assert!(chains[0].closed);
    assert_eq!(chains[0].nodes[0], c[0].0, "a loop starts at its lowest id");
    assert_eq!(g.chain_start(c[2].0), c[2].0, "around a loop the start is the point itself");
    assert_eq!(g.component(&[c[1].0]).len(), 3);
}

#[test]
fn insert_renumbers_the_points_after_and_their_references() {
    let mut s = state();
    let c = chain(&mut s, 4, false);
    let train_kv = [("target".to_owned(), "path_3".to_owned()), ("targetname".to_owned(), "train".to_owned())];
    let sides = s.doc.scene().solid(2).expect("floor").solid.sides.clone();
    let train = s.doc.create_brush_entity("func_tracktrain", &train_kv, vec![sides]).expect("train");
    s.doc
        .add_connection(
            &[train],
            &hp_vmf::Connection {
                output: "OnStart".into(),
                target: "train".into(),
                input: "TeleportToPathTrack".into(),
                param: "path_4".into(),
                ..Default::default()
            },
        )
        .expect("output");
    let before = s.doc.history_len().0;
    let ins = insert_after(&mut s.doc, s.assets.fgd(), c[1].0, None, "path").expect("insert");
    assert_eq!(s.doc.history_len().0, before + 1, "one undo step");
    assert_eq!(s.doc.undo_label(), Some(UNDO_INSERT));
    assert_eq!(ins.name, "path_3");
    assert_eq!(ins.origin, DVec3::new(192.0, 0.0, 8.0), "halfway to the next point");
    assert_eq!(ins.renamed.len(), 2);
    assert_eq!(ent(&s, c[2].0).get("targetname"), Some("path_4"));
    assert_eq!(ent(&s, c[3].0).get("targetname"), Some("path_5"));
    assert_eq!(next_of(&s, "path_2").as_deref(), Some("path_3"));
    assert_eq!(next_of(&s, "path_3").as_deref(), Some("path_4"));
    assert_eq!(next_of(&s, "path_4").as_deref(), Some("path_5"));
    assert_eq!(ent(&s, train).get("target"), Some("path_4"), "the train still starts on the same point");
    assert_eq!(ent(&s, train).connections[0].param, "path_5", "outputs follow the renamed points");
    assert_eq!(s.doc.selected_objects(), vec![ObjRef::Entity(ins.id)]);
    s.doc.undo();
    assert_eq!(ent(&s, c[2].0).get("targetname"), Some("path_3"));
    assert_eq!(next_of(&s, "path_2").as_deref(), Some("path_3"));
    assert_eq!(ent(&s, train).get("target"), Some("path_3"));
}

#[test]
fn insert_without_a_sequence_takes_a_free_name() {
    let mut s = state();
    let c = chain(&mut s, 3, false);
    s.doc.edit_entity(c[1].0, |e| e.set("targetname", "middle")).unwrap();
    s.doc.edit_entity(c[0].0, |e| e.set("target", "middle")).unwrap();
    let ins = insert_after(&mut s.doc, s.assets.fgd(), c[0].0, Some(DVec3::new(64.0, 64.0, 8.0)), "path").unwrap();
    assert_eq!(ins.name, "path_2", "path_2 is free");
    assert!(ins.renamed.is_empty());
    assert_eq!(next_of(&s, "path_1").as_deref(), Some("path_2"));
    assert_eq!(next_of(&s, "path_2").as_deref(), Some("middle"));
    let ins = insert_after(&mut s.doc, s.assets.fgd(), c[2].0, None, "path").unwrap();
    assert_eq!(ins.origin, DVec3::new(384.0, 0.0, 8.0));
    assert_eq!(next_of(&s, "path_3").as_deref(), Some(ins.name.as_str()));
    assert_eq!(ent(&s, ins.id).get("target"), None);
    let light = s.doc.create_point_entity("info_target", DVec3::ZERO, &[]).unwrap();
    assert!(insert_after(&mut s.doc, s.assets.fgd(), light, None, "path").is_err());
}

#[test]
fn insert_in_a_loop_stops_renumbering_at_the_start() {
    let mut s = state();
    let c = chain(&mut s, 4, true);
    let ins = insert_after(&mut s.doc, s.assets.fgd(), c[3].0, None, "path").unwrap();
    assert_eq!(ins.name, "path_5");
    assert!(ins.renamed.is_empty(), "path_1 after path_4 is not in sequence");
    assert_eq!(next_of(&s, "path_4").as_deref(), Some("path_5"));
    assert_eq!(next_of(&s, "path_5").as_deref(), Some("path_1"), "the loop stays closed");
    let mid = insert_after(&mut s.doc, s.assets.fgd(), c[0].0, None, "path").unwrap();
    assert_eq!(mid.name, "path_2");
    assert_eq!(mid.renamed.len(), 4, "path_2 to path_5 shift");
    assert_eq!(next_of(&s, "path_6").as_deref(), Some("path_1"));
    let g = PathGraph::build(&s.doc, s.assets.fgd());
    assert_eq!(g.chains()[0].nodes.len(), 6);
    assert!(g.chains()[0].closed);
}

#[test]
fn remove_relinks_the_neighbours() {
    let mut s = state();
    let c = chain(&mut s, 4, false);
    let before = s.doc.history_len().0;
    let n = remove_points(&mut s.doc, s.assets.fgd(), &[ObjRef::Entity(c[1].0)], true).unwrap();
    assert_eq!(n, 1);
    assert_eq!(s.doc.history_len().0, before + 1);
    assert_eq!(s.doc.undo_label(), Some(UNDO_REMOVE));
    assert_eq!(next_of(&s, "path_1").as_deref(), Some("path_3"));
    remove_points(&mut s.doc, s.assets.fgd(), &[ObjRef::Entity(c[3].0)], true).unwrap();
    assert_eq!(next_of(&s, "path_3"), None);
    remove_points(&mut s.doc, s.assets.fgd(), &[ObjRef::Entity(c[2].0)], false).unwrap();
    assert_eq!(next_of(&s, "path_1").as_deref(), Some("path_3"));
    s.doc.undo();
    s.doc.undo();
    s.doc.undo();
    assert_eq!(next_of(&s, "path_1").as_deref(), Some("path_2"));
}

#[test]
fn remove_in_a_loop_and_adjacent_points() {
    let mut s = state();
    let c = chain(&mut s, 5, true);
    remove_points(&mut s.doc, s.assets.fgd(), &[ObjRef::Entity(c[1].0), ObjRef::Entity(c[2].0)], true).unwrap();
    assert_eq!(next_of(&s, "path_1").as_deref(), Some("path_4"));
    remove_points(&mut s.doc, s.assets.fgd(), &[ObjRef::Entity(c[0].0)], true).unwrap();
    assert_eq!(next_of(&s, "path_5").as_deref(), Some("path_4"));
    remove_points(&mut s.doc, s.assets.fgd(), &[ObjRef::Entity(c[3].0)], true).unwrap();
    assert_eq!(next_of(&s, "path_5"), None);
}

#[test]
fn clicking_draws_a_chain_and_enter_ends_it() {
    let mut s = state();
    let mut tool = PathTool::new();
    let view = top();
    for (x, y) in [(0.0, 0.0), (128.0, 0.0), (128.0, 128.0)] {
        assert_eq!(click(&mut tool, &mut s, &view, at(&view, x + 3.0, y - 2.0)), ToolFlow::Handled);
    }
    assert_eq!(tool.drawing().len(), 3);
    assert_eq!(ent(&s, named(&s, "path_3")).origin(), Some(DVec3::new(128.0, 128.0, 0.0)), "snapped to the grid");
    assert_eq!(ent(&s, named(&s, "path_1")).classname, "path_track");
    assert_eq!(next_of(&s, "path_1").as_deref(), Some("path_2"));
    assert_eq!(next_of(&s, "path_2").as_deref(), Some("path_3"));
    assert_eq!(s.doc.undo_label(), Some(UNDO_ADD), "one undo step per point");
    assert_eq!(s.doc.selected_objects(), vec![ObjRef::Entity(named(&s, "path_3"))]);
    s.doc.undo();
    assert_eq!(next_of(&s, "path_2"), None);
    click(&mut tool, &mut s, &view, at(&view, 256.0, 0.0));
    assert_eq!(next_of(&s, "path_2").as_deref(), Some("path_3"));
    assert!(tool.end(&mut s));
    assert!(tool.drawing().is_empty());
    assert_eq!(next_of(&s, "path_3"), None, "no loop without the option");
    click(&mut tool, &mut s, &view, at(&view, -128.0, -128.0));
    assert_eq!(tool.drawing().len(), 1);
    assert!(s.doc.entities_named("path_4").len() == 1);
    assert_eq!(next_of(&s, "path_3"), None);
}

#[test]
fn the_loop_option_and_clicking_the_first_point_close_the_chain() {
    let mut s = state();
    s.settings.entities.path.close_loop = true;
    let mut tool = PathTool::new();
    let view = top();
    for (x, y) in [(0.0, 0.0), (128.0, 0.0), (128.0, 128.0)] {
        click(&mut tool, &mut s, &view, at(&view, x, y));
    }
    assert!(tool.end(&mut s));
    assert_eq!(next_of(&s, "path_3").as_deref(), Some("path_1"));
    assert_eq!(s.doc.undo_label(), Some(UNDO_CLOSE));
    s.settings.entities.path.close_loop = false;
    s.settings.entities.path.base_name = "loop".into();
    for (x, y) in [(0.0, -256.0), (128.0, -256.0), (64.0, -128.0)] {
        click(&mut tool, &mut s, &view, at(&view, x, y));
    }
    click(&mut tool, &mut s, &view, at(&view, 0.0, -256.0));
    assert!(tool.drawing().is_empty(), "linking to a point ends the path");
    assert_eq!(ent(&s, named(&s, "loop_3")).get("target"), Some("loop_1"));
    click(&mut tool, &mut s, &view, at(&view, 0.0, 256.0));
    s.frame.index += 1;
    tool.handle_mouse(&mut s, &view, &Mouse::double_click(at(&view, 0.0, 256.0)));
    assert!(tool.drawing().is_empty());
    assert_eq!(s.doc.entities_named("loop_5").len(), 0, "no second point at the same place");
}

#[test]
fn keys_end_the_path() {
    let mut s = state();
    let mut tool = PathTool::new();
    let view = top();
    click(&mut tool, &mut s, &view, at(&view, 0.0, 0.0));
    click(&mut tool, &mut s, &view, at(&view, 64.0, 0.0));
    let ctx = egui::Context::default();
    let key = |k: egui::Key| egui::RawInput {
        events: vec![egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        ..Default::default()
    };
    let mut handled = false;
    let _ = ctx.run_ui(key(egui::Key::Escape), |_| {
        handled = tool.key_input(&mut s, &ctx);
    });
    assert!(handled);
    assert!(tool.drawing().is_empty());
    let mut again = true;
    let _ = ctx.run_ui(key(egui::Key::Enter), |_| {
        again = tool.key_input(&mut s, &ctx);
    });
    assert!(!again, "nothing to end: Enter goes elsewhere");
}

#[test]
fn shift_drag_inserts_and_drag_moves() {
    let mut s = state();
    let c = chain(&mut s, 3, false);
    let mut tool = PathTool::new();
    let view = top();
    let before = s.doc.history_len().0;
    drag(&mut tool, &mut s, &view, at(&view, 0.0, 0.0), at(&view, 64.0, 96.0), Modifiers::SHIFT);
    assert_eq!(s.doc.history_len().0, before + 1);
    assert_eq!(s.doc.undo_label(), Some(UNDO_INSERT));
    let new = named(&s, "path_2");
    assert_eq!(ent(&s, new).origin(), Some(DVec3::new(64.0, 96.0, 8.0)), "depth of the point it comes from");
    assert_eq!(ent(&s, c[1].0).get("targetname"), Some("path_3"));
    assert_eq!(next_of(&s, "path_1").as_deref(), Some("path_2"));
    assert!(tool.drawing().is_empty(), "inserting in the middle draws nothing");
    drag(&mut tool, &mut s, &view, at(&view, 256.0, 0.0), at(&view, 384.0, 0.0), Modifiers::SHIFT);
    assert_eq!(next_of(&s, "path_4").as_deref(), Some("path_5"));
    assert_eq!(tool.drawing().len(), 5);
    click(&mut tool, &mut s, &view, at(&view, 384.0, 128.0));
    assert_eq!(next_of(&s, "path_5").as_deref(), Some("path_6"));
    tool.end(&mut s);
    let before = s.doc.history_len().0;
    drag(&mut tool, &mut s, &view, at(&view, 64.0, 96.0), at(&view, 64.0, 160.0), Modifiers::NONE);
    assert_eq!(s.doc.history_len().0, before + 1);
    assert_eq!(s.doc.undo_label(), Some("Move"));
    assert_eq!(ent(&s, new).origin(), Some(DVec3::new(64.0, 160.0, 8.0)));
    assert!(!s.doc.in_transaction());
    s.frame.index += 1;
    let from = at(&view, 64.0, 160.0);
    tool.handle_mouse(&mut s, &view, &Mouse::drag_start(from, from));
    s.frame.index += 1;
    tool.handle_mouse(&mut s, &view, &Mouse::drag(from, at(&view, 0.0, 256.0)));
    assert_eq!(ent(&s, new).origin(), Some(DVec3::new(0.0, 256.0, 8.0)));
    assert!(tool.cancel(&mut s));
    assert_eq!(ent(&s, new).origin(), Some(DVec3::new(64.0, 160.0, 8.0)));
    assert!(!s.doc.in_transaction());
}

#[test]
fn clicking_a_point_selects_it_and_delete_relinks() {
    let mut s = state();
    let c = chain(&mut s, 3, false);
    let mut tool = PathTool::new();
    let view = top();
    click(&mut tool, &mut s, &view, at(&view, 128.0, 0.0));
    assert_eq!(s.doc.selected_objects(), vec![ObjRef::Entity(c[1].0)]);
    assert!(tool.drawing().is_empty(), "selecting does not start a path");
    assert!(tool.action(&mut s, Action::Delete), "the tool deletes with relinking");
    assert_eq!(next_of(&s, "path_1").as_deref(), Some("path_3"));
    s.settings.entities.path.relink_on_delete = false;
    s.doc.select(&[ObjRef::Entity(c[2].0)], SelectOp::Replace);
    assert!(!tool.action(&mut s, Action::Delete));
    assert!(!tool.action(&mut s, Action::Undo));
}

#[test]
fn points_rest_on_surfaces_in_3d() {
    let mut s = state();
    s.settings.entities.path.surface_offset = 16.0;
    let mut tool = PathTool::new();
    let down = info_3d_looking_down(DVec3::new(0.0, 0.0, 600.0));
    let p = down.world_to_screen(DVec3::new(32.0, 32.0, 0.0)).expect("screen");
    s.frame.index += 1;
    tool.handle_mouse(&mut s, &down, &Mouse::click(p));
    let o = ent(&s, named(&s, "path_1")).origin().unwrap();
    assert_eq!(o.z, 24.0, "8 for the half box over the floor, 16 of height");
    assert!((o.x - 32.0).abs() <= 8.0 && (o.y - 32.0).abs() <= 8.0, "{o:?}");
}

#[test]
fn every_chain_is_drawn_with_arrows() {
    let mut s = state();
    chain(&mut s, 3, false);
    let other = chain(&mut s, 2, true);
    s.doc.select(&[ObjRef::Entity(other[0].0)], SelectOp::Replace);
    let tool = PathTool::new();
    let view = top();
    let mut out = Overlay::default();
    tool.overlay(&s, &view, &mut out);
    assert_eq!(out.points.len(), 5, "one marker per point");
    assert_eq!(out.lines_on_top.len(), 4 * 3);
    let big = out.points.iter().filter(|p| p.size_px > 7.0).count();
    assert_eq!(big, 2, "the selected chain is highlighted");
    let mut out3 = Overlay::default();
    tool.overlay(&s, &info_3d_looking_down(DVec3::new(0.0, 0.0, 600.0)), &mut out3);
    assert_eq!(out3.lines.len(), 2 + 4 + 2, "first chain lines and arrows, loop lines");
    assert_eq!(out3.lines_on_top.len(), 2 + 4, "faint loop lines and loop arrows");
    let first = named(&s, "path_1");
    s.doc.edit_entity(first, |e| e.set("altpath", "path_3")).unwrap();
    let mut out = Overlay::default();
    tool.overlay(&s, &view, &mut out);
    assert_eq!(out.lines_on_top.len(), 5 * 3);
}

#[test]
fn options_are_saved_in_the_settings() {
    let mut s = state();
    let mut tool = PathTool::new();
    assert!(tool.set_option(&mut s, "class", "path_corner"));
    assert!(tool.set_option(&mut s, "base_name", "track"));
    assert!(tool.set_option(&mut s, "loop", "on"));
    assert!(tool.set_option(&mut s, "offset", "8"));
    assert!(!tool.set_option(&mut s, "loop", "maybe"));
    assert!(!tool.set_option(&mut s, "offset", "nan"));
    assert!(!tool.set_option(&mut s, "colour", "red"));
    let p = &s.settings.entities.path;
    assert_eq!(
        (p.class.as_str(), p.base_name.as_str(), p.close_loop, p.surface_offset),
        ("path_corner", "track", true, 8.0)
    );
    assert!(s.settings_dirty);
    let values = tool.option_values();
    let get = |k: &str| values.iter().find(|o| o.key == k).map(|o| o.value.clone()).unwrap();
    assert_eq!(
        (get("class"), get("base_name"), get("loop"), get("offset")),
        ("path_corner".into(), "track".into(), "on".into(), "8".into())
    );
    hp_app::testing::run_ui(&mut s, |ui, st| tool.options_ui(ui, st));
}

#[test]
fn the_tool_hint_and_labels() {
    let mut s = state();
    let mut tool = PathTool::new();
    let view = top();
    assert!(tool.status_hint(&s).is_some());
    s.frame.index += 1;
    tool.handle_mouse(&mut s, &view, &Mouse::hover(at(&view, 0.0, 0.0)));
    click(&mut tool, &mut s, &view, at(&view, 0.0, 0.0));
    s.frame.index += 1;
    tool.handle_mouse(&mut s, &view, &Mouse::hover(at(&view, 64.0, 0.0)));
    let mut out = Overlay::default();
    tool.overlay(&s, &view, &mut out);
    assert!(out.points.iter().any(|p| p.pos == [64.0, 0.0, 0.0]), "the point the next click adds");
    assert!(tool.status_hint(&s).unwrap().contains('1'));
    let entries = tool.context_entries(&s);
    assert_eq!(entries.len(), 4);
    assert!(entries[2].enabled && !entries[3].enabled);
}

#[test]
fn strings_are_translated_and_used() {
    use super::tr::PATH_TR;
    assert!(hp_app::i18n::lint_table(PATH_TR).is_empty());
    let sources = [include_str!("tool.rs"), include_str!("chain.rs"), include_str!("mod.rs"), include_str!("tr.rs")];
    for src in sources {
        assert!(!src.contains('\u{2014}') && !src.contains('\u{2013}'), "no em or en dash");
    }
    let tool = sources[0];
    for (k, en, fr) in PATH_TR {
        assert!(k.starts_with("path."), "{k}");
        assert!(tool.contains(&format!("\"{k}\"")), "unused key {k}");
        let count = |s: &str| (0..4).filter(|i| s.contains(&format!("{{{i}}}"))).count();
        assert_eq!(count(en), count(fr), "{k}: placeholders differ");
    }
    let mut checked = 0;
    for prefix in ["t!(\"", "tf!(\""] {
        let mut rest = tool;
        while let Some(i) = rest.find(prefix) {
            let tail = &rest[i + prefix.len()..];
            let end = tail.find('"').unwrap_or(0);
            let key = &tail[..end];
            assert!(PATH_TR.iter().any(|(k, _, _)| *k == key), "missing translation {key}");
            checked += 1;
            rest = &tail[end..];
        }
    }
    assert!(checked > 20, "{checked}");
}
