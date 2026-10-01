//! The editor's terrain as ground.

use super::cell::CellShape;
use super::query::Ground;
use crate::terrain::Terrain;

impl Ground for Terrain {
    fn cell_size(&self) -> f32 {
        Self::cell_size(self)
    }

    fn cell_counts(&self) -> (u32, u32) {
        (self.size().cells_x(), self.size().cells_z())
    }

    fn vertex_height(&self, x: u32, z: u32) -> Option<f32> {
        let size = self.size();
        (x <= size.cells_x() && z <= size.cells_z()).then(|| self.height(x, z))
    }

    fn cell_shape(&self, cx: u32, cz: u32) -> Option<CellShape> {
        let size = self.size();
        if cx >= size.cells_x() || cz >= size.cells_z() {
            return None;
        }
        let flags = self.cell(cx, cz);
        Some(CellShape {
            main_diagonal: self.splits_on_main_diagonal(cx, cz),
            hole: flags.hole(),
            unwalkable: flags.unwalkable(),
            no_clutter: flags.no_clutter(),
        })
    }
}
