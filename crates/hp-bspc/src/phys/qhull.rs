#![allow(non_snake_case)]
#![allow(clippy::all)]
#![allow(dead_code)]

#[derive(Clone, Debug, PartialEq)]
pub struct QhFacet {
    pub id: u32,
    pub normal: [f64; 3],
    pub offset: f64,
    pub vertices: Vec<usize>,
}

pub fn qhull_ivp(points: &[[f64; 3]]) -> Result<Vec<QhFacet>, i32> {
    run(points, Opts { joggle: None, distround: 1.0e-6 })
}

pub fn qhull_ivp_joggle(points: &[[f64; 3]], joggle: f64) -> Result<Vec<QhFacet>, i32> {
    let j: f64 = format_g(joggle).parse().unwrap_or(joggle);
    run(points, Opts { joggle: Some(j), distround: 1.0e-18 })
}

pub fn ivp_joggle_sequence(n: usize) -> Vec<f64> {
    let mut v = Vec::with_capacity(n);
    let mut j = 1.0e-12f32 as f64;
    for _ in 0..n {
        v.push(j);
        j = j * (1.2f32 as f64) + (1.2e-12f32 as f64);
    }
    v
}

pub fn format_g(x: f64) -> String {
    if x == 0.0 {
        return "0".to_string();
    }
    if !x.is_finite() {
        return if x.is_nan() {
            "NAN".into()
        } else if x > 0.0 {
            "INF".into()
        } else {
            "-INF".into()
        };
    }
    let e = format!("{:.5e}", x);
    let (mant, exp) = e.split_at(e.find('e').unwrap());
    let exp: i32 = exp[1..].parse().unwrap();
    if exp < -4 || exp >= 6 {
        let mut m = mant.to_string();
        if m.contains('.') {
            while m.ends_with('0') {
                m.pop();
            }
            if m.ends_with('.') {
                m.pop();
            }
        }
        format!("{}E{}{:02}", m, if exp < 0 { '-' } else { '+' }, exp.abs())
    } else {
        let prec = (5 - exp).max(0) as usize;
        let mut s = format!("{:.*}", prec, x);
        if s.contains('.') {
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
        }
        s
    }
}

const NONE: u32 = u32::MAX;
const DUPRIDGE: u32 = u32::MAX - 1;
const MERGERIDGE: u32 = u32::MAX - 2;

const REALMAX: f64 = f64::MAX;
const REALMIN: f64 = f64::MIN_POSITIVE;
const REALEPS: f64 = f64::EPSILON;

const ERR_INPUT: i32 = 1;
const ERR_SINGULAR: i32 = 2;
const ERR_PREC: i32 = 3;
const ERR_QHULL: i32 = 5;

const QH_HASHFACTOR: i32 = 2;
const QH_JOGGLEMAXRETRY: i32 = 100;
const QH_JOGGLERETRY: i32 = 2;
const QH_JOGGLEINCREASE: f64 = 10.0;
const QH_JOGGLEMAXINCREASE: f64 = 1e-2;
const QH_DIMMERGEVERTEX: usize = 6;
const QH_DIMREDUCEBUILD: usize = 5;
const QH_BESTCENTRUM: usize = 20;
const QH_BESTCENTRUM2: usize = 2;
const QH_BESTNONCONVEX: usize = 15;
const QH_MAXNEWMERGES: i32 = 2;
const QH_MAXNEWCENTRUM: usize = 5;
const QH_COPLANARRATIO: f64 = 3.0;
const QH_RATIONEARINSIDE: f64 = 5.0;
const QH_USEFINDBESTNEW: u32 = 50;
const QH_WIDECOPLANAR: f64 = 6.0;
const QH_MAXNARROW: f64 = -0.98;
const QH_MAXNUMMERGE: u32 = 511;
const QH_ANGLEDEGEN: f64 = 5.0;
const QH_ANGLECONCAVE: f64 = 1.5;
const QH_RANDOMMAX: f64 = 2147483646.0;

type R<T> = Result<T, i32>;

const MRGCOPLANAR: u8 = 1;
const MRGANGLECOPLANAR: u8 = 2;
const MRGCONCAVE: u8 = 3;
const MRGFLIP: u8 = 4;
const MRGRIDGE: u8 = 5;
const MRGDEGEN: u8 = 6;
const MRGREDUNDANT: u8 = 7;

#[derive(Clone, Copy)]
struct Opts {
    joggle: Option<f64>,
    distround: f64,
}

#[derive(Clone)]
struct Facet {
    furthestdist: f64,
    maxoutside: f64,
    offset: f64,
    normal: [f64; 3],
    has_normal: bool,
    center: Option<[f64; 3]>,
    f: u32,
    previous: u32,
    next: u32,
    vertices: Vec<u32>,
    ridges: Vec<u32>,
    ridges_alloc: bool,
    neighbors: Vec<u32>,
    outsideset: Vec<u32>,
    coplanarset: Vec<u32>,
    visitid: u32,
    id: u32,
    nummerge: u32,
    newfacet: bool,
    visible: bool,
    toporient: bool,
    simplicial: bool,
    seen: bool,
    seen2: bool,
    flipped: bool,
    upperdelaunay: bool,
    notfurthest: bool,
    good: bool,
    dupridge: bool,
    mergeridge: bool,
    mergeridge2: bool,
    coplanar: bool,
    mergehorizon: bool,
    cycledone: bool,
    tested: bool,
    keepcentrum: bool,
    newmerge: bool,
    degenerate: bool,
    redundant: bool,
}

#[derive(Clone)]
struct Vertex {
    next: u32,
    previous: u32,
    neighbors: Vec<u32>,
    point: u32,
    visitid: u32,
    id: u32,
    seen: bool,
    seen2: bool,
    delridge: bool,
    deleted: bool,
    newlist: bool,
}

#[derive(Clone)]
struct Ridge {
    vertices: Vec<u32>,
    top: u32,
    bottom: u32,
    id: u32,
    seen: bool,
    tested: bool,
    nonconvex: bool,
}

#[derive(Clone, Copy)]
struct Merge {
    angle: f64,
    facet1: u32,
    facet2: u32,
    mtype: u8,
}

#[derive(Clone, Copy, PartialEq)]
enum ListHead {
    FacetList,
    FacetNext,
    VisibleList,
}

#[inline]
fn set_get(s: &[u32], i: usize) -> u32 {
    if i < s.len() { s[i] } else { NONE }
}

#[inline]
fn setappend(s: &mut Vec<u32>, e: u32) {
    if e != NONE {
        s.push(e);
    }
}

fn setappend2ndlast(s: &mut Vec<u32>, e: u32) {
    let n = s.len();
    if n == 0 {
        s.push(e);
    } else {
        s.insert(n - 1, e);
    }
}

fn setdel(s: &mut Vec<u32>, e: u32) -> bool {
    for i in 0..s.len() {
        let x = s[i];
        if x == NONE {
            return false;
        }
        if x == e {
            let last = s.pop().unwrap();
            if i < s.len() {
                s[i] = last;
            }
            return true;
        }
    }
    false
}

fn setdellast(s: &mut Vec<u32>) -> u32 {
    if s.is_empty() || s[0] == NONE {
        return NONE;
    }
    s.pop().unwrap()
}

fn setdelnth(s: &mut Vec<u32>, nth: usize) -> u32 {
    let e = s[nth];
    let last = s.pop().unwrap();
    if nth < s.len() {
        s[nth] = last;
    }
    e
}

fn setdelnthsorted(s: &mut Vec<u32>, nth: usize) -> u32 {
    s.remove(nth)
}

fn setdelsorted(s: &mut Vec<u32>, e: u32) -> bool {
    for i in 0..s.len() {
        let x = s[i];
        if x == NONE {
            return false;
        }
        if x == e {
            s.remove(i);
            return true;
        }
    }
    false
}

#[inline]
fn setaddnth(s: &mut Vec<u32>, nth: usize, e: u32) {
    s.insert(nth, e);
}

fn setreplace(s: &mut Vec<u32>, old: u32, new: u32) {
    for i in 0..s.len() {
        let x = s[i];
        if x == NONE {
            break;
        }
        if x == old {
            s[i] = new;
            return;
        }
    }
}

fn setin(s: &[u32], e: u32) -> bool {
    for &x in s {
        if x == NONE {
            return false;
        }
        if x == e {
            return true;
        }
    }
    false
}

fn setindex(s: &[u32], e: u32) -> i32 {
    for (i, &x) in s.iter().enumerate() {
        if x == e {
            return i as i32;
        }
    }
    -1
}

#[inline]
fn setlast(s: &[u32]) -> u32 {
    if s.is_empty() { NONE } else { s[s.len() - 1] }
}

fn setcompact(s: &mut Vec<u32>) {
    s.retain(|&x| x != NONE);
}

fn setunique(s: &mut Vec<u32>, e: u32) -> bool {
    if !setin(s, e) {
        setappend(s, e);
        true
    } else {
        false
    }
}

fn setnew_delnthsorted(s: &[u32], size: usize, nth: usize, prepend: usize) -> Vec<u32> {
    let mut v = Vec::with_capacity(size - 1 + prepend);
    for _ in 0..prepend {
        v.push(NONE);
    }
    v.extend_from_slice(&s[..nth]);
    v.extend_from_slice(&s[nth + 1..size]);
    v
}

fn setequal_skip(a: &[u32], skip_a: usize, b: &[u32], skip_b: usize) -> bool {
    let (mut ia, mut ib) = (0usize, 0usize);
    loop {
        if ia == skip_a {
            ia += 1;
        }
        if ib == skip_b {
            ib += 1;
        }
        let ea = set_get(a, ia);
        if ea == NONE {
            break;
        }
        let eb = set_get(b, ib);
        ia += 1;
        ib += 1;
        if ea != eb {
            return false;
        }
    }
    set_get(b, ib) == NONE
}

fn setequal_except(a: &[u32], skipelem_a: u32, b: &[u32], mut skipelem_b: u32) -> bool {
    let (mut ia, mut ib) = (0usize, 0usize);
    let mut skip = 0;
    loop {
        if set_get(a, ia) == skipelem_a {
            skip += 1;
            ia += 1;
        }
        if skipelem_b != NONE {
            if set_get(b, ib) == skipelem_b {
                skip += 1;
                ib += 1;
            }
        } else if set_get(a, ia) != set_get(b, ib) {
            skip += 1;
            skipelem_b = set_get(b, ib);
            ib += 1;
            if skipelem_b == NONE {
                return false;
            }
        }
        if set_get(a, ia) == NONE {
            break;
        }
        let ea = set_get(a, ia);
        let eb = set_get(b, ib);
        ia += 1;
        ib += 1;
        if ea != eb {
            return false;
        }
    }
    !(skip != 2 || set_get(b, ib) != NONE)
}

fn msvc_shortsort(a: &mut [u32], mut lo: usize, mut hi: usize, cmp: &mut dyn FnMut(u32, u32) -> i32) {
    while hi > lo {
        let mut max = lo;
        let mut p = lo + 1;
        while p <= hi {
            if cmp(a[p], a[max]) > 0 {
                max = p;
            }
            p += 1;
        }
        a.swap(max, hi);
        hi -= 1;
    }
    let _ = &mut lo;
}

fn msvc_qsort(a: &mut [u32], cmp: &mut dyn FnMut(u32, u32) -> i32) {
    let num = a.len();
    if num < 2 {
        return;
    }
    let mut lostk: Vec<usize> = Vec::new();
    let mut histk: Vec<usize> = Vec::new();
    let mut lo = 0usize;
    let mut hi = num - 1;
    loop {
        let size = hi - lo + 1;
        if size <= 8 {
            msvc_shortsort(a, lo, hi, cmp);
        } else {
            let mut mid = lo + size / 2;
            if cmp(a[lo], a[mid]) > 0 {
                a.swap(lo, mid);
            }
            if cmp(a[lo], a[hi]) > 0 {
                a.swap(lo, hi);
            }
            if cmp(a[mid], a[hi]) > 0 {
                a.swap(mid, hi);
            }
            let mut loguy = lo;
            let mut higuy = hi;
            loop {
                if mid > loguy {
                    loop {
                        loguy += 1;
                        if !(loguy < mid && cmp(a[loguy], a[mid]) <= 0) {
                            break;
                        }
                    }
                }
                if mid <= loguy {
                    loop {
                        loguy += 1;
                        if !(loguy <= hi && cmp(a[loguy], a[mid]) <= 0) {
                            break;
                        }
                    }
                }
                loop {
                    higuy -= 1;
                    if !(higuy > mid && cmp(a[higuy], a[mid]) > 0) {
                        break;
                    }
                }
                if higuy < loguy {
                    break;
                }
                a.swap(loguy, higuy);
                if mid == higuy {
                    mid = loguy;
                }
            }
            higuy += 1;
            if mid < higuy {
                loop {
                    higuy -= 1;
                    if !(higuy > mid && cmp(a[higuy], a[mid]) == 0) {
                        break;
                    }
                }
            }
            if mid >= higuy {
                loop {
                    if higuy == 0 {
                        higuy = usize::MAX;
                        break;
                    }
                    higuy -= 1;
                    if !(higuy > lo && cmp(a[higuy], a[mid]) == 0) {
                        break;
                    }
                }
            }
            let hig = higuy;
            let left_len = if hig == usize::MAX { -1isize } else { hig as isize - lo as isize };
            let right_len = hi as isize - loguy as isize;
            if left_len >= right_len {
                if hig != usize::MAX && lo < hig {
                    lostk.push(lo);
                    histk.push(hig);
                }
                if loguy < hi {
                    lo = loguy;
                    continue;
                }
            } else {
                if loguy < hi {
                    lostk.push(loguy);
                    histk.push(hi);
                }
                if hig != usize::MAX && lo < hig {
                    hi = hig;
                    continue;
                }
            }
        }
        match lostk.pop() {
            Some(l) => {
                lo = l;
                hi = histk.pop().unwrap();
            }
            None => return,
        }
    }
}

struct Qh {
    dim: usize,
    pts: Vec<f64>,
    num_points: usize,
    facets: Vec<Facet>,
    verts: Vec<Vertex>,
    ridges: Vec<Ridge>,
    merges: Vec<Merge>,

    facet_list: u32,
    facet_tail: u32,
    facet_next: u32,
    newfacet_list: u32,
    visible_list: u32,
    vertex_list: u32,
    vertex_tail: u32,
    newvertex_list: u32,
    num_facets: i32,
    num_vertices: i32,
    num_outside: i32,
    num_visible: i32,
    facet_id: u32,
    vertex_id: u32,
    ridge_id: u32,
    visit_id: u32,
    vertex_visit: u32,
    del_vertices: Vec<u32>,
    hash_table: Vec<u32>,
    facet_mergeset: Vec<u32>,
    degen_mergeset: Vec<u32>,
    searchset: Vec<u32>,
    interior_point: [f64; 3],

    ALLpoints: bool,
    PREmerge: bool,
    POSTmerge: bool,
    MERGEexact: bool,
    MERGING: bool,
    ZEROcentrum: bool,
    ZEROall_ok: bool,
    APPROXhull: bool,
    SETroundoff: bool,
    KEEPcoplanar: bool,
    KEEPinside: bool,
    KEEPnearinside: bool,
    DOcheckmax: bool,
    BESToutside: bool,
    NARROWhull: bool,
    findbestnew: bool,
    findbest_notsharp: bool,
    VERTEXneighbors: bool,
    NEWfacets: bool,
    POSTmerging: bool,
    WAScoplanar: bool,
    MERGEindependent: bool,
    ANGLEmerge: bool,
    MERGEvertices: bool,
    maxoutdone: bool,
    TESTvneighbors: bool,

    premerge_centrum: f64,
    postmerge_centrum: f64,
    premerge_cos: f64,
    postmerge_cos: f64,
    centrum_radius: f64,
    cos_max: f64,
    DISTround: f64,
    MINoutside: f64,
    MINvisible: f64,
    MAXcoplanar: f64,
    WIDEfacet: f64,
    ONEmerge: f64,
    NEARinside: f64,
    ANGLEround: f64,
    MINdenom_1: f64,
    MINdenom: f64,
    MINdenom_1_2: f64,
    MINdenom_2: f64,
    MAXabs_coord: f64,
    MAXsumcoord: f64,
    MAXwidth: f64,
    MAXlastcoord: f64,
    MINlastcoord: f64,
    NEARzero: [f64; 3],
    max_outside: f64,
    max_vertex: f64,
    min_vertex: f64,
    JOGGLEmax: f64,
    build_cnt: i32,
    furthest_id: i32,

    ztotmerge: u32,
    zsetplane: u32,
    wnewvertexmax: f64,

    rand_seed: i32,
}

impl Qh {
    fn new(points: &[[f64; 3]], o: &Opts) -> Qh {
        let mut pts = Vec::with_capacity(points.len() * 3);
        for p in points {
            pts.extend_from_slice(p);
        }
        Qh {
            dim: 3,
            pts,
            num_points: points.len(),
            facets: Vec::with_capacity(points.len() * 8 + 16),
            verts: Vec::with_capacity(points.len() + 16),
            ridges: Vec::new(),
            merges: Vec::new(),
            facet_list: NONE,
            facet_tail: NONE,
            facet_next: NONE,
            newfacet_list: NONE,
            visible_list: NONE,
            vertex_list: NONE,
            vertex_tail: NONE,
            newvertex_list: NONE,
            num_facets: 0,
            num_vertices: 0,
            num_outside: 0,
            num_visible: 0,
            facet_id: 0,
            vertex_id: 0,
            ridge_id: 0,
            visit_id: 0,
            vertex_visit: 0,
            del_vertices: Vec::new(),
            hash_table: Vec::new(),
            facet_mergeset: Vec::new(),
            degen_mergeset: Vec::new(),
            searchset: Vec::new(),
            interior_point: [0.0; 3],
            ALLpoints: true,
            PREmerge: true,
            POSTmerge: false,
            MERGEexact: false,
            MERGING: true,
            ZEROcentrum: false,
            ZEROall_ok: false,
            APPROXhull: true,
            SETroundoff: true,
            KEEPcoplanar: false,
            KEEPinside: false,
            KEEPnearinside: false,
            DOcheckmax: false,
            BESToutside: false,
            NARROWhull: false,
            findbestnew: false,
            findbest_notsharp: false,
            VERTEXneighbors: false,
            NEWfacets: false,
            POSTmerging: false,
            WAScoplanar: false,
            MERGEindependent: true,
            ANGLEmerge: true,
            MERGEvertices: true,
            maxoutdone: false,
            TESTvneighbors: false,
            premerge_centrum: 0.0,
            postmerge_centrum: 0.0,
            premerge_cos: REALMAX,
            postmerge_cos: REALMAX,
            centrum_radius: 0.0,
            cos_max: 0.0,
            DISTround: o.distround,
            MINoutside: 1e-14,
            MINvisible: REALMAX,
            MAXcoplanar: REALMAX,
            WIDEfacet: 0.0,
            ONEmerge: 0.0,
            NEARinside: 0.0,
            ANGLEround: 0.0,
            MINdenom_1: 0.0,
            MINdenom: 0.0,
            MINdenom_1_2: 0.0,
            MINdenom_2: 0.0,
            MAXabs_coord: 0.0,
            MAXsumcoord: 0.0,
            MAXwidth: -REALMAX,
            MAXlastcoord: 0.0,
            MINlastcoord: 0.0,
            NEARzero: [0.0; 3],
            max_outside: 0.0,
            max_vertex: 0.0,
            min_vertex: 0.0,
            JOGGLEmax: match o.joggle {
                Some(j) => j,
                None => REALMAX,
            },
            build_cnt: 0,
            furthest_id: -1,
            ztotmerge: 0,
            zsetplane: 0,
            wnewvertexmax: 0.0,
            rand_seed: 1,
        }
    }

    #[inline]
    fn pt(&self, id: u32) -> [f64; 3] {
        let i = id as usize * 3;
        [self.pts[i], self.pts[i + 1], self.pts[i + 2]]
    }
    #[inline]
    fn vpt(&self, v: u32) -> [f64; 3] {
        self.pt(self.verts[v as usize].point)
    }
    #[inline]
    fn fc(&self, f: u32) -> &Facet {
        &self.facets[f as usize]
    }
    #[inline]
    fn fm(&mut self, f: u32) -> &mut Facet {
        &mut self.facets[f as usize]
    }
    #[inline]
    fn vx(&self, v: u32) -> &Vertex {
        &self.verts[v as usize]
    }
    #[inline]
    fn vm(&mut self, v: u32) -> &mut Vertex {
        &mut self.verts[v as usize]
    }
    #[inline]
    fn rg(&self, r: u32) -> &Ridge {
        &self.ridges[r as usize]
    }
    #[inline]
    fn rm(&mut self, r: u32) -> &mut Ridge {
        &mut self.ridges[r as usize]
    }
    #[inline]
    fn otherfacet(&self, r: u32, f: u32) -> u32 {
        let rr = &self.ridges[r as usize];
        if rr.top == f { rr.bottom } else { rr.top }
    }
    #[inline]
    fn live(&self, f: u32) -> bool {
        f != NONE && self.facets[f as usize].next != NONE
    }
    #[inline]
    fn vlive(&self, v: u32) -> bool {
        v != NONE && self.verts[v as usize].next != NONE
    }

