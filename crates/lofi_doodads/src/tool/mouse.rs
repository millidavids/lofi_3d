//! The left button: place, select, drag.

use std::f32::consts::TAU;

use bevy::prelude::*;
use lofi_editor::document::{EditHistory, MapDocument, StatusLine};
use lofi_editor::pick::GroundCursor;
use lofi_editor::ui::ScenePointer;
use lofi_world::{ObjectId, hash01};

use super::edit::edit_objects;
use super::settings::{DoodadSettings, DoodadTool, SelectedObject};
use crate::pick::pick_object;

/// How far the pointer travels before a press becomes a drag, so a click
/// that selects doesn't also nudge.
const DRAG_START_PIXELS: f32 = 4.0;

/// The object being dragged, if any.
#[derive(Resource, Default)]
pub struct ObjectDrag(Option<Drag>);

struct Drag {
    id: ObjectId,
    /// Where the pointer went down, in window pixels.
    pressed_at: Vec2,
    /// The object's place relative to the ground point grabbed.
    offset: Vec2,
    /// How far above the ground it sits; kept as it moves.
    lift: f32,
    /// The pointer has travelled far enough to mean it.
    moving: bool,
}

pub(crate) fn run_mouse(
    mouse: Res<ButtonInput<MouseButton>>,
    pointer: Res<ScenePointer>,
    cursor: Res<GroundCursor>,
    mut settings: ResMut<DoodadSettings>,
    mut selected: ResMut<SelectedObject>,
    mut drag: ResMut<ObjectDrag>,
    mut document: ResMut<MapDocument>,
    mut history: ResMut<EditHistory>,
    mut status: ResMut<StatusLine>,
) {
    let pressed = mouse.just_pressed(MouseButton::Left) && !mouse.pressed(MouseButton::Right);

    // A drag in progress: follow the ground, or end it.
    if let Some(held) = drag.0.as_mut() {
        // Let go, or the history was cleared by opening another map.
        if !mouse.pressed(MouseButton::Left) || !history.is_recording() {
            history.commit(document.map());
            drag.0 = None;
            return;
        }
        if pointer
            .position
            .is_some_and(|at| at.distance(held.pressed_at) > DRAG_START_PIXELS)
        {
            held.moving = true;
        }
        if held.moving
            && let Some(hit) = cursor.hit
        {
            let xz = hit.xz() + held.offset;
            let ground = document.terrain().height_at(xz).unwrap_or(hit.y);
            let target = Vec3::new(xz.x, ground + held.lift, xz.y);
            // On a copy, so a frame that moves nothing leaves the map clean.
            let mut objects = document.map().objects.clone();
            if objects.update(held.id, |o| o.position = target) {
                document.map_mut().objects = objects;
            }
        }
        return;
    }
    let Some(pressed_at) = pointer.position.filter(|_| pressed) else {
        return;
    };
    if document.map().objects.is_foreign() {
        status.error("These objects were saved by a newer tool; editing them here is off.");
        return;
    }

    match settings.tool {
        DoodadTool::Place => {
            let Some(hit) = cursor.hit else {
                return;
            };
            settings.placed = settings.placed.wrapping_add(1);
            let yaw = if settings.random_yaw {
                hash01(settings.placed as i32, hit.x as i32, 41) * TAU
            } else {
                0.0
            };
            // On the surface itself, so a later drop or drag has nothing to
            // correct: the ray's hit is a hair off it.
            let ground = document.terrain().height_at(hit.xz()).unwrap_or(hit.y);
            let at = Vec3::new(hit.x, ground, hit.z);
            let (asset, scale) = (settings.asset, settings.scale);
            let placed = edit_objects(&mut document, &mut history, "Place object", |objects| {
                objects.add(asset, at, yaw, scale)
            });
            history.seal();
            match placed {
                Some(id) => selected.0 = Some(id),
                None => status.error("No room for another object here."),
            }
        }
        DoodadTool::Select => {
            let Some(ray) = cursor.ray else {
                return;
            };
            let ground = cursor.hit.map(|h| h.distance(ray.origin));
            selected.0 = pick_object(&document.map().objects, ray, ground);
            if let (Some(id), Some(hit)) = (selected.0, cursor.hit)
                && let Some(o) = document.map().objects.get(id)
            {
                let below = document.terrain().height_at(o.position.xz());
                let held = Drag {
                    id,
                    pressed_at,
                    offset: o.position.xz() - hit.xz(),
                    lift: below.map_or(0.0, |ground| o.position.y - ground),
                    moving: false,
                };
                history.begin("Move object");
                history.capture_fields(document.map());
                drag.0 = Some(held);
            }
        }
    }
}
