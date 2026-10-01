# lofi 3d

Tools for building low-fidelity fantasy worlds in the spirit of World of
Warcraft Classic. Rust, Bevy 0.19.

- **lofi_terrain** sculpts and paints the ground.
- **lofi_doodads** places trees, rocks and buildings on it.
- **lofi_cook** turns a map into the tiles a game streams.

All three work on the same `.lofimap` file.

## Docs

To understand how it's built, start with
[docs/PRIMER.md](docs/PRIMER.md): how the crates fit together, the ideas
they rest on, and what happens between a mouse press and a file on disk.
Each crate has a README (what it is, how to use it) and a primer (how it
works inside).

## Crates

| Crate | What it is | Docs |
|---|---|---|
| `lofi_world` | Map data every tool and the eventual game share: heightmap, materials and paint, cells and tint, objects, water, lighting, the `.lofimap` format, heightmaps, masks, meshing, ground queries (collision and position), and the cooked game format. No renderer or window. Holds the `lofi_cook` binary. | [README](crates/lofi_world/README.md) · [primer](crates/lofi_world/PRIMER.md) |
| `lofi_editor` | The shell both editors share: the map document with undo, files, autosave and locking; camera; terrain, water, sky and object view; menus, status bar and prompts. | [README](crates/lofi_editor/README.md) · [primer](crates/lofi_editor/PRIMER.md) |
| `lofi_terrain` | The terrain editor. | [README](crates/lofi_terrain/README.md) · [primer](crates/lofi_terrain/PRIMER.md) |
| `lofi_doodads` | The doodad editor. | [README](crates/lofi_doodads/README.md) · [primer](crates/lofi_doodads/PRIMER.md) |

## Run

```sh
cargo run -p lofi_terrain                       # new flat map
cargo run -p lofi_terrain -- path/to/zone.lofimap
cargo run -p lofi_doodads -- path/to/zone.lofimap
cargo run -p lofi_world --bin lofi_cook -- zone.lofimap cooked/ [--tile-chunks N]
cargo run -p lofi_world --bin lofi_cook -- --probe 256,256 cooked/
```

A sample map to start from (add a chunk count after the path for a bigger
island):

```sh
cargo run -p lofi_world --example demo_island -- demo_island.lofimap
cargo run -p lofi_terrain -- demo_island.lofimap
```

Both editors take these options:

| Option | Does |
|---|---|
| `--screenshot out.png` | Frame the map, save a picture, quit. |
| `--camera EX,EY,EZ,TX,TY,TZ` | Start with the eye at E looking at T. |
| `--flat` | Start in flat shading. |
| `--overlays slope,grid,full,cells` | Start with overlays on. |
| `--measure` | Time 300 frames with the whole map in view, print, quit. Use a release build. |

A map can be open in only one editor at a time; a second one is refused.
`--screenshot` and `--measure` only read the map, so they work on one that's
open, and exit 1 if it doesn't load.

## Controls

Both editors:

| Input | Action |
|---|---|
| RMB drag | Look around |
| W A S D, Q / E | Fly, down / up |
| Shift | Fly faster |
| MMB drag | Pan |
| Wheel | Dolly |
| F / G | Flat shading / wireframe |
| Home | Frame the map |
| Ctrl + Z / Ctrl + Y | Undo / redo |
| Ctrl + N / O / S | New / open / save (Shift for Save As) |

Terrain editor:

| Input | Action |
|---|---|
| LMB drag | Use the tool |
| Ctrl + LMB | Invert: lower, reverse noise, erase paint or tint, clear cell flags |
| Alt + wheel, `[` `]` | Brush size |
| 1 – 8 | Raise/Lower, Smooth, Flatten, Noise, Paint, Tint, Cells, Select |
| Shift / Ctrl + drag | Select: add / subtract |
| Esc, Ctrl + D | Select none |

Doodad editor:

| Input | Action |
|---|---|
| 1 / 2 | Place / Select and move |
| LMB | Place, or select; drag to move along the ground |
| R / Shift + R | Turn the selected object |
| `[` `]` | Scale it |
| End | Drop it to the ground |
| Delete | Remove it |
| Esc | Select none |

## Terrain tools

- **Sculpt:** Raise/Lower, Smooth, Flatten, Noise.
- **Paint:** lays down the selected material. Each chunk blends up to four.
- **Tint:** shades the ground toward a colour; mid grey changes nothing.
- **Cells:** cuts holes, forces a cell's diagonal, or flags cells as
  no-clutter or unwalkable.
