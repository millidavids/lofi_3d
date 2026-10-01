# Roadmap

Where the terrain editor goes next. Drawn from a research pass on how WoW
Classic's world was built and on current terrain and stylised-rendering
practice, then revised after a staff-level review (notes at the end).

## Where we are (0.7)

Phases 1 to 5 are done, and Phase 6's terrain half. The direction since
Phase 5: this tool builds the skeleton of the world, so the terrain comes
first and must describe itself (collision and position). Small assets'
collision and position belong in a separate crate.

- `lofi_world`:
  - a heightmap stored per chunk (f32 per vertex, 2 m cells by default),
    copy-on-write;
  - a map-level material table, up to 4 materials per chunk, painted per
    vertex;
  - per-cell flags (holes, forced diagonals, no clutter, unwalkable), a
    triangulation rule per map, and vertex tint;
  - placed objects with permanent ids and asset GUIDs, and a placeholder
    asset catalog;
  - one sea level and a light table (sun, ambient, sky, fog);
  - `.lofimap` v3: compressed, checksummed sections. v1 and v2 still read.
  - heightmap import/export (16-bit PNG, R16, R32);
  - masks (height, slope, curvature, noise);
  - chunk meshing, baked AO, noise;
  - `Ground`: where the surface is, its normal, whether it's walkable, an
    exact raycast, and each chunk's box and collision triangles. The
    editor's terrain and a cooked world both answer through it;
  - the cooked game format: tiles of self-contained chunks, written by
    `lofi_cook` or from the editors, with a layout and per-tile bounds that
    place every tile from the manifest alone.
- `lofi_editor`, shared by both editors:
  - copy-on-write undo, including map settings and objects;
  - new/open/save with native dialogs, an unsaved-changes guard, `.bak`
    backups, a lock so one map is open in one editor, and autosave with crash
    recovery;
  - fly camera;
  - textured, height-blended terrain with Lambert sun, hemisphere ambient,
    baked AO and cascaded shadows; shared distance and height fog; a sky
    dome; a sea with depth colour and foam; low-poly objects;
  - overlays: slope, chunk grid, full chunks, cell flags.
- `lofi_terrain`: raise/lower, smooth, flatten, noise, paint, tint and cell
  tools, all limited by masks and a rectangle/lasso selection; auto-paint;
  heightmap import/export.
- `lofi_doodads`: place, pick, move, turn, scale and delete objects.

## What the research changed

1. **WoW Classic terrain was smooth-shaded and textured.**
   - The look comes from four hand-painted tiling textures per chunk.
   - They are blended by a paint map about 8× denser than the height grid,
     about 0.5 m per texel.
   - Lighting is soft diffuse plus ambient, with no specular.
   - Fog and sky colours come from per-zone tables.
   - The editor now defaults to smooth shading; flat is a toggle.
2. **Paint is too coarse.** Per-vertex weights at 2 m are about 4× coarser than
   WoW's. Paint needs its own grid.
3. **Flat shading as built is too heavy.**
   - Duplicated vertices cost about 270 KB per chunk, plus the CPU copy Bevy
     keeps.
   - An indexed grid, with the face normal computed in the fragment shader, is
     about 65 KB.
4. **The v1 file won't last.**
   - No compression.
   - Floats written one tagged value at a time.
   - No holes, no material table.
   - It's pre-release, so replace it now.

## Phase 1: storage and saving — done

Changed how data is held and written, not what it is, so paint storage isn't
locked in before its renderer exists (Phase 2).

**1.1 Chunked copy-on-write storage**
- **Storage:** each chunk stores the vertices it owns: its west and north
  edges, plus the closing edge for the last chunk in each direction.
- **Copy-on-write:** heights and weights are separate `Arc`s, so a sculpt
  stroke copies only the heights of the chunks it touches.
- **Undo:**
  - Keeps each touched chunk's data from before the edit and swaps it back.
  - Redo uses the same entry, with no "after" copies.
  - Map settings (layers, water, name) are undoable too. Edits merge within
    one drag or typing session, never across a save.
- **Snapshots:** cloning the whole map (the autosave snapshot) copies pointers
  only.

**1.2 File format v2**
- **Layout:**
  - Header: magic, version, minimum reader version, crc32 of the body.
  - MessagePack metadata.
  - Named sections, each `{tag, critical, codec, raw length, crc32}`.
- **Bulk data:** little-endian, byte-shuffled, zstd through pure-Rust `ruzstd`.
  It only implements its fastest level (about zstd 1); the others
  `unimplemented!()`.
