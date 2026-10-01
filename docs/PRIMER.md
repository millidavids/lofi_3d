# How lofi 3d fits together

Start here. This is the map of the workspace: what each crate is for, how
they depend on each other, and what happens between a mouse press and a file
on disk. Each crate then has its own primer that goes into how it works.

## The crates

```text
lofi_terrain (bin)      lofi_doodads (bin)        the tools
        \                   /
         lofi_editor (lib)                         the shell they share
               |
         lofi_world (lib + lofi_cook bin)          the data; no engine
               |
         bevy_math, serde, zstd, png …
```

| Crate | One line | Read |
|---|---|---|
| `lofi_world` | Map data, the file formats, ground queries, cooking. Depends on `bevy_math` only. | [README](../crates/lofi_world/README.md) · [primer](../crates/lofi_world/PRIMER.md) |
| `lofi_editor` | The editor shell: document, undo, files, autosave, camera, drawing, UI frame. | [README](../crates/lofi_editor/README.md) · [primer](../crates/lofi_editor/PRIMER.md) |
| `lofi_terrain` | The terrain editor: brushes, selection, panel. | [README](../crates/lofi_terrain/README.md) · [primer](../crates/lofi_terrain/PRIMER.md) |
| `lofi_doodads` | The doodad editor: place and move objects. | [README](../crates/lofi_doodads/README.md) · [primer](../crates/lofi_doodads/PRIMER.md) |

The dependency rule runs one way. `lofi_world` knows nothing about Bevy,
windows or tools, which is why its tests run headless in a few seconds and
why a game can use it without pulling in an editor. Tools never talk to each
other; they meet in the `.lofimap` file.

## Reading order

1. **`lofi_world` primer, sections 1 to 3**: the grid, how terrain is
   stored, how cells become triangles. Everything else leans on these.
2. **`lofi_editor` primer, sections 1 to 4**: the document, the frame, how
   the view learns what changed, undo.
3. **`lofi_terrain` primer**: a stroke end to end. It's the best worked
   example of the two above.
4. Then whatever you're curious about: the file format and cooking
   (`lofi_world` 10 and 13), ground queries (`lofi_world` 6), rendering
   (`lofi_editor` 9).

## The ideas the whole thing rests on

**The map is a document.** One `WorldMap` value in one resource. The scene
you see is rebuilt from it; nothing is read back out of the scene. Undo,
save, autosave and cook all operate on that one value.

**Copy-on-write, per chunk, per kind of data.** Each chunk's heights,
paint, cells and tint sit in separate `Arc`s. Cloning a map is a handful of
pointer copies, and a write copies only the one array it touches. That one
decision is what makes undo cheap (keep the old chunk), autosave
non-blocking (clone, write on another thread), and cooking safe to run while
editing.

**Decided in one place.** How a cell is cut into triangles, and how its
surface is interpolated, each live in exactly one function. The mesher, the
height under a point, the ray, the water and the cook all call them, so they
can't drift apart.

**Changes are announced, not discovered.** Whoever edits terrain says what
they touched and what kind of change it was. The view rebuilds exactly that.

**The revision counter is the truth about "unsaved".** Getting `&mut` to the
map bumps it. So code is written to not touch the map unless something
really changes.

**Files never lose data.** Saves and cooks write beside the target and
rename into place. Readers validate before they allocate and never panic.
Sections a build doesn't understand are kept and written back.

## Life of a brush stroke

What happens when you drag the Raise tool across a hill, by frame stage:

```text
Input    ui::update_scene_pointer   pointer is over the 3D view → ScenePointer
Cursor   pick::update_ground_cursor ray from the camera → Terrain::raycast_solid → GroundCursor.hit
Stroke   sculpt::run_stroke
           press:  EditHistory::begin("Raise / Lower")
           frame:  footprint = Terrain::vertices_near(hit, radius)
                   EditHistory::capture(footprint)      keeps those chunks' old data (pointer copies)
                   MapDocument::edit_terrain(apply_dab) weights = falloff × selection × masks
                                                        Terrain::set_height → Arc::make_mut copies
                                                        the chunk's heights, once
                                                        revision += 1
                   TerrainChanges::mark_heights(footprint)
           release: EditHistory::commit                 drops chunks that didn't change
Edit     (autosave timer ticks; undo would run here)
View     view::remesh_changed_chunks
           chunks within 8 cells of the footprint → jobs
           per job, on the task pool: lofi_world::chunk_ao + build_chunk_mesh → Bevy Mesh
         view::sync_water                              rebuilds water over those chunks
UI       status bar shows height, slope, walkable; title shows "*"
```

Later:

- **Ctrl + Z**: `HistoryMessage::Undo` → the stored chunk data is swapped
  back in → `Reverted` says heights changed in that rectangle → same remesh.
- **Two minutes on**: autosave clones the map and writes
  `zone.lofimap.autosave` on another thread.
- **Ctrl + S**: `lofi_world::save_map` writes a temp file, fsyncs, copies
  the old file to `.bak`, renames. The autosave is deleted. `saved_revision`
  catches up and the "*" goes.

## Life of a map file

```text
lofi_terrain / lofi_doodads
        │  save_map                      .lofimap: header, MessagePack meta,
        ▼                                compressed checksummed sections
   zone.lofimap  ◄── load_map ──  (v1, v2 files still read; saving writes v3)
        │
        │  lofi_cook, or File > Cook for game…
        ▼
   cooked/world.lofiworld          manifest: grid, materials, light, tile index + bounds
   cooked/tiles/t_x_z.lofitile     self-contained chunks: 1 cm heights, baked diagonals,
        │                          normals, paint, tint, AO; plus the tile's objects
        ▼
   the game: CookedWorld::open → TileLayout::tiles_within → load_tile → CookedGround
```

The editor file is one file tuned for editing: full-precision heights,
everything in it. The cooked output is many small files tuned for a game:
quantised, self-contained per chunk, streamable by position. Cooked output
is a build artefact; it's regenerated, never migrated.

## Where to look for a thing

| Question | Go to |
|---|---|
| Where is a vertex's height stored? | `lofi_world/src/grid.rs` (`owner`), `terrain.rs` |
| Why did this cell get cut that way? | `lofi_world/src/cells.rs` (`splits_on_main`) |
| Why was my paint refused? | `lofi_world/src/paint.rs` (`can_fit`) |
| What's in the file? | `lofi_world/src/file/v3.rs` (top comment), `sections.rs` |
| What does the game get? | `lofi_world/src/cook/chunk.rs`, `manifest.rs` |
| Where does a character stand? | `lofi_world/src/ground/query.rs` |
| What order do things run in? | `lofi_editor/src/schedule.rs`, `ui/plugin.rs` |
| Why is the map marked unsaved? | `lofi_editor/src/document/map_document.rs` (`map_mut`) |
| How does undo keep so little? | `lofi_editor/src/document/history/stack.rs` |
| Why didn't my click reach the terrain? | `lofi_editor/src/ui/layout.rs` (`ScenePointer`) |
| How is the ground shaded? | `lofi_editor/src/view/shaders/terrain.wgsl` |
| What does a tool do each frame? | `lofi_terrain/src/sculpt/ops.rs` |

## Checking a change

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

To see a rendering change:

```sh
cargo run -p lofi_world --example demo_island -- /tmp/demo.lofimap
cargo run -p lofi_terrain -- /tmp/demo.lofimap --screenshot /tmp/shot.png
```

`docs/ROADMAP.md` records what was built in each phase, what was measured,
and what the reviews found.
