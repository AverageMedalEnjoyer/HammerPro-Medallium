use std::sync::Arc;
use std::time::{Duration, Instant};

use ahash::{AHashMap, AHashSet};
use glam::{DVec3, Vec3A};
use hp_editor::{Document, Id, SolidNode};
use hp_light::rad::TexLights;
use hp_render::{LightFalloff, LightKind, PreviewLight};
use hp_vmf::Entity;

use crate::producer::{DerivedBuild, DerivedProducer, ProducerChanges, ProducerCx};

use crate::util::parse_floats;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LightValue {
    pub linear: [f32; 3],
    pub srgb: [f32; 3],
    pub intensity: f32,
}

pub(crate) fn parse_light_value(s: &str) -> Option<LightValue> {
    let (v, n) = parse_floats::<4>(s);
    let (rgb, brightness) = match n {
        1 => ([v[0]; 3], 255.0),
        3 => ([v[0], v[1], v[2]], 255.0),
        4 => ([v[0], v[1], v[2]], v[3]),
        _ => return None,
    };
    if rgb.iter().any(|c| *c < 0.0) || brightness < 0.0 {
        return None;
    }
    let srgb = rgb.map(|c| (c / 255.0).clamp(0.0, 1.0) as f32);
    let linear = rgb.map(|c| ((c / 255.0).max(0.0)).powf(2.2) as f32);
    Some(LightValue { linear, srgb, intensity: (brightness / 255.0) as f32 })
}

fn light_value(e: &Entity, ldr: &str, hdr: &str) -> Option<LightValue> {
    e.get(ldr).and_then(parse_light_value).or_else(|| e.get(hdr).and_then(parse_light_value))
}