    fn qh_rand(&mut self) -> i32 {
        let seed = self.rand_seed;
        let hi = seed / 127773;
        let lo = seed % 127773;
        let mut test = 16807i32.wrapping_mul(lo).wrapping_sub(2836i32.wrapping_mul(hi));
        if test <= 0 {
            test = test.wrapping_add(2147483647);
        }
        self.rand_seed = test;
        test
    }
    fn qh_srand(&mut self, seed: i32) {
        let mut s = seed;
        if s < 1 {
            s = 1;
        } else if s >= 2147483647 {
            s = 2147483646;
        }
        self.rand_seed = s;
    }

    fn appendfacet(&mut self, facet: u32) {
        let tail = self.facet_tail;
        if tail == self.newfacet_list {
            self.newfacet_list = facet;
        }
        if tail == self.facet_next {
            self.facet_next = facet;
        }
        let tprev = self.fc(tail).previous;
        self.fm(facet).previous = tprev;
        self.fm(facet).next = tail;
        if tprev != NONE {
            self.fm(tprev).next = facet;
        } else {
            self.facet_list = facet;
        }
        self.fm(tail).previous = facet;
        self.num_facets += 1;
    }

    fn removefacet(&mut self, facet: u32) {
        let next = self.fc(facet).next;
        let previous = self.fc(facet).previous;
        if facet == self.newfacet_list {
            self.newfacet_list = next;
        }
        if facet == self.facet_next {
            self.facet_next = next;
        }
        if facet == self.visible_list {
            self.visible_list = next;
        }
        if previous != NONE {
            self.fm(previous).next = next;
            self.fm(next).previous = previous;
        } else {
            self.facet_list = next;
            let fl = self.facet_list;
            self.fm(fl).previous = NONE;
        }
        self.num_facets -= 1;
    }

    fn prependfacet(&mut self, facet: u32, head: ListHead) {
        let list = match head {
            ListHead::FacetList => self.facet_list,
            ListHead::FacetNext => self.facet_next,
            ListHead::VisibleList => self.visible_list,
        };
        let prevfacet = self.fc(list).previous;
        self.fm(facet).previous = prevfacet;
        if prevfacet != NONE {
            self.fm(prevfacet).next = facet;
        }
        self.fm(list).previous = facet;
        self.fm(facet).next = list;
        if self.facet_list == list {
            self.facet_list = facet;
        }
        if self.facet_next == list {
            self.facet_next = facet;
        }
        match head {
            ListHead::FacetList => self.facet_list = facet,
            ListHead::FacetNext => self.facet_next = facet,
            ListHead::VisibleList => self.visible_list = facet,
        }
        self.num_facets += 1;
    }

    fn appendvertex(&mut self, vertex: u32) {
        let tail = self.vertex_tail;
        if tail == self.newvertex_list {
            self.newvertex_list = vertex;
        }
        self.vm(vertex).newlist = true;
        let tprev = self.vx(tail).previous;
        self.vm(vertex).previous = tprev;
        self.vm(vertex).next = tail;
        if tprev != NONE {
            self.vm(tprev).next = vertex;
        } else {
            self.vertex_list = vertex;
        }
        self.vm(tail).previous = vertex;
        self.num_vertices += 1;
    }

    fn removevertex(&mut self, vertex: u32) {
        let next = self.vx(vertex).next;
        let previous = self.vx(vertex).previous;
        if vertex == self.newvertex_list {
            self.newvertex_list = next;
        }
        if previous != NONE {
            self.vm(previous).next = next;
            self.vm(next).previous = previous;
        } else {
            self.vertex_list = next;
            let vl = self.vertex_list;
            self.vm(vl).previous = NONE;
        }
        self.num_vertices -= 1;
    }

    fn newfacet(&mut self) -> u32 {
        let id = self.facet_id;
        self.facet_id += 1;
        let f = Facet {
            furthestdist: 0.0,
            maxoutside: self.DISTround,
            offset: 0.0,
            normal: [0.0; 3],
            has_normal: false,
            center: None,
            f: NONE,
            previous: NONE,
            next: NONE,
            vertices: Vec::new(),
            ridges: Vec::new(),
            ridges_alloc: false,
            neighbors: Vec::with_capacity(4),
            outsideset: Vec::new(),
            coplanarset: Vec::new(),
            visitid: 0,
            id,
            nummerge: 0,
            newfacet: true,
            visible: false,
            toporient: false,
            simplicial: true,
            seen: false,
            seen2: false,
            flipped: false,
            upperdelaunay: false,
            notfurthest: false,
            good: true,
            dupridge: false,
            mergeridge: false,
            mergeridge2: false,
            coplanar: false,
            mergehorizon: false,
            cycledone: false,
            tested: false,
            keepcentrum: false,
            newmerge: false,
            degenerate: false,
            redundant: false,
        };
        self.facets.push(f);
        assert!(self.facets.len() < MERGERIDGE as usize);
        (self.facets.len() - 1) as u32
    }

    fn newridge(&mut self) -> u32 {
        let id = self.ridge_id;
        self.ridge_id = self.ridge_id.wrapping_add(1);
        self.ridges.push(Ridge {
            vertices: Vec::new(),
            top: NONE,
            bottom: NONE,
            id,
            seen: false,
            tested: false,
            nonconvex: false,
        });
        (self.ridges.len() - 1) as u32
    }

    fn newvertex(&mut self, point: u32) -> R<u32> {
        if self.vertex_id == 0xFFFFFF {
            return Err(ERR_INPUT);
        }
        let id = self.vertex_id;
        self.vertex_id += 1;
        self.verts.push(Vertex {
            next: NONE,
            previous: NONE,
            neighbors: Vec::new(),
            point,
            visitid: 0,
            id,
            seen: false,
            seen2: false,
            delridge: false,
            deleted: false,
            newlist: false,
        });
        Ok((self.verts.len() - 1) as u32)
    }

    fn delfacet(&mut self, facet: u32) {
        self.removefacet(facet);
        let f = self.fm(facet);
        f.has_normal = false;
        f.center = None;
        f.neighbors = Vec::new();
        f.ridges = Vec::new();
        f.ridges_alloc = false;
        f.vertices = Vec::new();
        f.outsideset = Vec::new();
        f.coplanarset = Vec::new();
    }

    fn delridge(&mut self, ridge: u32) {
        let top = self.rg(ridge).top;
        let bottom = self.rg(ridge).bottom;
        setdel(&mut self.facets[top as usize].ridges, ridge);
        setdel(&mut self.facets[bottom as usize].ridges, ridge);
        self.rm(ridge).vertices = Vec::new();
    }

    fn delvertex(&mut self, vertex: u32) {
        self.removevertex(vertex);
        self.vm(vertex).neighbors = Vec::new();
    }
}

#[inline]
fn fmax_(a: f64, b: f64) -> f64 {
    if a < b { b } else { a }
}

impl Qh {
    #[inline]
    fn distplane(&self, p: &[f64; 3], facet: u32) -> f64 {
        let f = &self.facets[facet as usize];
        let n = &f.normal;
        f.offset + p[0] * n[0] + p[1] * n[1] + p[2] * n[2]
    }
    #[inline]
    fn distplane_pt(&self, point: u32, facet: u32) -> f64 {
        let p = self.pt(point);
        self.distplane(&p, facet)
    }

    fn normalize2(&mut self, normal: &mut [f64; 3], toporient: bool) {
        let dim = self.dim;
        let mut norm = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if norm > self.MINdenom {
            if !toporient {
                norm = -norm;
            }
            let t = 1.0 / norm;
            normal[0] *= t;
            normal[1] *= t;
            normal[2] *= t;
        } else if norm == 0.0 {
            let temp = (1.0 / dim as f64).sqrt();
            normal[0] = temp;
            normal[1] = temp;
            normal[2] = temp;
        } else {
            if !toporient {
                norm = -norm;
            }
            for k in 0..dim {
                let (temp, zerodiv) = divzero(normal[k], norm, self.MINdenom_1);
                if !zerodiv {
                    normal[k] = temp;
                } else {
                    let mut maxk = 0usize;
                    let mut maxval = -REALMAX;
                    for kk in 0..dim {
                        let a = normal[kk].abs();
                        if a > maxval {
                            maxval = a;
                            maxk = kk;
                        }
                    }
                    let temp = if normal[maxk] * norm >= 0.0 { 1.0 } else { -1.0 };
                    for kk in 0..dim {
                        normal[kk] = 0.0;
                    }
                    normal[maxk] = temp;
                    return;
                }
            }
        }
    }

    fn sethyperplane_det(
        &mut self,
        rows: &[[f64; 3]; 3],
        toporient: bool,
        normal: &mut [f64; 3],
        offset: &mut f64,
    ) -> bool {
        let p0 = rows[0];
        let dx = |i: usize| rows[i][0] - p0[0];
        let dy = |i: usize| rows[i][1] - p0[1];
        let dz = |i: usize| rows[i][2] - p0[2];
        normal[0] = dy(2) * dz(1) - dz(2) * dy(1);
        normal[1] = dx(1) * dz(2) - dz(1) * dx(2);
        normal[2] = dx(2) * dy(1) - dy(2) * dx(1);
        self.normalize2(normal, toporient);
        *offset = -(p0[0] * normal[0] + p0[1] * normal[1] + p0[2] * normal[2]);
        let maxround = self.DISTround;
        let mut nearzero = false;
        let mut i = 3;
        while i > 0 {
            i -= 1;
            if i != 0 {
                let point = rows[i];
                let dist = *offset + (point[0] * normal[0] + point[1] * normal[1] + point[2] * normal[2]);
                if dist > maxround || dist < -maxround {
                    nearzero = true;
                    break;
                }
            }
        }
        nearzero
    }

    fn gausselim(&mut self, rows: &mut [[f64; 3]; 3], numrow: usize, numcol: usize, sign: &mut bool) -> bool {
        let mut nearzero = false;
        let mut pivot_abs = 0.0;
        for k in 0..numrow {
            pivot_abs = rows[k][k].abs();
            let mut pivoti = k;
            for i in k + 1..numrow {
                let temp = rows[i][k].abs();
                if temp > pivot_abs {
                    pivot_abs = temp;
                    pivoti = i;
                }
            }
            if pivoti != k {
                rows.swap(pivoti, k);
                *sign ^= true;
            }
            if pivot_abs <= self.NEARzero[k] {
                nearzero = true;
                if pivot_abs == 0.0 {
                    continue;
                }
            }
            let pivot = rows[k][k];
            for i in k + 1..numrow {
                let n = (1.0 / pivot) * rows[i][k];
                for j in k + 1..numcol {
                    rows[i][j] -= n * rows[k][j];
                }
            }
        }
        let _ = pivot_abs;
        nearzero
    }

    fn backnormal(
        &mut self,
        rows: &[[f64; 3]; 3],
        numrow: usize,
        numcol: usize,
        sign: bool,
        normal: &mut [f64; 3],
    ) -> bool {
        let mut nearzero = false;
        let mut zerocol: i32 = -1;
        normal[numcol - 1] = if sign { -1.0 } else { 1.0 };
        let mut i = numrow;
        while i > 0 {
            i -= 1;
            normal[i] = 0.0;
            for j in i + 1..numcol {
                normal[i] -= rows[i][j] * normal[j];
            }
            let diagonal = rows[i][i];
            if diagonal.abs() > self.MINdenom_2 {
                normal[i] /= diagonal;
            } else {
                let (v, waszero) = divzero(normal[i], diagonal, self.MINdenom_1_2);
                normal[i] = v;
                if waszero {
                    zerocol = i as i32;
                    normal[i] = if sign { -1.0 } else { 1.0 };
                    for t in i + 1..numcol {
                        normal[t] = 0.0;
                    }
                }
            }
        }
        if zerocol != -1 {
            nearzero = true;
        }
        nearzero
    }

    fn sethyperplane_gauss(
        &mut self,
        rows: &mut [[f64; 3]; 3],
        point0: &[f64; 3],
        toporient: bool,
        normal: &mut [f64; 3],
        offset: &mut f64,
    ) -> bool {
        let dim = self.dim;
        let mut sign = toporient;
        let mut nearzero = self.gausselim(rows, dim - 1, dim, &mut sign);
        let mut k = dim - 1;
        while k > 0 {
            k -= 1;
            if rows[k][k] < 0.0 {
                sign ^= true;
            }
        }
        let nearzero2 = self.backnormal(rows, dim - 1, dim, sign, normal);
        if nearzero2 {
            nearzero = true;
        }
        self.normalize2(normal, true);
        *offset = -(point0[0] * normal[0]);
        for k in 1..dim {
            *offset -= point0[k] * normal[k];
        }
        nearzero
    }

    fn setfacetplane(&mut self, facet: u32) {
        self.zsetplane += 1;
        let nv = self.fc(facet).vertices.len();
        let v0 = self.fc(facet).vertices[0];
        let point0 = self.vpt(v0);
        let toporient = self.fc(facet).toporient;
        let mut normal = [0.0f64; 3];
        let mut offset = 0.0f64;
        let mut rows = [[0.0f64; 3]; 3];
        for i in 0..3.min(nv) {
            let v = self.fc(facet).vertices[i];
            rows[i] = self.vpt(v);
        }
        let nearzero = self.sethyperplane_det(&rows, toporient, &mut normal, &mut offset);
        if nearzero {
            let mut grows = [[0.0f64; 3]; 3];
            let mut i = 0;
            let point0id = self.vx(v0).point;
            for vi in 0..nv {
                let v = self.fc(facet).vertices[vi];
                if self.vx(v).point != point0id {
                    let p = self.vpt(v);
                    if i < 3 {
                        grows[i] = [p[0] - point0[0], p[1] - point0[1], p[2] - point0[2]];
                    }
                    i += 1;
                }
            }
            let nz = self.sethyperplane_gauss(&mut grows, &point0, toporient, &mut normal, &mut offset);
            {
                let f = self.fm(facet);
                f.normal = normal;
                f.offset = offset;
                f.has_normal = true;
            }
            if nz {
                self.orientoutside(facet);
                normal = self.fc(facet).normal;
                offset = self.fc(facet).offset;
            }
        }
        {
            let f = self.fm(facet);
            f.normal = normal;
            f.offset = offset;
            f.has_normal = true;
            f.upperdelaunay = false;
        }
        if self.JOGGLEmax < REALMAX {
            let point0id = self.vx(v0).point;
            for vi in 0..nv {
                let v = self.fc(facet).vertices[vi];
                let p = self.vx(v).point;
                if p != point0id {
                    let mut dist = self.distplane_pt(p, facet);
                    dist = dist.abs();
                    if dist > self.wnewvertexmax {
                        self.wnewvertexmax = dist;
                        if dist > self.max_outside {
                            self.max_outside = dist;
                        }
                    }
                }
            }
        }
    }

    fn orientoutside(&mut self, facet: u32) -> bool {
        let ip = self.interior_point;
        let dist = self.distplane(&ip, facet);
        if dist > 0.0 {
            let f = self.fm(facet);
            let mut k = 3;
            while k > 0 {
                k -= 1;
                f.normal[k] = -f.normal[k];
            }
            f.offset = -f.offset;
            return true;
        }
        false
    }

    fn getcenter(&self, vertices: &[u32]) -> R<[f64; 3]> {
        let count = vertices.len();
        if count < 2 {
            return Err(ERR_QHULL);
        }
        let mut c = [0.0f64; 3];
        let r = 1.0 / count as f64;
        for k in 0..3 {
            let mut acc = 0.0f64;
            for &v in vertices {
                if v == NONE {
                    break;
                }
                acc += self.pts[self.verts[v as usize].point as usize * 3 + k];
            }
            c[k] = r * acc;
        }
        Ok(c)
    }

    fn projectpoint(&self, point: &[f64; 3], facet: u32, dist: f64) -> [f64; 3] {
        let n = self.fc(facet).normal;
        [point[0] - dist * n[0], point[1] - dist * n[1], point[2] - dist * n[2]]
    }

    fn getcentrum(&self, facet: u32) -> R<[f64; 3]> {
        let point = self.getcenter(&self.fc(facet).vertices)?;
        let dist = self.distplane(&point, facet);
        Ok(self.projectpoint(&point, facet, dist))
    }

    #[inline]
    fn getangle(&self, a: &[f64; 3], b: &[f64; 3]) -> f64 {
        let mut angle = 0.0f64;
        for k in 0..3 {
            angle += a[k] * b[k];
        }
        angle
    }

    fn getdistance(&mut self, facet: u32, neighbor: u32) -> (f64, f64, f64) {
        let fv = self.fc(facet).vertices.clone();
        for &v in &fv {
            if v == NONE {
                break;
            }
            self.vm(v).seen = false;
        }
        let nv = self.fc(neighbor).vertices.clone();
        for &v in &nv {
            if v == NONE {
                break;
            }
            self.vm(v).seen = true;
        }
        let mut mind = 0.0f64;
        let mut maxd = 0.0f64;
        for &v in &fv {
            if v == NONE {
                break;
            }
            if !self.vx(v).seen {
                let dist = self.distplane_pt(self.vx(v).point, neighbor);
                if dist < mind {
                    mind = dist;
                } else if dist > maxd {
                    maxd = dist;
                }
            }
        }
        let mindist = mind;
        let maxdist = maxd;
        mind = -mind;
        let d = if maxd > mind { maxd } else { mind };
        (d, mindist, maxdist)
    }

    fn determinant(&self, rows: &[[f64; 3]; 3], dim: usize) -> (f64, bool) {
        let mut nearzero = false;
        let det;
        if dim == 2 {
            det = rows[0][0] * rows[1][1] - rows[0][1] * rows[1][0];
            if det.abs() < self.NEARzero[1] {
                nearzero = true;
            }
        } else {
            let (a1, a2, a3) = (rows[0][0], rows[0][1], rows[0][2]);
            let (b1, b2, b3) = (rows[1][0], rows[1][1], rows[1][2]);
            let (c1, c2, c3) = (rows[2][0], rows[2][1], rows[2][2]);
            det = a1 * (b2 * c3 - b3 * c2) - b1 * (a2 * c3 - a3 * c2) + c1 * (a2 * b3 - a3 * b2);
            if det.abs() < self.NEARzero[2] {
                nearzero = true;
            }
        }
        (det, nearzero)
    }

    fn detsimplex(&self, apex: u32, points: &[u32], dim: usize) -> R<(f64, bool)> {
        let a = self.pt(apex);
        let mut rows = [[0.0f64; 3]; 3];
        let mut i = 0;
        for &p in points {
            if p == NONE {
                break;
            }
            if i == dim {
                break;
            }
            let q = self.pt(p);
            for k in 0..dim {
                rows[i][k] = q[k] - a[k];
            }
            i += 1;
        }
        if i < dim {
            return Err(ERR_QHULL);
        }
        Ok(self.determinant(&rows, dim))
    }

    fn maxmin(&mut self) -> R<Vec<u32>> {
        let dimension = self.dim;
        self.max_outside = 0.0;
        self.MAXabs_coord = 0.0;
        self.MAXwidth = -REALMAX;
        self.MAXsumcoord = 0.0;
        self.min_vertex = 0.0;
        self.WAScoplanar = false;
        if self.ZEROcentrum {
            self.ZEROall_ok = true;
        }
        let mut set = Vec::with_capacity(2 * dimension);
        let n = self.num_points;
        for k in 0..dimension {
            let mut minimum = 0usize;
            let mut maximum = 0usize;
            for p in 0..n {
                let v = self.pts[p * 3 + k];
                if self.pts[maximum * 3 + k] < v {
                    maximum = p;
                } else if self.pts[minimum * 3 + k] > v {
                    minimum = p;
                }
            }
            let mx = self.pts[maximum * 3 + k];
            let mn = self.pts[minimum * 3 + k];
            if k == dimension - 1 {
                self.MINlastcoord = mn;
                self.MAXlastcoord = mx;
            }
            let maxcoord = fmax_(mx, -mn);
            let temp = mx - mn;
            if self.MAXwidth < temp {
                self.MAXwidth = temp;
            }
            if self.MAXabs_coord < maxcoord {
                self.MAXabs_coord = maxcoord;
            }
            self.MAXsumcoord += maxcoord;
            set.push(maximum as u32);
            set.push(minimum as u32);
            self.NEARzero[k] = 80.0 * self.MAXsumcoord * REALEPS;
        }
        Ok(set)
    }

