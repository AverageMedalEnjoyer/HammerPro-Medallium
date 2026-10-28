use hp_editor::{Document, EditResult, FaceEdit, FaceRef};

#[derive(Clone, Debug, PartialEq)]
pub enum Multi<T> {
    Empty,
    Same(T),
    Mixed(T),
}

impl<T: Clone> Multi<T> {
    pub fn value(&self) -> Option<T> {
        match self {
            Multi::Empty => None,
            Multi::Same(v) | Multi::Mixed(v) => Some(v.clone()),
        }
    }

    pub fn is_mixed(&self) -> bool {
        matches!(self, Multi::Mixed(_))
    }
}

impl<T> Multi<T> {
    fn push(&mut self, v: T, same: impl Fn(&T, &T) -> bool) {
        let differs = match self {
            Multi::Empty => {
                *self = Multi::Same(v);
                return;
            }
            Multi::Same(first) => !same(first, &v),
            Multi::Mixed(_) => false,
        };
        if differs && let Multi::Same(first) = std::mem::replace(self, Multi::Empty) {
            *self = Multi::Mixed(first);
        }
    }
}

fn close(a: &f64, b: &f64) -> bool {
    (a - b).abs() <= 1e-4 * a.abs().max(b.abs()).max(1.0)
}

#[derive(Clone, Debug, PartialEq)]
pub struct FaceSummary {
    pub count: usize,
    pub material: Multi<String>,
    pub u_scale: Multi<f64>,
    pub v_scale: Multi<f64>,
    pub u_shift: Multi<f64>,
    pub v_shift: Multi<f64>,
    pub rotation: Multi<f64>,
    pub lightmap_scale: Multi<i32>,
    pub smoothing_all: u32,
    pub smoothing_any: u32,
    pub displacements: usize,
    pub materials: Vec<String>,
}

impl Default for FaceSummary {
    fn default() -> Self {
        Self {
            count: 0,
            material: Multi::Empty,
            u_scale: Multi::Empty,
            v_scale: Multi::Empty,
            u_shift: Multi::Empty,
            v_shift: Multi::Empty,
            rotation: Multi::Empty,
            lightmap_scale: Multi::Empty,
            smoothing_all: 0,
            smoothing_any: 0,
            displacements: 0,
            materials: Vec::new(),
        }
    }
}

pub fn summarize(doc: &Document, faces: &[FaceRef]) -> FaceSummary {
    let mut s = FaceSummary { smoothing_all: u32::MAX, ..FaceSummary::default() };
    let mut buf = String::new();
    for f in faces {
        let Some(side) = doc.face_side(*f) else { continue };
        s.count += 1;
        crate::util::normalize_material_into(&side.material, &mut buf);
        if !s.materials.iter().any(|m| m == &buf) {
            s.materials.push(buf.clone());
        }
        s.material.push(buf.clone(), |a, b| a == b);
        s.u_scale.push(side.uaxis.scale, close);
        s.v_scale.push(side.vaxis.scale, close);
        s.u_shift.push(side.uaxis.shift, close);
        s.v_shift.push(side.vaxis.shift, close);
        s.rotation.push(side.rotation, close);
        s.lightmap_scale.push(side.lightmap_scale, |a, b| a == b);
        s.smoothing_all &= side.smoothing_groups;
        s.smoothing_any |= side.smoothing_groups;
        if side.disp.is_some() {
            s.displacements += 1;
        }
    }
    if s.count == 0 {
        s.smoothing_all = 0;
    }
    s
}

pub fn apply_edit(
    doc: &mut Document,
    faces: &[FaceRef],
    edit: &FaceEdit,
    merge_tag: Option<&str>,
) -> EditResult<usize> {
    match merge_tag {
        Some(tag) => {
            doc.begin_merge("Edit faces", tag);
            let r = doc.edit_faces(faces, edit);
            doc.commit();
            r
        }
        None => doc.edit_faces(faces, edit),
    }
}