pub(crate) fn light_direction(e: &Entity) -> DVec3 {
    let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
    let key = |k: &str| e.get(k).and_then(|s| s.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
    let angle = key("angle").unwrap_or(0.0);
    if angle == -1.0 {
        return DVec3::Z;
    }
    if angle == -2.0 {
        return DVec3::NEG_Z;
    }
    let yaw = if angle != 0.0 { angle } else { angles.y };
    let pitch = key("pitch").filter(|p| *p != 0.0).unwrap_or(angles.x);
    let (sp, cp) = pitch.to_radians().sin_cos();
    let (sy, cy) = yaw.to_radians().sin_cos();
    DVec3::new(cp * cy, cp * sy, sp)
}

const MIN_RANGE: f32 = 64.0;
const MAX_RANGE: f32 = 16384.0;

fn vrad_point(vrad: &hp_light::lights::Light, value: &LightValue) -> (f32, f32, LightFalloff) {
    let range = vrad.reach.clamp(MIN_RANGE, MAX_RANGE);
    let exponent = match vrad.kind {
        hp_light::lights::LightKind::Spot { exponent, .. } => exponent,
        _ => 0.0,
    };
    let falloff = LightFalloff {
        constant: vrad.constant,
        linear: vrad.linear,
        quadratic: vrad.quadratic,
        cap: vrad.cap,
        cutoff: vrad.cutoff,
        fade: [vrad.fade.0, vrad.fade.1],
        exponent,
    };
    let lin = value.linear.iter().fold(0.0f32, |a, b| a.max(*b));
    if lin <= 0.0 {
        return (0.0, range, falloff);
    }
    let intensity = vrad.intensity.max_element() / lin / 255.0;
    (if intensity.is_finite() { intensity.max(0.0) } else { value.intensity }, range, falloff)
}

impl crate::AssetManager {
    pub(crate) fn update_buried_lights(&mut self, doc: &Document) {
        let mut buried = AHashSet::new();
        let mut pending = false;
        for (id, s) in &self.scene.ents {
            let Some(light) = s.desc.light else { continue };
            if matches!(light.kind, LightKind::Directional { .. }) {
                continue;
            }
            let p = DVec3::new(f64::from(light.pos[0]), f64::from(light.pos[1]), f64::from(light.pos[2]));
            if !p.is_finite() {
                continue;
            }
            let around = hp_geom::Aabb::new(p - DVec3::splat(0.5), p + DVec3::splat(0.5));
            for sid in doc.solids_in_box(&around) {
                let Some(node) = doc.scene().solid(sid) else { continue };
                if !in_world_leaves(doc, node) || !strictly_inside(node, p) {
                    continue;
                }
                match self.brush_opaque(node) {
                    Some(true) => {
                        buried.insert(*id);
                        break;
                    }
                    Some(false) => {}
                    None => pending = true,
                }
            }
        }
        self.scene.buried_pending = pending;
        if buried != self.scene.buried_lights {
            self.scene.buried_lights = buried;
            self.scene.lights_dirty = true;
        }
    }

    fn brush_opaque(&self, node: &SolidNode) -> Option<bool> {
        use hp_vmt::ToolKind;
        for side in &node.solid.sides {
            let key = crate::util::material_key(&side.material);
            let Some(entry) = self.mats.analyses.get(&key) else { continue };
            let Some(a) = entry.value.as_ref()? else { continue };
            if a.water.is_some() || a.flags.intersects(hp_render::MaterialFlags::TRANSLUCENT) {
                return Some(false);
            }
            match a.tool {
                None | Some(ToolKind::Nodraw | ToolKind::Skybox | ToolKind::Skybox2d | ToolKind::BlockLight) => {}
                Some(_) => return Some(false),
            }
        }
        Some(true)
    }
}

fn in_world_leaves(doc: &Document, node: &SolidNode) -> bool {
    if node.disp.iter().any(Option::is_some) || !node.geom.valid {
        return false;
    }
    match node.owner {
        hp_editor::Owner::World => true,
        hp_editor::Owner::Entity(e) => {
            doc.scene().entity(e).is_some_and(|n| n.entity.classname.eq_ignore_ascii_case("func_detail"))
        }
    }
}

fn strictly_inside(node: &SolidNode, p: DVec3) -> bool {
    !node.geom.faces.is_empty() && node.geom.faces.iter().all(|f| f.plane.distance(p) < -0.01)
}

pub(crate) fn casts_no_shadow(e: &Entity) -> bool {
    e.get("disableshadows").and_then(|v| v.trim().parse::<f64>().ok()).is_some_and(|v| v != 0.0)
}

const DLIGHT_MIN_LIGHT: f32 = 20.0 / 256.0;

fn dynamic_direction(e: &Entity) -> DVec3 {
    let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
    let key = e.get("pitch").and_then(|s| s.trim().parse::<f64>().ok()).filter(|v| v.is_finite() && *v != 0.0);
    let pitch = key.map_or(angles.x, |p| -p);
    let (sp, cp) = pitch.to_radians().sin_cos();
    let (sy, cy) = angles.y.to_radians().sin_cos();
    DVec3::new(cp * cy, cp * sy, -sp)
}

fn dynamic_light(e: &Entity) -> (Option<PreviewLight>, Option<[f32; 3]>) {
    let (rgb, n) = parse_floats::<4>(e.get("_light").unwrap_or("255 255 255"));
    if n < 3 || rgb[..3].iter().any(|c| !c.is_finite() || *c < 0.0) {
        return (None, None);
    }
    let key = |k: &str| e.get(k).and_then(|s| s.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
    let srgb = [rgb[0], rgb[1], rgb[2]].map(|c| (c / 255.0).clamp(0.0, 1.0) as f32);
    let flags = key("spawnflags").unwrap_or(0.0) as i64;
    let radius = key("distance").unwrap_or(120.0) as f32;
    if flags & 3 == 3 || radius <= 0.0 {
        return (None, Some(srgb));
    }
    let exponent = key("brightness").unwrap_or(0.0).clamp(-16.0, 16.0);
    let scale = 2f64.powf(exponent);
    let color = [rgb[0], rgb[1], rgb[2]].map(|c| (c / 255.0 * scale) as f32);
    let strength = color.iter().fold(0.0f32, |a, b| a.max(*b));
    if !strength.is_finite() || strength <= 0.0 {
        return (None, Some(srgb));
    }
    let radius = radius.min(MAX_RANGE);
    let falloff = LightFalloff {
        quadratic: 1.0 / (DLIGHT_MIN_LIGHT * radius * radius).max(1e-6),
        cap: 1.0e9,
        cutoff: radius,
        exponent: 1.0,
        ..LightFalloff::default()
    };
    let outer = key("_cone").unwrap_or(0.0) as f32;
    let kind = if outer > 0.0 {
        let outer = outer.clamp(0.1, 179.0);
        let inner = (key("_inner_cone").unwrap_or(0.0) as f32).clamp(0.0, outer);
        LightKind::Spot { dir: dynamic_direction(e).as_vec3().to_array(), inner_deg: inner, outer_deg: outer }
    } else {
        LightKind::Point
    };
    let origin = e.origin().filter(|o| o.is_finite()).unwrap_or(DVec3::ZERO);
    let light = PreviewLight {
        kind,
        pos: origin.as_vec3().to_array(),
        color: color.map(|c| c / strength),
        intensity: strength,
        range: radius.max(1.0),
        falloff: Some(falloff),
    };
    (Some(light), Some(srgb))
}

pub(crate) fn parse_light(e: &Entity) -> (Option<PreviewLight>, Option<[f32; 3]>, Option<[f32; 3]>) {
    let class = e.classname.to_ascii_lowercase();
    let kind = match class.as_str() {
        "light" | "light_spot" | "light_environment" | "light_directional" => class.as_str(),
        "light_dynamic" => {
            let (light, srgb) = dynamic_light(e);
            return (light, None, srgb);
        }
        _ => return (None, None, None),
    };
    let Some(value) = light_value(e, "_light", "_lightHDR") else {
        return (None, None, None);
    };
    let origin = e.origin().filter(|o| o.is_finite()).unwrap_or(DVec3::ZERO);
    let pos = origin.as_vec3().to_array();
    let dir = light_direction(e).as_vec3().to_array();
    let light = |kind: LightKind, intensity: f32, range: f32, falloff: Option<LightFalloff>| PreviewLight {
        kind,
        pos,
        color: value.linear,
        intensity,
        range: range.clamp(MIN_RANGE, MAX_RANGE),
        falloff,
    };
    match kind {
        "light" | "light_spot" => {
            let Some(vrad) = hp_light::lights::parse_light(e, 1.0) else { return (None, None, Some(value.srgb)) };
            let (intensity, range, falloff) = vrad_point(&vrad, &value);
            let falloff = Some(falloff);
            let preview = match vrad.kind {
                hp_light::lights::LightKind::Spot { dir: axis, inner_cos, outer_cos, .. } => {
                    let deg = |c: f32| c.clamp(-1.0, 1.0).acos().to_degrees();
                    let outer = deg(outer_cos).clamp(0.1, 179.0);
                    let inner = deg(inner_cos).clamp(0.0, outer);
                    let dir = axis.to_array();
                    light(LightKind::Spot { dir, inner_deg: inner, outer_deg: outer }, intensity, range, falloff)
                }
                _ => light(LightKind::Point, intensity, range, falloff),
            };
            (Some(preview), None, Some(value.srgb))
        }
        _ => {
            let ambient = light_value(e, "_ambient", "_ambientHDR").map(|a| a.linear.map(|c| c * a.intensity));
            (Some(light(LightKind::Directional { dir }, value.intensity, MAX_RANGE, None)), ambient, Some(value.srgb))
        }
    }
}

pub(crate) fn producers() -> Vec<Box<dyn DerivedProducer>> {
    vec![Box::new(TexlightProducer::default())]
}

const TEXLIGHT_BUDGET: Duration = Duration::from_millis(3);
const TEXLIGHT_SLICE: usize = 256;

#[derive(Default)]
struct TexlightProducer {
    rad: Option<(u64, Vec<std::path::PathBuf>, Arc<TexLights>)>,
    values: AHashMap<String, Option<glam::Vec3>>,
    by_solid: AHashMap<Id, Vec<PreviewLight>>,
    queue: Vec<Id>,
    queued: AHashSet<Id>,
    on: bool,
    changed: bool,
}

fn rad_files(doc: &Document) -> Vec<std::path::PathBuf> {
    let map = doc.path();
    let mut files: Vec<std::path::PathBuf> = map.map(|p| p.with_extension("rad")).into_iter().collect();
    let custom = doc.light_plus().texlight;
    let custom = custom.trim();
    if !custom.is_empty() {
        let p = std::path::PathBuf::from(custom);
        if p.is_absolute() {
            files.push(p);
        } else if let Some(dir) = map.and_then(|m| m.parent()) {
            files.push(dir.join(p));
        }
    }
    files
}

impl TexlightProducer {
    fn queue_all(&mut self, doc: &Document) {
        self.by_solid.clear();
        self.queued.clear();
        self.queue = doc.scene().solids().keys().copied().collect();
        self.queue.sort_unstable();
        self.queued.extend(self.queue.iter().copied());
        self.changed = true;
    }
}

impl DerivedProducer for TexlightProducer {
    fn name(&self) -> &'static str {
        "texlights"
    }

    fn changed(&mut self, ch: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool {
        let on = cx.opts.lighting_preview;
        if !on {
            if self.on {
                *self = Self { rad: self.rad.take(), changed: true, ..Self::default() };
            }
            return self.changed;
        }
        let files = rad_files(cx.doc);
        let stale = self.rad.as_ref().is_none_or(|(g, f, _)| *g != cx.assets.generation || *f != files);
        if !self.on || ch.full || stale {
            self.on = true;
            if stale {
                self.rad = None;
                self.values.clear();
            }
            self.queue_all(cx.doc);
        } else {
            for &id in ch.doc.solids_changed.iter().chain(&ch.doc.solids_removed) {
                if self.queued.insert(id) {
                    self.queue.push(id);
                }
            }
        }
        !self.queue.is_empty() || self.changed
    }

    fn build(&mut self, cx: &mut ProducerCx<'_>) -> DerivedBuild {
        let mut out = DerivedBuild::default();
        if self.on && self.rad.is_none() {
            let files = rad_files(cx.doc);
            let extra: Vec<std::path::PathBuf> = files.iter().filter(|p| p.is_file()).cloned().collect();
            let fs = cx.assets.fs.as_ref();
            let rad = hp_light::rad::load(fs, fs.and_then(hp_fs::GameFs::game_dir), &extra);
            self.rad = Some((cx.assets.generation, files, Arc::new(rad)));
        }
        if let Some((_, _, rad)) = self.rad.clone()
            && self.on
        {
            let start = Instant::now();
            let scene = cx.doc.scene();
            while !self.queue.is_empty() {
                let n = self.queue.len().saturating_sub(TEXLIGHT_SLICE);
                for id in self.queue.split_off(n) {
                    self.queued.remove(&id);
                    let values = &mut self.values;
                    let mut value = |name: &str| match values.get(name) {
                        Some(v) => *v,
                        None => {
                            let v = rad.get(&hp_light::rad::normalize_material(name));
                            values.insert(name.to_owned(), v);
                            v
                        }
                    };
                    let lights = match scene.solid(id) {
                        Some(node) if !rad.is_empty() => solid_texlights(node, &mut value),
                        _ => Vec::new(),
                    };
                    let had =
                        if lights.is_empty() { self.by_solid.remove(&id) } else { self.by_solid.insert(id, lights) };
                    self.changed |= had.is_some() || self.by_solid.contains_key(&id);
                }
                if start.elapsed() >= TEXLIGHT_BUDGET {
                    break;
                }
            }
        }
        if self.changed {
            self.changed = false;
            let mut ids: Vec<&Id> = self.by_solid.keys().collect();
            ids.sort_unstable();
            out.lights = Some(ids.into_iter().flat_map(|id| self.by_solid[id].iter().copied()).collect());
        }
        out
    }
}

fn solid_texlights(node: &SolidNode, value: &mut impl FnMut(&str) -> Option<glam::Vec3>) -> Vec<PreviewLight> {
    if node.disp.iter().any(Option::is_some) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for face in &node.geom.faces {
        let Some(side) = node.solid.sides.get(face.side_index) else { continue };
        let Some(v) = value(&side.material) else { continue };
        let Some(light) = face_light(&face.vertices, face.normal(), v) else { continue };
        out.push(light);
    }
    out
}

fn face_light(vertices: &[DVec3], normal: DVec3, value: glam::Vec3) -> Option<PreviewLight> {
    if vertices.len() < 3 || !normal.is_finite() {
        return None;
    }
    let center = vertices.iter().copied().sum::<DVec3>() / vertices.len() as f64;
    let mut area = DVec3::ZERO;
    for i in 1..vertices.len() - 1 {
        area += (vertices[i] - vertices[0]).cross(vertices[i + 1] - vertices[0]);
    }
    let area = (area.length() * 0.5) as f32;
    if !center.is_finite() || !area.is_finite() || area <= 0.0 {
        return None;
    }
    let n = Vec3A::from(normal.as_vec3());
    let origin = Vec3A::from(center.as_vec3()) + n;
    let vrad = hp_light::lights::surface_light(origin, n, Vec3A::from(value), area, 1.0);
    let strength = vrad.intensity.max_element();
    if !strength.is_finite() || strength <= 0.0 {
        return None;
    }
    Some(PreviewLight {
        kind: LightKind::Spot { dir: n.to_array(), inner_deg: 0.0, outer_deg: 90.0 },
        pos: origin.to_array(),
        color: (vrad.intensity / strength).to_array(),
        intensity: strength / 255.0,
        range: vrad.reach.clamp(MIN_RANGE, MAX_RANGE),
        falloff: Some(LightFalloff {
            constant: vrad.constant,
            linear: vrad.linear,
            quadratic: vrad.quadratic,
            cap: vrad.cap,
            cutoff: vrad.cutoff,
            fade: [vrad.fade.0, vrad.fade.1],
            exponent: 0.0,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(class: &str, props: &[(&str, &str)]) -> Entity {
        let mut e = Entity { classname: class.to_owned(), ..Default::default() };
        for (k, v) in props {
            e.set(k, *v);
        }
        e
    }

    #[test]
    fn light_values() {
        let v = parse_light_value("255 255 255 200").unwrap();
        assert_eq!(v.linear, [1.0; 3]);
        assert!((v.intensity - 200.0 / 255.0).abs() < 1e-6);
        let v = parse_light_value("128").unwrap();
        assert_eq!(v.intensity, 1.0);
        assert!((v.linear[0] - (128.0f64 / 255.0).powf(2.2) as f32).abs() < 1e-6);
        assert_eq!(parse_light_value("-1 -1 -1 1"), None);
        assert_eq!(parse_light_value("1 2"), None);
        assert_eq!(parse_light_value(""), None);
    }

    #[test]
    fn lights() {
        let point = ent("light", &[("origin", "10 20 30"), ("_light", "255 255 255 255")]);
        let (l, amb, _) = parse_light(&point);
        let l = l.unwrap();
        assert_eq!(l.kind, LightKind::Point);
        assert_eq!(l.pos, [10.0, 20.0, 30.0]);
        assert_eq!(l.range, 16384.0);
        assert!(amb.is_none());
        let at = |l: &PreviewLight, d: f32| {
            let f = l.falloff.expect("VRAD terms");
            l.intensity / (f.constant + d * (f.linear + d * f.quadratic))
        };
        let quad = ent("light", &[("_light", "255 255 255 200"), ("_quadratic_attn", "1")]);
        let q = parse_light(&quad).0.unwrap();
        assert!((at(&q, 100.0) - 200.0 / 255.0).abs() < 1e-4, "{}", at(&q, 100.0));
        assert!((at(&q, 200.0) * 4.0 - 200.0 / 255.0).abs() < 1e-4);
        let lin = ent("light", &[("_light", "255 255 255 200"), ("_linear_attn", "1")]);
        let l = parse_light(&lin).0.unwrap();
        assert!((at(&l, 100.0) - 2.0 * at(&l, 200.0)).abs() < 1e-4, "linear falloff");
        assert!(l.falloff.is_some_and(|f| f.quadratic == 0.0 && f.linear > 0.0));
        let sun = ent("light_environment", &[("_light", "255 255 255 200")]);
        assert!(parse_light(&sun).0.is_some_and(|s| s.falloff.is_none()));
        assert!((q.range - 2000.0).abs() < 1.0, "{}", q.range);
        let dark = ent("light", &[("_light", "255 255 255 200"), ("targetname", "a"), ("spawnflags", "1")]);
        assert!(parse_light(&dark).0.is_none());

        let hdr_only = ent("light", &[("_lightHDR", "255 0 0 100")]);
        assert!(parse_light(&hdr_only).0.is_some());
        let unset = ent("light", &[("_light", "-1 -1 -1 1")]);
        assert!(parse_light(&unset).0.is_none());

        let spot = ent(
            "light_spot",
            &[("_light", "255 255 255 200"), ("pitch", "-90"), ("_cone", "60"), ("_inner_cone", "70")],
        );
        let Some(PreviewLight { kind: LightKind::Spot { dir, inner_deg, outer_deg }, .. }) = parse_light(&spot).0
        else {
            panic!("spot expected");
        };
        assert!((dir[2] + 1.0).abs() < 1e-5, "{dir:?}");
        assert!((inner_deg - 70.0).abs() < 1e-3 && (outer_deg - 70.0).abs() < 1e-3, "{inner_deg} {outer_deg}");

        let sun = ent(
            "light_environment",
            &[("_light", "255 255 255 300"), ("_ambient", "255 255 255 20"), ("angles", "-45 90 0"), ("pitch", "0")],
        );
        let (l, amb, _) = parse_light(&sun);
        let Some(PreviewLight { kind: LightKind::Directional { dir }, .. }) = l else { panic!("sun expected") };
        assert!(dir[2] < -0.7 && dir[1] > 0.7, "{dir:?}");
        let amb = amb.unwrap();
        assert!((amb[0] - 20.0 / 255.0).abs() < 1e-5);

        assert!(parse_light(&ent("info_target", &[("_light", "255 255 255 200")])).0.is_none());
        let down = ent("light_spot", &[("angle", "-2"), ("_light", "1 1 1")]);
        assert_eq!(light_direction(&down), DVec3::NEG_Z);
    }

    #[test]
    fn spot_cones_follow_vrad() {
        let spot = ent(
            "light_spot",
            &[
                ("_light", "255 255 255 200"),
                ("pitch", "-90"),
                ("_cone", "60"),
                ("_inner_cone", "30"),
                ("_exponent", "2"),
            ],
        );
        let l = parse_light(&spot).0.expect("spot");
        let LightKind::Spot { inner_deg, outer_deg, .. } = l.kind else { panic!("spot expected") };
        assert!((inner_deg - 30.0).abs() < 1e-3 && (outer_deg - 60.0).abs() < 1e-3, "{inner_deg} {outer_deg}");
        assert_eq!(l.falloff.map(|f| f.exponent), Some(2.0));
        let wide = ent("light_spot", &[("_light", "255 255 255 200"), ("_cone", "100"), ("_inner_cone", "95")]);
        let LightKind::Spot { outer_deg, .. } = parse_light(&wide).0.expect("spot").kind else { panic!("spot") };
        assert!((outer_deg - 100.0).abs() < 1e-3, "{outer_deg}");
        let point = ent("light", &[("_light", "255 255 255 200"), ("_quadratic_attn", "1")]);
        assert_eq!(parse_light(&point).0.and_then(|l| l.falloff).map(|f| f.exponent), Some(0.0));
    }

    #[test]
    fn dynamic_lights() {
        let e = ent(
            "light_dynamic",
            &[("origin", "0 0 100"), ("_light", "255 128 0 200"), ("brightness", "2"), ("distance", "300")],
        );
        let (l, amb, srgb) = parse_light(&e);
        let l = l.expect("dynamic light");
        assert!(amb.is_none());
        assert_eq!(srgb, Some([1.0, 128.0 / 255.0, 0.0]));
        assert_eq!(l.kind, LightKind::Point);
        assert!((l.intensity - 4.0).abs() < 1e-5, "{}", l.intensity);
        assert!((l.color[1] - 128.0 / 255.0).abs() < 1e-5);
        assert_eq!(l.range, 300.0);
        let f = l.falloff.expect("falloff");
        assert_eq!(f.cutoff, 300.0);
        let at = |d: f32| l.intensity / (f.quadratic * d * d);
        assert!((at(300.0) - 4.0 * DLIGHT_MIN_LIGHT).abs() < 1e-4, "{}", at(300.0));
        assert!(at(100.0) > at(200.0) * 3.9);
        let spot = ent(
            "light_dynamic",
            &[("_light", "255 255 255"), ("pitch", "-90"), ("_inner_cone", "20"), ("_cone", "40"), ("distance", "200")],
        );
        let LightKind::Spot { dir, inner_deg, outer_deg } = parse_light(&spot).0.expect("spot").kind else {
            panic!("spot expected")
        };
        assert!((dir[2] + 1.0).abs() < 1e-5, "{dir:?}");
        assert_eq!((inner_deg, outer_deg), (20.0, 40.0));
        let angled = ent("light_dynamic", &[("angles", "45 90 0"), ("_cone", "30")]);
        let LightKind::Spot { dir, .. } = parse_light(&angled).0.expect("spot").kind else { panic!("spot") };
        assert!(dir[2] < -0.7 && dir[1] > 0.7, "{dir:?}");
        assert!(parse_light(&ent("light_dynamic", &[("distance", "0")])).0.is_none());
        assert!(parse_light(&ent("light_dynamic", &[("spawnflags", "3")])).0.is_none());
        assert!(parse_light(&ent("light_dynamic", &[("_light", "0 0 0")])).0.is_none());
        assert!(parse_light(&ent("light_dynamic", &[("_light", "nan 1 1"), ("brightness", "inf")])).0.is_none());
    }

    #[test]
    fn shadows_follow_disableshadows() {
        assert!(!casts_no_shadow(&ent("prop_static", &[])));
        assert!(!casts_no_shadow(&ent("prop_static", &[("disableshadows", "0")])));
        assert!(casts_no_shadow(&ent("prop_static", &[("disableshadows", "1")])));
        assert!(casts_no_shadow(&ent("prop_physics", &[("disableshadows", " 1 ")])));
        assert!(!casts_no_shadow(&ent("prop_dynamic", &[("disableshadows", "x")])));
    }

    #[test]
    fn texlight_faces_become_preview_lights() {
        let square = [DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0), DVec3::new(64.0, 64.0, 0.0), DVec3::new(0.0, 64.0, 0.0)];
        let l = face_light(&square, DVec3::Z, glam::Vec3::new(200.0, 100.0, 50.0)).expect("light");
        assert_eq!(l.pos, [32.0, 32.0, 1.0]);
        assert!(matches!(l.kind, LightKind::Spot { dir: [0.0, 0.0, 1.0], .. }));
        assert_eq!(l.color, [1.0, 0.5, 0.25]);
        let expected = 200.0 * 64.0 * 64.0 / std::f32::consts::PI / 255.0;
        assert!((l.intensity - expected).abs() < expected * 1e-4, "{}", l.intensity);
        assert!(l.falloff.is_some_and(|f| f.quadratic == 1.0 && f.constant == 0.0));
        assert!(l.range >= MIN_RANGE && l.range <= MAX_RANGE);
        assert!(face_light(&square[..2], DVec3::Z, glam::Vec3::ONE).is_none());
        assert!(face_light(&[DVec3::ZERO; 4], DVec3::Z, glam::Vec3::ONE).is_none());
        assert!(face_light(&square, DVec3::NAN, glam::Vec3::ONE).is_none());
        assert!(face_light(&square, DVec3::Z, glam::Vec3::ZERO).is_none());
        assert!(face_light(&square, DVec3::Z, glam::Vec3::splat(f32::NAN)).is_none());
    }
}
