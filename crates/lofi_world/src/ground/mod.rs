//! The ground as collision and placement see it: where the surface is, which
//! way it faces, whether it can be walked on, where a ray meets it, and its
//! boxes and triangles for a physics engine.
//!
//! [`Ground`] is implemented by the editor's [`crate::Terrain`] and by a
//! cooked world. Implementors hand over heights and cell flags; the answers
//! are computed here, once, so a tool and the game can't disagree.

mod cell;
mod collision;
mod query;
mod ray;
mod terrain;

#[cfg(test)]
pub(crate) mod tests;

pub use cell::{CellShape, GroundCell};
pub(crate) use cell::{cell_plane, locate};
pub use collision::{Bounds, CollisionMesh};
pub use query::{Ground, GroundAt, GroundHit, GroundPoint, Holes, RayTrace};