- **Checksums:**
  - The review said zstd's frame checksum made extra ones redundant, but
    ruzstd writes none.
  - So each section carries a crc32 of its unpacked bytes, and the header one
    of the whole body. `crc32fast` was already in the tree.
  - Every single-bit change to a file fails to load.
- **Reading defensively:**
  - Section sizes are checked against the grid before unpacking.
  - Unknown critical sections refuse the open; unknown optional ones are kept
    and written back.
  - Out-of-range revisions are rejected.
- **Tests:**
  - Golden files for v1 and v2.
  - Round trips of random maps.
  - Every truncation, and every byte flipped three ways, both as-is and
    re-signed so the damage reaches the decoders: no panics.
- **Measured** (painted hills, release; dev build about 1.2× slower):

  | Map | Raw | File | Encode | Decode |
  |---|---|---|---|---|
  | 8×8 chunks | 0.5 MB | 0.3 MB | 6 ms | 1 ms |
  | 32×32 chunks | 8.0 MB | 5.3 MB | 77 ms | 13 ms |
  | 64×64 chunks | 32 MB | 21 MB | 331 ms | 71 ms |

  About 66% of raw: the low bits of float heights don't compress. Manual saves
  stay on the main thread; autosave runs on a worker.

**1.3 Saving safely**
- **Save order:**
  1. Write the temp file and fsync it.
  2. Write `.bak` as a fresh copy.
  3. Rename over the map.
  4. fsync the directory (Unix).
- **Save behaviour:**
  - A `.bak` failure is reported but doesn't block the save.
  - Read-only maps are refused ("not checked out").
  - Saves write through symlinks and keep the file's permissions.
  - Windows retries sharing and lock violations as well as access denied.
- **Autosave:**
  - Every 2 minutes on a worker thread, from a copy-on-write snapshot.
  - Next to the map as `<file>.autosave`. Untitled maps use a unique name per
    session in the app-data directory.
  - A session only writes or deletes a sidecar it owns. Anything else may be
    another session's unsaved work.
  - A failed autosave retries at the next interval.
- **Recovery:**
  - Stamps record the edit revision and the file revision the autosave grew
    from.
  - On open, a newer autosave is offered back. If the file changed on disk
    since, it's still offered, with a warning, rather than deleted.
  - Untitled work is offered at startup and on New.
- **Races closed:**
  - An Open picked after the map changed goes back through the
    unsaved-changes prompt.
  - Shortcuts are off while a prompt is up.
  - Quit and discard wait for an autosave in flight before deleting its
    sidecar.
  - Strokes commit before undo in the same frame.
  - Brush dabs that change nothing leave the map clean.

## Phase 2: paint and the WoW look, together — done

What changed from the plan below:

- **Paint stays per vertex** (a decision, see Decisions). Each vertex carries
  its chunk's four material ids and its weights as mesh attributes, so every
  chunk shares one material and 2.0's binding question went away.
- **2.0 measured** with `--measure`: the whole demo island in view from
  altitude, fog off, release build, RX 9070 XT, Wayland.

  | Map | Terrain triangles | GPU per frame | Frame (wall) |
  |---|---|---|---|
  | 8×8 chunks | 0.13 M | 0.4 ms | 1.2 ms |
  | 32×32 chunks | 2.1 M | 1.4 ms | 1.9 ms |
  | 64×64 chunks | 8.4 M | 3.5 ms | 4.3 ms |

  These are the figures after Phase 5, with the demo's objects in view.

  - **No LOD.** Even the format's largest map runs at over 200 fps.
  - **Read the GPU line on Wayland.** The compositor can pace frame
    callbacks, which made the first runs read 8–18 ms of wall time whatever
    the map; `--measure` prints GPU pass timings for that reason.
  - **The first 64×64 run really was CPU-bound**, at 18 ms: the side panel
    scanned every height each frame. That's cached now.
- **2.2 is a plain `Material`, not an `ExtendedMaterial`.** None of
  StandardMaterial's lighting is used. Terrain data is packed into standard
  attribute slots (`COLOR`, `UV_0`, `UV_1`) so Bevy's own shadow pass draws
  it.
- **Textures** are tileable patterns generated per material (128², mipped)
  and tinted by the material colour. A second, larger and rotated sample of
  each one shades the first, so the 4 m repeat doesn't show as rows at
  grazing angles.
- **Flat shading** was checked by diffing flat and smooth screenshots, which
  show per-triangle facets. A wrong `dpdy` sign would turn the lit ground
  black. Shaders can't run in a headless test, so there's no unit test for it.
- **Water** also has an open-sea frame out to the sky dome. Deep water is
  opaque, so the edge of the map doesn't show through it.

