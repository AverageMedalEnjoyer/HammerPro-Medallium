use glam::{DVec3, Vec3};

use crate::brush::MapBrush;
use crate::materials::{contents, surf};
use crate::planes::{PlaneSet, normalize_f32};
use crate::tree::{Frag, FragSide, Node, PSIDE_BACK, PSIDE_BOTH, PSIDE_FACING, PSIDE_FRONT, Tree, block_range_sized};
use crate::winding::Winding;

pub type FWinding = Vec<Vec3>;

pub const MAX_COORD: f32 = 16384.0;
const PLANESIDE_EPSILON: f64 = 0.001;
const EDGE_LENGTH: f64 = 0.2;

pub fn base_winding(n: Vec3, d: f32) -> FWinding {
    let mut x = 3;
    let mut max = -1.0f32;
    for i in 0..3 {
        let v = n[i].abs();
        if v > max {
            x = i;
            max = v;
        }
    }
    let mut vup = Vec3::ZERO;
    if x == 2 {
        vup.x = 1.0;
    } else {
        vup.z = 1.0;
    }
    let v = vup.dot(n);
    vup = Vec3::new(vup.x + -v * n.x, vup.y + -v * n.y, vup.z + -v * n.z);
    let (vup, _) = normalize_f32(vup);
    let org = Vec3::new(n.x * d, n.y * d, n.z * d);
    let vright = vup.cross(n);
    let s = MAX_COORD * 4.0;
    let vup = Vec3::new(vup.x * s, vup.y * s, vup.z * s);
    let vright = Vec3::new(vright.x * s, vright.y * s, vright.z * s);
    vec![(org - vright) + vup, (org + vright) + vup, (org + vright) - vup, (org - vright) - vup]
}

type Dists = smallvec::SmallVec<[f32; 24]>;
type Sides = smallvec::SmallVec<[u8; 24]>;

fn classify(w: &FWinding, n: Vec3, d: f32, eps: f32) -> (Dists, Sides, [usize; 3]) {
    let mut dists = Dists::with_capacity(w.len() + 1);
    let mut sides = Sides::with_capacity(w.len() + 1);
    let mut counts = [0usize; 3];
    for p in w {
        let mut dot = p.dot(n);
        dot -= d;
        let s = if dot > eps {
            0
        } else if dot < -eps {
            1
        } else {
            2
        };
        dists.push(dot);
        sides.push(s);
        counts[s as usize] += 1;
    }
    if !w.is_empty() {
        dists.push(dists[0]);
        sides.push(sides[0]);
    }
    (dists, sides, counts)
}

fn split_point(p1: Vec3, p2: Vec3, d1: f32, d2: f32, n: Vec3, d: f32) -> Vec3 {
    let dot = d1 / (d1 - d2);
    let mut mid = Vec3::ZERO;
    for j in 0..3 {
        mid[j] = if n[j] == 1.0 {
            d
        } else if n[j] == -1.0 {
            -d
        } else {
            p1[j] + dot * (p2[j] - p1[j])
        };
    }
    mid
}

pub fn chop_in_place(w: FWinding, n: Vec3, d: f32, eps: f32) -> Option<FWinding> {
    let (dists, sides, counts) = classify(&w, n, d, eps);
    if counts[0] == 0 {
        return None;
    }
    if counts[1] == 0 {
        return Some(w);
    }
    let m = w.len();
    let mut f = Vec::with_capacity(m + 4);
    for i in 0..m {
        let p1 = w[i];
        if sides[i] == 2 {
            f.push(p1);
            continue;
        }
        if sides[i] == 0 {
            f.push(p1);
        }
        if sides[i + 1] == 2 || sides[i + 1] == sides[i] {
            continue;
        }
        f.push(split_point(p1, w[(i + 1) % m], dists[i], dists[i + 1], n, d));
    }
    Some(f)
}

pub fn clip_epsilon(w: &FWinding, n: Vec3, d: f32, eps: f32) -> (Option<FWinding>, Option<FWinding>) {
    let (dists, sides, counts) = classify(w, n, d, eps);
    if counts[0] == 0 {
        return (None, Some(w.clone()));
    }
    if counts[1] == 0 {
        return (Some(w.clone()), None);
    }
    let m = w.len();
    let mut f = Vec::with_capacity(m + 4);
    let mut b = Vec::with_capacity(m + 4);
    for i in 0..m {
        let p1 = w[i];
        if sides[i] == 2 {
            f.push(p1);
            b.push(p1);
            continue;
        }
        if sides[i] == 0 {
            f.push(p1);
        }
        if sides[i] == 1 {
            b.push(p1);
        }
        if sides[i + 1] == 2 || sides[i + 1] == sides[i] {
            continue;
        }
        let mid = split_point(p1, w[(i + 1) % m], dists[i], dists[i + 1], n, d);
        f.push(mid);
        b.push(mid);
    }
    (Some(f), Some(b))
}

fn offset_dist(n: Vec3, d: f32, offset: Vec3) -> f32 {
    (n.x * offset.x + n.y * offset.y) + (n.z * offset.z + d)
}

pub fn classify_epsilon_offset(
    w: &mut FWinding,
    n: Vec3,
    d: f32,
    eps: f32,
    offset: Vec3,
) -> (Option<FWinding>, Option<FWinding>, Option<FWinding>) {
    translate(w, offset);
    let dd = offset_dist(n, d, offset);
    let (_, _, counts) = classify(w, n, dd, eps);
    let (mut f, mut b, mut on) = if counts[0] == 0 && counts[1] == 0 {
        (None, None, Some(w.clone()))
    } else {
        let (f, b) = clip_epsilon(w, n, dd, eps);
        (f, b, None)
    };
    translate(w, -offset);
    for x in [&mut f, &mut b, &mut on].into_iter().flatten() {
        translate(x, -offset);
    }
    (f, b, on)
}

pub fn center(w: &FWinding) -> Vec3 {
    let mut c = Vec3::ZERO;
    for p in w {
        c = *p + c;
    }
    let scale = (1.0 / w.len() as f64) as f32;
    Vec3::new(c.x * scale, c.y * scale, c.z * scale)
}

pub fn translate(w: &mut FWinding, o: Vec3) {
    for p in w.iter_mut() {
        *p += o;
    }
}

pub fn clip_epsilon_offset(
    w: &mut FWinding,
    n: Vec3,
    d: f32,
    eps: f32,
    offset: Vec3,
) -> (Option<FWinding>, Option<FWinding>) {
    translate(w, offset);
    let (mut f, mut b) = clip_epsilon(w, n, offset_dist(n, d, offset), eps);
    translate(w, -offset);
    if let Some(f) = f.as_mut() {
        translate(f, -offset);
    }
    if let Some(b) = b.as_mut() {
        translate(b, -offset);
    }
    (f, b)
}

fn length(v: Vec3) -> f32 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

