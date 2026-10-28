#![forbid(unsafe_code)]

pub mod generate;
mod parse;
mod stats;
pub mod write;

pub use generate::{GenInput, GenReport, GenSolid, generate};
pub use parse::parse;
pub use stats::{NavStats, area_components};
pub use write::write;

use std::path::Path;

pub const NAV_MAGIC: u32 = 0xFEED_FACE;

pub const MAX_VERSION: u32 = 16;

pub const MAX_FILE_SIZE: u64 = 256 << 20;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NavError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("not a navigation mesh file")]
    NotNav,
    #[error("unsupported navigation mesh version {0}")]
    UnsupportedVersion(u32),
    #[error("corrupt navigation mesh: {0}")]
    Corrupt(String),
    #[error("the navigation mesh file is too large")]
    TooLarge,
}

pub type Result<T> = std::result::Result<T, NavError>;

pub mod attr {
    pub const CROUCH: u32 = 0x0001;
    pub const JUMP: u32 = 0x0002;
    pub const PRECISE: u32 = 0x0004;
    pub const NO_JUMP: u32 = 0x0008;
    pub const STOP: u32 = 0x0010;
    pub const RUN: u32 = 0x0020;
    pub const WALK: u32 = 0x0040;
    pub const AVOID: u32 = 0x0080;
    pub const TRANSIENT: u32 = 0x0100;
    pub const DONT_HIDE: u32 = 0x0200;
    pub const STAND: u32 = 0x0400;
    pub const NO_HOSTAGES: u32 = 0x0800;
    pub const STAIRS: u32 = 0x1000;
    pub const NO_MERGE: u32 = 0x2000;
    pub const OBSTACLE_TOP: u32 = 0x4000;
    pub const CLIFF: u32 = 0x8000;
    pub const HAS_ELEVATOR: u32 = 0x4000_0000;
    pub const NAV_BLOCKER: u32 = 0x8000_0000;

    pub const NAMES: &[(u32, &str)] = &[
        (CROUCH, "crouch"),
        (JUMP, "jump"),
        (PRECISE, "precise"),
        (NO_JUMP, "no_jump"),
        (STOP, "stop"),
        (RUN, "run"),
        (WALK, "walk"),
        (AVOID, "avoid"),
        (TRANSIENT, "transient"),
        (DONT_HIDE, "dont_hide"),
        (STAND, "stand"),
        (NO_HOSTAGES, "no_hostages"),
        (STAIRS, "stairs"),
        (NO_MERGE, "no_merge"),
        (OBSTACLE_TOP, "obstacle_top"),
        (CLIFF, "cliff"),
        (HAS_ELEVATOR, "has_elevator"),
        (NAV_BLOCKER, "nav_blocker"),
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    North = 0,
    East = 1,
    South = 2,
    West = 3,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::North, Dir::East, Dir::South, Dir::West];
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HidingSpot {
    pub id: u32,
    pub pos: [f32; 3],
    pub flags: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NavArea {
    pub id: u32,
    pub flags: u32,
    pub nw: [f32; 3],
    pub se: [f32; 3],
    pub ne_z: f32,
    pub sw_z: f32,
    pub connections: [Vec<u32>; 4],
    pub hiding_spots: Vec<HidingSpot>,
    pub encounter_paths: u32,
    pub place: u16,
    pub ladders_up: Vec<u32>,
    pub ladders_down: Vec<u32>,
    pub light: Option<[f32; 4]>,
    pub visible_areas: u32,
}

impl NavArea {
    pub fn corners(&self) -> [[f32; 3]; 4] {
        [self.nw, [self.se[0], self.nw[1], self.ne_z], self.se, [self.nw[0], self.se[1], self.sw_z]]
    }

    pub fn center(&self) -> [f32; 3] {
        let c = self.corners();
        let mut out = [0.0f32; 3];
        for p in &c {
            for k in 0..3 {
                out[k] += p[k] * 0.25;
            }
        }
        out
    }

    pub fn size_2d(&self) -> f32 {
        ((self.se[0] - self.nw[0]).abs() * (self.se[1] - self.nw[1]).abs()).max(0.0)
    }

    pub fn all_connections(&self) -> impl Iterator<Item = (Dir, u32)> + '_ {
        Dir::ALL.into_iter().flat_map(move |d| self.connections[d as usize].iter().map(move |&id| (d, id)))
    }

    pub fn edge_center(&self, dir: Dir) -> [f32; 3] {
        let c = self.corners();
        let (a, b) = match dir {
            Dir::North => (c[0], c[1]),
            Dir::East => (c[1], c[2]),
            Dir::South => (c[2], c[3]),
            Dir::West => (c[3], c[0]),
        };
        [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NavLadder {
    pub id: u32,
    pub width: f32,
    pub top: [f32; 3],
    pub bottom: [f32; 3],
    pub length: f32,
    pub dir: u32,
    pub top_areas: [u32; 4],
    pub bottom_area: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NavMesh {
    pub version: u32,
    pub subversion: u32,
    pub bsp_size: Option<u32>,
    pub analyzed: bool,
    pub places: Vec<String>,
    pub has_unnamed_areas: bool,
    pub areas: Vec<NavArea>,
    pub ladders: Vec<NavLadder>,
    pub area_custom_bytes: usize,
    pub trailing_bytes: usize,
}

impl NavMesh {
    pub fn place_name(&self, area: &NavArea) -> Option<&str> {
        let i = usize::from(area.place).checked_sub(1)?;
        self.places.get(i).map(String::as_str)
    }

    pub fn index_by_id(&self) -> std::collections::HashMap<u32, usize> {
        let mut map = std::collections::HashMap::with_capacity(self.areas.len());
        for (i, a) in self.areas.iter().enumerate() {
            map.entry(a.id).or_insert(i);
        }
        map
    }

    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut it = self.areas.iter().flat_map(NavArea::corners).filter(|p| p.iter().all(|v| v.is_finite()));
        let first = it.next()?;
        let (mut lo, mut hi) = (first, first);
        for p in it {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        Some((lo, hi))
    }
}

pub fn load(path: &Path) -> Result<NavMesh> {
    let len = std::fs::metadata(path)?.len();
    if len > MAX_FILE_SIZE {
        return Err(NavError::TooLarge);
    }
    let data = std::fs::read(path)?;
    parse(&data)
}

#[cfg(test)]
mod tests;
