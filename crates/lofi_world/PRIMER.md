# lofi_world: how it works

A walk through the crate from the bottom up: the grid, how terrain is stored,
how cells become triangles, paint, the queries on top, and then the two file
formats. File paths are relative to `src/`.

Read `grid.rs` and `terrain.rs` first. Almost everything else is built on
those two.

## 1. The grid

A map is a regular grid of square **cells**, with a **vertex** at every cell
corner. Cells are grouped into **chunks** of 32×32 (`CHUNK_CELLS`), and a map
is always a whole number of chunks: 1 to 64 a side (`MAX_CHUNKS`).

```text
vertices   0 … cells_x        (cells_x + 1 of them per row)
cells      0 … cells_x - 1
chunk c    cells   32c … 32c + 31
           vertices 32c … 32c + 32   (33 a side: it shares its edges)
```

World space: vertex `(x, z)` sits at `(x × cell_size, height, z × cell_size)`.
The map runs from the origin to `extent` on X and Z, +Y is up. Rows run along
Z, so "row-major" means z outer, x inner.

`grid.rs` holds the arithmetic:

- `GridSize` converts between chunks, cells and vertices.
- `ChunkCoord::vertex_rect()` is the chunk's full 33×33 vertex rectangle,
  borders included.
- `VertexRect` is an inclusive rectangle of vertices, the unit edits and
  rebuilds are tracked in.
- `GridSize::owner(x, z)` says which chunk *stores* a vertex (next section).
- `chunks_touching(rect)` lists every chunk whose mesh uses a vertex in the
  rectangle; `chunks_owning(rect)` lists those that store one.

## 2. How terrain is stored

`Terrain` (`terrain.rs`) is a `Vec<ChunkData>`, one per chunk, plus the
grid size, the cell size and the map's triangulation rule.

`ChunkData` (`chunk_data.rs`) holds four things, each in its own `Arc`:

| Data | Per | Covers |
|---|---|---|
| `heights: Arc<Vec<f32>>` | vertex | the vertices the chunk **owns** |
| `tint: Arc<Vec<[u8; 3]>>` | vertex | the vertices the chunk owns |
| `cells: Arc<Vec<CellFlags>>` | cell | the chunk's own 32×32 cells |
| `paint: Arc<ChunkPaint>` | vertex | the chunk's **full 33×33** rectangle |

### Ownership

Neighbouring chunks share the vertices on their common edge. If each stored
its own copy of a height, two copies could disagree and crack the surface.
So each vertex has exactly one owner:

- a chunk owns its west and north edges and its interior (32×32 vertices);
- the last chunk in each direction also owns the closing edge (33 along
  that axis).

`GridSize::owner` and `owned_rect` implement that. `Terrain::height(x, z)`
goes through `locate_vertex`, which finds the owner and the index inside it.

Cells are never shared, so they need no such rule: `Terrain::cell_chunk`.

Paint is the exception: every chunk stores paint for all 33×33 of its
vertices, so a border vertex's paint exists in two or four chunks. That's
because paint's material slots are per chunk (section 4), and a chunk has to
render from its own data alone. `set_vertex_paint` writes every copy or none.

### Copy-on-write

`Terrain` is `Clone`, and a clone copies pointers, not samples. A write goes
through `Arc::make_mut`, which copies the one array it touches if anything
else still holds it.

That's what makes these cheap:

- **Undo** keeps the old `ChunkData` of each chunk a stroke touched. A raise
  stroke copies heights; paint, cells and tint stay shared.
- **Autosave** and **cooking** clone the whole map and work on the clone in
  another thread while editing carries on.

Two consequences to keep in mind:

- Every setter returns whether it changed anything, and an unchanged write
  copies nothing (`set_height` compares first). Callers use the return
  value to decide whether the map is now unsaved.
- Nothing hands out `&mut` to the raw arrays. All writes go through
  `set_height`, `set_cell`, `set_tint`, `set_vertex_paint`, `paint_vertex`,
  so the invariants below can't be bypassed.

`ChunkData::differences` compares two versions of a chunk by pointer, per
data type. Undo uses it to tell the view what kind of thing changed.

