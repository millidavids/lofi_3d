//! How vertices, cells and chunks index into each other.

/// Cells along one edge of a chunk. Chunks are the unit of meshing, and maps
/// are whole chunks so every chunk has the same shape.
pub const CHUNK_CELLS: u32 = 32;

/// Largest map edge, in chunks. Stops a corrupt file from asking for gigabytes.
pub const MAX_CHUNKS: u32 = 64;

/// Size of a terrain grid, in chunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GridSize {
    chunks_x: u32,
    chunks_z: u32,
}

impl GridSize {
    /// `None` unless both sides are in `1..=MAX_CHUNKS`.
    #[must_use]
    pub const fn new(chunks_x: u32, chunks_z: u32) -> Option<Self> {
        if chunks_x == 0 || chunks_z == 0 || chunks_x > MAX_CHUNKS || chunks_z > MAX_CHUNKS {
            return None;
        }
        Some(Self { chunks_x, chunks_z })
    }

    #[must_use]
    pub const fn chunks_x(self) -> u32 {
        self.chunks_x
    }

    #[must_use]
    pub const fn chunks_z(self) -> u32 {
        self.chunks_z
    }

    #[must_use]
    pub const fn cells_x(self) -> u32 {
        self.chunks_x * CHUNK_CELLS
    }

    #[must_use]
    pub const fn cells_z(self) -> u32 {
        self.chunks_z * CHUNK_CELLS
    }

    #[must_use]
    pub const fn verts_x(self) -> u32 {
        self.cells_x() + 1
    }

    #[must_use]
    pub const fn verts_z(self) -> u32 {
        self.cells_z() + 1
    }

    #[must_use]
    pub const fn vertex_count(self) -> usize {
        self.verts_x() as usize * self.verts_z() as usize
    }

    /// Row-major: `z` selects the row.
    #[must_use]
    pub const fn vertex_index(self, x: u32, z: u32) -> usize {
        z as usize * self.verts_x() as usize + x as usize
    }

    #[must_use]
    pub const fn chunk_count(self) -> usize {
        self.chunks_x as usize * self.chunks_z as usize
    }

    #[must_use]
    pub const fn chunk_index(self, chunk: ChunkCoord) -> usize {
        chunk.z as usize * self.chunks_x as usize + chunk.x as usize
    }

    #[must_use]
    pub const fn full_rect(self) -> VertexRect {
        VertexRect {
            min_x: 0,
            min_z: 0,
            max_x: self.cells_x(),
            max_z: self.cells_z(),
        }
    }

    pub fn chunks(self) -> impl Iterator<Item = ChunkCoord> {
        (0..self.chunks_z).flat_map(move |z| (0..self.chunks_x).map(move |x| ChunkCoord { x, z }))
    }

    /// Every chunk whose mesh uses a vertex in `rect`. Border vertices belong
    /// to the chunks on both sides of the border.
    pub fn chunks_touching(self, rect: VertexRect) -> impl Iterator<Item = ChunkCoord> {
        let min_x = rect.min_x.saturating_sub(1) / CHUNK_CELLS;
        let min_z = rect.min_z.saturating_sub(1) / CHUNK_CELLS;
        let max_x = (rect.max_x / CHUNK_CELLS).min(self.chunks_x - 1);
        let max_z = (rect.max_z / CHUNK_CELLS).min(self.chunks_z - 1);
        (min_z..=max_z).flat_map(move |z| (min_x..=max_x).map(move |x| ChunkCoord { x, z }))
    }

    /// The chunk that stores vertex `(x, z)`, and where inside it.
    ///
    /// A chunk stores the vertices on its west and north edges; the last
    /// chunk in each direction also stores the closing edge. Every vertex
    /// has exactly one owner, so chunks can't disagree about a border.
    #[must_use]
    pub const fn owner(self, x: u32, z: u32) -> (ChunkCoord, u32, u32) {
        // Off the grid would land in some other chunk's storage, silently.
        assert!(
            x <= self.chunks_x * CHUNK_CELLS && z <= self.chunks_z * CHUNK_CELLS,
            "vertex off the grid"
        );
        let cx = last_or(x / CHUNK_CELLS, self.chunks_x - 1);
        let cz = last_or(z / CHUNK_CELLS, self.chunks_z - 1);
        (
            ChunkCoord { x: cx, z: cz },
            x - cx * CHUNK_CELLS,
            z - cz * CHUNK_CELLS,
        )
    }

    /// The vertices `chunk` stores.
    #[must_use]
    pub const fn owned_rect(self, chunk: ChunkCoord) -> VertexRect {
        let min_x = chunk.x * CHUNK_CELLS;
        let min_z = chunk.z * CHUNK_CELLS;
        let closes_x = chunk.x + 1 == self.chunks_x;
        let closes_z = chunk.z + 1 == self.chunks_z;
        VertexRect {
            min_x,
            min_z,
            max_x: min_x + CHUNK_CELLS - if closes_x { 0 } else { 1 },
            max_z: min_z + CHUNK_CELLS - if closes_z { 0 } else { 1 },
        }
    }

    /// Every chunk that stores a vertex in `rect`.
    pub fn chunks_owning(self, rect: VertexRect) -> impl Iterator<Item = ChunkCoord> {
        let (lo, _, _) = self.owner(rect.min_x, rect.min_z);
        let (hi, _, _) = self.owner(rect.max_x, rect.max_z);
        (lo.z..=hi.z).flat_map(move |z| (lo.x..=hi.x).map(move |x| ChunkCoord { x, z }))
    }
}

