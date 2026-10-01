# lofi_editor: how it works

The shell is a Bevy app with one unusual choice at its centre: **the map is
a document, not a set of entities.** Everything follows from that. File
paths are relative to `src/`.

Read `document/map_document.rs`, `schedule.rs` and `document/history/stack.rs`
first.

## 1. The document

`MapDocument` (`document/map_document.rs`) is a resource holding one
`lofi_world::WorldMap` and what the editor needs to know about it:

| Field | Meaning |
|---|---|
| `map` | The map itself. |
| `path` | Where it's saved, if anywhere. |
| `revision` | Bumps on every change. Saved in the file. |
| `saved_revision` | The revision that's on disk. |
| `generation` | Bumps when the map is swapped for another. |
| `claim` | This session's lock on `path`. |

The map is **unsaved** when `revision != saved_revision`. There is no dirty
flag to forget to set: changing the map is the only way to get `&mut` to it.

- `map()` reads.
- `map_mut()` bumps the revision and hands out `&mut WorldMap`.
- `edit_terrain(|terrain| -> bool)` bumps it only if the closure reports a
  change. Brushes use this: flattening flat ground changes nothing and must
  not mark the map unsaved.

So the rule for all code: **only call `map_mut` when something really
changes.** UI code edits a copy and writes back on change (section 8).

`generation` is the other counter. Views compare it with the one they were
built for; when it differs, the map is a different map and they rebuild from
scratch instead of patching.

The terrain chunks you see are entities, but they're a *view* of the
document, rebuilt from it. Nothing ever reads the map back out of the ECS.

## 2. A frame

`schedule.rs` defines `EditorSystems`, a chain in `Update`. Each stage reads
what the one before wrote:

```text
Update
  Input    where the pointer is; shortcuts; window close
  Camera   look, fly, pan, frame the map, stay above ground
  Cursor   the pointer's ray and where it meets the ground
  Stroke   the tool's edits to the map (brush dabs, drags)
  Edit     undo/redo, file actions, recovery, import, cook, autosave
  View     meshes, water, objects, materials, overlays follow the document

PostUpdate (bevy_egui's pass)
  UiSystems::Begin    reset the layout
  UiSystems::Bars     menu bar, status bar
  UiSystems::Panels   the tool's side panels
  UiSystems::Finish   record the free rect; seal panel undo steps
  UiSystems::Windows  prompts, on top
```

Things that follow from the order:

- **`Stroke` runs before `Edit`**, so a stroke released this frame is
  committed before an undo in the same frame looks for it.
- **`View` runs last in `Update`**, so whatever changed the map this frame
  is drawn this frame.
- **The UI runs after `Update`.** A value changed in a panel reaches the
  view on the next frame. And the "is the pointer over the 3D view" answer
  that `Input` uses comes from the previous frame's layout.

Tools put their systems in these sets. A tool's mouse work goes in `Stroke`,
its keys in `Input`, anything that reacts to undo or a new map in `View`.

## 3. Telling the view what changed

The view doesn't diff the map. Whoever edits terrain says what they touched,
in `TerrainChanges` (`document/map_document.rs`):

| Call | Means | The view then |
|---|---|---|
| `mark_heights(rect)` | heights moved | remeshes chunks within 8 cells of the rect, rebakes their AO, rebuilds their water |
| `mark_shape(rect)` | holes or diagonals changed | remeshes those chunks, rebuilds their water |
| `mark_surface(rect)` | paint or tint changed | remeshes those chunks, reusing cached AO |
| `mark_all_shapes()`, `mark_all_surfaces()` | the rule or the material order changed | every chunk |

The three kinds are separate because they cost differently: AO is the
expensive part and depends on heights only.

Everything else in the map (water level, lighting, material colours,
objects) the view picks up by looking at the document when it changes.

## 4. Undo

`document/history/stack.rs`. `EditHistory` is a stack of `Edit`s, each
holding **the state from before** the edit:

```rust
struct Edit {
    label: &'static str,
    chunks: Vec<(ChunkCoord, ChunkData)>,  // old data of touched chunks
    fields: Option<MapFields>,             // old map settings, if they changed
    bytes: usize,
}
```