pub fn area(w: &FWinding) -> f32 {
    let mut total = 0.0f32;
    for i in 2..w.len() {
        let d1 = w[i - 1] - w[0];
        let d2 = w[i] - w[0];
        total += length(d1.cross(d2));
    }
    total * 0.5
}

pub fn is_tiny(w: &FWinding) -> bool {
    let mut edges = 0;
    for i in 0..w.len() {
        let j = if i == w.len() - 1 { 0 } else { i + 1 };
        if f64::from(length(w[j] - w[i])) > EDGE_LENGTH {
            edges += 1;
            if edges == 3 {
                return false;
            }
        }
    }
    true
}

#[derive(Clone, Debug)]
pub struct BSide {
    pub plane: usize,
    pub node: bool,
    pub winding: Option<FWinding>,
    pub visible: bool,
    pub bevel: bool,
    pub surf: u32,
    pub contents: u32,
}

#[derive(Clone, Debug)]
pub struct BBrush {
    pub original: usize,
    pub sides: Vec<BSide>,
    pub mins: Vec3,
    pub maxs: Vec3,
    pub side: u8,
    pub testside: u8,
}

impl BBrush {
    fn empty(original: usize) -> Self {
        Self { original, sides: Vec::new(), mins: Vec3::ZERO, maxs: Vec3::ZERO, side: 0, testside: 0 }
    }

    pub fn bound(&mut self) {
        let mut lo = Vec3::splat(99999.0);
        let mut hi = Vec3::splat(-99999.0);
        for w in self.sides.iter().filter_map(|s| s.winding.as_ref()) {
            for p in w {
                for i in 0..3 {
                    if p[i] < lo[i] {
                        lo[i] = p[i];
                    }
                    if p[i] > hi[i] {
                        hi[i] = p[i];
                    }
                }
            }
        }
        self.mins = lo;
        self.maxs = hi;
    }

    pub fn volume(&self, planes: &PlaneSet) -> f32 {
        let Some(first) = self.sides.iter().position(|s| s.winding.is_some()) else { return 0.0 };
        let corner = self.sides[first].winding.as_ref().map_or(Vec3::ZERO, |w| w[0]);
        let mut volume = 0.0f32;
        for s in &self.sides[first..] {
            let Some(w) = &s.winding else { continue };
            let (n, d) = planes.getf(s.plane);
            let dd = -(corner.dot(n) - d);
            volume += dd * area(w);
        }
        volume / 3.0
    }

    pub fn to_frag(&self, planes: &PlaneSet) -> Frag {
        let sides = self
            .sides
            .iter()
            .map(|s| {
                let winding = s.winding.as_ref().map(|w| {
                    let mut w = Winding::new(w.iter().map(|p| p.as_dvec3()).collect());
                    if w.points.len() >= 3 && w.normal().dot(planes.get(s.plane).normal) < 0.0 {
                        w = w.reversed();
                    }
                    w
                });
                FragSide {
                    plane: s.plane,
                    winding,
                    on_node: s.node,
                    visible: s.visible,
                    bevel: s.bevel,
                    no_split: false,
                    surf: s.surf,
                    tested: false,
                }
            })
            .collect();
        Frag {
            original: self.original,
            sides,
            mins: self.mins.as_dvec3(),
            maxs: self.maxs.as_dvec3(),
            test_side: self.testside,
            side: self.side,
        }
    }
}

fn mostly_on_side(b: &BBrush, n: Vec3, d: f32) -> u8 {
    let mut max = 0.0f32;
    let mut side = PSIDE_FRONT;
    for w in b.sides.iter().filter_map(|s| s.winding.as_ref()) {
        for p in w {
            let dd = p.dot(n) - d;
            if dd > max {
                max = dd;
                side = PSIDE_FRONT;
            }
            if -dd > max {
                max = -dd;
                side = PSIDE_BACK;
            }
        }
    }
    side
}

pub fn split_brush(planes: &PlaneSet, brush: &mut BBrush, pnum: usize) -> (Option<BBrush>, Option<BBrush>) {
    let (pn, pd) = planes.getf(pnum);
    let (mut d_front, mut d_back) = (0.0f32, 0.0f32);
    for w in brush.sides.iter().filter_map(|s| s.winding.as_ref()) {
        for p in w {
            let d = p.dot(pn) - pd;
            if d > 0.0 && d > d_front {
                d_front = d;
            }
            if d < 0.0 && d < d_back {
                d_back = d;
            }
        }
    }
    if f64::from(d_front) < 0.1 {
        return (None, Some(brush.clone()));
    }
    if f64::from(d_back) > -0.1 {
        return (Some(brush.clone()), None);
    }
    let offset = (brush.mins + brush.maxs) * -0.5;
    let mut w = Some(base_winding(pn, pd + pn.dot(offset)));
    for s in &brush.sides {
        let Some(cur) = w.take() else { break };
        let (n2, d2) = planes.getf(s.plane ^ 1);
        w = chop_in_place(cur, n2, offset_dist(n2, d2, offset), 0.0);
    }
    let mut mid = match w {
        Some(w) if !is_tiny(&w) => w,
        _ => {
            return if mostly_on_side(brush, pn, pd) == PSIDE_FRONT {
                (Some(brush.clone()), None)
            } else {
                (None, Some(brush.clone()))
            };
        }
    };
    translate(&mut mid, -offset);
    let mut b = [BBrush::empty(brush.original), BBrush::empty(brush.original)];
    for s in brush.sides.iter_mut() {
        let Some(w) = s.winding.as_mut() else { continue };
        let (f, k) = clip_epsilon_offset(w, pn, pd, 0.0, offset);
        for (j, cw) in [f, k].into_iter().enumerate() {
            let Some(cw) = cw else { continue };
            b[j].sides.push(BSide {
                plane: s.plane,
                node: s.node,
                winding: Some(cw),
                visible: s.visible,
                bevel: s.bevel,
                surf: s.surf,
                contents: s.contents,
            });
        }
    }
    let mut keep = [true, true];
    for i in 0..2 {
        b[i].bound();
        let bogus = (0..3).any(|j| b[i].mins[j] < -MAX_COORD || b[i].maxs[j] > MAX_COORD);
        if b[i].sides.len() < 3 || bogus {
            keep[i] = false;
        }
    }
    if !(keep[0] && keep[1]) {
        let front = keep[0].then(|| brush.clone());
        let back = keep[1].then(|| brush.clone());
        return (front, back);
    }
    for (i, bi) in b.iter_mut().enumerate() {
        bi.sides.push(BSide {
            plane: pnum ^ i ^ 1,
            node: true,
            winding: Some(mid.clone()),
            visible: false,
            bevel: false,
            surf: 0,
            contents: 0,
        });
    }
    let [b0, b1] = b;
    let f = (f64::from(b0.volume(planes)) >= 1.0).then_some(b0);
    let k = (f64::from(b1.volume(planes)) >= 1.0).then_some(b1);
    (f, k)
}

