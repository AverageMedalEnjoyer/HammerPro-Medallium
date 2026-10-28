use super::portals::BspVis;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderCost {
    pub faces: u32,
    pub triangles: u32,
    pub displacements: u32,
    pub props: u32,
    pub overlays: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CostLumps<'a> {
    pub leafs: &'a [u8],
    pub leaf_version: u32,
    pub leaf_faces: &'a [u8],
    pub faces: &'a [u8],
    pub texinfo: &'a [u8],
    pub dispinfo: &'a [u8],
    pub overlays: &'a [u8],
    pub static_props: &'a [u8],
}

const FACE_SIZE: usize = 56;
const TEXINFO_SIZE: usize = 72;
const DISPINFO_SIZE: usize = 176;
const OVERLAY_SIZE: usize = 352;
const OVERLAY_FACES: usize = 64;
const NOT_DRAWN: i32 = 0x2 | 0x4 | 0x80 | 0x100 | 0x200;
const HEAT_BUDGET: u64 = 400_000_000;

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn i16_at(d: &[u8], o: usize) -> Option<i16> {
    d.get(o..o + 2).map(|b| i16::from_le_bytes([b[0], b[1]]))
}

fn i32_at(d: &[u8], o: usize) -> Option<i32> {
    d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

struct Marks {
    stamp: u32,
    marks: Vec<u32>,
}

impl Marks {
    fn new(n: usize) -> Self {
        Self { stamp: 0, marks: vec![0; n] }
    }

    fn next(&mut self) {
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.marks.iter_mut().for_each(|m| *m = 0);
            self.stamp = 1;
        }
    }

    fn first(&mut self, i: usize) -> bool {
        match self.marks.get_mut(i) {
            Some(m) if *m != self.stamp => {
                *m = self.stamp;
                true
            }
            _ => false,
        }
    }

    fn seen(&self, i: usize) -> bool {
        self.marks.get(i).is_some_and(|m| *m == self.stamp)
    }
}

#[derive(Clone, Debug, Default)]
pub struct RenderCosts {
    cluster_faces: Vec<Vec<u32>>,
    cluster_props: Vec<Vec<u32>>,
    face_tris: Vec<u32>,
    face_disp: Vec<bool>,
    props: usize,
    overlay_faces: Vec<Vec<u32>>,
    heat: Vec<u32>,
}

