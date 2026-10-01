//! One cell of ground, and the only place its surface is interpolated.

use bevy_math::Vec2;

/// What a cell's flags mean to collision and placement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellShape {
    /// Cut along (0, 0)–(1, 1) rather than (1, 0)–(0, 1).
    pub main_diagonal: bool,
    pub hole: bool,
    pub unwalkable: bool,
    pub no_clutter: bool,
}

/// A cell's corner heights and how it's cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundCell {
    /// Heights at (x, z), (x+1, z), (x, z+1), (x+1, z+1).
    pub corners: [f32; 4],
    pub shape: CellShape,
}

/// The triangle under a point of a cell, as a plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Plane {
    pub height: f32,
    /// Rise across one whole cell along x, and along z.
    pub rise_x: f32,
    pub rise_z: f32,
}

/// The surface at `(u, v)` in `0..=1` across a cell. The editor's
/// `height_at`, cooked tiles and every [`super::Ground`] query come through
/// here, so they can't disagree about which triangle a point is on.
pub(crate) fn cell_plane(corners: [f32; 4], main_diagonal: bool, u: f32, v: f32) -> Plane {
    let [a, b, c, d] = corners;
    let (height, rise_x, rise_z) = if main_diagonal {
        if v >= u {
            (a + u * (d - c) + v * (c - a), d - c, c - a)
        } else {
            (a + u * (b - a) + v * (d - b), b - a, d - b)
        }
    } else if u + v <= 1.0 {
        (a + u * (b - a) + v * (c - a), b - a, c - a)
    } else {
        (d + (1.0 - u) * (c - d) + (1.0 - v) * (b - d), d - c, d - b)
    };
    Plane {
        height,
        rise_x,
        rise_z,
    }
}

/// The cell under a world XZ and where in it, `(cx, cz, u, v)`. The far
/// edges belong to the last cell. `None` off the map.
pub(crate) fn locate(cell_size: f32, cells: (u32, u32), xz: Vec2) -> Option<(u32, u32, f32, f32)> {
    let grid = xz / cell_size;
    let (cells_x, cells_z) = cells;
    if cells_x == 0
        || cells_z == 0
        || !(0.0..=cells_x as f32).contains(&grid.x)
        || !(0.0..=cells_z as f32).contains(&grid.y)
    {
        return None;
    }
    let cx = (grid.x as u32).min(cells_x - 1);
    let cz = (grid.y as u32).min(cells_z - 1);
    Some((cx, cz, grid.x - cx as f32, grid.y - cz as f32))
}
