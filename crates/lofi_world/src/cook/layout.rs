//! Where the cooked terrain is: how a map is cut into tiles, and the world
//! rectangle of each. The cook, the reader, and anything that streams tiles
//! or sorts things into them ask here.

use bevy_math::Vec2;

use super::format::MAX_TILE_CHUNKS;
use crate::grid::{CHUNK_CELLS, ChunkCoord, GridSize};
use crate::terrain::{MAX_CELL_SIZE, MIN_CELL_SIZE};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileLayout {
    size: GridSize,
    cell_size: f32,
    tile_chunks: u32,
}

impl TileLayout {
    /// `None` unless tiles are 1 to [`MAX_TILE_CHUNKS`] chunks on a side and
    /// the cell size is one a map can have.
    #[must_use]
    pub fn new(size: GridSize, cell_size: f32, tile_chunks: u32) -> Option<Self> {
        let valid = (1..=MAX_TILE_CHUNKS).contains(&tile_chunks)
            && (MIN_CELL_SIZE..=MAX_CELL_SIZE).contains(&cell_size);
        valid.then_some(Self {
            size,
            cell_size,
            tile_chunks,
        })
    }

    #[must_use]
    pub const fn size(&self) -> GridSize {
        self.size
    }

    #[must_use]
    pub const fn cell_size(&self) -> f32 {
        self.cell_size
    }

    /// Chunks along a tile's side.
    #[must_use]
    pub const fn tile_chunks(&self) -> u32 {
        self.tile_chunks
    }

    /// Tiles along x and along z.
    #[must_use]
    pub const fn tile_counts(&self) -> (u32, u32) {
        (
            self.size.chunks_x().div_ceil(self.tile_chunks),
            self.size.chunks_z().div_ceil(self.tile_chunks),
        )
    }

    /// The whole map on XZ, from the origin.
    #[must_use]
    pub fn extent(&self) -> Vec2 {
        Vec2::new(self.size.cells_x() as f32, self.size.cells_z() as f32) * self.cell_size
    }

    /// A full tile's side, in metres. Tiles on the far edges may be smaller.
    #[must_use]
    pub fn tile_metres(&self) -> f32 {
        (self.tile_chunks * CHUNK_CELLS) as f32 * self.cell_size
    }

    /// Every tile, row by row.
    pub fn tiles(&self) -> impl Iterator<Item = (u32, u32)> + use<> {
        let (tiles_x, tiles_z) = self.tile_counts();
        (0..tiles_z).flat_map(move |z| (0..tiles_x).map(move |x| (x, z)))
    }

    /// A tile's chunks, row by row: the order a tile file stores them in.
    /// Empty for a tile that isn't on the map.
    pub fn chunks_of(&self, tx: u32, tz: u32) -> impl Iterator<Item = ChunkCoord> + use<> {
        let size = self.size;
        let x0 = tx.saturating_mul(self.tile_chunks).min(size.chunks_x());
        let z0 = tz.saturating_mul(self.tile_chunks).min(size.chunks_z());
        let x1 = (x0 + self.tile_chunks).min(size.chunks_x());
        let z1 = (z0 + self.tile_chunks).min(size.chunks_z());
        (z0..z1).flat_map(move |z| (x0..x1).map(move |x| ChunkCoord { x, z }))
    }

    /// The tile a chunk is in.
    #[must_use]
    pub const fn tile_of(&self, chunk: ChunkCoord) -> (u32, u32) {
        (chunk.x / self.tile_chunks, chunk.z / self.tile_chunks)
    }

    /// The tile under a world XZ: the one holding the point's cell, found
    /// with the division every ground query uses, so the two can't disagree
    /// about a point on a border. A point off the map belongs to the nearest
    /// edge tile, so nothing placed there is lost.
    #[must_use]
    pub fn tile_at(&self, xz: Vec2) -> (u32, u32) {
        let side = self.tile_chunks * CHUNK_CELLS;
        let grid = xz / self.cell_size;
        // The cast saturates, and NaN lands on cell 0.
        let cell = |g: f32, count: u32| (g.max(0.0) as u32).min(count - 1);
        (
            cell(grid.x, self.size.cells_x()) / side,
            cell(grid.y, self.size.cells_z()) / side,
        )
    }

    /// Where vertex column or row `v` is, in metres: the same product the
    /// terrain's vertices use, so rectangles meet exactly.
    fn edge(&self, v: u32) -> f32 {
        v as f32 * self.cell_size
    }

    /// A tile's rectangle on XZ, as its low and high corners. `None` for a
    /// tile that isn't on the map.
    #[must_use]
    pub fn tile_rect(&self, tx: u32, tz: u32) -> Option<(Vec2, Vec2)> {
        let (tiles_x, tiles_z) = self.tile_counts();
        if tx >= tiles_x || tz >= tiles_z {
            return None;
        }
        let side = self.tile_chunks * CHUNK_CELLS;
        let (cells_x, cells_z) = (self.size.cells_x(), self.size.cells_z());
        let low = Vec2::new(self.edge(tx * side), self.edge(tz * side));
        let high = Vec2::new(
            self.edge(((tx + 1) * side).min(cells_x)),
            self.edge(((tz + 1) * side).min(cells_z)),
        );
        Some((low, high))
    }

    /// A chunk's rectangle on XZ, as its low and high corners.
    #[must_use]
    pub fn chunk_rect(&self, chunk: ChunkCoord) -> (Vec2, Vec2) {
        let low = |c: u32| self.edge(c.saturating_mul(CHUNK_CELLS));
        let high = |c: u32| self.edge(c.saturating_add(1).saturating_mul(CHUNK_CELLS));
        (
            Vec2::new(low(chunk.x), low(chunk.z)),
            Vec2::new(high(chunk.x), high(chunk.z)),
        )
    }