    fn detroundoff(&mut self) -> R<()> {
        let dimf = self.dim as f64;
        if !self.SETroundoff {
            self.DISTround = distround_dll(self.dim, self.MAXabs_coord, self.MAXsumcoord);
        }
        self.MINdenom_1 = fmax_(1.0 / REALMAX, REALMIN);
        self.MINdenom = self.MAXabs_coord * self.MINdenom_1;
        self.MINdenom_1_2 = (dimf * self.MINdenom_1).sqrt();
        self.MINdenom_2 = self.MINdenom_1_2 * self.MAXabs_coord;
        self.ANGLEround = dimf * (1.01 * REALEPS);
        if self.premerge_cos < REALMAX / 2.0 {
            self.premerge_cos -= self.ANGLEround;
        }
        if self.postmerge_cos < REALMAX / 2.0 {
            self.postmerge_cos -= self.ANGLEround;
        }
        let d2 = self.DISTround * 2.0;
        self.premerge_centrum += d2;
        self.postmerge_centrum += d2;
        {
            let mut maxangle = 1.0f64;
            if self.premerge_cos < maxangle {
                maxangle = self.premerge_cos;
            }
            if self.postmerge_cos < maxangle {
                maxangle = self.postmerge_cos;
            }
            self.ONEmerge = (1.0 - maxangle * maxangle).sqrt() * (dimf.sqrt() * self.MAXwidth) + self.DISTround;
            let maxrho = dimf * self.premerge_centrum + self.DISTround;
            if self.ONEmerge < maxrho {
                self.ONEmerge = maxrho;
            }
            let maxrho = dimf * self.postmerge_centrum + self.DISTround;
            if self.ONEmerge < maxrho {
                self.ONEmerge = maxrho;
            }
        }
        self.NEARinside = self.ONEmerge * QH_RATIONEARINSIDE;
        if self.JOGGLEmax < REALMAX / 2.0 && (self.KEEPcoplanar || self.KEEPinside) {
            self.KEEPnearinside = true;
            let mut maxdist = dimf.sqrt() * self.JOGGLEmax + self.DISTround;
            maxdist = maxdist + maxdist;
            if self.NEARinside < maxdist {
                self.NEARinside = maxdist;
            }
        }
        if self.JOGGLEmax < self.DISTround {
            return Err(ERR_INPUT);
        }
        if self.MINvisible > REALMAX / 2.0 {
            if !self.MERGING {
                self.MINvisible = self.DISTround;
            } else if self.dim <= 3 {
                self.MINvisible = self.premerge_centrum;
            } else {
                self.MINvisible = self.premerge_centrum * QH_COPLANARRATIO;
            }
            if self.APPROXhull && self.MINvisible > self.MINoutside {
                self.MINvisible = self.MINoutside;
            }
        }
        if self.MAXcoplanar > REALMAX / 2.0 {
            self.MAXcoplanar = self.MINvisible;
        }
        if !self.APPROXhull {
            self.MINoutside = self.MINvisible * 2.0;
            if self.premerge_cos < REALMAX / 2.0 {
                let t = (1.0 - self.premerge_cos) * self.MAXabs_coord;
                if self.MINoutside < t {
                    self.MINoutside = t;
                }
            }
        }
        self.WIDEfacet = self.MINoutside;
        if self.WIDEfacet < self.MAXcoplanar * QH_WIDECOPLANAR {
            self.WIDEfacet = self.MAXcoplanar * QH_WIDECOPLANAR;
        }
        if self.WIDEfacet < self.MINvisible * QH_WIDECOPLANAR {
            self.WIDEfacet = self.MINvisible * QH_WIDECOPLANAR;
        }
        self.max_vertex = self.DISTround;
        self.min_vertex = -self.DISTround;
        Ok(())
    }

    fn maxsimplex(&mut self, dim: usize, simplex: &mut Vec<u32>) -> R<()> {
        let n = self.num_points;
        let mut sizinit = simplex.len();
        if sizinit < 2 {
            let mut maxcoord = -REALMAX;
            let mut mincoord = REALMAX;
            let mut maxx = NONE;
            let mut minx = NONE;
            for p in 0..n {
                let x = self.pts[p * 3];
                if maxcoord < x {
                    maxcoord = x;
                    maxx = p as u32;
                }
                if mincoord > x {
                    mincoord = x;
                    minx = p as u32;
                }
            }
            setunique(simplex, minx);
            if simplex.len() < 2 {
                setunique(simplex, maxx);
            }
            sizinit = simplex.len();
            if sizinit < 2 {
                if self.zsetplane > self.dim as u32 + 1 {
                    return Err(ERR_PREC);
                } else {
                    return Err(ERR_INPUT);
                }
            }
        }
        for k in sizinit..dim + 1 {
            let mut maxpoint = NONE;
            let mut maxdet = -REALMAX;
            for p in 0..n as u32 {
                if !setin(simplex, p) {
                    let (det, _nz) = self.detsimplex(p, simplex, k)?;
                    let det = det.abs();
                    if det > maxdet {
                        maxdet = det;
                        maxpoint = p;
                    }
                }
            }
            if maxpoint == NONE {
                return Err(ERR_QHULL);
            }
            setappend(simplex, maxpoint);
        }
        Ok(())
    }

    fn checkflipped(&mut self, facet: u32, distp: Option<&mut f64>, allerror: bool) -> bool {
        if self.fc(facet).flipped && distp.is_none() {
            return false;
        }
        let ip = self.interior_point;
        let dist = self.distplane(&ip, facet);
        if let Some(d) = distp {
            *d = dist;
        }
        if (allerror && dist > -self.DISTround) || (!allerror && dist >= 0.0) {
            self.fm(facet).flipped = true;
            return false;
        }
        true
    }
}

fn divzero(numer: f64, denom: f64, mindenom1: f64) -> (f64, bool) {
    if numer < mindenom1 && numer > -mindenom1 {
        let numerx = numer.abs();
        let denomx = denom.abs();
        if numerx < denomx { (numer / denom, false) } else { (0.0, true) }
    } else {
        let temp = denom / numer;
        if temp > mindenom1 || temp < -mindenom1 { (numer / denom, false) } else { (0.0, true) }
    }
}

fn distround_dll(dimension: usize, maxabs: f64, maxsumabs: f64) -> f64 {
    let d = dimension as f64;
    let mut maxdistsum = d.sqrt() * maxabs;
    if maxsumabs <= maxdistsum {
        maxdistsum = maxsumabs;
    }
    d * maxdistsum * 2.242650509742816e-16 + maxabs * REALEPS
}

impl Qh {
    fn findbest(
        &mut self,
        point: &[f64; 3],
        startfacet: u32,
        bestoutside: bool,
        newfacets: bool,
        noupper: bool,
        isoutside: Option<&mut bool>,
        numpart: &mut i32,
    ) -> R<(u32, f64)> {
        let has_iso = isoutside.is_some();
        let mut bestdist = -REALMAX / 2.0;
        let mut bestfacet = NONE;
        let mut searchsize: i32 = 0;
        let ischeckmax = bestoutside && !newfacets && !has_iso;
        let ispartition = newfacets && has_iso;
        let isfindfacet = !newfacets && has_iso;
        let testhorizon = ispartition && (bestoutside || self.APPROXhull || self.MERGING);
        if !ischeckmax && !ispartition && !isfindfacet {
            return Err(ERR_QHULL);
        }
        let mut iso_val = true;
        let mut dist;
        if !self.fc(startfacet).flipped {
            *numpart = 1;
            dist = self.distplane(point, startfacet);
            if !self.fc(startfacet).upperdelaunay || (!noupper && dist >= self.MINoutside) {
                bestdist = dist;
                bestfacet = startfacet;
                if !bestoutside && dist >= self.MINoutside {
                    if let Some(o) = isoutside {
                        *o = true;
                    }
                    return Ok((bestfacet, dist));
                }
            }
            if ischeckmax && dist > self.fc(startfacet).maxoutside {
                self.fm(startfacet).maxoutside = dist;
            }
        }
        let fmx = fmax_(self.MINvisible, self.MAXcoplanar);
        let searchdist = if ispartition { self.DISTround * 2.0 } else { fmx + self.DISTround * 2.0 + self.max_outside };
        let mut cutoff = bestdist - searchdist;
        let mut mincutoff = 0.0;
        if ischeckmax {
            mincutoff = -(self.DISTround - fmx);
            if mincutoff < cutoff {
                cutoff = mincutoff;
            }
        }
        self.visit_id = self.visit_id.wrapping_add(1);
        let vid0 = self.visit_id;
        self.fm(startfacet).visitid = vid0;
        let mut facet = startfacet;
        'outer: loop {
            'restart: loop {
                let mut newbest = false;
                let mut ni = 0usize;
                loop {
                    let neighbor = set_get(&self.fc(facet).neighbors, ni);
                    if neighbor == NONE {
                        break;
                    }
                    ni += 1;
                    if ispartition && !self.fc(neighbor).newfacet {
                        continue;
                    }
                    if !self.fc(neighbor).flipped {
                        if self.fc(neighbor).visitid == self.visit_id {
                            continue;
                        }
                        let vid = self.visit_id;
                        self.fm(neighbor).visitid = vid;
                        *numpart += 1;
                        dist = self.distplane(point, neighbor);
                        let _ = dist;
                        if !bestoutside && dist >= self.MINoutside && (!noupper || !self.fc(facet).upperdelaunay) {
                            bestfacet = neighbor;
                            if let Some(o) = isoutside {
                                *o = true;
                            }
                            return Ok((bestfacet, dist));
                        }
                        if ischeckmax {
                            if dist > self.fc(neighbor).maxoutside {
                                self.fm(neighbor).maxoutside = dist;
                            } else if bestfacet != NONE && dist < cutoff {
                                continue;
                            }
                        } else if bestfacet != NONE && dist < cutoff {
                            continue;
                        }
                        if dist > bestdist {
                            if !self.fc(neighbor).upperdelaunay || (bestoutside && !noupper && dist >= self.MINoutside)
                            {
                                if ischeckmax {
                                    bestdist = dist;
                                    bestfacet = neighbor;
                                    cutoff = bestdist - searchdist;
                                    if mincutoff < cutoff {
                                        cutoff = mincutoff;
                                    }
                                } else if dist > bestdist + searchdist {
                                    bestdist = dist;
                                    bestfacet = neighbor;
                                    cutoff = bestdist - searchdist;
                                    searchsize = 0;
                                    facet = neighbor;
                                    if newbest {
                                        self.visit_id = self.visit_id.wrapping_add(1);
                                        let v = self.visit_id;
                                        self.fm(facet).visitid = v;
                                    }
                                    continue 'restart;
                                } else {
                                    bestdist = dist;
                                    bestfacet = neighbor;
                                    cutoff = bestdist - searchdist;
                                }
                                newbest = true;
                            }
                        }
                    }
                    if searchsize == 0 {
                        self.searchset.clear();
                        self.searchset.push(neighbor);
                    } else {
                        self.searchset.push(neighbor);
                    }
                    searchsize += 1;
                }
                let _ = newbest;
                break 'restart;
            }
            if searchsize == 0 {
                break 'outer;
            }
            let f = setdellast(&mut self.searchset);
            if f == NONE {
                break 'outer;
            }
            facet = f;
        }
        if !ischeckmax {
            if bestfacet == NONE {
                return Err(ERR_QHULL);
            }
            if ispartition && !self.findbest_notsharp && bestdist < -self.DISTround {
                let (issharp, bf, bd) = self.findbestsharp(point, bestfacet, bestdist, numpart);
                bestfacet = bf;
                bestdist = bd;
                if issharp {
                    self.findbestnew = true;
                } else {
                    self.findbest_notsharp = true;
                }
            }
            if testhorizon {
                let f = set_get(&self.fc(bestfacet).neighbors, 0);
                *numpart += 1;
                let d = self.distplane(point, f);
                if d > bestdist && (!self.fc(f).upperdelaunay || (!noupper && d >= self.MINoutside)) {
                    bestdist = d;
                    bestfacet = f;
                }
            }
        }
        if has_iso && bestdist < self.MINoutside {
            iso_val = false;
        }
        if let Some(o) = isoutside {
            *o = iso_val;
        }
        Ok((bestfacet, bestdist))
    }

    fn findbestsharp(
        &mut self,
        point: &[f64; 3],
        mut bestfacet: u32,
        mut bestdist: f64,
        numpart: &mut i32,
    ) -> (bool, u32, f64) {
        let mut issharp = false;
        let mut quadrant = [false; 3];
        let mut facet = self.newfacet_list;
        while self.live(facet) {
            if facet == self.newfacet_list {
                let n = self.fc(facet).normal;
                for k in 0..3 {
                    quadrant[k] = n[k] > 0.0;
                }
            } else if !issharp {
                let n = self.fc(facet).normal;
                let mut k = 3;
                while k > 0 {
                    k -= 1;
                    if quadrant[k] != (n[k] > 0.0) {
                        issharp = true;
                        break;
                    }
                }
            }
            if self.fc(facet).visitid != self.visit_id {
                let dist = self.distplane(point, facet);
                *numpart += 1;
                if dist > bestdist {
                    if !self.fc(facet).upperdelaunay || dist > self.MINoutside {
                        bestdist = dist;
                        bestfacet = facet;
                    }
                }
            }
            facet = self.fc(facet).next;
        }
        (issharp, bestfacet, bestdist)
    }

    fn findbestnew(
        &mut self,
        point: &[f64; 3],
        startfacet: u32,
        isoutside: Option<&mut bool>,
        numpart: &mut i32,
    ) -> R<(u32, f64)> {
        let mut bestdist = -REALMAX;
        let mut bestdist2 = -REALMAX;
        let mut bestfacet = NONE;
        let mut bestfacet2 = NONE;
        if startfacet == NONE {
            return Err(ERR_QHULL);
        }
        let has_iso = isoutside.is_some();
        let distoutside = if self.BESToutside || !has_iso {
            REALMAX
        } else if self.MERGING {
            fmax_(4.0 * self.MINoutside, 2.0 * self.max_outside)
        } else {
            self.MINoutside
        };
        let mut iso_val = true;
        *numpart = 0;
        let mut dist = 0.0;
        let mut i = 0;
        let mut facet = startfacet;
        while i < 2 {
            while self.live(facet) {
                if facet == startfacet && i != 0 {
                    break;
                }
                dist = self.distplane(point, facet);
                *numpart += 1;
                if self.fc(facet).upperdelaunay {
                    if dist > bestdist2 {
                        bestdist2 = dist;
                        bestfacet2 = facet;
                        if dist >= distoutside {
                            bestfacet = facet;
                            if let Some(o) = isoutside {
                                *o = true;
                            }
                            return Ok((bestfacet, dist));
                        }
                    }
                } else if dist > bestdist {
                    bestdist = dist;
                    bestfacet = facet;
                    if dist >= distoutside {
                        if let Some(o) = isoutside {
                            *o = true;
                        }
                        return Ok((bestfacet, dist));
                    }
                }
                facet = self.fc(facet).next;
            }
            i += 1;
            facet = self.newfacet_list;
        }
        let newfacet = if bestfacet != NONE { bestfacet } else { bestfacet2 };
        if newfacet == NONE {
            return Err(ERR_QHULL);
        }
        let mut ni = 0usize;
        loop {
            let neighbor = set_get(&self.fc(newfacet).neighbors, ni);
            if neighbor == NONE {
                break;
            }
            ni += 1;
            if !self.fc(neighbor).newfacet {
                let d = self.distplane(point, neighbor);
                *numpart += 1;
                if self.fc(neighbor).upperdelaunay {
                    if d > bestdist2 {
                        bestdist2 = d;
                        bestfacet2 = neighbor;
                    }
                } else if d > bestdist {
                    bestdist = d;
                    bestfacet = neighbor;
                }
            }
        }
        let _ = dist;
        if bestfacet == NONE || (has_iso && bestdist2 >= self.MINoutside && bestdist2 > bestdist) {
            dist = bestdist2;
            bestfacet = bestfacet2;
        } else {
            dist = bestdist;
        }
        if has_iso && dist < self.MINoutside {
            iso_val = false;
        }
        if let Some(o) = isoutside {
            *o = iso_val;
        }
        Ok((bestfacet, dist))
    }
}

impl Qh {
    fn ridge_append(&mut self, facet: u32, ridge: u32) {
        let f = self.fm(facet);
        f.ridges.push(ridge);
        f.ridges_alloc = true;
    }

    fn initbuild(&mut self) -> R<()> {
        self.furthest_id = -1;
        self.facet_id = 0;
        self.vertex_id = 0;
        self.ridge_id = 0;
        self.visit_id = 0;
        self.vertex_visit = 0;
        self.maxoutdone = false;
        let _maxpoints = self.maxmin()?;
        self.detroundoff()?;
        let vertices = self.initialvertices()?;
        self.initialhull(&vertices)?;
        self.partitionall(&vertices)?;
        self.resetlists();
        self.facet_next = self.facet_list;
        self.furthestnext();
        if self.PREmerge {
            self.cos_max = self.premerge_cos;
            self.centrum_radius = self.premerge_centrum;
        }
        Ok(())
    }

    fn initialvertices(&mut self) -> R<Vec<u32>> {
        let dim = self.dim;
        let mut simplex: Vec<u32> = Vec::with_capacity(dim + 1);
        self.maxsimplex(dim, &mut simplex)?;
        let mut vertices: Vec<u32> = Vec::with_capacity(dim + 1);
        for &p in &simplex {
            let v = self.newvertex(p)?;
            setaddnth(&mut vertices, 0, v);
        }
        Ok(vertices)
    }

    fn createsimplex(&mut self, vertices: &[u32]) -> R<()> {
        let tail = self.newfacet();
        self.facet_list = tail;
        self.newfacet_list = tail;
        self.facet_tail = tail;
        self.num_facets = 0;
        self.num_vertices = 0;
        let vt = self.newvertex(NONE)?;
        self.vertex_list = vt;
        self.newvertex_list = vt;
        self.vertex_tail = vt;
        let mut toporient = true;
        let n = vertices.len();
        for (vi, &vertex) in vertices.iter().enumerate() {
            let nf = self.newfacet();
            self.fm(nf).vertices = setnew_delnthsorted(vertices, n, vi, 0);
            self.fm(nf).toporient = toporient;
            self.appendfacet(nf);
            self.fm(nf).newfacet = true;
            self.appendvertex(vertex);
            toporient ^= true;
        }
        let mut newfacet = self.newfacet_list;
        while self.live(newfacet) {
            let mut nb = Vec::with_capacity(self.dim);
            let mut facet = self.newfacet_list;
            while self.live(facet) {
                if facet != newfacet {
                    nb.push(facet);
                }
                facet = self.fc(facet).next;
            }
            nb.truncate(self.dim);
            self.fm(newfacet).neighbors = nb;
            newfacet = self.fc(newfacet).next;
        }
        Ok(())
    }

    fn initialhull(&mut self, vertices: &[u32]) -> R<()> {
        self.createsimplex(vertices)?;
        self.resetlists();
        self.facet_next = self.facet_list;
        self.interior_point = self.getcenter(vertices)?;
        let firstfacet = self.facet_list;
        self.setfacetplane(firstfacet);
        let ip = self.interior_point;
        let dist = self.distplane(&ip, firstfacet);
        if dist > 0.0 {
            let mut facet = self.facet_list;
            while self.live(facet) {
                let t = self.fc(facet).toporient;
                self.fm(facet).toporient = !t;
                facet = self.fc(facet).next;
            }
        }
        let mut facet = self.facet_list;
        while self.live(facet) {
            self.setfacetplane(facet);
            facet = self.fc(facet).next;
        }
        let mut facet = self.facet_list;
        while self.live(facet) {
            if !self.checkflipped(facet, None, true) {
                self.fm(facet).flipped = false;
                let mut f2 = self.facet_list;
                while self.live(f2) {
                    let t = self.fc(f2).toporient;
                    self.fm(f2).toporient = !t;
                    self.orientoutside(f2);
                    f2 = self.fc(f2).next;
                }
                break;
            }
            facet = self.fc(facet).next;
        }
        let mut minangle = REALMAX;
        let mut facet = self.facet_list;
        while self.live(facet) {
            if !self.checkflipped(facet, None, false) {
                return Err(ERR_SINGULAR);
            }
            let mut ni = 0;
            loop {
                let neighbor = set_get(&self.fc(facet).neighbors, ni);
                if neighbor == NONE {
                    break;
                }
                ni += 1;
                let a = self.fc(facet).normal;
                let b = self.fc(neighbor).normal;
                let angle = self.getangle(&a, &b);
                if angle < minangle {
                    minangle = angle;
                }
            }
            facet = self.fc(facet).next;
        }
        if minangle < QH_MAXNARROW {
            self.NARROWhull = true;
        }
        self.checkpolygon_effects();
        self.checkconvex_datafault()?;
        Ok(())
    }

