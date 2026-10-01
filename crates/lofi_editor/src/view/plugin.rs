use bevy::prelude::*;
use bevy::shader::load_shader_library;

use super::chunks::{
    ChunkEntities, ChunksRebuiltMessage, mark_all_on_layout_change, remesh_changed_chunks,
    respawn_chunks_for_new_map,
};
use super::materials::{ObjectMaterial, SkyMaterial, TerrainMaterial, WaterMaterial};
use super::objects::{ObjectEntities, spawn_object_material, sync_object_material, sync_objects};
use super::overlays::{draw_cell_flags, draw_full_chunks};
use super::scene::{
    apply_wireframe, spawn_scene, sync_sun_and_sky, sync_terrain_material, sync_water_material,
};
use super::settings::ViewSettings;
use super::sky::{follow_camera, spawn_sky};
use super::water::{WaterEntities, sync_water};
use crate::document::MapDocument;
use crate::schedule::EditorSystems;

pub struct ViewPlugin;

/// Every tool's lines: brushes, selections, overlays.
fn configure_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<DefaultGizmoConfigGroup>();
    config.line.width = 2.0;
    // Stay visible through hills in front of them.
    config.depth_bias = -1.0;
}

impl Plugin for ViewPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "shaders/fog.wgsl");
        bevy::asset::embedded_asset!(app, "shaders/terrain.wgsl");
        bevy::asset::embedded_asset!(app, "shaders/water.wgsl");
        bevy::asset::embedded_asset!(app, "shaders/sky.wgsl");
        bevy::asset::embedded_asset!(app, "shaders/objects.wgsl");

        let look_changed =
            resource_changed::<MapDocument>.or_else(resource_changed::<ViewSettings>);
        app.add_plugins((
            MaterialPlugin::<TerrainMaterial>::default(),
            MaterialPlugin::<WaterMaterial>::default(),
            MaterialPlugin::<SkyMaterial>::default(),
            MaterialPlugin::<ObjectMaterial>::default(),
        ))
        .init_resource::<ViewSettings>()
        .init_resource::<ChunkEntities>()
        .init_resource::<WaterEntities>()
        .init_resource::<ObjectEntities>()
        .add_message::<ChunksRebuiltMessage>()
        .add_systems(
            Startup,
            (
                (spawn_scene, spawn_sky).chain(),
                spawn_object_material,
                configure_gizmos,
            ),
        )
        .add_systems(
            Update,
            (
                (
                    // Before the respawn, which drops what a new map marks.
                    mark_all_on_layout_change,
                    respawn_chunks_for_new_map,
                    remesh_changed_chunks,
                    sync_water,
                )
                    .chain(),
                sync_objects,
                (
                    sync_sun_and_sky,
                    sync_terrain_material,
                    sync_water_material,
                    sync_object_material,
                )
                    .run_if(look_changed),
                apply_wireframe.run_if(resource_changed::<ViewSettings>),
                follow_camera,
                draw_full_chunks,
                draw_cell_flags,
            )
                .in_set(EditorSystems::View),
        );
    }
}