Heights are clamped to ±4096 m (`HEIGHT_LIMIT`) and NaN is ignored, so a
runaway brush can't poison the map.

## 3. Cells into triangles

Each cell is two triangles, and there are two ways to cut a square. Which
way matters: it changes the surface between the vertices, so the mesh, the
height under a point, a ray's hit and the water must all agree.

One function decides: `Terrain::splits_on_main_diagonal(cx, cz)`
(`terrain_cells.rs`). "Main" is the diagonal from the cell's (0, 0) corner to
its (1, 1) corner. `cell_triangles` returns the two triangles, wound
counter-clockwise seen from above.

The decision, in order (`Triangulation::splits_on_main` in `cells.rs`):

1. **The cell's own flag.** Bits 1–2 of `CellFlags` can force main or anti.
2. **The map's rule**, a `Triangulation`:
   - `Checkerboard`: alternate by `(cx + cz)` parity. Maps from before the
     rule existed load as this.
   - `FollowTerrain`: cut so the crease bends the way the ground does.
     - `twist = (a + d) − (b + c)` over the four corner heights. Positive
       means the main diagonal's ends are the high pair, so cutting main
       makes a ridge; negative, a valley.
     - `bend` is the summed Laplacian of the four corners: negative on
       convex ground (hilltops, ridges), positive in hollows.
     - Cut main when `(twist > 0) == (bend < 0)`: a ridge crease on convex
       ground, a valley crease in a hollow.
     - If `|twist|` or `|bend|` is under 1 mm, fall back to the
       checkerboard. Without that, float noise flips near-flat cells.

Each rule is a file-format promise. A map stores its rule's code, and the
surface it was sculpted with must come back the same. Never change a rule;
add a new one.

`CellFlags` is one byte: bit 0 hole, bits 1–2 diagonal, bit 3 no clutter,
bit 4 unwalkable. Bits 5–7 are kept as they are, for a later tool.

## 4. Paint

A map has a **material table** of up to 32 materials (`materials.rs`), each
with a stable `MaterialId`, a colour, a procedural pattern and a tile size.
Ids are never reused: `next_id` only grows, so deleting a material never
renumbers the others. One material is the **base**: what erasing paints back
to.

Paint is stored per chunk (`paint.rs`):

```rust
struct ChunkPaint {
    slots: [Option<MaterialId>; 4],  // ascending ids, empty slots last
    weights: Vec<[u8; 4]>,           // 33 × 33, one weight per slot
}
```

Four materials per chunk is WoW's limit, and it's what lets the terrain
shader sample at most four textures. Two invariants hold after every write:

- **Slots are canonical**: ascending by id, empties last. Equal paint then
  compares equal, and files are deterministic.
- **Each vertex's weights sum to 255.**

`VertexPaint` is the same idea for one vertex, independent of any chunk: up
to four `(material, weight)` pairs.

### Writing paint

`Terrain::set_vertex_paint(x, z, paint)`:

1. For every chunk that has this vertex (`chunks_touching`), ask
   `ChunkPaint::can_fit`: does the chunk have a slot for each material, or a
   free one, or one it can reclaim because nothing else in the chunk uses it?
2. If any chunk can't, change nothing and return `Err(chunk)`.
3. Otherwise write all the copies.

So a fifth material in a chunk is refused, and the refusal names the chunk.
The editor shows that in the status bar.

`paint_vertex` is the brush's entry point: it moves a vertex some fraction
of the way toward one material (`VertexPaint::toward`, built on
`layers::paint_toward`):

- the target material's weight grows by `amount` of what's left to 255;
- the others shrink in proportion, with the rounding remainder given to the
  largest, so the sum stays 255;
- `dither` (0 to 1) decides which way the fractional step rounds. A fresh
  random value per vertex per frame means a light brush still arrives on
  average instead of rounding to nothing near full coverage.

`replace_material(from, to)` repaints everything in the map. It never fails:
where a chunk has no slot for `to`, it reuses `from`'s.

### Border agreement