Undo **swaps** stored and live state. The entry then holds what was live,
and the same entry serves as redo. Because chunk data is copy-on-write,
keeping the old data of a chunk is a pointer copy; only what the edit
actually wrote was duplicated.

`MapFields` (`history/map_fields.rs`) is the non-terrain part: name,
materials, water, lighting, triangulation rule, and the object list.

There are two ways to record, for two kinds of edit.

### Recorded edits: strokes and drags

For anything that runs over several frames or touches terrain:

```rust
history.begin("Raise / Lower");            // undo is refused from here…
history.capture(terrain, rect);            // before each write: keep those chunks
history.capture_fields(map);               // if it will change settings too
// … change the map …
history.commit(map);                       // …to here. Returns whether anything changed.
```

- `capture` keeps a chunk's old data the first time the edit touches it.
  It's called before every dab; later calls for the same chunk do nothing.
- `commit` drops chunks that ended up unchanged, and drops the whole edit if
  nothing changed.
- While recording, `undo` and `redo` return `None`, and `undo_label` is
  `None`, so the menu greys out.
- `begin` replaces a recording in progress. One-shot edits (auto-paint,
  import, delete material) check `is_recording()` first.

### Map edits: settings and one-offs

For a change that happens in one frame, with no terrain data:

```rust
history.record_map_edit("Lighting", MapFields::of(map));  // then change the map
```

Consecutive calls with the same label **merge** into one step. That's what
makes dragging a colour picker or typing a name one undo step rather than
sixty. The merge ends when `seal()` is called:

- the shell calls it whenever the UI is idle (mouse up, nothing focused):
  `seal_when_idle` in `ui/plugin.rs`;
- saving calls it, so undo can always step back to exactly what was saved;
- a tool calls it right after a one-off that shouldn't merge with the next
  (placing an object, pressing a rotate key).

### Applying it

`document/undo.rs`: `HistoryMessage::Undo` / `Redo` come from the menu and
shortcuts. `apply_history_messages` runs the swap and gets back a `Reverted`
saying which kinds of terrain data changed in which rectangle; it marks
`TerrainChanges` accordingly, so undoing paint doesn't rebake AO.

### Budget

History keeps at most 256 MB. Each edit records the bytes it holds that the
live map doesn't share, and the oldest edits drop once the total passes the
budget.

## 5. Files

`document/file_actions.rs`. Everything goes through one message:

```rust
FileActionMessage { action: FileAction, discard_confirmed: bool }
// New, Open, OpenFile(path), Save, SaveAs, ImportHeightmap,
// ExportHeightmap(format), Cook, Quit
```

`handle_file_actions` runs in `Edit`:

- **The unsaved-changes guard.** New, Open and Quit throw the map away. If
  it's unsaved and the message isn't marked confirmed, the action is parked
  in `PendingDiscard` and a prompt asks. "Discard" re-sends it confirmed.
  The window's close button is turned into a `Quit` action
  (`close_guard.rs`), so it goes through the same guard.
- **Dialogs run off the main thread** (`file_dialog.rs`, `rfd` on the IO
  task pool), so the editor keeps drawing behind them. That means the map
  can change while a dialog is up: an Open dialog remembers the revision it
  was opened at, and if the map changed by the time a file is picked, the
  guard asks again.
- **Open** (`open_document`): resolve the path, take the lock, load, offer
  any autosave, replace the document, clear the history.
- **Save** (`save_document`): resolve the path, check the claim, write with
  `lofi_world::save_map` (temp file, fsync, `.bak`, rename), retire the
  autosave, seal the history, record the new saved revision.

### One editor per map

`document/lock.rs`. Opening or saving a map takes an exclusive OS lock on a
small `.<name>.lock` file beside it (`File::try_lock`). A second session
gets "open in another lofi editor". The OS drops the lock if the editor
dies, so a crash never leaves a map claimed. Lock files are left in place:
deleting one could let two sessions lock different files for the same map.

A `MapClaim` is the lock plus the file's modification time as last seen.
Saving over a file that changed on disk since it was opened is refused.

