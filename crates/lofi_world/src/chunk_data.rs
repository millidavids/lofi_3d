//! The samples one chunk stores.

use std::sync::Arc;

use crate::cells::CellFlags;
use crate::grid::CHUNK_CELLS;
use crate::paint::ChunkPaint;

/// Per-vertex colour multiplier, `value / 128` per channel.
pub type Tint = [u8; 3];

/// The tint that changes nothing.
pub const NEUTRAL_TINT: Tint = [128; 3];

/// What differs between two versions of a chunk's data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChunkDifferences {
    pub heights: bool,
    pub cells: bool,
    /// Paint or tint.
    pub surface: bool,
}

/// Cells per chunk; every chunk has exactly its own.
pub(crate) const CHUNK_CELL_COUNT: usize = (CHUNK_CELLS * CHUNK_CELLS) as usize;

/// One chunk's data:
/// - heights and tint cover the vertices the chunk owns
///   ([`crate::GridSize::owned_rect`]);
/// - paint covers every vertex its mesh uses;
/// - cells are the chunk's own 32×32, row-major.
///
/// Each data type is its own `Arc`, shared until written. Cloning is a pointer
/// copy, so undo and save snapshots are cheap, and a sculpt stroke copies
/// heights without dragging the rest along.
#[derive(Clone, Debug, PartialEq)]
pub struct ChunkData {
    width: u32,
    pub(crate) heights: Arc<Vec<f32>>,
    pub(crate) paint: Arc<ChunkPaint>,
    pub(crate) cells: Arc<Vec<CellFlags>>,
    pub(crate) tint: Arc<Vec<Tint>>,
}

impl ChunkData {
    /// Cells start clear and tint neutral.
    pub(crate) fn new(width: u32, heights: Vec<f32>, paint: ChunkPaint) -> Self {
        let vertices = heights.len();
        Self::from_parts(
            width,
            heights,
            paint,
            vec![CellFlags::default(); CHUNK_CELL_COUNT],
            vec![NEUTRAL_TINT; vertices],
        )
    }

    pub(crate) fn from_parts(
        width: u32,
        heights: Vec<f32>,
        paint: ChunkPaint,
        cells: Vec<CellFlags>,
        tint: Vec<Tint>,
    ) -> Self {
        debug_assert_eq!(heights.len() % width as usize, 0);
        debug_assert_eq!(cells.len(), CHUNK_CELL_COUNT);
        debug_assert_eq!(tint.len(), heights.len());
        Self {
            width,
            heights: Arc::new(heights),
            paint: Arc::new(paint),
            cells: Arc::new(cells),
            tint: Arc::new(tint),
        }
    }

    pub(crate) const fn local_index(&self, lx: u32, lz: u32) -> usize {
        lz as usize * self.width as usize + lx as usize
    }

    pub(crate) fn same_shape(&self, other: &Self) -> bool {
        self.width == other.width
            && self.heights.len() == other.heights.len()
            && self.tint.len() == other.tint.len()
            && self.cells.len() == other.cells.len()
    }

    /// Which kinds of data aren't shared with `other`, so a swap knows what
    /// it changed. Unshared but equal counts as changed; it's only a pointer
    /// check.
    #[must_use]
    pub fn differences(&self, other: &Self) -> ChunkDifferences {
        ChunkDifferences {
            heights: !Arc::ptr_eq(&self.heights, &other.heights),
            cells: !Arc::ptr_eq(&self.cells, &other.cells),
            surface: !Arc::ptr_eq(&self.paint, &other.paint)
                || !Arc::ptr_eq(&self.tint, &other.tint),
        }
    }

    /// Heap bytes held here but not shared with `other`. Undo budgets with it.
    #[must_use]
    pub fn unshared_bytes(&self, other: &Self) -> usize {
        let mut bytes = 0;
        if !Arc::ptr_eq(&self.heights, &other.heights) {
            bytes += size_of_val(self.heights.as_slice());
        }
        if !Arc::ptr_eq(&self.paint, &other.paint) {
            bytes += size_of_val(self.paint.weights());
        }
        if !Arc::ptr_eq(&self.cells, &other.cells) {
            bytes += size_of_val(self.cells.as_slice());
        }
        if !Arc::ptr_eq(&self.tint, &other.tint) {
            bytes += size_of_val(self.tint.as_slice());
        }
        bytes
    }
}