const fn last_or(index: u32, last: u32) -> u32 {
    if index < last { index } else { last }
}

/// A chunk's position in the chunk grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkCoord {
    pub x: u32,
    pub z: u32,
}

impl ChunkCoord {
    /// The chunk's vertices, including the borders it shares with neighbours.
    #[must_use]
    pub const fn vertex_rect(self) -> VertexRect {
        VertexRect {
            min_x: self.x * CHUNK_CELLS,
            min_z: self.z * CHUNK_CELLS,
            max_x: (self.x + 1) * CHUNK_CELLS,
            max_z: (self.z + 1) * CHUNK_CELLS,
        }
    }
}

/// Inclusive rectangle of vertex coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexRect {
    pub min_x: u32,
    pub min_z: u32,
    pub max_x: u32,
    pub max_z: u32,
}

impl VertexRect {
    #[must_use]
    pub const fn width(self) -> u32 {
        self.max_x - self.min_x + 1
    }

    #[must_use]
    pub const fn depth(self) -> u32 {
        self.max_z - self.min_z + 1
    }

    #[must_use]
    pub const fn vertex_count(self) -> usize {
        self.width() as usize * self.depth() as usize
    }

    #[must_use]
    pub const fn contains(self, x: u32, z: u32) -> bool {
        x >= self.min_x && x <= self.max_x && z >= self.min_z && z <= self.max_z
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_z: self.min_z.min(other.min_z),
            max_x: self.max_x.max(other.max_x),
            max_z: self.max_z.max(other.max_z),
        }
    }

    /// Grows by `by` vertices on every side, clamped to the grid.
    #[must_use]
    pub fn grow(self, by: u32, size: GridSize) -> Self {
        Self {
            min_x: self.min_x.saturating_sub(by),
            min_z: self.min_z.saturating_sub(by),
            max_x: (self.max_x + by).min(size.cells_x()),
            max_z: (self.max_z + by).min(size.cells_z()),
        }
    }

    /// Row-major, the order chunks and files store samples in.
    pub fn iter(self) -> impl Iterator<Item = (u32, u32)> {
        (self.min_z..=self.max_z).flat_map(move |z| (self.min_x..=self.max_x).map(move |x| (x, z)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_out_of_range_sizes() {
        assert!(GridSize::new(0, 4).is_none());
        assert!(GridSize::new(4, MAX_CHUNKS + 1).is_none());
        assert!(GridSize::new(MAX_CHUNKS, 1).is_some());
    }

    #[test]
    fn border_vertices_touch_both_chunks() {
        let size = GridSize::new(3, 3).expect("valid size");
        let border = VertexRect {
            min_x: 32,
            min_z: 5,
            max_x: 32,
            max_z: 5,
        };
        let touched: Vec<_> = size.chunks_touching(border).collect();
        assert_eq!(
            touched,
            vec![ChunkCoord { x: 0, z: 0 }, ChunkCoord { x: 1, z: 0 }]
        );
    }

    #[test]
    fn interior_vertices_touch_one_chunk() {
        let size = GridSize::new(3, 3).expect("valid size");
        let inside = VertexRect {
            min_x: 33,
            min_z: 40,
            max_x: 63,
            max_z: 63,
        };
        let touched: Vec<_> = size.chunks_touching(inside).collect();
        assert_eq!(touched, vec![ChunkCoord { x: 1, z: 1 }]);
    }

    #[test]
    fn far_edge_clamps_to_last_chunk() {
        let size = GridSize::new(2, 2).expect("valid size");
        let touched: Vec<_> = size.chunks_touching(size.full_rect()).collect();
        assert_eq!(touched.len(), 4);
    }

    #[test]
    fn every_vertex_has_exactly_one_owner() {
        let size = GridSize::new(3, 2).expect("valid size");
        let mut owners = vec![0u32; size.vertex_count()];
        for chunk in size.chunks() {
            for (x, z) in size.owned_rect(chunk).iter() {
                owners[size.vertex_index(x, z)] += 1;
                let (owner, lx, lz) = size.owner(x, z);
                assert_eq!(owner, chunk);
                assert_eq!(
                    (lx, lz),
                    (x - chunk.x * CHUNK_CELLS, z - chunk.z * CHUNK_CELLS)
                );
            }
        }
        assert!(owners.iter().all(|&n| n == 1));
    }

    #[test]
    fn owners_of_a_border_rect() {
        let size = GridSize::new(3, 3).expect("valid size");
        let rect = VertexRect {
            min_x: 31,
            min_z: 64,
            max_x: 32,
            max_z: 96,
        };
        let owners: Vec<_> = size.chunks_owning(rect).collect();
        assert_eq!(
            owners,
            vec![ChunkCoord { x: 0, z: 2 }, ChunkCoord { x: 1, z: 2 }]
        );
    }

    #[test]
    fn grow_clamps_to_grid() {
        let size = GridSize::new(1, 1).expect("valid size");
        let rect = VertexRect {
            min_x: 0,
            min_z: 1,
            max_x: 32,
            max_z: 31,
        };
        assert_eq!(rect.grow(2, size), size.full_rect());
    }
}