    fn checkpolygon_effects(&mut self) {
        let mut numfacets = 0u32;
        let mut facet = self.facet_list;
        while self.live(facet) {
            if !self.fc(facet).visible {
                numfacets += 1;
            }
            facet = self.fc(facet).next;
        }
        let mut v = self.vertex_list;
        while self.vlive(v) {
            self.vm(v).seen = false;
            self.vm(v).visitid = 0;
            v = self.vx(v).next;
        }
        let mut facet = self.facet_list;
        while self.live(facet) {
            if !self.fc(facet).visible {
                let vs = self.fc(facet).vertices.clone();
                for &v in &vs {
                    if v == NONE {
                        break;
                    }
                    let vv = self.vm(v);
                    vv.visitid = vv.visitid.wrapping_add(1);
                    if !vv.seen {
                        vv.seen = true;
                    }
                }
            }
            facet = self.fc(facet).next;
        }
        self.vertex_visit = self.vertex_visit.wrapping_add(numfacets);
        self.vertex_visit = self.vertex_visit.wrapping_add(1);
    }

    fn checkconvex_datafault(&mut self) -> R<()> {
        let mut waserror = false;
        let mut facet = self.facet_list;
        while self.live(facet) {
            if self.fc(facet).flipped {
                waserror = true;
                facet = self.fc(facet).next;
                continue;
            }
            let mut allsimplicial;
            if self.MERGING && (!self.ZEROcentrum || !self.fc(facet).simplicial) {
                allsimplicial = false;
            } else {
                allsimplicial = true;
                let mut ni = 0;
                loop {
                    let neighbor = set_get(&self.fc(facet).neighbors, ni);
                    if neighbor == NONE {
                        break;
                    }
                    let vertex = set_get(&self.fc(facet).vertices, ni);
                    ni += 1;
                    if !self.fc(neighbor).simplicial {
                        allsimplicial = false;
                        continue;
                    }
                    let dist = self.distplane_pt(self.vx(vertex).point, neighbor);
                    if dist > -self.DISTround {
                        return Err(ERR_SINGULAR);
                    }
                }
            }
            if !allsimplicial {
                if self.fc(facet).center.is_none() {
                    let c = self.getcentrum(facet)?;
                    self.fm(facet).center = Some(c);
                }
                let centrum = self.fc(facet).center.unwrap();
                let mut ni = 0;
                loop {
                    let neighbor = set_get(&self.fc(facet).neighbors, ni);
                    if neighbor == NONE {
                        break;
                    }
                    ni += 1;
                    if self.ZEROcentrum && self.fc(facet).simplicial && self.fc(neighbor).simplicial {
                        continue;
                    }
                    let dist = self.distplane(&centrum, neighbor);
                    if dist > self.DISTround || dist >= 0.0 {
                        waserror = true;
                    }
                }
            }
            facet = self.fc(facet).next;
        }
        if waserror {
            return Err(ERR_PREC);
        }
        Ok(())
    }

    fn partitionall(&mut self, vertices: &[u32]) -> R<()> {
        let numpoints = self.num_points;
        let mut pointset: Vec<u32> = (0..numpoints as u32).collect();
        self.num_outside = 0;
        for &v in vertices {
            if v == NONE {
                break;
            }
            let id = self.vx(v).point;
            if id != NONE {
                pointset[id as usize] = NONE;
            }
        }
        if !self.BESToutside {
            let distoutside =
                if self.MERGING { fmax_(4.0 * self.MINoutside, 2.0 * self.max_outside) } else { self.MINoutside };
            let mut facet = self.facet_list;
            while self.live(facet) {
                let mut outside: Vec<u32> = Vec::new();
                let mut bestpoint = NONE;
                let mut bestdist = -REALMAX;
                let mut point_end = 0usize;
                let n = pointset.len();
                for i in 0..n {
                    let point = pointset[i];
                    if point != NONE {
                        let dist = self.distplane_pt(point, facet);
                        if dist < distoutside {
                            pointset[point_end] = point;
                            point_end += 1;
                        } else {
                            self.num_outside += 1;
                            if bestpoint == NONE {
                                bestpoint = point;
                                bestdist = dist;
                            } else if dist > bestdist {
                                outside.push(bestpoint);
                                bestpoint = point;
                                bestdist = dist;
                            } else {
                                outside.push(point);
                            }
                        }
                    }
                }
                if bestpoint != NONE {
                    outside.push(bestpoint);
                    self.fm(facet).furthestdist = bestdist;
                    self.fm(facet).outsideset = outside;
                } else {
                    self.fm(facet).outsideset = Vec::new();
                }
                pointset.truncate(point_end);
                facet = self.fc(facet).next;
            }
        }
        if self.BESToutside || self.MERGING || self.KEEPcoplanar || self.KEEPinside {
            self.findbestnew = true;
            let n = pointset.len();
            for i in 0..n {
                let point = pointset[i];
                if point != NONE {
                    let fl = self.facet_list;
                    self.partitionpoint(point, fl)?;
                }
            }
            self.findbestnew = false;
        }
        Ok(())
    }

    fn resetlists(&mut self) {
        let mut v = self.newvertex_list;
        while self.vlive(v) {
            self.vm(v).newlist = false;
            v = self.vx(v).next;
        }
        self.newvertex_list = NONE;
        let mut f = self.newfacet_list;
        while self.live(f) {
            self.fm(f).newfacet = false;
            f = self.fc(f).next;
        }
        self.newfacet_list = NONE;
        let mut f = self.visible_list;
        while f != NONE && self.fc(f).visible {
            let fm = self.fm(f);
            fm.f = NONE;
            fm.visible = false;
            f = self.fc(f).next;
        }
        self.visible_list = NONE;
        self.num_visible = 0;
        self.NEWfacets = false;
    }

    fn furthestnext(&mut self) {
        let mut bestfacet = NONE;
        let mut bestdist = -REALMAX;
        let mut facet = self.facet_list;
        while self.live(facet) {
            if !self.fc(facet).outsideset.is_empty() {
                let dist = self.fc(facet).furthestdist;
                if dist > bestdist {
                    bestfacet = facet;
                    bestdist = dist;
                }
            }
            facet = self.fc(facet).next;
        }
        if bestfacet != NONE {
            self.removefacet(bestfacet);
            self.prependfacet(bestfacet, ListHead::FacetNext);
        }
    }

    fn nextfurthest(&mut self) -> Option<(u32, u32)> {
        loop {
            let facet = self.facet_next;
            if facet == self.facet_tail {
                return None;
            }
            if self.fc(facet).outsideset.is_empty() {
                self.facet_next = self.fc(facet).next;
                continue;
            }
            if self.NARROWhull {
                if self.fc(facet).notfurthest {
                    self.furthestout(facet);
                }
                let dist = self.fc(facet).furthestdist;
                if dist < self.MINoutside {
                    self.facet_next = self.fc(facet).next;
                    continue;
                }
            }
            let p = setdellast(&mut self.fm(facet).outsideset);
            return Some((p, facet));
        }
    }

    fn furthestout(&mut self, facet: u32) {
        let mut bestdist = -REALMAX;
        let mut found = false;
        let os = self.fc(facet).outsideset.clone();
        for &p in &os {
            if p == NONE {
                break;
            }
            let dist = self.distplane_pt(p, facet);
            if dist > bestdist {
                found = true;
                bestdist = dist;
            }
        }
        if found {
            self.fm(facet).furthestdist = bestdist;
        }
        self.fm(facet).notfurthest = false;
    }

    fn buildhull(&mut self) -> R<()> {
        self.facet_next = self.facet_list;
        let mut furthest;
        loop {
            match self.nextfurthest() {
                Some((p, facet)) => {
                    furthest = p;
                    self.num_outside -= 1;
                    if !self.addpoint(p, facet)? {
                        break;
                    }
                }
                None => {
                    furthest = NONE;
                    break;
                }
            }
        }
        if self.NARROWhull {
            self.outcoplanar()?;
        }
        if self.num_outside != 0 && furthest == NONE {
            return Err(ERR_QHULL);
        }
        Ok(())
    }

    fn outcoplanar(&mut self) -> R<()> {
        let mut facet = self.facet_list;
        while self.live(facet) {
            let os = self.fc(facet).outsideset.clone();
            for &point in &os {
                if point == NONE {
                    break;
                }
                self.num_outside -= 1;
                if self.KEEPcoplanar || self.KEEPnearinside {
                    let mut dist = self.distplane_pt(point, facet);
                    self.partitioncoplanar(point, facet, Some(&mut dist))?;
                }
            }
            self.fm(facet).outsideset = Vec::new();
            facet = self.fc(facet).next;
        }
        Ok(())
    }

    fn addpoint(&mut self, furthest: u32, facet: u32) -> R<bool> {
        self.maxoutdone = false;
        if facet == NONE {
            return Err(ERR_QHULL);
        }
        let fp = self.pt(furthest);
        self.findhorizon(&fp, facet)?;
        let vertex = self.makenewfacets(furthest)?;
        self.makenewplanes();
        self.matchnewfacets()?;
        self.updatevertices();
        if self.PREmerge || self.MERGEexact {
            self.premerge(vertex)?;
            if self.ztotmerge > QH_USEFINDBESTNEW {
                self.findbestnew = true;
            } else {
                let mut nf = self.newfacet_list;
                while self.live(nf) {
                    if !self.fc(nf).simplicial {
                        self.findbestnew = true;
                        break;
                    }
                    nf = self.fc(nf).next;
                }
            }
        } else if self.BESToutside {
            self.findbestnew = true;
        }
        self.partitionvisible(false)?;
        self.findbestnew = false;
        self.findbest_notsharp = false;
        self.deletevisible()?;
        self.NEWfacets = false;
        self.resetlists();
        Ok(true)
    }

    fn findhorizon(&mut self, point: &[f64; 3], facet: u32) -> R<()> {
        let mut numhorizon = 0;
        self.removefacet(facet);
        self.appendfacet(facet);
        self.num_visible = 1;
        self.visible_list = facet;
        self.fm(facet).visible = true;
        self.fm(facet).f = NONE;
        self.visit_id = self.visit_id.wrapping_add(1);
        let mut visible = self.visible_list;
        while visible != NONE && self.fc(visible).visible {
            let vid = self.visit_id;
            self.fm(visible).visitid = vid;
            let mut ni = 0;
            loop {
                let neighbor = set_get(&self.fc(visible).neighbors, ni);
                if neighbor == NONE {
                    break;
                }
                ni += 1;
                if self.fc(neighbor).visitid == vid {
                    continue;
                }
                self.fm(neighbor).visitid = vid;
                let dist = self.distplane(point, neighbor);
                if dist > self.MINvisible {
                    self.removefacet(neighbor);
                    self.appendfacet(neighbor);
                    self.fm(neighbor).visible = true;
                    self.fm(neighbor).f = NONE;
                    self.num_visible += 1;
                } else {
                    if dist > -self.MAXcoplanar {
                        self.fm(neighbor).coplanar = true;
                        if self.MERGING {
                            if dist > 0.0 {
                                if self.max_outside < dist {
                                    self.max_outside = dist;
                                }
                                if self.max_vertex < dist {
                                    self.max_vertex = dist;
                                }
                                if self.fc(neighbor).maxoutside < dist {
                                    self.fm(neighbor).maxoutside = dist;
                                }
                            } else if self.min_vertex > dist {
                                self.min_vertex = dist;
                            }
                        }
                    } else {
                        self.fm(neighbor).coplanar = false;
                    }
                    numhorizon += 1;
                }
            }
            visible = self.fc(visible).next;
        }
        if numhorizon == 0 {
            return Err(ERR_PREC);
        }
        Ok(())
    }

    fn makenewfacets(&mut self, point: u32) -> R<u32> {
        self.newfacet_list = self.facet_tail;
        self.newvertex_list = self.vertex_tail;
        let apex = self.newvertex(point)?;
        self.appendvertex(apex);
        self.visit_id = self.visit_id.wrapping_add(1);
        self.NEWfacets = true;
        let mut newfacet = NONE;
        let mut newfacet2 = NONE;
        let mut visible = self.visible_list;
        while visible != NONE && self.fc(visible).visible {
            let nb = self.fc(visible).neighbors.clone();
            for &n in &nb {
                if n == NONE {
                    break;
                }
                self.fm(n).seen = false;
            }
            if self.fc(visible).ridges_alloc {
                let vid = self.visit_id;
                self.fm(visible).visitid = vid;
                newfacet2 = self.makenew_nonsimplicial(visible, apex)?;
            }
            if self.fc(visible).simplicial {
                newfacet = self.makenew_simplicial(visible, apex)?;
            }
            if newfacet2 != NONE {
                newfacet = newfacet2;
            }
            if newfacet != NONE {
                self.fm(visible).f = newfacet;
            } else if !self.fc(visible).neighbors.is_empty() {
                self.fm(visible).neighbors[0] = NONE;
            }
            visible = self.fc(visible).next;
        }
        Ok(apex)
    }

    fn makenewfacet(&mut self, vertices: Vec<u32>, toporient: bool, horizon: u32) -> u32 {
        for &v in &vertices {
            if v == NONE {
                break;
            }
            if !self.vx(v).newlist {
                self.removevertex(v);
                self.appendvertex(v);
            }
        }
        let nf = self.newfacet();
        self.fm(nf).vertices = vertices;
        self.fm(nf).toporient = toporient;
        setappend(&mut self.fm(nf).neighbors, horizon);
        self.appendfacet(nf);
        nf
    }

    fn makenew_nonsimplicial(&mut self, visible: u32, apex: u32) -> R<u32> {
        let mut newfacet = NONE;
        let mut ri = 0;
        loop {
            let ridge = set_get(&self.fc(visible).ridges, ri);
            if ridge == NONE {
                break;
            }
            ri += 1;
            let neighbor = self.otherfacet(ridge, visible);
            if self.fc(neighbor).visible {
            } else {
                let toporient = self.rg(ridge).top == visible;
                let mut vertices = Vec::with_capacity(self.dim);
                vertices.push(apex);
                let rv = self.rg(ridge).vertices.clone();
                for &v in &rv {
                    if v == NONE {
                        break;
                    }
                    vertices.push(v);
                }
                newfacet = self.makenewfacet(vertices, toporient, neighbor);
                if self.fc(neighbor).coplanar {
                    self.fm(newfacet).mergehorizon = true;
                    if !self.fc(neighbor).seen {
                        self.fm(newfacet).f = newfacet;
                        self.fm(neighbor).f = newfacet;
                    } else {
                        let samecycle = self.fc(neighbor).f;
                        let sc2 = self.fc(samecycle).f;
                        self.fm(newfacet).f = sc2;
                        self.fm(samecycle).f = newfacet;
                    }
                }
                if self.fc(neighbor).seen {
                    if self.fc(neighbor).simplicial {
                        return Err(ERR_QHULL);
                    }
                    setappend(&mut self.fm(neighbor).neighbors, newfacet);
                } else {
                    setreplace(&mut self.fm(neighbor).neighbors, visible, newfacet);
                }
                if self.fc(neighbor).simplicial {
                    setdel(&mut self.fm(neighbor).ridges, ridge);
                    self.rm(ridge).vertices = Vec::new();
                } else {
                    self.ridge_append(newfacet, ridge);
                    if toporient {
                        self.rm(ridge).top = newfacet;
                    } else {
                        self.rm(ridge).bottom = newfacet;
                    }
                }
            }
            self.fm(neighbor).seen = true;
        }
        if !self.fc(visible).ridges.is_empty() {
            self.fm(visible).ridges[0] = NONE;
        }
        Ok(newfacet)
    }

    fn makenew_simplicial(&mut self, visible: u32, apex: u32) -> R<u32> {
        let mut newfacet = NONE;
        let mut ni = 0;
        loop {
            let neighbor = set_get(&self.fc(visible).neighbors, ni);
            if neighbor == NONE {
                break;
            }
            ni += 1;
            if !self.fc(neighbor).seen && !self.fc(neighbor).visible {
                let (mut vertices, horizonskip, _visibleskip) = self.facetintersect(neighbor, visible, 1)?;
                vertices[0] = apex;
                let toporient =
                    if self.fc(neighbor).toporient { horizonskip & 1 == 1 } else { (horizonskip & 1) ^ 1 == 1 };
                newfacet = self.makenewfacet(vertices, toporient, neighbor);
                if self.fc(neighbor).coplanar && (self.PREmerge || self.MERGEexact) {
                    self.fm(newfacet).f = newfacet;
                    self.fm(newfacet).mergehorizon = true;
                }
                self.fm(neighbor).neighbors[horizonskip] = newfacet;
            }
        }
        Ok(newfacet)
    }

    fn facetintersect(&self, a: u32, b: u32, prepend: usize) -> R<(Vec<u32>, usize, usize)> {
        let na = &self.fc(a).neighbors;
        let nb = &self.fc(b).neighbors;
        let dim = self.dim;
        let mut skip_a = usize::MAX;
        let mut skip_b = usize::MAX;
        for i in 0..dim {
            if set_get(na, i) == b {
                skip_a = i;
                break;
            }
        }
        for j in 0..dim {
            if set_get(nb, j) == a {
                skip_b = j;
                break;
            }
        }
        if skip_a == usize::MAX || skip_b == usize::MAX {
            return Err(ERR_QHULL);
        }
        let inter = setnew_delnthsorted(&self.fc(a).vertices, dim, skip_a, prepend);
        Ok((inter, skip_a, skip_b))
    }

    fn makenewplanes(&mut self) {
        let mut nf = self.newfacet_list;
        while self.live(nf) {
            if !self.fc(nf).mergehorizon {
                self.setfacetplane(nf);
            }
            nf = self.fc(nf).next;
        }
        if self.JOGGLEmax < REALMAX / 2.0 {
            let t = -self.wnewvertexmax;
            if self.min_vertex > t {
                self.min_vertex = t;
            }
        }
    }

    fn gethash(&self, hashsize: usize, set: &[u32], size: usize, firstindex: usize, skipelem: u32) -> usize {
        let key = |v: u32| -> u64 { if v == NONE { 0 } else { self.vx(v).id as u64 + 1 } };
        let mut hash: u64 = 0;
        for i in firstindex..size {
            hash = hash.wrapping_add(key(set_get(set, i)));
        }
        hash = hash.wrapping_sub(key(skipelem));
        (hash % hashsize as u64) as usize
    }

    fn newhashtable(&mut self, newsize: usize) -> usize {
        let mut size = ((newsize as i32 + 1) * QH_HASHFACTOR) | 1;
        loop {
            if (size % 3) != 0 && (size % 5) != 0 {
                break;
            }
            size += 2;
        }
        self.hash_table = vec![NONE; size as usize];
        size as usize
    }

    fn addhash(&mut self, newelem: u32, hashsize: usize, hash: usize) {
        let mut scan = hash;
        loop {
            let elem = self.hash_table[scan];
            if elem == NONE {
                self.hash_table[scan] = newelem;
                return;
            }
            if elem == newelem {
                return;
            }
            scan += 1;
            if scan >= hashsize {
                scan = 0;
            }
        }
    }

    fn matchvertices(&self, firstindex: usize, va: &[u32], skip_a: usize, vb: &[u32]) -> Option<(usize, bool)> {
        let mut ia = firstindex;
        let mut ib = firstindex;
        let mut skip_bp: Option<usize> = None;
        loop {
            if ia != skip_a {
                loop {
                    let eb = set_get(vb, ib);
                    ib += 1;
                    if set_get(va, ia) == eb {
                        break;
                    }
                    if skip_bp.is_some() {
                        return None;
                    }
                    skip_bp = Some(ib);
                }
            }
            ia += 1;
            if set_get(va, ia) == NONE {
                break;
            }
        }
        let sbp = match skip_bp {
            Some(s) => s,
            None => {
                ib += 1;
                ib
            }
        };
        let skip_b = sbp - 1;
        let same = (skip_a & 1) == (skip_b & 1);
        Some((skip_b, same))
    }

    fn matchneighbor(&mut self, newfacet: u32, newskip: usize, hashsize: usize, hashcount: &mut i32) -> R<()> {
        let mut newfound = false;
        let skipv = self.fc(newfacet).vertices[newskip];
        let hash = self.gethash(hashsize, &self.fc(newfacet).vertices, self.dim, 1, skipv);
        let mut scan = hash;
        loop {
            let facet = self.hash_table[scan];
            if facet == NONE {
                break;
            }
            if facet == newfacet {
                newfound = true;
            } else {
                let m = self.matchvertices(1, &self.fc(newfacet).vertices, newskip, &self.fc(facet).vertices);
                if let Some((skip, same)) = m {
                    if self.fc(newfacet).vertices[newskip] == self.fc(facet).vertices[skip] {
                        return Err(ERR_PREC);
                    }
                    let ismatch = same == (self.fc(newfacet).toporient ^ self.fc(facet).toporient);
                    let matchfacet = set_get(&self.fc(facet).neighbors, skip);
                    if ismatch && matchfacet == NONE {
                        self.fm(facet).neighbors[skip] = newfacet;
                        self.fm(newfacet).neighbors[newskip] = facet;
                        *hashcount -= 1;
                        return Ok(());
                    }
                    if !self.PREmerge && !self.MERGEexact {
                        return Err(ERR_PREC);
                    }
                    self.fm(newfacet).neighbors[newskip] = DUPRIDGE;
                    self.fm(newfacet).dupridge = true;
                    if !self.fc(newfacet).has_normal {
                        self.setfacetplane(newfacet);
                    }
                    self.addhash(newfacet, hashsize, hash);
                    *hashcount += 1;
                    if !self.fc(facet).has_normal {
                        self.setfacetplane(facet);
                    }
                    if matchfacet != DUPRIDGE {
                        self.fm(facet).neighbors[skip] = DUPRIDGE;
                        self.fm(facet).dupridge = true;
                        if !self.fc(facet).has_normal {
                            self.setfacetplane(facet);
                        }
                        if matchfacet != NONE {
                            let matchskip = setindex(&self.fc(matchfacet).neighbors, facet);
                            if matchskip >= 0 {
                                self.fm(matchfacet).neighbors[matchskip as usize] = DUPRIDGE;
                            }
                            self.fm(matchfacet).dupridge = true;
                            if !self.fc(matchfacet).has_normal {
                                self.setfacetplane(matchfacet);
                            }
                            self.addhash(matchfacet, hashsize, hash);
                            *hashcount += 2;
                        }
                    }
                    return Ok(());
                }
            }
            scan += 1;
            if scan >= hashsize {
                scan = 0;
            }
        }
        if !newfound {
            self.hash_table[scan] = newfacet;
        }
        *hashcount += 1;
        Ok(())
    }