fn create_windings(planes: &PlaneSet, b: &mut BBrush) {
    let mut inside = Vec3::ZERO;
    let mut ok = false;
    let mut k = 0;
    while k < 4 && !ok {
        ok = true;
        for s in &b.sides {
            let (n, d) = planes.getf(s.plane);
            let dd = n.dot(inside) - d;
            if dd < 0.0 {
                ok = false;
                inside -= Vec3::new(n.x * dd, n.y * dd, n.z * dd);
            }
        }
        k += 1;
    }
    let offset = -inside;
    let list: Vec<(usize, bool)> = b.sides.iter().map(|s| (s.plane, s.bevel)).collect();
    for i in 0..b.sides.len() {
        let (n, d) = planes.getf(list[i].0);
        let mut w = Some(base_winding(n, (n.x * offset.x + n.z * offset.z) + (n.y * offset.y + d)));
        for (j, &(pj, bevel)) in list.iter().enumerate() {
            let Some(cur) = w.take() else { break };
            if i == j || bevel {
                w = Some(cur);
                continue;
            }
            let (n2, d2) = planes.getf(pj ^ 1);
            w = chop_in_place(cur, n2, offset_dist(n2, d2, offset), 0.0);
        }
        if let Some(w) = w.as_mut() {
            translate(w, -offset);
        }
        b.sides[i].winding = w;
    }
    b.bound();
}

fn bounds_plane_ids(planes: &mut PlaneSet, mins: Vec3, maxs: Vec3) -> [usize; 6] {
    let mut ids = [0usize; 6];
    for i in 0..3 {
        let mut n = Vec3::ZERO;
        n[i] = 1.0;
        ids[i] = planes.find_f32(n, maxs[i]);
        n[i] = -1.0;
        ids[3 + i] = planes.find_f32(n, -mins[i]);
    }
    ids
}

fn brush_from_bounds(planes: &PlaneSet, ids: [usize; 6]) -> BBrush {
    let mut b = BBrush::empty(usize::MAX);
    for p in ids {
        b.sides.push(BSide {
            plane: p,
            node: false,
            winding: None,
            visible: false,
            bevel: false,
            surf: 0,
            contents: 0,
        });
    }
    create_windings(planes, &mut b);
    b
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DetailScreen {
    Full,
    NoDetail,
    OnlyDetail,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Windings,
    Marked,
}

pub fn make_brush_list(
    planes: &mut PlaneSet,
    brushes: &[MapBrush],
    ids: &[usize],
    clipmins: Vec3,
    clipmaxs: Vec3,
    screen: DetailScreen,
    vis: Visibility,
) -> Vec<BBrush> {
    let (maxp, minp) = bounding_planes(planes, clipmins, clipmaxs);
    clip_brush_list(planes, brushes, ids, clipmins, clipmaxs, (maxp, minp), screen, vis, None)
}

fn bounding_planes(planes: &mut PlaneSet, clipmins: Vec3, clipmaxs: Vec3) -> ([usize; 2], [usize; 2]) {
    let mut maxp = [0usize; 2];
    let mut minp = [0usize; 2];
    for i in 0..2 {
        let mut n = Vec3::ZERO;
        n[i] = 1.0;
        maxp[i] = planes.find_f32(n, clipmaxs[i]);
        minp[i] = planes.find_f32(n, clipmins[i]);
    }
    (maxp, minp)
}

#[allow(clippy::too_many_arguments)]
fn clip_brush_list(
    planes: &PlaneSet,
    brushes: &[MapBrush],
    ids: &[usize],
    clipmins: Vec3,
    clipmaxs: Vec3,
    (maxp, minp): ([usize; 2], [usize; 2]),
    screen: DetailScreen,
    vis: Visibility,
    bounds: Option<&[Option<(Vec3, Vec3)>]>,
) -> Vec<BBrush> {
    let mut out = Vec::new();
    for (k, &i) in ids.iter().enumerate() {
        let mb = &brushes[i];
        let detail = mb.contents & contents::DETAIL != 0;
        if (screen == DetailScreen::NoDetail && detail) || (screen == DetailScreen::OnlyDetail && !detail) {
            continue;
        }
        if mb.sides.is_empty() {
            continue;
        }
        let bounds = bounds.and_then(|b| b[k]).unwrap_or_else(|| map_brush_bounds(mb));
        if let Some(b) = clipped_brush(planes, i, mb, bounds, clipmins, clipmaxs, maxp, minp, vis) {
            out.push(b);
        }
    }
    out.reverse();
    out
}

pub fn map_brush_bounds(mb: &MapBrush) -> (Vec3, Vec3) {
    let mut lo = Vec3::splat(99999.0);
    let mut hi = Vec3::splat(-99999.0);
    for p in mb.sides.iter().filter_map(|s| s.fwinding.as_ref()).flatten() {
        crate::brush::add_point_to_bounds(*p, &mut lo, &mut hi);
    }
    (lo, hi)
}

pub fn map_bounds(brushes: &[MapBrush], ids: &[usize]) -> (Vec3, Vec3) {
    let mut mins = Vec3::splat(99999.0);
    let mut maxs = Vec3::splat(-99999.0);
    for &i in ids {
        if brushes[i].sides.is_empty() {
            continue;
        }
        let (lo, hi) = map_brush_bounds(&brushes[i]);
        if lo.x > MAX_COORD {
            continue;
        }
        for p in [lo, hi] {
            crate::brush::add_point_to_bounds(p, &mut mins, &mut maxs);
        }
    }
    (mins, maxs)
}

pub fn brushes_bounds(brushes: &[MapBrush], ids: &[usize]) -> (DVec3, DVec3) {
    let mut lo = Vec3::splat(99999.0);
    let mut hi = Vec3::splat(-99999.0);
    for &i in ids {
        if brushes[i].sides.is_empty() {
            continue;
        }
        let (a, b) = map_brush_bounds(&brushes[i]);
        for p in [a, b] {
            crate::brush::add_point_to_bounds(p, &mut lo, &mut hi);
        }
    }
    (lo.as_dvec3(), hi.as_dvec3())
}

#[allow(clippy::too_many_arguments)]
fn clipped_brush(
    planes: &PlaneSet,
    index: usize,
    mb: &MapBrush,
    (mins, maxs): (Vec3, Vec3),
    clipmins: Vec3,
    clipmaxs: Vec3,
    maxp: [usize; 2],
    minp: [usize; 2],
    vis: Visibility,
) -> Option<BBrush> {
    for j in 0..3 {
        if mins[j] >= clipmaxs[j] || maxs[j] <= clipmins[j] {
            return None;
        }
    }
    let mut b = BBrush::empty(index);
    let clip = mb.sides.iter().any(|s| !s.bevel && s.no_split);
    for s in &mb.sides {
        let visible = match vis {
            Visibility::Windings => s.fwinding.is_some(),
            Visibility::Marked => s.visible,
        };
        b.sides.push(BSide {
            plane: s.plane,
            node: if s.bevel { clip } else { s.no_split },
            winding: s.fwinding.clone(),
            visible: visible || s.surf & surf::HINT != 0,
            bevel: s.bevel,
            surf: s.surf,
            contents: s.contents,
        });
    }
    b.mins = mins;
    b.maxs = maxs;
    for j in 0..2 {
        if b.maxs[j] > clipmaxs[j] {
            let (_, back) = split_brush(planes, &mut b, maxp[j]);
            b = back?;
        }
        if b.mins[j] < clipmins[j] {
            let (front, _) = split_brush(planes, &mut b, minp[j]);
            b = front?;
        }
    }
    for s in &mut b.sides {
        let p = s.plane & !1;
        if p == maxp[0] || p == maxp[1] || p == minp[0] || p == minp[1] {
            s.node = true;
            s.visible = false;
        }
    }
    Some(b)
}

fn disjoint(a: &BBrush, b: &BBrush) -> bool {
    for i in 0..3 {
        if a.mins[i] >= b.maxs[i] || a.maxs[i] <= b.mins[i] {
            return true;
        }
    }
    a.sides.iter().any(|s| b.sides.iter().any(|t| s.plane == (t.plane ^ 1)))
}

fn subtract(planes: &PlaneSet, a: &mut BBrush, b: &BBrush) -> Option<Option<Vec<BBrush>>> {
    let mut out: Vec<BBrush> = Vec::new();
    let mut inside: Option<BBrush> = None;
    let mut first = true;
    for s in &b.sides {
        let (front, back) =
            if first { split_brush(planes, a, s.plane) } else { split_brush(planes, inside.as_mut()?, s.plane) };
        first = false;
        if let Some(f) = front {
            out.push(f);
        }
        inside = back;
        if inside.is_none() {
            break;
        }
    }
    if first {
        return Some(None);
    }
    match inside {
        Some(_) if out.is_empty() => None,
        Some(_) => {
            out.reverse();
            Some(Some(out))
        }
        None => Some(None),
    }
}

fn brush_ge(c1: u32, c2: u32) -> bool {
    if c2 & contents::MASK_WATER != 0 && c1 & contents::AREAPORTAL != 0 {
        return true;
    }
    if c1 & contents::DETAIL != 0 && c2 & contents::DETAIL == 0 {
        return false;
    }
    if c1 & contents::SOLID != 0 {
        return true;
    }
    let trans = contents::GRATE | contents::WINDOW;
    c1 & trans != 0 && c2 & trans != 0
}

pub fn chop_brushes(planes: &PlaneSet, contents_of: &[u32], mut head: Vec<BBrush>) -> Vec<BBrush> {
    let mut keep: Vec<BBrush> = Vec::new();
    let c = |b: &BBrush| contents_of.get(b.original).copied().unwrap_or(0);
    'newlist: loop {
        if head.is_empty() {
            break;
        }
        let mut b1i = 0;
        while b1i < head.len() {
            let mut b2i = b1i + 1;
            while b2i < head.len() {
                if disjoint(&head[b1i], &head[b2i]) {
                    b2i += 1;
                    continue;
                }
                let (c1, c2) = (c(&head[b1i]), c(&head[b2i]));
                let mut sub: Option<Vec<BBrush>> = None;
                let mut sub2: Option<Vec<BBrush>> = None;
                let mut n1 = 999_999usize;
                let mut n2 = 999_999usize;
                if brush_ge(c2, c1) {
                    let (a, b) = two(&mut head, b1i, b2i);
                    match subtract(planes, a, b) {
                        Some(None) => {
                            b2i += 1;
                            continue;
                        }
                        None => {
                            head = cull(head, b1i, b1i);
                            continue 'newlist;
                        }
                        Some(Some(list)) => {
                            n1 = list.len();
                            sub = Some(list);
                        }
                    }
                }
                if brush_ge(c1, c2) {
                    let (b, a) = two(&mut head, b2i, b1i);
                    match subtract(planes, b, a) {
                        Some(None) => {
                            b2i += 1;
                            continue;
                        }
                        None => {
                            head = cull(head, b1i, b2i);
                            continue 'newlist;
                        }
                        Some(Some(list)) => {
                            n2 = list.len();
                            sub2 = Some(list);
                        }
                    }
                }
                if sub.is_none() && sub2.is_none() {
                    b2i += 1;
                    continue;
                }
                if n1 > 1 && n2 > 1 && (c1 & c2 & contents::DETAIL) == 0 && ((c1 | c2) & contents::AREAPORTAL) == 0 {
                    b2i += 1;
                    continue;
                }
                if n1 < n2 {
                    head.extend(sub.unwrap_or_default());
                    head = cull(head, b1i, b1i);
                } else {
                    head.extend(sub2.unwrap_or_default());
                    head = cull(head, b1i, b2i);
                }
                continue 'newlist;
            }
            keep.push(std::mem::replace(&mut head[b1i], BBrush::empty(usize::MAX)));
            b1i += 1;
        }
        break;
    }
    keep.reverse();
    keep
}

