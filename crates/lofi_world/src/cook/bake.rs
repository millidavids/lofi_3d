//! Baking a chunk of terrain into its cooked form.

use super::chunk::{CELL_BITS, CookedChunk, EMPTY_SLOT};
use super::normal::encode_normal;
use super::{CookError, HEIGHT_STEP};
use crate::ao::chunk_ao;
use crate::grid::{CHUNK_CELLS, ChunkCoord};
use crate::terrain::Terrain;

impl CookedChunk {
    pub(super) fn cook(terrain: &Terrain, chunk: ChunkCoord) -> Result<Self, CookError> {
        let rect = chunk.vertex_rect();
        let steps: Vec<i64> = rect
            .iter()
            .map(|(x, z)| quantise(terrain.height(x, z)))
            .collect();
        let base = steps.iter().copied().min().unwrap_or(0);
        let top = steps.iter().copied().max().unwrap_or(0);
        // A sheer 655 m cliff inside 64 m is rare but legal: spend four bytes.
        let wide = top - base > i64::from(u16::MAX);
        let heights = if wide {
            steps
                .iter()
                .flat_map(|s| ((s - base) as u32).to_le_bytes())
                .collect()
        } else {
            steps
                .iter()
                .flat_map(|s| ((s - base) as u16).to_le_bytes())
                .collect()
        };

        let mut split = vec![0; CELL_BITS];
        let mut hole = vec![0; CELL_BITS];
        let mut no_clutter = vec![0; CELL_BITS];
        let mut unwalkable = vec![0; CELL_BITS];
        for lz in 0..CHUNK_CELLS {
            for lx in 0..CHUNK_CELLS {
                let (cx, cz) = (rect.min_x + lx, rect.min_z + lz);
                let flags = terrain.cell(cx, cz);
                let bit = (lz * CHUNK_CELLS + lx) as usize;
                let set = |bits: &mut Vec<u8>, on: bool| {
                    if on {
                        bits[bit / 8] |= 1 << (bit % 8);
                    }
                };
                set(&mut split, terrain.splits_on_main_diagonal(cx, cz));
                set(&mut hole, flags.hole());
                set(&mut no_clutter, flags.no_clutter());
                set(&mut unwalkable, flags.unwalkable());
            }
        }

        let normals = rect
            .iter()
            .flat_map(|(x, z)| encode_normal(terrain.normal(x, z)))
            .map(|v| v as u8)
            .collect();
        let paint = terrain.chunk_paint(chunk);
        let slots = paint.slots().map(|s| s.map_or(EMPTY_SLOT, |m| m.0));
        let weights = paint.weights().as_flattened().to_vec();
        let ao = chunk_ao(terrain, chunk);
        let light = rect
            .iter()
            .zip(ao)
            .flat_map(|((x, z), ao)| {
                let [r, g, b] = terrain.tint(x, z);
                [r, g, b, ao]
            })
            .collect();

        Ok(Self {
            x: chunk.x,
            z: chunk.z,
            base: base as i32,
            heights,
            wide,
            split,
            hole,
            no_clutter,
            unwalkable,
            normals,
            slots,
            weights,
            light,
        })
    }
}

fn quantise(h: f32) -> i64 {
    (f64::from(h) / f64::from(HEIGHT_STEP)).round() as i64
}