**2.0 Spike: binding per-chunk paint in Bevy 0.19** (S, first)
- Textures bind per material, so a single material can't have per-chunk weight
  textures. Options:
  - **(a) One material instance per chunk.** Simplest; costs batching.
  - **(b) Per-tile texture arrays** (64 layers, well under the 256-layer wgpu
    default) indexed by `MeshTag`.
  - **(c) One atlas.**
- Measure (a) on the largest map with fog off from altitude: triangles, draws,
  shadow cascades. That answers both this question and whether LOD is needed.
  The editor's far plane is 20 km, not WoW's 710 m, so the easy triangle
  budget doesn't hold there.

**2.1 Paint model** (M; with 2.2)
- **Material table:** map-level, with stable IDs. Each entry holds a name and
  preview colour now; texture, tile size and blend settings come later.
  Deleting a material in use asks what replaces it.
- **Chunk layers:**
  - Each chunk has up to 4 layers, keyed by material ID rather than slot
    position.
  - Border texels are stored in every chunk that contains them.
  - A stroke writes all copies together, looking up each chunk's layer by
    material ID, so neighbours never disagree.
- **Paint grid:** 4 texels per cell, fixed: 0.5 m at 2 m cells, WoW's density.
  That's `129²` texels per layer per chunk. At the worst case of 4 layers:
  - 66 KB per chunk;
  - 68 MB for a 32×32 map;
  - 273 MB at the format's 64×64 limit.
  GPU weight textures cost the same again. Density 2 is a quarter of that,
  if 2.0's measurements say so.
- **The 4-material limit:** painting a fifth material into a full chunk is
  refused with a status message. Nothing is silently replaced. The limit also
  shows on the overlay (4.2).
- **v1 migration:** each vertex weight is spread over its texels.

**2.2 Terrain material and lighting** (M)
- `ExtendedMaterial<StandardMaterial, TerrainExt>` with its own lighting in
  the fragment shader. Bevy's PBR uses Burley diffuse and flat ambient, so
  this is custom shading, not a setting.
- **Lighting:**
  - Lambert (optionally half-Lambert) plus hemisphere ambient, no specular.
  - Shadows and fog applied by hand.
  - `Tonemapping::None` with exposure control; both TonyMcMapface and AgX
    shift painted colours.
- **Textures:**
  - A texture array of materials, world-space UVs, height-based blending.
  - Filtering: linear with anisotropy plus the `fwidth` pixel-UV trick for
    crisp texels. Nearest magnification would disable anisotropy and blur the
    ground at grazing angles.
  - Placeholder textures are generated from each material's colour until
    there's art.
- **Flat shading:** in the shader via `cross(dpdy(p), dpdx(p))` on the indexed
  mesh, with a test pinning the `dpdy` sign. A normal prepass (e.g. SSAO)
  would still see vertex normals; note it if one is added.

**2.3 Fog shared by everything** (S)
- One WGSL import for linear fog plus analytic height fog, used by terrain,
  water, sky and later objects. Terrain-only fog would leave the water
  unfogged.

**2.4 Sky** (S)
- A gradient dome with a handful of bands. Fog colour comes from the horizon
  band.
- A map-level light table holds sun, ambient, fog and sky. Zones and time of
  day come later.

**2.5 Water** (S)
- One sea level means water depth is just `level − height`.
- Build a water mesh per chunk with depth per vertex, computed on the CPU.
  That gives shallow-to-deep colour and shore foam with no `DepthPrepass`, and
  the result doesn't change with view angle.

**2.6 Lighter meshes** (S)
- Use `RenderAssetUsages::RENDER_WORLD`. Bevy 0.19 stores each mesh's bounds
  at extraction, so the CPU copy isn't needed for culling.

## Phase 3: cells, tint, baked light — done

What changed from the plan below:

- **Auto diagonals match the ground's curvature.** "Split along the smaller
  height difference" cuts across a ridge that rises along its length, and
  ties on a level one. Instead:
  - On convex ground (summed corner Laplacian below zero) the cell creases
    along its higher corner pair; on concave ground, along its lower pair.
  - Twist or bend under 1 mm is a tie, cut by the checkerboard, so smoothing
    noise can't flip cells.
  - Rule codes are a format promise: 0 checkerboard (v1/v2 and early v3
    files), 1 follow terrain (new maps). The Map panel switches between
    them, undoably.
- **Tint is RGB, not RGBA.** WoW's MCCV alpha was unused, and the four bytes
  of `UV_1` hold r, g, b and AO.
