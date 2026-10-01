//! The loaded part of a cooked world, as ground the game collides with.

use std::collections::HashMap;

use super::chunk::CookedChunk;
use super::format::CookError;
use super::layout::TileLayout;
use super::tile::CookedTile;
use crate::grid::{CHUNK_CELLS, ChunkCoord};
use crate::ground::{CellShape, Ground};

/// Whichever tiles are loaded, answering as one piece of ground. Put tiles
/// in as they stream in and take them out as they leave: queries cross tile
/// seams, and ground that isn't loaded reads as unknown, never as a hole.
#[derive(Clone, Debug)]
pub struct CookedGround {
    layout: TileLayout,
    tiles: HashMap<(u32, u32), CookedTile>,
}

impl CookedGround {
    #[must_use]
    pub fn new(layout: TileLayout) -> Self {
        Self {
            layout,
            tiles: HashMap::new(),
        }
    }

    #[must_use]
    pub const fn layout(&self) -> TileLayout {
        self.layout
    }

    /// Adds a tile, replacing one already at its place.
    ///
    /// # Errors
    /// If the tile isn't shaped as this world's layout says, which would
    /// send lookups to the wrong chunk. Tiles from
    /// [`super::CookedWorld::load_tile`] of the same world always fit. (A
    /// chunk's own blobs were checked when it was read.)
    pub fn insert(&mut self, tile: CookedTile) -> Result<(), CookError> {
        let expected = self.layout.chunks_of(tile.x, tile.z);
        let fits = tile.cell_size == self.layout.cell_size()
            && !tile.chunks.is_empty()
            && tile.chunks.iter().map(CookedChunk::coord).eq(expected);
        if !fits {
            return Err(CookError::Invalid(format!(
                "tile {}, {} is from another world",
                tile.x, tile.z
            )));
        }
        self.tiles.insert((tile.x, tile.z), tile);
        Ok(())
    }

    pub fn remove(&mut self, x: u32, z: u32) -> Option<CookedTile> {
        self.tiles.remove(&(x, z))
    }

    #[must_use]
    pub fn is_loaded(&self, x: u32, z: u32) -> bool {
        self.tiles.contains_key(&(x, z))
    }

    /// The loaded tiles' coordinates, in no particular order.
    pub fn loaded(&self) -> impl Iterator<Item = (u32, u32)> {
        self.tiles.keys().copied()
    }

    /// A loaded chunk. Tiles store their chunks row by row, which `insert`
    /// checked, so this is arithmetic, not a search.
    fn chunk(&self, coord: ChunkCoord) -> Option<&CookedChunk> {
        let size = self.layout.size();
        if coord.x >= size.chunks_x() || coord.z >= size.chunks_z() {
            return None;
        }
        let side = self.layout.tile_chunks();
        let (tx, tz) = self.layout.tile_of(coord);
        let tile = self.tiles.get(&(tx, tz))?;
        let across = side.min(size.chunks_x() - tx * side);
        let index = (coord.z - tz * side) * across + (coord.x - tx * side);
        tile.chunks.get(index as usize)
    }
}

impl Ground for CookedGround {
    fn cell_size(&self) -> f32 {
        self.layout.cell_size()
    }

    fn cell_counts(&self) -> (u32, u32) {
        let size = self.layout.size();
        (size.cells_x(), size.cells_z())
    }

    fn vertex_height(&self, x: u32, z: u32) -> Option<f32> {
        // A border vertex is in the chunks on both sides of the border, and
        // either may be the one that's loaded.
        let chunks = |v: u32| {
            let after = v / CHUNK_CELLS;
            let before = (v.is_multiple_of(CHUNK_CELLS) && after > 0).then(|| after - 1);
            [Some(after), before].into_iter().flatten()
        };
        chunks(z)
            .flat_map(|cz| chunks(x).map(move |cx| ChunkCoord { x: cx, z: cz }))
            .find_map(|coord| {
                let chunk = self.chunk(coord)?;
                Some(chunk.height(x - coord.x * CHUNK_CELLS, z - coord.z * CHUNK_CELLS))
            })
    }

    fn cell_shape(&self, cx: u32, cz: u32) -> Option<CellShape> {
        let coord = ChunkCoord {
            x: cx / CHUNK_CELLS,
            z: cz / CHUNK_CELLS,
        };
        let chunk = self.chunk(coord)?;
        Some(chunk.shape(cx % CHUNK_CELLS, cz % CHUNK_CELLS))
    }

    // A cell's four corners are all in its own chunk: one lookup, not four.
    fn cell_corners(&self, cx: u32, cz: u32) -> Option<[f32; 4]> {
        let coord = ChunkCoord {
            x: cx / CHUNK_CELLS,
            z: cz / CHUNK_CELLS,
        };
        let chunk = self.chunk(coord)?;
        Some(chunk.corners(cx % CHUNK_CELLS, cz % CHUNK_CELLS))
    }
}
