//! Water: a surface per chunk carrying the ground height under each vertex,
//! and open sea around the map that fades into the fog. The shader takes
//! depth as sea level minus ground, so moving the sea level rebuilds nothing.

use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::wireframe::NoWireframe;
use bevy::prelude::*;
use bevy::tasks::{ComputeTaskPool, TaskPool};
use lofi_world::{ChunkCoord, Terrain, WorldMap};

use super::chunks::ChunksRebuiltMessage;
use super::scene::WaterMaterialHandle;
use crate::document::MapDocument;

/// Ground height the open sea reports: deep under any sea level.
const OPEN_SEA_FLOOR: f32 = -100_000.0;
/// The open sea reaches this many map widths past each edge, up to a cap
/// that stays inside the sky dome.
const SEA_MARGIN: f32 = 4.0;
const MAX_SEA_MARGIN: f32 = 30_000.0;

#[derive(Component)]
pub struct WaterSurface;

#[derive(Resource, Default)]
pub struct WaterEntities {
    generation: Option<u64>,
    /// Sea level and switch the surfaces were last placed for.
    placed_for: Option<(f32, bool)>,
    chunks: Vec<Entity>,
    /// Each chunk's lowest ground; the chunk shows water where that's under
    /// the sea.
    lowest: Vec<f32>,
    sea: Option<Entity>,
}

/// Rebuilds water where the terrain's heights or triangles changed, moves it
/// with the sea level, and shows or hides it with the map's water switch.
pub(super) fn sync_water(
    mut commands: Commands,
    document: Res<MapDocument>,
    material: Res<WaterMaterialHandle>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut water: ResMut<WaterEntities>,
    mut rebuilt: MessageReader<ChunksRebuiltMessage>,
    mut surfaces: Query<(&Mesh3d, &mut Visibility, &mut Transform), With<WaterSurface>>,
) {
    let map = document.map();
    let size = map.terrain.size();
    if water.generation != Some(document.generation()) {
        let sea = water.sea.take();
        for entity in water.chunks.drain(..).chain(sea) {
            commands.entity(entity).despawn();
        }
        let all: Vec<ChunkCoord> = size.chunks().collect();
        water.lowest.clear();
        for (&chunk, (mesh, lowest)) in all.iter().zip(build_meshes(&map.terrain, &all)) {
            let entity = spawn_surface(
                &mut commands,
                meshes.add(mesh),
                &material,
                map.terrain.chunk_origin(chunk),
            );
            water.chunks.push(entity);
            water.lowest.push(lowest);
        }
        water.sea = Some(spawn_surface(
            &mut commands,
            meshes.add(open_sea_mesh(map)),
            &material,
            Vec3::ZERO,
        ));
        water.generation = Some(document.generation());
        // Spawned at height zero and hidden; placed below once they exist.
        water.placed_for = None;
        rebuilt.clear();
        return;
    }

    let mut todo: Vec<ChunkCoord> = Vec::new();
    for message in rebuilt.read() {
        match message {
            ChunksRebuiltMessage::All => todo = size.chunks().collect(),
            ChunksRebuiltMessage::Some(chunks) => todo.extend(chunks),
        }
    }
    todo.sort_unstable_by_key(|c| size.chunk_index(*c));
    todo.dedup();
    for (&chunk, (mesh, lowest)) in todo.iter().zip(build_meshes(&map.terrain, &todo)) {
        let index = size.chunk_index(chunk);
        let Some(&entity) = water.chunks.get(index) else {
            continue;
        };
        if let Ok((handle, _, _)) = surfaces.get(entity)
            && let Some(mut asset) = meshes.get_mut(&handle.0)
        {
            *asset = mesh;
        }
        water.lowest[index] = lowest;
    }

    let placement = (map.water.level, map.water.enabled);
    let replace_all = water.placed_for != Some(placement);
    let (level, enabled) = placement;
    let show = |lowest: f32| visible(enabled && lowest < level);
    if replace_all {
        for (index, &entity) in water.chunks.iter().enumerate() {
            if let Ok((_, mut visibility, mut transform)) = surfaces.get_mut(entity) {
                transform.translation.y = level;
                visibility.set_if_neq(show(water.lowest[index]));
            }
        }
        if let Some(sea) = water.sea
            && let Ok((_, mut visibility, mut transform)) = surfaces.get_mut(sea)
        {
            transform.translation.y = level;
            visibility.set_if_neq(visible(enabled));
        }
        water.placed_for = Some(placement);
    } else {
        for chunk in todo {
            let index = size.chunk_index(chunk);
            if let Ok((_, mut visibility, _)) = surfaces.get_mut(water.chunks[index]) {
                visibility.set_if_neq(show(water.lowest[index]));
            }
        }
    }
}

