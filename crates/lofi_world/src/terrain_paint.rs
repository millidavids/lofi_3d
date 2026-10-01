//! Painting the terrain. See `paint` for how chunks store it.

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::cells::Triangulation;
use crate::grid::{ChunkCoord, GridSize, VertexRect};
use crate::layers::{LayerWeights, normalize_weights};
use crate::materials::MaterialId;
use crate::paint::{ChunkPaint, VertexPaint};
use crate::terrain::{Terrain, TerrainParts};

impl Terrain {
    #[must_use]
    pub fn chunk_paint(&self, chunk: ChunkCoord) -> &ChunkPaint {
        &self.chunk_data(chunk).paint
    }

    #[must_use]
    pub fn paint_at(&self, x: u32, z: u32) -> VertexPaint {
        let (chunk, _, _) = self.size().owner(x, z);
        let rect = chunk.vertex_rect();
        self.chunk_paint(chunk).at(x - rect.min_x, z - rect.min_z)
    }

    /// Writes a vertex's paint into every chunk that shows it. Changes nothing
    /// and returns the chunk at fault if one has no slot left for a material.
    pub fn set_vertex_paint(
        &mut self,
        x: u32,
        z: u32,
        paint: VertexPaint,
    ) -> Result<bool, ChunkCoord> {
        let size = self.size();
        let here = VertexRect {
            min_x: x,
            min_z: z,
            max_x: x,
            max_z: z,
        };
        let local = |chunk: ChunkCoord| {
            let rect = chunk.vertex_rect();
            (x - rect.min_x, z - rect.min_z)
        };
        for chunk in size.chunks_touching(here) {
            let (lx, lz) = local(chunk);
            if !self.chunk_paint(chunk).can_fit(lx, lz, &paint) {
                return Err(chunk);
            }
        }
        let mut changed = false;
        for chunk in size.chunks_touching(here) {
            let (lx, lz) = local(chunk);
            let i = size.chunk_index(chunk);
            if self.chunks[i].paint.at(lx, lz) != paint {
                changed |= Arc::make_mut(&mut self.chunks[i].paint).set(lx, lz, &paint);
            }
        }
        Ok(changed)
    }

    /// Moves a vertex toward `material` by `amount`; see [`VertexPaint::toward`].
    pub fn paint_vertex(
        &mut self,
        x: u32,
        z: u32,
        material: MaterialId,
        amount: f32,
        dither: f32,
    ) -> Result<bool, ChunkCoord> {
        let current = self.paint_at(x, z);
        let Some(next) = current.toward(material, amount, dither) else {
            return Err(self.size().owner(x, z).0);
        };
        if next == current {
            return Ok(false);
        }
        self.set_vertex_paint(x, z, next)
    }

    /// Repaints everything painted `from` as `to`, e.g. before deleting
    /// `from`. Never fails: `from`'s slot is reused where `to` has none.
    pub fn replace_material(&mut self, from: MaterialId, to: MaterialId) -> bool {
        let mut changed = false;
        for chunk in &mut self.chunks {
            if chunk.paint.slots().contains(&Some(from)) {
                changed |= Arc::make_mut(&mut chunk.paint).replace(from, to);
            }
        }
        changed
    }

    #[must_use]
    pub fn materials_in_use(&self) -> BTreeSet<MaterialId> {
        self.chunks
            .iter()
            .flat_map(|chunk| chunk.paint.materials().collect::<Vec<_>>())
            .collect()
    }

    /// Where a shared vertex's copies differ, copies the owner's paint over
    /// the others. Files this crate writes always agree; this is for the rest.
    ///
    /// # Errors
    /// If a copy can't take the owner's paint because its chunk lacks the
    /// materials. Left alone it would show a seam and refuse every later
    /// stroke there.
    pub(crate) fn make_shared_paint_agree(&mut self) -> Result<(), String> {
        let size = self.size();
        for chunk in size.chunks() {
            let rect = chunk.vertex_rect();
            let owned = size.owned_rect(chunk);
            let i = size.chunk_index(chunk);
            for (x, z) in rect.iter().filter(|&(x, z)| !owned.contains(x, z)) {
                let truth = self.paint_at(x, z);
                let (lx, lz) = (x - rect.min_x, z - rect.min_z);
                let paint = &self.chunks[i].paint;
                if paint.at(lx, lz) == truth {
                    continue;
                }
                if !paint.can_fit(lx, lz, &truth) {
                    return Err(format!(
                        "vertex {x}, {z} is painted with materials chunk {}, {} doesn't have",
                        chunk.x, chunk.z
                    ));
                }
                Arc::make_mut(&mut self.chunks[i].paint).set(lx, lz, &truth);
            }
        }
        Ok(())
    }

