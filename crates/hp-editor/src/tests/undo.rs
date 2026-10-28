use std::time::Duration;

use glam::DVec3;

use super::{cube, doc_with_cube, vmf};
use crate::{Document, EditError, ObjRef, Owner, SelectOp, UndoLimits};

#[test]
fn single_operation_is_one_step() {
    let (mut doc, id) = doc_with_cube();
    assert!(!doc.can_undo());
    let before = vmf(&doc);
    doc.translate(&[ObjRef::Solid(id)], DVec3::new(8.0, 0.0, 0.0)).expect("move");
    assert!(doc.can_undo());
    assert_eq!(doc.undo_label(), Some("Move"));
    assert!(doc.undo());
    assert_eq!(vmf(&doc), before);
    assert_eq!(doc.redo_label(), Some("Move"));
    assert!(!doc.undo());
    assert!(doc.redo());
    assert!(!doc.redo());
}

#[test]
fn transaction_groups_operations() {
    let (mut doc, id) = doc_with_cube();
    let before = vmf(&doc);
    doc.begin("Complex edit");
    doc.translate(&[ObjRef::Solid(id)], DVec3::X * 16.0).expect("move");
    let e = doc.create_point_entity("info_target", DVec3::new(0.0, 0.0, 128.0), &[]).expect("entity");
    doc.begin("Inner");
    doc.set_entity_key(&[e], "targetname", "t1").expect("key");
    assert!(!doc.commit());
    assert!(doc.in_transaction());
    assert!(!doc.can_undo());
    assert!(!doc.undo(), "undo is refused while a transaction is open");
    assert!(doc.commit());
    assert_eq!(doc.history_len(), (1, 0));
    assert_eq!(doc.undo_label(), Some("Complex edit"));
    let after = vmf(&doc);
    assert!(doc.undo());
    assert_eq!(vmf(&doc), before);
    assert!(doc.redo());
    assert_eq!(vmf(&doc), after);
}

#[test]
fn cancel_reverts_everything() {
    let (mut doc, id) = doc_with_cube();
    let before = vmf(&doc);
    let changes_before = doc.history_len();
    doc.begin("Drag");
    for _ in 0..10 {
        doc.translate(&[ObjRef::Solid(id)], DVec3::Y).expect("move");
    }
    doc.delete(&[ObjRef::Solid(id)]).expect("delete");
    doc.cancel();
    assert!(!doc.in_transaction());
    assert_eq!(vmf(&doc), before);
    assert_eq!(doc.history_len(), changes_before);
    assert!(doc.scene().solid(id).is_some());
}

#[test]
fn closure_helper_commits_or_cancels() {
    let (mut doc, id) = doc_with_cube();
    let before = vmf(&doc);
    let r: Result<(), EditError> = doc.transaction("Failing", |d| {
        d.translate(&[ObjRef::Solid(id)], DVec3::Z)?;
        Err(EditError::Invalid("stop".into()))
    });
    assert!(r.is_err());
    assert_eq!(vmf(&doc), before);
    assert!(!doc.can_undo());
    let r: Result<u32, EditError> = doc.transaction("Working", |d| {
        d.translate(&[ObjRef::Solid(id)], DVec3::Z)?;
        Ok(5)
    });
    assert_eq!(r.ok(), Some(5));
    assert_eq!(doc.undo_label(), Some("Working"));
}

#[test]
fn empty_transactions_record_nothing() {
    let (mut doc, id) = doc_with_cube();
    doc.begin("Nothing");
    assert!(!doc.commit());
    doc.begin("Back and forth");
    doc.translate(&[ObjRef::Solid(id)], DVec3::X).expect("move");
    doc.translate(&[ObjRef::Solid(id)], -DVec3::X).expect("move");
    assert!(!doc.commit());
    doc.begin("Select");
    doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
    assert!(!doc.commit());
    assert!(!doc.can_undo());
    assert!(doc.create_solid(Vec::new(), Owner::World).is_err());
    assert!(!doc.can_undo());
}

#[test]
fn undo_restores_selection() {
    let (mut doc, id) = doc_with_cube();
    doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
    doc.delete_selection().expect("delete");
    assert!(doc.selection().is_empty());
    assert!(doc.undo());
    assert!(doc.selection().contains(ObjRef::Solid(id)));
    assert!(doc.redo());
    assert!(doc.selection().is_empty());
}

