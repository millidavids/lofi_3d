//! One mesh entity per terrain chunk, rebuilt only where the terrain changed.

use bevy::prelude::*;
use bevy::tasks::{ComputeTaskPool, TaskPool};
use lofi_world::{
    AO_RADIUS_CELLS, ChunkCoord, GridSize, MaterialId, Triangulation, WorldMap, build_chunk_mesh,
    chunk_ao,
};

use super::materials::TerrainMaterial;
use super::terrain_mesh::terrain_mesh;
use crate::document::{MapDocument, PendingChanges, TerrainChange, TerrainChanges};

#[derive(Component)]
pub struct TerrainChunk;

/// Chunk entities by chunk index, for the document generation they were built
/// from, with each chunk's baked AO. AO depends on heights only, so other
/// edits reuse it.
#[derive(Resource, Default)]
pub struct ChunkEntities {
    generation: Option<u64>,
    entities: Vec<Entity>,
    ao: Vec<Vec<u8>>,
}

/// The one terrain material every chunk shares; chunks differ only in mesh data.
#[derive(Resource)]
pub struct TerrainMaterialHandle(pub Handle<TerrainMaterial>);

/// Chunks whose heights or triangles changed this frame; water follows them.
#[derive(Message, Clone, Debug)]
pub enum ChunksRebuiltMessage {
    All,
    Some(Vec<ChunkCoord>),
}

/// One chunk to rebuild.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Job {
    chunk: ChunkCoord,
    /// Heights moved within AO's reach: bake it again.
    fresh_ao: bool,
    /// Heights or triangles changed, so the water over it did too.
    water: bool,
}

/// Respawns every chunk when the document switches to a different map.
pub(super) fn respawn_chunks_for_new_map(
    mut commands: Commands,
    document: Res<MapDocument>,
    material: Res<TerrainMaterialHandle>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut chunks: ResMut<ChunkEntities>,
    mut changes: ResMut<TerrainChanges>,
    mut rebuilt: MessageWriter<ChunksRebuiltMessage>,
) {
    if chunks.generation == Some(document.generation()) {
        return;
    }
    for entity in chunks.entities.drain(..) {
        commands.entity(entity).despawn();
    }

    let map = document.map();
    let all: Vec<Job> = map
        .terrain
        .size()
        .chunks()
        .map(|chunk| Job {
            chunk,
            fresh_ao: true,
            water: true,
        })
        .collect();
    let built = build_meshes(map, &all, &[]);
    chunks.ao.clear();
    for (job, (mesh, ao)) in all.iter().zip(built) {
        let entity = commands
            .spawn((
                TerrainChunk,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material.0.clone()),
                Transform::from_translation(map.terrain.chunk_origin(job.chunk)),
            ))
            .id();
        chunks.entities.push(entity);
        chunks.ao.push(ao.unwrap_or_default());
    }
    chunks.generation = Some(document.generation());
    // Everything was just built from the current state.
    changes.take();
    rebuilt.write(ChunksRebuiltMessage::All);
}

/// Material layers are positions in the table, baked into chunk meshes, and
/// the triangulation rule picks each cell's triangles. Removing or reordering
/// materials, or changing the rule, rebuilds every chunk (not its AO).
/// Adding a material moves no positions, so it rebuilds nothing.
pub(super) fn mark_all_on_layout_change(
    document: Res<MapDocument>,
    mut layout: Local<Option<(Vec<MaterialId>, Triangulation)>>,
    mut changes: ResMut<TerrainChanges>,
) {
    let map = document.map();
    let ids: Vec<MaterialId> = map.materials.iter().map(|m| m.id).collect();
    let rule = map.terrain.triangulation();
    // The first look has nothing to compare with; the respawn builds it.
    if let Some((old_ids, old_rule)) = layout.as_ref() {
        if !ids.starts_with(old_ids) {
            changes.mark_all_surfaces();
        }
        if *old_rule != rule {
            changes.mark_all_shapes();
        }
    }
    if layout.as_ref() != Some(&(ids.clone(), rule)) {
        *layout = Some((ids, rule));
    }
}

pub(super) fn remesh_changed_chunks(
    document: Res<MapDocument>,
    mut chunks: ResMut<ChunkEntities>,
    mut changes: ResMut<TerrainChanges>,
    handles: Query<&Mesh3d, With<TerrainChunk>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut rebuilt: MessageWriter<ChunksRebuiltMessage>,
) {
    let pending = changes.take();
    if pending == PendingChanges::default() {
        return;
    }
    let map = document.map();
    let size = map.terrain.size();
    let jobs = remesh_jobs(size, pending);

    let built = build_meshes(map, &jobs, &chunks.ao);
    for (job, (mesh, ao)) in jobs.iter().zip(built) {
        let index = size.chunk_index(job.chunk);
        if let Some(ao) = ao
            && let Some(slot) = chunks.ao.get_mut(index)
        {
            *slot = ao;
        }
        let Some(&entity) = chunks.entities.get(index) else {
            continue;
        };
        let Ok(handle) = handles.get(entity) else {
            continue;
        };
        if let Some(mut existing) = meshes.get_mut(&handle.0) {
            *existing = mesh;
        }
    }
    let whole_map = |c: Option<TerrainChange>| c == Some(TerrainChange::All);
    if whole_map(pending.heights) || whole_map(pending.shape) {
        rebuilt.write(ChunksRebuiltMessage::All);
    } else {
        let wet: Vec<ChunkCoord> = jobs.iter().filter(|j| j.water).map(|j| j.chunk).collect();
        if !wet.is_empty() {
            rebuilt.write(ChunksRebuiltMessage::Some(wet));
        }
    }
}