    /// Builds from the pre-v3 model: four map-wide layers, weights per vertex,
    /// checkerboard cells.
    pub(crate) fn from_legacy(
        size: GridSize,
        cell_size: f32,
        heights: &[f32],
        weights: &[LayerWeights],
        layers: [MaterialId; 4],
    ) -> Result<Self, String> {
        if weights.len() != size.vertex_count() {
            return Err(format!(
                "expected {} weights, found {}",
                size.vertex_count(),
                weights.len()
            ));
        }
        let paints = size
            .chunks()
            .map(|chunk| {
                let w = chunk
                    .vertex_rect()
                    .iter()
                    .map(|(x, z)| normalize_weights(weights[size.vertex_index(x, z)]))
                    .collect();
                ChunkPaint::from_parts(layers.map(Some), w)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_parts(
            size,
            cell_size,
            TerrainParts {
                heights,
                paints,
                cells: None,
                tints: None,
                triangulation: Triangulation::Checkerboard,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRASS: MaterialId = MaterialId(0);
    const DIRT: MaterialId = MaterialId(1);

    fn terrain() -> Terrain {
        Terrain::flat(GridSize::new(2, 2).expect("valid size"), 2.0, 0.0, GRASS)
    }

    #[test]
    fn shared_vertices_are_written_everywhere() {
        let mut t = terrain();
        assert_eq!(t.paint_vertex(32, 32, DIRT, 1.0, 0.5), Ok(true));
        for (cx, cz, lx, lz) in [(0, 0, 32, 32), (1, 0, 0, 32), (0, 1, 32, 0), (1, 1, 0, 0)] {
            let paint = t.chunk_paint(ChunkCoord { x: cx, z: cz }).at(lx, lz);
            assert_eq!(paint, VertexPaint::solid(DIRT), "chunk {cx},{cz}");
        }
    }

    #[test]
    fn a_full_neighbour_refuses_the_whole_vertex() {
        let mut t = terrain();
        // Fill chunk (1,0) with four materials away from the border.
        for (i, m) in [1, 2, 3].into_iter().enumerate() {
            let x = 40 + i as u32;
            t.set_vertex_paint(x, 10, VertexPaint::solid(MaterialId(m)))
                .expect("room");
        }
        let snow = MaterialId(4);
        let before = t.clone();
        assert_eq!(
            t.set_vertex_paint(32, 10, VertexPaint::solid(snow)),
            Err(ChunkCoord { x: 1, z: 0 })
        );
        assert_eq!(t, before, "chunk (0,0) wasn't half-painted");
    }

    #[test]
    fn replacing_a_material_everywhere() {
        let mut t = terrain();
        t.paint_vertex(5, 5, DIRT, 1.0, 0.5).expect("room");
        assert!(t.materials_in_use().contains(&DIRT));
        t.replace_material(DIRT, GRASS);
        assert_eq!(t.materials_in_use(), BTreeSet::from([GRASS]));
    }

    #[test]
    fn replacing_a_material_with_itself_changes_nothing() {
        let mut t = terrain();
        t.paint_vertex(5, 5, DIRT, 0.5, 0.5).expect("room");
        let before = t.clone();
        assert!(!t.replace_material(DIRT, DIRT));
        assert_eq!(t, before);
    }

    #[test]
    fn border_copies_that_cannot_agree_are_refused() {
        let size = GridSize::new(2, 1).expect("valid size");
        let n = size.vertex_count();
        // Chunk 1 owns the x = 32 border and paints it rock; chunk 0 is full
        // of four other materials, so its copy can't take rock.
        let rect = ChunkCoord { x: 0, z: 0 }.vertex_rect();
        let mut weights = vec![[255, 0, 0, 0]; rect.vertex_count()];
        weights[1] = [0, 255, 0, 0];
        weights[2] = [0, 0, 255, 0];
        weights[3] = [0, 0, 0, 255];
        let full = [0, 1, 3, 4].map(|m| Some(MaterialId(m)));
        let left = ChunkPaint::from_parts(full, weights).expect("valid paint");
        let right = ChunkPaint::solid(MaterialId(2));
        let heights = vec![0.0; n];
        let parts = TerrainParts {
            heights: &heights,
            paints: vec![left, right],
            cells: None,
            tints: None,
            triangulation: Triangulation::FollowTerrain,
        };
        assert!(Terrain::from_parts(size, 2.0, parts).is_err());
    }

    #[test]
    fn legacy_weights_convert_consistently() {
        let size = GridSize::new(2, 1).expect("valid size");
        let n = size.vertex_count();
        let weights: Vec<LayerWeights> = (0..n)
            .map(|i| {
                if i % 3 == 0 {
                    [0, 255, 0, 0]
                } else {
                    [255, 0, 0, 0]
                }
            })
            .collect();
        let ids = [0, 1, 2, 3].map(MaterialId);
        let t = Terrain::from_legacy(size, 2.0, &vec![0.0; n], &weights, ids).expect("valid");
        for (x, z) in size.full_rect().iter() {
            let expected = if size.vertex_index(x, z).is_multiple_of(3) {
                DIRT
            } else {
                GRASS
            };
            assert_eq!(t.paint_at(x, z), VertexPaint::solid(expected));
        }
        // The border column agrees between both chunks.
        let right = t.chunk_paint(ChunkCoord { x: 1, z: 0 });
        assert_eq!(right.at(0, 7), t.paint_at(32, 7));
    }
}
