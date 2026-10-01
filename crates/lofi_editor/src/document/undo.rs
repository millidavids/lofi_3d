//! Applies undo and redo requests to the document.

use bevy::prelude::*;

use super::history::EditHistory;
use super::map_document::{MapDocument, TerrainChanges};

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryMessage {
    Undo,
    Redo,
}

pub(super) fn apply_history_messages(
    mut messages: MessageReader<HistoryMessage>,
    mut history: ResMut<EditHistory>,
    mut document: ResMut<MapDocument>,
    mut changes: ResMut<TerrainChanges>,
) {
    for message in messages.read() {
        let possible = match message {
            HistoryMessage::Undo => history.undo_label().is_some(),
            HistoryMessage::Redo => history.redo_label().is_some(),
        };
        // `map_mut` marks the map unsaved, so don't touch it for a no-op.
        if !possible {
            continue;
        }
        let map = document.map_mut();
        let reverted = match message {
            HistoryMessage::Undo => history.undo(map),
            HistoryMessage::Redo => history.redo(map),
        };
        // Settings changes reach the view through the document itself; the
        // view rebuilds chunk meshes if the material list moved. Terrain is
        // marked by what changed, so undoing paint doesn't rebake AO.
        let Some(reverted) = reverted else {
            continue;
        };
        if let Some(rect) = reverted.heights {
            changes.mark_heights(rect);
        }
        if let Some(rect) = reverted.cells {
            changes.mark_shape(rect);
        }
        if let Some(rect) = reverted.surface {
            changes.mark_surface(rect);
        }
    }
}
