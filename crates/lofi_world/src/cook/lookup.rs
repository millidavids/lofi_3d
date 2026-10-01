//! Reading a cooked chunk: its vertices, its cells, the ground on it.

use bevy_math::{Vec2, Vec3};

use super::HEIGHT_STEP;
use super::chunk::{CookedChunk, SIDE};
use super::normal::decode_normal;
use crate::grid::{CHUNK_CELLS, ChunkCoord};
use crate::ground::{CellShape, cell_plane};

impl CookedChunk {
    #[must_use]
    pub const fn coord(&self) -> ChunkCoord {
        ChunkCoord {
            x: self.x,
            z: self.z,
        }
    }

    fn vertex(lx: u32, lz: u32) -> usize {
        (lz * SIDE + lx) as usize
    }

    /// Height of a vertex, `lx, lz` in `0..=32`.
    #[must_use]
    pub fn height(&self, lx: u32, lz: u32) -> f32 {
        let v = Self::vertex(lx, lz);
        let step = if self.wide {
            let i = v * 4;
            u32::from_le_bytes([
                self.heights[i],
                self.heights[i + 1],
                self.heights[i + 2],
                self.heights[i + 3],
            ])
        } else {
            let i = v * 2;
            u32::from(u16::from_le_bytes([self.heights[i], self.heights[i + 1]]))
        };
        // Integers first, so shared vertices come out bit-identical.
        (i64::from(self.base) + i64::from(step)) as f32 * HEIGHT_STEP
    }

    /// The smoothed normal the vertex is shaded with. For the face a foot
    /// stands on, ask the ground: [`crate::Ground::ground_at`].
    #[must_use]
    pub fn normal(&self, lx: u32, lz: u32) -> Vec3 {
        let i = Self::vertex(lx, lz) * 2;
        decode_normal([self.normals[i] as i8, self.normals[i + 1] as i8])
    }

    /// Weights for the four slots at a vertex.
    #[must_use]
    pub fn weights(&self, lx: u32, lz: u32) -> [u8; 4] {
        let i = Self::vertex(lx, lz) * 4;
        [
            self.weights[i],
            self.weights[i + 1],
            self.weights[i + 2],
            self.weights[i + 3],
        ]
    }

    /// Tint r, g, b and AO at a vertex.
    #[must_use]
    pub fn light(&self, lx: u32, lz: u32) -> [u8; 4] {
        let i = Self::vertex(lx, lz) * 4;
        [
            self.light[i],
            self.light[i + 1],
            self.light[i + 2],
            self.light[i + 3],
        ]
    }

    fn bit(bits: &[u8], cx: u32, cz: u32) -> bool {
        let bit = (cz * CHUNK_CELLS + cx) as usize;
        bits[bit / 8] & (1 << (bit % 8)) != 0
    }

    /// `cx, cz` in `0..32`.
    #[must_use]
    pub fn splits_on_main(&self, cx: u32, cz: u32) -> bool {
        Self::bit(&self.split, cx, cz)
    }

    #[must_use]
    pub fn is_hole(&self, cx: u32, cz: u32) -> bool {
        Self::bit(&self.hole, cx, cz)
    }

    #[must_use]
    pub fn no_clutter(&self, cx: u32, cz: u32) -> bool {
        Self::bit(&self.no_clutter, cx, cz)
    }

    #[must_use]
    pub fn unwalkable(&self, cx: u32, cz: u32) -> bool {
        Self::bit(&self.unwalkable, cx, cz)
    }

    /// The ground at a point, metres from the chunk's first vertex, on the
    /// same triangles the editor drew. `None` outside the chunk or in a hole.
    #[must_use]
    pub fn height_at(&self, local: Vec2, cell_size: f32) -> Option<f32> {
        self.height_at_cells(local / cell_size)
    }

    /// [`Self::height_at`], with the point in cells from the first vertex.
    #[must_use]
    pub fn height_at_cells(&self, grid: Vec2) -> Option<f32> {
        let last = CHUNK_CELLS as f32;
        if !(0.0..=last).contains(&grid.x) || !(0.0..=last).contains(&grid.y) {
            return None;
        }
        let cx = (grid.x as u32).min(CHUNK_CELLS - 1);
        let cz = (grid.y as u32).min(CHUNK_CELLS - 1);
        if self.is_hole(cx, cz) {
            return None;
        }
        let (u, v) = (grid.x - cx as f32, grid.y - cz as f32);
        Some(cell_plane(self.corners(cx, cz), self.splits_on_main(cx, cz), u, v).height)
    }

    /// Heights at a cell's (x, z), (x+1, z), (x, z+1), (x+1, z+1); `cx, cz`
    /// in `0..32`.
    #[must_use]
    pub fn corners(&self, cx: u32, cz: u32) -> [f32; 4] {
        [
            self.height(cx, cz),
            self.height(cx + 1, cz),
            self.height(cx, cz + 1),
            self.height(cx + 1, cz + 1),
        ]
    }

    /// How a cell is cut and flagged, as collision sees it.
    #[must_use]
    pub fn shape(&self, cx: u32, cz: u32) -> CellShape {
        CellShape {
            main_diagonal: self.splits_on_main(cx, cz),
            hole: self.is_hole(cx, cz),
            unwalkable: self.unwalkable(cx, cz),
            no_clutter: self.no_clutter(cx, cz),
        }
    }

    /// The chunk's lowest and highest vertex, in height steps.
    #[must_use]
    pub fn step_range(&self) -> (i64, i64) {
        let width = if self.wide { 4 } else { 2 };
        let steps = self.heights.chunks_exact(width).map(|b| match *b {
            [a, b] => u32::from(u16::from_le_bytes([a, b])),
            [a, b, c, d] => u32::from_le_bytes([a, b, c, d]),
            _ => 0,
        });
        // A cook writes steps up from `base`, so the lowest is 0; scanned
        // anyway, since the reader checks this against the manifest.
        let (low, top) = steps.fold((u32::MAX, 0), |(low, top), s| (low.min(s), top.max(s)));
        let base = i64::from(self.base);
        (base + i64::from(low.min(top)), base + i64::from(top))
    }
}
