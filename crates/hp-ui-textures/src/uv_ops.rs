use glam::{DVec2, DVec3};
use hp_app::AppState;
use hp_editor::FaceRef;

use crate::uv_editor::{UvFrame, normalize_deg, wrap_shifts};
use crate::uv_layers::{Affine2, LayerModel, LayerValues, Transform};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeAxis {
    X,
    Y,
}

impl EdgeAxis {
    pub(crate) fn nearest(d: DVec2) -> EdgeAxis {
        if d.x.abs() >= d.y.abs() { EdgeAxis::X } else { EdgeAxis::Y }
    }
}

pub(crate) fn align_angle(d: DVec2, axis: EdgeAxis) -> Option<f64> {
    if !(d.is_finite() && d.length_squared() > 1e-18) {
        return None;
    }
    let theta = d.y.atan2(d.x).to_degrees();
    let targets = match axis {
        EdgeAxis::X => [0.0, 180.0],
        EdgeAxis::Y => [90.0, -90.0],
    };
    targets.into_iter().map(|t| normalize_deg(t - theta)).min_by(|a, b| a.abs().total_cmp(&b.abs()))
}

fn nearest_corner(uv: DVec2, tex: DVec2) -> DVec2 {
    DVec2::new((uv.x / tex.x).round() * tex.x, (uv.y / tex.y).round() * tex.y)
}

pub(crate) fn frame_edge(
    frame: &UvFrame,
    p0: DVec3,
    p1: DVec3,
    axis: EdgeAxis,
    fit: Option<f64>,
    tex: DVec2,
) -> Option<UvFrame> {
    let (u0, u1) = (frame.uv(p0), frame.uv(p1));
    let deg = align_angle(u1 - u0, axis)?;
    let mut out = frame.rotated(deg, (u0 + u1) * 0.5);
    if let Some(n) = fit.filter(|n| n.is_finite() && *n > 0.0) {
        let len = (u1 - u0).length();
        let span = match axis {
            EdgeAxis::X => tex.x,
            EdgeAxis::Y => tex.y,
        };
        let k = n * span / len;
        if !(k.is_finite() && k > 1e-6) {
            return None;
        }
        out = out.scaled(DVec2::splat(k), out.uv(p0));
        let at = out.uv(p0);
        out = out.moved(nearest_corner(at, tex) - at);
    }
    Some(out)
}

pub(crate) fn frame_origin(frame: &UvFrame, p: DVec3, tex: DVec2) -> UvFrame {
    let at = frame.uv(p);
    frame.moved(nearest_corner(at, tex) - at)
}

pub(crate) fn layer_edge(
    model: &LayerModel,
    v: &LayerValues,
    r0: [f64; 2],
    r1: [f64; 2],
    axis: EdgeAxis,
    fit: Option<f64>,
) -> Option<Transform> {
    let w = model.written(v);
    let (q0, q1) = (w.apply(r0), w.apply(r1));
    let d = DVec2::new(q1[0] - q0[0], q1[1] - q0[1]);
    let mut deg = align_angle(d, axis)?;
    if model.bakes() {
        deg = (deg / 90.0).round() * 90.0;
    }
    let mut tr = Transform { rotate: normalize_deg(v.tr.rotate + deg), ..v.tr };
    let mid = [(r0[0] + r1[0]) * 0.5, (r0[1] + r1[1]) * 0.5];
    tr = model.anchored(v, tr, mid, w.apply(mid));
    if let Some(n) = fit.filter(|n| n.is_finite() && *n > 0.0) {
        let k = n / d.length();
        if !(k.is_finite() && k > 1e-6) {
            return None;
        }
        tr.scale = [tr.scale[0] * k, tr.scale[1] * k];
        if model.bakes() {
            for s in &mut tr.scale {
                *s = if s.round() == 0.0 { s.signum() } else { s.round() };
            }
        }
        let at = model.written(&LayerValues { tr, ..*v }).apply(r0);
        tr = model.anchored(v, tr, r0, [at[0].round(), at[1].round()]);
    }
    Some(tr)
}

pub(crate) fn layer_origin(model: &LayerModel, v: &LayerValues, r: [f64; 2]) -> Transform {
    let q = model.written(v).apply(r);
    model.anchored(v, v.tr, r, [q[0].round(), q[1].round()])
}