impl RenderCosts {
    pub fn from_lumps(l: &CostLumps<'_>, vis: &BspVis) -> RenderCosts {
        let clusters = vis.cluster_count() as usize;
        let face_count = l.faces.len() / FACE_SIZE;
        let mut face_tris = vec![0u32; face_count];
        let mut face_disp = vec![false; face_count];
        for (i, f) in l.faces.chunks_exact(FACE_SIZE).enumerate() {
            let edges = i16_at(f, 8).unwrap_or(0);
            let texinfo = i16_at(f, 10).unwrap_or(-1);
            let disp = i16_at(f, 12).unwrap_or(-1);
            let flags = usize::try_from(texinfo)
                .ok()
                .and_then(|t| i32_at(l.texinfo, t.checked_mul(TEXINFO_SIZE)?.checked_add(64)?))
                .unwrap_or(0);
            if flags & NOT_DRAWN != 0 {
                continue;
            }
            if let Ok(d) = usize::try_from(disp) {
                let power = d.checked_mul(DISPINFO_SIZE).and_then(|o| i32_at(l.dispinfo, o + 20)).unwrap_or(0);
                if (2..=4).contains(&power) {
                    let side = 1u32 << power;
                    face_tris[i] = 2 * side * side;
                    face_disp[i] = true;
                }
            } else if edges >= 3 {
                face_tris[i] = u32::from(edges.unsigned_abs()) - 2;
            }
        }
        let leaf_size = if l.leaf_version == 0 { 56 } else { 32 };
        let leaf_count = l.leafs.len() / leaf_size;
        let leaf_cluster = |leaf: usize| -> Option<usize> {
            let c = i16_at(l.leafs, leaf.checked_mul(leaf_size)? + 4)?;
            usize::try_from(c).ok().filter(|c| *c < clusters)
        };
        let mut cluster_faces: Vec<Vec<u32>> = vec![Vec::new(); clusters];
        for leaf in 0..leaf_count {
            let Some(c) = leaf_cluster(leaf) else { continue };
            let base = leaf * leaf_size;
            let first = usize::from(u16_at(l.leafs, base + 20).unwrap_or(0));
            let n = usize::from(u16_at(l.leafs, base + 22).unwrap_or(0));
            for k in first..first + n {
                let Some(face) = u16_at(l.leaf_faces, k * 2) else { break };
                if face_tris.get(usize::from(face)).is_some_and(|t| *t > 0) {
                    cluster_faces[c].push(u32::from(face));
                }
            }
        }
        for list in &mut cluster_faces {
            list.sort_unstable();
            list.dedup();
        }
        let mut cluster_props: Vec<Vec<u32>> = vec![Vec::new(); clusters];
        let props = read_props(l.static_props, &mut |prop, leaf| {
            if let Some(c) = leaf_cluster(leaf) {
                cluster_props[c].push(prop);
            }
        });
        for list in &mut cluster_props {
            list.sort_unstable();
            list.dedup();
        }
        let overlay_faces: Vec<Vec<u32>> = l
            .overlays
            .chunks_exact(OVERLAY_SIZE)
            .map(|o| {
                let n = usize::from(u16_at(o, 6).unwrap_or(0) & 0x3fff).min(OVERLAY_FACES);
                (0..n).filter_map(|k| i32_at(o, 8 + k * 4).and_then(|f| u32::try_from(f).ok())).collect()
            })
            .collect();
        let mut costs =
            RenderCosts { cluster_faces, cluster_props, face_tris, face_disp, props, overlay_faces, heat: Vec::new() };
        costs.heat = costs.compute_heat(vis);
        costs
    }

    pub fn from_heat(heat: Vec<u32>) -> RenderCosts {
        RenderCosts { cluster_faces: vec![Vec::new(); heat.len()], heat, ..RenderCosts::default() }
    }

    pub fn cluster_count(&self) -> usize {
        self.cluster_faces.len()
    }

    pub fn cost(&self, visible: &[bool]) -> RenderCost {
        let mut faces = Marks::new(self.face_tris.len());
        let mut props = Marks::new(self.props);
        faces.next();
        props.next();
        self.cost_with(visible.iter().enumerate().filter(|(_, v)| **v).map(|(c, _)| c), &mut faces, &mut props)
    }

    fn cost_with(&self, clusters: impl Iterator<Item = usize>, faces: &mut Marks, props: &mut Marks) -> RenderCost {
        let mut out = RenderCost::default();
        for c in clusters {
            for &f in self.cluster_faces.get(c).map_or(&[][..], Vec::as_slice) {
                let f = f as usize;
                if faces.first(f) {
                    out.faces += 1;
                    out.triangles = out.triangles.saturating_add(self.face_tris.get(f).copied().unwrap_or(0));
                    out.displacements += u32::from(self.face_disp.get(f).copied().unwrap_or(false));
                }
            }
            for &p in self.cluster_props.get(c).map_or(&[][..], Vec::as_slice) {
                if props.first(p as usize) {
                    out.props += 1;
                }
            }
        }
        out.overlays = self.overlay_faces.iter().filter(|o| o.iter().any(|f| faces.seen(*f as usize))).count() as u32;
        out
    }

    pub fn heat(&self) -> &[u32] {
        &self.heat
    }