fn two(v: &mut [BBrush], a: usize, b: usize) -> (&mut BBrush, &BBrush) {
    if a < b {
        let (l, r) = v.split_at_mut(b);
        (&mut l[a], &r[0])
    } else {
        let (l, r) = v.split_at_mut(a);
        (&mut r[0], &l[b])
    }
}

fn cull(head: Vec<BBrush>, start: usize, skip: usize) -> Vec<BBrush> {
    let mut out: Vec<BBrush> =
        head.into_iter().enumerate().skip(start).filter(|(i, _)| *i != skip).map(|(_, b)| b).collect();
    out.reverse();
    out
}

pub fn fixup_areaportal_water(planes: &PlaneSet, contents_of: &mut [u32], list: &mut [BBrush]) {
    for ai in 0..list.len() {
        if contents_of.get(list[ai].original).is_none_or(|c| c & contents::AREAPORTAL == 0) {
            continue;
        }
        for wi in 0..list.len() {
            if wi == ai {
                continue;
            }
            let wc = contents_of.get(list[wi].original).copied().unwrap_or(0);
            if wc & contents::AREAPORTAL != 0 || wc & contents::MASK_WATER == 0 {
                continue;
            }
            if disjoint(&list[ai], &list[wi]) {
                continue;
            }
            let (a, w) = two(list, ai, wi);
            if intersect(planes, a, w) {
                let o = list[ai].original;
                contents_of[o] |= wc;
            }
        }
    }
}

fn intersect(planes: &PlaneSet, a: &mut BBrush, b: &BBrush) -> bool {
    let mut inside: Option<BBrush> = None;
    let mut first = true;
    for s in &b.sides {
        let (_, back) = if first {
            split_brush(planes, a, s.plane)
        } else {
            match inside.as_mut() {
                Some(i) => split_brush(planes, i, s.plane),
                None => break,
            }
        };
        first = false;
        inside = back;
        if inside.is_none() {
            break;
        }
    }
    !first && inside.is_some()
}

