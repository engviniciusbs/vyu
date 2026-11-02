//! Pure geometry for the free-form canvas (no egui types, fully unit-tested).

pub mod geometry;
pub mod snap;

pub use geometry::*;
pub use snap::{Guide, snap};