/// Builds chunk surfaces on the compute pool, in order, each with its lowest
/// ground.
fn build_meshes(terrain: &Terrain, chunks: &[ChunkCoord]) -> Vec<(Mesh, f32)> {
    let pool = ComputeTaskPool::get_or_init(TaskPool::default);
    let batch = chunks.len().div_ceil(pool.thread_num().max(1) * 4).max(1);
    pool.scope(|scope| {
        for part in chunks.chunks(batch) {
            scope.spawn(async move {
                part.iter()
                    .map(|&chunk| chunk_mesh(terrain, chunk))
                    .collect::<Vec<_>>()
            });
        }
    })
    .into_iter()
    .flatten()
    .collect()
}

/// A surface over one chunk at height 0 (the entity sits at sea level), with
/// the ground height under each vertex. Cut into the terrain's own triangles,
/// so depth interpolates the way the ground does.
fn chunk_mesh(terrain: &Terrain, chunk: ChunkCoord) -> (Mesh, f32) {
    let rect = chunk.vertex_rect();
    let origin = terrain.chunk_origin(chunk);
    let mut positions = Vec::with_capacity(rect.vertex_count());
    let mut ground = Vec::with_capacity(rect.vertex_count());
    let mut lowest = f32::INFINITY;
    for (x, z) in rect.iter() {
        let p = terrain.vertex_position(x, z);
        lowest = lowest.min(p.y);
        positions.push([p.x - origin.x, 0.0, p.z - origin.z]);
        ground.push([p.y, 0.0]);
    }
    let local = |(x, z): (u32, u32)| (z - rect.min_z) * rect.width() + (x - rect.min_x);
    let mut indices = Vec::with_capacity(((rect.width() - 1) * (rect.depth() - 1) * 6) as usize);
    for cz in rect.min_z..rect.max_z {
        for cx in rect.min_x..rect.max_x {
            for tri in terrain.cell_triangles(cx, cz) {
                indices.extend(tri.map(local));
            }
        }
    }
    (surface_mesh(positions, ground, indices), lowest)
}

/// Four strips around the map, all deep.
fn open_sea_mesh(map: &WorldMap) -> Mesh {
    let extent = map.terrain.extent();
    let m = (extent.max_element() * SEA_MARGIN).min(MAX_SEA_MARGIN);
    let (w, d) = (extent.x, extent.y);
    let strips = [
        (-m, -m, w + m, 0.0),
        (-m, d, w + m, d + m),
        (-m, 0.0, 0.0, d),
        (w, 0.0, w + m, d),
    ];
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for (x0, z0, x1, z1) in strips {
        let base = positions.len() as u32;
        positions.extend([[x0, 0.0, z0], [x1, 0.0, z0], [x0, 0.0, z1], [x1, 0.0, z1]]);
        indices.extend([base, base + 2, base + 3, base, base + 3, base + 1]);
    }
    let ground = vec![[OPEN_SEA_FLOOR, 0.0]; positions.len()];
    surface_mesh(positions, ground, indices)
}

fn surface_mesh(positions: Vec<[f32; 3]>, ground: Vec<[f32; 2]>, indices: Vec<u32>) -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, ground)
    .with_inserted_indices(Indices::U32(indices))
}

fn spawn_surface(
    commands: &mut Commands,
    mesh: Handle<Mesh>,
    material: &WaterMaterialHandle,
    origin: Vec3,
) -> Entity {
    commands
        .spawn((
            WaterSurface,
            Mesh3d(mesh),
            MeshMaterial3d(material.0.clone()),
            Transform::from_translation(origin),
            Visibility::Hidden,
            NotShadowCaster,
            NoWireframe,
        ))
        .id()
}

const fn visible(shown: bool) -> Visibility {
    if shown {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}
