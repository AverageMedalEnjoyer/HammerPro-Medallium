#[doc(hidden)]
pub mod ledge;
#[doc(hidden)]
pub mod qhull;
#[doc(hidden)]
pub mod surface;
#[doc(hidden)]
pub mod vmesh;

use std::fmt::Write as _;

use glam::DVec3;
use rayon::prelude::*;

use crate::emit::Buf;

#[derive(Clone, Debug, Default)]
pub struct BrushSide {
    pub plane: [f32; 4],
    pub bevel: bool,
    pub visible: bool,
    pub surfaceprop: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ConvexSolid {
    pub sides: Vec<BrushSide>,
    pub brush: i32,
}

#[derive(Clone, Debug, Default)]
pub struct WorldExtras {
    pub statics: Vec<(u32, Vec<ConvexSolid>)>,
    pub terrain: Vec<Vec<SoupTriangle>>,
    pub virtual_terrain: bool,
    pub fluids: Vec<Fluid>,
    pub surface_props: Option<SurfaceProps>,
}

#[derive(Clone, Debug, Default)]
pub struct PhysModel {
    pub model_index: i32,
    pub solids: Vec<ConvexSolid>,
    pub size: [f32; 3],
    pub faces: Vec<(String, f32)>,
}

#[derive(Clone, Debug, Default)]
pub struct Fluid {
    pub convexes: Vec<ConvexSolid>,
    pub surfaceprop: String,
    pub damping: f32,
    pub contents: u32,
    pub surface: Option<[f32; 4]>,
}

#[derive(Clone, Debug)]
pub struct SoupTriangle {
    pub points: [DVec3; 3],
    pub surfaceprop: String,
}

const WORLD_CONTENTS: u32 = 0x0200_4003;
const MERGE: f32 = 0.01;
const SHRINK: f32 = 0.5;
const MAX_MASS: f32 = 50000.0;
const METERS_PER_INCH: f32 = 0.0254;
const CUBIC_METERS_PER_CUBIC_INCH: f32 = METERS_PER_INCH * METERS_PER_INCH * METERS_PER_INCH;
const MAX_MATERIALS: usize = 126;
const MAX_PLANES: usize = 512;

pub fn physcollide_lump(models: &[PhysModel]) -> Vec<u8> {
    physcollide_lump_world(models, &WorldExtras::default())
}

pub fn physcollide_lump_world(models: &[PhysModel], extras: &WorldExtras) -> Vec<u8> {
    let mut order: Vec<&PhysModel> = models.iter().filter(|m| m.model_index >= 0).collect();
    order.sort_by_key(|m| m.model_index);
    order.dedup_by_key(|m| m.model_index);
    let standard;
    let props = match &extras.surface_props {
        Some(p) => p,
        None => {
            standard = SurfaceProps::standard();
            &standard
        }
    };
    let records: Vec<Option<(Vec<Vec<u8>>, String)>> = order
        .par_iter()
        .map(|m| if m.model_index == 0 { Some(world_record(m, extras, props)) } else { entity_record(m, props) })
        .collect();
    let mut out = Buf::default();
    for (model, rec) in order.iter().zip(records) {
        let Some((blobs, text)) = rec else { continue };
        if blobs.is_empty() {
            continue;
        }
        let data_size: usize = blobs.iter().map(|b| b.len() + 4).sum();
        out.i32(model.model_index);
        out.i32(clamp_i32(data_size));
        out.i32(clamp_i32(text.len() + 1));
        out.i32(clamp_i32(blobs.len()));
        for blob in &blobs {
            out.i32(clamp_i32(blob.len()));
            out.0.extend_from_slice(blob);
        }
        out.0.extend_from_slice(text.as_bytes());
        out.u8(0);
    }
    out.i32(-1);
    out.i32(-1);
    out.i32(0);
    out.i32(0);
    out.0
}

pub fn lump_brush_sides(brushes: &[u8], sides: &[u8], brush: usize) -> Vec<(usize, i32, bool)> {
    let rd = |b: &[u8], o: usize| b.get(o..o + 4).map_or(0, |v| i32::from_le_bytes([v[0], v[1], v[2], v[3]]));
    let first = usize::try_from(rd(brushes, brush * 12)).unwrap_or(0);
    let count = usize::try_from(rd(brushes, brush * 12 + 4)).unwrap_or(0);
    (first..first + count)
        .filter_map(|s| sides.get(s * 8..s * 8 + 8))
        .map(|r| {
            (usize::from(u16::from_le_bytes([r[0], r[1]])), i32::from(i16::from_le_bytes([r[2], r[3]])), r[6] != 0)
        })
        .collect()
}

fn clamp_i32(v: usize) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

fn hull(points: &[[f32; 3]]) -> Option<Vec<u8>> {
    ledge::pointsoup_to_ledge(points, &mut ledge::ivp_qhull)
}

fn convex(planes: &[[f32; 4]]) -> Option<Vec<u8>> {
    if planes.len() > MAX_PLANES || planes.iter().flatten().any(|v| !v.is_finite()) {
        return None;
    }
    ledge::convex_from_planes(planes, MERGE, &mut ledge::ivp_qhull)
}

fn brush_convex(solid: &ConvexSolid) -> Option<Vec<u8>> {
    let planes: Vec<[f32; 4]> = solid.sides.iter().filter(|s| !s.bevel).map(|s| s.plane).collect();
    let mut l = convex(&planes)?;
    set_game_data(&mut l, solid.brush);
    Some(l)
}

fn set_game_data(ledge: &mut [u8], data: i32) {
    if ledge.len() >= 8 {
        ledge[4..8].copy_from_slice(&data.to_le_bytes());
    }
}

fn shrunk_convex(solid: &ConvexSolid) -> Option<Vec<u8>> {
    let base: Vec<[f32; 4]> = solid.sides.iter().filter(|s| !s.bevel).map(|s| s.plane).collect();
    let test =
        convex(&base).and_then(|l| surface::convex_to_collide(&[l], &surface::ConvertParams::default(), &mut hull));
    let planes: Vec<[f32; 4]> = solid
        .sides
        .iter()
        .filter(|s| !s.bevel)
        .map(|s| {
            let [nx, ny, nz, d] = s.plane;
            let mut by = if s.visible { SHRINK } else { 0.0 };
            if let Some(t) = &test
                && by != 0.0
            {
                let start = surface::collide_extent(t, [nx, ny, nz]);
                let end = surface::collide_extent(t, [-nx, -ny, -nz]);
                let thick = ((end[0] - start[0]) * nx + (end[1] - start[1]) * ny) + (end[2] - start[2]) * nz;
                if thick.abs() < SHRINK * 3.0 {
                    by = 0.0;
                }
            }
            [nx, ny, nz, d - by]
        })
        .collect();
    let mut l = convex(&planes)?;
    set_game_data(&mut l, solid.brush);
    Some(l)
}

fn convexes(solids: &[ConvexSolid]) -> Vec<Vec<u8>> {
    solids.par_iter().map(brush_convex).collect::<Vec<_>>().into_iter().flatten().collect()
}

fn world_record(model: &PhysModel, extras: &WorldExtras, props: &SurfaceProps) -> (Vec<Vec<u8>>, String) {
    let mut groups: Vec<(u32, &[ConvexSolid])> = vec![(WORLD_CONTENTS, &model.solids)];
    groups.extend(extras.statics.iter().map(|(c, s)| (*c, s.as_slice())));
    let statics: Vec<Option<Vec<u8>>> = groups
        .par_iter()
        .map(|(_, solids)| {
            let ledges = convexes(solids);
            if ledges.is_empty() {
                None
            } else {
                surface::convex_to_collide(&ledges, &surface::ConvertParams::default(), &mut hull)
            }
        })
        .collect();
    let mut table: Vec<Option<usize>> = Vec::new();
    let mut blobs: Vec<Vec<u8>> = Vec::new();
    let mut text = String::new();
    for ((contents, solids), collide) in groups.iter().zip(statics) {
        let Some(mut collide) = collide else { continue };
        world_materials(&mut collide, solids, props, &mut table);
        let _ =
            write!(text, "staticsolid {{\n\"index\" \"{}\"\n\"contents\" \"{}\"\n}}\n", blobs.len(), *contents as i32);
        blobs.push(collide);
    }
    let soups: Vec<Vec<([[f32; 3]; 3], i32)>> = extras
        .terrain
        .iter()
        .map(|soup| {
            soup.iter()
                .map(|t| {
                    (
                        t.points.map(|p| [p.x as f32, p.y as f32, p.z as f32]),
                        i32::from(table_index(&mut table, props.find(&t.surfaceprop))),
                    )
                })
                .collect()
        })
        .collect();
    let soups: Vec<Option<Vec<u8>>> =
        soups.par_iter().map(|tris| surface::polysoup_to_collide(tris, &mut hull)).collect();
    for collide in soups.into_iter().flatten() {
        let _ = write!(text, "staticsolid {{\n\"index\" \"{}\"\n}}\n", blobs.len());
        blobs.push(collide);
    }
    let fluids: Vec<Option<Vec<u8>>> = extras
        .fluids
        .par_iter()
        .map(|f| {
            let ledges = convexes(&f.convexes);
            if ledges.is_empty() {
                None
            } else {
                surface::convex_to_collide(&ledges, &surface::ConvertParams::default(), &mut hull)
            }
        })
        .collect();
    for (f, collide) in extras.fluids.iter().zip(fluids) {
        let Some(collide) = collide else { continue };
        let plane = f.surface.unwrap_or_else(|| [0.0, 0.0, 1.0, surface::collide_extent(&collide, [0.0, 0.0, 1.0])[2]]);
        text.push_str(&fluid_text(f, plane, blobs.len()));
        blobs.push(collide);
    }
    if extras.virtual_terrain {
        text.push_str("virtualterrain {}\n");
    }
    if !table.is_empty() {
        text.push_str("materialtable {\n");
        for (i, prop) in table.iter().enumerate() {
            let _ = writeln!(text, "\"{}\" \"{}\"", props.name(*prop), i + 1);
        }
        text.push_str("}\n");
    }
    (blobs, text)
}

fn world_materials(collide: &mut [u8], solids: &[ConvexSolid], props: &SurfaceProps, table: &mut Vec<Option<usize>>) {
    for (at, side) in world_triangle_sides(collide, solids) {
        if let Some(prop) = side.surfaceprop.as_deref() {
            let m = u32::from(table_index(table, props.find(prop)) & 0x7f) << 24;
            let w = (u32::from_le_bytes([collide[at], collide[at + 1], collide[at + 2], collide[at + 3]])
                & 0x80ff_ffff)
                | m;
            collide[at..at + 4].copy_from_slice(&w.to_le_bytes());
        }
    }
}

fn world_triangle_sides<'a>(collide: &[u8], solids: &'a [ConvexSolid]) -> Vec<(usize, &'a BrushSide)> {
    const K: f32 = 39.370_08;
    let by_brush: ahash::AHashMap<i32, &ConvexSolid> = solids.iter().map(|s| (s.brush, s)).collect();
    let mut out = Vec::new();
    if collide.len() < 28 + 48 {
        return out;
    }
    let s = &collide[28..];
    let rd = |o: usize| s.get(o..o + 4).map_or(0, |v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]));
    for lo in surface::surface_ledges(s) {
        let Some(solid) = by_brush.get(&(rd(lo + 4) as i32)) else { continue };
        let points = (lo as i64 + i64::from(rd(lo) as i32)) as usize;
        let ntris = (rd(lo + 12) as u16 as i16).max(0) as usize;
        for t in 0..ntris {
            let to = lo + 16 * (t + 1);
            let v = [3usize, 2, 1].map(|e| {
                let p = points + 16 * (rd(to + 4 * e) & 0xffff) as usize;
                let [x, y, z] = [0, 4, 8].map(|k| f32::from_bits(rd(p + k)));
                [K * x, K * z, -(K * y)]
            });
            let n = triangle_normal(v);
            let mut best = -1.0f32;
            let mut side = None;
            for sd in &solid.sides {
                let d = (n[0] * sd.plane[0] + n[1] * sd.plane[1]) + n[2] * sd.plane[2];
                if d > best {
                    best = d;
                    side = Some(sd);
                }
            }
            if let Some(sd) = side {
                out.push((28 + to, sd));
            }
        }
    }
    out
}

