use glam::DVec3;
use hp_fgd::{PropertyType, ResolvedFgd};
use hp_vmf::Entity;

pub const MAX_HANDLES: usize = 16;
pub const HANDLE_PICK: f32 = 7.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HelperKind {
    Origin,
    VecLine,
    Axis,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HelperHandle {
    pub key: String,
    pub index: usize,
    pub kind: HelperKind,
    pub pos: DVec3,
}

pub fn parse_axis(text: &str) -> Option<[DVec3; 2]> {
    let (a, b) = text.split_once(',')?;
    let a = hp_vmf::parse_vec3(a)?;
    let b = hp_vmf::parse_vec3(b)?;
    (a.is_finite() && b.is_finite()).then_some([a, b])
}

pub fn format_axis(points: [DVec3; 2]) -> String {
    format!("{}, {}", hp_vmf::fmt_vec3(points[0]), hp_vmf::fmt_vec3(points[1]))
}

fn helper_keys(fgd: &ResolvedFgd, class: &str) -> Vec<(String, HelperKind)> {
    let Some(c) = fgd.class(class) else { return Vec::new() };
    let mut out: Vec<(String, HelperKind)> = Vec::new();
    let mut add = |key: &str, kind: HelperKind| {
        let key = key.trim();
        if !key.is_empty() && !out.iter().any(|(k, _)| k.eq_ignore_ascii_case(key)) && out.len() < MAX_HANDLES {
            out.push((key.to_owned(), kind));
        }
    };
    for p in &c.props {
        match p.kind {
            PropertyType::Origin => add(&p.name, HelperKind::Origin),
            PropertyType::VecLine => add(&p.name, HelperKind::VecLine),
            PropertyType::Axis => add(&p.name, HelperKind::Axis),
            _ => {}
        }
    }
    for h in &c.helpers {
        let kind = if h.is("origin") {
            HelperKind::Origin
        } else if h.is("vecline") {
            HelperKind::VecLine
        } else if h.is("axis") {
            HelperKind::Axis
        } else {
            continue;
        };
        if let Some(key) = h.first_arg() {
            add(key, kind);
        }
    }
    out
}

pub fn helper_handles(fgd: Option<&ResolvedFgd>, e: &Entity, is_brush: bool) -> Vec<HelperHandle> {
    let Some(fgd) = fgd else { return Vec::new() };
    let mut out = Vec::new();
    for (key, kind) in helper_keys(fgd, &e.classname) {
        if kind == HelperKind::Origin && key.eq_ignore_ascii_case("origin") && !is_brush {
            continue;
        }
        let Some(value) = e.get(&key) else { continue };
        match kind {
            HelperKind::Axis => {
                if let Some(points) = parse_axis(value) {
                    for (index, pos) in points.into_iter().enumerate() {
                        out.push(HelperHandle { key: key.clone(), index, kind, pos });
                    }
                }
            }
            HelperKind::Origin | HelperKind::VecLine => {
                if let Some(pos) = hp_vmf::parse_vec3(value).filter(|p| p.is_finite()) {
                    out.push(HelperHandle { key: key.clone(), index: 0, kind, pos });
                }
            }
        }
        if out.len() >= MAX_HANDLES {
            break;
        }
    }
    out
}

pub fn value_with_point(e: &Entity, handle: &HelperHandle, p: DVec3) -> Option<String> {
    if !p.is_finite() {
        return None;
    }
    let p = DVec3::new(clean(p.x), clean(p.y), clean(p.z));
    match handle.kind {
        HelperKind::Axis => {
            let mut points = parse_axis(e.get(&handle.key)?)?;
            *points.get_mut(handle.index)? = p;
            Some(format_axis(points))
        }
        HelperKind::Origin | HelperKind::VecLine => Some(hp_vmf::fmt_vec3(p)),
    }
}

fn clean(v: f64) -> f64 {
    let r = (v * 100.0).round() / 100.0;
    if (v - r).abs() < 1e-4 { r + 0.0 } else { v }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FGD: &str = r#"
@SolidClass = func_rotating : "Rotating"
[
    origin(origin) : "Origin" : "0 0 0"
]
@PointClass axis(hingeaxis) = phys_hinge : "Hinge"
[
    hingeaxis(axis) : "Hinge axis" : ""
]
@PointClass = point_beam : "Beam"
[
    origin(origin) : "Origin"
    endpoint(vecline) : "End point"
]
"#;

    fn fgd() -> ResolvedFgd {
        hp_fgd::Fgd::parse(FGD, None).resolve()
    }

    fn entity(class: &str, keys: &[(&str, &str)]) -> Entity {
        let mut e = Entity { classname: class.into(), ..Entity::default() };
        for (k, v) in keys {
            e.set(k, *v);
        }
        e
    }

    #[test]
    fn handles_of_keys() {
        let fgd = fgd();
        let hinge = entity("phys_hinge", &[("origin", "0 0 0"), ("hingeaxis", "0 0 0, 0 0 64")]);
        let h = helper_handles(Some(&fgd), &hinge, false);
        assert_eq!(h.len(), 2);
        assert_eq!((h[1].kind, h[1].index, h[1].pos), (HelperKind::Axis, 1, DVec3::new(0.0, 0.0, 64.0)));
        let v = value_with_point(&hinge, &h[1], DVec3::new(16.0, 0.0, 64.000_000_01)).expect("value");
        assert_eq!(v, "0 0 0, 16 0 64");
        let beam = entity("point_beam", &[("origin", "1 2 3"), ("endpoint", "64 0 0")]);
        let h = helper_handles(Some(&fgd), &beam, false);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].kind, HelperKind::VecLine);
        let rot = entity("func_rotating", &[("origin", "32 32 32")]);
        assert_eq!(helper_handles(Some(&fgd), &rot, true).len(), 1);
        assert!(helper_handles(Some(&fgd), &entity("phys_hinge", &[("hingeaxis", "1 2")]), false).is_empty());
        assert!(helper_handles(Some(&fgd), &entity("unknown", &[]), false).is_empty());
        assert!(helper_handles(None, &hinge, false).is_empty());
        assert_eq!(parse_axis("1 2 3,4 5 6"), Some([DVec3::new(1.0, 2.0, 3.0), DVec3::new(4.0, 5.0, 6.0)]));
        assert_eq!(parse_axis("nan 0 0, 0 0 0"), None);
    }
}