fn box_on_plane_side(mins: Vec3, maxs: Vec3, planes: &PlaneSet, pnum: usize) -> u8 {
    let p = planes.get(pnum);
    let (n, d) = planes.getf(pnum);
    if p.kind < 3 {
        let t = p.kind as usize;
        let mut side = 0;
        if f64::from(maxs[t]) > f64::from(d) + PLANESIDE_EPSILON {
            side |= PSIDE_FRONT;
        }
        if f64::from(mins[t]) < f64::from(d) - PLANESIDE_EPSILON {
            side |= PSIDE_BACK;
        }
        return side;
    }
    let mut c0 = Vec3::ZERO;
    let mut c1 = Vec3::ZERO;
    for i in 0..3 {
        if n[i] < 0.0 {
            c0[i] = mins[i];
            c1[i] = maxs[i];
        } else {
            c1[i] = mins[i];
            c0[i] = maxs[i];
        }
    }
    let d1 = n.dot(c0) - d;
    let d2 = n.dot(c1) - d;
    let mut side = 0;
    if f64::from(d1) >= PLANESIDE_EPSILON {
        side = PSIDE_FRONT;
    }
    if f64::from(d2) < PLANESIDE_EPSILON {
        side |= PSIDE_BACK;
    }
    side
}

fn test_brush(planes: &PlaneSet, b: &BBrush, pnum: usize) -> (u8, i32, bool, bool) {
    for s in &b.sides {
        if s.plane == pnum {
            return (PSIDE_BACK | PSIDE_FACING, 0, false, false);
        }
        if s.plane == (pnum ^ 1) {
            return (PSIDE_FRONT | PSIDE_FACING, 0, false, false);
        }
    }
    let side = box_on_plane_side(b.mins, b.maxs, planes, pnum);
    if side != PSIDE_BOTH {
        return (side, 0, false, false);
    }
    let (n, d) = planes.getf(pnum);
    let (mut splits, mut hint) = (0, false);
    let (mut d_front, mut d_back) = (0.0f32, 0.0f32);
    for s in &b.sides {
        if s.node || !s.visible {
            continue;
        }
        let Some(w) = &s.winding else { continue };
        let (mut front, mut back) = (false, false);
        for p in w {
            let dd = p.dot(n) - d;
            if dd > d_front {
                d_front = dd;
            }
            if dd < d_back {
                d_back = dd;
            }
            if f64::from(dd) > 0.1 {
                front = true;
            }
            if f64::from(dd) < -0.1 {
                back = true;
            }
        }
        if front && back && s.surf & surf::SKIP == 0 {
            splits += 1;
            if s.surf & surf::HINT != 0 {
                hint = true;
            }
        }
    }
    let eps = (d_front > 0.0 && d_front < 1.0) || (d_back < 0.0 && d_back > -1.0);
    (side, splits, hint, eps)
}

struct BNode {
    plane: Option<usize>,
    children: [usize; 2],
    parent: Option<usize>,
    volume: Option<BBrush>,
    contents: u32,
    brushes: Vec<BBrush>,
}

struct Bsp<'a> {
    planes: &'a PlaneSet,
    contents_of: &'a [u32],
    nodes: Vec<BNode>,
}