/// Chunks to rebuild, each once.
///
/// A height edit moves AO up to its radius away, which also covers the
/// normals (one vertex) and auto diagonals (two) it moves. Shape and surface
/// edits stay in the chunks holding their vertices.
fn remesh_jobs(size: GridSize, pending: PendingChanges) -> Vec<Job> {
    let chunks = |change: Option<TerrainChange>, grow: u32| -> Vec<ChunkCoord> {
        match change {
            None => Vec::new(),
            Some(TerrainChange::All) => size.chunks().collect(),
            Some(TerrainChange::Rect(rect)) => {
                size.chunks_touching(rect.grow(grow, size)).collect()
            }
        }
    };
    let mut jobs: Vec<Option<Job>> = vec![None; size.chunk_count()];
    let mut order = Vec::new();
    let mut add = |chunk: ChunkCoord, fresh_ao: bool, water: bool| {
        let index = size.chunk_index(chunk);
        match &mut jobs[index] {
            Some(job) => {
                job.fresh_ao |= fresh_ao;
                job.water |= water;
            }
            empty => {
                *empty = Some(Job {
                    chunk,
                    fresh_ao,
                    water,
                });
                order.push(index);
            }
        }
    };
    for chunk in chunks(pending.heights, AO_RADIUS_CELLS) {
        add(chunk, true, true);
    }
    for chunk in chunks(pending.shape, 0) {
        add(chunk, false, true);
    }
    for chunk in chunks(pending.surface, 0) {
        add(chunk, false, false);
    }
    order.into_iter().filter_map(|i| jobs[i]).collect()
}

/// Builds `jobs` on the compute pool, in order. Jobs with `fresh_ao` bake it
/// and return it; the others reuse `cached`.
fn build_meshes(map: &WorldMap, jobs: &[Job], cached: &[Vec<u8>]) -> Vec<(Mesh, Option<Vec<u8>>)> {
    let size = map.terrain.size();
    let build = |job: &Job| {
        let ao = job.fresh_ao.then(|| chunk_ao(&map.terrain, job.chunk));
        let baked = ao
            .as_deref()
            .or_else(|| cached.get(size.chunk_index(job.chunk)).map(Vec::as_slice))
            .unwrap_or_default();
        let mesh = terrain_mesh(
            build_chunk_mesh(&map.terrain, job.chunk),
            baked,
            &map.materials,
        );
        (mesh, ao)
    };
    let pool = ComputeTaskPool::get_or_init(TaskPool::default);
    // A few batches per thread keeps every thread busy without a task per chunk.
    let batch = jobs.len().div_ceil(pool.thread_num().max(1) * 4).max(1);
    pool.scope(|scope| {
        for part in jobs.chunks(batch) {
            scope.spawn(async move { part.iter().map(build).collect::<Vec<_>>() });
        }
    })
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(test)]
mod tests {
    use lofi_world::VertexRect;

    use super::*;

    fn size() -> GridSize {
        GridSize::new(4, 4).expect("valid size")
    }

    /// One vertex in the middle of chunk (1, 1), clear of its borders.
    const MIDDLE: VertexRect = VertexRect {
        min_x: 48,
        min_z: 48,
        max_x: 48,
        max_z: 48,
    };

    const fn job(x: u32, z: u32, fresh_ao: bool, water: bool) -> Job {
        Job {
            chunk: ChunkCoord { x, z },
            fresh_ao,
            water,
        }
    }

    #[test]
    fn surface_edits_stay_in_their_chunk_and_keep_ao() {
        let jobs = remesh_jobs(
            size(),
            PendingChanges {
                surface: Some(TerrainChange::Rect(MIDDLE)),
                ..PendingChanges::default()
            },
        );
        assert_eq!(jobs, vec![job(1, 1, false, false)]);
    }

    #[test]
    fn shape_edits_move_water_but_keep_ao() {
        let jobs = remesh_jobs(
            size(),
            PendingChanges {
                shape: Some(TerrainChange::Rect(MIDDLE)),
                ..PendingChanges::default()
            },
        );
        assert_eq!(jobs, vec![job(1, 1, false, true)]);
    }

    #[test]
    fn height_edits_reach_neighbours_within_the_ao_radius() {
        // Eight cells from chunk (1, 1)'s east edge at x = 64.
        let near_edge = VertexRect {
            min_x: 64 - AO_RADIUS_CELLS,
            max_x: 64 - AO_RADIUS_CELLS,
            ..MIDDLE
        };
        let jobs = remesh_jobs(
            size(),
            PendingChanges {
                heights: Some(TerrainChange::Rect(near_edge)),
                surface: Some(TerrainChange::Rect(MIDDLE)),
                ..PendingChanges::default()
            },
        );
        assert_eq!(
            jobs,
            vec![job(1, 1, true, true), job(2, 1, true, true)],
            "the surface edit's chunk is queued once, with fresh AO"
        );
    }

    #[test]
    fn everything_at_once_queues_each_chunk_once() {
        let jobs = remesh_jobs(
            size(),
            PendingChanges {
                heights: Some(TerrainChange::All),
                shape: Some(TerrainChange::All),
                surface: Some(TerrainChange::All),
            },
        );
        assert_eq!(jobs.len(), 16);
        assert!(jobs.iter().all(|j| j.fresh_ao && j.water));
    }
}