    fn matchduplicates(&mut self, atfacet: u32, atskip: usize, hashsize: usize, hashcount: &mut i32) -> R<()> {
        let mut maxmatch = NONE;
        let mut maxmatch2 = NONE;
        let mut maxskip = 0usize;
        let mut maxskip2 = 0usize;
        let mut maxdist = -REALMAX;
        let skipv = self.fc(atfacet).vertices[atskip];
        let hash = self.gethash(hashsize, &self.fc(atfacet).vertices, self.dim, 1, skipv);
        for makematch in 0..2 {
            self.visit_id = self.visit_id.wrapping_add(1);
            let mut newfacet = atfacet;
            let mut newskip = atskip;
            let mut nextskip = 0usize;
            while newfacet != NONE {
                let mut nextfacet = NONE;
                let vid = self.visit_id;
                self.fm(newfacet).visitid = vid;
                let mut scan = hash;
                loop {
                    let facet = self.hash_table[scan];
                    if facet == NONE {
                        break;
                    }
                    if !(!self.fc(facet).dupridge || self.fc(facet).visitid == self.visit_id) {
                        let m = self.matchvertices(1, &self.fc(newfacet).vertices, newskip, &self.fc(facet).vertices);
                        if let Some((skip, same)) = m {
                            let ismatch = same == (self.fc(newfacet).toporient ^ self.fc(facet).toporient);
                            if set_get(&self.fc(facet).neighbors, skip) != DUPRIDGE {
                                if makematch == 0 {
                                    return Err(ERR_QHULL);
                                }
                            } else if ismatch && makematch != 0 {
                                if set_get(&self.fc(newfacet).neighbors, newskip) == DUPRIDGE {
                                    self.fm(facet).neighbors[skip] = newfacet;
                                    self.fm(newfacet).neighbors[newskip] = MERGERIDGE;
                                    *hashcount -= 2;
                                }
                            } else if ismatch {
                                let (mut mindist, _, _) = self.getdistance(facet, newfacet);
                                let (dist2, _, _) = self.getdistance(newfacet, facet);
                                if mindist > dist2 {
                                    mindist = dist2;
                                }
                                if mindist > maxdist {
                                    maxdist = mindist;
                                    maxmatch = facet;
                                    maxskip = skip;
                                    maxmatch2 = newfacet;
                                    maxskip2 = newskip;
                                }
                            } else {
                                nextfacet = facet;
                                nextskip = skip;
                            }
                        }
                    }
                    scan += 1;
                    if scan >= hashsize {
                        scan = 0;
                    }
                }
                newfacet = nextfacet;
                newskip = nextskip;
            }
            if makematch == 0 {
                if maxmatch == NONE {
                    return Err(ERR_QHULL);
                }
                self.fm(maxmatch).neighbors[maxskip] = maxmatch2;
                self.fm(maxmatch2).neighbors[maxskip2] = maxmatch;
                *hashcount -= 2;
            }
        }
        Ok(())
    }

    fn matchnewfacets(&mut self) -> R<()> {
        let dim = self.dim;
        let mut numnew = 0usize;
        let mut hashcount: i32 = 0;
        let mut nf = self.newfacet_list;
        while self.live(nf) {
            numnew += 1;
            let f = self.fm(nf);
            f.neighbors.resize(dim, NONE);
            for k in 1..dim {
                f.neighbors[k] = NONE;
            }
            nf = self.fc(nf).next;
        }
        self.newhashtable(numnew * (dim - 1));
        let hashsize = self.hash_table.len();
        let mut nf = self.newfacet_list;
        while self.live(nf) {
            for newskip in 1..dim {
                self.matchneighbor(nf, newskip, hashsize, &mut hashcount)?;
            }
            nf = self.fc(nf).next;
        }
        if hashcount != 0 {
            let mut nf = self.newfacet_list;
            while self.live(nf) {
                if self.fc(nf).dupridge {
                    let n = self.fc(nf).neighbors.len();
                    for ni in 0..n {
                        let neighbor = self.fc(nf).neighbors[ni];
                        if neighbor == DUPRIDGE {
                            self.matchduplicates(nf, ni, hashsize, &mut hashcount)?;
                        }
                    }
                }
                nf = self.fc(nf).next;
            }
        }
        if hashcount != 0 {
            return Err(ERR_QHULL);
        }
        self.hash_table = Vec::new();
        if self.PREmerge || self.MERGEexact {
            let mut nf = self.newfacet_list;
            while self.live(nf) {
                if self.fc(nf).has_normal {
                    self.checkflipped(nf, None, true);
                }
                nf = self.fc(nf).next;
            }
        }
        Ok(())
    }

    fn updatevertices(&mut self) {
        if self.VERTEXneighbors {
            let mut v = self.newvertex_list;
            while self.vlive(v) {
                let nb = std::mem::take(&mut self.vm(v).neighbors);
                let mut nb2 = nb;
                let n = nb2.len();
                let mut i = 0;
                while i < n {
                    let x = nb2[i];
                    if x == NONE {
                        break;
                    }
                    if self.fc(x).visible {
                        nb2[i] = NONE;
                    }
                    i += 1;
                }
                setcompact(&mut nb2);
                self.vm(v).neighbors = nb2;
                v = self.vx(v).next;
            }
            let mut nf = self.newfacet_list;
            while self.live(nf) {
                let vs = self.fc(nf).vertices.clone();
                for &v in &vs {
                    if v == NONE {
                        break;
                    }
                    setappend(&mut self.vm(v).neighbors, nf);
                }
                nf = self.fc(nf).next;
            }
            let mut visible = self.visible_list;
            while visible != NONE && self.fc(visible).visible {
                let vs = self.fc(visible).vertices.clone();
                for &v in &vs {
                    if v == NONE {
                        break;
                    }
                    if !self.vx(v).newlist && !self.vx(v).deleted {
                        let mut found_nonvisible = false;
                        for &n in &self.vx(v).neighbors {
                            if n == NONE {
                                break;
                            }
                            if !self.fc(n).visible {
                                found_nonvisible = true;
                                break;
                            }
                        }
                        if found_nonvisible {
                            setdel(&mut self.vm(v).neighbors, visible);
                        } else {
                            self.vm(v).deleted = true;
                            setappend(&mut self.del_vertices, v);
                        }
                    }
                }
                visible = self.fc(visible).next;
            }
        } else {
            let mut visible = self.visible_list;
            while visible != NONE && self.fc(visible).visible {
                let vs = self.fc(visible).vertices.clone();
                for &v in &vs {
                    if v == NONE {
                        break;
                    }
                    if !self.vx(v).newlist && !self.vx(v).deleted {
                        self.vm(v).deleted = true;
                        setappend(&mut self.del_vertices, v);
                    }
                }
                visible = self.fc(visible).next;
            }
        }
    }

    fn partitionvisible(&mut self, allpoints: bool) -> R<i32> {
        let mut numoutside = 0;
        let mut visible = self.visible_list;
        while visible != NONE && self.fc(visible).visible {
            if self.fc(visible).outsideset.is_empty() && self.fc(visible).coplanarset.is_empty() {
                visible = self.fc(visible).next;
                continue;
            }
            let mut newfacet = self.fc(visible).f;
            let mut count: u32 = 0;
            while newfacet != NONE && self.fc(newfacet).visible {
                newfacet = self.fc(newfacet).f;
                if count > self.facet_id {
                    return Err(ERR_QHULL);
                }
                count += 1;
            }
            if newfacet == NONE {
                newfacet = self.newfacet_list;
            }
            if !self.fc(visible).outsideset.is_empty() {
                let os = self.fc(visible).outsideset.clone();
                let size = os.len() as i32;
                numoutside += size;
                self.num_outside -= size;
                for &p in &os {
                    if p == NONE {
                        break;
                    }
                    self.partitionpoint(p, newfacet)?;
                }
            }
            if !self.fc(visible).coplanarset.is_empty() && (self.KEEPcoplanar || self.KEEPinside || self.KEEPnearinside)
            {
                let cs = self.fc(visible).coplanarset.clone();
                for &p in &cs {
                    if p == NONE {
                        break;
                    }
                    if allpoints {
                        self.partitionpoint(p, newfacet)?;
                    } else {
                        self.partitioncoplanar(p, newfacet, None)?;
                    }
                }
            }
            visible = self.fc(visible).next;
        }
        let dv = self.del_vertices.clone();
        for &v in &dv {
            if v == NONE {
                break;
            }
            let p = self.vx(v).point;
            if p != NONE {
                let nfl = self.newfacet_list;
                if allpoints {
                    self.partitionpoint(p, nfl)?;
                } else {
                    self.partitioncoplanar(p, nfl, None)?;
                }
            }
        }
        Ok(numoutside)
    }

    fn partitionpoint(&mut self, point: u32, facet: u32) -> R<()> {
        let p = self.pt(point);
        let mut isoutside = false;
        let mut numpart = 0;
        let (bestfacet, bestdist) = if self.findbestnew {
            self.findbestnew(&p, facet, Some(&mut isoutside), &mut numpart)?
        } else {
            let bo = self.BESToutside;
            self.findbest(&p, facet, bo, true, false, Some(&mut isoutside), &mut numpart)?
        };
        if self.NARROWhull {
            if self.KEEPnearinside {
                if bestdist >= -self.NEARinside {
                    isoutside = true;
                }
            } else if bestdist >= -self.MAXcoplanar {
                isoutside = true;
            }
        }
        if isoutside {
            if self.fc(bestfacet).outsideset.is_empty() || setlast(&self.fc(bestfacet).outsideset) == NONE {
                setappend(&mut self.fm(bestfacet).outsideset, point);
                if !self.fc(bestfacet).newfacet {
                    self.removefacet(bestfacet);
                    self.appendfacet(bestfacet);
                }
                self.fm(bestfacet).furthestdist = bestdist;
            } else if self.fc(bestfacet).furthestdist < bestdist {
                setappend(&mut self.fm(bestfacet).outsideset, point);
                self.fm(bestfacet).furthestdist = bestdist;
            } else {
                setappend2ndlast(&mut self.fm(bestfacet).outsideset, point);
            }
            self.num_outside += 1;
        } else if bestdist >= -self.MAXcoplanar {
            if self.KEEPcoplanar || self.KEEPnearinside || bestdist > self.max_outside {
                let mut d = bestdist;
                self.partitioncoplanar(point, bestfacet, Some(&mut d))?;
            }
        } else if self.KEEPnearinside && bestdist > -self.NEARinside {
            let mut d = bestdist;
            self.partitioncoplanar(point, bestfacet, Some(&mut d))?;
        } else if self.KEEPinside {
            let mut d = bestdist;
            self.partitioncoplanar(point, bestfacet, Some(&mut d))?;
        }
        Ok(())
    }

    fn partitioncoplanar(&mut self, point: u32, facet: u32, dist: Option<&mut f64>) -> R<()> {
        self.WAScoplanar = true;
        let p = self.pt(point);
        let bestfacet;
        let bestdist;
        match dist {
            None => {
                let mut numpart = 0;
                let (bf, bd) = if self.findbestnew {
                    self.findbestnew(&p, facet, None, &mut numpart)?
                } else {
                    let mut iso = false;
                    self.findbest(&p, facet, true, false, false, Some(&mut iso), &mut numpart)?
                };
                bestfacet = bf;
                bestdist = bd;
                if !self.KEEPinside {
                    if self.KEEPnearinside {
                        if bestdist < -self.NEARinside {
                            return Ok(());
                        }
                    } else if bestdist < -self.MAXcoplanar {
                        return Ok(());
                    }
                }
            }
            Some(d) => {
                bestfacet = facet;
                bestdist = *d;
            }
        }
        if self.KEEPcoplanar || self.KEEPinside || self.KEEPnearinside {
            let oldfurthest = setlast(&self.fc(bestfacet).coplanarset);
            let mut dist2 = 0.0;
            if oldfurthest != NONE {
                dist2 = self.distplane_pt(oldfurthest, bestfacet);
            }
            if oldfurthest == NONE || dist2 < bestdist {
                setappend(&mut self.fm(bestfacet).coplanarset, point);
                if bestdist > self.max_outside {
                    self.max_outside = bestdist;
                }
            } else {
                setappend2ndlast(&mut self.fm(bestfacet).coplanarset, point);
            }
        } else if bestdist > self.max_outside {
            self.max_outside = bestdist;
        }
        Ok(())
    }

    fn deletevisible(&mut self) -> R<()> {
        let mut numvisible = 0;
        let mut visible = self.visible_list;
        while visible != NONE && self.fc(visible).visible {
            let nextfacet = self.fc(visible).next;
            numvisible += 1;
            self.delfacet(visible);
            visible = nextfacet;
        }
        if numvisible != self.num_visible {
            return Err(ERR_QHULL);
        }
        self.num_visible = 0;
        let dv = std::mem::take(&mut self.del_vertices);
        for &v in &dv {
            if v == NONE {
                break;
            }
            self.delvertex(v);
        }
        Ok(())
    }

    fn vertexneighbors(&mut self) {
        if self.VERTEXneighbors {
            return;
        }
        self.vertex_visit = self.vertex_visit.wrapping_add(1);
        let vv = self.vertex_visit;
        let mut facet = self.facet_list;
        while self.live(facet) {
            if !self.fc(facet).visible {
                let vs = self.fc(facet).vertices.clone();
                for &v in &vs {
                    if v == NONE {
                        break;
                    }
                    if self.vx(v).visitid != vv {
                        self.vm(v).visitid = vv;
                        self.vm(v).neighbors = Vec::with_capacity(4);
                    }
                    setappend(&mut self.vm(v).neighbors, facet);
                }
            }
            facet = self.fc(facet).next;
        }
        self.VERTEXneighbors = true;
    }

    fn vertexintersect_new(&self, a: &[u32], b: &[u32]) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.dim - 1);
        let (mut i, mut j) = (0usize, 0usize);
        loop {
            let va = set_get(a, i);
            let vb = set_get(b, j);
            if va == NONE || vb == NONE {
                break;
            }
            if va == vb {
                out.push(va);
                i += 1;
                j += 1;
            } else if self.vx(va).id > self.vx(vb).id {
                i += 1;
            } else {
                j += 1;
            }
        }
        out
    }

    fn nextridge3d(&self, atridge: u32, facet: u32) -> (u32, u32) {
        let at = self.rg(atridge);
        let atvertex = if at.top == facet { set_get(&at.vertices, 1) } else { set_get(&at.vertices, 0) };
        for &ridge in &self.fc(facet).ridges {
            if ridge == NONE {
                break;
            }
            if ridge == atridge {
                continue;
            }
            let r = self.rg(ridge);
            let (vertex, othervertex) = if r.top == facet {
                (set_get(&r.vertices, 0), set_get(&r.vertices, 1))
            } else {
                (set_get(&r.vertices, 1), set_get(&r.vertices, 0))
            };
            if vertex == atvertex {
                return (ridge, othervertex);
            }
        }
        (NONE, NONE)
    }

    fn facet3vertex(&self, facet: u32) -> R<Vec<u32>> {
        let f = self.fc(facet);
        let cntvertices = f.vertices.len();
        let mut vertices = Vec::with_capacity(cntvertices);
        if f.simplicial {
            if cntvertices != 3 {
                return Err(ERR_QHULL);
            }
            vertices.push(f.vertices[0]);
            if f.toporient {
                vertices.push(f.vertices[1]);
            } else {
                vertices.insert(0, f.vertices[1]);
            }
            vertices.push(f.vertices[2]);
        } else {
            let firstridge = set_get(&f.ridges, 0);
            let mut ridge = firstridge;
            let mut cntprojected = 0usize;
            loop {
                if ridge == NONE {
                    break;
                }
                let (r, vertex) = self.nextridge3d(ridge, facet);
                ridge = r;
                if ridge == NONE {
                    break;
                }
                vertices.push(vertex);
                cntprojected += 1;
                if cntprojected > cntvertices || ridge == firstridge {
                    break;
                }
            }
            if ridge == NONE || cntprojected != cntvertices {
                return Err(ERR_QHULL);
            }
        }
        Ok(vertices)
    }
}

impl Qh {
    fn newmerge(&mut self, m: Merge) -> u32 {
        self.merges.push(m);
        (self.merges.len() - 1) as u32
    }

    fn samecycle_list(&self, cycle: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut same = self.fc(cycle).f;
        while same != NONE {
            out.push(same);
            if same == cycle {
                break;
            }
            same = self.fc(same).f;
        }
        out
    }

    fn premerge(&mut self, apex: u32) -> R<()> {
        let mut othermerge = false;
        if self.ZEROcentrum && self.checkzero(false) {
            return Ok(());
        }
        let _ = apex;
        self.centrum_radius = self.premerge_centrum;
        self.cos_max = self.premerge_cos;
        self.degen_mergeset = Vec::new();
        self.facet_mergeset = Vec::new();
        if self.dim >= 3 {
            let nfl = self.newfacet_list;
            self.mark_dupridges(nfl);
            self.mergecycle_all(nfl, &mut othermerge)?;
            self.forcedmerges(&mut othermerge)?;
            let mut nf = self.newfacet_list;
            while self.live(nf) {
                if !self.fc(nf).simplicial && !self.fc(nf).mergeridge {
                    self.degen_redundant_neighbors(nf, NONE);
                }
                nf = self.fc(nf).next;
            }
            if self.merge_degenredundant()? != 0 {
                othermerge = true;
            }
        } else {
            let nfl = self.newfacet_list;
            self.mergecycle_all(nfl, &mut othermerge)?;
        }
        let nfl = self.newfacet_list;
        self.flippedmerges(nfl, &mut othermerge)?;
        if !self.MERGEexact || self.ztotmerge != 0 {
            self.POSTmerging = false;
            let nfl = self.newfacet_list;
            self.getmergeset_initial(nfl)?;
            self.all_merges(othermerge, false)?;
        }
        self.facet_mergeset = Vec::new();
        self.degen_mergeset = Vec::new();
        Ok(())
    }

    fn checkzero(&mut self, testall: bool) -> bool {
        let facetlist;
        if testall {
            facetlist = self.facet_list;
        } else {
            facetlist = self.newfacet_list;
            let mut facet = facetlist;
            while self.live(facet) {
                let horizon = set_get(&self.fc(facet).neighbors, 0);
                if !self.fc(horizon).simplicial {
                    self.ZEROall_ok = false;
                    return false;
                }
                if self.fc(facet).flipped || self.fc(facet).dupridge || !self.fc(facet).has_normal {
                    self.ZEROall_ok = false;
                    return false;
                }
                facet = self.fc(facet).next;
            }
            if self.MERGEexact && self.ZEROall_ok {
                return true;
            }
        }
        let mut facet = facetlist;
        while self.live(facet) {
            self.vertex_visit = self.vertex_visit.wrapping_add(1);
            let vv = self.vertex_visit;
            let mut neighbor_i = 0usize;
            let mut horizon = NONE;
            let mut ni = 0usize;
            loop {
                let neighbor = set_get(&self.fc(facet).neighbors, ni);
                if neighbor == NONE {
                    break;
                }
                ni += 1;
                if neighbor_i == 0 && !testall {
                    horizon = neighbor;
                    neighbor_i += 1;
                    continue;
                }
                let vertex = set_get(&self.fc(facet).vertices, neighbor_i);
                neighbor_i += 1;
                self.vm(vertex).visitid = vv;
                let dist = self.distplane_pt(self.vx(vertex).point, neighbor);
                if dist >= -self.DISTround {
                    self.ZEROall_ok = false;
                    if !self.MERGEexact || testall || dist > self.DISTround {
                        return false;
                    }
                }
            }
            if !testall {
                let hv = self.fc(horizon).vertices.clone();
                for &vertex in &hv {
                    if vertex == NONE {
                        break;
                    }
                    if self.vx(vertex).visitid != vv {
                        let dist = self.distplane_pt(self.vx(vertex).point, facet);
                        if dist >= -self.DISTround {
                            self.ZEROall_ok = false;
                            if !self.MERGEexact || dist > self.DISTround {
                                return false;
                            }
                        }
                        break;
                    }
                }
            }
            facet = self.fc(facet).next;
        }
        true
    }

