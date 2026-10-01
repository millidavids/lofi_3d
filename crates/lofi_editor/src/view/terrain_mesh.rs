//! Packs a chunk's vertex data into the mesh attributes the terrain shader reads.
//!
//! Only standard attributes, so Bevy's own shadow pass still draws the mesh:
//! - `COLOR`: the four slot weights, 0..1
//! - `UV_0`: the chunk's four material layers, two per float (a + b × 256)
//! - `UV_1`: tint and ambient occlusion bytes, two per float
//!   (r + g × 256, b + ao × 256)
//!
//! Integers below 2^16 are exact in `f32`, and the vertex shader unpacks them
//! before anything is interpolated.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use lofi_world::{ChunkMesh, MaterialTable};

/// 255 means open sky.
const NO_OCCLUSION: u8 = 255;

/// `ao` is the chunk's baked AO, one byte per vertex; anything else reads as
/// open sky.
pub fn terrain_mesh(data: ChunkMesh, ao: &[u8], materials: &MaterialTable) -> Mesh {
    let layer = |slot: Option<lofi_world::MaterialId>| {
        slot.and_then(|m| materials.index_of(m)).unwrap_or(0) as f32
    };
    let [a, b, c, d] = data.slots.map(layer);
    let count = data.positions.len();
    let layers = vec![[a + b * 256.0, c + d * 256.0]; count];
    let weights: Vec<[f32; 4]> = data
        .weights
        .iter()
        .map(|w| w.map(|v| f32::from(v) / 255.0))
        .collect();
    let ao_at = |i: usize| {
        if ao.len() == count {
            ao[i]
        } else {
            NO_OCCLUSION
        }
    };
    let pair = |lo: u8, hi: u8| f32::from(lo) + f32::from(hi) * 256.0;
    let tint_ao: Vec<[f32; 2]> = data
        .tints
        .iter()
        .enumerate()
        .map(|(i, &[r, g, b])| [pair(r, g), pair(b, ao_at(i))])
        .collect();

    // Render-world only: Bevy keeps the bounds it needs for culling.
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, data.positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, layers)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, tint_ao)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, weights)
    .with_inserted_indices(Indices::U32(data.indices))
}

#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;
    use lofi_world::{ChunkCoord, GridSize, MaterialId, VertexPaint, WorldMap, build_chunk_mesh};

    use super::*;

    #[test]
    fn layers_are_packed_by_table_position() {
        let mut map = WorldMap::flat("t", GridSize::new(1, 1).expect("valid size"), 2.0, 0.0);
        map.terrain
            .set_vertex_paint(3, 3, VertexPaint::solid(MaterialId(4)))
            .expect("room");
        let chunk = build_chunk_mesh(&map.terrain, ChunkCoord { x: 0, z: 0 });
        let mesh = terrain_mesh(chunk, &[], &map.materials);
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("uv0 present");
        };
        // Slots are grass (layer 0) and snow (layer 4).
        assert_eq!(uv[0], [4.0 * 256.0, 0.0]);
    }

    #[test]
    fn tint_and_ao_are_packed_in_pairs() {
        let mut map = WorldMap::flat("t", GridSize::new(1, 1).expect("valid size"), 2.0, 0.0);
        map.terrain.set_tint(1, 0, [10, 20, 30]);
        let chunk = ChunkCoord { x: 0, z: 0 };
        let mut ao = vec![255; 33 * 33];
        ao[1] = 40;
        let mesh = terrain_mesh(build_chunk_mesh(&map.terrain, chunk), &ao, &map.materials);
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("uv1 present");
        };
        assert_eq!(uv[1], [10.0 + 20.0 * 256.0, 30.0 + 40.0 * 256.0]);
        assert_eq!(uv[0], [128.0 + 128.0 * 256.0, 128.0 + 255.0 * 256.0]);
    }
}
