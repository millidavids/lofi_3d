//! lofi terrain: sculpt, paint and save lofi world maps.

mod panel;
mod sculpt;
mod select;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use lofi_editor::app::editor_app;
use lofi_editor::launch::{LaunchOptions, USAGE};
use lofi_editor::ui::UiSystems;

fn main() -> AppExit {
    let options = match LaunchOptions::parse(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("{err}\n{USAGE}");
            return AppExit::from_code(2);
        }
    };
    let mut app = editor_app("lofi terrain", options);
    app.add_plugins((sculpt::SculptPlugin, select::SelectPlugin))
        .add_systems(
            EguiPrimaryContextPass,
            panel::tool_panel.in_set(UiSystems::Panels),
        );
    app.run()
}