- **Select:** a rectangle or lasso. Every other tool then works only inside it.
- **Masks** (in the panel) limit every tool to ground of a given height,
  slope, curvature or noise. With the Paint tool, "Paint where masks pass"
  paints the whole map, or the selection, in one undoable step.
- **Overlays** (View menu): ground too steep to walk on, the chunk grid,
  chunks already blending four materials, and flagged cells.
- **Heightmaps** (File menu): export as 16-bit PNG, R16 or R32, with a
  `.json` sidecar holding the height range. Import reads the same, stretching
  or cropping to fit the map. Import is undoable.

## Maps

A map is a grid of 32×32-cell chunks (up to 32×32 chunks from the editor)
with a height per vertex. It also holds:

- a material table of up to 32 materials, each with a colour, pattern and
  tile size;
- paint: each chunk blends up to four materials, weighted per vertex;
- a colour tint per vertex;
- flags per cell: hole, forced diagonal, no ground clutter, unwalkable;
- how cells are cut into triangles: following the terrain's ridges and
  valleys (new maps) or a checkerboard (maps from before 0.4);
- placed objects, each with a permanent id and an asset GUID (never a file
  path), so assets can move without breaking maps;
- a sea level;
- a light table: sun, ambient, sky colours and fog.

Ambient occlusion is baked from the heights whenever they change. It's never
saved.

Painting a fifth material into a chunk is refused; the status bar says which
chunk is full.

Files are `.lofimap` v3: MessagePack metadata plus compressed, checksummed
sections. A painted 32×32-chunk map is about 3–5 MB. v1 and v2 files still
open, and saving converts them. See `crates/lofi_world/src/file/`.

Saving never writes in place, and keeps the previous save as `zone.lofimap.bak`.
Read-only maps are refused rather than overwritten, and so is saving over a
map that changed on disk since it was opened.

Unsaved work autosaves every two minutes to `zone.lofimap.autosave`, or to the
app-data directory for an untitled map. If the editor dies, opening that map
again offers the work back. Untitled work is offered at the next start or New.

## Cooking for the game

`lofi_cook`, or File > Cook for game…, writes a folder holding:

- `world.lofiworld`: grid, materials, water, lighting and a tile index;
- `tiles/t_<x>_<z>.lofitile`: one per square of chunks, so a game streams
  the world a tile at a time.

Each chunk in a tile is self-contained:

- heights in 1 cm steps above the chunk's lowest point, on one global step
  so neighbours meet exactly;
- the editor's triangle diagonals, holes and cell flags as bits, so
  collision matches what the editor drew;
- normals, paint, tint and baked AO.

Each tile also carries its objects.

A cook goes to a temporary folder first and replaces the target only when
complete; if that fails, the earlier cook is still there. The target must be
empty or an earlier cook.

## The terrain describes itself

The terrain is the skeleton of the world, so it answers for its own
collision and position. `lofi_world::Ground` is the one set of questions,
answered the same by the editor's terrain and by a cooked world:

| Question | Answer |
|---|---|
| `ground_at(xz)` | Height, the normal underfoot, walkable or not; or a hole; or no data there. |
| `cast_ray(origin, dir, max, holes)` | The first point a ray meets the ground from above. |
| `trace_ray(…)` | The same, but tells a clear line from one that ran into ground that isn't loaded. |
| `chunk_bounds(chunk)` | The chunk's box, with its real height range. |
| `chunk_collision(chunk)` | The chunk's triangles, holes left out, for a triangle-mesh collider. |

In a game:

```rust
let world = CookedWorld::open("cooked")?;
let layout = world.layout();
let mut ground = world.ground();
// Keep the tiles around the player loaded.
for (x, z) in layout.tiles_within(player.xz(), 300.0) {
    if !ground.is_loaded(x, z) {
        ground.insert(world.load_tile(x, z)?)?;
    }
}
match ground.ground_at(player.xz()) {
    GroundAt::Surface(point) => { /* stand at point.position, if point.walkable */ }
    GroundAt::Hole => { /* fall */ }
    GroundAt::Unknown => { /* not loaded yet, or off the map: wait */ }
}
```

The manifest places and boxes every tile without opening one
(`Manifest::tile_bounds`), and `lofi_cook --probe X,Z DIR` prints what a
cook says about a point. `examples/cooked_walk.rs` walks across a cook this
way, streaming tiles as it goes:

```sh
cargo run -p lofi_world --example cooked_walk -- cooked/ 100
```

In the editors, the status bar shows the slope under the pointer and whether
the game will let a character walk there.

See [docs/ROADMAP.md](docs/ROADMAP.md) for where this is going.
