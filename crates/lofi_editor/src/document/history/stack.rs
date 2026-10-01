//! Undo and redo.
//!
//! An edit keeps each touched chunk's data from before the edit, and the map
//! settings if it changed them. Chunk data is copy-on-write, so keeping it is
//! a pointer copy, and a chunk is duplicated only when the edit first writes
//! to it. Undo swaps the stored state with the live state, and the same entry
//! then serves as redo.

use std::collections::BTreeMap;

use bevy::prelude::*;
use lofi_world::{ChunkCoord, ChunkData, Terrain, VertexRect, WorldMap};

use super::map_fields::MapFields;

/// Oldest edits drop once history holds more than this.
const BUDGET_BYTES: usize = 256 * 1024 * 1024;

#[derive(Resource, Default)]
pub struct EditHistory {
    pub(super) undo: Vec<Edit>,
    redo: Vec<Edit>,
    pub(super) bytes: usize,
    recording: Option<Recording>,
    /// The last settings edit is still growing (a drag or typing in progress).
    map_edit_open: bool,
}

pub(super) struct Edit {
    label: &'static str,
    pub(super) chunks: Vec<(ChunkCoord, ChunkData)>,
    fields: Option<MapFields>,
    bytes: usize,
}

struct Recording {
    label: &'static str,
    before: BTreeMap<ChunkCoord, ChunkData>,
    fields: Option<MapFields>,
}

/// What an undo or redo changed: for each kind of terrain data, the vertices
/// of the chunks where it changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reverted {
    pub heights: Option<VertexRect>,
    /// Cell flags.
    pub cells: Option<VertexRect>,
    /// Paint or tint.
    pub surface: Option<VertexRect>,
    /// Whether map settings (materials, water, lighting, name) changed.
    pub map: bool,
}

#[cfg(test)]
impl Reverted {
    /// All terrain it touched.
    pub fn terrain(&self) -> Option<VertexRect> {
        [self.heights, self.cells, self.surface]
            .into_iter()
            .flatten()
            .reduce(VertexRect::union)
    }
}

impl EditHistory {
    /// Starts an edit. Undo and redo are refused until [`Self::commit`].
    pub fn begin(&mut self, label: &'static str) {
        self.recording = Some(Recording {
            label,
            before: BTreeMap::new(),
            fields: None,
        });
    }

    pub const fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    /// Call before changing `rect`. Captures every chunk that stores data
    /// for those vertices, paint on shared borders included. No-op when not
    /// recording.
    pub fn capture(&mut self, terrain: &Terrain, rect: VertexRect) {
        let Some(recording) = &mut self.recording else {
            return;
        };
        for chunk in terrain.size().chunks_touching(rect) {
            recording
                .before
                .entry(chunk)
                .or_insert_with(|| terrain.chunk_data(chunk).clone());
        }
    }

    /// Call before changing map settings inside a recorded edit.
    pub fn capture_fields(&mut self, map: &WorldMap) {
        if let Some(recording) = &mut self.recording
            && recording.fields.is_none()
        {
            recording.fields = Some(MapFields::of(map));
        }
    }

    /// Ends the edit. Returns whether it changed anything.
    pub fn commit(&mut self, map: &WorldMap) -> bool {
        let Some(recording) = self.recording.take() else {
            return false;
        };
        let terrain = &map.terrain;
        let mut bytes = 0;
        let chunks: Vec<_> = recording
            .before
            .into_iter()
            .filter(|(chunk, before)| before != terrain.chunk_data(*chunk))
            .inspect(|(chunk, before)| bytes += before.unshared_bytes(terrain.chunk_data(*chunk)))
            .collect();
        let fields = recording.fields.filter(|f| *f != MapFields::of(map));
        if chunks.is_empty() && fields.is_none() {
            return false;
        }
        if let Some(fields) = &fields {
            bytes += fields.unshared_bytes(map);
        }
        self.push(Edit {
            label: recording.label,
            chunks,
            fields,
            bytes,
        });
        true
    }

    /// Records map settings as they were before a change. Changes under one
    /// label merge until [`Self::seal`], so dragging a colour picker is one
    /// undo step.
    pub fn record_map_edit(&mut self, label: &'static str, before: MapFields) {
        let continues_last = self.map_edit_open
            && self.redo.is_empty()
            && self
                .undo
                .last()
                .is_some_and(|e| e.label == label && e.chunks.is_empty());
        if continues_last {
            return;
        }
        // An object edit is about to replace the live list, so the old one
        // counts in full; a settings edit shares it with an earlier step.
        let earlier = self.undo.iter().rev().find_map(|e| e.fields.as_ref());
        let bytes = before.bytes_after(earlier);
        self.push(Edit {
            label,
            chunks: Vec::new(),
            fields: Some(before),
            bytes,
        });
        self.map_edit_open = true;
    }

    /// Ends the settings edit in progress: the next one starts a new undo
    /// step. Call when an interaction ends and on save, so undo can always
    /// step back to exactly what was saved.
    pub const fn seal(&mut self) {
        self.map_edit_open = false;
    }

    pub fn undo(&mut self, map: &mut WorldMap) -> Option<Reverted> {
        if self.is_recording() {
            return None;
        }
        let mut edit = self.undo.pop()?;
        let reverted = edit.swap(map);
        self.redo.push(edit);
        Some(reverted)
    }

    pub fn redo(&mut self, map: &mut WorldMap) -> Option<Reverted> {
        if self.is_recording() {
            return None;
        }
        let mut edit = self.redo.pop()?;
        let reverted = edit.swap(map);
        self.undo.push(edit);
        Some(reverted)
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// What undo would revert, or `None` if it can't run now.
    pub fn undo_label(&self) -> Option<&'static str> {
        self.undo
            .last()
            .filter(|_| !self.is_recording())
            .map(|edit| edit.label)
    }

    /// What redo would reapply, or `None` if it can't run now.
    pub fn redo_label(&self) -> Option<&'static str> {
        self.redo
            .last()
            .filter(|_| !self.is_recording())
            .map(|edit| edit.label)
    }

    fn push(&mut self, edit: Edit) {
        for dropped in self.redo.drain(..) {
            self.bytes -= dropped.bytes;
        }
        self.bytes += edit.bytes;
        self.undo.push(edit);
        while self.bytes > BUDGET_BYTES && self.undo.len() > 1 {
            let oldest = self.undo.remove(0);
            self.bytes -= oldest.bytes;
        }
    }
}

impl Edit {
    /// Swaps stored and live state, so the entry now holds what was live.
    fn swap(&mut self, map: &mut WorldMap) -> Reverted {
        let mut reverted = Reverted {
            map: self.fields.is_some(),
            ..Reverted::default()
        };
        let grow = |rect: &mut Option<VertexRect>, touched: VertexRect| {
            *rect = Some(rect.map_or(touched, |r| r.union(touched)));
        };
        for (chunk, data) in &mut self.chunks {
            let live = map.terrain.replace_chunk_data(*chunk, data.clone());
            let changed = live.differences(data);
            *data = live;
            let touched = chunk.vertex_rect();
            if changed.heights {
                grow(&mut reverted.heights, touched);
            }
            if changed.cells {
                grow(&mut reverted.cells, touched);
            }
            if changed.surface {
                grow(&mut reverted.surface, touched);
            }
        }
        if let Some(fields) = &mut self.fields {
            fields.swap(map);
        }
        reverted
    }
}
