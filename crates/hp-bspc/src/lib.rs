#![deny(unsafe_code)]

pub mod areas;
pub mod brush;
pub mod brushbsp;
pub mod compile;
pub mod cubemap;
pub mod detail;
pub mod detail_props;
pub mod disp;
pub mod emit;
pub mod faces;
pub mod materials;
pub mod occluder;
pub mod phys;
pub mod planes;
pub mod portals;
pub mod prop_debug;
pub mod props;
pub mod rad;
pub mod stats;
pub mod texinfo;
pub mod tree;
pub mod validate;
pub mod vis;
pub mod water;
pub mod winding;

#[cfg(test)]
mod parity_tests;

pub use compile::{CompileError, CompileOptions, CompileOutput, VisLevel, compile};
pub use materials::{MaterialFacts, MaterialSource};