Paths go through `lock::resolved` first (absolute, links followed), so the
same file reached by two spellings is still one file with one lock.

## 6. Autosave and recovery

`document/autosave.rs`, `sidecar.rs`, `recovery.rs`.

Every two minutes, if the map is unsaved and that revision hasn't been
autosaved yet, the editor clones the map (pointers only) and writes it on
the IO task pool to a **sidecar**:

- `<map>.autosave` beside a saved map;
- for an untitled map, a uniquely named file in the app-data directory.

Saving for real, or discarding the map, deletes the sidecar (`retire`,
which first waits for a write in flight). So a sidecar left behind means the
editor died with unsaved work.

The sidecar carries a `FileStamp`: its own `revision` and `autosave_base`,
the revision of the file it was taken from. On open, `classify` compares
them with the map file's revision:

| Sidecar | Verdict |
|---|---|
| no `autosave_base` | not an autosave; leave it alone |
| base ≠ the file's revision | offer it, warning that the file changed since |
| base = the file's revision, and its revision is newer | offer it |
| otherwise | stale; delete it |

An offer sets `PendingRecovery`, and a prompt asks Recover or Discard.
Recovering replaces the document with the autosave's map, still unsaved, and
adopts the sidecar as this session's own.

Safety rules that the code enforces:

- Autosave only ever writes or deletes the sidecar it **owns**. Any other
  may be another session's unsaved work: it's offered, never overwritten.
  If the sidecar it would write already exists and isn't its own, autosave
  turns itself off and says so.
- Untitled sidecars are locked by the session that owns them, and from the
  moment one is offered, so two sessions can't both adopt it.
- Anything that replaces the document calls `Autosave::retire` first.

## 7. Picking and the pointer

Two resources, both updated early in the frame:

- `ui::ScenePointer { position }` (`ui/layout.rs`): the pointer's position
  **only while it's over the 3D view and not over any UI**. `None` otherwise.
- `pick::GroundCursor { ray, hit }` (`pick.rs`): the pointer's ray into the
  scene and where it meets the ground. Holes count as ground here, so a tool
  can point at one to fill it.

Anything that reacts to the mouse in the 3D view reads these, not the raw
cursor. The reason is an egui detail:

> Through bevy_egui, egui 0.36 panels are drawn into a `Ui` and don't count
> as "pointer over egui". A click on a side panel would also paint the
> terrain behind it.

So the shell tracks the space the panels leave. `UiLayout` starts each
frame as the whole window; each panel takes a `root()` over what's left and
`claim()`s what it used; `finish_layout` records the remainder as the scene
rectangle. `ScenePointer` is `Some` only inside it, and only when egui
doesn't want the pointer for a window, a menu or a drag.

## 8. The UI

`ui/`. Plain egui, immediate mode.

- **Menu bar** (`menu_bar.rs`): File, Edit, View. It sends messages; it
  doesn't do the work.
- **Status bar** (`status_bar.rs`): the last message (`StatusLine`), the
  pointer's position and height, the slope there and whether a character
  could walk on it, and the map's size.
- **Prompts** (`dialogs.rs`, `recovery_prompt.rs`, `import_prompt.rs`): new
  map, discard changes, recover autosave, import heightmap. The last three
  are modal. `document::no_prompt_open` is a run condition: shortcuts and
  flying are off while a prompt waits, so a key can't act on a map the user
  hasn't decided about.
- **Shortcuts** (`shortcuts.rs`): also off while a text field has focus
  (`egui_wants_any_keyboard_input`).

### Panels must not dirty the map

Two egui behaviours rewrite a value just by drawing it: colour buttons
round-trip through HSV, and sliders clamp to their range. Shown on a freshly
opened map, either would bump the revision and mark it unsaved. So:

- panels **edit copies** of the map's values and write back only if the copy
  differs afterwards;
- map colours and ranged numbers use `ui::widgets::srgb_button` and
  `widgets::slider`, which leave the stored value alone until the user
  changes it.

## 9. Drawing the world

`view/`. Terrain, water, sky and objects each have a custom Bevy `Material`
with its own shader. None use Bevy's PBR lighting: the look is WoW Classic's
(soft sun, sky and ground ambient, no specular), and colours should land on
screen as painted.

