# lofi_editor

The shell both editors are built on. It owns everything that isn't specific
to one tool: the map being edited, undo, files, autosave and crash recovery,
the camera, drawing the world, the menu and status bars, and the prompts.

A tool starts from `app::editor_app`, adds its own systems and a side panel,
and runs. `lofi_terrain` and `lofi_doodads` are each a few hundred lines on
top of this.

How it works inside: [PRIMER.md](PRIMER.md).

## What's in it

| Module | What it holds |
|---|---|
| `app` | `editor_app`: the window, the engine plugins and the shell's own. |
| `schedule` | `EditorSystems`: the order of a frame. |
| `launch` | Command-line options. |
| `document` | The map being edited (`MapDocument`), undo (`EditHistory`), file actions and dialogs, locks, autosave, recovery, heightmap import and export, cooking. |
| `camera` | The fly camera, and framing the map. |
| `pick` | `GroundCursor`: the pointer's ray and where it meets the ground. |
| `view` | Chunk meshes, water, sky, objects and overlays; their materials and shaders; generated textures. |
| `ui` | Menu bar, status bar, prompts, shortcuts, layout, and widgets that don't dirty the map. |
| `capture` | `--screenshot` and `--measure`. |

## Building a tool on it

```rust
use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use lofi_editor::app::editor_app;
use lofi_editor::launch::{LaunchOptions, USAGE};
use lofi_editor::schedule::EditorSystems;
use lofi_editor::ui::UiSystems;

fn main() -> AppExit {
    let options = match LaunchOptions::parse(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("{err}\n{USAGE}");
            return AppExit::from_code(2);
        }
    };
    let mut app = editor_app("my tool", options);
    app.add_systems(Update, my_tool.in_set(EditorSystems::Stroke))
        .add_systems(EguiPrimaryContextPass, my_panel.in_set(UiSystems::Panels));
    app.run()
}
```

What a tool reads and writes:

| Resource | For |
|---|---|
| `document::MapDocument` | The map. `map()` to read, `map_mut()` or `edit_terrain()` to change. |
| `document::EditHistory` | Recording every change so it can be undone. |
| `document::TerrainChanges` | Telling the view which terrain to rebuild. |
| `document::StatusLine` | The message in the status bar. |
| `pick::GroundCursor` | What the pointer is on. |
| `ui::ScenePointer` | Whether the pointer is over the 3D view at all. |
| `ui::UiLayout` | Claiming screen space for a panel. |

Rules a tool must follow are in the primer's last section.

## Command line

Every tool built on the shell takes these:

| Option | Does |
|---|---|
| `MAP.lofimap` | Opens it on start. |
| `--screenshot OUT.png` | Frames the map, saves a picture after 30 frames, quits. |
| `--camera EX,EY,EZ,TX,TY,TZ` | Starts with the eye at E looking at T. |
| `--flat` | Starts in flat shading. |
| `--overlays slope,grid,full,cells` | Starts with overlays on. |
| `--measure` | Times 300 frames with the whole map in view, prints, quits. Use a release build. |

`--screenshot` and `--measure` read the map without locking it, and exit 1
if it doesn't load.

## Tests

```sh
cargo test -p lofi_editor
```

Document logic (undo, file actions, locks, autosave, sidecars) is tested
headless, with real temp files where files matter. Nothing here needs a
window or a GPU.

To look at the generated material textures:

```sh
LOFI_TEXTURE_DUMP=/tmp/patterns cargo test -p lofi_editor dump_patterns -- --ignored
```
