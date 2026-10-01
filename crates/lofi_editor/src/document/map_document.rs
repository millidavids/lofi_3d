//! The map being edited and what changed in it.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use lofi_world::{FileStamp, Terrain, VertexRect, WorldMap};

use super::lock::MapClaim;

#[derive(Resource)]
pub struct MapDocument {
    map: WorldMap,
    path: Option<PathBuf>,
    /// Bumps on every change and is saved with the map, so an autosave can
    /// prove it holds newer work than the file it came from.
    revision: u64,
    /// The revision on disk at `path`.
    saved_revision: u64,
    generation: u64,
    /// This session's lock on `path`, so no other editor opens it too.
    claim: Option<MapClaim>,
}

impl MapDocument {
    pub const fn new(map: WorldMap) -> Self {
        Self {
            map,
            path: None,
            revision: 0,
            saved_revision: 0,
            generation: 0,
            claim: None,
        }
    }

    pub const fn claim(&self) -> Option<&MapClaim> {
        self.claim.as_ref()
    }

    pub const fn claim_mut(&mut self) -> Option<&mut MapClaim> {
        self.claim.as_mut()
    }

    /// Holds `claim` for the current path, letting go of any earlier one.
    pub fn set_claim(&mut self, claim: MapClaim) {
        self.claim = Some(claim);
    }

    pub const fn map(&self) -> &WorldMap {
        &self.map
    }

    pub const fn terrain(&self) -> &Terrain {
        &self.map.terrain
    }

    /// Marks the document unsaved.
    pub const fn map_mut(&mut self) -> &mut WorldMap {
        self.revision += 1;
        &mut self.map
    }

    /// Runs `edit`, marking the document unsaved only if it reports a change.
    /// For brushes, which often touch terrain without changing it.
    pub fn edit_terrain(&mut self, edit: impl FnOnce(&mut Terrain) -> bool) -> bool {
        let changed = edit(&mut self.map.terrain);
        if changed {
            self.revision += 1;
        }
        changed
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub const fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// What a save of the current state records.
    pub const fn stamp(&self) -> FileStamp {
        FileStamp {
            revision: self.revision,
            autosave_base: None,
        }
    }

    /// What an autosave of the current state records.
    pub const fn autosave_stamp(&self) -> FileStamp {
        FileStamp {
            revision: self.revision,
            autosave_base: Some(self.saved_revision),
        }
    }

    /// Bumps when the map is swapped for another, so views rebuild from scratch
    /// instead of patching chunks that may no longer exist.
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Swaps in another map. A recovered autosave stays unsaved: its stamp
    /// names the older revision that's actually on disk.
    pub fn replace(&mut self, map: WorldMap, path: Option<PathBuf>, stamp: FileStamp) {
        // A claim on another file is let go; the caller claims the new one.
        if self.claim.as_ref().map(MapClaim::path) != path.as_deref() {
            self.claim = None;
        }
        self.map = map;
        self.path = path;
        self.revision = stamp.revision;
        self.saved_revision = stamp.autosave_base.unwrap_or(stamp.revision);
        self.generation += 1;
    }

    pub fn mark_saved(&mut self, path: PathBuf) {
        self.path = Some(path);
        self.saved_revision = self.revision;
    }

    /// File name if saved, map name otherwise.
    pub fn title(&self) -> String {
        self.path.as_deref().and_then(Path::file_name).map_or_else(
            || self.map.name.clone(),
            |name| name.to_string_lossy().into_owned(),
        )
    }
}

/// Terrain changed since the view last rebuilt, by how far the change
/// reaches:
/// - heights: normals, auto diagonals and baked AO around them, and water;
/// - shape (holes, diagonals): their chunks' triangles, and water;
/// - surface (paint, tint): their chunks' vertex data only.
#[derive(Resource, Default)]
pub struct TerrainChanges {
    heights: Option<TerrainChange>,
    shape: Option<TerrainChange>,
    surface: Option<TerrainChange>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainChange {
    All,
    Rect(VertexRect),
}

impl TerrainChange {
    fn merge(prev: Option<Self>, next: Self) -> Self {
        match (prev, next) {
            (None, next) => next,
            (Some(Self::Rect(a)), Self::Rect(b)) => Self::Rect(a.union(b)),
            _ => Self::All,
        }
    }
}

/// What [`TerrainChanges::take`] hands the view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PendingChanges {
    pub heights: Option<TerrainChange>,
    pub shape: Option<TerrainChange>,
    pub surface: Option<TerrainChange>,
}

impl TerrainChanges {
    /// Heights changed at these vertices. Also covers anything else there,
    /// e.g. an undo that may have restored paint too.
    pub fn mark_heights(&mut self, rect: VertexRect) {
        self.heights = Some(TerrainChange::merge(
            self.heights,
            TerrainChange::Rect(rect),
        ));
    }

    /// Cell flags changed in the cells between these vertices.
    pub fn mark_shape(&mut self, rect: VertexRect) {
        self.shape = Some(TerrainChange::merge(self.shape, TerrainChange::Rect(rect)));
    }

    /// Paint or tint changed at these vertices.
    pub fn mark_surface(&mut self, rect: VertexRect) {
        self.surface = Some(TerrainChange::merge(
            self.surface,
            TerrainChange::Rect(rect),
        ));
    }

    /// Every chunk's triangles, e.g. after the triangulation rule changed.
    pub const fn mark_all_shapes(&mut self) {
        self.shape = Some(TerrainChange::All);
    }

    /// Every chunk's vertex data, e.g. after the material list changed.
    pub const fn mark_all_surfaces(&mut self) {
        self.surface = Some(TerrainChange::All);
    }

    pub const fn take(&mut self) -> PendingChanges {
        PendingChanges {
            heights: self.heights.take(),
            shape: self.shape.take(),
            surface: self.surface.take(),
        }
    }
}
