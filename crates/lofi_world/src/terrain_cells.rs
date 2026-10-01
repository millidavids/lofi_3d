//! Cell flags, triangulation and vertex tint. See `cells` for the flag bits.

use std::sync::Arc;

use bevy_math::Vec2;

use crate::cells::{CellFlags, Triangulation};
use crate::chunk_data::Tint;
use crate::grid::{CHUNK_CELLS, ChunkCoord};
use crate::ground::locate;
use crate::terrain::Terrain;

impl Terrain {
    #[must_use]
    pub const fn triangulation(&self) -> Triangulation {
        self.triangulation
    }

    /// Changes how every auto cell is cut. Returns whether it changed.
    pub fn set_triangulation(&mut self, rule: Triangulation) -> bool {
        let changed = self.triangulation != rule;
        self.triangulation = rule;
        changed
    }

    #[must_use]
    pub fn cell(&self, cx: u32, cz: u32) -> CellFlags {
        let (chunk, i) = self.locate_cell(cx, cz);
        self.chunks[chunk].cells[i]
    }

    /// Returns whether the flags changed; an unchanged write copies nothing.
    pub fn set_cell(&mut self, cx: u32, cz: u32, flags: CellFlags) -> bool {
        let (chunk, i) = self.locate_cell(cx, cz);
        if self.chunks[chunk].cells[i] == flags {
            return false;
        }
        Arc::make_mut(&mut self.chunks[chunk].cells)[i] = flags;
        true
    }

    /// The chunk a cell belongs to. Cells, unlike vertices, are never shared.
    #[must_use]
    pub const fn cell_chunk(cx: u32, cz: u32) -> ChunkCoord {
        ChunkCoord {
            x: cx / CHUNK_CELLS,
            z: cz / CHUNK_CELLS,
        }
    }

    /// Whether the cell under a world XZ is a hole. Off the map is not.
    #[must_use]
    pub fn is_hole_at(&self, xz: Vec2) -> bool {
        self.cell_at(xz)
            .is_some_and(|(cx, cz)| self.cell(cx, cz).hole())
    }

    /// The cell under a world XZ; the far edges belong to the last cell.
    #[must_use]
    pub fn cell_at(&self, xz: Vec2) -> Option<(u32, u32)> {
        let cells = (self.size().cells_x(), self.size().cells_z());
        locate(self.cell_size(), cells, xz).map(|(cx, cz, _, _)| (cx, cz))
    }

    /// Whether a cell is cut along its (0, 0)–(1, 1) diagonal. The mesher,
    /// `height_at` and the raycast all ask this, so they always agree.
    #[must_use]
    pub fn splits_on_main_diagonal(&self, cx: u32, cz: u32) -> bool {
        let corners = [
            self.height(cx, cz),
            self.height(cx + 1, cz),
            self.height(cx, cz + 1),
            self.height(cx + 1, cz + 1),
        ];
        self.triangulation
            .splits_on_main(self.cell(cx, cz), (cx, cz), corners, || {
                self.laplacian(cx, cz)
                    + self.laplacian(cx + 1, cz)
                    + self.laplacian(cx, cz + 1)
                    + self.laplacian(cx + 1, cz + 1)
            })
    }

    /// The two triangles of a cell as vertex coords, counter-clockwise seen
    /// from +Y.
    #[must_use]
    pub fn cell_triangles(&self, cx: u32, cz: u32) -> [[(u32, u32); 3]; 2] {
        let a = (cx, cz);
        let b = (cx + 1, cz);
        let c = (cx, cz + 1);
        let d = (cx + 1, cz + 1);
        if self.splits_on_main_diagonal(cx, cz) {
            [[a, c, d], [a, d, b]]
        } else {
            [[a, c, b], [b, c, d]]
        }
    }

    #[must_use]
    pub fn tint(&self, x: u32, z: u32) -> Tint {
        let (chunk, i) = self.locate_vertex(x, z);
        self.chunks[chunk].tint[i]
    }

    /// Returns whether the tint changed; an unchanged write copies nothing.
    pub fn set_tint(&mut self, x: u32, z: u32, tint: Tint) -> bool {
        let (chunk, i) = self.locate_vertex(x, z);
        if self.chunks[chunk].tint[i] == tint {
            return false;
        }
        Arc::make_mut(&mut self.chunks[chunk].tint)[i] = tint;
        true
    }

    /// Moves a vertex's tint `amount` (0..1) of the way to `target`.
    /// `dither` (0..1) decides the rounding, so slow strokes still arrive
    /// instead of stalling a byte short.
    pub fn tint_toward(&mut self, x: u32, z: u32, target: Tint, amount: f32, dither: f32) -> bool {
        let current = self.tint(x, z);
        let amount = amount.clamp(0.0, 1.0);
        let next: Tint = std::array::from_fn(|i| {
            let (from, to) = (f32::from(current[i]), f32::from(target[i]));
            let moved = (from + (to - from) * amount + dither - 0.5).round();
            moved.clamp(from.min(to), from.max(to)) as u8
        });
        self.set_tint(x, z, next)
    }

    /// All cell flags in row-major order, for files.
    #[must_use]
    pub fn cells_row_major(&self) -> Vec<CellFlags> {
        let size = self.size();
        (0..size.cells_z())
            .flat_map(|cz| (0..size.cells_x()).map(move |cx| (cx, cz)))
            .map(|(cx, cz)| self.cell(cx, cz))
            .collect()
    }

