#![allow(dead_code)]

pub mod lumps;
pub mod maps;

use glam::DVec3;
use hp_bspc::materials::{MaterialFacts, facts_by_name, facts_from_vmt};

#[derive(Clone, Debug)]
pub struct Side {
    pub points: [DVec3; 3],
    pub material: String,
    pub uaxis: String,
    pub vaxis: String,
    pub lightmap_scale: u32,
    pub disp: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Solid {
    pub sides: Vec<Side>,
}

#[derive(Clone, Debug, Default)]
pub struct Ent {
    pub class: String,
    pub keys: Vec<(String, String)>,
    pub solids: Vec<Solid>,
}

#[derive(Clone, Debug, Default)]
pub struct Vmf {
    pub world: Vec<Solid>,
    pub entities: Vec<Ent>,
}

pub fn num(x: f64) -> String {
    if x == x.trunc() && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        let s = format!("{x:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

fn point(p: DVec3) -> String {
    format!("({} {} {})", num(p.x), num(p.y), num(p.z))
}

pub fn world_axes(n: DVec3) -> (String, String) {
    let a = n.abs();
    if a.z >= a.x && a.z >= a.y {
        ("[1 0 0 0] 0.25".into(), "[0 -1 0 0] 0.25".into())
    } else if a.x >= a.y {
        ("[0 1 0 0] 0.25".into(), "[0 0 -1 0] 0.25".into())
    } else {
        ("[1 0 0 0] 0.25".into(), "[0 0 -1 0] 0.25".into())
    }
}

impl Side {
    pub fn through(p: [DVec3; 3], inside: DVec3, material: &str) -> Side {
        let mut p = p;
        let n = (p[0] - p[1]).cross(p[2] - p[1]);
        if n.dot(inside - p[1]) > 0.0 {
            p.swap(0, 2);
        }
        let n = (p[0] - p[1]).cross(p[2] - p[1]).normalize();
        let (uaxis, vaxis) = world_axes(n);
        Side { points: p, material: material.to_owned(), uaxis, vaxis, lightmap_scale: 16, disp: None }
    }
}

pub fn block(lo: DVec3, hi: DVec3, material: &str) -> Solid {
    block_sides(lo, hi, [material; 6])
}

pub fn block_sides(lo: DVec3, hi: DVec3, mats: [&str; 6]) -> Solid {
    let (a, b) = (lo, hi);
    let v = DVec3::new;
    let pts = [
        [v(a.x, b.y, b.z), v(b.x, b.y, b.z), v(b.x, a.y, b.z)],
        [v(a.x, a.y, a.z), v(b.x, a.y, a.z), v(b.x, b.y, a.z)],
        [v(a.x, b.y, b.z), v(a.x, a.y, b.z), v(a.x, a.y, a.z)],
        [v(b.x, b.y, a.z), v(b.x, a.y, a.z), v(b.x, a.y, b.z)],
        [v(b.x, b.y, b.z), v(a.x, b.y, b.z), v(a.x, b.y, a.z)],
        [v(b.x, a.y, a.z), v(a.x, a.y, a.z), v(a.x, a.y, b.z)],
    ];
    let c = (lo + hi) * 0.5;
    Solid { sides: pts.iter().zip(mats).map(|(p, m)| Side::through(*p, c, m)).collect() }
}

pub fn convex(planes: &[[DVec3; 3]], inside: DVec3, material: &str) -> Solid {
    Solid { sides: planes.iter().map(|p| Side::through(*p, inside, material)).collect() }
}

pub fn room(lo: DVec3, hi: DVec3, material: &str) -> Vec<Solid> {
    let t = 16.0;
    let v = DVec3::new;
    vec![
        block(v(lo.x - t, lo.y - t, lo.z - t), v(hi.x + t, hi.y + t, lo.z), material),
        block(v(lo.x - t, lo.y - t, hi.z), v(hi.x + t, hi.y + t, hi.z + t), material),
        block(v(lo.x - t, lo.y - t, lo.z), v(lo.x, hi.y + t, hi.z), material),
        block(v(hi.x, lo.y - t, lo.z), v(hi.x + t, hi.y + t, hi.z), material),
        block(v(lo.x, lo.y - t, lo.z), v(hi.x, lo.y, hi.z), material),
        block(v(lo.x, hi.y, lo.z), v(hi.x, hi.y + t, hi.z), material),
    ]
}

pub fn disp_block(power: u32, start: DVec3, seed: f64) -> String {
    let n = (1usize << power) + 1;
    let rows = |f: &dyn Fn(usize, usize) -> String| -> String {
        (0..n)
            .map(|r| format!("\t\t\t\t\t\"row{r}\" \"{}\"\n", (0..n).map(|c| f(r, c)).collect::<Vec<_>>().join(" ")))
            .collect()
    };
    let dist = |r: usize, c: usize| {
        let (x, y) = (r as f64 / (n - 1) as f64, c as f64 / (n - 1) as f64);
        let h = 24.0 * (x * std::f64::consts::PI).sin() * (y * std::f64::consts::PI).sin() + seed * (x - y);
        num((h * 1000.0).round() / 1000.0 + 0.125)
    };
    let tris = 1usize << power;
    let tag_rows: String =
        (0..tris).map(|r| format!("\t\t\t\t\t\"row{r}\" \"{}\"\n", vec!["9"; 2 * tris].join(" "))).collect();
    format!(
        "\t\t\tdispinfo\n\t\t\t{{\n\t\t\t\t\"power\" \"{power}\"\n\t\t\t\t\"startposition\" \"[{} {} {}]\"\n\t\t\t\t\"flags\" \"0\"\n\t\t\t\t\"elevation\" \"0\"\n\t\t\t\t\"subdiv\" \"0\"\n\
         \t\t\t\tnormals\n\t\t\t\t{{\n{}\t\t\t\t}}\n\
         \t\t\t\tdistances\n\t\t\t\t{{\n{}\t\t\t\t}}\n\
         \t\t\t\toffsets\n\t\t\t\t{{\n{}\t\t\t\t}}\n\
         \t\t\t\toffset_normals\n\t\t\t\t{{\n{}\t\t\t\t}}\n\
         \t\t\t\talphas\n\t\t\t\t{{\n{}\t\t\t\t}}\n\
         \t\t\t\ttriangle_tags\n\t\t\t\t{{\n{}\t\t\t\t}}\n\
         \t\t\t\tallowed_verts\n\t\t\t\t{{\n\t\t\t\t\t\"10\" \"-1 -1 -1 -1 -1 -1 -1 -1 -1 -1\"\n\t\t\t\t}}\n\t\t\t}}\n",
        num(start.x),
        num(start.y),
        num(start.z),
        rows(&|_, _| "0 0 1".to_owned()),
        rows(&dist),
        rows(&|_, _| "0 0 0".to_owned()),
        rows(&|_, _| "0 0 1".to_owned()),
        rows(&|r, c| if (r + c) % 3 == 0 { "255".to_owned() } else { "0".to_owned() }),
        tag_rows,
    )
}

impl Vmf {
    pub fn new() -> Vmf {
        Vmf::default()
    }

    pub fn point(&mut self, class: &str, origin: DVec3, keys: &[(&str, &str)]) -> &mut Self {
        let mut k: Vec<(String, String)> = keys.iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect();
        k.push(("origin".into(), format!("{} {} {}", num(origin.x), num(origin.y), num(origin.z))));
        self.entities.push(Ent { class: class.to_owned(), keys: k, solids: Vec::new() });
        self
    }

    pub fn brush_entity(&mut self, class: &str, keys: &[(&str, &str)], solids: Vec<Solid>) -> &mut Self {
        let k = keys.iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect();
        self.entities.push(Ent { class: class.to_owned(), keys: k, solids });
        self
    }

    pub fn text(&self) -> String {
        let mut id = 1u32;
        let mut next = || {
            id += 1;
            id
        };
        let mut s = String::from(
            "versioninfo\n{\n\t\"editorversion\" \"400\"\n\t\"editorbuild\" \"8000\"\n\t\"mapversion\" \"1\"\n\t\"formatversion\" \"100\"\n\t\"prefab\" \"0\"\n}\n\
             world\n{\n\t\"id\" \"1\"\n\t\"mapversion\" \"1\"\n\t\"classname\" \"worldspawn\"\n\t\"skyname\" \"sky_day01_01\"\n",
        );
        let solid_text = |solid: &Solid, next: &mut dyn FnMut() -> u32| -> String {
            let mut t = format!("\tsolid\n\t{{\n\t\t\"id\" \"{}\"\n", next());
            for side in &solid.sides {
                t += &format!(
                    "\t\tside\n\t\t{{\n\t\t\t\"id\" \"{}\"\n\t\t\t\"plane\" \"{} {} {}\"\n\t\t\t\"material\" \"{}\"\n\t\t\t\"uaxis\" \"{}\"\n\t\t\t\"vaxis\" \"{}\"\n\t\t\t\"rotation\" \"0\"\n\t\t\t\"lightmapscale\" \"{}\"\n\t\t\t\"smoothing_groups\" \"0\"\n",
                    next(),
                    point(side.points[0]),
                    point(side.points[1]),
                    point(side.points[2]),
                    side.material,
                    side.uaxis,
                    side.vaxis,
                    side.lightmap_scale
                );
                if let Some(d) = &side.disp {
                    t += d;
                }
                t += "\t\t}\n";
            }
            t + "\t}\n"
        };
        for solid in &self.world {
            s += &solid_text(solid, &mut next);
        }
        s += "}\n";
        for e in &self.entities {
            s += &format!("entity\n{{\n\t\"id\" \"{}\"\n\t\"classname\" \"{}\"\n", next(), e.class);
            for (k, v) in &e.keys {
                s += &format!("\t\"{k}\" \"{v}\"\n");
            }
            for solid in &e.solids {
                s += &solid_text(solid, &mut next);
            }
            s += "}\n";
        }
        s
    }
}

pub struct ByName;

impl hp_bspc::MaterialSource for ByName {
    fn facts(&self, name: &str) -> MaterialFacts {
        facts_by_name(name)
    }
}

struct GameMaterial {
    name: &'static str,
    shader: &'static str,
    keys: &'static [(&'static str, &'static str)],
    size: (i32, i32),
    reflectivity: [f32; 3],
}

const GAME_MATERIALS: &[GameMaterial] = &[
    GameMaterial {
        name: "dev/dev_measuregeneric01b",
        shader: "LightmappedGeneric",
        keys: &[("$basetexture", "Dev/dev_measuregeneric01b"), ("$surfaceprop", "concrete")],
        size: (128, 128),
        reflectivity: [0.11243543, 0.109838225, 0.109838225],
    },
    GameMaterial {
        name: "dev/dev_measurewall01c",
        shader: "LightmappedGeneric",
        keys: &[("$basetexture", "Dev/dev_measurewall01c"), ("$surfaceprop", "concrete")],
        size: (512, 512),
        reflectivity: [0.7735817, 0.2337758, 0.017808326],
    },
    GameMaterial {
        name: "tools/toolsnodraw",
        shader: "LightmappedGeneric",
        keys: &[("$basetexture", "Tools/toolsnodraw"), ("%compilenodraw", "1")],
        size: (64, 64),
        reflectivity: [0.9134962, 0.66987884, 0.011979198],
    },
    GameMaterial {
        name: "tools/toolsareaportal",
        shader: "LightmappedGeneric",
        keys: &[("$translucent", "1"), ("$basetexture", "Tools/toolsareaportal"), ("%compilenolight", "1")],
        size: (64, 64),
        reflectivity: [4.8105203e-6, 0.8961836, 0.70453143],
    },
    GameMaterial {
        name: "tools/toolsorigin",
        shader: "LightmappedGeneric",
        keys: &[("$basetexture", "Tools/toolsorigin"), ("%compileorigin", "1")],
        size: (64, 64),
        reflectivity: [0.5, 0.5, 0.5],
    },
    GameMaterial {
        name: "dev/dev_water2",
        shader: "Water",
        keys: &[
            ("%tooltexture", "dev/water_normal"),
            ("%compilewater", "1"),
            ("$abovewater", "1"),
            ("$envmap", "env_cubemap"),
            ("$bumpmap", "dev/water_dudv"),
            ("$normalmap", "dev/water_normal"),
            ("$surfaceprop", "water"),
            ("$bottommaterial", "dev/dev_waterbeneath2"),
        ],
        size: (256, 256),
        reflectivity: [0.2, 0.2, 0.2],
    },
    GameMaterial {
        name: "dev/dev_waterbeneath2",
        shader: "Water",
        keys: &[
            ("$abovewater", "0"),
            ("%compilewater", "1"),
            ("%tooltexture", "dev/water_normal"),
            ("$bumpmap", "dev/water_dudv"),
            ("$normalmap", "dev/water_normal"),
            ("$surfaceprop", "water"),
        ],
        size: (256, 256),
        reflectivity: [0.2, 0.2, 0.2],
    },
];

pub struct GameMats;

impl hp_bspc::MaterialSource for GameMats {
    fn facts(&self, name: &str) -> MaterialFacts {
        let lower = name.to_ascii_lowercase().replace('\\', "/");
        let Some(m) = GAME_MATERIALS.iter().find(|m| m.name == lower) else {
            let mut f = facts_by_name(name);
            f.found = false;
            f.has_surfaceprop = true;
            return f;
        };
        let key = |k: &str| m.keys.iter().find(|(a, _)| a.eq_ignore_ascii_case(k)).map(|(_, v)| (*v).to_owned());
        facts_from_vmt(m.shader, &key, Some(m.size), Some(m.reflectivity))
    }
}

pub fn compile_with(
    text: &str,
    mats: &dyn hp_bspc::MaterialSource,
    opts: &hp_bspc::CompileOptions,
) -> hp_bspc::CompileOutput {
    let map = hp_vmf::parse(text).expect("the VMF parses");
    hp_bspc::compile(&map, mats, &hp_bspc::props::NoModels, opts, &mut |_| {}).expect("the map compiles")
}

pub fn compile(text: &str) -> hp_bspc::CompileOutput {
    compile_with(text, &GameMats, &hp_bspc::CompileOptions::default())
}

pub fn fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures")
}