    fn appendmergeset(&mut self, facet: u32, neighbor: u32, mergetype: u8, angle: Option<f64>) {
        if self.fc(facet).redundant {
            return;
        }
        if self.fc(facet).degenerate && mergetype == MRGDEGEN {
            return;
        }
        let a = match angle {
            Some(a) if self.ANGLEmerge => a,
            _ => 0.0,
        };
        let m = self.newmerge(Merge { angle: a, facet1: facet, facet2: neighbor, mtype: mergetype });
        if mergetype < MRGDEGEN {
            self.facet_mergeset.push(m);
        } else if mergetype == MRGDEGEN {
            self.fm(facet).degenerate = true;
            let lastmerge = setlast(&self.degen_mergeset);
            if lastmerge == NONE || self.merges[lastmerge as usize].mtype == MRGDEGEN {
                self.degen_mergeset.push(m);
            } else {
                self.degen_mergeset.insert(0, m);
            }
        } else {
            self.fm(facet).redundant = true;
            self.degen_mergeset.push(m);
        }
    }

    fn mark_dupridges(&mut self, facetlist: u32) {
        let mut nummerge = 0;
        let mut facet = facetlist;
        while self.live(facet) {
            if self.fc(facet).dupridge {
                let mut ni = 0usize;
                loop {
                    let neighbor = set_get(&self.fc(facet).neighbors, ni);
                    if neighbor == NONE {
                        break;
                    }
                    ni += 1;
                    if neighbor == MERGERIDGE {
                        self.fm(facet).mergeridge = true;
                        continue;
                    }
                    if neighbor == DUPRIDGE {
                        continue;
                    }
                    if self.fc(neighbor).dupridge && !setin(&self.fc(neighbor).neighbors, facet) {
                        self.appendmergeset(facet, neighbor, MRGRIDGE, None);
                        self.fm(facet).mergeridge2 = true;
                        self.fm(facet).mergeridge = true;
                        nummerge += 1;
                    }
                }
            }
            facet = self.fc(facet).next;
        }
        if nummerge == 0 {
            return;
        }
        let mut facet = facetlist;
        while self.live(facet) {
            if self.fc(facet).mergeridge && !self.fc(facet).mergeridge2 {
                self.makeridges(facet);
            }
            facet = self.fc(facet).next;
        }
        let ms = self.facet_mergeset.clone();
        for &m in &ms {
            let mm = self.merges[m as usize];
            if mm.mtype == MRGRIDGE {
                setappend(&mut self.fm(mm.facet2).neighbors, mm.facet1);
                self.makeridges(mm.facet1);
            }
        }
    }

    fn makeridges(&mut self, facet: u32) {
        if !self.fc(facet).simplicial {
            return;
        }
        self.fm(facet).simplicial = false;
        let mut mergeridge = false;
        let nb = self.fc(facet).neighbors.clone();
        for &n in &nb {
            if n == NONE {
                break;
            }
            if n == MERGERIDGE {
                mergeridge = true;
            } else {
                self.fm(n).seen = false;
            }
        }
        let rs = self.fc(facet).ridges.clone();
        for &r in &rs {
            if r == NONE {
                break;
            }
            let o = self.otherfacet(r, facet);
            self.fm(o).seen = true;
        }
        let n = self.fc(facet).neighbors.len();
        for neighbor_i in 0..n {
            let neighbor = self.fc(facet).neighbors[neighbor_i];
            if neighbor == MERGERIDGE {
                continue;
            } else if !self.fc(neighbor).seen {
                let ridge = self.newridge();
                let rv = setnew_delnthsorted(&self.fc(facet).vertices, self.dim, neighbor_i, 0);
                self.rm(ridge).vertices = rv;
                let toporient = self.fc(facet).toporient ^ (neighbor_i & 1 == 1);
                if toporient {
                    self.rm(ridge).top = facet;
                    self.rm(ridge).bottom = neighbor;
                } else {
                    self.rm(ridge).top = neighbor;
                    self.rm(ridge).bottom = facet;
                }
                self.ridge_append(facet, ridge);
                self.ridge_append(neighbor, ridge);
            }
        }
        if mergeridge {
            while setdel(&mut self.fm(facet).neighbors, MERGERIDGE) {}
        }
    }

    fn mergecycle_all(&mut self, facetlist: u32, wasmerge: &mut bool) -> R<()> {
        let mut cycles = 0;
        let mut facet = facetlist;
        while facet != NONE {
            let mut nextfacet = self.fc(facet).next;
            if nextfacet == NONE {
                break;
            }
            if self.fc(facet).has_normal {
                facet = nextfacet;
                continue;
            }
            if !self.fc(facet).mergehorizon {
                return Err(ERR_QHULL);
            }
            let horizon = set_get(&self.fc(facet).neighbors, 0);
            if self.fc(facet).f == facet {
                let apex = self.fc(facet).vertices[0];
                let vs = self.fc(facet).vertices.clone();
                for &v in &vs {
                    if v == NONE {
                        break;
                    }
                    if v != apex {
                        self.vm(v).delridge = true;
                    }
                }
                self.fm(horizon).f = NONE;
                self.mergefacet(facet, horizon, None, true)?;
            } else {
                let samecycle = facet;
                let mut facets = 0u32;
                let mut prev = facet;
                let mut same = self.fc(facet).f;
                while same != NONE {
                    let nextsame = self.fc(same).f;
                    if self.fc(same).cycledone || self.fc(same).visible {
                        return Err(ERR_QHULL);
                    }
                    self.fm(same).cycledone = true;
                    if self.fc(same).has_normal {
                        let sf = self.fc(same).f;
                        self.fm(prev).f = sf;
                        self.fm(same).f = NONE;
                    } else {
                        prev = same;
                        facets += 1;
                    }
                    same = if same == facet { NONE } else { nextsame };
                }
                while nextfacet != NONE && self.fc(nextfacet).cycledone {
                    nextfacet = self.fc(nextfacet).next;
                }
                self.fm(horizon).f = NONE;
                self.mergecycle(samecycle, horizon)?;
                let nummerge = self.fc(horizon).nummerge + facets;
                if nummerge > QH_MAXNUMMERGE {
                    self.fm(horizon).nummerge = QH_MAXNUMMERGE;
                } else {
                    self.fm(horizon).nummerge = nummerge;
                }
            }
            cycles += 1;
            facet = nextfacet;
        }
        if cycles != 0 {
            *wasmerge = true;
        }
        Ok(())
    }

    fn mergecycle(&mut self, samecycle: u32, newfacet: u32) -> R<()> {
        if !self.VERTEXneighbors {
            self.vertexneighbors();
        }
        self.ztotmerge += 1;
        let apex = self.fc(samecycle).vertices[0];
        self.makeridges(newfacet);
        self.mergecycle_neighbors(samecycle, newfacet)?;
        self.mergecycle_ridges(samecycle, newfacet)?;
        self.mergecycle_vneighbors(samecycle, newfacet);
        if set_get(&self.fc(newfacet).vertices, 0) != apex {
            setaddnth(&mut self.fm(newfacet).vertices, 0, apex);
        }
        if !self.fc(newfacet).newfacet {
            let vs = self.fc(newfacet).vertices.clone();
            self.newvertices(&vs);
        }
        self.mergecycle_facets(samecycle, newfacet);
        Ok(())
    }

    fn mergecycle_facets(&mut self, samecycle: u32, newfacet: u32) {
        self.removefacet(newfacet);
        self.appendfacet(newfacet);
        {
            let f = self.fm(newfacet);
            f.newfacet = true;
            f.simplicial = false;
            f.newmerge = true;
        }
        let mut same = self.fc(samecycle).f;
        while same != NONE {
            let next = self.fc(same).f;
            self.willdelete(same, newfacet);
            same = if same == samecycle { NONE } else { next };
        }
        if self.fc(newfacet).center.is_some() && self.fc(newfacet).vertices.len() <= self.dim + QH_MAXNEWCENTRUM {
            self.fm(newfacet).center = None;
        }
    }

    fn mergecycle_neighbors(&mut self, samecycle: u32, newfacet: u32) -> R<()> {
        self.visit_id = self.visit_id.wrapping_add(1);
        let samevisitid = self.visit_id;
        let cyc = self.samecycle_list(samecycle);
        for &same in &cyc {
            if self.fc(same).visitid == samevisitid || self.fc(same).visible {
                return Err(ERR_QHULL);
            }
            self.fm(same).visitid = samevisitid;
        }
        self.visit_id = self.visit_id.wrapping_add(1);
        let vid = self.visit_id;
        self.fm(newfacet).visitid = vid;
        {
            let n = self.fc(newfacet).neighbors.len();
            for i in 0..n {
                let neighbor = self.fc(newfacet).neighbors[i];
                if neighbor == NONE {
                    break;
                }
                if self.fc(neighbor).visitid == samevisitid {
                    self.fm(newfacet).neighbors[i] = NONE;
                } else {
                    self.fm(neighbor).visitid = vid;
                }
            }
            setcompact(&mut self.fm(newfacet).neighbors);
        }
        for &same in &cyc {
            let nb = self.fc(same).neighbors.clone();
            for &neighbor in &nb {
                if neighbor == NONE {
                    break;
                }
                if self.fc(neighbor).visitid == samevisitid {
                    continue;
                }
                if self.fc(neighbor).simplicial {
                    if self.fc(neighbor).visitid != vid {
                        setappend(&mut self.fm(newfacet).neighbors, neighbor);
                        setreplace(&mut self.fm(neighbor).neighbors, same, newfacet);
                        self.fm(neighbor).visitid = vid;
                        let rs = self.fc(neighbor).ridges.clone();
                        for &ridge in &rs {
                            if ridge == NONE {
                                break;
                            }
                            if self.rg(ridge).top == same {
                                self.rm(ridge).top = newfacet;
                                break;
                            } else if self.rg(ridge).bottom == same {
                                self.rm(ridge).bottom = newfacet;
                                break;
                            }
                        }
                    } else {
                        self.makeridges(neighbor);
                        setdel(&mut self.fm(neighbor).neighbors, same);
                    }
                } else {
                    setdel(&mut self.fm(neighbor).neighbors, same);
                    if self.fc(neighbor).visitid != vid {
                        setappend(&mut self.fm(neighbor).neighbors, newfacet);
                        setappend(&mut self.fm(newfacet).neighbors, neighbor);
                        self.fm(neighbor).visitid = vid;
                    }
                }
            }
        }
        Ok(())
    }

    fn mergecycle_ridges(&mut self, samecycle: u32, newfacet: u32) -> R<()> {
        let samevisitid = self.visit_id.wrapping_sub(1);
        {
            let n = self.fc(newfacet).ridges.len();
            for i in 0..n {
                let ridge = self.fc(newfacet).ridges[i];
                if ridge == NONE {
                    break;
                }
                let neighbor = self.otherfacet(ridge, newfacet);
                if self.fc(neighbor).visitid == samevisitid {
                    self.fm(newfacet).ridges[i] = NONE;
                }
            }
            setcompact(&mut self.fm(newfacet).ridges);
        }
        let cyc = self.samecycle_list(samecycle);
        for &same in &cyc {
            let rs = self.fc(same).ridges.clone();
            for &ridge in &rs {
                if ridge == NONE {
                    break;
                }
                let neighbor;
                if self.rg(ridge).top == same {
                    self.rm(ridge).top = newfacet;
                    neighbor = self.rg(ridge).bottom;
                } else if self.rg(ridge).bottom == same {
                    self.rm(ridge).bottom = newfacet;
                    neighbor = self.rg(ridge).top;
                } else if self.rg(ridge).top == newfacet || self.rg(ridge).bottom == newfacet {
                    self.ridge_append(newfacet, ridge);
                    continue;
                } else {
                    return Err(ERR_QHULL);
                }
                if neighbor == newfacet {
                    self.rm(ridge).vertices = Vec::new();
                } else if self.fc(neighbor).visitid == samevisitid {
                    setdel(&mut self.fm(neighbor).ridges, ridge);
                    self.rm(ridge).vertices = Vec::new();
                } else {
                    self.ridge_append(newfacet, ridge);
                }
            }
            if self.fc(same).ridges_alloc {
                self.fm(same).ridges.clear();
            }
            if !self.fc(same).simplicial {
                continue;
            }
            let n = self.fc(same).neighbors.len();
            for neighbor_i in 0..n {
                let neighbor = self.fc(same).neighbors[neighbor_i];
                if self.fc(neighbor).visitid != samevisitid && self.fc(neighbor).simplicial {
                    let ridge = self.newridge();
                    let rv = setnew_delnthsorted(&self.fc(same).vertices, self.dim, neighbor_i, 0);
                    self.rm(ridge).vertices = rv;
                    let toporient = self.fc(same).toporient ^ (neighbor_i & 1 == 1);
                    if toporient {
                        self.rm(ridge).top = newfacet;
                        self.rm(ridge).bottom = neighbor;
                    } else {
                        self.rm(ridge).top = neighbor;
                        self.rm(ridge).bottom = newfacet;
                    }
                    self.ridge_append(newfacet, ridge);
                    self.ridge_append(neighbor, ridge);
                }
            }
        }
        Ok(())
    }

    fn basevertices(&mut self, samecycle: u32) -> Vec<u32> {
        let mut vertices = Vec::new();
        let apex = self.fc(samecycle).vertices[0];
        self.vertex_visit = self.vertex_visit.wrapping_add(1);
        let vv = self.vertex_visit;
        self.vm(apex).visitid = vv;
        let cyc = self.samecycle_list(samecycle);
        for &same in &cyc {
            if self.fc(same).mergeridge {
                continue;
            }
            let vs = self.fc(same).vertices.clone();
            for &v in &vs {
                if v == NONE {
                    break;
                }
                if self.vx(v).visitid != vv {
                    vertices.push(v);
                    self.vm(v).visitid = vv;
                    self.vm(v).seen = false;
                }
            }
        }
        vertices
    }

    fn mergecycle_vneighbors(&mut self, samecycle: u32, newfacet: u32) {
        let mergeid = self.visit_id.wrapping_sub(1);
        self.fm(newfacet).visitid = mergeid;
        let mut vertices = self.basevertices(samecycle);
        let apex = self.fc(samecycle).vertices[0];
        setappend(&mut vertices, apex);
        for &vertex in &vertices {
            self.vm(vertex).delridge = true;
            {
                let n = self.vx(vertex).neighbors.len();
                for i in 0..n {
                    let neighbor = self.vx(vertex).neighbors[i];
                    if neighbor == NONE {
                        break;
                    }
                    if self.fc(neighbor).visitid == mergeid {
                        self.vm(vertex).neighbors[i] = NONE;
                    }
                }
                setcompact(&mut self.vm(vertex).neighbors);
            }
            setappend(&mut self.vm(vertex).neighbors, newfacet);
            if set_get(&self.vx(vertex).neighbors, 1) == NONE {
                setdelsorted(&mut self.fm(newfacet).vertices, vertex);
                self.vm(vertex).deleted = true;
                setappend(&mut self.del_vertices, vertex);
            }
        }
    }

    fn forcedmerges(&mut self, wasmerge: &mut bool) -> R<()> {
        let mut nummerge = 0;
        let othermerges = std::mem::take(&mut self.facet_mergeset);
        self.facet_mergeset = Vec::new();
        for &m in &othermerges {
            let mm = self.merges[m as usize];
            if mm.mtype != MRGRIDGE {
                continue;
            }
            let mut facet1 = mm.facet1;
            let mut facet2 = mm.facet2;
            while self.fc(facet1).visible {
                facet1 = self.fc(facet1).f;
            }
            while self.fc(facet2).visible {
                facet2 = self.fc(facet2).f;
            }
            if facet1 == facet2 {
                continue;
            }
            if !setin(&self.fc(facet2).neighbors, facet1) {
                return Err(ERR_QHULL);
            }
            let (dist1, mut mindist1, mut maxdist1) = self.getdistance(facet1, facet2);
            let (dist2, mut mindist2, mut maxdist2) = self.getdistance(facet2, facet1);
            if dist1 < dist2 {
                self.mergefacet(facet1, facet2, Some((&mut mindist1, &mut maxdist1)), false)?;
            } else {
                self.mergefacet(facet2, facet1, Some((&mut mindist2, &mut maxdist2)), false)?;
                facet1 = facet2;
            }
            if !self.fc(facet1).flipped {
                nummerge += 1;
            }
        }
        for &m in &othermerges {
            if self.merges[m as usize].mtype != MRGRIDGE {
                self.facet_mergeset.push(m);
            }
        }
        if nummerge != 0 {
            *wasmerge = true;
        }
        Ok(())
    }

    fn degen_redundant_neighbors(&mut self, facet: u32, delfacet: u32) {
        if self.fc(facet).neighbors.len() < self.dim {
            self.appendmergeset(facet, facet, MRGDEGEN, None);
        }
        let delfacet = if delfacet == NONE { facet } else { delfacet };
        self.vertex_visit = self.vertex_visit.wrapping_add(1);
        let vv = self.vertex_visit;
        let vs = self.fc(facet).vertices.clone();
        for &v in &vs {
            if v == NONE {
                break;
            }
            self.vm(v).visitid = vv;
        }
        let nb = self.fc(delfacet).neighbors.clone();
        for &neighbor in &nb {
            if neighbor == NONE {
                break;
            }
            if neighbor == facet {
                continue;
            }
            let mut all = true;
            for &v in &self.fc(neighbor).vertices {
                if v == NONE {
                    break;
                }
                if self.vx(v).visitid != vv {
                    all = false;
                    break;
                }
            }
            if all {
                self.appendmergeset(neighbor, facet, MRGREDUNDANT, None);
            }
        }
        for &neighbor in &nb {
            if neighbor == NONE {
                break;
            }
            if neighbor == facet {
                continue;
            }
            if self.fc(neighbor).neighbors.len() < self.dim {
                self.appendmergeset(neighbor, neighbor, MRGDEGEN, None);
            }
        }
    }

    fn degen_redundant_facet(&mut self, facet: u32) {
        let nb = self.fc(facet).neighbors.clone();
        for &neighbor in &nb {
            if neighbor == NONE {
                break;
            }
            self.vertex_visit = self.vertex_visit.wrapping_add(1);
            let vv = self.vertex_visit;
            let nv = self.fc(neighbor).vertices.clone();
            for &v in &nv {
                if v == NONE {
                    break;
                }
                self.vm(v).visitid = vv;
            }
            let mut all = true;
            for &v in &self.fc(facet).vertices {
                if v == NONE {
                    break;
                }
                if self.vx(v).visitid != vv {
                    all = false;
                    break;
                }
            }
            if all {
                self.appendmergeset(facet, neighbor, MRGREDUNDANT, None);
                return;
            }
        }
        if self.fc(facet).neighbors.len() < self.dim {
            self.appendmergeset(facet, facet, MRGDEGEN, None);
        }
    }

    fn merge_degenredundant(&mut self) -> R<i32> {
        let mut nummerges = 0;
        loop {
            let m = setdellast(&mut self.degen_mergeset);
            if m == NONE {
                break;
            }
            let mm = self.merges[m as usize];
            let facet1 = mm.facet1;
            let mut facet2 = mm.facet2;
            let mergetype = mm.mtype;
            if self.fc(facet1).visible {
                continue;
            }
            self.fm(facet1).degenerate = false;
            self.fm(facet1).redundant = false;
            if mergetype == MRGREDUNDANT {
                while self.fc(facet2).visible {
                    if self.fc(facet2).f == NONE {
                        return Err(ERR_QHULL);
                    }
                    facet2 = self.fc(facet2).f;
                }
                if facet1 == facet2 {
                    self.degen_redundant_facet(facet1);
                    continue;
                }
                self.mergefacet(facet1, facet2, None, false)?;
                nummerges += 1;
            } else {
                let size = self.fc(facet1).neighbors.len();
                if size == 0 {
                    self.willdelete(facet1, NONE);
                    let vs = self.fc(facet1).vertices.clone();
                    for &v in &vs {
                        if v == NONE {
                            break;
                        }
                        setdel(&mut self.vm(v).neighbors, facet1);
                        if set_get(&self.vx(v).neighbors, 0) == NONE {
                            self.vm(v).deleted = true;
                            setappend(&mut self.del_vertices, v);
                        }
                    }
                    nummerges += 1;
                } else if size < self.dim {
                    let (bestneighbor, _d, mut mindist, mut maxdist) = self.findbestneighbor(facet1)?;
                    self.mergefacet(facet1, bestneighbor, Some((&mut mindist, &mut maxdist)), false)?;
                    nummerges += 1;
                }
            }
        }
        Ok(nummerges)
    }

