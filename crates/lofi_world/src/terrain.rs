//! The heightmap.

use std::sync::Arc;

use bevy_math::{Vec2, Vec3};

use crate::cells::{CellFlags, Triangulation};
use crate::chunk_data::{CHUNK_CELL_COUNT, ChunkData, NEUTRAL_TINT, Tint};
use crate::grid::{CHUNK_CELLS, ChunkCoord, GridSize};
use crate::materials::MaterialId;
use crate::paint::ChunkPaint;

/// Heights are clamped to ±this, in metres. Keeps a runaway brush finite.
pub const HEIGHT_LIMIT: f32 = 4096.0;

/// Cell edge limits, in metres.
pub const MIN_CELL_SIZE: f32 = 0.5;
pub const MAX_CELL_SIZE: f32 = 16.0;

/// A regular heightmap with a vertex at every cell corner, plus its paint
/// (see `terrain_paint`).
///
/// World space: vertex `(x, z)` sits at `(x * cell_size, height, z * cell_size)`,
/// so the map spans `0..extent` on X and Z with +Y up. Heights live in the
/// chunk that owns them (see [`GridSize::owner`]); cloning a terrain copies
/// pointers, not samples.
#[derive(Clone, Debug, PartialEq)]
pub struct Terrain {
    size: GridSize,
    cell_size: f32,
    pub(crate) triangulation: Triangulation,
    pub(crate) chunks: Vec<ChunkData>,
}

/// Everything a terrain is built from, e.g. by a file reader. Vertex and cell
/// data are row-major over the whole map.
pub(crate) struct TerrainParts<'a> {
    pub heights: &'a [f32],
    /// One per chunk, in chunk order.
    pub paints: Vec<ChunkPaint>,
    /// `None`: every cell clear.
    pub cells: Option<&'a [CellFlags]>,
    /// `None`: every vertex neutral.
    pub tints: Option<&'a [Tint]>,
    pub triangulation: Triangulation,
}

impl Terrain {
    /// Flat ground painted all `base`, cut the way new maps are.
    #[must_use]
    pub fn flat(size: GridSize, cell_size: f32, height: f32, base: MaterialId) -> Self {
        let height = clamp_height(height);
        let chunks = size
            .chunks()
            .map(|chunk| {
                let rect = size.owned_rect(chunk);
                ChunkData::new(
                    rect.width(),
                    vec![height; rect.vertex_count()],
                    ChunkPaint::solid(base),
                )
            })
            .collect();
        Self {
            size,
            cell_size: clamp_cell_size(cell_size),
            triangulation: Triangulation::FollowTerrain,
            chunks,
        }
    }

    /// Errors say what's wrong.
    pub(crate) fn from_parts(
        size: GridSize,
        cell_size: f32,
        parts: TerrainParts<'_>,
    ) -> Result<Self, String> {
        let TerrainParts {
            heights,
            paints,
            cells,
            tints,
            triangulation,
        } = parts;
        if !(MIN_CELL_SIZE..=MAX_CELL_SIZE).contains(&cell_size) {
            return Err(format!(
                "cell size {cell_size} is outside {MIN_CELL_SIZE}..={MAX_CELL_SIZE}"
            ));
        }
        let expect = |what: &str, expected: usize, found: usize| {
            if expected == found {
                Ok(())
            } else {
                Err(format!("expected {expected} {what}, found {found}"))
            }
        };
        expect("heights", size.vertex_count(), heights.len())?;
        expect("chunk paints", size.chunk_count(), paints.len())?;
        if let Some(cells) = cells {
            expect(
                "cells",
                size.cells_x() as usize * size.cells_z() as usize,
                cells.len(),
            )?;
        }
        if let Some(tints) = tints {
            expect("tints", size.vertex_count(), tints.len())?;
        }
        if heights.iter().any(|h| !h.is_finite()) {
            return Err("heights contain NaN or infinity".to_owned());
        }
        let chunks = size
            .chunks()
            .zip(paints)
            .map(|(chunk, paint)| {
                let rect = size.owned_rect(chunk);
                let index = |(x, z): (u32, u32)| size.vertex_index(x, z);
                let h = rect
                    .iter()
                    .map(|v| clamp_height(heights[index(v)]))
                    .collect();
                let tint = rect
                    .iter()
                    .map(|v| tints.map_or(NEUTRAL_TINT, |t| t[index(v)]))
                    .collect();
                let chunk_cells = cells.map_or_else(
                    || vec![CellFlags::default(); CHUNK_CELL_COUNT],
                    |cells| chunk_cells_from(cells, size, chunk),
                );
                ChunkData::from_parts(rect.width(), h, paint, chunk_cells, tint)
            })
            .collect();
        let mut terrain = Self {
            size,
            cell_size,
            triangulation,
            chunks,
        };
        terrain.make_shared_paint_agree()?;
        Ok(terrain)
    }

