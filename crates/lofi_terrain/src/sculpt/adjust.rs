//! Keyboard and wheel shortcuts for the brush.

use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;

use super::brush::{BrushSettings, MAX_RADIUS, MIN_RADIUS, Tool};
use lofi_editor::ui::ScenePointer;

/// Radius multiplier per `[`/`]` press or wheel notch.
const RADIUS_STEP: f32 = 1.15;
/// Touchpads report pixels; this many make one notch.
const PIXELS_PER_NOTCH: f32 = 40.0;

const TOOL_KEYS: [KeyCode; Tool::ALL.len()] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
];

pub(super) fn brush_keys(keys: Res<ButtonInput<KeyCode>>, mut brush: ResMut<BrushSettings>) {
    for (key, tool) in TOOL_KEYS.iter().zip(Tool::ALL) {
        if keys.just_pressed(*key) {
            brush.tool = tool;
        }
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        scale_radius(&mut brush, 1.0 / RADIUS_STEP);
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        scale_radius(&mut brush, RADIUS_STEP);
    }
}

/// Alt + wheel resizes the brush. Plain wheel belongs to the camera.
pub(super) fn brush_wheel(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    pointer: Res<ScenePointer>,
    mut brush: ResMut<BrushSettings>,
) {
    if pointer.position.is_none() || !keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]) {
        return;
    }
    let notches = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / PIXELS_PER_NOTCH,
    };
    if notches != 0.0 {
        scale_radius(&mut brush, RADIUS_STEP.powf(notches));
    }
}

fn scale_radius(brush: &mut BrushSettings, factor: f32) {
    brush.radius = (brush.radius * factor).clamp(MIN_RADIUS, MAX_RADIUS);
}