#[test]
fn nudges_merge() {
    let (mut doc, id) = doc_with_cube();
    let mut limits = doc.undo_limits();
    limits.merge_window = None;
    doc.set_undo_limits(limits);
    let before = vmf(&doc);
    for _ in 0..5 {
        doc.nudge(&[ObjRef::Solid(id)], DVec3::X).expect("nudge");
    }
    assert_eq!(doc.history_len(), (1, 0));
    assert_eq!(bounds_min_x(&doc, id), 5.0);
    assert!(doc.undo());
    assert_eq!(vmf(&doc), before);
    assert!(doc.redo());
    doc.translate(&[ObjRef::Solid(id)], DVec3::Y).expect("move");
    doc.nudge(&[ObjRef::Solid(id)], DVec3::X).expect("nudge");
    assert_eq!(doc.history_len(), (3, 0));
    doc.nudge(&[ObjRef::Solid(id)], -DVec3::X).expect("nudge");
    assert_eq!(doc.history_len(), (2, 0));
}

#[test]
fn merge_window_is_respected() {
    let (mut doc, id) = doc_with_cube();
    let mut limits = doc.undo_limits();
    limits.merge_window = Some(Duration::ZERO);
    doc.set_undo_limits(limits);
    doc.nudge(&[ObjRef::Solid(id)], DVec3::X).expect("nudge");
    std::thread::sleep(Duration::from_millis(5));
    doc.nudge(&[ObjRef::Solid(id)], DVec3::X).expect("nudge");
    assert_eq!(doc.history_len(), (2, 0));
}

fn bounds_min_x(doc: &Document, id: crate::Id) -> f64 {
    doc.scene().solid(id).expect("solid").bounds.min.x
}

#[test]
fn history_limits() {
    let (mut doc, id) = doc_with_cube();
    doc.set_undo_limits(UndoLimits { max_steps: 3, ..UndoLimits::default() });
    for i in 0..10 {
        doc.translate(&[ObjRef::Solid(id)], DVec3::X * f64::from(i + 1)).expect("move");
    }
    assert_eq!(doc.history_len(), (3, 0));
    while doc.undo() {}
    assert_eq!(bounds_min_x(&doc, id), (1..=7).sum::<i32>() as f64);
    assert!(doc.is_dirty());
    doc.set_undo_limits(UndoLimits { max_steps: 100, max_bytes: 1, merge_window: None });
    for _ in 0..5 {
        doc.translate(&[ObjRef::Solid(id)], DVec3::Y).expect("move");
    }
    assert_eq!(doc.history_len(), (1, 0));
    assert!(doc.history_bytes() > 0);
}

#[test]
fn dirty_flag_follows_history() {
    let mut doc = Document::new_empty();
    assert!(!doc.is_dirty());
    let id = doc.create_solid(cube(DVec3::ZERO, 32.0), Owner::World).expect("create");
    assert!(doc.is_dirty());
    assert!(doc.undo());
    assert!(!doc.is_dirty(), "undo back to the loaded state is clean");
    assert!(doc.redo());
    assert!(doc.is_dirty());
    doc.clear_history();
    assert!(doc.is_dirty(), "clearing history keeps unsaved changes dirty");
    assert!(doc.scene().solid(id).is_some());
    let mut doc = Document::new_empty();
    doc.mark_dirty();
    assert!(doc.is_dirty());
}

#[test]
fn world_settings_are_undoable() {
    let mut doc = Document::new_empty();
    let before = vmf(&doc);
    doc.set_world_key("skyname", "sky_wasteland02").expect("sky");
    assert_eq!(doc.scene().world().get("skyname"), Some("sky_wasteland02"));
    assert!(doc.set_world_key("classname", "x").is_err());
    assert!(doc.set_world_key("mapversion", "3").is_err());
    let removed = doc.remove_world_key("detailvbsp").expect("remove");
    assert_eq!(removed.as_deref(), Some("detail.vbsp"));
    let mut cordons = doc.scene().cordons().clone();
    cordons.active = true;
    doc.set_cordons(cordons).expect("cordons");
    assert!(doc.undo() && doc.undo() && doc.undo());
    assert_eq!(vmf(&doc), before);
    let mut cams = doc.scene().cameras().clone();
    cams.active = 0;
    cams.cameras.push(hp_vmf::Camera { position: DVec3::ONE, look: DVec3::ZERO });
    doc.set_cameras(cams);
    doc.set_view_setting("nGridSpacing", "16");
    assert_eq!(doc.scene().view_settings().get("nGridSpacing"), Some("16"));
    assert!(!doc.is_dirty());
}