    /// The tiles within `radius` of a point, nearest first: what to keep
    /// loaded around a player.
    #[must_use]
    pub fn tiles_within(&self, centre: Vec2, radius: f32) -> Vec<(u32, u32)> {
        let distance = |&(tx, tz): &(u32, u32)| {
            let (low, high) = self.tile_rect(tx, tz)?;
            Some(centre.distance(centre.clamp(low, high)))
        };
        let mut near: Vec<(f32, (u32, u32))> = self
            .tiles()
            .filter_map(|tile| Some((distance(&tile)?, tile)))
            .filter(|(d, _)| *d <= radius)
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        near.into_iter().map(|(_, tile)| tile).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 5×3 chunks at 2 m cells, in tiles of 2: 3×2 tiles, the last column
    /// and row half size.
    fn layout() -> TileLayout {
        TileLayout::new(GridSize::new(5, 3).expect("valid size"), 2.0, 2).expect("valid")
    }

    #[test]
    fn tiles_cover_every_chunk_once() {
        let l = layout();
        assert_eq!(l.tile_counts(), (3, 2));
        let mut chunks: Vec<ChunkCoord> = l.tiles().flat_map(|(x, z)| l.chunks_of(x, z)).collect();
        chunks.sort();
        let mut all: Vec<ChunkCoord> = l.size().chunks().collect();
        all.sort();
        assert_eq!(chunks, all);
        for chunk in all {
            let (tx, tz) = l.tile_of(chunk);
            assert!(l.chunks_of(tx, tz).any(|c| c == chunk));
        }
        assert_eq!(l.chunks_of(9, 9).count(), 0);
    }

    #[test]
    fn a_point_is_in_the_rectangle_of_its_tile() {
        let l = layout();
        assert_eq!(l.extent(), Vec2::new(320.0, 192.0));
        assert_eq!(l.tile_rect(0, 0), Some((Vec2::ZERO, Vec2::splat(128.0))));
        // The far tiles stop at the map's edge.
        assert_eq!(
            l.tile_rect(2, 1),
            Some((Vec2::new(256.0, 128.0), Vec2::new(320.0, 192.0)))
        );
        assert_eq!(l.tile_rect(3, 0), None);
        for i in 0..500 {
            let p = Vec2::new(
                crate::noise::hash01(i, 0, 3) * 320.0,
                crate::noise::hash01(i, 1, 3) * 192.0,
            );
            let (tx, tz) = l.tile_at(p);
            let (low, high) = l.tile_rect(tx, tz).expect("on the map");
            assert!(p.cmpge(low).all() && p.cmple(high).all(), "{p}");
        }
    }

    #[test]
    fn a_point_on_a_border_is_in_the_tile_that_holds_its_cell() {
        // Cell sizes that aren't exact in binary, where metres divided by a
        // tile's side and metres divided by a cell's can land either way.
        for cell_size in [0.7, 1.1, 1.3, 3.3, 2.0] {
            let size = GridSize::new(18, 2).expect("valid size");
            let l = TileLayout::new(size, cell_size, 3).expect("valid");
            let cells = (size.cells_x(), size.cells_z());
            for border in 1..6 {
                let x = (border * 96) as f32 * cell_size;
                // The border itself and the floats either side of it.
                for x in [x.next_down(), x, x.next_up()] {
                    let p = Vec2::new(x, 5.0);
                    let (cx, cz, _, _) =
                        crate::ground::locate(cell_size, cells, p).expect("on the map");
                    let chunk = ChunkCoord {
                        x: cx / CHUNK_CELLS,
                        z: cz / CHUNK_CELLS,
                    };
                    assert_eq!(l.tile_at(p), l.tile_of(chunk), "{cell_size}: x = {x}");
                }
            }
            // Neighbouring rectangles share their edge exactly.
            for tx in 0..5 {
                let (_, high) = l.tile_rect(tx, 0).expect("on the map");
                let (low, _) = l.tile_rect(tx + 1, 0).expect("on the map");
                assert_eq!(high.x, low.x, "{cell_size}");
            }
            assert_eq!(l.tile_rect(5, 0).expect("on the map").1, l.extent());
        }
    }

    #[test]
    fn points_off_the_map_go_to_the_nearest_edge_tile() {
        let l = layout();
        assert_eq!(l.tile_at(Vec2::new(-50.0, 500.0)), (0, 1));
        assert_eq!(l.tile_at(Vec2::new(1e9, -1e9)), (2, 0));
        assert_eq!(l.tile_at(Vec2::new(f32::NAN, 10.0)), (0, 0));
    }

    #[test]
    fn nearby_tiles_come_nearest_first() {
        let l = layout();
        // Just inside tile (1, 0), near its border with (0, 0).
        let at = Vec2::new(130.0, 60.0);
        assert_eq!(l.tiles_within(at, 0.0), vec![(1, 0)]);
        assert_eq!(l.tiles_within(at, 10.0), vec![(1, 0), (0, 0)]);
        assert_eq!(l.tiles_within(at, 1000.0).len(), 6);
        assert_eq!(l.tiles_within(at, 1000.0)[0], (1, 0));
    }

    #[test]
    fn bad_layouts_are_refused() {
        let size = GridSize::new(1, 1).expect("valid size");
        assert!(TileLayout::new(size, 2.0, 0).is_none());
        assert!(TileLayout::new(size, 2.0, 17).is_none());
        assert!(TileLayout::new(size, f32::NAN, 4).is_none());
        assert!(TileLayout::new(size, 0.0, 4).is_none());
    }
}
