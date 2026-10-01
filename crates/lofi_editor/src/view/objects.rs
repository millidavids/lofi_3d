//! One entity per placed object, kept in step with the map's object list.

use std::collections::HashMap;

use bevy::prelude::*;
use lofi_world::{AssetGuid, ObjectId, ObjectList, asset};

use super::materials::{ObjectMaterial, ObjectParams, linear};
use super::scene::fog_uniform;
use super::settings::ViewSettings;
use super::shapes::asset_mesh;
use crate::document::MapDocument;

/// Which object an entity shows.
#[derive(Component, Clone, Copy, Debug)]
pub struct ObjectView(pub ObjectId);

#[derive(Resource)]
pub struct ObjectMaterialHandle(pub Handle<ObjectMaterial>);

#[derive(Resource, Default)]
pub struct ObjectEntities {
    generation: Option<u64>,
    /// The list the entities match; unchanged lists skip the diff.
    list: Option<ObjectList>,
    entities: HashMap<ObjectId, Entity>,
    meshes: HashMap<AssetGuid, Handle<Mesh>>,
}

pub(super) fn spawn_object_material(
    mut commands: Commands,
    mut materials: ResMut<Assets<ObjectMaterial>>,
) {
    let handle = materials.add(ObjectMaterial {
        params: ObjectParams::default(),
    });
    commands.insert_resource(ObjectMaterialHandle(handle));
}

pub(super) fn sync_objects(
    mut commands: Commands,
    document: Res<MapDocument>,
    material: Res<ObjectMaterialHandle>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut view: ResMut<ObjectEntities>,
    mut transforms: Query<&mut Transform, With<ObjectView>>,
) {
    let generation = document.generation();
    let layer = &document.map().objects;
    if view.generation != Some(generation) {
        for (_, entity) in view.entities.drain() {
            commands.entity(entity).despawn();
        }
        view.generation = Some(generation);
        view.list = None;
    }
    let list = layer.list();
    if view.list.as_ref().is_some_and(|last| last.shares(&list)) {
        return;
    }

    let mut alive = HashMap::with_capacity(layer.len());
    for o in layer.iter() {
        let transform = Transform {
            translation: o.position,
            rotation: Quat::from_rotation_y(o.yaw),
            scale: Vec3::splat(o.scale),
        };
        let existing = view.entities.remove(&o.id);
        let entity = match existing {
            Some(entity) => {
                if let Ok(mut current) = transforms.get_mut(entity) {
                    current.set_if_neq(transform);
                }
                entity
            }
            None => {
                let catalog = asset(o.asset);
                // Unknown GUIDs all share the placeholder box.
                let key = if catalog.shape == lofi_world::Shape::Unknown {
                    AssetGuid(0)
                } else {
                    o.asset
                };
                let mesh = view
                    .meshes
                    .entry(key)
                    .or_insert_with(|| meshes.add(asset_mesh(&catalog)))
                    .clone();
                commands
                    .spawn((
                        ObjectView(o.id),
                        Mesh3d(mesh),
                        MeshMaterial3d(material.0.clone()),
                        transform,
                    ))
                    .id()
            }
        };
        alive.insert(o.id, entity);
    }
    // Whatever's left was removed from the map.
    for (_, entity) in view.entities.drain() {
        commands.entity(entity).despawn();
    }
    view.entities = alive;
    view.list = Some(list);
}

pub(super) fn sync_object_material(
    document: Res<MapDocument>,
    settings: Res<ViewSettings>,
    handle: Res<ObjectMaterialHandle>,
    mut materials: ResMut<Assets<ObjectMaterial>>,
) {
    let lighting = &document.map().lighting;
    let params = ObjectParams {
        sky_ambient: linear(lighting.sky_ambient, 1.0) * lighting.ambient_strength,
        ground_ambient: linear(lighting.ground_ambient, 1.0) * lighting.ambient_strength,
        fog: fog_uniform(lighting, settings.fog),
    };
    if materials.get(&handle.0).is_some_and(|m| m.params != params)
        && let Some(mut material) = materials.get_mut(&handle.0)
    {
        material.params = params;
    }
}
