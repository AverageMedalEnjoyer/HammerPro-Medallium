use glam::DVec3;
use hp_editor::{Document, EditResult, FaceRef};
use hp_geom::{
    DISP_TAG_FORCE_BUILDABLE_BIT, DISP_TAG_FORCE_BUILDABLE_VAL, DISP_TAG_FORCE_REMOVE_BIT, DISP_TAG_FORCE_WALKABLE_BIT,
    DISP_TAG_FORCE_WALKABLE_VAL, DispMesh,
};
use hp_vmf::DispInfo;

pub(crate) const UNDO_TAG: &str = "Tag displacement";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TagKind {
    #[default]
    Walkable,
    Buildable,
    Remove,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TagValue {
    #[default]
    On,
    Off,
    Auto,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TagSettings {
    pub kind: TagKind,
    pub value: TagValue,
}

impl TagSettings {
    pub fn effective(&self, invert: bool) -> TagValue {
        match (self.value, invert) {
            (TagValue::On, true) => TagValue::Off,
            (TagValue::Off, true) => TagValue::On,
            (v, _) => v,
        }
    }
}

pub(crate) fn apply_tag(tag: i32, kind: TagKind, value: TagValue) -> i32 {
    let (bit, val) = match kind {
        TagKind::Walkable => (DISP_TAG_FORCE_WALKABLE_BIT, DISP_TAG_FORCE_WALKABLE_VAL),
        TagKind::Buildable => (DISP_TAG_FORCE_BUILDABLE_BIT, DISP_TAG_FORCE_BUILDABLE_VAL),
        TagKind::Remove => {
            return match value {
                TagValue::On => tag | DISP_TAG_FORCE_REMOVE_BIT,
                TagValue::Off | TagValue::Auto => tag & !DISP_TAG_FORCE_REMOVE_BIT,
            };
        }
    };
    match value {
        TagValue::On => tag | bit | val,
        TagValue::Off => (tag | bit) & !val,
        TagValue::Auto => tag & !(bit | val),
    }
}

pub(crate) fn is_removed(tag: i32) -> bool {
    tag & DISP_TAG_FORCE_REMOVE_BIT != 0
}

fn corners(mesh: &DispMesh, positions: &[DVec3], t: usize) -> Option<[DVec3; 3]> {
    let idx = mesh.indices.get(t * 3..t * 3 + 3)?;
    Some([*positions.get(idx[0] as usize)?, *positions.get(idx[1] as usize)?, *positions.get(idx[2] as usize)?])
}

pub(crate) fn triangles_under(mesh: &DispMesh, center: DVec3, radius: f64, spatial: bool) -> Vec<usize> {
    let n = mesh.face_normal;
    let source = if spatial { &mesh.positions } else { &mesh.base_positions };
    (0..mesh.indices.len() / 3)
        .filter(|&t| {
            let Some(c) = corners(mesh, source, t) else { return false };
            let mid = (c[0] + c[1] + c[2]) / 3.0;
            let d = mid - center;
            let dist = if spatial { d.length() } else { (d - n * d.dot(n)).length() };
            dist <= radius || contains(&c, center)
        })
        .collect()
}

fn contains(c: &[DVec3; 3], p: DVec3) -> bool {
    let Some(normal) = (c[1] - c[0]).cross(c[2] - c[0]).try_normalize() else { return false };
    let side = |a: DVec3, b: DVec3| (b - a).cross(p - a).dot(normal);
    let s = [side(c[0], c[1]), side(c[1], c[2]), side(c[2], c[0])];
    s.iter().all(|x| *x >= -1e-9) || s.iter().all(|x| *x <= 1e-9)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TagBrush {
    pub center: DVec3,
    pub radius: f64,
    pub spatial: bool,
    pub kind: TagKind,
    pub value: TagValue,
}

pub(crate) fn tag_disp(disp: &mut DispInfo, mesh: &DispMesh, face_vertices: &[DVec3], brush: &TagBrush) -> usize {
    let count = mesh.indices.len() / 3;
    if disp.triangle_tags.len() != count {
        hp_geom::update_disp_tags(disp, face_vertices);
        disp.triangle_tags.resize(count, 0);
    }
    let mut changed = 0;
    for t in triangles_under(mesh, brush.center, brush.radius, brush.spatial) {
        if let Some(tag) = disp.triangle_tags.get_mut(t) {
            let new = apply_tag(*tag, brush.kind, brush.value);
            if new != *tag {
                *tag = new;
                changed += 1;
            }
        }
    }
    changed
}

pub(crate) fn tag_sample(doc: &mut Document, faces: &[FaceRef], brush: &TagBrush) -> EditResult<usize> {
    if !brush.center.is_finite() || !brush.radius.is_finite() || brush.radius <= 0.0 {
        return Ok(0);
    }
    let mut updates: Vec<(FaceRef, DispInfo)> = Vec::new();
    let mut total = 0;
    for f in faces {
        let Some(node) = doc.scene().solid(f.solid) else { continue };
        let (Some(mesh), Some(disp), Some(poly)) = (
            node.disp_mesh(f.side),
            node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()),
            node.geom.face_for_side(f.side),
        ) else {
            continue;
        };
        let mut d = disp.clone();
        let n = tag_disp(&mut d, mesh, &poly.vertices, brush);
        if n > 0 {
            total += n;
            updates.push((*f, d));
        }
    }
    if updates.is_empty() {
        return Ok(0);
    }
    doc.set_disp_data(UNDO_TAG, updates)?;
    Ok(total)
}

#[allow(clippy::too_many_arguments)]
pub fn tag_brush(
    doc: &mut Document,
    faces: &[FaceRef],
    center: DVec3,
    radius: f64,
    spatial: bool,
    kind: TagKind,
    value: TagValue,
    label: &str,
) -> EditResult<usize> {
    doc.begin(label);
    let r = tag_sample(doc, faces, &TagBrush { center, radius, spatial, kind, value });
    match r {
        Ok(n) => {
            doc.commit();
            Ok(n)
        }
        Err(e) => {
            doc.cancel();
            Err(e)
        }
    }
}

pub fn tag_all(
    doc: &mut Document,
    faces: &[FaceRef],
    kind: TagKind,
    value: TagValue,
    label: &str,
) -> EditResult<usize> {
    let mut updates: Vec<(FaceRef, DispInfo)> = Vec::new();
    let mut total = 0;
    for f in faces {
        let Some(node) = doc.scene().solid(f.solid) else { continue };
        let (Some(mesh), Some(disp), Some(poly)) = (
            node.disp_mesh(f.side),
            node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()),
            node.geom.face_for_side(f.side),
        ) else {
            continue;
        };
        let mut d = disp.clone();
        let count = mesh.indices.len() / 3;
        if d.triangle_tags.len() != count {
            hp_geom::update_disp_tags(&mut d, &poly.vertices);
            d.triangle_tags.resize(count, 0);
        }
        let mut n = 0;
        for tag in &mut d.triangle_tags {
            let new = apply_tag(*tag, kind, value);
            if new != *tag {
                *tag = new;
                n += 1;
            }
        }
        if n > 0 {
            total += n;
            updates.push((*f, d));
        }
    }
    if updates.is_empty() {
        return Ok(0);
    }
    doc.set_disp_data(label, updates)?;
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_geom::disp_tag_state;

    #[test]
    fn tags_force_and_release() {
        let slope = hp_geom::DISP_TAG_WALKABLE | hp_geom::DISP_TAG_BUILDABLE;
        let off = apply_tag(slope, TagKind::Walkable, TagValue::Off);
        assert_eq!(disp_tag_state(off), (false, true));
        let on = apply_tag(0, TagKind::Walkable, TagValue::On);
        assert_eq!(disp_tag_state(on), (true, false));
        assert_eq!(apply_tag(off, TagKind::Walkable, TagValue::Auto), slope, "back to the slope bits");
        let b = apply_tag(slope, TagKind::Buildable, TagValue::Off);
        assert_eq!(disp_tag_state(b), (true, false));
        let r = apply_tag(slope, TagKind::Remove, TagValue::On);
        assert!(is_removed(r) && disp_tag_state(r) == (true, true));
        assert_eq!(apply_tag(r, TagKind::Remove, TagValue::Off), slope);
        let s = TagSettings { kind: TagKind::Walkable, value: TagValue::On };
        assert_eq!(s.effective(true), TagValue::Off);
        assert_eq!(TagSettings { value: TagValue::Auto, ..s }.effective(true), TagValue::Auto);
    }
}
