# lofi_terrain

The terrain editor: sculpt the ground, paint it, tint it, cut holes, and
save the map. Built on [`lofi_editor`](../lofi_editor/README.md), which
supplies the window, files, undo, camera and drawing. This crate adds the
brushes, the selection and the side panel.

How it works inside: [PRIMER.md](PRIMER.md).

## Run

```sh
cargo run -p lofi_terrain                       # a new flat map
cargo run -p lofi_terrain -- path/to/zone.lofimap
cargo run --release -p lofi_terrain -- zone.lofimap   # for big maps
```

The shared command-line options (`--screenshot`, `--camera`, `--flat`,
`--overlays`, `--measure`) are listed in the
[`lofi_editor` README](../lofi_editor/README.md#command-line).

## Tools

| Key | Tool | Drag | Ctrl + drag |
|---|---|---|---|
| 1 | Raise / Lower | raises | lowers |
| 2 | Smooth | softens bumps and hard edges | |
| 3 | Flatten | levels toward the height where the stroke began | |
| 4 | Noise | roughens | inverts the noise |
| 5 | Paint | paints the selected material | erases to the base material |
| 6 | Tint | shades toward the tint colour | clears the tint |
| 7 | Cells | sets the chosen flag: hole, a forced diagonal, no clutter, unwalkable | clears it |
| 8 | Select | draws a rectangle or lasso | subtracts (Shift adds) |

- **Brush**: radius (`[` `]` or Alt + wheel), strength, hardness.
- **Masks** limit every tool to ground of a given height, slope, curvature
  or noise. With the Paint tool, "Paint where masks pass" paints the whole
  map, or the selection, in one undoable step.
- **Selection**: once something is selected, every other tool works only
  inside it. Esc or Ctrl + D clears it.

The side panel also edits the material table, the sea, the lighting, the
map's name and its triangulation rule.

## Controls

| Input | Action |
|---|---|
| LMB drag | Use the tool |
| Ctrl + LMB | Invert it |
| RMB drag | Look around |
| W A S D, Q / E | Fly, down / up |
| Shift | Fly faster |
| MMB drag | Pan |
| Wheel | Dolly |
| Alt + wheel, `[` `]` | Brush size |
| 1 – 8 | Pick a tool |
| Esc, Ctrl + D | Select none |
| F / G | Flat shading / wireframe |
| Home | Frame the map |
| Ctrl + Z / Ctrl + Y | Undo / redo |
| Ctrl + N / O / S | New / open / save (Shift + S: Save As) |

## Layout

| Module | What it holds |
|---|---|
| `sculpt` | Tools and brush settings, what each tool does in one frame (`ops`), strokes, brush keys, the brush preview. |
| `select` | The selection (shapes and a per-vertex mask), the Select tool, its outline. |
| `panel` | The side panel: tools and masks, materials, water, lighting, map facts. |

## Tests

```sh
cargo test -p lofi_terrain
```

Tool behaviour is tested on small terrains without a window: each tool's
effect, masks and selections limiting it, footprints, auto-paint, the lasso.