pub(crate) fn face_tex(state: &mut AppState, face: FaceRef) -> DVec2 {
    let material = state.doc.face_side(face).map(|s| crate::util::normalize_material(&s.material)).unwrap_or_default();
    crate::util::material_sizes(state, [material.as_str()])
        .get(&material)
        .map_or(DVec2::splat(128.0), |(w, h)| DVec2::new(f64::from((*w).max(1)), f64::from((*h).max(1))))
}

pub fn face_texels(state: &AppState, face: FaceRef) -> Vec<[f64; 2]> {
    let Some(frame) = state.doc.face_side(face).and_then(UvFrame::of) else { return Vec::new() };
    state.doc.face_polygon(face).map(|poly| poly.iter().map(|p| frame.uv(*p).to_array()).collect()).unwrap_or_default()
}

pub(crate) fn edge_of(state: &AppState, face: FaceRef, i: usize) -> Option<(DVec3, DVec3)> {
    let poly = state.doc.face_polygon(face)?;
    (i < poly.len()).then(|| (poly[i], poly[(i + 1) % poly.len()]))
}

fn parallel_edge(state: &AppState, face: FaceRef, dir: DVec3) -> Option<(DVec3, DVec3)> {
    let poly = state.doc.face_polygon(face)?;
    let n = poly.len();
    let dir = dir.normalize_or_zero();
    (0..n)
        .map(|i| (poly[i], poly[(i + 1) % n]))
        .map(|(a, b)| {
            let c = (b - a).normalize_or_zero().dot(dir);
            if c >= 0.0 { (c, (a, b)) } else { (-c, (b, a)) }
        })
        .filter(|(c, _)| *c > 10f64.to_radians().cos())
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, e)| e)
}

pub mod labels {
    pub const MOVE_VERTICES: &str = "UV move vertices";
    pub const MOVE_EDGES: &str = "UV move edges";
    pub const ALIGN_EDGE: &str = "UV align edge";
    pub const FIT_EDGE: &str = "UV fit along edge";
    pub const TURN_EDGE: &str = "UV turn about edge";
    pub const ORIGIN: &str = "UV tile corner on vertex";
    pub const MOVE_FACE: &str = "UV move";
    pub const SCALE_FACE: &str = "UV scale";
    pub const ROTATE_FACE: &str = "UV rotate";
}

pub(crate) fn write_frame(
    state: &mut AppState,
    face: FaceRef,
    new: &UvFrame,
    label: &str,
    tag: &str,
) -> Result<(), String> {
    let side = state.doc.face_side(face).ok_or("no such face")?;
    let dirs = (side.uaxis.axis, side.vaxis.axis);
    let edit = new.edit(dirs).ok_or("the texture axes of this face are invalid")?;
    crate::faces::apply_edit_named(&mut state.doc, &[face], &edit, label, tag).map_err(|e| e.to_string())?;
    wrap_shifts(state, face, tag);
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EdgeOp {
    Align(EdgeAxis),
    Fit(EdgeAxis, f64),
    Angle(f64),
    Turn(f64),
}

impl EdgeOp {
    pub fn label(self) -> &'static str {
        match self {
            EdgeOp::Align(_) => labels::ALIGN_EDGE,
            EdgeOp::Fit(..) => labels::FIT_EDGE,
            EdgeOp::Angle(_) | EdgeOp::Turn(_) => labels::TURN_EDGE,
        }
    }
}

pub(crate) fn frame_edge_op(frame: &UvFrame, p0: DVec3, p1: DVec3, op: EdgeOp, tex: DVec2) -> Option<UvFrame> {
    let (u0, u1) = (frame.uv(p0), frame.uv(p1));
    let mid = (u0 + u1) * 0.5;
    match op {
        EdgeOp::Align(axis) => frame_edge(frame, p0, p1, axis, None, tex),
        EdgeOp::Fit(axis, n) => frame_edge(frame, p0, p1, axis, Some(n), tex),
        EdgeOp::Angle(deg) => {
            let now = crate::uv_select::edge_angle(u0, u1)?;
            deg.is_finite().then(|| frame.rotated(normalize_deg(deg - now), mid))
        }
        EdgeOp::Turn(deg) => deg.is_finite().then(|| frame.rotated(deg, mid)),
    }
}

