//! The cooked world: what the game loads, made from a map.
//!
//! A directory holding `world.lofiworld` (the manifest: grid, materials,
//! water, lighting and a tile index) and `tiles/t_<x>_<z>.lofitile`, one per
//! square of `tile_chunks` × `tile_chunks` chunks, so the game streams the
//! world a tile at a time.
//!
//! Both kinds of file share the map files' header: magic, version, minimum
//! reader version, then a crc32 and the unpacked length of a zstd'd
//! MessagePack body. Cooking the same map always writes the same bytes.

//!
//! A cooked world also says where it is and what it is to collide with:
//! [`TileLayout`] and [`Manifest::tile_bounds`] place every tile without
//! opening one, and [`CookedGround`] answers the same [`crate::Ground`]
//! queries the editor's terrain does, over whichever tiles are loaded.

mod bake;
mod chunk;
mod format;
mod ground;
mod layout;
mod lookup;
mod manifest;
mod normal;
mod read;
mod tile;
mod write;

#[cfg(test)]
mod ground_tests;
#[cfg(test)]
mod tests;

pub use chunk::CookedChunk;
pub use format::{
    COOK_VERSION, CookError, DEFAULT_TILE_CHUNKS, HEIGHT_STEP, MANIFEST_FILE, MAX_TILE_CHUNKS,
    TILE_DIR,
};
pub use ground::CookedGround;
pub use layout::TileLayout;
pub use manifest::{CookSource, CookedLighting, CookedMaterial, CookedWater, Manifest, TileEntry};
pub use read::CookedWorld;
pub use tile::{CookedObject, CookedTile};
pub use write::cook_map;