impl Bsp<'_> {
    fn alloc(&mut self, parent: Option<usize>) -> usize {
        self.nodes.push(BNode {
            plane: None,
            children: [0, 0],
            parent,
            volume: None,
            contents: 0,
            brushes: Vec::new(),
        });
        self.nodes.len() - 1
    }

    fn orig_contents(&self, b: &BBrush) -> u32 {
        self.contents_of.get(b.original).copied().unwrap_or(0)
    }

    fn cuts_volume(&mut self, node: usize, pnum: usize) -> bool {
        let planes = self.planes;
        let Some(v) = self.nodes[node].volume.as_mut() else { return false };
        let (f, b) = split_brush(planes, v, pnum);
        f.is_some() && b.is_some()
    }

    fn select_split(&mut self, node: usize, brushes: &mut [BBrush]) -> Option<usize> {
        use rayon::prelude::*;
        let planes = self.planes;
        let mut best: Option<usize> = None;
        let mut bestvalue = -99_999i64;
        let mut tested: ahash::AHashSet<usize> = ahash::AHashSet::new();
        let mut last: Option<usize> = None;
        for pass in 0..2 {
            let mut candidates: Vec<(usize, u32, u32)> = Vec::new();
            for b in brushes.iter() {
                for side in &b.sides {
                    if side.bevel || side.winding.is_none() || side.node || side.surf & surf::SKIP != 0 {
                        continue;
                    }
                    if side.visible != (pass < 1) {
                        continue;
                    }
                    let pnum = side.plane & !1;
                    if tested.contains(&pnum) {
                        continue;
                    }
                    if !self.cuts_volume(node, pnum) {
                        continue;
                    }
                    tested.insert(pnum);
                    candidates.push((pnum, side.surf, side.contents));
                }
            }
            let list: &[BBrush] = brushes;
            let value_of = |&(pnum, side_surf, side_contents): &(usize, u32, u32)| -> i64 {
                let (mut front, mut back, mut facing, mut splits, mut eps) = (0i64, 0i64, 0i64, 0i64, 0i64);
                let mut hintsplit = false;
                for t in list {
                    let (s, bs, h, e) = test_brush(planes, t, pnum);
                    splits += i64::from(bs);
                    hintsplit = h;
                    if e {
                        eps += 1;
                    }
                    if s & PSIDE_FACING != 0 {
                        facing += 1;
                    }
                    if s & PSIDE_FRONT != 0 {
                        front += 1;
                    }
                    if s & PSIDE_BACK != 0 {
                        back += 1;
                    }
                }
                let mut value = 5 * facing - 5 * splits - (front - back).abs();
                if planes.get(pnum).kind < 3 {
                    value += 5;
                }
                value -= eps * 1000;
                if side_surf & surf::TRANS != 0 {
                    value -= 500;
                }
                if hintsplit && side_surf & surf::HINT == 0 {
                    value = -9_999_999;
                }
                if side_contents & contents::MASK_WATER != 0 {
                    value = 9_999_999;
                }
                value
            };
            let values: Vec<i64> = if candidates.len() * list.len() >= PAR_SPLIT_WORK {
                candidates.par_iter().map(value_of).collect()
            } else {
                candidates.iter().map(value_of).collect()
            };
            for (&(pnum, _, _), &value) in candidates.iter().zip(&values) {
                if value > bestvalue {
                    bestvalue = value;
                    best = Some(pnum);
                }
            }
            if let Some(&(pnum, _, _)) = candidates.last() {
                last = Some(pnum);
            }
            if best.is_some() {
                break;
            }
        }
        if let Some(pnum) = best {
            for t in brushes.iter_mut() {
                t.side = test_brush(planes, t, pnum).0;
            }
        }
        if let Some(pnum) = last {
            for t in brushes.iter_mut() {
                t.testside = test_brush(planes, t, pnum).0;
            }
        }
        best
    }

    fn leaf(&mut self, node: usize, brushes: Vec<BBrush>) {
        let mut c = 0u32;
        for b in &brushes {
            let bc = self.orig_contents(b);
            if bc & contents::SOLID != 0 && b.sides.iter().all(|s| s.node) {
                c = contents::SOLID;
                break;
            }
            c |= bc;
        }
        let n = &mut self.nodes[node];
        n.plane = None;
        n.contents = c;
        n.brushes = brushes;
    }

    fn split_node(
        &mut self,
        node: usize,
        pnum: usize,
        brushes: Vec<BBrush>,
    ) -> (Vec<BBrush>, Vec<BBrush>, Option<BBrush>, Option<BBrush>) {
        self.nodes[node].plane = Some(pnum);
        let (mut front, mut back) = (Vec::new(), Vec::new());
        for mut b in brushes {
            let s = b.side;
            if s == PSIDE_BOTH {
                let (f, k) = split_brush(self.planes, &mut b, pnum);
                if let Some(f) = f {
                    front.push(f);
                }
                if let Some(k) = k {
                    back.push(k);
                }
                continue;
            }
            if s & PSIDE_FACING != 0 {
                for side in &mut b.sides {
                    if side.plane & !1 == pnum {
                        side.node = true;
                    }
                }
            }
            if s & PSIDE_FRONT != 0 {
                front.push(b);
            } else if s & PSIDE_BACK != 0 {
                back.push(b);
            }
        }
        front.reverse();
        back.reverse();
        let (vf, vb) = match self.nodes[node].volume.as_mut() {
            Some(v) => split_brush(self.planes, v, pnum),
            None => (None, None),
        };
        (front, back, vf, vb)
    }

    fn build_serial(&mut self, head: usize, brushes: Vec<BBrush>) {
        let mut stack = vec![(head, brushes)];
        while let Some((node, mut brushes)) = stack.pop() {
            let Some(pnum) = self.select_split(node, &mut brushes) else {
                self.leaf(node, brushes);
                continue;
            };
            let (front, back, vf, vb) = self.split_node(node, pnum, brushes);
            let f = self.alloc(Some(node));
            let k = self.alloc(Some(node));
            self.nodes[node].children = [f, k];
            self.nodes[f].volume = vf;
            self.nodes[k].volume = vb;
            stack.push((k, back));
            stack.push((f, front));
        }
    }

    fn build(&mut self, head: usize, brushes: Vec<BBrush>) {
        let volume = self.nodes[head].volume.take();
        let sub = subtree(self.planes, self.contents_of, volume, brushes, 0);
        let base = self.nodes.len();
        let at = |i: usize| if i == 0 { head } else { base + i - 1 };
        let parent = self.nodes[head].parent;
        for (i, mut n) in sub.into_iter().enumerate() {
            if n.plane.is_some() {
                n.children = [at(n.children[0]), at(n.children[1])];
            }
            if i == 0 {
                n.parent = parent;
                self.nodes[head] = n;
            } else {
                n.parent = n.parent.map(at);
                self.nodes.push(n);
            }
        }
    }

    fn brush_bsp(&mut self, list: Vec<BBrush>, box_planes: [usize; 6]) -> usize {
        let head = self.alloc(None);
        self.nodes[head].volume = Some(brush_from_bounds(self.planes, box_planes));
        self.build(head, list);
        head
    }

    fn block_tree(
        &mut self,
        blocks: &[Option<usize>],
        range: (i32, i32, i32, i32),
        r: (i32, i32, i32, i32),
        split_planes: &[usize],
        next: &mut usize,
    ) -> usize {
        let (xl, yl, xh, yh) = r;
        if xl == xh && yl == yh {
            let at = (xl >= range.0 && xl <= range.2 && yl >= range.1 && yl <= range.3)
                .then(|| ((yl - range.1) * (range.2 - range.0 + 1) + (xl - range.0)) as usize);
            if let Some(n) = at.and_then(|i| blocks[i]) {
                return n;
            }
            let n = self.alloc(None);
            self.nodes[n].contents = 0;
            return n;
        }
        let node = self.alloc(None);
        let (_, _, hi, lo) = block_split(r);
        self.nodes[node].plane = Some(split_planes[*next]);
        *next += 1;
        let a = self.block_tree(blocks, range, hi, split_planes, next);
        let b = self.block_tree(blocks, range, lo, split_planes, next);
        self.nodes[node].children = [a, b];
        node
    }

    fn finish(self, head: usize, mins: DVec3, maxs: DVec3) -> Tree {
        let planes = self.planes;
        let mut map = vec![usize::MAX; self.nodes.len()];
        let mut order = Vec::new();
        let mut stack = vec![head];
        while let Some(n) = stack.pop() {
            map[n] = order.len();
            order.push(n);
            if self.nodes[n].plane.is_some() {
                let [a, b] = self.nodes[n].children;
                stack.push(b);
                stack.push(a);
            }
        }
        let mut nodes: Vec<Node> = {
            use rayon::prelude::*;
            order
                .par_iter()
                .map(|&n| {
                    let b = &self.nodes[n];
                    Node {
                        plane: b.plane,
                        children: if b.plane.is_some() { [map[b.children[0]], map[b.children[1]]] } else { [0, 0] },
                        parent: b.parent.map(|p| map[p]),
                        volume: b.volume.as_ref().map(|v| v.to_frag(planes)),
                        contents: b.contents,
                        brushes: b.brushes.iter().map(|x| x.to_frag(planes)).collect(),
                        cluster: -1,
                        ..Node::default()
                    }
                })
                .collect()
        };
        let outside = nodes.len();
        nodes.push(Node { cluster: -1, ..Node::default() });
        Tree { nodes, head: 0, outside, mins, maxs }
    }
}

const PAR_BRUSHES: usize = 24;
const PAR_DEPTH: usize = 16;
const PAR_SPLIT_WORK: usize = 4096;

fn subtree(
    planes: &PlaneSet,
    contents_of: &[u32],
    volume: Option<BBrush>,
    mut brushes: Vec<BBrush>,
    depth: usize,
) -> Vec<BNode> {
    let mut bsp = Bsp { planes, contents_of, nodes: Vec::new() };
    let root = bsp.alloc(None);
    bsp.nodes[root].volume = volume;
    if depth >= PAR_DEPTH || brushes.len() < PAR_BRUSHES {
        bsp.build_serial(root, brushes);
        return bsp.nodes;
    }
    let Some(pnum) = bsp.select_split(root, &mut brushes) else {
        bsp.leaf(root, brushes);
        return bsp.nodes;
    };
    let (front, back, vf, vb) = bsp.split_node(root, pnum, brushes);
    let (f, b) = rayon::join(
        || subtree(planes, contents_of, vf, front, depth + 1),
        || subtree(planes, contents_of, vb, back, depth + 1),
    );
    let fi = bsp.nodes.len();
    let bi = fi + f.len();
    bsp.nodes[root].children = [fi, bi];
    for (base, sub) in [(fi, f), (bi, b)] {
        for (i, mut n) in sub.into_iter().enumerate() {
            if n.plane.is_some() {
                n.children = [n.children[0] + base, n.children[1] + base];
            }
            n.parent = if i == 0 { Some(root) } else { n.parent.map(|p| p + base) };
            bsp.nodes.push(n);
        }
    }
    bsp.nodes
}

