//! World data shared by every lofi tool and, later, the game: the terrain
//! heightmap and its paint, materials, lighting, the map file format, and
//! engine-neutral meshing and picking. No renderer or window here, so it tests
//! headless.

mod ao;
mod assets;
mod cells;
mod chunk_data;
pub mod cook;
mod file;
mod grid;
mod ground;
mod heightmap;
mod layers;
mod map;
mod masks;
mod materials;
mod mesh;
mod noise;
mod objects;
mod paint;
mod raycast;
mod sampling;
mod terrain;
mod terrain_cells;
mod terrain_paint;

pub use ao::{AO_RADIUS_CELLS, chunk_ao, vertex_ao};
pub use assets::{Asset, BUILTIN as BUILTIN_ASSETS, Shape, UNKNOWN_ASSET, asset};
pub use cells::{CellFlags, Diagonal, Triangulation};
pub use chunk_data::{ChunkData, ChunkDifferences, NEUTRAL_TINT, Tint};
pub use file::{
    FileStamp, LoadedMap, MAP_FILE_EXTENSION, MAP_FORMAT_VERSION, MapFileError, SaveReport,
    UnknownSection, backup_path, decode_map, encode_map, load_map, save_autosave, save_map,
};
pub use grid::{CHUNK_CELLS, ChunkCoord, GridSize, MAX_CHUNKS, VertexRect};
pub use ground::{
    Bounds, CellShape, CollisionMesh, Ground, GroundAt, GroundCell, GroundHit, GroundPoint, Holes,
    RayTrace,
};
pub use heightmap::{
    Heightmap, HeightmapError, HeightmapFormat, HeightmapInfo, MAX_SIDE, Units, export_heightmap,
    read_heightmap, sidecar_path,
};
pub use layers::{LayerWeights, normalize_weights, paint_toward};
pub use map::{Lighting, Water, WorldMap};
pub use masks::{Mask, MaskKind, MaskStack, WALKABLE_SLOPE_DEGREES};
pub use materials::{
    MAX_MATERIALS, MAX_TILE_SIZE, MIN_TILE_SIZE, Material, MaterialId, MaterialTable, Pattern,
    clamp_tile_size,
};
pub use mesh::{ChunkMesh, build_chunk_mesh};
pub use noise::{fbm, hash01};
pub use objects::{
    AssetGuid, MAX_OBJECT_SCALE, MAX_OBJECTS, MIN_OBJECT_SCALE, ObjectId, ObjectLayer, ObjectList,
    PlacedObject,
};
pub use paint::{CHUNK_PAINT_VERTICES, ChunkPaint, SLOTS, VertexPaint};
pub use terrain::{HEIGHT_LIMIT, MAX_CELL_SIZE, MIN_CELL_SIZE, Terrain};