Because border paint is stored twice, a file written by something else could
disagree with itself. `Terrain::from_parts` calls `make_shared_paint_agree`,
which copies each owner's paint over the other copies, or fails with a
message if a chunk lacks the materials. Left alone, such a seam would refuse
every later stroke there.

## 5. Tint

Per-vertex `[u8; 3]`, a colour multiplier where 128 means "no change", so
it can brighten as well as darken. Owned like heights. `tint_toward` moves a
vertex toward a target with the same dithered rounding as paint.

## 6. Reading the surface

Two layers:

**`sampling.rs`**, on `Terrain` only:

- `height_at(xz)`: the surface height at any point, on the cell's actual
  triangles. In a hole it still returns the height the ground would have.
- `normal(x, z)`: the *smooth* vertex normal, from central differences over
  the neighbours, reading across chunk borders so shading has no seams.
- `curvature(x, z)` (in `terrain_cells.rs`): the Laplacian per metre,
  positive in hollows. Used by masks.

**`ground/`**, the collision and placement queries, on anything that
implements `Ground`:

| Query | Answer |
|---|---|
| `ground_at(xz)` | `Surface(point)`, `Hole`, or `Unknown` (no data) |
| `point_at(xz)` | the `Surface` case as an `Option` |
| `trace_ray(…)` | `Hit`, `Clear`, or `Unknown { distance }` |
| `cast_ray(…)` | the `Hit` case as an `Option` |
| `chunk_bounds(chunk)` | the chunk's box with its real height range |
| `chunk_collision(chunk)` | positions and triangle indices, holes left out |

A `GroundPoint` carries the position, the **face** normal (the triangle
under the point, which is what a foot stands on, not the smoothed shading
normal), `walkable`, and `no_clutter`. Walkable means not flagged unwalkable
and no steeper than 50° (`WALKABLE_SLOPE_DEGREES`).

### One implementation

An implementor supplies four things: `cell_size`, `cell_counts`,
`vertex_height(x, z)` and `cell_shape(cx, cz)`. Everything else is a
provided method in `ground/query.rs`. `Terrain` implements it
(`ground/terrain.rs`), and so does the cooked world (`cook/ground.rs`), so
the editor and the game compute answers with the same code and can differ
only by the cook's 1 cm height step.

The interpolation itself is one function, `ground::cell_plane(corners,
main_diagonal, u, v)`. It returns the height at `(u, v)` across a cell and
the rise per cell along x and z, on whichever triangle holds the point.
`Terrain::height_at`, cooked chunks and every `Ground` query call it.
`ground::locate` is the matching "which cell is this point in" function;
points exactly on the map's far edge belong to the last cell.

### The ray

`ground/ray.rs`. The ray is clipped to the map's footprint, then walks the
cells under it one at a time (a 2D grid walk). In each cell it looks at
`ray height − surface height` at the cell's entry, at the point where the
ray crosses the cell's diagonal, and at its exit. Over one triangle that
difference is a straight line, so if it goes from positive to negative the
crossing is found by interpolation.

Details worth knowing:

- **No triangle test.** An earlier version tested each cell's two triangles
  and long rays slipped between them once float error outgrew the edge
  tolerance. A sign change has no tolerance to outgrow: the surface is one
  continuous function.
- **Cells well above or below the ray are skipped** before asking how the
  cell is cut. For `Terrain`, working out the diagonal is the expensive
  part.
- **From above only.** A ray that starts under the surface doesn't hit it
  from below. One that starts within a millimetre under it counts as on it
  and, heading down, hits at distance 0. So a point taken from `ground_at`
  and cast from again doesn't fall through.
- **Holes**: `Holes::Open` lets the ray through (a character falls);
  `Holes::Solid` stops it (the editor's cursor must be able to point at a
  hole to fill it). Under an open hole the ray is still "underground".
- **Unloaded ground**: the walk stops and returns `Unknown { distance }`.
  That is not a clear line.

`raycast.rs` keeps `Terrain::raycast` and `raycast_solid` as shorthand
returning just the hit point.

## 7. Meshing and baked light