fn triangle_normal(p: [[f32; 3]; 3]) -> [f32; 3] {
    let e0 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
    let e1 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
    let n = [e1[1] * e0[2] - e1[2] * e0[1], e1[2] * e0[0] - e1[0] * e0[2], e1[0] * e0[1] - e1[1] * e0[0]];
    let len = ((n[0] * n[0] + n[1] * n[1]) + n[2] * n[2]).sqrt();
    if len > 0.0 {
        let inv = 1.0 / len;
        [n[0] * inv, n[1] * inv, n[2] * inv]
    } else {
        [0.0, 0.0, 0.0]
    }
}

fn table_index(table: &mut Vec<Option<usize>>, prop: Option<usize>) -> u8 {
    let at = match table.iter().position(|t| *t == prop) {
        Some(a) => a,
        None => {
            if table.len() >= MAX_MATERIALS {
                return 0;
            }
            table.push(prop);
            table.len() - 1
        }
    };
    (at + 1) as u8
}

fn key_char(c: char) -> bool {
    !c.is_control() && c != '"' && c != '{' && c != '}'
}

fn fluid_text(f: &Fluid, plane: [f32; 4], index: usize) -> String {
    let name =
        if !f.surfaceprop.is_empty() && f.surfaceprop.chars().all(key_char) { f.surfaceprop.as_str() } else { "water" };
    let mut s = format!(
        "fluid {{\n\"index\" \"{index}\"\n\"surfaceprop\" \"{name}\"\n\"damping\" \"{:.6}\"\n\"contents\" \"{}\"\n\"surfaceplane\" \"",
        f.damping, f.contents as i32
    );
    for v in plane {
        let _ = write!(s, "{v:.6} ");
    }
    s.push_str("\"\n\"currentvelocity\" \"0.000000 0.000000 0.000000 \"\n}\n");
    s
}

