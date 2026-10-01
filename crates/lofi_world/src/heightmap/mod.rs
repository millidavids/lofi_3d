//! Heightmaps in and out: 16-bit PNG, R16 and R32, the formats terrain tools
//! trade in, plus a JSON sidecar holding what the pixels don't.
//!
//! PNG and R16 store heights as 0..65535 across a metre range; R32 stores
//! metres as `f32`, so it round-trips exactly. The sidecar, `<file>.json`
//! (`island.png.json`), records the range, the size and the cell size. PNGs
//! also carry the range in a text chunk, in case the sidecar goes astray.
//!
//! Rows run along z and columns along x, row 0 at z = 0, like the map.

mod export;
mod format;
mod read;
mod samples;

#[cfg(test)]
mod tests;

pub use export::export_heightmap;
pub use format::{HeightmapError, HeightmapFormat, HeightmapInfo, MAX_SIDE, sidecar_path};
pub use read::read_heightmap;
pub use samples::{Heightmap, Units};