`mesh.rs`: `build_chunk_mesh` turns a chunk into plain vertex buffers: one
vertex per grid point (33×33), positions relative to the chunk's origin,
smooth normals, the chunk's four slot ids, weights and tints per vertex, and
indices from `cell_triangles`. Hole cells emit no triangles. It knows
nothing about any renderer; the editor packs this into a Bevy mesh.

`ao.rs`: baked ambient occlusion, one byte per vertex, 255 for open sky. For
each of eight compass directions it finds the highest horizon within 8 cells
(sampling at 1, 2, 3, 4, 6 and 8 cells out), measured against the surface's
own tangent plane, and takes `cos²` of that angle as how much sky that side
sees. Measuring from the tangent plane means a bare slope reads as open sky,
so slopes aren't darkened twice (the hemisphere ambient already shades
them). AO depends on heights only and is never saved; a height edit
invalidates it up to 8 cells away (`AO_RADIUS_CELLS`).

## 8. Masks and noise

`masks.rs`: a `Mask` gives a weight 0 to 1 at a vertex from the ground's own
shape: height, slope in degrees, curvature, or fractal noise. Each has a
range, a soft edge (`fade`), and can be inverted. A `MaskStack` multiplies
its enabled masks, so "steep and high" is two masks. Brushes and auto-paint
multiply their strength by it.

`noise.rs`: `hash01(x, z, seed)` is a uniform 0 to 1 per lattice point, and
`fbm` is four octaves of value noise built on it. Both are fixed in space
and deterministic, so a noise brush held still grows the same features
taller, and generated maps are repeatable.

## 9. The rest of a map

`map.rs`. `WorldMap` is the terrain plus:

- `name`;
- `materials: MaterialTable`;
- `water: Water`: one sea level, shallow and deep colours, the depth at
  which it reads fully deep;
- `lighting: Lighting`: sun bearing, elevation, colour and strength; sky and
  ground ambient; sky colours; fog. Modelled on WoW's light table.
  `sanitized()` clamps everything after a load;
- `objects: ObjectLayer` (section 12);
- `unknown_sections`: file sections this build doesn't understand, kept to
  write back untouched.

## 10. The `.lofimap` file

`file/`. One file per map.

### Layout (version 3)

```text
8 bytes   magic "LOFIMAP\0"
u32       format version (3)
u32       minimum reader version
u32       crc32 of the body
body      MessagePack: { meta, sections }
```

All integers little-endian. `meta` is small: name, size, cell size, the
material table, water, lighting, the triangulation code, and the revision
counters (below). The bulk is in `sections`, a list of:

```rust
struct SectionRecord {
    tag: String,       // "heights", "paint", …
    critical: bool,
    codec: String,     // "raw", "zstd", "shuffle4-zstd"
    raw_len: u64,      // unpacked size
    crc32: u32,        // of the unpacked bytes
    data: Vec<u8>,
}
```

| Section | Contents | Codec | Critical |
|---|---|---|---|
| `heights` | `f32` per vertex, row-major | shuffle4-zstd | yes |
| `paint` | per chunk: four `u16` slot ids (`0xFFFF` empty), then 33×33×4 weight bytes | shuffle4-zstd | yes |
| `cells` | one flag byte per cell, row-major | zstd | yes |
| `tint` | three planes: all red, all green, all blue | zstd | yes |
| `objects` | header then 44-byte records (section 12) | zstd | no |

**shuffle4** (`file/codec.rs`) groups byte 0 of every 4-byte value, then
byte 1, and so on, before compressing. The slow-changing high bytes of
heights and the mostly-zero paint channels then compress far better.
Compression is `ruzstd`, which only implements its fastest level.

### How the format grows without breaking

- **Unknown sections are kept.** A section this build doesn't recognise is
  stored as it came (`UnknownSection`) and written back byte-for-byte, so an
  older editor doesn't destroy a newer tool's data. That's why tool-owned
  data goes in a section, not in `meta`: unknown `meta` fields are dropped
  on save.
- **Critical** means "an editor that ignores this would corrupt the map".
  An unknown critical section refuses to load. `objects` is not critical.