#[test]
fn random_operation_sequences_undo_exactly() {
    use crate::{ClipKeep, FaceRef, SelectMode};
    let mut seed = 0xC0FF_EE11_u64;
    let mut rnd = move |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n.max(1) as u64) as usize
    };
    let mut maps = vec![hp_vmf::parse(&hp_vmf::write(&super::roundtrip::feature_map())).expect("parse"); 4];
    for path in super::sample_vmfs() {
        if let Ok(m) = hp_vmf::load(&path) {
            maps.push(m);
        }
    }
    for (round, map) in maps.into_iter().enumerate() {
        let mut doc = Document::from_map(map);
        let mut states = vec![vmf(&doc)];
        for step in 0..40 {
            let units: Vec<ObjRef> = doc.scene().units().collect();
            let solids: Vec<crate::Id> = doc.scene().solids().keys().copied().collect();
            let pick = |r: &mut dyn FnMut(usize) -> usize, v: &[ObjRef]| -> Vec<ObjRef> {
                let k = 1 + r(v.len().min(3));
                (0..k).filter_map(|_| v.get(r(v.len())).copied()).collect()
            };
            let objs = if units.is_empty() { Vec::new() } else { pick(&mut rnd, &units) };
            let op = rnd(14);
            let _ = match op {
                0 => doc.translate(&objs, DVec3::new(16.0, -8.0, 4.0)).map(|_| ()),
                1 => doc.rotate_axis(&objs, DVec3::ZERO, DVec3::Z, 90.0).map(|_| ()),
                2 => doc.delete(&objs).map(|_| ()),
                3 => doc.duplicate(&objs, DVec3::new(0.0, 0.0, 64.0)).map(|_| ()),
                4 => {
                    let plane = hp_geom::Plane::from_normal_point(DVec3::X, DVec3::new(16.0, 0.0, 0.0)).expect("plane");
                    doc.clip(&solids, &plane, ClipKeep::Both).map(|_| ())
                }
                5 => doc
                    .create_solid(cube(DVec3::new(rnd(10) as f64 * 40.0, 300.0, 0.0), 24.0), Owner::World)
                    .map(|_| ()),
                6 => doc.group(&objs).map(|_| ()),
                7 => doc.ungroup(&objs).map(|_| ()),
                8 => {
                    let ents: Vec<crate::Id> = doc.scene().entities().keys().copied().collect();
                    doc.set_entity_key(&ents, "targetname", &format!("n{step}")).map(|_| ())
                }
                9 => {
                    doc.set_hidden(&objs, rnd(2) == 0);
                    Ok(())
                }
                10 => {
                    let faces: Vec<FaceRef> = solids.iter().take(3).map(|s| FaceRef::new(*s, rnd(6))).collect();
                    doc.apply_material(&faces, &format!("MAT/M{step}")).map(|_| ())
                }
                11 => doc.create_visgroup(&format!("V{step}"), None, &objs).map(|_| ()),
                12 => doc.tie_to_entity(&objs, "func_detail", &[]).map(|_| ()),
                _ => {
                    let first = objs.first().copied();
                    if let Some(o) = first {
                        doc.select_picked(o, SelectMode::Groups, SelectOp::Replace);
                    }
                    doc.move_to_world(&objs).map(|_| ())
                }
            };
            let now = vmf(&doc);
            if doc.history_len().0 + 1 > states.len() {
                states.push(now);
            } else {
                assert_eq!(states.last(), Some(&now), "round {round} step {step} op {op} changed without an undo step");
            }
        }
        assert!(states.len() > 20, "round {round}: only {} steps recorded", states.len());
        for (i, expected) in states.iter().enumerate().rev().skip(1) {
            assert!(doc.undo(), "undo {i}");
            assert_eq!(&vmf(&doc), expected, "round {round}: undo to state {i}");
        }
        assert!(!doc.undo());
        for (i, expected) in states.iter().enumerate().skip(1) {
            assert!(doc.redo(), "redo {i}");
            assert_eq!(&vmf(&doc), expected, "round {round}: redo to state {i}");
        }
    }
}