pub(crate) fn layer_edge_op(
    model: &LayerModel,
    v: &LayerValues,
    r0: [f64; 2],
    r1: [f64; 2],
    op: EdgeOp,
) -> Option<Transform> {
    let turn = |deg: f64| {
        let w = model.written(v);
        let mut deg = deg;
        if model.bakes() {
            deg = (deg / 90.0).round() * 90.0;
        }
        let tr = Transform { rotate: normalize_deg(v.tr.rotate + deg), ..v.tr };
        let mid = [(r0[0] + r1[0]) * 0.5, (r0[1] + r1[1]) * 0.5];
        model.anchored(v, tr, mid, w.apply(mid))
    };
    match op {
        EdgeOp::Align(axis) => layer_edge(model, v, r0, r1, axis, None),
        EdgeOp::Fit(axis, n) => layer_edge(model, v, r0, r1, axis, Some(n)),
        EdgeOp::Angle(deg) => {
            let w = model.written(v);
            let (q0, q1) = (w.apply(r0), w.apply(r1));
            let now = crate::uv_select::edge_angle(DVec2::new(q0[0], q0[1]), DVec2::new(q1[0], q1[1]))?;
            deg.is_finite().then(|| turn(normalize_deg(deg - now)))
        }
        EdgeOp::Turn(deg) => deg.is_finite().then(|| turn(deg)),
    }
}

pub(crate) fn layer_fit(model: &LayerModel, v: &LayerValues, w: &Affine2) -> Option<Transform> {
    let s = model.out_scale(v);
    let a = w.source_unscaled(s)?;
    let mut rotate = normalize_deg(a.m[1][0].atan2(a.m[0][0]).to_degrees());
    if model.bakes() {
        rotate = normalize_deg((rotate / 90.0).round() * 90.0);
    }
    let mut tr = model.solve(w, v, rotate)?;
    if model.bakes() {
        for x in &mut tr.scale {
            *x = if x.round() == 0.0 { x.signum() } else { x.round() };
        }
        let probe = [0.5, 0.5];
        tr = model.anchored(v, tr, probe, w.apply(probe));
    }
    Some(tr)
}

pub(crate) type EdgeEnds = (FaceRef, (DVec3, DVec3));

pub(crate) fn edge_work(
    state: &AppState,
    picks: &[(FaceRef, usize)],
    others: &[FaceRef],
) -> Result<Vec<EdgeEnds>, String> {
    let Some(&(f0, e0)) = picks.first() else { return Ok(Vec::new()) };
    let (a0, b0) = edge_of(state, f0, e0).ok_or("no such edge")?;
    let mut work: Vec<EdgeEnds> = Vec::new();
    for (f, e) in picks {
        if work.iter().any(|(g, _)| g == f) {
            continue;
        }
        work.push((*f, edge_of(state, *f, *e).ok_or("no such edge")?));
    }
    for f in others {
        if work.iter().any(|(g, _)| g == f) {
            continue;
        }
        if let Some(e) = parallel_edge(state, *f, b0 - a0) {
            work.push((*f, e));
        }
    }
    Ok(work)
}

pub fn edges_op(
    state: &mut AppState,
    picks: &[(FaceRef, usize)],
    others: &[FaceRef],
    op: EdgeOp,
    tag: &str,
) -> Result<usize, String> {
    let mut done = 0;
    for (f, (a, b)) in edge_work(state, picks, others)? {
        let Some(frame) = state.doc.face_side(f).and_then(UvFrame::of) else { continue };
        let tex = face_tex(state, f);
        let Some(new) = frame_edge_op(&frame, a, b, op, tex) else { continue };
        write_frame(state, f, &new, op.label(), tag)?;
        done += 1;
    }
    Ok(done)
}

pub fn align_edge(
    state: &mut AppState,
    face: FaceRef,
    edge: usize,
    axis: EdgeAxis,
    fit: Option<f64>,
    others: &[FaceRef],
    tag: &str,
) -> Result<usize, String> {
    let op = match fit {
        Some(n) => EdgeOp::Fit(axis, n),
        None => EdgeOp::Align(axis),
    };
    edges_op(state, &[(face, edge)], others, op, tag)
}

pub fn vertex_origin(
    state: &mut AppState,
    face: FaceRef,
    vertex: usize,
    others: &[FaceRef],
    tag: &str,
) -> Result<usize, String> {
    let p = state.doc.face_polygon(face).and_then(|poly| poly.get(vertex).copied()).ok_or("no such vertex")?;
    let mut done = 0;
    for f in std::iter::once(face).chain(others.iter().copied().filter(|f| *f != face)) {
        let shared = state.doc.face_polygon(f).is_some_and(|poly| poly.iter().any(|q| q.distance(p) < 0.5));
        if !shared {
            continue;
        }
        let Some(frame) = state.doc.face_side(f).and_then(UvFrame::of) else { continue };
        let tex = face_tex(state, f);
        write_frame(state, f, &frame_origin(&frame, p, tex), labels::ORIGIN, tag)?;
        done += 1;
    }
    Ok(done)
}