- **Minimum reader version** lets a newer writer stay readable by older
  builds. A build refuses files whose minimum is above what it supports.
- **New `meta` fields** get `#[serde(default)]`.
- The version number is bumped only when an old reader would *misread* the
  file.

### Reading safely

`decode_map` never panics on bad input, and never allocates on a file's
say-so before checking it:

1. Magic, version, header checksum.
2. Decode the MessagePack body. Check the grid size and revision range.
3. For each known section, check `raw_len` against what the grid size
   implies **before** unpacking, then unpack into exactly that many bytes
   and check its crc.
4. `Terrain::from_parts` and `ChunkPaint::from_parts` are the last gate:
   lengths, finite heights, unique slot ids, normalised weights, border
   agreement.

### Older versions

`v1.rs` and `v2.rs` are frozen readers. Both stored four map-wide layers
with weights per vertex; `Terrain::from_legacy` converts them to per-chunk
paint with the checkerboard rule. Saving always writes version 3.

Each version has a golden file in `tests/golden/` that must keep loading to
exactly the same map.

### Saving without losing a map

`file/save.rs`, `write_atomically`:

1. Resolve symlinks, so the link survives. Refuse a read-only target.
2. Write `<file>.tmp`, `fsync` it, copy the old file's permissions onto it.
3. Copy the old file to `<file>.bak` (a fresh file, not `fs::copy`, so a
   read-only backup can't block later saves).
4. Rename the temp file over the target, then `fsync` the directory.

A crash at any point leaves the old map or the new one. On Windows, the
rename is retried briefly when antivirus or a sync client holds the file.

### `FileStamp`

Two counters saved in `meta`:

- `revision` counts edits.
- `autosave_base` is set only in autosaves: the revision of the file the
  autosave was taken from.

Together they let an editor prove an autosave holds newer work than the map
beside it, or that the map has moved on since. The editor's autosave and
recovery are built on that.

## 11. Heightmaps

`heightmap/`. Three formats: 16-bit PNG, R16 (raw `u16`), R32 (raw `f32`,
metres, exact).

- PNG and R16 store 0 to 65535 across a height range the pixels don't carry.
  A JSON sidecar, `<file>.json`, records the range, the size and the cell
  size. PNGs also carry the range in a `lofi-range` text chunk in case the
  sidecar is lost.
- Reading (`read.rs`) checks the file size against the sidecar's dimensions
  before reading it, caps a side at 4097 samples, and refuses R32 values
  that are NaN or past a million metres.
- `Heightmap::resample` fits another size one axis at a time, in `f64`:
  linear where an axis grows or shrinks a little, an area average where it
  shrinks by half or more (linear would alias). `crop_or_pad` and `flipped`
  are the other two fits.

## 12. Objects

`objects.rs`, `assets.rs`.

- `PlacedObject`: an `ObjectId`, an `AssetGuid`, position, yaw, uniform
  scale.
- `ObjectId`s are allocated in order and **never reused**: `next_id` only
  grows, even when objects are deleted or a placement is undone, so
  something referring to an id can't silently retarget.
- Objects name their asset by GUID, never by file path, so assets can move.
- `ObjectLayer` keeps the list sorted by id, in an `Arc` like terrain
  chunks. `list()` and `swap_list()` exist for undo: it swaps the list and
  leaves `next_id` alone.
- A layer read from a newer `objects` format is **foreign**: empty,
  uneditable, with its section kept raw and written back.

The `objects` section: `format: u32` (1), `next_id: u64`, `count: u32`, then
per object `id: u64`, `asset: u128`, position 3 × `f32`, `yaw: f32`,
`scale: f32`.

`assets.rs` is a placeholder catalog of six shapes with fixed GUIDs, until
there are real models. An unknown GUID draws as a box.

## 13. Cooking for the game

`cook/`. The editor's file is one big file tuned for editing. The game
wants small files it can stream. Cooking turns one into the other.

### Output

```text
cooked/
  world.lofiworld            the manifest
  tiles/t_<x>_<z>.lofitile   one per tile
```

A **tile** is a square of chunks, 4×4 by default. Both file kinds share a
header: 8-byte magic, version, minimum reader, crc32 of the packed body, the
unpacked length, then a zstd-packed MessagePack body. Cooking the same map
writes the same bytes.

The **manifest** (`manifest.rs`): grid size, cell size, tile size, the
height step, materials, water, lighting, and a `TileEntry` per tile with its
file name, size, crc32 and height range.

A **cooked chunk** (`chunk.rs`) is self-contained: all 33×33 vertices, so it
renders and collides without its neighbours.

| Field | Contents |
|---|---|
| `base`, `heights` | height in 1 cm steps above the chunk's lowest point, `u16` per vertex (`u32` when the chunk spans more than 655 m) |
| `split`, `hole`, `no_clutter`, `unwalkable` | one bit per cell |
| `normals` | two bytes per vertex, octahedral |
| `slots`, `weights` | the four material ids and per-vertex weights |
| `light` | tint r, g, b and baked AO per vertex |

Two choices matter for collision:

- **One global height step** (`HEIGHT_STEP`, 1 cm), with heights counted
  from each chunk's own base. A vertex two chunks share then quantises to
  the same value in both, so seams can't crack. A per-chunk scale would
  crack them.
- **Diagonals are baked as bits**, computed from the *unquantised* heights.
  The follow-terrain rule needs neighbours' heights, and quantising could
  flip it; baking keeps the cooked triangles the editor's triangles.

### Where things are

`layout.rs`: `TileLayout` answers position questions without touching a
tile: which tile holds a point (`tile_at`, derived from the point's cell so
it agrees with the ground queries), a tile's rectangle, the tiles within a
radius, nearest first. `Manifest::tile_bounds` adds the height range for a
full box.

### Writing safely

`write.rs`, `cook_map`:

1. Refuse tile sizes outside 1 to 16, and maps whose objects are foreign.
2. Resolve the target folder, so cooking through a link fills the folder it
   points at. The target must be missing, empty, or an earlier cook
   (`previous_cook` checks by file name).
3. Take a lock file beside the target, so two cooks can't share staging.
4. Write everything into `.<name>.cooking` beside the target. The manifest
   is written last.
5. `swap_in`: move the old cook aside to `.<name>.cooked-old`, move staging
   into place, then delete the old one. If the second move fails, the old
   cook is moved back.

It only ever deletes a cook's own files, by name.

### Reading back

`read.rs`, `CookedWorld::open` and `load_tile`. Nothing is trusted:

- the manifest's sizes must be ones a cook writes, and tile paths must stay
  inside the folder;
- a tile's bytes must match the manifest's size and crc;
- a tile must hold exactly its chunks, in order;
- its height range must match the manifest's (a wrong bound would cull real
  ground);
