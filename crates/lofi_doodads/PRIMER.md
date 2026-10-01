# lofi_doodads: how it works

A small tool: a few hundred lines on top of the shell. Read the
[`lofi_editor` primer](../lofi_editor/PRIMER.md) first; this assumes the
document, undo and frame order described there. File paths are relative to
`src/`.

## 1. Where the data lives

Nothing in this crate stores objects. They're part of the map:

- `lofi_world::ObjectLayer` on `WorldMap.objects`: a list of
  `PlacedObject { id, asset, position, yaw, scale }`, sorted by id.
- `lofi_world::assets`: the placeholder catalog (six shapes with fixed
  GUIDs) and each asset's size, used here for picking and rings.

Three properties of the layer shape this tool:

- **Ids only grow.** A deleted or undone object's id is never handed out
  again.
- **The list is an `Arc`.** Cloning the layer is a pointer copy, which makes
  "work on a copy" cheap to write (it does copy the list on the first
  write).
- **A layer can be foreign**: saved by a newer tool in a format this build
  can't read. It's shown as empty and nothing here may edit it. Every entry
  point checks `is_foreign()`.

## 2. Wiring

`main.rs`. Four systems in the shell's frame order, and the panel:

| System | Set | Does |
|---|---|---|
| `tool::object_keys` | `Input` | Tool keys and edits to the selected object. Off while a text field has focus or a prompt is open. |
| `tool::run_mouse` | `Stroke` | Place, select, drag. |
| `tool::forget_vanished` | `View` | Drops a selection whose object is gone. |
| `draw::draw_object_rings` | `View` | Rings on the ground. |
| `panel::doodad_panel` | `UiSystems::Panels` | The side panel. |

Three resources (`tool/settings.rs`, `tool/mouse.rs`): `DoodadSettings` (the
tool, the asset to place, random turn, scale), `SelectedObject`, and
`ObjectDrag`.

## 3. One helper for every edit

`tool/edit.rs`, `edit_objects`. Every change to the objects, from the mouse,
the keys or the panel, goes through it:

```rust
let before = MapFields::of(document.map());
let mut objects = document.map().objects.clone();   // a copy
let changed = edit(&mut objects)?;                  // None: nothing happened
history.record_map_edit(label, before);
document.map_mut().objects = objects;               // only now is the map touched
```

The order matters. Many edits can turn out to change nothing: scaling an
object already at the limit, placing where there's no room, typing a
position that's out of bounds. `ObjectLayer`'s methods report that
(`update` returns `false`, `add` returns `None`). Recording first and
finding out afterwards would leave an empty undo step and mark the map
unsaved, so the edit runs on a copy and the map is touched only if it
changed something.

Callers that want the edit to be its own step call `history.seal()` after.

## 4. The mouse

`tool/mouse.rs`, `run_mouse`. It acts only on a left press over the 3D view
(`ScenePointer`), not while the right button is held.

### Place

1. The cursor's ground hit gives X and Z. Y is taken from
   `Terrain::height_at`, not from the hit itself: the ray's hit is a hair
   off the surface, and an object placed exactly on it has nothing for a
   later drag or drop to "correct".
2. With "Random turn" on, the yaw comes from a hash of how many objects this
   session has placed, so a wood doesn't look stamped.
3. `edit_objects` adds it, as one sealed step, and it becomes the selection.

### Select and drag

A press picks the object under the pointer (section 5) and selects it, or
selects nothing. If it picked one, a drag begins:

```rust
struct Drag {
    id: ObjectId,
    pressed_at: Vec2,   // window pixels
    offset: Vec2,       // object XZ minus the ground point grabbed
    lift: f32,          // how far above the ground it sits
    moving: bool,
}
```

and a recorded edit opens: `history.begin("Move object")`,
`capture_fields`.

While the button is held:

- Nothing moves until the pointer has travelled **4 pixels** from where it
  went down. A click that selects must not nudge.
- Then the object goes to `ground hit + offset`, at `ground height there +
  lift`. The offset means it doesn't jump to put its origin under the
  cursor; the lift means an object raised off the ground stays raised.
- The update runs on a copy and is written back only if it moved, so a
  frame that changes nothing leaves the map clean.

On release, `history.commit`. If nothing moved, there's no undo step: commit
drops an edit that changed nothing. The drag also ends if the history
stopped recording, which happens when another map is opened mid-drag.

A whole drag is one undo step, and undo is refused while it's in progress.

## 5. Picking

`pick.rs`, `pick_object`. Each object is treated as a sphere:
`Asset::bounds(scale)` gives a centre above the object's base and a radius
that holds the whole shape. The ray is tested against every sphere and the
nearest hit wins.

The ground hides what's behind it: an object whose sphere the ray enters
more than a metre past the ground hit is skipped. The metre of slack is
because spheres poke into the ground their objects stand on.

It's a linear scan over all objects: each frame in Select mode, for the
hover ring, and once per click.

## 6. Keys and the panel

`tool/edit.rs`, `object_keys` and `apply_edit`:

| Key | Edit |
|---|---|
| R / Shift + R | `Turn(±15°)` |
| `]` / `[` | `Scale(×1.1)` / `Scale(÷1.1)` |
| End | `Drop`: Y becomes the ground height under it |
| Delete | `Delete` |

Each is `edit_objects` plus `seal`, so each press is its own undo step, and
a press that changes nothing is no step. Keys are ignored mid-drag, since
the drag's recording owns the history. Backspace deliberately doesn't
delete: it's what you press to fix a number just typed in the panel.

`panel.rs` follows the shell's panel rule. It copies the selected object,
lets egui edit the copy's fields, and if the copy differs afterwards, passes
it to `edit_objects` under the label "Edit object". It doesn't seal, so
dragging a number is one undo step: the shell seals it when the UI goes
idle. The Drop and Delete buttons go through `apply_edit`, like the keys.

`ObjectLayer::update` is the last gate for typed values: it wraps yaw,
clamps scale, refuses positions out of bounds, and never lets an edit change
an object's id or asset.

## 7. Keeping the selection honest

`forget_vanished` runs in `View`, after `Edit`, which is where undo and file
actions happen. It clears the selection when:

- the selected id no longer exists (an undo took the object away), or
- the document's `generation` changed (another map was opened).

The second check is not redundant. Ids start over in every map, so the same
id in a different map is a different object, and a Delete would remove it.

## 8. Drawing

`draw.rs` draws gizmo rings on the ground: yellow for the selection (with a
line up through it), translucent white for the object under the pointer in
Select, cyan where a placement would land in Place.

The objects themselves are drawn by the shell (`lofi_editor/src/view/
objects.rs`), which keeps one entity per object in step with the map's list.
That's why the terrain editor shows them too.

## 9. Limits

- Every edit copies the whole object list (44 bytes per object), and the
  view then walks all of it; picking scans all of it. Fine for thousands.
  Bucketing objects by chunk is the fix when maps get dense.
- Objects don't follow the terrain. Sculpt under one in the terrain editor
  and it floats or sinks until it's dropped again here.
- No collision or placement rules beyond "on the map": objects can overlap,
  and the terrain's no-clutter and unwalkable flags aren't consulted.
- No multi-select, no copy and paste.