- **AO is per vertex**, since paint is too.
  - The horizon is measured from the surface's tangent plane, so a bare slope
    isn't darkened on top of the hemisphere ambient that already shades it.
  - 8 directions, samples out to 8 cells, no trigonometry.
  - The view caches AO per chunk and bakes it again only after height edits;
    paint, tint, cell and material edits reuse it.
  - Chunk meshes and water build on Bevy's compute pool. One thread does
    0.18 ms per chunk (mesh plus AO) in release, 0.33 ms in dev.
- **Holes:** a ray that goes under the surface inside a hole misses, rather
  than hitting the hole's far wall.
- **Format:** `cells` (one byte per cell, row-major) and `tint` (three byte
  planes) landed inside v3, which hadn't shipped. The v3 golden file is now
  blessed. Unknown cell bits are kept byte-for-byte.
- **Cell brushes** (hole, diagonal, tint) are 4.4.

**3.1 Cell data and triangulation** (S)
- One byte per cell:
  - hole;
  - diagonal mode (2 bits: auto / force A / force B);
  - no ground clutter;
  - unwalkable.
- **Auto diagonal:** splits along the smaller height difference, so it follows
  ridges and valleys. Ties fall back to the checkerboard.
- **Versioned rule:** the rule gets a version number. v1 maps migrate as
  rule 0, the checkerboard, so they look the same.
- **One function:** `height_at`, the mesher and later the game's collision all
  call it.

**3.2 Vertex tint** (S)
- RGBA8 per vertex, multiplier `value / 128`, so 128 is exactly neutral.
  WoW's MCCV.

**3.3 Baked ambient occlusion** (S)
- A horizon pass independent of the sun, at paint density, rebuilt on load and
  never saved.
- An edit dirties everything within the maximum occlusion distance, not just
  its chunk.
- Where it overlaps object shadow cascades, combine with `min`, not multiply.
- A sun-shadow bake waits until the light table and time of day are designed;
  a fixed sun would conflict with them.

## Phase 4: editor tools — done

What changed from the plan below:

- **Masks are one stack** on the brush: height, slope, curvature (Laplacian
  per m², about 2 / R for a bowl of radius R) and noise, each with a soft
  edge and invert. They live in `lofi_world` so the game or a later tool can
  reuse them. Weights are taken for a whole dab before it writes.
- **Auto-paint** is "Paint where masks pass", over the map or the selection,
  as one undo step. Weights under 0.1 are skipped so faint edges don't claim
  chunk slots, and chunks without room are counted and reported.
- **Overlays:**
  - the slope (50°, `WALKABLE_SLOPE_DEGREES`) and the chunk grid are in the
    terrain shader;
  - full chunks and flagged cells are drawn as lines, capped at the 64
    nearest chunks and 4000 cells within 64 cells of the camera.
  - A flag texture is the upgrade if these ever show up in a profile.
- **Heightmaps:**
  - The sidecar keeps the extension (`island.png.json`) and is replaced, not
    left stale. PNGs also carry the range in a `tEXt` chunk.
  - Sizes are checked against the file before reading, up to 4097 a side.
  - Import is a modal: range, fit (stretch, or place and crop/extend), and an
    optional vertical flip. Shrinking by 2× or more averages instead of
    aliasing.
- **Cell tools** are two tools: Tint, and Cells, which sets a hole, a forced
  diagonal, no-clutter or unwalkable. The brush cursor treats holes as solid
  so a hole can be clicked to fill it.
- **Selection** is a rectangle or lasso, rasterised to a vertex mask when it
  changes. It's editor state: not saved, not undoable.
- **Undo** now reports, per chunk, whether heights, cells or paint/tint
  changed, so undoing paint doesn't rebake AO.

- **4.1 Brush masks and auto-paint.**
  - A stack of masks (height band, slope band, concavity, noise) that
    multiplies into any brush.
  - Run over the map or a selection, the same masks become paint rules.
- **4.2 Overlays.** Slope/walkability, chunk grid, chunks at the 4-material
  limit.
- **4.3 Heightmap import and export.**
  - Formats: 16-bit PNG, R16, and R32 for lossless round trips.
  - A JSON sidecar holds the metre range, cell size and grid size.
  - Resample or crop is an explicit choice: other tools use power-of-two
    sizes, ours are 32·N + 1.
- **4.4 Cell brushes.** Hole, diagonal, tint.
- **4.5 Selection.** Rectangle and lasso, to scope masks and auto-paint.

## Phase 5: game runtime and next tools — done

What changed from the plan below, and what was decided along the way:

