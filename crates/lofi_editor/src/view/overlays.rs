//! Overlays drawn as lines on the terrain: chunks that can't take another
//! material, and flagged cells. Slope and the chunk grid are in the terrain
//! shader.

use bevy::prelude::*;
use lofi_world::{CHUNK_CELLS, ChunkCoord, SLOTS, Terrain};

use super::settings::ViewSettings;
use crate::camera::EditorCamera;
use crate::document::{EditHistory, MapDocument};

/// Outlines drawn at most, nearest the camera first.
const MAX_CHUNK_OUTLINES: usize = 64;
/// Flagged cells are looked for this many cells from the point under the
/// camera: a bounded scan whatever the cell size.
const CELL_RADIUS_CELLS: u32 = 64;
const MAX_CELL_OUTLINES: usize = 4_000;
/// Lift so lines don't dip under the surface between samples.
const LIFT: f32 = 0.2;

const FULL_COLOR: Color = Color::srgb(1.0, 0.55, 0.1);
const UNWALKABLE_COLOR: Color = Color::srgb(0.95, 0.2, 0.15);
const NO_CLUTTER_COLOR: Color = Color::srgb(0.4, 0.75, 1.0);

/// Chunks whose four material slots are all taken, recounted when the map
/// changes, but not mid-stroke: a stroke changes the map every frame.
#[derive(Default)]
pub(super) struct FullChunks {
    seen: Option<(u64, u64)>,
    chunks: Vec<ChunkCoord>,
}

pub(super) fn draw_full_chunks(
    settings: Res<ViewSettings>,
    document: Res<MapDocument>,
    history: Res<EditHistory>,
    camera: Single<&Transform, With<EditorCamera>>,
    mut full: Local<FullChunks>,
    mut gizmos: Gizmos,
) {
    if !settings.overlays.full_chunks {
        return;
    }
    let key = (document.generation(), document.revision());
    let same_map = full.seen.is_some_and(|(generation, _)| generation == key.0);
    if full.seen != Some(key) && !(history.is_recording() && same_map) {
        full.chunks = full_chunks(document.terrain());
        full.seen = Some(key);
    }
    let terrain = document.terrain();
    let eye = camera.translation.xz();
    let mut nearest: Vec<(f32, ChunkCoord)> = full
        .chunks
        .iter()
        .map(|&c| (chunk_centre(terrain, c).distance_squared(eye), c))
        .collect();
    nearest.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, chunk) in nearest.into_iter().take(MAX_CHUNK_OUTLINES) {
        let rect = chunk.vertex_rect();
        // Every fourth vertex is close enough to follow the ground.
        let step = 4;
        let mut ring = Vec::new();
        ring.extend(
            (rect.min_x..rect.max_x)
                .step_by(step)
                .map(|x| (x, rect.min_z)),
        );
        ring.extend(
            (rect.min_z..rect.max_z)
                .step_by(step)
                .map(|z| (rect.max_x, z)),
        );
        ring.extend(
            (rect.min_x + 1..=rect.max_x)
                .rev()
                .step_by(step)
                .map(|x| (x, rect.max_z)),
        );
        ring.extend(
            (rect.min_z + 1..=rect.max_z)
                .rev()
                .step_by(step)
                .map(|z| (rect.min_x, z)),
        );
        ring.push((rect.min_x, rect.min_z));
        gizmos.linestrip(
            ring.into_iter()
                .map(|(x, z)| terrain.vertex_position(x, z) + Vec3::Y * LIFT),
            FULL_COLOR,
        );
    }
}

/// Chunks using all four slots.
fn full_chunks(terrain: &Terrain) -> Vec<ChunkCoord> {
    terrain
        .size()
        .chunks()
        .filter(|&c| terrain.chunk_paint(c).materials().count() == SLOTS)
        .collect()
}

fn chunk_centre(terrain: &Terrain, chunk: ChunkCoord) -> Vec2 {
    let half = CHUNK_CELLS as f32 * terrain.cell_size() * 0.5;
    terrain.chunk_origin(chunk).xz() + Vec2::splat(half)
}

/// Outlines unwalkable cells in red and no-clutter cells in blue, around the
/// point below the camera.
pub(super) fn draw_cell_flags(
    settings: Res<ViewSettings>,
    document: Res<MapDocument>,
    camera: Single<&Transform, With<EditorCamera>>,
    mut gizmos: Gizmos,
) {
    if !settings.overlays.cell_flags {
        return;
    }
    let terrain = document.terrain();
    let radius = CELL_RADIUS_CELLS as f32 * terrain.cell_size();
    let Some(rect) = terrain.vertices_near(camera.translation.xz(), radius) else {
        return;
    };
    let mut drawn = 0;
    for cz in rect.min_z..rect.max_z {
        for cx in rect.min_x..rect.max_x {
            let flags = terrain.cell(cx, cz);
            let color = if flags.unwalkable() {
                UNWALKABLE_COLOR
            } else if flags.no_clutter() {
                NO_CLUTTER_COLOR
            } else {
                continue;
            };
            let corner = |x: u32, z: u32| terrain.vertex_position(x, z) + Vec3::Y * LIFT;
            gizmos.linestrip(
                [
                    corner(cx, cz),
                    corner(cx + 1, cz),
                    corner(cx + 1, cz + 1),
                    corner(cx, cz + 1),
                    corner(cx, cz),
                ],
                color,
            );
            drawn += 1;
            if drawn >= MAX_CELL_OUTLINES {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use lofi_world::{GridSize, MaterialId, VertexPaint};

    use super::*;

    #[test]
    fn finds_chunks_with_every_slot_taken() {
        let size = GridSize::new(2, 1).expect("valid size");
        let mut t = Terrain::flat(size, 2.0, 0.0, MaterialId(0));
        for (x, m) in [(40, 1), (41, 2), (42, 3)] {
            t.set_vertex_paint(x, 5, VertexPaint::solid(MaterialId(m)))
                .expect("room");
        }
        assert_eq!(full_chunks(&t), vec![ChunkCoord { x: 1, z: 0 }]);
    }
}
