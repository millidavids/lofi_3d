use bevy::prelude::*;
use bevy_egui::input::egui_wants_any_keyboard_input;

use super::fly::{fly, look, pan, spawn_camera, stay_above_ground};
use super::framing::{FrameMapMessage, frame_map};
use crate::document::no_prompt_open;
use crate::schedule::EditorSystems;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<FrameMapMessage>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (
                    look,
                    fly.run_if(not(egui_wants_any_keyboard_input))
                        .run_if(no_prompt_open),
                    pan,
                    frame_map,
                    stay_above_ground,
                )
                    .chain()
                    .in_set(EditorSystems::Camera),
            );
    }
}
