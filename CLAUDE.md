# lofi 3d

A Cargo workspace of tools for building low-fidelity fantasy worlds in the
style of WoW Classic. Rust edition 2024, Bevy 0.19.1, bevy_egui 0.42 (egui
0.36). Toolchain pinned in `rust-toolchain.toml`.

The terrain comes first: these tools build the skeleton of the world, and
the terrain describes itself (collision and position) to whatever uses it.
Small assets' collision and position belong in a separate crate, not here.

## Layout

- `crates/lofi_world`: shared map data. Terrain, materials and paint, cells
  and tint, objects and the placeholder asset catalog, water, lighting,
  `.lofimap` format, heightmap import/export, masks, chunk meshing, AO,
  `ground` (collision and placement queries), noise, and `cook` (the game's
  tiled format, plus the `lofi_cook` binary). Depends on `bevy_math` only,
  never `bevy`, so it tests headless in milliseconds. Anything a tool or the
  game shares belongs here.
- `crates/lofi_editor`: the shell every editor builds on (`app::editor_app`).
  Modules by feature:
  - `document`: the map being edited, undo, file actions and dialogs,
    autosave, crash recovery, file locks, heightmap import/export, cooking.
  - `view`: chunk meshes, water, sky, objects, overlays; their materials and
    WGSL in `view/shaders`; generated material textures.
  - `camera`: fly camera, framing. `pick`: the ground under the pointer.
  - `ui`: menu and status bars, prompts, shortcuts, `widgets`.
  - `capture`: `--screenshot`, `--measure`. `launch`: command line.
- `crates/lofi_terrain`: the terrain editor: `sculpt` (tools, brushes, masks
  applied to brushes, strokes, auto-paint), `select` (rectangle, lasso),
  `panel` (its side panel).
- `crates/lofi_doodads`: the doodad editor: `tool` (place, pick, drag, keys),
  `pick`, `draw`, `panel`.
- A tool adds its plugins and its side panel, in `UiSystems::Panels`, to
  `editor_app`. Tool-specific things stay out of `lofi_editor`.

## Conventions

- `plugin.rs` registers only. `mod.rs` declares modules and re-exports only.
- Group files by feature, not by kind. Split files past ~300 lines.
- Messages use `#[derive(Message)]` and a `Message` suffix.
- The map is a document, not entities: `MapDocument` is a resource. Chunk
  entities are a view of it, rebuilt from `TerrainChanges`.
- Only call `MapDocument::map_mut` when something actually changes; it bumps
  the revision, which marks the map unsaved. In UI code, edit a copy and write
  back on change.
- Every change must be undoable: terrain edits and drags go through
  `EditHistory::begin`/`capture`/`capture_fields`/`commit` (undo is refused
  until commit); one-off settings or object changes through
  `record_map_edit`, then `seal()` if they shouldn't merge with the next.
  Panel edits merge until the shell seals them when the UI goes idle.
  `begin` replaces a recording in progress, so one-shot edits (auto-paint,
  import, delete material) check `is_recording` first.
- Object ids only grow. Undo restores the object list, never the next id.
- An edit that may turn out to change nothing (clamped, refused) runs on a
  copy and is recorded and written back only if it did: see `edit_objects`
  in `lofi_doodads`. Recording first leaves an empty undo step and an
  unsaved map.
- Per-map editor state (a selected object, a selection) is cleared when
  `MapDocument::generation` changes: ids start over in every map.
- Brush weights are computed for the whole dab before any write: falloff ×
  selection × masks. Masks then read the ground as it was.
- The selection is editor state: not saved, not undoable, cleared on a new
  map.
- System order in `Update` is the `EditorSystems` chain in `schedule.rs`.

## Terrain storage

Each chunk stores the heights and tints it owns (`GridSize::owner`): its west
and north edges, plus the closing edge for the last chunk in each direction.
Cells belong to exactly one chunk (`Terrain::cell_chunk`). Paint is
different: each chunk holds its full 33×33 vertex rect, so border vertices are
stored in every chunk that has them, and `set_vertex_paint` writes all the
copies or none.