fn entity_record(model: &PhysModel, props: &SurfaceProps) -> Option<(Vec<Vec<u8>>, String)> {
    let ledges: Vec<Vec<u8>> =
        model.solids.par_iter().map(shrunk_convex).collect::<Vec<_>>().into_iter().flatten().collect();
    if ledges.is_empty() {
        return None;
    }
    let mut total_volume = 0.0f32;
    for l in &ledges {
        total_volume += surface::convex_volume(l);
    }
    let params = surface::ConvertParams::vbsp_brush_entity(ledges.len(), model.size);
    let collide = surface::convex_to_collide(&ledges, &params, &mut hull)?;
    let volume = surface::collide_volume(&collide);
    let mut areas: Vec<(Option<usize>, f32)> = vec![(None, 1.0)];
    let first_side = || {
        model
            .solids
            .first()
            .and_then(|s| s.sides.iter().find(|x| !x.bevel))
            .map(|x| x.surfaceprop.clone().unwrap_or_default())
    };
    let faces: Vec<(String, f32)> = if model.faces.is_empty() {
        first_side().map(|p| vec![(p, 2.0)]).unwrap_or_default()
    } else {
        model.faces.clone()
    };
    for (name, area) in faces {
        let prop = props.find(&name);
        match areas.iter_mut().find(|(p, _)| *p == prop) {
            Some(e) => e.1 += area,
            None => {
                if areas.len() < 256 {
                    areas.push((prop, area));
                }
            }
        }
    }
    let mut max_index = None;
    let mut max_area = 0.0f32;
    let mut total_area = 0.0f32;
    for (i, (_, area)) in areas.iter().enumerate() {
        if *area > max_area {
            max_index = Some(i);
            max_area = *area;
        }
        total_area += *area;
    }
    let mut mass = 1.0f32;
    let mut material = "default";
    if let Some(i) = max_index {
        let prop = areas[i].0.unwrap_or(0);
        material = props.name(Some(prop));
        let (density, thickness) = props.physics(prop);
        mass = if thickness != 0.0 {
            total_area * thickness * density * CUBIC_METERS_PER_CUBIC_INCH
        } else {
            total_volume * density * CUBIC_METERS_PER_CUBIC_INCH
        };
    }
    if mass > MAX_MASS {
        mass = MAX_MASS;
    }
    let mut text =
        format!("solid {{\n\"index\" \"0\"\n\"mass\" \"{:.6}\"\n\"surfaceprop\" \"{material}\"\n", f64::from(mass));
    if volume != 0.0 {
        let _ = writeln!(text, "\"volume\" \"{:.6}\"", f64::from(volume));
    }
    text.push_str("}\n");
    Some((vec![collide], text))
}