pub fn shared_masks(
    state: &AppState,
    explicit: &[(FaceRef, Vec<bool>)],
    others: &[FaceRef],
) -> Vec<(FaceRef, Vec<bool>)> {
    let mut out: Vec<(FaceRef, Vec<bool>)> = explicit.iter().filter(|(_, m)| m.iter().any(|x| *x)).cloned().collect();
    let mut points: Vec<DVec3> = Vec::new();
    for (f, m) in explicit {
        if let Some(poly) = state.doc.face_polygon(*f) {
            points.extend(poly.iter().zip(m).filter(|(_, x)| **x).map(|(p, _)| *p));
        }
    }
    for f in others {
        if out.iter().any(|(g, _)| g == f) {
            continue;
        }
        let Some(poly) = state.doc.face_polygon(*f) else { continue };
        let mask: Vec<bool> = poly.iter().map(|p| points.iter().any(|q| q.distance(*p) < 0.5)).collect();
        if mask.iter().any(|x| *x) {
            out.push((*f, mask));
        }
    }
    out
}

pub fn move_masks(
    state: &mut AppState,
    masks: &[(FaceRef, Vec<bool>)],
    d: DVec2,
    label: &str,
    tag: &str,
) -> Result<usize, String> {
    let mut done = 0;
    for (f, mask) in masks {
        let Some(frame) = state.doc.face_side(*f).and_then(UvFrame::of) else { continue };
        let Some(poly) = state.doc.face_polygon(*f) else { continue };
        if poly.len() != mask.len() {
            continue;
        }
        let uvs: Vec<DVec2> = poly.iter().map(|p| frame.uv(*p)).collect();
        let t = crate::uv_select::vertex_affine(&uvs, mask, d)
            .ok_or("the texture would be flattened: a vertex moved onto one that stays")?;
        let new = frame.transformed(&t).ok_or("the texture would be flattened")?;
        write_frame(state, *f, &new, label, tag)?;
        done += 1;
    }
    Ok(done)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    Edge { edge: usize, op: EdgeOp },
    Vertex(usize),
    Move { mask: Vec<bool>, delta: [f64; 2] },
}

