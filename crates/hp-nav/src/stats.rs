use std::collections::HashMap;

use crate::{NavMesh, attr};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NavStats {
    pub areas: usize,
    pub ladders: usize,
    pub places: usize,
    pub connections: usize,
    pub two_way_pairs: usize,
    pub one_way: usize,
    pub dangling: usize,
    pub unconnected_areas: usize,
    pub components: usize,
    pub isolated_areas: usize,
    pub attributes: Vec<(&'static str, usize)>,
    pub walkable_area: f64,
    pub hiding_spots: usize,
}

fn find(parent: &mut [u32], mut i: u32) -> u32 {
    while parent[i as usize] != i {
        let p = parent[i as usize];
        parent[i as usize] = parent[p as usize];
        i = p;
    }
    i
}

fn union(parent: &mut [u32], a: u32, b: u32) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[ra.max(rb) as usize] = ra.min(rb);
    }
}

pub fn area_components(mesh: &NavMesh) -> (Vec<u32>, usize) {
    let n = mesh.areas.len();
    let index = mesh.index_by_id();
    let mut parent: Vec<u32> = (0..n as u32).collect();
    for (i, a) in mesh.areas.iter().enumerate() {
        for (_, id) in a.all_connections() {
            if let Some(&j) = index.get(&id) {
                union(&mut parent, i as u32, j as u32);
            }
        }
    }
    for l in &mesh.ladders {
        let ends: Vec<usize> =
            l.top_areas.iter().chain(std::iter::once(&l.bottom_area)).filter_map(|id| index.get(id).copied()).collect();
        if let Some((&first, rest)) = ends.split_first() {
            for &j in rest {
                union(&mut parent, first as u32, j as u32);
            }
        }
    }
    let roots: Vec<u32> = (0..n as u32).map(|i| find(&mut parent, i)).collect();
    let mut sizes: HashMap<u32, (f64, usize)> = HashMap::new();
    for (i, &r) in roots.iter().enumerate() {
        let s = sizes.entry(r).or_default();
        let a = f64::from(mesh.areas[i].size_2d());
        s.0 += if a.is_finite() { a } else { 0.0 };
        s.1 += 1;
    }
    let mut order: Vec<(u32, (f64, usize))> = sizes.into_iter().collect();
    order.sort_by(|a, b| b.1.0.total_cmp(&a.1.0).then(b.1.1.cmp(&a.1.1)).then(a.0.cmp(&b.0)));
    let rank: HashMap<u32, u32> = order.iter().enumerate().map(|(i, (r, _))| (*r, i as u32)).collect();
    (roots.iter().map(|r| rank.get(r).copied().unwrap_or(0)).collect(), order.len())
}

const MAX_SEEDS: usize = 256;

const STAND_HEIGHT: f32 = 100.0;

impl NavMesh {
    pub fn area_at(&self, p: [f32; 3]) -> Option<usize> {
        let mut best: Option<(f32, usize)> = None;
        for (i, a) in self.areas.iter().enumerate() {
            let (x0, x1) = (a.nw[0].min(a.se[0]), a.nw[0].max(a.se[0]));
            let (y0, y1) = (a.nw[1].min(a.se[1]), a.nw[1].max(a.se[1]));
            if p[0] < x0 || p[0] > x1 || p[1] < y0 || p[1] > y1 {
                continue;
            }
            let z = a.corners().iter().map(|c| c[2]).fold(f32::MIN, f32::max);
            let dz = p[2] - z;
            if (-16.0..=STAND_HEIGHT).contains(&dz) && best.is_none_or(|(b, _)| dz.abs() < b) {
                best = Some((dz.abs(), i));
            }
        }
        best.map(|(_, i)| i)
    }

    pub fn components_from(&self, seeds: &[[f32; 3]]) -> (Vec<u32>, usize) {
        let (mut groups, count) = area_components(self);
        let mut votes: HashMap<u32, usize> = HashMap::new();
        for s in seeds.iter().take(MAX_SEEDS) {
            if let Some(i) = self.area_at(*s) {
                *votes.entry(groups[i]).or_default() += 1;
            }
        }
        let main = votes.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))).map(|(g, _)| g);
        if let Some(main) = main.filter(|&g| g != 0) {
            for g in &mut groups {
                if *g == main {
                    *g = 0;
                } else if *g == 0 {
                    *g = main;
                }
            }
        }
        (groups, count)
    }

    pub fn stats_from(&self, seeds: &[[f32; 3]]) -> NavStats {
        let mut s = self.stats();
        let (groups, _) = self.components_from(seeds);
        s.isolated_areas = groups.iter().filter(|&&g| g != 0).count();
        s
    }

    pub fn stats(&self) -> NavStats {
        let index = self.index_by_id();
        let mut s = NavStats {
            areas: self.areas.len(),
            ladders: self.ladders.len(),
            places: self.places.len(),
            ..NavStats::default()
        };
        let mut flag_counts = vec![0usize; attr::NAMES.len()];
        let mut has_ladder = vec![false; self.areas.len()];
        for l in &self.ladders {
            for id in l.top_areas.iter().chain(std::iter::once(&l.bottom_area)) {
                if let Some(&j) = index.get(id) {
                    has_ladder[j] = true;
                }
            }
        }
        for (i, a) in self.areas.iter().enumerate() {
            let mut any = !a.ladders_up.is_empty() || !a.ladders_down.is_empty() || has_ladder[i];
            for (_, id) in a.all_connections() {
                s.connections += 1;
                match index.get(&id) {
                    None => s.dangling += 1,
                    Some(&j) => {
                        any = true;
                        let back = self.areas[j].all_connections().any(|(_, b)| b == a.id);
                        if back {
                            if i < j {
                                s.two_way_pairs += 1;
                            }
                        } else {
                            s.one_way += 1;
                        }
                    }
                }
            }
            if !any {
                s.unconnected_areas += 1;
            }
            for (k, (bit, _)) in attr::NAMES.iter().enumerate() {
                if a.flags & bit != 0 {
                    flag_counts[k] += 1;
                }
            }
            if a.size_2d().is_finite() {
                s.walkable_area += f64::from(a.size_2d());
            }
            s.hiding_spots += a.hiding_spots.len();
        }
        if s.unconnected_areas > 0 {
            let mut incoming = vec![false; self.areas.len()];
            for a in &self.areas {
                for (_, id) in a.all_connections() {
                    if let Some(&j) = index.get(&id) {
                        incoming[j] = true;
                    }
                }
            }
            s.unconnected_areas = self
                .areas
                .iter()
                .enumerate()
                .filter(|(i, a)| {
                    !incoming[*i]
                        && !has_ladder[*i]
                        && a.ladders_up.is_empty()
                        && a.ladders_down.is_empty()
                        && !a.all_connections().any(|(_, id)| index.contains_key(&id))
                })
                .count();
        }
        let (groups, count) = area_components(self);
        s.components = count;
        s.isolated_areas = groups.iter().filter(|&&g| g != 0).count();
        s.attributes =
            attr::NAMES.iter().zip(flag_counts).filter(|(_, n)| *n > 0).map(|((_, name), n)| (*name, n)).collect();
        s
    }
}