#[derive(Clone, Debug, Default)]
pub struct SurfaceProps {
    entries: Vec<(String, f32, f32)>,
}

impl SurfaceProps {
    pub fn load(read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Option<SurfaceProps> {
        let manifest = hp_kv::parse_bytes(&read("scripts/surfaceproperties_manifest.txt")?).ok()?;
        let mut props = SurfaceProps::default();
        let mut seen: Vec<String> = Vec::new();
        for block in &manifest.children {
            for (k, file) in &block.props {
                if !k.eq_ignore_ascii_case("file") || seen.iter().any(|f| f.eq_ignore_ascii_case(file)) {
                    continue;
                }
                seen.push(file.clone());
                if let Some(text) = read(file) {
                    props.parse(&text);
                }
            }
        }
        (!props.entries.is_empty()).then_some(props)
    }

    pub fn parse(&mut self, text: &[u8]) {
        let Ok(root) = hp_kv::parse_bytes(text) else { return };
        let first = self.entries.is_empty();
        for block in &root.children {
            let at = match self.index(&block.name) {
                Some(i) => i,
                None => {
                    let (d, t) = self.physics_of("default");
                    self.entries.push((block.name.to_ascii_lowercase(), d, t));
                    self.entries.len() - 1
                }
            };
            for (k, v) in &block.props {
                if k.eq_ignore_ascii_case("base") {
                    if let Some(b) = self.index(v) {
                        (self.entries[at].1, self.entries[at].2) = (self.entries[b].1, self.entries[b].2);
                    }
                } else if k.eq_ignore_ascii_case("density") {
                    self.entries[at].1 = atof(v);
                } else if k.eq_ignore_ascii_case("thickness") {
                    self.entries[at].2 = atof(v);
                }
            }
        }
        if first && !self.entries.is_empty() {
            let (d, t) = self.physics_of("default");
            self.entries.push(("$MATERIAL_INDEX_SHADOW".to_owned(), d, t));
        }
    }

    pub fn standard() -> SurfaceProps {
        SurfaceProps { entries: STANDARD_PROPS.iter().map(|&(n, d, t)| (n.to_owned(), d, t)).collect() }
    }

    fn index(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.0.eq_ignore_ascii_case(name))
    }