    fn compute_heat(&self, vis: &BspVis) -> Vec<u32> {
        let n = self.cluster_count();
        if n == 0 || !vis.has_visibility() {
            return Vec::new();
        }
        let mut work: u64 = 0;
        let mut faces = Marks::new(self.face_tris.len());
        let mut out = Vec::with_capacity(n);
        for c in 0..n {
            let Some(visible) = vis.visible_from(c as u32) else { return Vec::new() };
            work += n as u64;
            faces.next();
            let mut tris = 0u32;
            for (v, _) in visible.iter().enumerate().filter(|(_, v)| **v) {
                let list = self.cluster_faces.get(v).map_or(&[][..], Vec::as_slice);
                work += list.len() as u64 + 1;
                for &f in list {
                    if faces.first(f as usize) {
                        tris = tris.saturating_add(self.face_tris.get(f as usize).copied().unwrap_or(0));
                    }
                }
            }
            if work > HEAT_BUDGET {
                return Vec::new();
            }
            out.push(tris);
        }
        out
    }
}

fn read_props(d: &[u8], leaf: &mut dyn FnMut(u32, usize)) -> usize {
    let count = |o: usize| i32_at(d, o).and_then(|n| usize::try_from(n).ok());
    let Some(names) = count(0) else { return 0 };
    let Some(leaves_at) = names.checked_mul(128).and_then(|n| n.checked_add(4)) else { return 0 };
    let Some(leaves) = count(leaves_at) else { return 0 };
    let leaf_list = leaves_at + 4;
    let Some(props_at) = leaves.checked_mul(2).and_then(|n| n.checked_add(leaf_list)) else { return 0 };
    let Some(props) = count(props_at) else { return 0 };
    let start = props_at + 4;
    let rest = d.len().saturating_sub(start);
    if props == 0 || rest / props < 30 {
        return 0;
    }
    let size = rest / props;
    for p in 0..props {
        let base = start + p * size;
        let first = usize::from(u16_at(d, base + 26).unwrap_or(0));
        let n = usize::from(u16_at(d, base + 28).unwrap_or(0));
        for k in first..(first + n).min(leaves) {
            if let Some(l) = u16_at(d, leaf_list + k * 2) {
                leaf(p as u32, usize::from(l));
            }
        }
    }
    props
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_i16(v: &mut [u8], o: usize, x: i16) {
        v[o..o + 2].copy_from_slice(&x.to_le_bytes());
    }

    fn put_i32(v: &mut [u8], o: usize, x: i32) {
        v[o..o + 4].copy_from_slice(&x.to_le_bytes());
    }

    struct TestLumps {
        leafs: Vec<u8>,
        leaf_faces: Vec<u8>,
        faces: Vec<u8>,
        texinfo: Vec<u8>,
        dispinfo: Vec<u8>,
        overlays: Vec<u8>,
        props: Vec<u8>,
    }

    impl TestLumps {
        fn lumps(&self) -> CostLumps<'_> {
            CostLumps {
                leafs: &self.leafs,
                leaf_version: 1,
                leaf_faces: &self.leaf_faces,
                faces: &self.faces,
                texinfo: &self.texinfo,
                dispinfo: &self.dispinfo,
                overlays: &self.overlays,
                static_props: &self.props,
            }
        }
    }

    fn test_lumps() -> TestLumps {
        let mut faces = vec![0u8; FACE_SIZE * 4];
        for (i, (edges, texinfo, disp)) in [(4, 0, -1), (3, 0, -1), (4, 1, -1), (4, 0, 0)].into_iter().enumerate() {
            put_i16(&mut faces, i * FACE_SIZE + 8, edges);
            put_i16(&mut faces, i * FACE_SIZE + 10, texinfo);
            put_i16(&mut faces, i * FACE_SIZE + 12, disp);
        }
        let mut texinfo = vec![0u8; TEXINFO_SIZE * 2];
        put_i32(&mut texinfo, TEXINFO_SIZE + 64, 0x80);
        let mut dispinfo = vec![0u8; DISPINFO_SIZE];
        put_i32(&mut dispinfo, 20, 3);
        let leaf_faces: Vec<u8> = [0u16, 1, 0, 2, 3, 1].iter().flat_map(|f| f.to_le_bytes()).collect();
        let mut leafs = vec![0u8; 32 * 3];
        for (i, (cluster, first, n)) in [(0i16, 0i16, 3i16), (1, 3, 2), (2, 5, 1)].into_iter().enumerate() {
            put_i16(&mut leafs, i * 32 + 4, cluster);
            put_i16(&mut leafs, i * 32 + 20, first);
            put_i16(&mut leafs, i * 32 + 22, n);
        }
        let mut overlays = vec![0u8; OVERLAY_SIZE];
        put_i16(&mut overlays, 6, 1);
        put_i32(&mut overlays, 8, 3);
        let mut props = Vec::new();
        props.extend(0i32.to_le_bytes());
        props.extend(2i32.to_le_bytes());
        props.extend(1u16.to_le_bytes());
        props.extend(2u16.to_le_bytes());
        props.extend(1i32.to_le_bytes());
        let mut prop = vec![0u8; 76];
        put_i16(&mut prop, 26, 0);
        put_i16(&mut prop, 28, 2);
        props.extend(prop);
        TestLumps { leafs, leaf_faces, faces, texinfo, dispinfo, overlays, props }
    }

    #[test]
    fn costs_of_visible_clusters() {
        let t = test_lumps();
        let l = t.lumps();
        let vis = BspVis::for_tests(3, Vec::new());
        let c = RenderCosts::from_lumps(&l, &vis);
        assert_eq!(c.cluster_count(), 3);
        let only = |k: usize| (0..3).map(|i| i == k).collect::<Vec<bool>>();
        assert_eq!(c.cost(&only(0)), RenderCost { faces: 2, triangles: 3, ..RenderCost::default() });
        let one = RenderCost { faces: 1, triangles: 128, displacements: 1, props: 1, overlays: 1 };
        assert_eq!(c.cost(&only(1)), one);
        let all = c.cost(&[true; 3]);
        assert_eq!(all, RenderCost { faces: 3, triangles: 131, displacements: 1, props: 1, overlays: 1 });
        assert_eq!(c.cost(&[false; 3]), RenderCost::default());
        assert!(c.heat().is_empty());
        let mut lump = Vec::new();
        lump.extend(3i32.to_le_bytes());
        for row in 0..3i32 {
            lump.extend((28 + row).to_le_bytes());
            lump.extend(0i32.to_le_bytes());
        }
        lump.extend([0b011u8, 0b111, 0b110]);
        let c = RenderCosts::from_lumps(&l, &BspVis::for_tests(3, lump));
        assert_eq!(c.heat(), [131, 131, 129]);
    }

    #[test]
    fn hostile_lumps_do_not_panic() {
        let vis = BspVis::for_tests(4, vec![0xff; 64]);
        let junk: Vec<u8> = (0..4000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
        for cut in [0, 1, 7, 33, 57, 353, 1000, 4000] {
            let d = &junk[..cut];
            let l = CostLumps {
                leafs: d,
                leaf_version: 0,
                leaf_faces: d,
                faces: d,
                texinfo: d,
                dispinfo: d,
                overlays: d,
                static_props: d,
            };
            let c = RenderCosts::from_lumps(&l, &vis);
            let _ = c.cost(&[true; 4]);
            let _ = c.cost(&[]);
        }
        let mut props = vec![0u8; 16];
        put_i32(&mut props, 0, i32::MAX);
        assert_eq!(read_props(&props, &mut |_, _| {}), 0);
        let mut props = vec![0u8; 12];
        put_i32(&mut props, 8, i32::MAX);
        assert_eq!(read_props(&props, &mut |_, _| {}), 0);
    }

    #[test]
    fn marks_count_each_item_once_per_round() {
        let mut m = Marks::new(3);
        m.next();
        assert!(m.first(1) && !m.first(1) && m.seen(1) && !m.seen(0));
        assert!(!m.first(7), "out of range");
        m.next();
        assert!(!m.seen(1) && m.first(1));
        m.stamp = u32::MAX;
        m.next();
        assert_eq!(m.stamp, 1);
        assert!(m.first(1));
    }
}
