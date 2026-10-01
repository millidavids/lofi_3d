//! A dome around the camera, drawn with the sky material.

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::pbr::wireframe::NoWireframe;
use bevy::prelude::*;

use super::scene::SkyMaterialHandle;
use crate::camera::EditorCamera;

/// Inside the camera's far plane, outside any map.
pub const SKY_RADIUS: f32 = 40_000.0;

#[derive(Component)]
pub struct SkyDome;

pub(super) fn spawn_sky(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    material: Res<SkyMaterialHandle>,
) {
    commands.spawn((
        SkyDome,
        Mesh3d(meshes.add(Sphere::new(1.0).mesh().uv(48, 24))),
        MeshMaterial3d(material.0.clone()),
        Transform::from_scale(Vec3::splat(SKY_RADIUS)),
        NotShadowCaster,
        NotShadowReceiver,
        NoWireframe,
    ));
}

pub(super) fn follow_camera(
    camera: Single<&Transform, (With<EditorCamera>, Without<SkyDome>)>,
    mut sky: Single<&mut Transform, With<SkyDome>>,
) {
    sky.translation = camera.translation;
}
