//! Tarugo's furniture model, without any UI: materials, parametric parts, the front sketch,
//! free-form outlines, joints, screws analysis and the cut list. All lengths are millimetres.

pub mod analysis;
pub mod cutlist;
pub mod edit;
pub mod expr;
pub mod front;
pub mod joints;
pub mod material;
pub mod model;
pub mod shape;
pub mod solve;

/// Two lengths closer than this are the same (mm).
pub const EPS: f64 = 0.01;
