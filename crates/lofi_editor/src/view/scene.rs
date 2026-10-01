//! Sun, sky and surface materials, kept in step with the map's light table
//! and the view settings.

use std::f32::consts::PI;

use bevy::camera::Exposure;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::pbr::wireframe::WireframeConfig;
use bevy::prelude::*;
use lofi_world::{CHUNK_CELLS, Lighting, MAX_MATERIALS, Pattern, WALKABLE_SLOPE_DEGREES};

use super::chunks::TerrainMaterialHandle;
use super::materials::{
    FLAG_CHUNK_GRID, FLAG_CRISP, FLAG_FLAT, FLAG_SLOPE, FogUniform, SkyMaterial, SkyParams,
    TerrainMaterial, TerrainParams, WaterMaterial, WaterParams, linear,
};
use super::settings::{Shading, ViewSettings};
use super::textures::{TEXTURE_SIZE, material_textures};
use crate::document::MapDocument;

#[derive(Component)]
pub struct Sun;

#[derive(Resource)]
pub struct WaterMaterialHandle(pub Handle<WaterMaterial>);

#[derive(Resource)]
pub struct SkyMaterialHandle(pub Handle<SkyMaterial>);

pub(super) fn spawn_scene(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut terrain: ResMut<Assets<TerrainMaterial>>,
    mut water: ResMut<Assets<WaterMaterial>>,
    mut sky: ResMut<Assets<SkyMaterial>>,
) {
    commands.spawn((
        Sun,
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 60.0,
            maximum_distance: 1_200.0,
            ..default()
        }
        .build(),
    ));
    // The custom materials do their own ambient; nothing else is lit by Bevy's.
    commands.insert_resource(GlobalAmbientLight {
        brightness: 0.0,
        ..default()
    });
    let textures = images.add(material_textures(&[Pattern::Plain]));
    commands.insert_resource(TerrainMaterialHandle(terrain.add(TerrainMaterial {
        params: TerrainParams::default(),
        textures,
    })));
    commands.insert_resource(WaterMaterialHandle(water.add(WaterMaterial {
        params: WaterParams::default(),
    })));
    commands.insert_resource(SkyMaterialHandle(sky.add(SkyMaterial {
        params: SkyParams::default(),
    })));
}

pub(super) fn sync_sun_and_sky(
    document: Res<MapDocument>,
    sun: Single<(&mut DirectionalLight, &mut Transform), With<Sun>>,
    sky: Res<SkyMaterialHandle>,
    mut skies: ResMut<Assets<SkyMaterial>>,
    mut clear: ResMut<ClearColor>,
) {
    let lighting = &document.map().lighting;
    let toward_sun = lighting.sun_direction();
    let (mut light, mut transform) = sun.into_inner();
    let [r, g, b] = lighting.sun_color;
    light.color = Color::srgb(r, g, b);
    // So the shader's Lambert term comes out as `sun_strength` of white.
    light.illuminance = lighting.sun_strength * PI / Exposure::default().exposure();
    let up = if toward_sun.y.abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    *transform = Transform::default().looking_to(-toward_sun, up);

    let [r, g, b] = lighting.sky_horizon;
    let horizon = Color::srgb(r, g, b);
    if clear.0 != horizon {
        clear.0 = horizon;
    }
    let params = SkyParams {
        zenith: linear(lighting.sky_zenith, 1.0),
        horizon: linear(lighting.sky_horizon, 1.0),
        sun_color: linear(lighting.sun_color, 1.0),
        sun_direction: toward_sun.extend(0.0),
    };
    if skies.get(&sky.0).is_some_and(|m| m.params != params)
        && let Some(mut material) = skies.get_mut(&sky.0)
    {
        material.params = params;
    }
}

/// Uniforms every frame the map or view changes; the texture array only when
/// the list of material patterns does.
pub(super) fn sync_terrain_material(
    document: Res<MapDocument>,
    settings: Res<ViewSettings>,
    handle: Res<TerrainMaterialHandle>,
    mut terrain: ResMut<Assets<TerrainMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut patterns: Local<Vec<Pattern>>,
) {
    let map = document.map();
    let current: Vec<Pattern> = map.materials.iter().map(|m| m.pattern).collect();
    if *patterns != current
        && let Some(mut material) = terrain.get_mut(&handle.0)
    {
        material.textures = images.add(material_textures(&current));
        *patterns = current;
    }

    let mut colors = [Vec4::new(0.5, 0.5, 0.5, 0.25); MAX_MATERIALS];
    for (slot, material) in colors.iter_mut().zip(map.materials.iter()) {
        *slot = linear(material.color, 1.0 / material.tile_size);
    }
    let lighting = &map.lighting;
    let mut flags = 0;
    if settings.shading == Shading::Flat {
        flags |= FLAG_FLAT;
    }
    if settings.crisp_texels {
        flags |= FLAG_CRISP;
    }
    if settings.overlays.slope {
        flags |= FLAG_SLOPE;
    }
    if settings.overlays.chunk_grid {
        flags |= FLAG_CHUNK_GRID;
    }
    let params = TerrainParams {
        colors,
        sky_ambient: linear(lighting.sky_ambient, 1.0) * lighting.ambient_strength,
        ground_ambient: linear(lighting.ground_ambient, 1.0) * lighting.ambient_strength,
        fog: fog_uniform(lighting, settings.fog),
        flags,
        texture_size: TEXTURE_SIZE as f32,
        chunk_size: CHUNK_CELLS as f32 * map.terrain.cell_size(),
        walkable_cos: WALKABLE_SLOPE_DEGREES.to_radians().cos(),
    };
    if terrain.get(&handle.0).is_some_and(|m| m.params != params)
        && let Some(mut material) = terrain.get_mut(&handle.0)
    {
        material.params = params;
    }
}

pub(super) fn sync_water_material(
    document: Res<MapDocument>,
    settings: Res<ViewSettings>,
    handle: Res<WaterMaterialHandle>,
    mut water: ResMut<Assets<WaterMaterial>>,
) {
    let map = document.map();
    let lighting = &map.lighting;
    // Deep water is opaque: the open sea around the map has no floor, and
    // any see-through would show where the map's floor ends.
    let params = WaterParams {
        shallow: linear(map.water.shallow, 0.55),
        deep: linear(map.water.deep, 1.0),
        ambient: linear(lighting.sky_ambient, 1.0) * lighting.ambient_strength,
        fog: fog_uniform(lighting, settings.fog),
        depth_range: map.water.depth,
        level: map.water.level,
    };
    if water.get(&handle.0).is_some_and(|m| m.params != params)
        && let Some(mut material) = water.get_mut(&handle.0)
    {
        material.params = params;
    }
}

pub(super) fn apply_wireframe(
    settings: Res<ViewSettings>,
    wireframe: Option<ResMut<WireframeConfig>>,
) {
    if let Some(mut wireframe) = wireframe {
        wireframe.global = settings.wireframe;
    }
}

/// Horizon-coloured fog. Turned off in the view, it stays off whatever the map says.
pub(super) fn fog_uniform(lighting: &Lighting, enabled: bool) -> FogUniform {
    FogUniform {
        color: linear(lighting.sky_horizon, if enabled { 1.0 } else { 0.0 }),
        distance: Vec4::new(
            lighting.fog_start,
            lighting.fog_end,
            lighting.height_fog_density,
            lighting.height_fog_falloff,
        ),
    }
}
