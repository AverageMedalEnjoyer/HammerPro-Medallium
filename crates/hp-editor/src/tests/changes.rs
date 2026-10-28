use glam::DVec3;

use super::{cube, doc_with_cube};
use crate::{Document, FaceRef, ObjRef, Owner, SelectOp};

#[test]
fn changes_are_reported_and_reset() {
    let mut doc = Document::new_empty();
    assert!(doc.take_changes().everything);
    assert!(doc.take_changes().is_empty());
    let id = doc.create_solid(cube(DVec3::ZERO, 32.0), Owner::World).expect("create");
    let c = doc.take_changes();
    assert!(c.solids_changed.contains(&id));
    assert!(c.selection_changed);
    assert!(c.solids_dirty());
    let rev = doc.revision();
    doc.translate(&[ObjRef::Solid(id)], DVec3::X).expect("move");
    assert!(doc.revision() > rev);
    let c = doc.take_changes();
    assert_eq!(c.solids_changed.len(), 1);
    doc.delete(&[ObjRef::Solid(id)]).expect("delete");
    let c = doc.take_changes();
    assert!(c.solids_removed.contains(&id) && !c.solids_changed.contains(&id));
    doc.undo();
    let c = doc.take_changes();
    assert!(c.solids_changed.contains(&id) && !c.solids_removed.contains(&id));
}

#[test]
fn entity_changes_refresh_their_solids() {
    let mut doc = Document::new_empty();
    let e = doc.create_brush_entity("func_wall", &[], vec![cube(DVec3::ZERO, 16.0)]).expect("e");
    let s = doc.scene().entity_solid_ids(e)[0];
    doc.take_changes();
    doc.set_entity_key(&[e], "rendercolor", "255 0 0").expect("key");
    let c = doc.take_changes();
    assert!(c.entities_changed.contains(&e));
    assert!(c.solids_changed.contains(&s));
    doc.set_hidden(&[ObjRef::Entity(e)], true);
    assert!(doc.take_changes().visibility_changed);
}

#[test]
fn texture_edits_and_selection() {
    let (mut doc, id) = doc_with_cube();
    doc.take_changes();
    doc.apply_material(&[FaceRef::new(id, 0)], "X/Y").expect("material");
    let c = doc.take_changes();
    assert!(c.solids_changed.contains(&id));
    assert!(!c.selection_changed);
    doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
    let c = doc.take_changes();
    assert!(c.selection_changed && c.solids_changed.is_empty());
    doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
    assert!(doc.take_changes().is_empty());
    doc.create_visgroup("V", None, &[ObjRef::Solid(id)]).expect("visgroup");
    assert!(doc.take_changes().visgroups_changed);
    doc.set_world_key("skyname", "x").expect("sky");
    assert!(doc.take_changes().world_changed);
    doc.group(&[ObjRef::Solid(id)]).expect("group");
    assert!(doc.take_changes().groups_changed);
    doc.mark_everything_changed();
    assert!(doc.pending_changes().everything);
}

#[test]
fn document_is_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<Document>();
    check::<crate::Changes>();
    let (doc, _) = doc_with_cube();
    std::thread::scope(|s| {
        s.spawn(|| doc.prepare_picking());
    });
}