    #[must_use]
    pub const fn size(&self) -> GridSize {
        self.size
    }

    #[must_use]
    pub const fn cell_size(&self) -> f32 {
        self.cell_size
    }

    /// World-space size on X and Z.
    #[must_use]
    pub fn extent(&self) -> Vec2 {
        Vec2::new(self.size.cells_x() as f32, self.size.cells_z() as f32) * self.cell_size
    }

    #[must_use]
    pub fn height(&self, x: u32, z: u32) -> f32 {
        let (chunk, i) = self.locate_vertex(x, z);
        self.chunks[chunk].heights[i]
    }

    /// Clamps to [`HEIGHT_LIMIT`]. NaN is ignored. Returns whether the height
    /// changed; an unchanged write doesn't copy a shared chunk.
    pub fn set_height(&mut self, x: u32, z: u32, height: f32) -> bool {
        if height.is_nan() {
            return false;
        }
        let height = clamp_height(height);
        let (chunk, i) = self.locate_vertex(x, z);
        if self.chunks[chunk].heights[i] == height {
            return false;
        }
        Arc::make_mut(&mut self.chunks[chunk].heights)[i] = height;
        true
    }

    /// All heights in row-major order, for files and exports.
    #[must_use]
    pub fn heights_row_major(&self) -> Vec<f32> {
        self.size
            .full_rect()
            .iter()
            .map(|(x, z)| self.height(x, z))
            .collect()
    }

    #[must_use]
    pub fn chunk_data(&self, chunk: ChunkCoord) -> &ChunkData {
        &self.chunks[self.size.chunk_index(chunk)]
    }

    /// Puts `data` in place of a chunk's samples and returns the old ones.
    ///
    /// # Panics
    /// If `data` came from a different grid. Undo only ever swaps a chunk's
    /// own earlier data back in, so a mismatch is a bug, not bad input.
    pub fn replace_chunk_data(&mut self, chunk: ChunkCoord, data: ChunkData) -> ChunkData {
        let slot = &mut self.chunks[self.size.chunk_index(chunk)];
        assert!(slot.same_shape(&data), "chunk data from another grid");
        std::mem::replace(slot, data)
    }

    #[must_use]
    pub fn vertex_position(&self, x: u32, z: u32) -> Vec3 {
        Vec3::new(
            x as f32 * self.cell_size,
            self.height(x, z),
            z as f32 * self.cell_size,
        )
    }

    /// World position of a chunk's first vertex, at height zero.
    #[must_use]
    pub fn chunk_origin(&self, chunk: ChunkCoord) -> Vec3 {
        let rect = chunk.vertex_rect();
        Vec3::new(
            rect.min_x as f32 * self.cell_size,
            0.0,
            rect.min_z as f32 * self.cell_size,
        )
    }

    /// Lowest and highest vertex.
    #[must_use]
    pub fn height_range(&self) -> (f32, f32) {
        self.chunks
            .iter()
            .flat_map(|chunk| chunk.heights.iter())
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &h| {
                (lo.min(h), hi.max(h))
            })
    }

    /// Owning chunk index and index inside it.
    pub(crate) fn locate_vertex(&self, x: u32, z: u32) -> (usize, usize) {
        let (chunk, lx, lz) = self.size.owner(x, z);
        let index = self.size.chunk_index(chunk);
        (index, self.chunks[index].local_index(lx, lz))
    }
}

/// One chunk's cells, cut out of a row-major whole-map list.
fn chunk_cells_from(cells: &[CellFlags], size: GridSize, chunk: ChunkCoord) -> Vec<CellFlags> {
    let row = size.cells_x() as usize;
    let x0 = (chunk.x * CHUNK_CELLS) as usize;
    let z0 = (chunk.z * CHUNK_CELLS) as usize;
    (z0..z0 + CHUNK_CELLS as usize)
        .flat_map(|z| &cells[z * row + x0..z * row + x0 + CHUNK_CELLS as usize])
        .copied()
        .collect()
}

fn clamp_height(h: f32) -> f32 {
    h.clamp(-HEIGHT_LIMIT, HEIGHT_LIMIT)
}