type BlockRange = (i32, i32, i32, i32);
fn block_split(r: BlockRange) -> (Vec3, i32, BlockRange, BlockRange) {
    let (xl, yl, xh, yh) = r;
    if xh - xl > yh - yl {
        let mid = xl + (xh - xl) / 2 + 1;
        (Vec3::X, mid, (mid, yl, xh, yh), (xl, yl, mid - 1, yh))
    } else {
        let mid = yl + (yh - yl) / 2 + 1;
        (Vec3::Y, mid, (xl, mid, xh, yh), (xl, yl, xh, mid - 1))
    }
}

fn block_tree_planes(planes: &mut PlaneSet, r: BlockRange, size: f32, out: &mut Vec<usize>) {
    let (xl, yl, xh, yh) = r;
    if xl == xh && yl == yh {
        return;
    }
    let (normal, mid, hi, lo) = block_split(r);
    out.push(planes.find_f32(normal, mid as f32 * size));
    block_tree_planes(planes, hi, size, out);
    block_tree_planes(planes, lo, size, out);
}

struct Block {
    mins: Vec3,
    maxs: Vec3,
    bounding: ([usize; 2], [usize; 2]),
    box_planes: Option<[usize; 6]>,
}

fn block_bsp(planes: &PlaneSet, contents_of: &[u32], list: Vec<BBrush>, box_planes: [usize; 6]) -> (Vec<BNode>, usize) {
    let list = chop_brushes(planes, contents_of, list);
    let mut bsp = Bsp { planes, contents_of, nodes: Vec::new() };
    let head = bsp.brush_bsp(list, box_planes);
    (bsp.nodes, head)
}

pub fn world_tree(
    planes: &mut PlaneSet,
    brushes: &[MapBrush],
    contents_of: &mut [u32],
    ids: &[usize],
    vis: Visibility,
    block_size: u32,
) -> Tree {
    use rayon::prelude::*;
    if block_size == 0 {
        return world_tree_whole(planes, brushes, contents_of, ids, vis);
    }
    let size = block_size as f32;
    let (mins, maxs) = map_bounds(brushes, ids);
    let range = block_range_sized(mins.as_dvec3(), maxs.as_dvec3(), f64::from(block_size));
    let (xl, yl, xh, yh) = range;
    if xl > xh || yl > yh {
        return world_tree_whole(planes, brushes, contents_of, ids, vis);
    }
    let count = ((xh - xl + 1) * (yh - yl + 1)).max(0) as usize;
    let boxes: Vec<(Vec3, Vec3)> = (0..count)
        .map(|bn| {
            let yb = yl + bn as i32 / (xh - xl + 1);
            let xb = xl + bn as i32 % (xh - xl + 1);
            let bmins = Vec3::new(xb as f32 * size, yb as f32 * size, -MAX_COORD);
            let bmaxs = Vec3::new((xb + 1) as f32 * size, (yb + 1) as f32 * size, MAX_COORD);
            (bmins, bmaxs)
        })
        .collect();
    let bounds: Vec<Option<(Vec3, Vec3)>> = ids
        .iter()
        .map(|&i| {
            let mb = &brushes[i];
            (mb.contents & contents::DETAIL == 0 && !mb.sides.is_empty()).then(|| map_brush_bounds(mb))
        })
        .collect();
    let reached: Vec<bool> = boxes
        .par_iter()
        .map(|(lo, hi)| bounds.iter().flatten().any(|(bl, bh)| (0..3).all(|j| !(bl[j] >= hi[j] || bh[j] <= lo[j]))))
        .collect();
    let before = planes.clone();
    let mut blocks_info: Vec<Block> = Vec::with_capacity(count);
    for (k, &(bmins, bmaxs)) in boxes.iter().enumerate() {
        let bounding = bounding_planes(planes, bmins, bmaxs);
        let box_planes = reached[k].then(|| bounds_plane_ids(planes, bmins, bmaxs));
        blocks_info.push(Block { mins: bmins, maxs: bmaxs, bounding, box_planes });
    }
    let shared = &*planes;
    let mut lists: Vec<Vec<BBrush>> = blocks_info
        .par_iter()
        .map(|b| {
            clip_brush_list(
                shared,
                brushes,
                ids,
                b.mins,
                b.maxs,
                b.bounding,
                DetailScreen::NoDetail,
                vis,
                Some(&bounds),
            )
        })
        .collect();
    if lists.iter().zip(&blocks_info).any(|(l, b)| l.is_empty() == b.box_planes.is_some()) {
        *planes = before;
        return world_tree_serial(planes, brushes, contents_of, ids, vis, block_size);
    }
    let mut current = contents_of.to_vec();
    let mut seen: Vec<Option<Vec<u32>>> = vec![None; count];
    for (k, list) in lists.iter_mut().enumerate() {
        if list.is_empty() {
            continue;
        }
        fixup_areaportal_water(shared, &mut current, list);
        if list.iter().any(|b| current.get(b.original).is_some_and(|c| c & contents::AREAPORTAL != 0)) {
            seen[k] = Some(current.clone());
        }
    }
    let built: Vec<Option<(Vec<BNode>, usize)>> = lists
        .into_par_iter()
        .zip(blocks_info.par_iter())
        .zip(seen.par_iter())
        .map(|((list, b), seen)| {
            let box_planes = b.box_planes?;
            Some(block_bsp(shared, seen.as_deref().unwrap_or(&current), list, box_planes))
        })
        .collect();
    let mut split_planes = Vec::new();
    block_tree_planes(planes, (xl - 1, yl - 1, xh + 1, yh + 1), size, &mut split_planes);
    contents_of.copy_from_slice(&current);
    let mut bsp = Bsp { planes: &*planes, contents_of, nodes: Vec::new() };
    let mut blocks: Vec<Option<usize>> = vec![None; count];
    for (k, b) in built.into_iter().enumerate() {
        match b {
            Some((nodes, head)) => {
                let base = bsp.nodes.len();
                for mut n in nodes {
                    if n.plane.is_some() {
                        n.children = [n.children[0] + base, n.children[1] + base];
                    }
                    n.parent = n.parent.map(|p| p + base);
                    bsp.nodes.push(n);
                }
                blocks[k] = Some(head + base);
            }
            None => {
                let n = bsp.alloc(None);
                bsp.nodes[n].contents = contents::SOLID;
                blocks[k] = Some(n);
            }
        }
    }
    let head = bsp.block_tree(&blocks, range, (xl - 1, yl - 1, xh + 1, yh + 1), &split_planes, &mut 0);
    let tmins = DVec3::new(
        f64::from(xl) * f64::from(block_size),
        f64::from(yl) * f64::from(block_size),
        f64::from(mins.z) - 8.0,
    );
    let tmaxs = DVec3::new(
        f64::from(xh + 1) * f64::from(block_size),
        f64::from(yh + 1) * f64::from(block_size),
        f64::from(maxs.z) + 8.0,
    );
    bsp.finish(head, tmins, tmaxs)
}