- every chunk's blobs are size-checked **as it is deserialised**
  (`#[serde(try_from)]` in `chunk.rs`), so its accessors can't index out of
  bounds however it was read.

Cooked output is a build artefact. A format change bumps `COOK_VERSION`, and
old cooks are refused with "cook the map again" rather than migrated.

### The cooked ground

`ground.rs`: `CookedGround` holds the tiles that are currently loaded and
implements `Ground` across them. Tiles are inserted as they stream in and
removed as they leave. Queries cross tile seams, and ground that isn't
loaded is `Unknown`, never a hole. `examples/cooked_walk.rs` shows the loop
a game would run.

## 14. What must stay true

A checklist of the invariants above, for when you change something:

- Every vertex height and tint has one owner. Paint is the only data stored
  in more than one chunk, and its copies agree.
- A chunk's paint slots are ascending with empties last; each vertex's
  weights sum to 255.
- Setters report whether they changed anything, and don't copy when they
  didn't.
- How a cell is cut is decided in `splits_on_main_diagonal`. How its surface
  is interpolated is decided in `cell_plane`. Nothing else re-derives
  either.
- A `Triangulation` rule never changes once shipped.
- Material ids and object ids are never reused.
- A file is validated before anything is allocated from it, and reading
  never panics.
- Saving and cooking never write in place.
- Golden files are never rewritten.
