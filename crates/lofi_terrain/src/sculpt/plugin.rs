use bevy::prelude::*;
use bevy_egui::input::egui_wants_any_keyboard_input;
use lofi_editor::document::no_prompt_open;
use lofi_editor::schedule::EditorSystems;

use super::adjust::{brush_keys, brush_wheel};
use super::brush::BrushSettings;
use super::preview::draw_brush;
use super::stroke::{ActiveStroke, run_stroke};

pub struct SculptPlugin;

impl Plugin for SculptPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BrushSettings>()
            .init_resource::<ActiveStroke>()
            .add_systems(
                Update,
                (
                    (
                        brush_keys
                            .run_if(not(egui_wants_any_keyboard_input))
                            .run_if(no_prompt_open),
                        brush_wheel,
                    )
                        .in_set(EditorSystems::Input),
                    run_stroke.in_set(EditorSystems::Stroke),
                    draw_brush.in_set(EditorSystems::View),
                ),
            );
    }
}
