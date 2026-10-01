//! Baked ambient occlusion: how much sky each vertex sees.
//!
//! Depends on heights only, never saved, rebuilt on load. Edits reach
//! [`AO_RADIUS_CELLS`] beyond what they change.

use bevy_math::Vec2;

use crate::grid::ChunkCoord;
use crate::terrain::Terrain;

/// How far, in cells, a vertex looks for occluders. In cells rather than
/// metres so the cost doesn't depend on cell size: 16 m at the default 2 m.
pub const AO_RADIUS_CELLS: u32 = 8;

/// Distances sampled along each direction, in steps.
const STEPS: [u32; 6] = [1, 2, 3, 4, 6, AO_RADIUS_CELLS];

/// The eight compass directions as vertex offsets.
const DIRECTIONS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// Sky visibility for each vertex of a chunk's mesh, row-major over
/// [`ChunkCoord::vertex_rect`]. 255 is open sky.
#[must_use]
pub fn chunk_ao(terrain: &Terrain, chunk: ChunkCoord) -> Vec<u8> {
    chunk
        .vertex_rect()
        .iter()
        .map(|(x, z)| vertex_ao(terrain, x, z))
        .collect()
}

/// For each direction, the highest horizon above the surface's own tangent
/// plane; the vertex sees `cos²` of that much sky, as a surface does past a
/// uniform horizon. A plain slope with nothing on it is open sky, so slopes
/// aren't darkened twice: hemisphere ambient already shades them.
#[must_use]
pub fn vertex_ao(terrain: &Terrain, x: u32, z: u32) -> u8 {
    let size = terrain.size();
    let (max_x, max_z) = (size.cells_x() as i32, size.cells_z() as i32);
    let here = terrain.height(x, z);
    let normal = terrain.normal(x, z);
    let mut visible = 0.0;
    for (dx, dz) in DIRECTIONS {
        let step = Vec2::new(dx as f32, dz as f32) * terrain.cell_size();
        // The tangent plane's rise per metre along this direction.
        let along = step.normalize();
        let tangent = -(normal.x * along.x + normal.z * along.y) / normal.y;
        let mut horizon = f32::NEG_INFINITY;
        for s in STEPS {
            let (sx, sz) = (x as i32 + dx * s as i32, z as i32 + dz * s as i32);
            if !(0..=max_x).contains(&sx) || !(0..=max_z).contains(&sz) {
                break;
            }
            let rise = terrain.height(sx as u32, sz as u32) - here;
            horizon = horizon.max(rise / (step.length() * s as f32));
        }
        visible += visibility(horizon, tangent);
    }
    (visible / DIRECTIONS.len() as f32 * 255.0).round() as u8
}

/// `cos²` of the angle between the horizon and the tangent, both given as
/// slopes, without trigonometry: `tan(h − t)` by the difference formula,
/// then `cos² = 1 / (1 + tan²)`.
fn visibility(horizon: f32, tangent: f32) -> f32 {
    if horizon <= tangent {
        return 1.0;
    }
    let across = 1.0 + horizon * tangent;
    if across <= 0.0 {
        // More than a right angle above the tangent: this side is walled off.
        return 0.0;
    }
    let tan = (horizon - tangent) / across;
    1.0 / (1.0 + tan * tan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::GridSize;
    use crate::materials::MaterialId;

    fn flat() -> Terrain {
        Terrain::flat(
            GridSize::new(1, 1).expect("valid size"),
            2.0,
            0.0,
            MaterialId(0),
        )
    }

    #[test]
    fn flat_ground_sees_all_the_sky() {
        let t = flat();
        assert!(
            chunk_ao(&t, ChunkCoord { x: 0, z: 0 })
                .iter()
                .all(|&a| a == 255)
        );
    }

    #[test]
    fn a_plain_slope_sees_all_the_sky() {
        let mut t = flat();
        for (x, z) in t.size().full_rect().iter() {
            t.set_height(x, z, x as f32 * 1.5);
        }
        assert_eq!(vertex_ao(&t, 16, 16), 255);
    }

    #[test]
    fn a_pit_floor_is_darker_than_its_rim() {
        let mut t = flat();
        for (x, z) in t.size().full_rect().iter() {
            let d = (x as f32 - 16.0).abs().max((z as f32 - 16.0).abs());
            t.set_height(x, z, if d <= 3.0 { -6.0 } else { 0.0 });
        }
        let floor = vertex_ao(&t, 16, 16);
        let rim = vertex_ao(&t, 16, 24);
        assert!(floor < 200, "floor {floor}");
        assert!(rim > floor, "rim {rim} vs floor {floor}");
    }

    #[test]
    fn walls_past_vertical_block_everything() {
        assert_eq!(visibility(10.0, -10.0), 0.0);
        assert!((visibility(1.0, 0.0) - 0.5).abs() < 1e-6, "45° above level");
        assert_eq!(visibility(-1.0, 0.0), 1.0);
    }
}