### Chunks

`view/chunks.rs`. One entity per chunk, all sharing one material; chunks
differ only in mesh data.

- `respawn_chunks_for_new_map`: when `generation` changes, despawn
  everything and build every chunk.
- `mark_all_on_layout_change`: material layer indices are baked into
  meshes, and the triangulation rule picks triangles. Removing or reordering
  materials, or changing the rule, marks every chunk. Appending a material
  moves nothing and rebuilds nothing.
- `remesh_changed_chunks`: takes `TerrainChanges`, turns them into one job
  per chunk (`remesh_jobs`), and builds the jobs on Bevy's compute task
  pool, a few batches per thread. AO is cached per chunk and rebaked only
  for jobs that need it. Measured: about 0.2 ms per chunk in release.

### How terrain data reaches the shader

`view/terrain_mesh.rs` packs a `lofi_world::ChunkMesh` into **standard
attribute slots**, so Bevy's own shadow pass still draws the mesh unchanged:

| Attribute | Holds |
|---|---|
| `POSITION`, `NORMAL` | as usual (smooth normals) |
| `COLOR` | the four slot weights, 0 to 1 |
| `UV_0` | the chunk's four material layer indices, two per float (`a + b × 256`) |
| `UV_1` | tint and AO bytes, two per float (`r + g × 256`, `b + ao × 256`) |

Integers below 2¹⁶ are exact in `f32`, and the vertex shader unpacks them
before anything is interpolated. A material's layer index is its position in
the map's material table.

### The terrain shader

`view/shaders/terrain.wgsl`, per pixel:

1. **Sample** each painted slot's texture from one texture array, by world
   XZ over the material's tile size. Unpainted slots are skipped.
   - A second, larger, rotated sample of the same texture varies the
     brightness, so the tile's repeat doesn't read as rows at grazing
     angles.
   - With "crisp texels" on, UVs snap to texel centres with a one-pixel
     blended seam: chunky squares up close, no shimmer far away.
2. **Height-blend** them. Each texture's alpha is a height; where two
   materials meet, the taller texel wins, so sand settles into the cracks of
   rock instead of smearing over it.
3. **Light** it: Lambert sun with shadows, plus an ambient term blending
   ground colour to sky colour by how much the normal faces up, multiplied
   by baked AO. In flat shading the normal comes from screen-space
   derivatives instead of the mesh.
4. **Tint**: multiply by the vertex tint (128 is neutral).
5. **Fog**, then overlays on top (slope in red, the chunk grid).

Two shader rules that are easy to break:

- Layers are sampled inside per-pixel `if`s, so derivatives (`dpdx`,
  `fwidth`) are taken once outside and passed in, and textures are sampled
  with `textureSampleGrad`.
- Fog is one import, `lofi::fog` (`shaders/fog.wgsl`), used by every
  surface, so everything fades into the horizon colour together. Anything
  new that draws in the world uses it too.

### Textures

`view/textures.rs` generates the material textures: one 128×128 tileable
layer per material pattern (grass, dirt, rock, sand, snow, cobble, plain),
with a full mip chain, cached per pattern. They're detail maps centred on
0.5 that the shader multiplies by the material's colour, so recolouring a
material needs no new texture. The array is rebuilt only when the list of
patterns changes.

### Light and colour

`view/scene.rs` keeps the sun, the sky and the materials' uniforms in step
with the map's light table.

- Colours in the map are sRGB; `linear()` converts them for the shaders.
- `Tonemapping` is `None` on the camera.
- The sun's illuminance is set to `sun_strength × π / exposure`. The shader
  computes `light colour × N·L / π × exposure`, so the two cancel and a
  surface facing the sun gets exactly `sun_strength` of its albedo.
- Bevy's ambient light is set to zero; the shaders do their own.
- Shadows are Bevy's cascaded shadow maps, out to 1200 m.

### Water

`view/water.rs`, `shaders/water.wgsl`. One flat surface per chunk, plus four
big strips of open sea around the map.

The trick: each water vertex carries the **ground height** under it, and the
shader takes depth as `sea level − ground`. So:

