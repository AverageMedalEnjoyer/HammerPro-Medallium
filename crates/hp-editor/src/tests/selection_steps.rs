use glam::DVec3;

use super::cube;
use crate::{Document, FACE_SELECTION_STEP, FaceRef, ObjRef, Owner, SELECTION_STEP, SelectOp, UndoLimits};

fn doc3() -> (Document, [ObjRef; 3]) {
    let mut doc = Document::new_empty();
    let mut ids = [ObjRef::Solid(0); 3];
    for (i, o) in ids.iter_mut().enumerate() {
        let id = doc.create_solid(cube(DVec3::new(i as f64 * 128.0, 0.0, 0.0), 64.0), Owner::World).expect("cube");
        *o = ObjRef::Solid(id);
    }
    doc.clear_selection();
    doc.clear_history();
    doc.set_record_selection(true);
    (doc, ids)
}

fn gesture(doc: &mut Document, f: impl FnOnce(&mut Document)) {
    f(doc);
    doc.end_selection_burst();
}

#[test]
fn off_by_default_and_off_keeps_the_old_behaviour() {
    let (mut doc, [a, ..]) = doc3();
    doc.set_record_selection(false);
    assert!(!doc.records_selection());
    gesture(&mut doc, |d| d.select(&[a], SelectOp::Replace));
    gesture(&mut doc, |d| d.clear_selection());
    assert!(!doc.can_undo());
    assert!(!Document::new_empty().records_selection());
}

#[test]
fn undo_brings_back_the_previous_selection_and_redo_reapplies_it() {
    let (mut doc, [a, b, _]) = doc3();
    gesture(&mut doc, |d| d.select(&[a], SelectOp::Replace));
    gesture(&mut doc, |d| d.select(&[b], SelectOp::Add));
    gesture(&mut doc, |d| d.clear_selection());
    assert_eq!(doc.undo_labels(), [SELECTION_STEP; 3]);
    assert!(doc.undo());
    assert_eq!(doc.selected_objects(), [a, b], "order kept");
    assert!(doc.take_selection_restored());
    assert!(!doc.take_selection_restored(), "reported once");
    assert!(doc.undo());
    assert_eq!(doc.selected_objects(), [a]);
    assert!(doc.undo());
    assert!(doc.selection().is_empty());
    assert!(!doc.can_undo());
    assert!(doc.redo());
    assert_eq!(doc.selected_objects(), [a]);
    assert!(doc.redo() && doc.redo());
    assert!(doc.selection().is_empty());
    assert!(!doc.can_redo());
}

#[test]
fn faces_come_back_after_they_were_deselected() {
    let (mut doc, [a, _, c]) = doc3();
    let (fa, fc) = (FaceRef::new(a.id(), 0), FaceRef::new(c.id(), 2));
    gesture(&mut doc, |d| d.select_faces(&[fa], SelectOp::Replace));
    gesture(&mut doc, |d| d.select_faces(&[fc], SelectOp::Add));
    gesture(&mut doc, |d| d.clear_face_selection());
    assert_eq!(doc.undo_labels(), [FACE_SELECTION_STEP; 3]);
    assert!(doc.undo());
    assert_eq!(doc.selected_faces(), [fa, fc], "the last face stays the active one");
    assert!(doc.undo());
    assert_eq!(doc.selected_faces(), [fa]);
    assert!(doc.redo());
    assert_eq!(doc.selected_faces(), [fa, fc]);
    gesture(&mut doc, |d| d.select(&[a], SelectOp::Replace));
    assert_eq!(doc.undo_label(), Some(SELECTION_STEP));
    assert!(!doc.can_redo(), "a new selection drops the redo steps, like an edit");
}

#[test]
fn one_gesture_is_one_step_and_changes_that_cancel_out_are_none() {
    let (mut doc, [a, b, c]) = doc3();
    gesture(&mut doc, |d| {
        d.select(&[a], SelectOp::Replace);
        d.select(&[b], SelectOp::Add);
        d.select(&[c], SelectOp::Add);
    });
    assert_eq!(doc.history_len(), (1, 0));
    gesture(&mut doc, |d| {
        d.clear_selection();
        d.select(&[a, b, c], SelectOp::Replace);
    });
    assert_eq!(doc.history_len(), (1, 0), "back where it started: no step");
    assert!(doc.undo());
    assert!(doc.selection().is_empty());
    doc.redo();
    doc.select(&[a], SelectOp::Replace);
    doc.select(&[b], SelectOp::Replace);
    assert_eq!(doc.history_len(), (2, 0));
    assert!(doc.undo());
    assert_eq!(doc.selected_objects(), [a, b, c]);
}

#[test]
fn edits_and_selection_steps_interleave() {
    let (mut doc, [a, b, _]) = doc3();
    gesture(&mut doc, |d| d.select(&[a], SelectOp::Replace));
    gesture(&mut doc, |d| {
        d.translate(&[a], DVec3::X * 16.0).expect("move");
    });
    gesture(&mut doc, |d| d.select(&[b], SelectOp::Replace));
    assert_eq!(doc.undo_labels(), [SELECTION_STEP, "Move", SELECTION_STEP]);
    let x = |d: &Document| d.object_bounds(a, &super::no_bounds).expect("bounds").min.x;
    assert!(doc.undo());
    assert_eq!(doc.selected_objects(), [a], "the selection of the move");
    assert_eq!(x(&doc), 16.0);
    assert!(doc.undo());
    assert_eq!(x(&doc), 0.0);
    assert_eq!(doc.selected_objects(), [a], "undoing the edit keeps the selection it had");
    assert!(doc.undo());
    assert!(doc.selection().is_empty());
    assert!(doc.redo() && doc.redo() && doc.redo());
    assert_eq!((x(&doc), doc.selected_objects()), (16.0, vec![b]));
}

