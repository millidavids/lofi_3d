//! A chunk's box and its triangles, for a physics engine.

use bevy_math::Vec3;

use super::query::Ground;
use crate::grid::{CHUNK_CELLS, ChunkCoord};

/// An axis-aligned box in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl Bounds {
    #[must_use]
    pub fn contains(&self, p: Vec3) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max).all()
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}

/// A chunk's 33×33 vertices in world space and the triangles between them,
/// wound counter-clockwise seen from above, holes left out.
#[derive(Clone, Debug, PartialEq)]
pub struct CollisionMesh {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

/// Vertices along a chunk's side.
const SIDE: u32 = CHUNK_CELLS + 1;

fn inside<G: Ground + ?Sized>(ground: &G, chunk: ChunkCoord) -> bool {
    let (cells_x, cells_z) = ground.cell_counts();
    chunk.x < cells_x / CHUNK_CELLS && chunk.z < cells_z / CHUNK_CELLS
}

pub(super) fn chunk_bounds<G: Ground + ?Sized>(ground: &G, chunk: ChunkCoord) -> Option<Bounds> {
    if !inside(ground, chunk) {
        return None;
    }
    let rect = chunk.vertex_rect();
    let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
    for (x, z) in rect.iter() {
        let h = ground.vertex_height(x, z)?;
        low = low.min(h);
        high = high.max(h);
    }
    let size = ground.cell_size();
    Some(Bounds {
        min: Vec3::new(rect.min_x as f32 * size, low, rect.min_z as f32 * size),
        max: Vec3::new(rect.max_x as f32 * size, high, rect.max_z as f32 * size),
    })
}

pub(super) fn chunk_mesh<G: Ground + ?Sized>(
    ground: &G,
    chunk: ChunkCoord,
) -> Option<CollisionMesh> {
    if !inside(ground, chunk) {
        return None;
    }
    let rect = chunk.vertex_rect();
    let size = ground.cell_size();
    let positions = rect
        .iter()
        .map(|(x, z)| {
            Some([
                x as f32 * size,
                ground.vertex_height(x, z)?,
                z as f32 * size,
            ])
        })
        .collect::<Option<Vec<_>>>()?;
    let mut indices = Vec::with_capacity((CHUNK_CELLS * CHUNK_CELLS * 6) as usize);
    for lz in 0..CHUNK_CELLS {
        for lx in 0..CHUNK_CELLS {
            let shape = ground.cell_shape(rect.min_x + lx, rect.min_z + lz)?;
            if shape.hole {
                continue;
            }
            let a = lz * SIDE + lx;
            let (b, c, d) = (a + 1, a + SIDE, a + SIDE + 1);
            // The same two triangles the mesher and the ray test use.
            indices.extend(if shape.main_diagonal {
                [a, c, d, a, d, b]
            } else {
                [a, c, b, b, c, d]
            });
        }
    }
    Some(CollisionMesh { positions, indices })
}
