//! What's under the mouse: the pointer's ray and where it meets the ground.

use bevy::prelude::*;

use crate::camera::EditorCamera;
use crate::document::MapDocument;
use crate::schedule::EditorSystems;
use crate::ui::ScenePointer;

/// Far enough to cross the largest map diagonally.
const MAX_PICK_DISTANCE: f32 = 50_000.0;

/// Updated every frame in [`EditorSystems::Cursor`].
#[derive(Resource, Default)]
pub struct GroundCursor {
    /// The pointer's ray into the scene, if it's over the scene.
    pub ray: Option<Ray3d>,
    /// Where that ray meets the ground. Holes count as ground here, so a
    /// tool can point at one to fill it.
    pub hit: Option<Vec3>,
}

pub struct PickPlugin;

impl Plugin for PickPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GroundCursor>()
            .add_systems(Update, update_ground_cursor.in_set(EditorSystems::Cursor));
    }
}

fn update_ground_cursor(
    pointer: Res<ScenePointer>,
    camera: Single<(&Camera, &Transform), With<EditorCamera>>,
    document: Res<MapDocument>,
    mut cursor: ResMut<GroundCursor>,
) {
    let (camera, transform) = *camera;
    // `Transform`, not `GlobalTransform`: the camera moved earlier this frame
    // and its global transform won't catch up until `PostUpdate`.
    let camera_transform = GlobalTransform::from(*transform);
    cursor.ray = pointer
        .position
        .and_then(|position| camera.viewport_to_world(&camera_transform, position).ok());
    cursor.hit = cursor.ray.and_then(|ray| {
        document
            .terrain()
            .raycast_solid(ray.origin, *ray.direction, MAX_PICK_DISTANCE)
    });
}