    fn flippedmerges(&mut self, facetlist: u32, wasmerge: &mut bool) -> R<()> {
        let mut nummerge = 0;
        let mut facet = facetlist;
        while self.live(facet) {
            if self.fc(facet).flipped && !self.fc(facet).visible {
                self.appendmergeset(facet, facet, MRGFLIP, None);
            }
            facet = self.fc(facet).next;
        }
        let othermerges = std::mem::take(&mut self.facet_mergeset);
        self.facet_mergeset = Vec::new();
        for &m in &othermerges {
            let mm = self.merges[m as usize];
            let facet1 = mm.facet1;
            if mm.mtype != MRGFLIP || self.fc(facet1).visible {
                continue;
            }
            let (neighbor, _dist, mut mindist, mut maxdist) = self.findbestneighbor(facet1)?;
            self.mergefacet(facet1, neighbor, Some((&mut mindist, &mut maxdist)), false)?;
            nummerge += 1;
            self.merge_degenredundant()?;
        }
        for &m in &othermerges {
            let mm = self.merges[m as usize];
            if self.fc(mm.facet1).visible || self.fc(mm.facet2).visible {
            } else {
                self.facet_mergeset.push(m);
            }
        }
        if nummerge != 0 {
            *wasmerge = true;
        }
        Ok(())
    }

    fn findbest_test(&mut self, testcentrum: bool, facet: u32, neighbor: u32, best: &mut (u32, f64, f64, f64)) {
        let dist;
        let mut mindist = 0.0;
        let maxdist;
        if testcentrum {
            let c = self.fc(facet).center.unwrap();
            let mut d = self.distplane(&c, neighbor);
            d = d * self.dim as f64;
            if d < 0.0 {
                maxdist = 0.0;
                mindist = d;
                dist = -d;
            } else {
                maxdist = d;
                dist = d;
            }
        } else {
            let (d, mn, mx) = self.getdistance(facet, neighbor);
            dist = d;
            mindist = mn;
            maxdist = mx;
        }
        if dist < best.1 {
            best.0 = neighbor;
            best.2 = mindist;
            best.3 = maxdist;
            best.1 = dist;
        }
    }

    fn findbestneighbor(&mut self, facet: u32) -> R<(u32, f64, f64, f64)> {
        let mut best: (u32, f64, f64, f64) = (NONE, REALMAX, 0.0, 0.0);
        let mut testcentrum = false;
        let size = self.fc(facet).vertices.len();
        if size > QH_BESTCENTRUM2 * self.dim + QH_BESTCENTRUM {
            testcentrum = true;
            if self.fc(facet).center.is_none() {
                let c = self.getcentrum(facet)?;
                self.fm(facet).center = Some(c);
            }
        }
        if size > self.dim + QH_BESTNONCONVEX {
            let rs = self.fc(facet).ridges.clone();
            for &ridge in &rs {
                if ridge == NONE {
                    break;
                }
                if self.rg(ridge).nonconvex {
                    let neighbor = self.otherfacet(ridge, facet);
                    self.findbest_test(testcentrum, facet, neighbor, &mut best);
                }
            }
        }
        if best.0 == NONE {
            let nb = self.fc(facet).neighbors.clone();
            for &neighbor in &nb {
                if neighbor == NONE {
                    break;
                }
                self.findbest_test(testcentrum, facet, neighbor, &mut best);
            }
        }
        if best.0 == NONE {
            return Err(ERR_QHULL);
        }
        if testcentrum {
            let (_d, mn, mx) = self.getdistance(facet, best.0);
            best.2 = mn;
            best.3 = mx;
        }
        Ok(best)
    }

    fn sort_mergeset(&mut self) {
        let mut ms = std::mem::take(&mut self.facet_mergeset);
        let merges = &self.merges;
        if self.ANGLEmerge {
            msvc_qsort(&mut ms, &mut |a, b| if merges[a as usize].angle > merges[b as usize].angle { 1 } else { -1 });
        } else {
            msvc_qsort(&mut ms, &mut |a, b| merges[a as usize].mtype as i32 - merges[b as usize].mtype as i32);
        }
        self.facet_mergeset = ms;
    }

    fn getmergeset_initial(&mut self, facetlist: u32) -> R<()> {
        self.visit_id = self.visit_id.wrapping_add(1);
        let vid = self.visit_id;
        let mut facet = facetlist;
        while self.live(facet) {
            self.fm(facet).visitid = vid;
            self.fm(facet).tested = true;
            let nb = self.fc(facet).neighbors.clone();
            for &neighbor in &nb {
                if neighbor == NONE {
                    break;
                }
                if self.fc(neighbor).visitid != vid {
                    if self.test_appendmerge(facet, neighbor)? {
                        let rs = self.fc(neighbor).ridges.clone();
                        for &ridge in &rs {
                            if ridge == NONE {
                                break;
                            }
                            if facet == self.otherfacet(ridge, neighbor) {
                                self.rm(ridge).nonconvex = true;
                                break;
                            }
                        }
                    }
                }
            }
            let rs = self.fc(facet).ridges.clone();
            for &ridge in &rs {
                if ridge == NONE {
                    break;
                }
                self.rm(ridge).tested = true;
            }
            facet = self.fc(facet).next;
        }
        self.sort_mergeset();
        Ok(())
    }

    fn getmergeset(&mut self, facetlist: u32) -> R<()> {
        self.visit_id = self.visit_id.wrapping_add(1);
        let vid = self.visit_id;
        let mut facet = facetlist;
        while self.live(facet) {
            if self.fc(facet).tested {
                facet = self.fc(facet).next;
                continue;
            }
            self.fm(facet).visitid = vid;
            self.fm(facet).tested = true;
            let nb = self.fc(facet).neighbors.clone();
            for &neighbor in &nb {
                if neighbor == NONE {
                    break;
                }
                self.fm(neighbor).seen = false;
            }
            let rs = self.fc(facet).ridges.clone();
            for &ridge in &rs {
                if ridge == NONE {
                    break;
                }
                if self.rg(ridge).tested && !self.rg(ridge).nonconvex {
                    continue;
                }
                let neighbor = self.otherfacet(ridge, facet);
                if self.fc(neighbor).seen {
                    self.rm(ridge).tested = true;
                    self.rm(ridge).nonconvex = false;
                } else if self.fc(neighbor).visitid != vid {
                    self.rm(ridge).tested = true;
                    self.rm(ridge).nonconvex = false;
                    self.fm(neighbor).seen = true;
                    if self.test_appendmerge(facet, neighbor)? {
                        self.rm(ridge).nonconvex = true;
                    }
                }
            }
            facet = self.fc(facet).next;
        }
        self.sort_mergeset();
        Ok(())
    }

    fn test_appendmerge(&mut self, facet: u32, neighbor: u32) -> R<bool> {
        let mut dist2 = -REALMAX;
        let mut angle = -REALMAX;
        let mut isconcave = false;
        let mut iscoplanar = false;
        let mut okangle = false;
        if (!self.MERGEexact || self.POSTmerging) && self.cos_max < REALMAX / 2.0 {
            let a = self.fc(facet).normal;
            let b = self.fc(neighbor).normal;
            angle = self.getangle(&a, &b);
            if angle > self.cos_max {
                self.appendmergeset(facet, neighbor, MRGANGLECOPLANAR, Some(angle));
                return Ok(true);
            } else {
                okangle = true;
            }
        }
        if self.fc(facet).center.is_none() {
            let c = self.getcentrum(facet)?;
            self.fm(facet).center = Some(c);
        }
        let fcen = self.fc(facet).center.unwrap();
        let dist = self.distplane(&fcen, neighbor);
        if dist > self.centrum_radius {
            isconcave = true;
        } else {
            if dist > -self.centrum_radius {
                iscoplanar = true;
            }
            if self.fc(neighbor).center.is_none() {
                let c = self.getcentrum(neighbor)?;
                self.fm(neighbor).center = Some(c);
            }
            let ncen = self.fc(neighbor).center.unwrap();
            dist2 = self.distplane(&ncen, facet);
            if dist2 > self.centrum_radius {
                isconcave = true;
            } else if !iscoplanar && dist2 > -self.centrum_radius {
                iscoplanar = true;
            }
        }
        let _ = dist2;
        if !isconcave && (!iscoplanar || (self.MERGEexact && !self.POSTmerging)) {
            return Ok(false);
        }
        if !okangle && self.ANGLEmerge {
            let a = self.fc(facet).normal;
            let b = self.fc(neighbor).normal;
            angle = self.getangle(&a, &b);
        }
        if isconcave {
            if self.ANGLEmerge {
                angle += QH_ANGLECONCAVE + 0.5;
            }
            self.appendmergeset(facet, neighbor, MRGCONCAVE, Some(angle));
        } else {
            self.appendmergeset(facet, neighbor, MRGCOPLANAR, Some(angle));
        }
        Ok(true)
    }

    fn all_merges(&mut self, mut othermerge: bool, vneighbors: bool) -> R<()> {
        let mut numnewmerges = 0;
        loop {
            let mut wasmerge = false;
            while !self.facet_mergeset.is_empty() {
                loop {
                    let m = setdellast(&mut self.facet_mergeset);
                    if m == NONE {
                        break;
                    }
                    let mm = self.merges[m as usize];
                    let facet1 = mm.facet1;
                    let facet2 = mm.facet2;
                    let mergetype = mm.mtype;
                    if self.fc(facet1).visible || self.fc(facet2).visible {
                        continue;
                    }
                    if (self.fc(facet1).newfacet && !self.fc(facet1).tested)
                        || (self.fc(facet2).newfacet && !self.fc(facet2).tested)
                    {
                        if self.MERGEindependent && mergetype <= MRGANGLECOPLANAR {
                            continue;
                        }
                    }
                    self.merge_nonconvex(facet1, facet2, mergetype)?;
                    self.merge_degenredundant()?;
                    numnewmerges += 1;
                    wasmerge = true;
                }
                if self.POSTmerging && self.dim <= QH_DIMREDUCEBUILD && numnewmerges > QH_MAXNEWMERGES {
                    numnewmerges = 0;
                    self.reducevertices()?;
                }
                let nfl = self.newfacet_list;
                self.getmergeset(nfl)?;
            }
            if self.VERTEXneighbors {
                let mut isreduce = false;
                if self.dim >= 4 && self.POSTmerging {
                    let mut v = self.vertex_list;
                    while self.vlive(v) {
                        self.vm(v).delridge = true;
                        v = self.vx(v).next;
                    }
                    isreduce = true;
                }
                if (wasmerge || othermerge) && (!self.MERGEexact || self.POSTmerging) && self.dim <= QH_DIMREDUCEBUILD {
                    othermerge = false;
                    isreduce = true;
                }
                if isreduce {
                    if self.reducevertices()? {
                        let nfl = self.newfacet_list;
                        self.getmergeset(nfl)?;
                        continue;
                    }
                }
            }
            if vneighbors && self.test_vneighbors()? {
                continue;
            }
            break;
        }
        Ok(())
    }

    fn test_vneighbors(&mut self) -> R<bool> {
        Ok(false)
    }

    fn merge_nonconvex(&mut self, facet1: u32, facet2: u32, _mergetype: u8) -> R<()> {
        let (bestfacet, facet1, facet2) =
            if !self.fc(facet1).newfacet { (facet2, facet2, facet1) } else { (facet1, facet1, facet2) };
        let _ = facet1;
        let (bestneighbor, dist, mut mindist, mut maxdist) = self.findbestneighbor(bestfacet)?;
        let (neighbor, dist2, mut mindist2, mut maxdist2) = self.findbestneighbor(facet2)?;
        if dist < dist2 {
            self.mergefacet(bestfacet, bestneighbor, Some((&mut mindist, &mut maxdist)), false)?;
        } else {
            self.mergefacet(facet2, neighbor, Some((&mut mindist2, &mut maxdist2)), false)?;
        }
        Ok(())
    }

    fn willdelete(&mut self, facet: u32, replace: u32) {
        self.removefacet(facet);
        self.prependfacet(facet, ListHead::VisibleList);
        self.num_visible += 1;
        self.fm(facet).visible = true;
        self.fm(facet).f = replace;
    }

    fn newvertices(&mut self, vertices: &[u32]) {
        for &v in vertices {
            if v == NONE {
                break;
            }
            if !self.vx(v).newlist {
                self.removevertex(v);
                self.appendvertex(v);
            }
        }
    }

    fn updatetested(&mut self, facet1: u32, facet2: u32) {
        self.fm(facet2).tested = false;
        let rs = self.fc(facet1).ridges.clone();
        for &r in &rs {
            if r == NONE {
                break;
            }
            self.rm(r).tested = false;
        }
        if self.fc(facet2).center.is_none() {
            return;
        }
        let size = self.fc(facet2).vertices.len();
        if !self.fc(facet2).keepcentrum {
            if size > self.dim + QH_MAXNEWCENTRUM {
                self.fm(facet2).keepcentrum = true;
            }
        } else if size <= self.dim + QH_MAXNEWCENTRUM {
            if size == self.dim || self.POSTmerging {
                self.fm(facet2).keepcentrum = false;
            }
        }
        if !self.fc(facet2).keepcentrum {
            self.fm(facet2).center = None;
            let rs = self.fc(facet2).ridges.clone();
            for &r in &rs {
                if r == NONE {
                    break;
                }
                self.rm(r).tested = false;
            }
        }
    }

    fn mergefacet(&mut self, facet1: u32, facet2: u32, minmax: Option<(&mut f64, &mut f64)>, mergeapex: bool) -> R<()> {
        self.ztotmerge += 1;
        if facet1 == facet2 || self.fc(facet1).visible || self.fc(facet2).visible {
            return Err(ERR_QHULL);
        }
        if self.num_facets - self.num_visible <= self.dim as i32 + 1 {
            return Err(ERR_INPUT);
        }
        if !self.VERTEXneighbors {
            self.vertexneighbors();
        }
        self.makeridges(facet1);
        self.makeridges(facet2);
        if let Some((mindist, maxdist)) = minmax {
            let mx = *maxdist;
            let mn = *mindist;
            if self.max_outside < mx {
                self.max_outside = mx;
            }
            if self.max_vertex < mx {
                self.max_vertex = mx;
            }
            if self.fc(facet2).maxoutside < mx {
                self.fm(facet2).maxoutside = mx;
            }
            if self.min_vertex > mn {
                self.min_vertex = mn;
            }
            if !self.fc(facet2).keepcentrum && (mx > self.WIDEfacet || mn < -self.WIDEfacet) {
                self.fm(facet2).keepcentrum = true;
            }
        }
        let nummerge = self.fc(facet1).nummerge + self.fc(facet2).nummerge + 1;
        if nummerge >= QH_MAXNUMMERGE {
            self.fm(facet2).nummerge = QH_MAXNUMMERGE;
        } else {
            self.fm(facet2).nummerge = nummerge;
        }
        self.fm(facet2).newmerge = true;
        self.fm(facet2).dupridge = false;
        self.updatetested(facet1, facet2);
        if self.dim > 2 && self.fc(facet1).vertices.len() == self.dim {
            self.mergesimplex(facet1, facet2, mergeapex)?;
        } else {
            self.vertex_visit = self.vertex_visit.wrapping_add(1);
            let vv = self.vertex_visit;
            let vs = self.fc(facet2).vertices.clone();
            for &v in &vs {
                if v == NONE {
                    break;
                }
                self.vm(v).visitid = vv;
            }
            self.mergeneighbors(facet1, facet2);
            self.mergevertices(facet1, facet2)?;
            self.mergeridges(facet1, facet2);
            self.mergevertex_neighbors(facet1, facet2);
            if !self.fc(facet2).newfacet {
                let vs = self.fc(facet2).vertices.clone();
                self.newvertices(&vs);
            }
        }
        if !mergeapex {
            self.degen_redundant_neighbors(facet2, facet1);
        }
        self.willdelete(facet1, facet2);
        self.removefacet(facet2);
        self.appendfacet(facet2);
        self.fm(facet2).newfacet = true;
        self.fm(facet2).tested = false;
        Ok(())
    }

    fn mergesimplex(&mut self, facet1: u32, facet2: u32, mergeapex: bool) -> R<()> {
        let mut issubset = false;
        let apex;
        if mergeapex {
            if !self.fc(facet2).newfacet {
                let vs = self.fc(facet2).vertices.clone();
                self.newvertices(&vs);
            }
            apex = self.fc(facet1).vertices[0];
            if set_get(&self.fc(facet2).vertices, 0) != apex {
                setaddnth(&mut self.fm(facet2).vertices, 0, apex);
            } else {
                issubset = true;
            }
        } else {
            let vs = self.fc(facet1).vertices.clone();
            for &v in &vs {
                if v == NONE {
                    break;
                }
                self.vm(v).seen = false;
            }
            let rs = self.fc(facet1).ridges.clone();
            for &ridge in &rs {
                if ridge == NONE {
                    break;
                }
                if self.otherfacet(ridge, facet1) == facet2 {
                    let rv = self.rg(ridge).vertices.clone();
                    for &v in &rv {
                        if v == NONE {
                            break;
                        }
                        self.vm(v).seen = true;
                        self.vm(v).delridge = true;
                    }
                    break;
                }
            }
            let mut a = NONE;
            for &v in &vs {
                if v == NONE {
                    break;
                }
                if !self.vx(v).seen {
                    a = v;
                    break;
                }
            }
            apex = a;
            if apex == NONE {
                return Err(ERR_QHULL);
            }
            let n = self.fc(facet2).vertices.len();
            let mut vertex_i = 0;
            while vertex_i < n {
                let v = self.fc(facet2).vertices[vertex_i];
                if self.vx(v).id < self.vx(apex).id {
                    break;
                } else if self.vx(v).id == self.vx(apex).id {
                    issubset = true;
                    break;
                }
                vertex_i += 1;
            }
            if !issubset {
                setaddnth(&mut self.fm(facet2).vertices, vertex_i, apex);
            }
            if !self.fc(facet2).newfacet {
                let vs2 = self.fc(facet2).vertices.clone();
                self.newvertices(&vs2);
            } else if !self.vx(apex).newlist {
                self.removevertex(apex);
                self.appendvertex(apex);
            }
        }
        let vs = self.fc(facet1).vertices.clone();
        for &v in &vs {
            if v == NONE {
                break;
            }
            if v == apex && !issubset {
                setreplace(&mut self.vm(v).neighbors, facet1, facet2);
            } else {
                setdel(&mut self.vm(v).neighbors, facet1);
                if set_get(&self.vx(v).neighbors, 1) == NONE {
                    self.mergevertex_del(v, facet2);
                }
            }
        }
        self.visit_id = self.visit_id.wrapping_add(1);
        let vid = self.visit_id;
        let nb = self.fc(facet2).neighbors.clone();
        for &n in &nb {
            if n == NONE {
                break;
            }
            self.fm(n).visitid = vid;
        }
        let rs = self.fc(facet1).ridges.clone();
        for &ridge in &rs {
            if ridge == NONE {
                break;
            }
            let otherfacet = self.otherfacet(ridge, facet1);
            if otherfacet == facet2 {
                setdel(&mut self.fm(facet2).ridges, ridge);
                self.rm(ridge).vertices = Vec::new();
                setdel(&mut self.fm(facet2).neighbors, facet1);
            } else {
                self.ridge_append(facet2, ridge);
                if self.fc(otherfacet).visitid != vid {
                    setappend(&mut self.fm(facet2).neighbors, otherfacet);
                    setreplace(&mut self.fm(otherfacet).neighbors, facet1, facet2);
                    self.fm(otherfacet).visitid = vid;
                } else {
                    if self.fc(otherfacet).simplicial {
                        self.makeridges(otherfacet);
                    }
                    if set_get(&self.fc(otherfacet).neighbors, 0) != facet1 {
                        setdel(&mut self.fm(otherfacet).neighbors, facet1);
                    } else {
                        setdel(&mut self.fm(otherfacet).neighbors, facet2);
                        setreplace(&mut self.fm(otherfacet).neighbors, facet1, facet2);
                    }
                }
                if self.rg(ridge).top == facet1 {
                    self.rm(ridge).top = facet2;
                } else {
                    self.rm(ridge).bottom = facet2;
                }
            }
        }
        if !self.fc(facet1).ridges.is_empty() {
            self.fm(facet1).ridges[0] = NONE;
        }
        Ok(())
    }

    fn mergevertex_del(&mut self, vertex: u32, facet2: u32) {
        setdelsorted(&mut self.fm(facet2).vertices, vertex);
        self.vm(vertex).deleted = true;
        setappend(&mut self.del_vertices, vertex);
    }

    fn mergevertex_neighbors(&mut self, facet1: u32, facet2: u32) {
        let vv = self.vertex_visit;
        let vs = self.fc(facet1).vertices.clone();
        for &v in &vs {
            if v == NONE {
                break;
            }
            if self.vx(v).visitid != vv {
                setreplace(&mut self.vm(v).neighbors, facet1, facet2);
            } else {
                setdel(&mut self.vm(v).neighbors, facet1);
                if set_get(&self.vx(v).neighbors, 1) == NONE {
                    self.mergevertex_del(v, facet2);
                }
            }
        }
    }