fn clamp_cell_size(s: f32) -> f32 {
    if s.is_finite() {
        s.clamp(MIN_CELL_SIZE, MAX_CELL_SIZE)
    } else {
        MIN_CELL_SIZE
    }
}

/// One chunk of uneven, repeatable heights for tests.
#[cfg(test)]
pub(crate) fn bumpy() -> Terrain {
    let size = GridSize::new(1, 1).expect("valid size");
    let mut t = Terrain::flat(size, 2.0, 0.0, MaterialId(0));
    for (x, z) in size.full_rect().iter() {
        t.set_height(x, z, ((x * 7 + z * 13) % 11) as f32);
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_clamp_and_ignore_nan() {
        let mut t = bumpy();
        t.set_height(1, 1, 1e9);
        assert_eq!(t.height(1, 1), HEIGHT_LIMIT);
        t.set_height(1, 1, f32::NAN);
        assert_eq!(t.height(1, 1), HEIGHT_LIMIT);
    }

    fn parts(heights: &[f32], paints: Vec<ChunkPaint>) -> TerrainParts<'_> {
        TerrainParts {
            heights,
            paints,
            cells: None,
            tints: None,
            triangulation: Triangulation::FollowTerrain,
        }
    }

    #[test]
    fn from_parts_rejects_bad_samples() {
        let size = GridSize::new(1, 1).expect("valid size");
        let n = size.vertex_count();
        let paint = || vec![ChunkPaint::solid(MaterialId(0))];
        let ok = vec![0.0; n];
        assert!(Terrain::from_parts(size, 2.0, parts(&ok, paint())).is_ok());
        assert!(Terrain::from_parts(size, 2.0, parts(&ok[1..], paint())).is_err());
        assert!(Terrain::from_parts(size, 0.1, parts(&ok, paint())).is_err());
        assert!(Terrain::from_parts(size, 2.0, parts(&ok, Vec::new())).is_err());
        let mut heights = ok.clone();
        heights[3] = f32::NAN;
        assert!(Terrain::from_parts(size, 2.0, parts(&heights, paint())).is_err());
        let short_cells = vec![CellFlags::default(); 10];
        let short_tints = vec![NEUTRAL_TINT; n - 1];
        let mut bad = parts(&ok, paint());
        bad.cells = Some(&short_cells);
        assert!(Terrain::from_parts(size, 2.0, bad).is_err());
        let mut bad = parts(&ok, paint());
        bad.tints = Some(&short_tints);
        assert!(Terrain::from_parts(size, 2.0, bad).is_err());
    }

    #[test]
    fn heights_round_trip_across_chunks() {
        let size = GridSize::new(3, 2).expect("valid size");
        let mut t = Terrain::flat(size, 2.0, 0.0, MaterialId(0));
        let unique = |x: u32, z: u32| (x * 100 + z) as f32 * 0.01;
        for (x, z) in size.full_rect().iter() {
            t.set_height(x, z, unique(x, z));
        }
        let paints = size
            .chunks()
            .map(|c| (*t.chunk_data(c).paint).clone())
            .collect();
        let heights = t.heights_row_major();
        let rebuilt = Terrain::from_parts(size, 2.0, parts(&heights, paints)).expect("valid");
        assert_eq!(rebuilt, t);
        // The closing corner and a four-chunk corner.
        assert_eq!(t.height(96, 64), unique(96, 64));
        assert_eq!(t.height(32, 32), unique(32, 32));
    }

    #[test]
    fn clones_share_until_written() {
        let original = bumpy();
        let mut copy = original.clone();
        let chunk = ChunkCoord { x: 0, z: 0 };
        assert_eq!(
            copy.chunk_data(chunk)
                .unshared_bytes(original.chunk_data(chunk)),
            0
        );
        copy.set_height(4, 4, 50.0);
        let unshared = copy
            .chunk_data(chunk)
            .unshared_bytes(original.chunk_data(chunk));
        assert_eq!(
            unshared,
            33 * 33 * size_of::<f32>(),
            "only heights were copied"
        );
        assert_ne!(original.height(4, 4), 50.0);
    }

    #[test]
    fn replacing_chunk_data_swaps_it_back() {
        let mut t = bumpy();
        let chunk = ChunkCoord { x: 0, z: 0 };
        let before = t.chunk_data(chunk).clone();
        t.set_height(3, 3, 42.0);
        let after = t.replace_chunk_data(chunk, before);
        assert_ne!(t.height(3, 3), 42.0);
        t.replace_chunk_data(chunk, after);
        assert_eq!(t.height(3, 3), 42.0);
    }
}