Data is an `Arc` per type per chunk, so cloning a `Terrain` is cheap and
writes copy only the chunk and type they touch. Undo keeps old `ChunkData` and
swaps it back; autosave clones the whole map as its snapshot. Don't hand out
`&mut` to raw sample storage; go through the `set_*` methods and
`paint_vertex`.

How a cell splits into triangles is decided in one place,
`Terrain::splits_on_main_diagonal`. The mesher, `height_at`, the raycast and
water all go through it; anything new that needs the surface must too. Each
`Triangulation` rule is a file-format promise: never change one, add a new
one.

## Ground

`lofi_world::Ground` is what collision and placement ask: `ground_at` (height,
the normal underfoot, walkable, or a hole, or no data), `trace_ray` /
`cast_ray`, `chunk_bounds`, `chunk_collision`.

- An implementor supplies `vertex_height` and `cell_shape` only. Every answer
  is computed in `ground`, so `Terrain` (editor) and `cook::CookedGround`
  (game) can't disagree beyond the cook's 1 cm height step. A cell's surface
  is interpolated in one function, `ground::cell_plane`.
- Anything new that asks where the ground is goes through `Ground`, not its
  own interpolation or its own ray march.
- `None`/`Unknown` means no data (off the map, tile not loaded). It is not a
  hole, and for a ray it is not a clear line: keep them apart. `cast_ray`
  folds unknown into `None`; anything that must not change as tiles stream
  in uses `trace_ray`.
- The ray is a walk over the cells under it, watching the ray's height over
  the surface change sign. Don't replace it with per-triangle tests (long
  rays slip between triangles once float error outgrows the edge tolerance)
  or a fixed-step march (steps over spikes); both were tried.
- Rays hit the surface from above only. A ray starting on the surface and
  heading down hits at distance 0.
- A cooked chunk is checked as it's deserialised (`try_from`), so its
  accessors can't index out of bounds. Keep new blobs in that check.
- Where the cooked terrain is comes from `cook::TileLayout` (tile of a point,
  a tile's rectangle, tiles within a radius) and `Manifest::tile_bounds`.
  Anything that sorts things into tiles asks the layout.

A chunk's paint has four slots of material ids in canonical order (ascending,
empty last), and each vertex's weights sum to 255. Every `ChunkPaint` method
that writes keeps both true; anything new that writes must too.

## Rendering

- Terrain, water and sky are plain `Material`s with their own lighting. Bevy's
  PBR isn't used, `Tonemapping` is `None`, and the sun's illuminance is
  divided by exposure so the light table's colours land on screen as painted.
- Terrain data rides in standard attribute slots so Bevy's shadow prepass
  works unchanged: `COLOR` holds weights, `UV_0` the four material ids, and
  `UV_1` tint and AO, two bytes per float. See `view/terrain_mesh.rs` and the
  header of `terrain.wgsl`.
- Fog is one WGSL import (`lofi::fog`) used by every surface. Anything new that
  draws in the world must use it too.
- Mark terrain edits in `TerrainChanges` by what they touch: heights (rebakes
  AO up to `AO_RADIUS_CELLS` away, rebuilds water), shape (cell flags:
  triangles and water), surface (paint, tint). The view caches baked AO per
  chunk and rebuilds meshes on the compute pool.
- In the terrain shader, take derivatives (`dpdx`, `fwidth`) in uniform
  control flow and pass them in: layers are sampled inside per-pixel
  branches.
- egui colour buttons and default sliders rewrite values just by drawing
  them. In panels that edit the map, use `lofi_editor::ui::widgets`.
- Shader paths are built from `env!("CARGO_CRATE_NAME")`, because
  `embedded_asset!` files shaders under the crate's name.
- Objects are one entity each, synced from the object list only when its
  `Arc` changes. Placeholder meshes come from `view/shapes.rs`; their colours
  are baked into the vertices, so all objects share one material.

## Map format rules

`.lofimap` v3: header (magic, version, minimum reader version, body crc32),
MessagePack metadata, then named sections of zstd-packed, crc32-checked bytes
(`heights`, `paint`, `cells`, `tint`, and optional `objects`; see the top of
`file/v3.rs`). See `lofi_world::file`. `v1.rs` and `v2.rs` are frozen
readers; don't change them. v3 has golden files (`v3.lofimap`,
`v3-objects.lofimap`), so its layout is frozen too.

