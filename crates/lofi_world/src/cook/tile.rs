//! One tile: a square of chunks and the objects standing on it.

use bevy_math::Vec2;
use serde::{Deserialize, Serialize};

use super::chunk::CookedChunk;
use crate::grid::CHUNK_CELLS;

/// An object as the game places it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CookedObject {
    pub id: u64,
    /// The asset GUID as two halves, high first.
    pub asset: [u64; 2],
    pub position: [f32; 3],
    pub yaw: f32,
    pub scale: f32,
}

impl CookedObject {
    pub(super) fn is_sound(&self) -> bool {
        self.position.iter().all(|v| v.is_finite())
            && self.yaw.is_finite()
            && self.scale.is_finite()
            && self.scale > 0.0
    }
}

/// One tile: its chunks, row by row, and the objects standing on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CookedTile {
    pub x: u32,
    pub z: u32,
    pub cell_size: f32,
    pub chunks: Vec<CookedChunk>,
    /// By id. Objects past the map's edge belong to the nearest edge tile.
    pub objects: Vec<CookedObject>,
}

impl CookedTile {
    /// The ground under a world XZ, if this tile holds it and it isn't a
    /// hole.
    #[must_use]
    pub fn height_at(&self, xz: Vec2) -> Option<f32> {
        // In cells, as the editor does it: dividing chunk by chunk rounds
        // differently, and can drop a point on the map's far edge.
        let grid = xz / self.cell_size;
        let cells = CHUNK_CELLS as f32;
        self.chunks.iter().find_map(|chunk| {
            let local = grid - Vec2::new(chunk.x as f32, chunk.z as f32) * cells;
            chunk.height_at_cells(local)
        })
    }
}

impl CookedTile {
    /// The tile's lowest and highest ground, in height steps. `None` for a
    /// tile with no chunks.
    #[must_use]
    pub fn step_range(&self) -> Option<(i64, i64)> {
        self.chunks
            .iter()
            .map(CookedChunk::step_range)
            .reduce(|(low, high), (l, h)| (low.min(l), high.max(h)))
    }
}