- **No LOD.** The format's largest map, 64×64 chunks, draws in 4.3 ms.
- **Cooked format** (`lofi_world::cook`, `lofi_cook`, File > Cook for
  game…):
  - a manifest plus one file per tile (4×4 chunks by default), each with the
    map header style, crc32 and zstd;
  - each chunk is self-contained:
    - heights in 1 cm steps above its lowest point, on one global step;
    - a chunk spanning more than 655 m switches to 4-byte steps rather than
      failing;
    - diagonal, hole and flag bits;
    - octahedral normals, paint, and tint plus baked AO;
  - collision from a cooked chunk matches the editor's surface to within
    half a step (tested at random points);
  - cooking is deterministic, writes a staging folder and swaps it in, and
    only deletes an earlier cook's files. The earlier cook is moved aside
    and deleted last, so a failed swap leaves it whole;
  - the manifest records the source revision and whether it was unsaved.
- **Objects** live in `lofi_world`:
  - `ObjectId`s are never reused, and assets are named by `AssetGuid`;
  - they're stored in an optional v3 `objects` section, so older v3 builds
    keep them;
  - the section carries its own format number: a newer one is kept as-is and
    left uneditable;
  - undo restores the list, never the next id, so an undone placement's id
    stays spent;
  - a second golden file, `v3-objects.lofimap`, pins the section.
- **Shared editor crate** `lofi_editor`: everything but the terrain tools
  moved there.
  - Tools add a panel in `UiSystems::Panels`.
  - The ground pick is shared as `GroundCursor`.
  - Panel undo steps are sealed by the shell.
- **One editor per map, enforced.** Opening or saving takes an OS lock on
  `.<map>.lock`; a second session is refused. Untitled autosaves are locked
  by their session, so other sessions don't offer them back. Saving over a
  file changed on disk since it was opened is refused.
- **Second tool** `lofi_doodads`:
  - place, select, drag along the ground, turn, scale, drop to ground and
    delete, with undo;
  - picking uses bounding spheres centred on each shape and stops at the
    ground;
  - objects use placeholder low-poly meshes until there's art. Every tool,
    the terrain editor included, draws them with shadows.

- **5.1 Cooked runtime format.**
  - u16 heights on one global fixed step (1 cm), offset from each chunk's
    minimum. Per-chunk scales would put shared border vertices at different
    heights and crack the seams.
  - Diagonal choices are baked as bits, so collision matches the editor even
    after quantising.
  - Normals and lighting are computed at cook time.
- **5.2 Streaming by tile** in the cooked output, not the editor file.
- **5.3 Second tool (doodads and buildings).**
  - Extract a shared editor crate first: camera, terrain view, dialogs, undo.
  - Placed objects get stable u64 IDs allocated map-wide and asset GUIDs, not
    paths.
- **LOD, only if 2.0's measurements ask for it:** 2–3 discrete meshes per
  chunk with `VisibilityRange` crossfade, plus skirts. No CDLOD or clipmaps:
  they make facets swim.

## Phase 6: the terrain describes itself — terrain half done

The terrain is the skeleton, so it has to tell a tool or the game where its
ground is and how to collide with it.

- **`Ground`** (`lofi_world::ground`), one set of queries:
  - `ground_at` / `point_at`: height, the normal of the triangle underfoot,
    walkable or not (the cell's flag and the 50° slope the editor's overlay
    shows), and the no-clutter flag. `ground_at` tells a hole from a place
    with no data, so a character over a tile still streaming in isn't told
    to fall;
  - `trace_ray` / `cast_ray`: it walks the cells under the ray, watching the
    ray's height over the surface change sign, so nothing can be stepped
    over or slipped between. Holes are open to characters, solid to the
    editor's cursor. `trace_ray` also says when a ray ran into ground that
    isn't loaded, which is not a clear line;
  - `chunk_bounds` and `chunk_collision`: a chunk's box, and its triangles
    with holes left out, ready for a triangle-mesh collider.
- **One implementation.** An implementor supplies vertex heights and cell
  flags; the answers are computed once, in `ground`. `Terrain::height_at`,
  the editor's cursor and cooked chunks all go through the same
  interpolation (`cell_plane`). The old quarter-cell ray march is gone.
- **The cooked world answers too.** `CookedGround` holds whichever tiles are
  loaded and implements `Ground` across them: rays and characters cross tile
  seams, and unloaded ground reads as unknown.
- **Position.** `TileLayout` says how the world is cut into tiles, which tile
  a point is in, each tile's rectangle, and which tiles are within a radius
  (nearest first, for streaming). The manifest carries each tile's height
  range, checked when the tile loads, so `Manifest::tile_bounds` boxes every
  tile without opening one.
- **Cook format 2.** A version-1 cook is refused with "cook the map again".
- `lofi_cook` prints the world's bounds, and `lofi_cook --probe X,Z DIR`
  asks a cook what its ground is at a point. `examples/cooked_walk.rs` walks
  a cook as a game would, streaming tiles around the walker.
