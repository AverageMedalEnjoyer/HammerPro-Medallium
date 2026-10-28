use glam::DVec3;

use super::{bounds, check_undo_redo, cube};
use crate::{Document, EditError, ObjRef, Owner, PasteSpecial, SelectMode};

fn source() -> (Document, Vec<ObjRef>) {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 32.0), Owner::World).expect("a");
    let door = doc
        .create_brush_entity(
            "func_door",
            &[("targetname".into(), "door".into())],
            vec![cube(DVec3::new(64.0, 0.0, 0.0), 32.0)],
        )
        .expect("door");
    let button = doc.create_point_entity("func_button_point", DVec3::new(0.0, 64.0, 0.0), &[]).expect("button");
    doc.add_connection(&[button], &hp_vmf::Connection::parse("OnPressed", "door,Open,,0,-1")).expect("output");
    doc.group(&[ObjRef::Solid(a), ObjRef::Entity(door)]).expect("group");
    let objs = vec![ObjRef::Solid(a), ObjRef::Entity(door), ObjRef::Entity(button)];
    (doc, objs)
}

#[test]
fn copy_paste_round_trip_across_documents() {
    let (src, objs) = source();
    let text = src.copy(&objs);
    assert!(text.contains("hammerpro_clipboard"));
    let (solids, entities) = hp_vmf::parse_fragment(&text).expect("fragment");
    assert_eq!((solids.len(), entities.len()), (1, 2));
    let mut dst = Document::new_empty();
    let pasted = check_undo_redo(&mut dst, |d| d.paste(&text, DVec3::new(0.0, 0.0, 64.0)).expect("paste"));
    assert_eq!(pasted.len(), 3);
    assert_eq!(dst.selected_objects(), pasted);
    assert_eq!(dst.scene().solids().len(), 2);
    assert_eq!(dst.scene().groups().len(), 1, "groups survive the clipboard");
    let solid = pasted[0];
    assert_eq!(bounds(&dst, solid).min.z, 64.0);
    assert_eq!(dst.resolve(solid, SelectMode::Groups).len(), 2);
    let again = dst.paste(&text, DVec3::ZERO).expect("again");
    assert!(again.iter().all(|o| !pasted.contains(o)));
    assert_eq!(dst.scene().groups().len(), 2);
    let back = dst.copy(&pasted);
    let mut third = Document::new_empty();
    third.paste(&back, DVec3::new(0.0, 0.0, -64.0)).expect("third");
    let b1: Vec<_> = third.scene().solids().values().map(|n| n.bounds).collect();
    let b0: Vec<_> = src.scene().solids().values().map(|n| n.bounds).collect();
    assert_eq!(b0, b1);
}

#[test]
fn paste_errors_and_foreign_text() {
    let mut doc = Document::new_empty();
    assert!(matches!(doc.paste("", DVec3::ZERO), Err(EditError::EmptyClipboard)));
    assert!(matches!(doc.paste("just some text", DVec3::ZERO), Err(EditError::EmptyClipboard)));
    assert!(doc.paste("solid { }", DVec3::NAN).is_err());
    let (src, _) = source();
    let whole = src.to_vmf_string();
    let objs = doc.paste(&whole, DVec3::ZERO).expect("whole map");
    assert_eq!(objs.len(), 3);
    let text = hp_vmf::write_solid(&src.scene().solids().values().next().expect("solid").solid);
    let objs = doc.paste(&text, DVec3::ZERO).expect("hammer clipboard");
    assert_eq!(objs.len(), 1);
    assert_eq!(doc.scene().object_group(objs[0]), None);
}

#[test]
fn cut() {
    let (mut doc, objs) = source();
    doc.set_selection(&objs);
    let text = doc.cut_selection().expect("cut");
    assert!(doc.scene().solids().is_empty());
    assert!(doc.scene().entities().is_empty());
    assert!(doc.scene().groups().is_empty());
    doc.paste(&text, DVec3::ZERO).expect("paste");
    assert_eq!(doc.scene().solids().len(), 2);
    assert_eq!(doc.copy_selection(), doc.copy(&doc.selected_objects()));
}

#[test]
fn paste_special_copies() {
    let (src, objs) = source();
    let text = src.copy(&objs);
    let mut doc = Document::new_empty();
    let opts = PasteSpecial {
        copies: 3,
        offset: DVec3::new(256.0, 0.0, 0.0),
        rotation: DVec3::new(0.0, 0.0, 90.0),
        group: true,
        unique_names: true,
        name_prefix: "p_".into(),
        ..PasteSpecial::default()
    };
    let all = check_undo_redo(&mut doc, |d| d.paste_special(&text, &opts).expect("special"));
    assert_eq!(all.len(), 9);
    assert_eq!(doc.scene().solids().len(), 6);
    assert_eq!(doc.scene().groups().len(), 6);
    let mut names: Vec<String> =
        doc.scene().entities().values().filter_map(|n| n.entity.get("targetname").map(str::to_owned)).collect();
    names.sort();
    assert_eq!(names, vec!["p_door", "p_door_2", "p_door_3"]);
    for n in doc.scene().entities().values() {
        for c in &n.entity.connections {
            assert!(c.target.starts_with("p_door"));
        }
    }
    let xs: Vec<f64> = doc.scene().solids().values().map(|n| n.bounds.center().x).collect();
    assert!(xs.iter().any(|x| *x > 700.0));
    assert!(doc.paste_special(&text, &PasteSpecial { copies: 0, ..PasteSpecial::default() }).is_err());
    let at_original = PasteSpecial { copies: 1, start_at_original: true, ..PasteSpecial::default() };
    let objs = doc.paste_special(&text, &at_original).expect("at original");
    assert_eq!(bounds(&doc, objs[0]).min, DVec3::ZERO);
}
