use hp_bspc::emit::lump;

pub struct Lumps {
    pub data: Vec<Vec<u8>>,
}

fn rd_i32(d: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}
fn rd_u32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}
fn rd_f32(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}
fn rd_i16(d: &[u8], o: usize) -> i16 {
    i16::from_le_bytes(d[o..o + 2].try_into().unwrap())
}
fn rd_u16(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(d[o..o + 2].try_into().unwrap())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    pub normal: [f32; 3],
    pub dist: f32,
    pub kind: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    pub plane: i32,
    pub children: [i32; 2],
    pub mins: [i16; 3],
    pub maxs: [i16; 3],
    pub first_face: u16,
    pub num_faces: u16,
    pub area: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Leaf {
    pub contents: i32,
    pub cluster: i16,
    pub area: u16,
    pub flags: u16,
    pub mins: [i16; 3],
    pub maxs: [i16; 3],
    pub first_leaf_face: u16,
    pub num_leaf_faces: u16,
    pub first_leaf_brush: u16,
    pub num_leaf_brushes: u16,
    pub water_data: i16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    pub plane: u16,
    pub side: u8,
    pub on_node: u8,
    pub first_edge: i32,
    pub num_edges: i16,
    pub texinfo: i16,
    pub dispinfo: i16,
    pub fog_volume: i16,
    pub styles: [u8; 4],
    pub light_ofs: i32,
    pub area: f32,
    pub lightmap_mins: [i32; 2],
    pub lightmap_size: [i32; 2],
    pub orig_face: i32,
    pub num_prims: u16,
    pub first_prim: u16,
    pub smoothing: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexInfo {
    pub texture: [[f32; 4]; 2],
    pub lightmap: [[f32; 4]; 2],
    pub flags: i32,
    pub texdata: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TexData {
    pub reflectivity: [f32; 3],
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub view_width: i32,
    pub view_height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Brush {
    pub first_side: i32,
    pub num_sides: i32,
    pub contents: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrushSide {
    pub plane: u16,
    pub texinfo: i16,
    pub dispinfo: i16,
    pub bevel: u8,
    pub thin: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Model {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub origin: [f32; 3],
    pub head_node: i32,
    pub first_face: i32,
    pub num_faces: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub num_portals: i32,
    pub first_portal: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AreaPortal {
    pub key: u16,
    pub other_area: u16,
    pub first_vert: u16,
    pub num_verts: u16,
    pub plane: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeafWater {
    pub surface_z: f32,
    pub min_z: f32,
    pub texinfo: i16,
}

fn f3(d: &[u8], o: usize) -> [f32; 3] {
    [rd_f32(d, o), rd_f32(d, o + 4), rd_f32(d, o + 8)]
}
fn s3(d: &[u8], o: usize) -> [i16; 3] {
    [rd_i16(d, o), rd_i16(d, o + 2), rd_i16(d, o + 4)]
}

impl Lumps {
    pub fn from_bytes(bytes: &[u8]) -> Lumps {
        let bsp = hp_bsp::Bsp::from_vec(bytes.to_vec()).expect("a BSP");
        Lumps { data: (0..64).map(|i| bsp.lump(i).map(|l| l.into_owned()).unwrap_or_default()).collect() }
    }

    pub fn open(path: &std::path::Path) -> Lumps {
        Lumps::from_bytes(&std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
    }

    pub fn raw(&self, l: usize) -> &[u8] {
        &self.data[l]
    }

    fn recs(&self, l: usize, size: usize) -> impl Iterator<Item = (&[u8], usize)> {
        self.data[l].chunks_exact(size).map(|c| (c, 0))
    }

    pub fn planes(&self) -> Vec<Plane> {
        self.recs(lump::PLANES, 20)
            .map(|(d, o)| Plane { normal: f3(d, o), dist: rd_f32(d, 12), kind: rd_i32(d, 16) })
            .collect()
    }

    pub fn nodes(&self) -> Vec<Node> {
        self.recs(lump::NODES, 32)
            .map(|(d, _)| Node {
                plane: rd_i32(d, 0),
                children: [rd_i32(d, 4), rd_i32(d, 8)],
                mins: s3(d, 12),
                maxs: s3(d, 18),
                first_face: rd_u16(d, 24),
                num_faces: rd_u16(d, 26),
                area: rd_i16(d, 28),
            })
            .collect()
    }

    pub fn leaves(&self) -> Vec<Leaf> {
        self.recs(lump::LEAFS, 32)
            .map(|(d, _)| {
                let af = rd_u16(d, 6);
                Leaf {
                    contents: rd_i32(d, 0),
                    cluster: rd_i16(d, 4),
                    area: af & 0x1ff,
                    flags: af >> 9,
                    mins: s3(d, 8),
                    maxs: s3(d, 14),
                    first_leaf_face: rd_u16(d, 20),
                    num_leaf_faces: rd_u16(d, 22),
                    first_leaf_brush: rd_u16(d, 24),
                    num_leaf_brushes: rd_u16(d, 26),
                    water_data: rd_i16(d, 28),
                }
            })
            .collect()
    }

    fn faces_of(&self, l: usize) -> Vec<Face> {
        self.recs(l, 56)
            .map(|(d, _)| Face {
                plane: rd_u16(d, 0),
                side: d[2],
                on_node: d[3],
                first_edge: rd_i32(d, 4),
                num_edges: rd_i16(d, 8),
                texinfo: rd_i16(d, 10),
                dispinfo: rd_i16(d, 12),
                fog_volume: rd_i16(d, 14),
                styles: [d[16], d[17], d[18], d[19]],
                light_ofs: rd_i32(d, 20),
                area: rd_f32(d, 24),
                lightmap_mins: [rd_i32(d, 28), rd_i32(d, 32)],
                lightmap_size: [rd_i32(d, 36), rd_i32(d, 40)],
                orig_face: rd_i32(d, 44),
                num_prims: rd_u16(d, 48),
                first_prim: rd_u16(d, 50),
                smoothing: rd_u32(d, 52),
            })
            .collect()
    }

    pub fn faces(&self) -> Vec<Face> {
        self.faces_of(lump::FACES)
    }

    pub fn original_faces(&self) -> Vec<Face> {
        self.faces_of(lump::ORIGINALFACES)
    }

    pub fn vertices(&self) -> Vec<[f32; 3]> {
        self.recs(lump::VERTEXES, 12).map(|(d, _)| f3(d, 0)).collect()
    }

    pub fn edges(&self) -> Vec<[u16; 2]> {
        self.recs(lump::EDGES, 4).map(|(d, _)| [rd_u16(d, 0), rd_u16(d, 2)]).collect()
    }

    pub fn surfedges(&self) -> Vec<i32> {
        self.recs(lump::SURFEDGES, 4).map(|(d, _)| rd_i32(d, 0)).collect()
    }

    pub fn leaf_faces(&self) -> Vec<u16> {
        self.recs(lump::LEAFFACES, 2).map(|(d, _)| rd_u16(d, 0)).collect()
    }

    pub fn leaf_brushes(&self) -> Vec<u16> {
        self.recs(lump::LEAFBRUSHES, 2).map(|(d, _)| rd_u16(d, 0)).collect()
    }

    pub fn texinfos(&self) -> Vec<TexInfo> {
        self.recs(lump::TEXINFO, 72)
            .map(|(d, _)| {
                let v = |o: usize| [rd_f32(d, o), rd_f32(d, o + 4), rd_f32(d, o + 8), rd_f32(d, o + 12)];
                TexInfo {
                    texture: [v(0), v(16)],
                    lightmap: [v(32), v(48)],
                    flags: rd_i32(d, 64),
                    texdata: rd_i32(d, 68),
                }
            })
            .collect()
    }

    pub fn texdata_names(&self) -> Vec<String> {
        let table = &self.data[lump::TEXDATA_STRING_TABLE];
        let strings = &self.data[lump::TEXDATA_STRING_DATA];
        table
            .chunks_exact(4)
            .map(|c| {
                let o = i32::from_le_bytes(c.try_into().unwrap()) as usize;
                let end = strings[o..].iter().position(|&b| b == 0).map_or(strings.len(), |e| o + e);
                String::from_utf8_lossy(&strings[o..end]).into_owned()
            })
            .collect()
    }

    pub fn texdatas(&self) -> Vec<TexData> {
        let names = self.texdata_names();
        self.recs(lump::TEXDATA, 32)
            .map(|(d, _)| TexData {
                reflectivity: f3(d, 0),
                name: names.get(rd_i32(d, 12) as usize).cloned().unwrap_or_default(),
                width: rd_i32(d, 16),
                height: rd_i32(d, 20),
                view_width: rd_i32(d, 24),
                view_height: rd_i32(d, 28),
            })
            .collect()
    }

    pub fn brushes(&self) -> Vec<Brush> {
        self.recs(lump::BRUSHES, 12)
            .map(|(d, _)| Brush { first_side: rd_i32(d, 0), num_sides: rd_i32(d, 4), contents: rd_i32(d, 8) })
            .collect()
    }

    pub fn brush_sides(&self) -> Vec<BrushSide> {
        self.recs(lump::BRUSHSIDES, 8)
            .map(|(d, _)| BrushSide {
                plane: rd_u16(d, 0),
                texinfo: rd_i16(d, 2),
                dispinfo: rd_i16(d, 4),
                bevel: d[6],
                thin: d[7],
            })
            .collect()
    }

    pub fn models(&self) -> Vec<Model> {
        self.recs(lump::MODELS, 48)
            .map(|(d, _)| Model {
                mins: f3(d, 0),
                maxs: f3(d, 12),
                origin: f3(d, 24),
                head_node: rd_i32(d, 36),
                first_face: rd_i32(d, 40),
                num_faces: rd_i32(d, 44),
            })
            .collect()
    }

    pub fn areas(&self) -> Vec<Area> {
        self.recs(lump::AREAS, 8).map(|(d, _)| Area { num_portals: rd_i32(d, 0), first_portal: rd_i32(d, 4) }).collect()
    }

    pub fn area_portals(&self) -> Vec<AreaPortal> {
        self.recs(lump::AREAPORTALS, 12)
            .map(|(d, _)| AreaPortal {
                key: rd_u16(d, 0),
                other_area: rd_u16(d, 2),
                first_vert: rd_u16(d, 4),
                num_verts: rd_u16(d, 6),
                plane: rd_i32(d, 8),
            })
            .collect()
    }

    pub fn clip_portal_verts(&self) -> Vec<[f32; 3]> {
        self.recs(lump::CLIPPORTALVERTS, 12).map(|(d, _)| f3(d, 0)).collect()
    }

    pub fn leaf_water(&self) -> Vec<LeafWater> {
        self.recs(lump::LEAFWATERDATA, 12)
            .map(|(d, _)| LeafWater { surface_z: rd_f32(d, 0), min_z: rd_f32(d, 4), texinfo: rd_i16(d, 8) })
            .collect()
    }

    pub fn leaf_min_dist_to_water(&self) -> Vec<u16> {
        self.recs(lump::LEAFMINDISTTOWATER, 2).map(|(d, _)| rd_u16(d, 0)).collect()
    }

    pub fn vis_clusters(&self) -> i32 {
        let v = &self.data[lump::VISIBILITY];
        if v.len() >= 4 { rd_i32(v, 0) } else { 0 }
    }

    pub fn entities(&self) -> Vec<Vec<(String, String)>> {
        let text = String::from_utf8_lossy(&self.data[lump::ENTITIES]).trim_end_matches('\0').to_owned();
        let mut out = Vec::new();
        let mut cur: Option<Vec<(String, String)>> = None;
        for line in text.lines() {
            let line = line.trim();
            if line == "{" {
                cur = Some(Vec::new());
            } else if line == "}" {
                out.extend(cur.take());
            } else if let Some(c) = cur.as_mut() {
                let parts: Vec<&str> = line.split('"').collect();
                if parts.len() >= 5 {
                    c.push((parts[1].to_owned(), parts[3].to_owned()));
                }
            }
        }
        out
    }

    pub fn face_points(&self, f: &Face) -> Vec<[f32; 3]> {
        let (edges, surf, verts) = (self.edges(), self.surfedges(), self.vertices());
        (0..f.num_edges as i32).map(|k| verts[vertex_of(&edges, surf[(f.first_edge + k) as usize]) as usize]).collect()
    }
}

pub fn vertex_of(edges: &[[u16; 2]], se: i32) -> u16 {
    if se >= 0 { edges[se as usize][0] } else { edges[(-se) as usize][1] }
}