    fn mergevertices(&mut self, facet1: u32, facet2: u32) -> R<()> {
        let v1 = self.fc(facet1).vertices.clone();
        let v2 = std::mem::take(&mut self.fm(facet2).vertices);
        let newsize = v1.len() + v2.len() - self.dim + 1;
        let mut merged = Vec::with_capacity(newsize);
        let mut j = 0usize;
        for &vertex in &v1 {
            if vertex == NONE {
                break;
            }
            let cur = set_get(&v2, j);
            if cur == NONE || self.vx(vertex).id > self.vx(cur).id {
                merged.push(vertex);
            } else {
                while set_get(&v2, j) != NONE && self.vx(set_get(&v2, j)).id > self.vx(vertex).id {
                    merged.push(v2[j]);
                    j += 1;
                }
                let cur = set_get(&v2, j);
                if cur == NONE || self.vx(cur).id < self.vx(vertex).id {
                    merged.push(vertex);
                } else {
                    merged.push(cur);
                    j += 1;
                }
            }
        }
        while set_get(&v2, j) != NONE {
            merged.push(v2[j]);
            j += 1;
        }
        if newsize < merged.len() {
            return Err(ERR_QHULL);
        }
        self.fm(facet2).vertices = merged;
        Ok(())
    }

    fn mergeneighbors(&mut self, facet1: u32, facet2: u32) {
        self.visit_id = self.visit_id.wrapping_add(1);
        let vid = self.visit_id;
        let nb2 = self.fc(facet2).neighbors.clone();
        for &n in &nb2 {
            if n == NONE {
                break;
            }
            self.fm(n).visitid = vid;
        }
        let nb1 = self.fc(facet1).neighbors.clone();
        for &neighbor in &nb1 {
            if neighbor == NONE {
                break;
            }
            if self.fc(neighbor).visitid == vid {
                if self.fc(neighbor).simplicial {
                    self.makeridges(neighbor);
                }
                if set_get(&self.fc(neighbor).neighbors, 0) != facet1 {
                    setdel(&mut self.fm(neighbor).neighbors, facet1);
                } else {
                    setdel(&mut self.fm(neighbor).neighbors, facet2);
                    setreplace(&mut self.fm(neighbor).neighbors, facet1, facet2);
                }
            } else if neighbor != facet2 {
                setappend(&mut self.fm(facet2).neighbors, neighbor);
                setreplace(&mut self.fm(neighbor).neighbors, facet1, facet2);
            }
        }
        setdel(&mut self.fm(facet1).neighbors, facet2);
        setdel(&mut self.fm(facet2).neighbors, facet1);
    }

    fn mergeridges(&mut self, facet1: u32, facet2: u32) {
        let mut i = 0usize;
        loop {
            let ridge = set_get(&self.fc(facet2).ridges, i);
            if ridge == NONE {
                break;
            }
            i += 1;
            if self.rg(ridge).top == facet1 || self.rg(ridge).bottom == facet1 {
                let rv = self.rg(ridge).vertices.clone();
                for &v in &rv {
                    if v == NONE {
                        break;
                    }
                    self.vm(v).delridge = true;
                }
                self.delridge(ridge);
                i -= 1;
            }
        }
        let rs = self.fc(facet1).ridges.clone();
        for &ridge in &rs {
            if ridge == NONE {
                break;
            }
            if self.rg(ridge).top == facet1 {
                self.rm(ridge).top = facet2;
            } else {
                self.rm(ridge).bottom = facet2;
            }
            self.ridge_append(facet2, ridge);
        }
    }

    fn reducevertices(&mut self) -> R<bool> {
        let mut degenredun = false;
        if self.dim == 2 {
            return Ok(false);
        }
        if self.merge_degenredundant()? != 0 {
            degenredun = true;
        }
        {
            let mut nf = self.newfacet_list;
            while self.live(nf) {
                if self.fc(nf).newmerge {
                    if !self.MERGEvertices {
                        self.fm(nf).newmerge = false;
                    }
                    self.remove_extravertices(nf);
                }
                nf = self.fc(nf).next;
            }
            if !self.MERGEvertices {
                return Ok(false);
            }
            let mut nf = self.newfacet_list;
            while self.live(nf) {
                if self.fc(nf).newmerge {
                    self.fm(nf).newmerge = false;
                    let mut i = 0usize;
                    loop {
                        let vertex = set_get(&self.fc(nf).vertices, i);
                        if vertex == NONE {
                            break;
                        }
                        i += 1;
                        if self.vx(vertex).delridge {
                            if self.rename_sharedvertex(vertex, nf)? != NONE {
                                i -= 1;
                            }
                        }
                    }
                }
                nf = self.fc(nf).next;
            }
            let mut v = self.newvertex_list;
            while self.vlive(v) {
                if self.vx(v).delridge && !self.vx(v).deleted {
                    self.vm(v).delridge = false;
                }
                v = self.vx(v).next;
            }
        }
        Ok(degenredun)
    }

    fn remove_extravertices(&mut self, facet: u32) -> bool {
        let mut foundrem = false;
        let vs = self.fc(facet).vertices.clone();
        for &v in &vs {
            if v == NONE {
                break;
            }
            self.vm(v).seen = false;
        }
        let rs = self.fc(facet).ridges.clone();
        for &r in &rs {
            if r == NONE {
                break;
            }
            let rv = self.rg(r).vertices.clone();
            for &v in &rv {
                if v == NONE {
                    break;
                }
                self.vm(v).seen = true;
            }
        }
        let mut i = 0usize;
        loop {
            let vertex = set_get(&self.fc(facet).vertices, i);
            if vertex == NONE {
                break;
            }
            i += 1;
            if !self.vx(vertex).seen {
                foundrem = true;
                setdelsorted(&mut self.fm(facet).vertices, vertex);
                setdel(&mut self.vm(vertex).neighbors, facet);
                if self.vx(vertex).neighbors.is_empty() {
                    self.vm(vertex).deleted = true;
                    setappend(&mut self.del_vertices, vertex);
                } else {
                    i -= 1;
                }
            }
        }
        foundrem
    }

    fn rename_sharedvertex(&mut self, vertex: u32, facet: u32) -> R<u32> {
        let mut neighbor_a = NONE;
        if self.vx(vertex).neighbors.len() == 2 {
            neighbor_a = self.vx(vertex).neighbors[0];
            if neighbor_a == facet {
                neighbor_a = self.vx(vertex).neighbors[1];
            }
        } else if self.dim == 3 {
            return Ok(NONE);
        } else {
            self.visit_id = self.visit_id.wrapping_add(1);
            let vid = self.visit_id;
            let nb = self.fc(facet).neighbors.clone();
            for &n in &nb {
                if n == NONE {
                    break;
                }
                self.fm(n).visitid = vid;
            }
            let vn = self.vx(vertex).neighbors.clone();
            for &n in &vn {
                if n == NONE {
                    break;
                }
                if self.fc(n).visitid == vid {
                    if neighbor_a != NONE {
                        return Ok(NONE);
                    }
                    neighbor_a = n;
                }
            }
            if neighbor_a == NONE {
                return Err(ERR_QHULL);
            }
        }
        let mut ridges: Vec<u32> = Vec::new();
        self.visit_id = self.visit_id.wrapping_add(1);
        let vid = self.visit_id;
        self.fm(neighbor_a).visitid = vid;
        self.vertexridges_facet(vertex, facet, &mut ridges);
        let mut vertices = self.vertexintersect_new(&self.fc(facet).vertices, &self.fc(neighbor_a).vertices);
        setdel(&mut vertices, vertex);
        let newvertex = self.find_newvertex(vertex, &mut vertices, &ridges);
        if newvertex != NONE {
            self.renamevertex(vertex, newvertex, &ridges, facet, neighbor_a)?;
        }
        Ok(newvertex)
    }

    fn vertexridges_facet(&mut self, vertex: u32, facet: u32, ridges: &mut Vec<u32>) {
        let vid = self.visit_id;
        let rs = self.fc(facet).ridges.clone();
        for &ridge in &rs {
            if ridge == NONE {
                break;
            }
            let neighbor = self.otherfacet(ridge, facet);
            if self.fc(neighbor).visitid == vid && setin(&self.rg(ridge).vertices, vertex) {
                ridges.push(ridge);
            }
        }
        self.fm(facet).visitid = vid.wrapping_sub(1);
    }

    fn vertexridges(&mut self, vertex: u32) -> Vec<u32> {
        let mut ridges = Vec::new();
        self.visit_id = self.visit_id.wrapping_add(1);
        let vid = self.visit_id;
        let vn = self.vx(vertex).neighbors.clone();
        for &n in &vn {
            if n == NONE {
                break;
            }
            self.fm(n).visitid = vid;
        }
        let mut i = 0usize;
        loop {
            let n = set_get(&vn, i);
            if n == NONE {
                break;
            }
            i += 1;
            if set_get(&vn, i) != NONE {
                self.vertexridges_facet(vertex, n, &mut ridges);
            }
        }
        ridges
    }

    fn ridgehash(&self, hashsize: usize, ridge: u32, skip: u32) -> usize {
        let rv = &self.rg(ridge).vertices;
        self.gethash(hashsize, rv, self.dim - 1, 0, skip)
    }

    fn hashridge(&mut self, hashsize: usize, ridge: u32, oldvertex: u32) {
        let mut hash = self.ridgehash(hashsize, ridge, oldvertex);
        loop {
            let r = self.hash_table[hash];
            if r == NONE {
                self.hash_table[hash] = ridge;
                break;
            } else if r == ridge {
                break;
            }
            hash += 1;
            if hash == hashsize {
                hash = 0;
            }
        }
    }

    fn hashridge_find(&self, hashsize: usize, ridge: u32, vertex: u32, oldvertex: u32) -> u32 {
        let mut hash = self.ridgehash(hashsize, ridge, vertex);
        loop {
            let ra = self.hash_table[hash];
            if ra == NONE {
                break;
            }
            if ra != ridge {
                if setequal_except(&self.rg(ridge).vertices, vertex, &self.rg(ra).vertices, oldvertex) {
                    return ra;
                }
            }
            hash += 1;
            if hash == hashsize {
                hash = 0;
            }
        }
        NONE
    }

    fn find_newvertex(&mut self, oldvertex: u32, vertices: &mut Vec<u32>, ridges: &[u32]) -> u32 {
        for &v in vertices.iter() {
            if v == NONE {
                break;
            }
            self.vm(v).visitid = 0;
        }
        for &r in ridges {
            if r == NONE {
                break;
            }
            let rv = self.rg(r).vertices.clone();
            for &v in &rv {
                if v == NONE {
                    break;
                }
                let vv = self.vm(v);
                vv.visitid = vv.visitid.wrapping_add(1);
            }
        }
        let mut i = 0usize;
        loop {
            let v = set_get(vertices, i);
            if v == NONE {
                break;
            }
            i += 1;
            if self.vx(v).visitid == 0 {
                setdelnth(vertices, i - 1);
                i -= 1;
            }
        }
        self.vertex_visit = self.vertex_visit.wrapping_add(ridges.len() as u32);
        if vertices.is_empty() {
            return NONE;
        }
        {
            let verts = &self.verts;
            msvc_qsort(vertices, &mut |a, b| {
                (verts[a as usize].visitid.wrapping_sub(verts[b as usize].visitid)) as i32
            });
        }
        let hashsize = self.newhashtable(ridges.len());
        for &r in ridges {
            if r == NONE {
                break;
            }
            self.hashridge(hashsize, r, oldvertex);
        }
        let mut result = NONE;
        for &vertex in vertices.iter() {
            if vertex == NONE {
                break;
            }
            let newridges = self.vertexridges(vertex);
            let mut found_all = true;
            for &ridge in &newridges {
                if self.hashridge_find(hashsize, ridge, vertex, oldvertex) != NONE {
                    found_all = false;
                    break;
                }
            }
            if found_all {
                result = vertex;
                break;
            }
        }
        self.hash_table = Vec::new();
        result
    }

    fn renameridgevertex(&mut self, ridge: u32, oldvertex: u32, newvertex: u32) {
        let oldnth = setindex(&self.rg(ridge).vertices, oldvertex);
        if oldnth >= 0 {
            setdelnthsorted(&mut self.rm(ridge).vertices, oldnth as usize);
        }
        let mut nth: i32 = 0;
        let rv = self.rg(ridge).vertices.clone();
        for &v in &rv {
            if v == NONE {
                break;
            }
            if v == newvertex {
                if self.rg(ridge).nonconvex {
                    self.copynonconvex(ridge);
                }
                self.delridge(ridge);
                return;
            }
            if self.vx(v).id < self.vx(newvertex).id {
                break;
            }
            nth += 1;
        }
        setaddnth(&mut self.rm(ridge).vertices, nth as usize, newvertex);
        if (oldnth - nth).abs() % 2 != 0 {
            let r = self.rm(ridge);
            std::mem::swap(&mut r.top, &mut r.bottom);
        }
    }

    fn copynonconvex(&mut self, atridge: u32) {
        let facet = self.rg(atridge).top;
        let otherfacet = self.rg(atridge).bottom;
        let rs = self.fc(facet).ridges.clone();
        for &ridge in &rs {
            if ridge == NONE {
                break;
            }
            if otherfacet == self.otherfacet(ridge, facet) && ridge != atridge {
                self.rm(ridge).nonconvex = true;
                break;
            }
        }
    }

    fn renamevertex(
        &mut self,
        oldvertex: u32,
        newvertex: u32,
        ridges: &[u32],
        oldfacet: u32,
        neighbor_a: u32,
    ) -> R<()> {
        for &r in ridges {
            if r == NONE {
                break;
            }
            self.renameridgevertex(r, oldvertex, newvertex);
        }
        if oldfacet == NONE {
            let mut i = 0usize;
            loop {
                let neighbor = set_get(&self.vx(oldvertex).neighbors, i);
                if neighbor == NONE {
                    break;
                }
                i += 1;
                self.maydropneighbor(neighbor);
                setdelsorted(&mut self.fm(neighbor).vertices, oldvertex);
                if self.remove_extravertices(neighbor) {
                    i -= 1;
                }
            }
            if !self.vx(oldvertex).deleted {
                self.vm(oldvertex).deleted = true;
                setappend(&mut self.del_vertices, oldvertex);
            }
        } else if self.vx(oldvertex).neighbors.len() == 2 {
            let vn = self.vx(oldvertex).neighbors.clone();
            for &n in &vn {
                if n == NONE {
                    break;
                }
                setdelsorted(&mut self.fm(n).vertices, oldvertex);
            }
            self.vm(oldvertex).deleted = true;
            setappend(&mut self.del_vertices, oldvertex);
        } else {
            setdelsorted(&mut self.fm(oldfacet).vertices, oldvertex);
            setdel(&mut self.vm(oldvertex).neighbors, oldfacet);
            self.remove_extravertices(neighbor_a);
        }
        Ok(())
    }

    fn maydropneighbor(&mut self, facet: u32) {
        self.visit_id = self.visit_id.wrapping_add(1);
        let vid = self.visit_id;
        let rs = self.fc(facet).ridges.clone();
        for &r in &rs {
            if r == NONE {
                break;
            }
            let (t, b) = (self.rg(r).top, self.rg(r).bottom);
            self.fm(t).visitid = vid;
            self.fm(b).visitid = vid;
        }
        let mut i = 0usize;
        loop {
            let neighbor = set_get(&self.fc(facet).neighbors, i);
            if neighbor == NONE {
                break;
            }
            i += 1;
            if self.fc(neighbor).visitid != vid {
                setdel(&mut self.fm(facet).neighbors, neighbor);
                i -= 1;
                setdel(&mut self.fm(neighbor).neighbors, facet);
                if self.fc(neighbor).neighbors.len() < self.dim {
                    self.appendmergeset(neighbor, neighbor, MRGDEGEN, Some(QH_ANGLEDEGEN));
                }
            }
        }
        if self.fc(facet).neighbors.len() < self.dim {
            self.appendmergeset(facet, facet, MRGDEGEN, Some(QH_ANGLEDEGEN));
        }
    }
}

impl Qh {
    fn init_globals(&mut self) -> R<()> {
        self.premerge_centrum = 0.0;
        self.PREmerge = true;
        self.MERGING = true;
        self.ALLpoints = true;
        self.APPROXhull = true;
        self.SETroundoff = true;
        if self.MERGING && !self.POSTmerge && self.premerge_cos > REALMAX / 2.0 && self.premerge_centrum == 0.0 {
            self.ZEROcentrum = true;
            self.ZEROall_ok = true;
        }
        self.DOcheckmax = self.MERGING;
        self.KEEPnearinside = self.DOcheckmax && !(self.KEEPinside && self.KEEPcoplanar);
        if self.dim > QH_DIMMERGEVERTEX {
            self.MERGEvertices = false;
        }
        self.qh_srand(1);
        for _ in 0..1000 {
            self.qh_rand();
        }
        self.qh_srand(1);
        if self.num_points < self.dim + 1 {
            return Err(ERR_INPUT);
        }
        Ok(())
    }

    fn joggleinput(&mut self) -> R<()> {
        if self.build_cnt > 1 && self.JOGGLEmax > fmax_(self.MAXwidth * 0.25, 0.1) {
            return Err(ERR_QHULL);
        }
        let _seed = self.qh_rand();
        let randa = self.JOGGLEmax * 2.0 * (1.0 / QH_RANDOMMAX);
        let randb = -self.JOGGLEmax;
        let size = self.num_points * self.dim;
        for i in 0..size {
            let randr = self.qh_rand() as f64;
            self.pts[i] = self.pts[i] + (randr * randa + randb);
        }
        Ok(())
    }

    fn qhull(&mut self) -> R<()> {
        if self.JOGGLEmax < REALMAX / 2.0 {
            self.build_cnt += 1;
            self.joggleinput()?;
            self.initbuild()?;
            self.buildhull()?;
        } else {
            self.initbuild()?;
            self.buildhull()?;
        }
        if self.ZEROall_ok && !self.TESTvneighbors && self.MERGEexact {
            self.checkzero(true);
        }
        Ok(())
    }

    fn output(&self) -> R<Vec<QhFacet>> {
        let mut out = Vec::with_capacity(self.num_facets.max(0) as usize);
        let mut f = self.facet_list;
        while self.live(f) {
            let vs = self.facet3vertex(f)?;
            let fc = self.fc(f);
            out.push(QhFacet {
                id: fc.id,
                normal: fc.normal,
                offset: fc.offset,
                vertices: vs.iter().map(|&v| self.vx(v).point as usize).collect(),
            });
            f = fc.next;
        }
        Ok(out)
    }
}

fn run(points: &[[f64; 3]], o: Opts) -> Result<Vec<QhFacet>, i32> {
    let mut qh = Qh::new(points, &o);
    qh.init_globals()?;
    qh.qhull()?;
    qh.output()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_box() {
        let mut pts = Vec::new();
        for &x in &[0.0, 1.6256] {
            for &y in &[0.0, 0.8128] {
                for &z in &[0.0, 0.4064] {
                    pts.push([x, y, z]);
                }
            }
        }
        let f = qhull_ivp(&pts).unwrap();
        assert_eq!(f.len(), 6);
        assert_eq!(f[0].id, 1);
        assert_eq!(f[0].vertices, vec![4, 6, 2, 0]);
    }

    #[test]
    fn format_g_matches_printf() {
        assert_eq!(format_g(1e-12), "1E-12");
        assert_eq!(format_g(9.999999960041972e-13), "1E-12");
        assert_eq!(format_g(2.2e-12), "2.2E-12");
        assert_eq!(format_g(0.5), "0.5");
        assert_eq!(format_g(123456.7), "123457");
        assert_eq!(format_g(1234567.0), "1.23457E+06");
    }
}

#[doc(hidden)]
pub fn msvc_qsort_test(keys: &[f64], mode: u32) -> Vec<u32> {
    let mut a: Vec<u32> = (0..keys.len() as u32).collect();
    if mode == 0 {
        msvc_qsort(&mut a, &mut |x, y| if keys[x as usize] > keys[y as usize] { 1 } else { -1 });
    } else {
        msvc_qsort(&mut a, &mut |x, y| keys[x as usize] as i32 - keys[y as usize] as i32);
    }
    a
}

pub fn qhull_ivp_retry(points: &[[f64; 3]], max_joggles: usize) -> Result<(Vec<QhFacet>, usize), i32> {
    let mut rc = match qhull_ivp(points) {
        Ok(f) => return Ok((f, 0)),
        Err(e) => e,
    };
    let mut j = 1.0e-12f32 as f64;
    for k in 0..max_joggles {
        match qhull_ivp_joggle(points, j) {
            Ok(f) => return Ok((f, k + 1)),
            Err(e) => rc = e,
        }
        j = j * (1.2f32 as f64) + (1.2e-12f32 as f64);
    }
    Err(rc)
}