- The editors' status bar shows the slope under the pointer and whether the
  game will let a character walk there.
- Measured: a ray from one corner of an 8×8-chunk map to the other takes
  about 2 µs in release.

Not done, waiting on a decision: whether placed objects (their data, undo,
view and cook) should leave `lofi_world` and `lofi_editor` for a crate of
their own, and whether the terrain editor should stop drawing them. Nothing
about objects changed in this phase.

Open: a map sits at the origin of its own space. If several maps are to
tile into one world, each needs a world offset.

## Later, maybe

- **Splines and shapes** (roads, rivers, plateaus as non-destructive
  features).
  - Every reader (brushes, raycast, mesher, bake, masks) would have to choose
    between base and composite heights, so it waits until after the game
    exists.
  - The composite is derived and never saved.
- **Erosion, terracing, ridged noise.**
  - Possibly off-style: WoW Classic terrain was hand-sculpted.
  - runevision's filter is MPL-2.0, a per-file licence that doesn't block a
    closed product.
  - Noggit is GPL-3: use it as a reference only, port nothing.
- **Low internal resolution** with nearest upscale. Picking must remap the
  cursor.
- **Outlines.**

## Deliberately skipped

- **PS1 vertex snapping and affine texturing:** wrong era.
- **Dithering and palette quantisation:** fights painted colour.
- **Hard cel bands on terrain:** they draw contour lines on hills.
- **Terrain rendering tricks:**
  - Meshlets: for dense meshes.
  - Occlusion culling: until buildings exist.
  - GPU vertex pulling and virtual texturing: runtime scale, not editor.
- **Not copied from WoW:** 8×8 centre heights; stored normals; 4-bit alpha;
  per-tile duplicate object records; filename references.
- **Cut from the first draft:** 8 paint slots with 4 rendered; variable paint
  density; tiles inside the editor file; a sparse-tile bitmap; per-section
  xxh3 (zstd checksums its frames); saving the height composite.

## Testing

- **Unit tests** in `lofi_world` for every data rule: ownership, triangulation,
  paint normalisation.
- **Format:** golden files per version, round-trip property tests, and a
  fuzzed reader.
- **Headless ECS tests** for editor flows, like `sculpt::stroke` today.
- **Screenshot regression:** `--screenshot` on the demo island, compared with
  a tolerance.

## Order

1. ~~Phase 1~~ done.
2. ~~Phase 2~~ done.
3. ~~Phase 3~~ done.
4. ~~Phase 4~~ done.
5. ~~Phase 5~~ done.
6. Phase 6: the terrain half is done; objects wait on a decision.
7. Next: a game that loads the cooked tiles and collides with `CookedGround`;
   ground clutter driven by paint and the no-clutter flag. Asset collision
   and position, in their own crate.

## Decisions

