# lofi_doodads

The doodad editor: place trees, rocks and buildings on a map, then move,
turn, scale and delete them. Built on
[`lofi_editor`](../lofi_editor/README.md), so files, undo, autosave, the
camera and the view work exactly as in the terrain editor. It edits the same
`.lofimap`; a map can be open in only one editor at a time.

The assets are low-poly placeholders until there is real art. An object
names its asset by GUID, so swapping in real models later won't touch maps.

How it works inside: [PRIMER.md](PRIMER.md).

## Run

```sh
cargo run -p lofi_doodads -- path/to/zone.lofimap
```

The shared command-line options are listed in the
[`lofi_editor` README](../lofi_editor/README.md#command-line).

## Controls

| Input | Action |
|---|---|
| 1 / 2 | Place / Select and move |
| LMB (Place) | Place the chosen asset on the ground |
| LMB (Select) | Select the object under the pointer; drag to move it along the ground |
| R / Shift + R | Turn the selected object 15° |
| `[` `]` | Scale it down / up by 10% |
| End | Drop it to the ground |
| Delete | Remove it |
| Esc | Select none |

Camera, undo and file shortcuts are the shell's: see the
[`lofi_terrain` README](../lofi_terrain/README.md#controls).

The panel lists the assets, has a "Random turn" switch and a scale for new
placements, and shows the selected object's position, turn and scale as
editable numbers.

## Layout

| Module | What it holds |
|---|---|
| `tool` | Settings and selection (`settings`), the left button (`mouse`), key and panel edits (`edit`). |
| `pick` | Which object the pointer is on. |
| `draw` | Rings on the ground for the selected, hovered and about-to-be-placed objects. |
| `panel` | The side panel. |

Object data (`ObjectLayer`, `PlacedObject`, the asset catalog) lives in
`lofi_world`. Drawing the objects themselves is in `lofi_editor`'s view, so
the terrain editor shows them too.

## Tests

```sh
cargo test -p lofi_doodads
```

The mouse system runs in a headless Bevy app with a scripted pointer:
placing, selecting without nudging, dragging as one undo step, keeping an
object's height above the ground, edits that change nothing, and the
selection clearing on another map.