pub fn apply_edit_named(
    doc: &mut Document,
    faces: &[FaceRef],
    edit: &FaceEdit,
    label: &str,
    merge_tag: &str,
) -> EditResult<usize> {
    doc.begin_merge(label, merge_tag);
    let r = doc.edit_faces(faces, edit);
    doc.commit();
    r
}

pub fn set_smoothing_bit(doc: &mut Document, faces: &[FaceRef], bit: u32, on: bool) -> EditResult<usize> {
    let mask = 1u32.checked_shl(bit).unwrap_or(0);
    if mask == 0 {
        return Ok(0);
    }
    let mut groups: Vec<(u32, Vec<FaceRef>)> = Vec::new();
    for f in faces {
        let Some(side) = doc.face_side(*f) else { continue };
        let new = if on { side.smoothing_groups | mask } else { side.smoothing_groups & !mask };
        if new == side.smoothing_groups {
            continue;
        }
        match groups.iter_mut().find(|(m, _)| *m == new) {
            Some((_, list)) => list.push(*f),
            None => groups.push((new, vec![*f])),
        }
    }
    if groups.is_empty() {
        return Ok(0);
    }
    doc.transaction("Smoothing groups", |doc| {
        let mut n = 0;
        for (mask, list) in &groups {
            n += doc.edit_faces(list, &FaceEdit { smoothing_groups: Some(*mask), ..FaceEdit::default() })?;
        }
        Ok(n)
    })
}

pub fn faces_with_materials(doc: &Document, materials: &[String]) -> Vec<FaceRef> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for node in doc.scene().solids().values() {
        if !doc.is_solid_visible(node.solid.id) {
            continue;
        }
        for (i, side) in node.solid.sides.iter().enumerate() {
            crate::util::normalize_material_into(&side.material, &mut buf);
            if materials.iter().any(|m| m == &buf) {
                out.push(FaceRef::new(node.solid.id, i));
            }
        }
    }
    out
}

