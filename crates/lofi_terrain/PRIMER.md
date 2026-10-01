# lofi_terrain: how it works

The terrain editor is three things on top of the shell: a brush that edits
the map a little every frame the mouse is down, a selection that limits
where it acts, and a panel. Read the
[`lofi_editor` primer](../lofi_editor/PRIMER.md) first; this one assumes the
document, undo and frame order described there. File paths are relative to
`src/`.

`main.rs` is the whole wiring: `editor_app`, plus `SculptPlugin`,
`SelectPlugin` and the panel.

## 1. Brush settings

`sculpt/brush.rs`. `BrushSettings` is a resource: the active `Tool`, radius
in metres, strength and hardness (0 to 1), the material Paint lays down, the
tint colour, the Cells tool's mode, and a `MaskStack`.

`falloff(distance, radius, hardness)` is the brush's shape: 1 inside the
hard core (`hardness` × radius), easing smoothly to 0 at the radius.

## 2. A stroke

`sculpt/stroke.rs`, `run_stroke`, in `EditorSystems::Stroke`. One stroke is
one undo step.

```text
press over ground      history.begin(tool label)
                       remember the height under the cursor (Flatten's target)

each frame held        footprint = vertices the brush can reach
                       history.capture(terrain, footprint)
                       apply one dab (section 3)
                       mark TerrainChanges by what the tool changes

release                history.commit(map)
```

Details:

- It starts only when the left button goes down **over the 3D view** and
  **over ground** (`ScenePointer`, `GroundCursor`), and not while the right
  button is held for looking around. The Select tool is not a stroke.
- With the cursor over a panel or off the map, the stroke pauses; it doesn't
  end.
- It also ends if the history stopped recording under it, which happens when
  another map is opened mid-stroke.
- A dab goes through `MapDocument::edit_terrain`, so a frame that changed
  nothing (flattening flat ground) doesn't mark the map unsaved.
- What gets marked: sculpting tools mark heights, the Cells tool marks shape
  for holes and diagonals (and nothing for the two flags that only draw as
  an overlay), everything else marks surface.
- If a chunk refuses paint because it already blends four materials, the
  status bar says which chunk, once per stroke.

## 3. A dab

`sculpt/ops.rs`, `apply_dab`. One frame of one tool.

### Weights first

Before anything is written, every vertex in the footprint gets a weight:

```text
weight = falloff × selection × masks
```

All of them are computed up front, from the ground as it was. Otherwise a
height mask would chase its own edits across the dab: raise a vertex out of
the mask's range and its neighbour's weight would already see the change.

### Rates, not amounts

Every tool's effect is scaled by the frame's `dt`, so a stroke feels the
same at 30 or 240 frames a second.

- Raise and Noise move at a speed: metres per second at full strength.
- Smooth, Flatten, Paint and Tint **approach** a target. `approach(rate, dt)
  = 1 − e^(−rate × dt)` is the fraction of the remaining distance covered
  this frame, which is frame-rate independent in a way a plain `rate × dt`
  isn't.

### The tools

| Tool | What a dab does to each weighted vertex |
|---|---|
| **Raise** | `height += 20 m/s × strength × dt × weight`. Ctrl flips the sign. |
| **Noise** | The same, times `fbm` at the vertex's position, at 10 m/s. Feature size is half the brush radius. The noise is fixed in space, so holding still grows the same lumps taller. |
| **Smooth** | Moves toward the mean of its 3×3 neighbourhood. All the means are read before any height is written, so the result doesn't depend on visiting order. |
| **Flatten** | Moves toward the height where the stroke began. |
| **Paint** | `paint_vertex` toward the brush's material. Ctrl paints toward the map's base material instead. |
| **Tint** | `tint_toward` the tint colour. Ctrl goes toward neutral. |

Paint and Tint pass a **dither** value, `hash01(x, z, dab number)`, a fresh
random number per vertex per frame. Weights are bytes; without dither a
light brush would round to no change every frame and never arrive.

Erasing has one extra case. In a chunk that already blends four materials
and doesn't include the base, painting toward the base is refused. Then
`erase_fallback` paints toward the strongest *other* material already at the
vertex, so the brush's material can still be taken off.

### Cells

`apply_cells`. Flags are on or off, so this one doesn't use weights:

- A cell is hit if its **centre** is inside the brush radius, or it's the
  cell under the cursor (so a brush smaller than a cell still does
  something). `footprint` is built to match.