- **One editor per map at a time**, so one file per map.
- **v1 files keep loading** through the migration; saving converts them.
- **Paint density stays per-vertex for now.** 2.0's numbers leave GPU room
  for 4 texels per cell (WoW's density) or 2. The cost would be memory and
  file size (2.1).

## Review notes

A staff-level review of the first draft found:

- **Paint slots vs shared borders (critical).** Slot-indexed weights on
  shared border texels let neighbours read different materials, and "replace
  the least-used slot" silently erased paint. *Fixed in 2.1:* layers keyed by
  material ID, borders duplicated and written together, the fifth material
  refused.
- **Paint memory.** The file-size numbers left paint out, and per-chunk `Arc`s
  made sculpting copy paint. *Fixed:* 2.1 numbers, separate `Arc`s per type
  (1.1), measurement in 1.2 and 2.0.
- **Bevy binding and filtering.** Per-chunk textures can't share one material,
  and nearest filtering kills anisotropy. *Fixed:* 2.0 spike, and 2.2 uses
  linear with anisotropy plus `fwidth`.
- **Big-bang Phase 1.** It locked paint storage before its renderer existed.
  *Fixed:* reordered.
- **Over-built container.** Tiles, bitmap, xxh3 and C zstd. *Fixed:* cut;
  length checks, critical flags and unknown-section preservation added; the
  multi-editor question is now yours.
- **Triangulation and quantising.** Collision drifted after cooking, flip bits
  changed meaning, and per-chunk scales cracked seams. *Fixed:* 3.1 and 5.1.
- **Lighting size.** It was sized too small. *Fixed:* moved into 2.2, fog
  shared (2.3), `Tonemapping::None`.
- **Baked light.** Wrong dirty region, too coarse, and a conflict with time of
  day. *Fixed:* 3.3.
- **Saving.** Windows, `.bak` order, untitled autosave, recovery by
  generation, and background-save races. *Fixed:* 1.3.
- **LOD numbers.** They used the game's far clip, not the editor's. *Fixed:*
  measured in 2.0.
- **Splines.** They touch every reader. *Moved* to Later.
- **Smaller:** water depth on the CPU (2.5), no `NoAutoAabb` needed (2.6),
  exact tint neutral (3.2), material deletion and map-level undo (1.1, 2.1),
  and a testing section.
- **Erosion (partly accepted).** The review said cut it as off-style. Deferred
  instead: it may suit specific zones, and it costs nothing to keep on the
  list.

A code review of the finished Phase 1 found 11 problems. All are fixed and
described under 1.3. The worst:
- an autosave could overwrite another session's crash work;
- edits made while the Open dialog was up were dropped;
- an autosave was deleted when the file had changed on disk;
- one read-only `.bak` blocked every later save.

A code review of the finished Phase 2 found no data loss. Fixed:
- **Opening a map marked it unsaved.** egui's colour buttons round-trip
  through HSV and its sliders clamp, just by drawing. The panel now uses
  widgets that write back only on an edit, and shows colours as sRGB.
- **Black texels** where an unpainted slot's texture outranked every painted
  one in the height blend. Only painted slots compete now, and unpainted
  ones aren't sampled.
- **Ctrl-erase was stuck** in a full chunk without the base material. It
  falls back to a material the chunk already has.
- **Files whose shared border paint can't agree** are refused, instead of
  loading with a seam nothing could paint over.
- **Water depth was cut on the wrong diagonal** in half the shoreline cells.
  It now uses the terrain's triangles.
- **Needless rebuilds:**
  - sea level is a uniform, so dragging it rebuilds nothing;
  - generated textures are cached per pattern;
  - adding a material remeshes nothing;
  - the panel's height range is cached instead of scanning the map every
    frame.
- `replace_material(m, m)` corrupted weights.

A code review of the finished Phase 3 found nothing critical. Fixed:
- the panel's height range rescanned the map every frame of a stroke; it
  now waits for the stroke to end;
- undo rebaked AO for any change; it now marks heights, cells and paint
  separately;
- out-of-range cell or vertex coordinates wrapped into another chunk; they
  now panic.

A code review of the finished Phase 4 found one crash. Fixed:
- **Stretching an imported heightmap could index past the end:** the last
  sample's position rounded past the edge in f32. Resampling now works one
  axis at a time in f64, averaging where an axis shrinks by half or more and
  interpolating where it doesn't. A sweep test covers the failing sizes.
- **The Cells tool missed cells** whose centre was in the brush but whose far
  corner wasn't. Its footprint is now built from cell centres.
- **A selection outlived the map it was made on** for one frame. It's now
  cleared right after a map opens.
- **The full-chunk overlay rescanned every frame of a stroke**, and
  `ChunkPaint::materials` scanned weights for empty slots.
- **R32 heights past a million metres** overflowed resampling; they're
  refused.
- **Exporting** to a name the dialog hadn't asked about overwrote it; that's
  refused now.

A code review of the finished Phase 5 found nothing critical. Fixed:
- **A recook deleted the old cook before the new one was in place**, so a
  failed swap left neither. It's now moved aside and deleted last. Cooking
  through a link fills the folder it points at.
- **A map whose objects came from a newer tool cooked to no objects.** It's
  refused.
- **Clicking an object to select it moved it**: onto the ground if it had
  been raised, and by a hair otherwise, which recorded a step and marked the
  map unsaved. A press is now a drag only after 4 pixels, a dragged object
  keeps its height above the ground, and placing snaps to the surface.
- **The same file by another path counted as another editor's.** Paths are
  resolved once, on open and save.
- **`--screenshot` and `--measure` of a map open elsewhere** captured the
  default map and exited 0. Captures now read without locking, and exit 1
  if the map doesn't load.
- **Object data from disk:**
  - a next id of 0 lost the next object placed;
  - running out of ids overflowed;
  - a newer `objects` section in an unknown codec, or bigger than today's
    layout allows, made the map unreadable. It's kept as it is.
- **Two sessions could be offered the same untitled autosave.** It's locked
  from the moment it's offered. Lock files left behind there are swept.
- **The selection survived a map switch** and pointed at the new map's
  object with that id.
- **The cooked reader** accepted absolute tile paths, tiles with missing,
  doubled or stray chunks, and objects at NaN.
- **A cooked tile had no ground on the map's far edge** at some cell sizes.
- **Edits that changed nothing** (scaling past the limit, placing where
  there's no room) recorded an undo step and marked the map unsaved.
- **Staging**: a leftover staging folder was deleted whatever it held; now
  only a cook's own files go. Two cooks into one folder are refused.
- **Backspace deleted the selected object.** Only Delete does.
- **Settings steps in undo** each counted the whole object list against the
  budget, though they share it.

Left as it is: every object edit copies the object list and the view walks
all of it. That's about 5 MB and 100k lookups per drag frame at 100k
objects. Bucketing objects by chunk, like terrain, is the fix when maps get
that dense.

The staff review of the Phase 6 plan changed its scope and its tests:
- moving object storage out of the terrain crates read more into the
  direction than it said, so that half waits for a decision;
- a three-way answer under a point (ground, hole, no data);
- tile bounds stored as height steps and checked on load;
- old cooks refused by version;
- no "cooked ray within 1 cm of the editor's" test (a grazing ray moves far
  for a small height change), and a test that can see a ray slipping between
  triangles, which brute force with the same triangle test can't.

A code review of the finished Phase 6 terrain half found nothing critical.
Fixed:
- **Long rays slipped through the ground.** The first version tested each
  cell's two triangles; over a few hundred cells, float error outgrew the
  edge tolerance and a ray landing near an edge was rejected by both. Up to
  0.4% of rays over 1,000 cells long, aimed at vertices. The ray is now a
  sign-change walk with no triangle test and no tolerance to outgrow. The
  same change fixed rays under the ground reporting a hit, and vertical rays
  missing a needle-thin spike.
- **A ray couldn't say "unknown".** Over partly loaded ground it skipped
  unloaded cells as air and could report a hit the whole world would have
  blocked earlier. `trace_ray` now stops there and says so.
- **A ray cast from a point the ground had just returned missed** about half
  the time, by rounding. Starting on the surface now counts as on it.
- **On a loaded tile's far edge** `ground_at` found ground and a ray
  straight down didn't. The ray takes the loaded side too.
- **`tile_at` and the ground disagreed on a tile border** for cell sizes
  that aren't exact in binary. The tile now comes from the cell index, and
  tile rectangles share their edges exactly.
- **A cooked chunk deserialised by hand** with a short blob panicked in an
  accessor. Chunks are checked as they're deserialised, however that's done.
- **Absurd rays** (origin at 1e38, endless) could return NaN. They don't.

The staff reviews of the Phase 4 and 5 plans shaped those phases before they
were built:
- per-cell picking through holes;
- rasterised selections;
- curvature per metre;
- stale-sidecar handling for heightmaps;
- per-kind undo marks;
- map locks;
- the optional, versioned objects section;
- object ids out of undo;
- shader paths that follow the crate name;
- staged cooks.

## Sources

- **WoW data and lighting:** https://wowdev.wiki/ADT/v18,
  https://wowdev.wiki/WDT, https://wowdev.wiki/DB/Light,
  https://wowdev.wiki/DB/LightIntBand
- **WoW map viewer shaders:** https://github.com/purpleKarrot/wowmapview
- **Noggit:** https://gitlab.com/prophecy-rp/noggit-red,
  https://github.com/wowdev/noggit3/issues/100
- **Splat blending:**
  https://www.gamedeveloper.com/programming/advanced-terrain-texture-splatting
- **Bevy array textures:**
  https://raw.githubusercontent.com/bevyengine/bevy/v0.19.0/examples/shader/array_texture.rs
- **Flat normals in shader:**
  https://github.com/TokisanGames/Terrain3D/discussions/435
- **Horizon AO:** https://karim.naaji.fr/lsao.html
- **Fog:** https://iquilezles.org/articles/fog/
- **Pixel-art filtering:** https://jorenjoestar.github.io/post/pixel_art_filtering/
- **Toon water:** https://roystan.net/articles/toon-water/
- **Visibility ranges:**
  https://docs.rs/bevy/latest/bevy/camera/visibility/struct.VisibilityRange.html
- **Brush filter stacks:**
  https://docs.unity3d.com/Packages/com.unity.terrain-tools@5.2/manual/create-filterstacks-and-filters.html
- **Landscape splines:**
  https://dev.epicgames.com/documentation/unreal-engine/landscape-splines-in-unreal-engine
- **Erosion:** https://blog.runevision.com/2026/03/fast-and-gorgeous-erosion-filter.html,
  https://frozenfractal.com/blog/2025/6/6/around-the-world-23-hydraulic-erosion/
- **Far Cry 5 terrain:**
  https://media.gdcvault.com/gdc2018/presentations/TerrainRenderingFarCry5.pdf
- **Quantized mesh:** https://github.com/CesiumGS/quantized-mesh
- **Terrain3D control map:**
  https://terrain3d.readthedocs.io/en/latest/docs/controlmap_format.html