    /// All tints in row-major vertex order, for files.
    #[must_use]
    pub fn tints_row_major(&self) -> Vec<Tint> {
        self.size()
            .full_rect()
            .iter()
            .map(|(x, z)| self.tint(x, z))
            .collect()
    }

    /// How hollow the ground is at a vertex, per metre: positive in dips and
    /// valleys, negative on bumps and ridges. A bowl curving with radius R
    /// reads about 2 / R at any cell size.
    #[must_use]
    pub fn curvature(&self, x: u32, z: u32) -> f32 {
        self.laplacian(x, z) / (self.cell_size() * self.cell_size())
    }

    /// Sum of the four neighbours minus four times the vertex. Negative on
    /// convex ground. At the map edge the missing side counts as level.
    /// Part of [`Triangulation::FollowTerrain`]'s definition; don't change it.
    fn laplacian(&self, x: u32, z: u32) -> f32 {
        let size = self.size();
        let h = self.height(x, z);
        let west = self.height(x.saturating_sub(1), z);
        let east = self.height((x + 1).min(size.cells_x()), z);
        let north = self.height(x, z.saturating_sub(1));
        let south = self.height(x, (z + 1).min(size.cells_z()));
        west + east + north + south - 4.0 * h
    }

    fn locate_cell(&self, cx: u32, cz: u32) -> (usize, usize) {
        // Off the grid would wrap into another chunk's cells, silently.
        assert!(
            cx < self.size().cells_x() && cz < self.size().cells_z(),
            "cell {cx}, {cz} off the grid"
        );
        let chunk = Self::cell_chunk(cx, cz);
        let local = (cz % CHUNK_CELLS) * CHUNK_CELLS + cx % CHUNK_CELLS;
        (self.size().chunk_index(chunk), local as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cells::Diagonal;
    use crate::grid::GridSize;
    use crate::materials::MaterialId;
    use crate::terrain::bumpy;

    fn two_by_two() -> Terrain {
        Terrain::flat(
            GridSize::new(2, 2).expect("valid size"),
            2.0,
            0.0,
            MaterialId(0),
        )
    }

    #[test]
    fn cells_live_in_their_own_chunk() {
        let mut t = two_by_two();
        let before = t.clone();
        let hole = CellFlags::default().with_hole(true);
        assert!(t.set_cell(32, 31, hole));
        assert!(!t.set_cell(32, 31, hole), "same flags again");
        assert_eq!(t.cell(32, 31), hole);
        assert_eq!(t.cell(31, 31), CellFlags::default());
        let touched = |c: ChunkCoord| t.chunk_data(c).unshared_bytes(before.chunk_data(c));
        assert_eq!(touched(ChunkCoord { x: 1, z: 0 }), 32 * 32);
        assert_eq!(touched(ChunkCoord { x: 0, z: 0 }), 0);
    }

    #[test]
    fn tint_round_trips_on_shared_vertices() {
        let mut t = two_by_two();
        assert!(t.set_tint(32, 32, [10, 20, 30]));
        assert_eq!(t.tint(32, 32), [10, 20, 30]);
        assert_eq!(
            t.tints_row_major()[t.size().vertex_index(32, 32)],
            [10, 20, 30]
        );
    }

    #[test]
    fn tint_toward_arrives_without_overshoot() {
        let mut t = two_by_two();
        let target = [200, 100, 128];
        // Tiny steps still get there, thanks to the dither.
        for i in 0..2000 {
            let dither = crate::noise::hash01(i, 0, 9);
            t.tint_toward(3, 3, target, 0.01, dither);
        }
        assert_eq!(t.tint(3, 3), target);
        assert!(!t.tint_toward(3, 3, target, 1.0, 0.99), "already there");
        assert!(t.tint_toward(3, 3, [128; 3], 1.0, 0.0));
        assert_eq!(t.tint(3, 3), [128; 3]);
    }

    #[test]
    fn row_major_cells_match_lookups() {
        let mut t = two_by_two();
        t.set_cell(5, 40, CellFlags(7));
        let cells = t.cells_row_major();
        assert_eq!(cells.len(), 64 * 64);
        assert_eq!(cells[40 * 64 + 5], CellFlags(7));
    }

    #[test]
    fn forced_diagonal_changes_the_triangles() {
        let mut t = bumpy();
        let auto = t.cell_triangles(3, 3);
        let flipped = if t.splits_on_main_diagonal(3, 3) {
            Diagonal::Anti
        } else {
            Diagonal::Main
        };
        t.set_cell(3, 3, CellFlags::default().with_diagonal(flipped));
        assert_ne!(t.cell_triangles(3, 3), auto);
    }

    #[test]
    #[should_panic(expected = "off the grid")]
    fn cells_off_the_grid_panic_rather_than_wrap() {
        let mut t = two_by_two();
        t.set_cell(64, 0, CellFlags::default().with_hole(true));
    }

    #[test]
    fn hole_lookup_by_position() {
        let mut t = two_by_two();
        t.set_cell(1, 2, CellFlags::default().with_hole(true));
        assert!(t.is_hole_at(Vec2::new(3.0, 5.0)));
        assert!(!t.is_hole_at(Vec2::new(1.0, 5.0)));
        assert!(!t.is_hole_at(Vec2::new(-1.0, 5.0)));
    }
}