pub fn layer_pick(
    state: &mut AppState,
    face: FaceRef,
    layer: crate::uv_layers::Layer,
    pick: &Pick,
) -> Result<(String, crate::uv_layers::LayerEdit), String> {
    let side = state.doc.face_side(face).ok_or("no such face")?;
    let material = crate::util::normalize_material(&side.material);
    let frame = UvFrame::of(side).ok_or("the texture axes of this face are invalid")?;
    let poly = state.doc.face_polygon(face).ok_or("no such face")?.to_vec();
    let mat = crate::uv_layers::resolved_material(state, &material).ok_or_else(|| format!("no material {material}"))?;
    if !crate::uv_layers::layers_of(&mat).contains(&layer) {
        return Err(format!("{material} has no {} layer", layer.mcp_name()));
    }
    let model = LayerModel::of(&mat, layer);
    if !model.editable() {
        return Err(format!("the game does not draw the {} layer of {material}", layer.mcp_name()));
    }
    let tex = face_tex(state, face);
    let raw = |p: DVec3| {
        let uv = frame.uv(p);
        [uv.x / tex.x, uv.y / tex.y]
    };
    let v = model.values;
    let n = poly.len();
    let tr = match pick {
        Pick::Edge { edge, op } if *edge < n => {
            layer_edge_op(&model, &v, raw(poly[*edge]), raw(poly[(*edge + 1) % n]), *op).ok_or("the edge is a point")?
        }
        Pick::Vertex(i) if *i < n => layer_origin(&model, &v, raw(poly[*i])),
        Pick::Move { mask, delta } if mask.len() == n => {
            let uvs: Vec<DVec2> = poly.iter().map(|p| frame.uv(*p)).collect();
            let t = crate::uv_select::vertex_affine(&uvs, mask, DVec2::new(delta[0], delta[1]))
                .ok_or("the layer would be flattened")?;
            let w =
                crate::uv_select::layer_map_moved(&model.written(&v), &t, tex).ok_or("the layer would be flattened")?;
            layer_fit(&model, &v, &w).ok_or("the layer would be flattened")?
        }
        _ => return Err(format!("the face has {n} vertices")),
    };
    let edit = crate::uv_layers::LayerEdit {
        center: Some(tr.center),
        scale: Some(tr.scale),
        rotate: Some(tr.rotate),
        translate: Some(tr.translate),
        detail_scale: None,
    };
    Ok((material, edit))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> UvFrame {
        let a = DVec3::new(1.0, 0.3, 0.0) * 2.0;
        let b = DVec3::new(-0.3, 1.0, 0.0) * 4.0;
        UvFrame { a, b, shift: DVec2::new(5.0, -3.0), rotation: 0.0 }
    }

    #[test]
    fn edges_land_on_the_axes() {
        let f = frame();
        let tex = DVec2::new(128.0, 64.0);
        let (p0, p1) = (DVec3::new(0.0, 0.0, 0.0), DVec3::new(40.0, 10.0, 0.0));
        for axis in [EdgeAxis::X, EdgeAxis::Y] {
            let g = frame_edge(&f, p0, p1, axis, None, tex).expect("aligned");
            let d = g.uv(p1) - g.uv(p0);
            match axis {
                EdgeAxis::X => assert!(d.y.abs() < 1e-9, "{d:?}"),
                EdgeAxis::Y => assert!(d.x.abs() < 1e-9, "{d:?}"),
            }
            assert!((d.length() - (f.uv(p1) - f.uv(p0)).length()).abs() < 1e-9);
            let before = f.uv(p1) - f.uv(p0);
            assert!(d.dot(before) > 0.0, "{d:?} {before:?}");
        }
        let g = frame_edge(&f, p0, p1, EdgeAxis::X, Some(2.0), tex).expect("fit");
        let d = g.uv(p1) - g.uv(p0);
        assert!((d.x.abs() - 256.0).abs() < 1e-6 && d.y.abs() < 1e-6, "{d:?}");
        let s = g.uv(p0);
        assert!((s.x / 128.0 - (s.x / 128.0).round()).abs() < 1e-9 && (s.y / 64.0 - (s.y / 64.0).round()).abs() < 1e-9);
        let o = frame_origin(&f, p1, tex).uv(p1);
        assert!((o.x / 128.0).fract().abs() < 1e-9 && (o.y / 64.0).fract().abs() < 1e-9, "{o:?}");
        assert_eq!(align_angle(DVec2::ZERO, EdgeAxis::X), None);
        let a = align_angle(DVec2::new(-1.0, 0.1), EdgeAxis::X).unwrap();
        assert!((a - 0.1f64.atan().to_degrees()).abs() < 1e-9, "the smaller turn: {a}");
    }

    #[test]
    fn edges_get_an_angle_or_turn_about_their_middle() {
        let f = frame();
        let tex = DVec2::new(128.0, 64.0);
        let (p0, p1) = (DVec3::new(0.0, 0.0, 0.0), DVec3::new(40.0, 10.0, 0.0));
        let len = (f.uv(p1) - f.uv(p0)).length();
        let mid = (f.uv(p0) + f.uv(p1)) * 0.5;
        for deg in [0.0, 30.0, -135.0, 90.0, 180.0] {
            let g = frame_edge_op(&f, p0, p1, EdgeOp::Angle(deg), tex).expect("angle");
            let a = crate::uv_select::edge_angle(g.uv(p0), g.uv(p1)).expect("edge");
            assert!(normalize_deg(a - deg).abs() < 1e-9, "{deg}: {a}");
            assert!(((g.uv(p1) - g.uv(p0)).length() - len).abs() < 1e-9, "the scale is kept");
            assert!(((g.uv(p0) + g.uv(p1)) * 0.5 - mid).length() < 1e-9, "the middle stays");
        }
        let before = crate::uv_select::edge_angle(f.uv(p0), f.uv(p1)).expect("edge");
        let g = frame_edge_op(&f, p0, p1, EdgeOp::Turn(45.0), tex).expect("turn");
        let after = crate::uv_select::edge_angle(g.uv(p0), g.uv(p1)).expect("edge");
        assert!((normalize_deg(after - before) - 45.0).abs() < 1e-9);
        assert!(frame_edge_op(&f, p0, p0, EdgeOp::Angle(10.0), tex).is_none(), "a point has no direction");
        let g = frame_edge_op(&f, p0, p1, EdgeOp::Align(EdgeAxis::Y), tex).expect("align");
        assert!((g.uv(p1) - g.uv(p0)).x.abs() < 1e-9);
        assert_eq!(EdgeOp::Fit(EdgeAxis::X, 2.0).label(), labels::FIT_EDGE);
    }
}