    fn physics_of(&self, name: &str) -> (f32, f32) {
        self.index(name).map_or((0.0, 0.0), |i| (self.entries[i].1, self.entries[i].2))
    }

    fn find(&self, surfaceprop: &str) -> Option<usize> {
        if surfaceprop.is_empty() {
            return None;
        }
        self.index(surfaceprop).or_else(|| self.index("default"))
    }

    fn name(&self, prop: Option<usize>) -> &str {
        match prop.and_then(|i| self.entries.get(i)) {
            Some(e) if !e.0.is_empty() && e.0.chars().all(key_char) => &e.0,
            _ => "default",
        }
    }

    fn physics(&self, prop: usize) -> (f32, f32) {
        self.entries.get(prop).map_or((0.0, 0.0), |e| (e.1, e.2))
    }
}

fn atof(s: &str) -> f32 {
    let s = s.trim_start();
    let mut best = 0.0f64;
    for end in (1..=s.len().min(64)).filter(|&e| s.is_char_boundary(e)) {
        if let Ok(v) = s[..end].parse::<f64>() {
            best = v;
        }
    }
    best as f32
}

const STANDARD_PROPS: &[(&str, f32, f32)] = &[
    ("default", 2000.0, 0.0),
    ("solidmetal", 2700.0, 0.0),
    ("metal_box", 2700.0, 0.1),
    ("metal", 2700.0, 0.1),
    ("metal_bouncy", 10000.0, 0.0),
    ("slipperymetal", 2700.0, 0.1),
    ("metalgrate", 1600.0, 0.5),
    ("metalvent", 2700.0, 0.04),
    ("metalpanel", 2700.0, 0.1),
    ("dirt", 1600.0, 0.0),
    ("mud", 1600.0, 0.0),
    ("slipperyslime", 1600.0, 0.0),
    ("grass", 1600.0, 0.0),
    ("tile", 2700.0, 0.5),
    ("wood", 700.0, 0.0),
    ("wood_lowdensity", 300.0, 0.0),
    ("wood_box", 700.0, 0.0),
    ("wood_crate", 700.0, 0.0),
    ("wood_plank", 700.0, 0.0),
    ("wood_solid", 700.0, 0.0),
    ("wood_furniture", 700.0, 0.0),
    ("wood_panel", 700.0, 1.0),
    ("water", 1000.0, 0.0),
    ("slime", 2000.0, 0.0),
    ("quicksand", 600.0, 0.0),
    ("wade", 1000.0, 0.0),
    ("ladder", 2700.0, 0.1),
    ("woodladder", 700.0, 0.0),
    ("glass", 2700.0, 0.5),
    ("computer", 2700.0, 0.1),
    ("concrete", 2400.0, 0.0),
    ("rock", 2400.0, 0.0),
    ("porcelain", 2400.0, 0.0),
    ("boulder", 2400.0, 0.0),
    ("gravel", 2400.0, 0.0),
    ("brick", 2400.0, 0.0),
    ("concrete_block", 2400.0, 0.0),
    ("chainlink", 1600.0, 0.5),
    ("chain", 1600.0, 0.5),
    ("flesh", 900.0, 0.0),
    ("bloodyflesh", 900.0, 0.0),
    ("alienflesh", 900.0, 0.0),
    ("armorflesh", 900.0, 0.0),
    ("watermelon", 900.0, 0.0),
    ("snow", 800.0, 0.0),
    ("ice", 917.0, 0.0),
    ("carpet", 500.0, 0.1),
    ("plaster", 1600.0, 0.0),
    ("cardboard", 500.0, 0.25),
    ("plastic_barrel", 500.0, 0.25),
    ("plastic_box", 500.0, 0.25),
    ("plastic", 500.0, 0.25),
    ("item", 600.0, 0.25),
    ("floatingstandable", 800.0, 0.0),
    ("sand", 1600.0, 0.0),
    ("rubber", 1600.0, 0.0),
    ("rubbertire", 1600.0, 0.0),
    ("jeeptire", 1600.0, 0.0),
    ("slidingrubbertire", 1600.0, 0.0),
    ("brakingrubbertire", 1600.0, 0.0),
    ("slidingrubbertire_front", 1600.0, 0.0),
    ("slidingrubbertire_rear", 1600.0, 0.0),
    ("glassbottle", 2700.0, 0.5),
    ("pottery", 2700.0, 0.5),
    ("grenade", 2700.0, 0.1),
    ("canister", 2700.0, 0.1),
    ("metal_barrel", 2700.0, 0.1),
    ("floating_metal_barrel", 500.0, 0.1),
    ("plastic_barrel_buoyant", 150.0, 0.25),
    ("roller", 2700.0, 0.1),
    ("popcan", 2700.0, 0.1),
    ("paintcan", 2700.0, 0.1),
    ("paper", 500.0, 0.25),
    ("papercup", 500.0, 0.25),
    ("ceiling_tile", 500.0, 0.25),
    ("weapon", 2700.0, 0.1),
    ("default_silent", 2000.0, 0.0),
    ("player", 1000.0, 0.0),
    ("player_control_clip", 2000.0, 0.0),
    ("no_decal", 900.0, 0.0),
    ("foliage", 700.0, 0.0),
    ("metalvehicle", 2700.0, 0.1),
    ("crowbar", 2700.0, 0.1),
    ("antlionsand", 1600.0, 0.0),
    ("metal_seafloorcar", 2700.0, 0.1),
    ("gunship", 2700.0, 0.1),
    ("strider", 2700.0, 0.1),
    ("antlion", 900.0, 0.0),
    ("combine_metal", 2700.0, 0.0),
    ("combine_glass", 2700.0, 0.5),
    ("zombieflesh", 900.0, 0.0),
    ("cavern_rock", 700.0, 0.0),
    ("hunter", 900.0, 0.0),
    ("jalopytire", 1600.0, 0.0),
    ("slidingrubbertire_jalopyfront", 1600.0, 0.0),
    ("slidingrubbertire_jalopyrear", 1600.0, 0.0),
    ("jalopy", 2700.0, 0.1),
    ("gmod_ice", 2000.0, 0.0),
    ("gmod_bouncy", 10000.0, 0.0),
    ("gmod_silent", 2000.0, 0.0),
    ("stone", 2400.0, 0.0),
    ("phx_tire_normal", 15000.0, 0.0),
    ("gm_ps_egg", 50.0, 0.0),
    ("phx_explosiveball", 10000.0, 0.0),
    ("phx_flakshell", 10000.0, 0.0),
    ("phx_ww2bomb", 10000.0, 0.1),
    ("gm_torpedo", 600.0, 0.1),
    ("gm_ps_woodentire", 550.0, 0.0),
    ("gm_ps_metaltire", 2500.0, 0.0),
    ("gm_ps_soccerball", 4500.0, 0.0),
    ("phx_rubbertire", 1600.0, 0.0),
    ("phx_rubbertire2", 1600.0, 0.0),
    ("brass_bell_large", 2000.0, 0.0),
    ("brass_bell_medium", 2000.0, 0.0),
    ("brass_bell_small", 2000.0, 0.0),
    ("brass_bell_smallest", 2000.0, 0.0),
    ("hay", 700.0, 0.0),
    ("metal_barrellight_hl", 690.0, 0.1),
    ("metal_barrel_hl", 940.0, 0.1),
    ("plastic_barrel_verybuoyant", 630.0, 0.25),
    ("friction_00", 917.0, 0.0),
    ("friction_10", 917.0, 0.0),
    ("friction_25", 917.0, 0.0),
    ("frictionm_00", 917.0, 0.1),
    ("frictionm_10", 917.0, 0.1),
    ("frictionm_25", 917.0, 0.1),
    ("$MATERIAL_INDEX_SHADOW", 2000.0, 0.0),
];

#[cfg(test)]
mod tests;