fn world_tree_whole(
    planes: &mut PlaneSet,
    brushes: &[MapBrush],
    contents_of: &mut [u32],
    ids: &[usize],
    vis: Visibility,
) -> Tree {
    let (mins, maxs) = map_bounds(brushes, ids);
    let lo = Vec3::splat(-MAX_COORD);
    let hi = Vec3::splat(MAX_COORD);
    let bounding = bounding_planes(planes, lo, hi);
    let box_planes = bounds_plane_ids(planes, lo, hi);
    let mut list = clip_brush_list(planes, brushes, ids, lo, hi, bounding, DetailScreen::NoDetail, vis, None);
    fixup_areaportal_water(planes, contents_of, &mut list);
    let mut bsp = Bsp { planes: &*planes, contents_of, nodes: Vec::new() };
    let head = if list.is_empty() {
        let n = bsp.alloc(None);
        bsp.nodes[n].contents = contents::SOLID;
        n
    } else {
        let list = chop_brushes(bsp.planes, bsp.contents_of, list);
        bsp.brush_bsp(list, box_planes)
    };
    let margin = DVec3::splat(8.0);
    bsp.finish(head, mins.as_dvec3() - margin, maxs.as_dvec3() + margin)
}

pub(crate) fn world_tree_serial(
    planes: &mut PlaneSet,
    brushes: &[MapBrush],
    contents_of: &mut [u32],
    ids: &[usize],
    vis: Visibility,
    block_size: u32,
) -> Tree {
    if block_size == 0 {
        return world_tree_whole(planes, brushes, contents_of, ids, vis);
    }
    let size = block_size as f32;
    let (mins, maxs) = map_bounds(brushes, ids);
    let range = block_range_sized(mins.as_dvec3(), maxs.as_dvec3(), f64::from(block_size));
    let (xl, yl, xh, yh) = range;
    if xl > xh || yl > yh {
        return world_tree_whole(planes, brushes, contents_of, ids, vis);
    }
    let count = ((xh - xl + 1) * (yh - yl + 1)).max(0) as usize;
    let mut built: Vec<Option<(Vec<BNode>, usize)>> = Vec::with_capacity(count);
    for bn in 0..count {
        let yb = yl + bn as i32 / (xh - xl + 1);
        let xb = xl + bn as i32 % (xh - xl + 1);
        let bmins = Vec3::new(xb as f32 * size, yb as f32 * size, -MAX_COORD);
        let bmaxs = Vec3::new((xb + 1) as f32 * size, (yb + 1) as f32 * size, MAX_COORD);
        let mut list = make_brush_list(planes, brushes, ids, bmins, bmaxs, DetailScreen::NoDetail, vis);
        if list.is_empty() {
            built.push(None);
            continue;
        }
        fixup_areaportal_water(planes, contents_of, &mut list);
        let box_planes = bounds_plane_ids(planes, bmins, bmaxs);
        built.push(Some(block_bsp(planes, contents_of, list, box_planes)));
    }
    let mut split_planes = Vec::new();
    block_tree_planes(planes, (xl - 1, yl - 1, xh + 1, yh + 1), size, &mut split_planes);
    let mut bsp = Bsp { planes: &*planes, contents_of, nodes: Vec::new() };
    let mut blocks: Vec<Option<usize>> = vec![None; count];
    for (k, b) in built.into_iter().enumerate() {
        match b {
            Some((nodes, head)) => {
                let base = bsp.nodes.len();
                for mut n in nodes {
                    if n.plane.is_some() {
                        n.children = [n.children[0] + base, n.children[1] + base];
                    }
                    n.parent = n.parent.map(|p| p + base);
                    bsp.nodes.push(n);
                }
                blocks[k] = Some(head + base);
            }
            None => {
                let n = bsp.alloc(None);
                bsp.nodes[n].contents = contents::SOLID;
                blocks[k] = Some(n);
            }
        }
    }
    let head = bsp.block_tree(&blocks, range, (xl - 1, yl - 1, xh + 1, yh + 1), &split_planes, &mut 0);
    let tmins = DVec3::new(
        f64::from(xl) * f64::from(block_size),
        f64::from(yl) * f64::from(block_size),
        f64::from(mins.z) - 8.0,
    );
    let tmaxs = DVec3::new(
        f64::from(xh + 1) * f64::from(block_size),
        f64::from(yh + 1) * f64::from(block_size),
        f64::from(maxs.z) + 8.0,
    );
    bsp.finish(head, tmins, tmaxs)
}

#[derive(Clone, Copy, Debug)]
pub struct ModelPlanes {
    bounding: ([usize; 2], [usize; 2]),
    box_planes: [usize; 6],
}

pub fn model_planes(planes: &mut PlaneSet) -> ModelPlanes {
    let lo = Vec3::splat(-MAX_COORD);
    let hi = Vec3::splat(MAX_COORD);
    let bounding = bounding_planes(planes, lo, hi);
    let box_planes = bounds_plane_ids(planes, lo, hi);
    ModelPlanes { bounding, box_planes }
}

pub fn model_tree(planes: &mut PlaneSet, brushes: &[MapBrush], contents_of: &[u32], ids: &[usize]) -> Tree {
    let mp = model_planes(planes);
    model_tree_with(planes, mp, brushes, contents_of, ids)
}

pub fn model_tree_with(
    planes: &PlaneSet,
    mp: ModelPlanes,
    brushes: &[MapBrush],
    contents_of: &[u32],
    ids: &[usize],
) -> Tree {
    let lo = Vec3::splat(-MAX_COORD);
    let hi = Vec3::splat(MAX_COORD);
    let list =
        clip_brush_list(planes, brushes, ids, lo, hi, mp.bounding, DetailScreen::Full, Visibility::Windings, None);
    let list = chop_brushes(planes, contents_of, list);
    let mut tmins = Vec3::splat(99999.0);
    let mut tmaxs = Vec3::splat(-99999.0);
    for b in &list {
        tmins = tmins.min(b.mins);
        tmaxs = tmaxs.max(b.maxs);
        tmins = tmins.min(b.maxs);
        tmaxs = tmaxs.max(b.mins);
    }
    let mut bsp = Bsp { planes, contents_of, nodes: Vec::new() };
    let head = bsp.brush_bsp(list, mp.box_planes);
    bsp.finish(head, tmins.as_dvec3(), tmaxs.as_dvec3())
}
