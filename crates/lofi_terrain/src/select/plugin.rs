use bevy::prelude::*;
use bevy_egui::input::egui_wants_any_keyboard_input;
use lofi_editor::document::no_prompt_open;
use lofi_editor::schedule::EditorSystems;

use super::draw::draw_selection;
use super::selection::Selection;
use super::tool::{SelectDrag, SelectSettings, forget_on_new_map, run_select, select_keys};

pub struct SelectPlugin;

impl Plugin for SelectPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selection>()
            .init_resource::<SelectDrag>()
            .init_resource::<SelectSettings>()
            .add_systems(
                Update,
                (
                    select_keys
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(no_prompt_open)
                        .in_set(EditorSystems::Input),
                    run_select.in_set(EditorSystems::Stroke),
                    // Right after Edit opens a map, before the panel or the
                    // next stroke can use a selection made for the last one.
                    (forget_on_new_map, draw_selection)
                        .chain()
                        .in_set(EditorSystems::View),
                ),
            );
    }
}
