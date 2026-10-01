//! Turns a chunk of terrain into engine-neutral vertex buffers.
//!
//! One indexed vertex per grid point. Faceted shading, if wanted, is the
//! renderer's job (a face normal from screen-space derivatives), so the mesh
//! never duplicates vertices for it.

use crate::chunk_data::Tint;
use crate::grid::ChunkCoord;
use crate::materials::MaterialId;
use crate::terrain::Terrain;

/// Vertex buffers for one chunk, positioned relative to [`Terrain::chunk_origin`].
#[derive(Clone, Debug, Default)]
pub struct ChunkMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Paint weights per vertex, in `slots` order.
    pub weights: Vec<[u8; 4]>,
    /// The materials this chunk blends; the same for every vertex.
    pub slots: [Option<MaterialId>; 4],
    pub tints: Vec<Tint>,
    /// Hole cells have no triangles. Ambient occlusion is separate
    /// ([`crate::chunk_ao`]): it costs more and changes less.
    pub indices: Vec<u32>,
}

#[must_use]
pub fn build_chunk_mesh(terrain: &Terrain, chunk: ChunkCoord) -> ChunkMesh {
    let rect = chunk.vertex_rect();
    let origin = terrain.chunk_origin(chunk);
    let paint = terrain.chunk_paint(chunk);
    let count = rect.vertex_count();
    let mut mesh = ChunkMesh {
        positions: Vec::with_capacity(count),
        normals: Vec::with_capacity(count),
        weights: paint.weights().to_vec(),
        slots: paint.slots(),
        tints: Vec::with_capacity(count),
        indices: Vec::with_capacity((rect.width() - 1) as usize * (rect.depth() - 1) as usize * 6),
    };

    for (x, z) in rect.iter() {
        mesh.positions
            .push((terrain.vertex_position(x, z) - origin).to_array());
        mesh.normals.push(terrain.normal(x, z).to_array());
        mesh.tints.push(terrain.tint(x, z));
    }

    let local = |(x, z): (u32, u32)| (z - rect.min_z) * rect.width() + (x - rect.min_x);
    for cz in rect.min_z..rect.max_z {
        for cx in rect.min_x..rect.max_x {
            if terrain.cell(cx, cz).hole() {
                continue;
            }
            for tri in terrain.cell_triangles(cx, cz) {
                mesh.indices.extend(tri.map(local));
            }
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use bevy_math::Vec3;

    use super::*;
    use crate::grid::{CHUNK_CELLS, GridSize};
    use crate::paint::VertexPaint;

    const GRASS: MaterialId = MaterialId(0);

    #[test]
    fn triangles_face_up() {
        let t = Terrain::flat(GridSize::new(1, 1).expect("valid size"), 2.0, 0.0, GRASS);
        let mesh = build_chunk_mesh(&t, ChunkCoord { x: 0, z: 0 });
        for tri in mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(mesh.positions[tri[i] as usize]));
            assert!((b - a).cross(c - a).y > 0.0, "triangle faces down");
        }
    }

    #[test]
    fn vertex_counts() {
        let t = Terrain::flat(GridSize::new(2, 1).expect("valid size"), 2.0, 0.0, GRASS);
        let mesh = build_chunk_mesh(&t, ChunkCoord { x: 1, z: 0 });
        let verts = ((CHUNK_CELLS + 1) * (CHUNK_CELLS + 1)) as usize;
        assert_eq!(mesh.positions.len(), verts);
        assert_eq!(mesh.weights.len(), verts);
        assert_eq!(mesh.indices.len(), (CHUNK_CELLS * CHUNK_CELLS) as usize * 6);
    }

    #[test]
    fn positions_are_chunk_local() {
        let t = Terrain::flat(GridSize::new(2, 2).expect("valid size"), 2.0, 3.0, GRASS);
        let mesh = build_chunk_mesh(&t, ChunkCoord { x: 1, z: 1 });
        assert_eq!(mesh.positions[0], [0.0, 3.0, 0.0]);
    }

    #[test]
    fn holes_drop_their_triangles() {
        let mut t = Terrain::flat(GridSize::new(1, 1).expect("valid size"), 2.0, 0.0, GRASS);
        t.set_cell(4, 9, crate::cells::CellFlags::default().with_hole(true));
        let mesh = build_chunk_mesh(&t, ChunkCoord { x: 0, z: 0 });
        assert_eq!(
            mesh.indices.len(),
            (CHUNK_CELLS * CHUNK_CELLS - 1) as usize * 6
        );
    }

    #[test]
    fn tint_reaches_the_mesh() {
        let mut t = Terrain::flat(GridSize::new(1, 1).expect("valid size"), 2.0, 0.0, GRASS);
        t.set_tint(1, 0, [1, 2, 3]);
        let mesh = build_chunk_mesh(&t, ChunkCoord { x: 0, z: 0 });
        assert_eq!(mesh.tints[1], [1, 2, 3]);
    }

    #[test]
    fn paint_reaches_the_mesh() {
        let mut t = Terrain::flat(GridSize::new(1, 1).expect("valid size"), 2.0, 0.0, GRASS);
        t.set_vertex_paint(2, 0, VertexPaint::solid(MaterialId(3)))
            .expect("room");
        let mesh = build_chunk_mesh(&t, ChunkCoord { x: 0, z: 0 });
        assert_eq!(mesh.slots, [Some(GRASS), Some(MaterialId(3)), None, None]);
        assert_eq!(mesh.weights[2], [0, 255, 0, 0]);
        assert_eq!(mesh.weights[0], [255, 0, 0, 0]);
    }
}