- depth doesn't depend on the view angle and needs no depth prepass;
- moving the sea level changes one uniform and the entities' height. No mesh
  is rebuilt;
- pixels where depth ≤ 0 are discarded, which draws the shoreline.

Colour runs from shallow to deep over the map's "deep at" distance, with a
foam band in the first 70 cm and a sun highlight. A chunk's surface is
hidden when its lowest ground is above sea level. Water is cut into the
terrain's own triangles so depth interpolates the way the ground does, and
it's rebuilt only for chunks whose heights or holes changed
(`ChunksRebuiltMessage`).

### Sky

`view/sky.rs`, `shaders/sky.wgsl`. A sphere 40 km in radius that follows the
camera, drawn from the inside: horizon colour blending to zenith, a sun disc
and glow. The horizon colour is also the fog colour and the clear colour, so
distant land fades straight into the sky.

### Objects

`view/objects.rs`, `view/shapes.rs`. One entity per placed object, kept in
step with the map's object list. The sync runs only when the list's `Arc`
changes: it updates transforms of entities that still exist, spawns new
ones, despawns the rest. Meshes are low-poly placeholders built in code, one
per asset, with colours baked into the vertices so every object shares one
material.

### Overlays

- **Slope** and **chunk grid** are in the terrain shader, switched by flags.
- **Full chunks** (already blending four materials) and **flagged cells**
  are drawn with gizmo lines (`view/overlays.rs`), capped and limited to
  near the camera. The full-chunk list is recounted when the map changes,
  but not mid-stroke.

## 10. Camera

`camera/fly.rs`. Right-drag looks (the cursor is grabbed and hidden while
held), WASD and Q/E fly, middle-drag pans, the wheel dollies. Speed scales
with height above the ground, so crossing a big map from high up doesn't
crawl and close work stays precise. The camera is kept at least 1 m above
the ground. Flying is off while Ctrl is held (so Ctrl+S doesn't fly), while
a text field has focus, and while a prompt is open.

`camera/framing.rs` puts the whole map in view on start, after a new or
opened map (`generation` again), and on Home.

## 11. Screenshots and measuring

`capture/`. `--screenshot` waits 30 frames for meshes to upload and shadows
to settle, saves the frame, and quits. `--measure` skips 60 frames, times
300 with the whole map in view, vsync and fog off, and prints frame times
and GPU pass timings.

On Wayland the compositor can pace frames, so wall-clock frame time may read
high whatever the map; the GPU line is the one to trust.

Both read the map without locking (`document/startup.rs`), so they work on a
map that's open in an editor, and exit 1 if the map doesn't load.

## 12. Rules for a tool

What a tool built on the shell must do, and why:

1. **Read `ScenePointer` and `GroundCursor`**, never the raw cursor. Panels
   don't block the pointer on their own.
2. **Call `map_mut` only on a real change.** It marks the map unsaved. For
   an edit that may turn out to change nothing, work on a copy and write
   back if it differs.
3. **Record every change** in `EditHistory`: `begin`/`capture`/`commit` for
   strokes and drags, `record_map_edit` (+ `seal` for one-offs) for the
   rest. Check `is_recording()` before starting a one-shot edit.
4. **Mark terrain edits** in `TerrainChanges` by kind.
5. **Put systems in the right `EditorSystems` set**, and the panel in
   `UiSystems::Panels`. Take a `UiLayout::root`, show the panel in it, then
   `claim` it.
6. **Gate keys** on `not(egui_wants_any_keyboard_input)` and
   `no_prompt_open`.
7. **Clear per-map state** (a selection, a selected object) when
   `generation` changes.
8. **Use `ui::widgets`** for map colours and ranged values.

## 13. Rough edges

Things that are known and left as they are:

- The window title says "lofi terrain" in both tools
  (`ui/window_title.rs` hardcodes it).
- The terrain editor draws objects but can't edit them; sculpting under them
  leaves them floating or buried.
- Every object edit copies the whole object list and the view walks all of
  it. Fine for thousands of objects, not for hundreds of thousands.
- New maps are capped at 32×32 chunks in the New Map window; the format
  allows 64×64.