- New per-object data goes in a new section keyed by object id, never into
  the `objects` records. A newer `objects` format, or one this build can't
  unpack, is kept as unknown data and the objects are left uneditable
  (`ObjectLayer::is_foreign`). Foreign objects are never cooked.
- One editor per map: opening or saving to a file takes its lock
  (`document/lock.rs`); a second session is refused. Saving over a file that
  changed on disk since it was opened is refused too. Paths go through
  `lock::resolved` first, so one file has one spelling. Capture runs
  (`--screenshot`, `--measure`) read without locking.
- Cooked output is a build artefact: cook again rather than migrate it. A
  change to what a cook holds bumps `COOK_VERSION` and `OLDEST_READ`, so an
  old cook is refused by version, not as damage. A cook writes a staging
  folder and swaps it in, moving the earlier cook aside until the new one is
  in place. It only ever deletes a cook's own files, by name.
- What a manifest claims about a tile (size, crc, height range) is checked
  against the tile when it loads.

- Maps must keep loading. New metadata fields get `#[serde(default)]`.
- Older editors drop metadata they don't know when they save, so new
  tool-owned data goes in a new section, which they keep byte-for-byte. Mark a
  section critical only if an editor ignoring it would corrupt the map.
- Bump `MAP_FORMAT_VERSION` only when an old reader would misread the file.
  Keep a reader for every older version and bless a golden file for the new
  one: `cargo test -p lofi_world --test golden -- --ignored bless`. Golden
  files are never rewritten.
- Validate everything read from disk before allocating on its say-so;
  `Terrain::from_parts` and `ChunkPaint::from_parts` are the final gates.
  `tests/format_robustness.rs` truncates and corrupts files and must never
  panic. Heightmap import checks sizes against the file before reading it.
- Saves go through `save_map`: temp file, fsync, `.bak` copy, rename. Never
  write a map in place.
- Autosave only ever writes or deletes the sidecar it owns (`Autosave::owned`).
  Any other sidecar may be another session's unsaved work: offer it, never
  delete or overwrite it on a guess.
- Anything that replaces the document calls `Autosave::retire` first and
  clears `EditHistory`.
- ruzstd only implements `CompressionLevel::Fastest`; the other levels panic.

## egui gotcha

Through bevy_egui, egui 0.36 panels are drawn into a `Ui` and do not count as
"pointer over egui". `ui::layout` tracks the rect the panels leave free, and
`ScenePointer` is `Some` only over that rect. Anything that reacts to the
mouse in the 3D view must read `ScenePointer`, not the raw cursor.

## Checks

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

To see a rendering change, generate the demo map and screenshot it:

```sh
cargo run -p lofi_world --example demo_island -- /tmp/demo.lofimap
cargo run -p lofi_terrain -- /tmp/demo.lofimap --screenshot /tmp/shot.png
```

`--camera EX,EY,EZ,TX,TY,TZ` picks the view, `--flat` the shading and
`--overlays` the overlays. The demo island has a hole at about (306, 256), is
tinted throughout, and has about 150 trees, rocks and a hut. `lofi_doodads`
takes the same options. `examples/measure_mesh.rs` and `measure_format.rs`
time meshing and files. To look at a generated texture on its own:
`LOFI_TEXTURE_DUMP=/tmp/patterns cargo test -p lofi_editor dump_patterns -- --ignored`.

For performance, `--measure` in a release build prints frame and GPU pass
times. On Wayland the frame time is the compositor's pacing; read the GPU line.

## Docs

- `docs/PRIMER.md`: how the workspace fits together. Each crate has a
  `README.md` (what it is, how to use it) and a `PRIMER.md` (how it works
  inside). `docs/ROADMAP.md` is the history: what each phase built, measured
  and got reviewed for.
- The primers describe mechanisms and name files and functions. A change
  that makes one wrong updates it in the same change.
- Use Context7 for Bevy/egui docs, but check against the sources in
  `~/.cargo/registry/src` — most Bevy code online predates 0.19.