pub fn faces_of_selected_solids(doc: &Document) -> Vec<FaceRef> {
    doc.all_faces_of(&doc.selected_objects())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_geom::Justify;

    fn floor_faces(doc: &Document) -> Vec<FaceRef> {
        (0..6).map(|i| FaceRef::new(2, i)).filter(|f| doc.face_side(*f).is_some()).collect()
    }

    #[test]
    fn summary_detects_mixed_values() {
        let mut state = hp_app::testing::test_state();
        let faces = floor_faces(&state.doc);
        assert_eq!(faces.len(), 6);
        let s = summarize(&state.doc, &faces);
        assert_eq!(s.count, 6);
        assert_eq!(s.material, Multi::Same("dev/dev_measuregeneric01b".to_owned()));
        assert_eq!(s.u_scale, Multi::Same(0.25));
        assert_eq!(s.lightmap_scale, Multi::Same(16));
        assert_eq!(s.smoothing_all, 0);

        let one = [faces[0]];
        apply_edit(&mut state.doc, &one, &FaceEdit { u_scale: Some(0.5), ..FaceEdit::default() }, None).unwrap();
        let s = summarize(&state.doc, &faces);
        assert!(s.u_scale.is_mixed());
        assert_eq!(s.v_scale, Multi::Same(0.25));

        let mixed = [FaceRef::new(2, 0), FaceRef::new(3, 0), FaceRef::new(999, 0)];
        let s = summarize(&state.doc, &mixed);
        assert_eq!(s.count, 2);
        assert!(s.material.is_mixed());
        assert_eq!(s.materials.len(), 2);
        assert_eq!(summarize(&state.doc, &[]).material, Multi::Empty);
    }

    #[test]
    fn scale_edit_and_one_undo_step_per_drag() {
        let mut state = hp_app::testing::test_state();
        let faces = floor_faces(&state.doc);
        let before = state.doc.history_len().0;
        for v in [0.3, 0.4, 0.5] {
            apply_edit(&mut state.doc, &faces, &FaceEdit { u_scale: Some(v), ..FaceEdit::default() }, Some("drag-1"))
                .unwrap();
        }
        assert_eq!(state.doc.history_len().0, before + 1);
        assert_eq!(summarize(&state.doc, &faces).u_scale, Multi::Same(0.5));
        apply_edit(&mut state.doc, &faces, &FaceEdit { v_scale: Some(1.0), ..FaceEdit::default() }, Some("drag-2"))
            .unwrap();
        assert_eq!(state.doc.history_len().0, before + 2);
        assert!(state.doc.undo());
        assert_eq!(summarize(&state.doc, &faces).v_scale, Multi::Same(0.25));
        assert!(state.doc.undo());
        assert_eq!(summarize(&state.doc, &faces).u_scale, Multi::Same(0.25));
    }

    #[test]
    fn justify_and_apply_material() {
        let mut state = hp_app::testing::test_state();
        let faces = vec![FaceRef::new(2, 0)];
        apply_edit(
            &mut state.doc,
            &faces,
            &FaceEdit { u_shift: Some(37.0), v_shift: Some(13.0), ..FaceEdit::default() },
            None,
        )
        .unwrap();
        let sizes = std::collections::HashMap::new();
        let lookup = crate::util::size_lookup(&sizes);
        let before = state.doc.history_len().0;
        assert_eq!(state.doc.justify_faces(&faces, Justify::Left, false, &lookup).unwrap(), 1);
        assert_eq!(state.doc.justify_faces(&faces, Justify::Top, false, &lookup).unwrap(), 1);
        assert_eq!(state.doc.history_len().0, before + 2);
        let s = summarize(&state.doc, &faces);
        assert_ne!(s.u_shift, Multi::Same(37.0));
        assert_ne!(s.v_shift, Multi::Same(13.0));
        let n = state.doc.apply_material(&faces, "brick/brickwall001a").unwrap();
        assert_eq!(n, 1);
        assert_eq!(summarize(&state.doc, &faces).material, Multi::Same("brick/brickwall001a".to_owned()));
        assert_eq!(state.doc.history_len().0, before + 3);
        assert!(state.doc.undo());
        assert_eq!(summarize(&state.doc, &faces).material, Multi::Same("dev/dev_measuregeneric01b".to_owned()));
    }

    #[test]
    fn smoothing_bits() {
        let mut state = hp_app::testing::test_state();
        let faces = floor_faces(&state.doc);
        apply_edit(
            &mut state.doc,
            &faces[..1],
            &FaceEdit { smoothing_groups: Some(0b100), ..FaceEdit::default() },
            None,
        )
        .unwrap();
        let before = state.doc.history_len().0;
        let n = set_smoothing_bit(&mut state.doc, &faces, 0, true).unwrap();
        assert_eq!(n, 6);
        assert_eq!(state.doc.history_len().0, before + 1);
        let s = summarize(&state.doc, &faces);
        assert_eq!(s.smoothing_all, 0b1);
        assert_eq!(s.smoothing_any, 0b101);
        assert_eq!(state.doc.face_side(faces[0]).map(|s| s.smoothing_groups), Some(0b101));
        set_smoothing_bit(&mut state.doc, &faces, 0, false).unwrap();
        assert_eq!(summarize(&state.doc, &faces).smoothing_any, 0b100);
        assert_eq!(set_smoothing_bit(&mut state.doc, &faces, 40, true).unwrap(), 0);
    }

    #[test]
    fn mark_faces_by_material() {
        let state = hp_app::testing::test_state();
        let f = faces_with_materials(&state.doc, &["concrete/concretefloor001a".to_owned()]);
        assert_eq!(f.len(), 6);
        assert!(f.iter().all(|f| f.solid == 3));
    }
}
