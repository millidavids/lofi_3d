//! Editor fly camera: right-drag to look, WASD to fly, middle-drag to pan,
//! wheel to dolly.

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::document::MapDocument;
use crate::ui::ScenePointer;

const LOOK_SENSITIVITY: f32 = 0.003;
const MAX_PITCH: f32 = 1.54;
/// Metres per second, before height scaling and the Shift boost.
const FLY_SPEED: f32 = 30.0;
const BOOST: f32 = 4.0;
/// Never closer to the ground than this.
const MIN_CLEARANCE: f32 = 1.0;
/// Touchpads report pixels; this many make one wheel notch.
const PIXELS_PER_NOTCH: f32 = 40.0;
/// Beyond the sky dome, which sits past any map.
const FAR_PLANE: f32 = 60_000.0;

#[derive(Component, Default)]
pub struct EditorCamera {
    yaw: f32,
    pitch: f32,
    looking: bool,
    panning: bool,
}

impl EditorCamera {
    /// Points the camera from `eye` toward `target`.
    pub fn aim(&mut self, transform: &mut Transform, eye: Vec3, target: Vec3) {
        *transform = Transform::from_translation(eye).looking_at(target, Vec3::Y);
        let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
        self.yaw = yaw;
        self.pitch = pitch;
    }
}

pub(super) fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        EditorCamera::default(),
        Projection::from(PerspectiveProjection {
            far: FAR_PLANE,
            ..default()
        }),
        // Colours are authored for the screen; tonemapping would shift them.
        Tonemapping::None,
    ));
}

pub(super) fn look(
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    pointer: Res<ScenePointer>,
    camera: Single<(&mut Transform, &mut EditorCamera)>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let (mut transform, mut camera) = camera.into_inner();
    if mouse.just_pressed(MouseButton::Right) && pointer.position.is_some() {
        camera.looking = true;
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
    if camera.looking && !mouse.pressed(MouseButton::Right) {
        camera.looking = false;
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
    if !camera.looking || motion.delta == Vec2::ZERO {
        return;
    }
    camera.yaw -= motion.delta.x * LOOK_SENSITIVITY;
    camera.pitch = (camera.pitch - motion.delta.y * LOOK_SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);
    transform.rotation = Quat::from_euler(EulerRot::YXZ, camera.yaw, camera.pitch, 0.0);
}

/// WASD, Q/E and the wheel. Speed grows with height so crossing a big map
/// from high up doesn't crawl, and fine work near the ground stays precise.
pub(super) fn fly(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    pointer: Res<ScenePointer>,
    document: Res<MapDocument>,
    camera: Single<&mut Transform, With<EditorCamera>>,
) {
    let mut transform = camera.into_inner();
    let clearance = height_above_ground(&document, transform.translation);
    let speed = FLY_SPEED * (1.0 + clearance / 60.0);

    // Ctrl is the brush modifier and the shortcut key; don't fly on Ctrl+S.
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let mut direction = Vec3::ZERO;
    if !ctrl {
        let forward = *transform.forward();
        let right = *transform.right();
        for (key, dir) in [
            (KeyCode::KeyW, forward),
            (KeyCode::KeyS, -forward),
            (KeyCode::KeyD, right),
            (KeyCode::KeyA, -right),
            (KeyCode::KeyE, Vec3::Y),
            (KeyCode::KeyQ, Vec3::NEG_Y),
        ] {
            if keys.pressed(key) {
                direction += dir;
            }
        }
    }
    let boost = if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
        BOOST
    } else {
        1.0
    };
    transform.translation += direction.normalize_or_zero() * speed * boost * time.delta_secs();

    // Alt + wheel is brush size.
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    if pointer.position.is_some() && !alt {
        let notches = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / PIXELS_PER_NOTCH,
        };
        let step = (clearance * 0.15).max(2.0);
        let forward = *transform.forward();
        transform.translation += forward * notches * step;
    }
}

/// Middle-drag slides the view sideways and up/down.
pub(super) fn pan(
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    pointer: Res<ScenePointer>,
    document: Res<MapDocument>,
    camera: Single<(&mut Transform, &mut EditorCamera)>,
) {
    let (mut transform, mut camera) = camera.into_inner();
    if mouse.just_pressed(MouseButton::Middle) && pointer.position.is_some() {
        camera.panning = true;
    }
    if !mouse.pressed(MouseButton::Middle) {
        camera.panning = false;
    }
    if !camera.panning {
        return;
    }
    let per_pixel = height_above_ground(&document, transform.translation).max(5.0) * 0.0025;
    let right = *transform.right();
    let up = *transform.up();
    transform.translation += (-right * motion.delta.x + up * motion.delta.y) * per_pixel;
}

pub(super) fn stay_above_ground(
    document: Res<MapDocument>,
    camera: Single<&mut Transform, With<EditorCamera>>,
) {
    let mut transform = camera.into_inner();
    if let Some(ground) = document.terrain().height_at(transform.translation.xz()) {
        transform.translation.y = transform.translation.y.max(ground + MIN_CLEARANCE);
    }
}

fn height_above_ground(document: &MapDocument, position: Vec3) -> f32 {
    let ground = document.terrain().height_at(position.xz()).unwrap_or(0.0);
    (position.y - ground).max(0.0)
}