#[test]
fn selecting_what_an_edit_made_joins_its_step() {
    let (mut doc, [a, ..]) = doc3();
    gesture(&mut doc, |d| d.select(&[a], SelectOp::Replace));
    gesture(&mut doc, |d| {
        let copy = d.create_solid(cube(DVec3::new(0.0, 256.0, 0.0), 32.0), Owner::World).expect("cube");
        d.select(&[ObjRef::Solid(copy)], SelectOp::Replace);
    });
    assert_eq!(doc.history_len(), (2, 0));
    assert!(doc.undo());
    assert_eq!(doc.selected_objects(), [a], "one undo removes the copy and gives the selection back");
    assert_eq!(doc.scene().solids().len(), 3);
    gesture(&mut doc, |d| {
        d.begin("Pick");
        d.clear_selection();
        d.commit();
    });
    assert_eq!(doc.undo_label(), Some(SELECTION_STEP));
    assert!(doc.undo());
    assert_eq!(doc.selected_objects(), [a]);
}

#[test]
fn deleted_objects_are_left_out() {
    let (mut doc, [a, b, _]) = doc3();
    gesture(&mut doc, |d| d.select(&[a, b], SelectOp::Replace));
    gesture(&mut doc, |d| d.clear_selection());
    gesture(&mut doc, |d| {
        d.delete(&[b]).expect("delete");
    });
    assert!(doc.undo() && doc.undo());
    assert_eq!(doc.selected_objects(), [a, b]);
    let (mut doc, [a, b, _]) = doc3();
    let faces = [FaceRef::new(a.id(), 1), FaceRef::new(b.id(), 1)];
    gesture(&mut doc, |d| {
        d.select(&[a, b], SelectOp::Replace);
        d.select_faces(&faces, SelectOp::Replace);
    });
    gesture(&mut doc, |d| d.clear_selection());
    doc.write_solids(vec![(b.id(), None)]);
    assert!(doc.undo());
    assert_eq!(doc.selected_objects(), [a]);
    assert_eq!(doc.selected_faces(), [faces[0]]);
    gesture(&mut doc, |d| d.select_faces(&faces, SelectOp::Replace));
    assert_eq!(doc.selected_faces(), [faces[0]], "unknown faces are not selected");
}

#[test]
fn selection_steps_never_make_the_map_modified() {
    let dir = std::env::temp_dir().join(format!("hp_selundo_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("sel.vmf");
    let (mut doc, [a, b, _]) = doc3();
    doc.save(&path).expect("save");
    assert!(!doc.is_dirty());
    gesture(&mut doc, |d| d.select(&[a], SelectOp::Replace));
    gesture(&mut doc, |d| d.select_faces(&[FaceRef::new(b.id(), 0)], SelectOp::Replace));
    assert!(!doc.is_dirty());
    assert!(doc.undo());
    assert!(!doc.is_dirty());
    assert!(doc.redo());
    assert!(!doc.is_dirty());
    gesture(&mut doc, |d| {
        d.translate(&[a], DVec3::Y).expect("move");
    });
    assert!(doc.is_dirty());
    gesture(&mut doc, |d| d.clear_selection());
    assert!(doc.is_dirty());
    assert!(doc.undo() && doc.undo());
    assert!(!doc.is_dirty());
    gesture(&mut doc, |d| d.select(&[b], SelectOp::Replace));
    assert!(!doc.is_dirty(), "a selection after undoing an edit is still the saved map");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn undo_passes_over_steps_that_change_nothing() {
    let (mut doc, [a, b, _]) = doc3();
    gesture(&mut doc, |d| d.select(&[a], SelectOp::Replace));
    gesture(&mut doc, |d| d.select(&[b], SelectOp::Replace));
    doc.set_record_selection(false);
    doc.select(&[a], SelectOp::Replace);
    doc.set_record_selection(true);
    assert!(doc.undo());
    assert!(doc.selection().is_empty());
    assert_eq!(doc.history_len(), (0, 2));
}

#[test]
fn selection_steps_are_small_and_dropped_first() {
    let (mut doc, [a, b, c]) = doc3();
    doc.set_undo_limits(UndoLimits { max_steps: 4, ..doc.undo_limits() });
    gesture(&mut doc, |d| {
        d.translate(&[a], DVec3::X).expect("move");
    });
    for o in [a, b, c, a, b, c] {
        gesture(&mut doc, |d| d.select(&[o], SelectOp::Replace));
    }
    assert_eq!(doc.undo_labels(), ["Move", SELECTION_STEP, SELECTION_STEP, SELECTION_STEP]);
    let mut big = Document::new_empty();
    for i in 0..300 {
        big.create_solid(cube(DVec3::new(f64::from(i) * 128.0, 0.0, 0.0), 64.0), Owner::World).expect("cube");
    }
    big.clear_selection();
    big.clear_history();
    big.set_record_selection(true);
    let first = big.scene().solids().keys().next().copied().expect("solid");
    gesture(&mut big, |d| d.select(&[ObjRef::Solid(first)], SelectOp::Replace));
    let one = big.history_bytes();
    gesture(&mut big, |d| d.select_all());
    gesture(&mut big, |d| d.clear_selection());
    let all = big.history_bytes() - 2 * one;
    assert!(one < 1024, "{one} bytes for one object");
    assert!(all < 300 * 12, "{all} bytes for 300 objects");
}
