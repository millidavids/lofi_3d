//! lofi doodads: place trees, rocks and buildings on lofi world maps.
//!
//! Built on the same shell as the terrain editor, so files, undo, autosave,
//! the camera and the view work the same. Edits the same `.lofimap`; a map
//! can only be open in one editor at a time.

mod draw;
mod panel;
mod pick;
mod tool;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use bevy_egui::input::egui_wants_any_keyboard_input;
use lofi_editor::app::editor_app;
use lofi_editor::document::no_prompt_open;
use lofi_editor::launch::{LaunchOptions, USAGE};
use lofi_editor::schedule::EditorSystems;
use lofi_editor::ui::UiSystems;

use tool::{DoodadSettings, ObjectDrag, SelectedObject};

fn main() -> AppExit {
    let options = match LaunchOptions::parse(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("{err}\n{USAGE}");
            return AppExit::from_code(2);
        }
    };
    let mut app = editor_app("lofi doodads", options);
    app.init_resource::<DoodadSettings>()
        .init_resource::<SelectedObject>()
        .init_resource::<ObjectDrag>()
        .add_systems(
            Update,
            (
                tool::object_keys
                    .run_if(not(egui_wants_any_keyboard_input))
                    .run_if(no_prompt_open)
                    .in_set(EditorSystems::Input),
                tool::run_mouse.in_set(EditorSystems::Stroke),
                // After Edit, which undoes and opens maps.
                (tool::forget_vanished, draw::draw_object_rings)
                    .chain()
                    .in_set(EditorSystems::View),
            ),
        )
        .add_systems(
            EguiPrimaryContextPass,
            panel::doodad_panel.in_set(UiSystems::Panels),
        );
    app.run()
}