- Hardness doesn't apply.
- Masks must pass by at least half, averaged over the cell's four corners.
- The selection is tested at the cell's centre.

## 4. The selection

`select/`. Editor state: not saved, not undoable, cleared when another map
is opened.

`Selection` (`selection.rs`) keeps two things:

- the **shapes** the user drew, each a rectangle or a lasso outline with an
  add or subtract operation, in order. Kept for drawing.
- a **per-vertex mask**, rebuilt from the shapes whenever they change. This
  is what lookups use: `weight(x, z)` is an array read, called for every
  vertex of every dab.

Rasterising goes row by row. A rectangle fills its span directly. A lasso
finds where the outline crosses that row, sorts the crossings, and fills
between pairs (the even-odd rule), so a big lasso over a big map stays
cheap. Later shapes overwrite earlier ones, which is how subtract works.

An empty selection limits nothing: `weight` returns 1 everywhere. If
subtracting removes everything, the selection clears itself rather than
limit every tool to nothing.

`contains_point` answers for points between vertices (the Cells tool's cell
centres) by testing the shapes directly.

The **Select tool** (`tool.rs`): a drag draws a shape; on release, plain
replaces the selection, Shift adds, Ctrl subtracts. A click without a drag
clears it. Esc cancels a drag in progress, or else clears; Ctrl + D clears.
A lasso keeps a point each time the cursor has moved a cell, up to 1024.

`draw.rs` outlines the shapes on the ground with gizmo lines, sampled about
once per cell.

## 5. Auto-paint

`ops.rs`, `auto_paint`. "Paint where masks pass" paints the brush's material
over the whole map, or the selection's bounds, in one pass: each vertex goes
toward the material by `selection × masks` at that vertex. Weights under 0.1
are skipped, so a mask's faint tail doesn't claim one of a chunk's four
slots for a trace of paint. It reports how many vertices were refused by
full chunks.

It's a recorded edit (`begin`, `capture`, `commit`), so it's one undo step,
and it won't start while a stroke is recording.

## 6. The panel

`panel/`. One system, `tool_panel`, in `UiSystems::Panels`.

It follows the shell's rule for panels: **copy, draw, compare, write back.**

```text
copy     materials, water, lighting, name, triangulation out of the map
draw     the sections edit the copies
compare  which copy differs from the map?
write    record_map_edit(label), then map_mut() and assign
```

The label is the first group that changed ("Materials", "Water",
"Lighting", "Rename", "Diagonals"). Consecutive frames with the same label
merge into one undo step until the shell seals it when the UI goes idle, so
dragging the sea level is one step.

Two things in the panel are more than a settings change:

- **Delete material**: repaints everything painted with it as another
  material, then removes it from the table. That touches terrain and
  settings, so it's a recorded edit capturing every chunk and the map
  fields.
- **Auto-paint**, above.

The sections:

- `tools.rs`: the tool list, brush sliders, each tool's own options, and the
  mask stack. Brush state isn't map data, so it uses plain egui widgets.
- `materials.rs`: pick the paint material; edit colour, name, pattern and
  tile size; set the base; add; delete.
- `environment.rs`: sea level and colours; the light table.
- `info.rs`: name, triangulation rule, size, and the height range.

The height range needs a scan of every vertex, which is millions on a big
map. `HeightRange` caches it against the document's generation and revision,
and holds the old value while a stroke is recording, since a stroke changes
the revision every frame.

## 7. Keys and the preview

- `sculpt/adjust.rs`: 1 to 8 pick tools; `[` and `]` scale the radius by
  1.15; Alt + wheel does the same. Plain wheel belongs to the camera.
- `sculpt/preview.rs`: the brush is drawn as a ring lying on the ground at
  the radius, a fainter one at the hard core, and a short vertical line at
  the centre. The Select tool shows a small cross instead.

## 8. Where to change things

| To | Look at |
|---|---|
| add a tool | `Tool` in `brush.rs`, a branch in `apply_dab`, what it marks in `stroke.rs`, its options in `panel/tools.rs` |
| change how strong a tool is | the rate constants at the top of `ops.rs` |
| add a kind of mask | `lofi_world::masks`, then its row in `panel/tools.rs` |
| add a selection shape | `Shape` in `selection.rs`: `contains`, `bounds`, `row_spans` |
| add a map setting to the panel | the copy/compare/write lists in `tool_panel.rs`; if it's new data, `MapFields` in the shell |
