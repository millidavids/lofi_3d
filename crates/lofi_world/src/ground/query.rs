//! What the ground can be asked. An implementor supplies heights and cell
//! flags; every answer is worked out here, so the editor's terrain and a
//! cooked world answer alike.

use bevy_math::{Vec2, Vec3};

use super::cell::{CellShape, GroundCell, cell_plane, locate};
use super::collision::{Bounds, CollisionMesh};
use super::{collision, ray};
use crate::grid::ChunkCoord;
use crate::masks::WALKABLE_SLOPE_DEGREES;

/// The ground under a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundPoint {
    pub position: Vec3,
    /// Of the triangle under the point: what a foot stands on, not the
    /// smoothed normal the ground is shaded with.
    pub normal: Vec3,
    /// Not flagged unwalkable, and no steeper than
    /// [`WALKABLE_SLOPE_DEGREES`].
    pub walkable: bool,
    /// Ground clutter (grass, pebbles) stays off this cell.
    pub no_clutter: bool,
}

/// What's under a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GroundAt {
    Surface(GroundPoint),
    /// A hole cut in the terrain: nothing to stand on.
    Hole,
    /// No data: off the map, or a part of a cooked world that isn't loaded.
    /// Not a hole. A character here should wait, not fall.
    Unknown,
}

/// Where a ray met the ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundHit {
    pub distance: f32,
    pub point: Vec3,
    pub normal: Vec3,
    pub cell: (u32, u32),
    pub walkable: bool,
}

/// What a ray found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RayTrace {
    Hit(GroundHit),
    /// Nothing in the way, over ground that's all known.
    Clear,
    /// The ray reached ground with no data (a tile that isn't loaded) before
    /// it hit anything. What lies past `distance` can't be said: it is not
    /// a clear line.
    Unknown {
        distance: f32,
    },
}

/// What a ray does at a hole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holes {
    /// Falls through, as a character would.
    Open,
    /// Stops as if the hole were filled in: an editor's cursor must be able
    /// to point at a hole to fill it.
    Solid,
}

pub trait Ground {
    /// Metres between vertices.
    fn cell_size(&self) -> f32;

    /// Cells along x and along z, over the whole map.
    fn cell_counts(&self) -> (u32, u32);

    /// A vertex's height. `None` where this ground has no data: off the
    /// map, or in a part of a cooked world that isn't loaded.
    fn vertex_height(&self, x: u32, z: u32) -> Option<f32>;

    /// How a cell is cut and flagged. `None` as for [`Self::vertex_height`].
    fn cell_shape(&self, cx: u32, cz: u32) -> Option<CellShape>;

    /// Heights at a cell's (x, z), (x+1, z), (x, z+1), (x+1, z+1).
    fn cell_corners(&self, cx: u32, cz: u32) -> Option<[f32; 4]> {
        Some([
            self.vertex_height(cx, cz)?,
            self.vertex_height(cx + 1, cz)?,
            self.vertex_height(cx, cz + 1)?,
            self.vertex_height(cx + 1, cz + 1)?,
        ])
    }

    fn ground_cell(&self, cx: u32, cz: u32) -> Option<GroundCell> {
        Some(GroundCell {
            corners: self.cell_corners(cx, cz)?,
            shape: self.cell_shape(cx, cz)?,
        })
    }

    /// What's under a world XZ: ground, a hole, or nothing known.
    fn ground_at(&self, xz: Vec2) -> GroundAt {
        let size = self.cell_size();
        let Some((cx, cz, u, v)) = locate(size, self.cell_counts(), xz) else {
            return GroundAt::Unknown;
        };
        // A point exactly on a cell's edge is on the next cell's edge too,
        // and at a streaming border only one of them may be loaded. The
        // flags then come from that one, and may change when the other
        // loads.
        let across = |c: u32, t: f32| (t == 0.0 && c > 0).then(|| (c - 1, 1.0));
        let xs = [Some((cx, u)), across(cx, u)];
        let zs = [Some((cz, v)), across(cz, v)];
        let found = zs.into_iter().flatten().find_map(|(cz, v)| {
            let cell = |(cx, u)| Some((self.ground_cell(cx, cz)?, u, v));
            xs.into_iter().flatten().find_map(cell)
        });
        let Some((cell, u, v)) = found else {
            return GroundAt::Unknown;
        };
        if cell.shape.hole {
            return GroundAt::Hole;
        }
        let plane = cell_plane(cell.corners, cell.shape.main_diagonal, u, v);
        let normal = Vec3::new(-plane.rise_x, size, -plane.rise_z).normalize();
        GroundAt::Surface(GroundPoint {
            position: Vec3::new(xz.x, plane.height, xz.y),
            normal,
            walkable: walkable(normal, cell.shape),
            no_clutter: cell.shape.no_clutter,
        })
    }

    /// The ground under a world XZ, if there is ground there. See
    /// [`Self::ground_at`] to tell a hole from a place with no data.
    fn point_at(&self, xz: Vec2) -> Option<GroundPoint> {
        match self.ground_at(xz) {
            GroundAt::Surface(point) => Some(point),
            GroundAt::Hole | GroundAt::Unknown => None,
        }
    }

    /// Follows a ray to the first point where it meets the ground from
    /// above, or to where the loaded ground runs out. A ray that starts
    /// under the surface doesn't hit it from below; one that starts on it
    /// and heads down hits at distance 0.
    fn trace_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        holes: Holes,
    ) -> RayTrace {
        ray::trace(self, origin, direction, max_distance, holes)
    }

    /// [`Self::trace_ray`] for callers who only want a hit. `None` is also
    /// what a ray into unloaded ground gives: where line of sight or a
    /// projectile must not change as tiles stream in, use `trace_ray`.
    fn cast_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        holes: Holes,
    ) -> Option<GroundHit> {
        match self.trace_ray(origin, direction, max_distance, holes) {
            RayTrace::Hit(hit) => Some(hit),
            RayTrace::Clear | RayTrace::Unknown { .. } => None,
        }
    }

    /// The box around a chunk, with its real height range.
    fn chunk_bounds(&self, chunk: ChunkCoord) -> Option<Bounds> {
        collision::chunk_bounds(self, chunk)
    }

    /// A chunk's surface as triangles, holes left out: what a triangle-mesh
    /// collider takes.
    fn chunk_collision(&self, chunk: ChunkCoord) -> Option<CollisionMesh> {
        collision::chunk_mesh(self, chunk)
    }
}

pub(super) fn walkable(normal: Vec3, shape: CellShape) -> bool {
    !shape.unwalkable && normal.y >= WALKABLE_SLOPE_DEGREES.to_radians().cos()
}
