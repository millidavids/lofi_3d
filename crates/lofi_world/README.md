# lofi_world

The map data every lofi tool and the game share. No renderer, no window, no
Bevy: it depends on `bevy_math` for vectors and nothing else from the engine,
so its tests run headless in a couple of seconds.

If a tool and the game both need something, it lives here.

How it works inside: [PRIMER.md](PRIMER.md).

## What's in it

| Module | What it holds |
|---|---|
| `grid` | Sizes and indexing: chunks, cells, vertices, who owns which vertex. |
| `terrain`, `chunk_data` | The heightmap, stored per chunk, copy-on-write. |
| `terrain_cells`, `cells` | Cell flags (hole, diagonal, no clutter, unwalkable), triangulation rules, vertex tint. |
| `paint`, `terrain_paint`, `layers` | Paint: up to four materials per chunk, weights per vertex. |
| `materials` | The map's material table. |
| `map` | `WorldMap`: terrain, materials, water, lighting, objects. |
| `sampling` | Height between vertices, smooth vertex normals. |
| `ground` | Collision and placement queries: `Ground`, the raycast, bounds, collision meshes. |
| `raycast` | `Terrain::raycast`, shorthand over `ground`. |
| `mesh` | A chunk as vertex buffers, for any renderer. |
| `ao` | Baked ambient occlusion per vertex. |
| `masks` | Height, slope, curvature and noise masks for brushes. |
| `noise` | Deterministic value noise and a hash. |
| `heightmap` | Import and export: 16-bit PNG, R16, R32. |
| `file` | The `.lofimap` format: header, sections, save and load, readers for older versions. |
| `cook` | The game's format: a manifest and streamable tiles, plus reading them back. |
| `objects`, `assets` | Placed objects and the placeholder asset catalog. |

## Using it

A map, edited and saved:

```rust
use lofi_world::{FileStamp, GridSize, MaterialId, WorldMap, load_map, save_map};

let size = GridSize::new(8, 8).expect("1 to 64 chunks a side");
let mut map = WorldMap::flat("Zone", size, 2.0, 4.0); // 2 m cells, ground at 4 m

map.terrain.set_height(10, 10, 12.0);
// Towards material 1 by 80%. `Err(chunk)` if that chunk already blends four
// others; `Ok(changed)` otherwise.
map.terrain
    .paint_vertex(10, 10, MaterialId(1), 0.8, 0.5)
    .expect("a new map's chunks have free slots");

save_map(&map, FileStamp::default(), "zone.lofimap".as_ref())?;
let loaded = load_map("zone.lofimap".as_ref())?;
assert_eq!(loaded.map, map);
```

Asking the ground (the same calls work on a cooked world):

```rust
use lofi_world::{Ground, GroundAt, Holes};

match map.terrain.ground_at(xz) {
    GroundAt::Surface(p) => { /* p.position, p.normal, p.walkable */ }
    GroundAt::Hole => {}
    GroundAt::Unknown => {} // off the map
}
let hit = map.terrain.cast_ray(origin, direction, 500.0, Holes::Open);
```

Cooking for the game and reading it back:

```rust
use lofi_world::cook::{CookSource, CookedWorld, DEFAULT_TILE_CHUNKS, cook_map};

cook_map(&map, "cooked".as_ref(), DEFAULT_TILE_CHUNKS, CookSource::default())?;

let world = CookedWorld::open("cooked".as_ref())?;
let mut ground = world.ground();
for (x, z) in world.layout().tiles_within(player_xz, 300.0) {
    ground.insert(world.load_tile(x, z)?)?;
}
```

## Binaries and examples

```sh
# Cook a map; ask a cook about a point.
cargo run -p lofi_world --bin lofi_cook -- zone.lofimap cooked/ [--tile-chunks N]
cargo run -p lofi_world --bin lofi_cook -- --probe 256,256 cooked/

# A painted island to start from. CHUNKS is its width (default 8).
cargo run -p lofi_world --example demo_island -- demo.lofimap [CHUNKS]

# Walk a cook the way a game would, streaming tiles.
cargo run -p lofi_world --example cooked_walk -- cooked/ [RADIUS]

# Timings.
cargo run --release -p lofi_world --example measure_mesh
cargo run --release -p lofi_world --example measure_format
```

## Limits

| What | Limit |
|---|---|
| Cells per chunk side | 32 |
| Chunks per map side | 1 to 64 (the editors make up to 32) |
| Cell size | 0.5 to 16 m |
| Height | ±4096 m |
| Materials per map | 32 |
| Materials blended per chunk | 4 |
| Objects per map | 1,000,000 |
| Heightmap side, import | 4097 samples |
| Cooked height step | 1 cm |
| Chunks per cooked tile side | 1 to 16 (default 4) |

## Tests

```sh
cargo test -p lofi_world
```

- Unit tests sit beside the code, or in a `tests.rs` next to it.
- `tests/golden.rs` loads one saved file per format version from
  `tests/golden/`. Those files are never rewritten. A new format version
  gets a new one: `cargo test -p lofi_world --test golden -- --ignored bless`.
- `tests/format_robustness.rs` truncates and corrupts map files. The reader
  must return an error or a map, never panic.
