//! Offering back work an autosave kept when the editor died.

use std::path::{Path, PathBuf};

use bevy::prelude::*;

use super::autosave::Autosave;
use super::history::EditHistory;
use super::map_document::MapDocument;
use super::sidecar::{Found, remove_if_present};
use super::status::StatusLine;

#[derive(Resource, Default)]
pub struct PendingRecovery(Option<Recovery>);

struct Recovery {
    found: Found,
    /// The map the autosave belongs to; `None` when untitled.
    path: Option<PathBuf>,
}

impl PendingRecovery {
    pub fn offer(&mut self, found: Found, path: Option<PathBuf>) {
        self.0 = Some(Recovery { found, path });
    }

    pub fn clear(&mut self) {
        self.0 = None;
    }

    pub const fn is_pending(&self) -> bool {
        self.0.is_some()
    }

    /// The prompt's explanation, while one is waiting.
    pub fn explanation(&self) -> Option<String> {
        let recovery = self.0.as_ref()?;
        let subject = recovery
            .path
            .as_deref()
            .and_then(Path::file_name)
            .map_or_else(
                || format!("the untitled map \"{}\"", recovery.found.loaded.map.name),
                |name| name.to_string_lossy().into_owned(),
            );
        let mut text =
            format!("The editor closed with unsaved changes to {subject}. An autosave kept them.");
        if recovery.found.file_changed {
            text.push_str(
                " The file has changed on disk since; recovering replaces those changes.",
            );
        }
        Some(text)
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryMessage {
    Recover,
    Discard,
}

pub(super) fn handle_recovery(
    mut messages: MessageReader<RecoveryMessage>,
    mut pending: ResMut<PendingRecovery>,
    mut document: ResMut<MapDocument>,
    mut history: ResMut<EditHistory>,
    mut autosave: ResMut<Autosave>,
    mut status: ResMut<StatusLine>,
) {
    for message in messages.read() {
        let Some(Recovery { found, path }) = pending.0.take() else {
            continue;
        };
        match message {
            RecoveryMessage::Recover => {
                // Stays unsaved: the stamp's base is an older revision than
                // what it holds.
                document.replace(found.loaded.map, path, found.loaded.stamp);
                history.clear();
                autosave.adopt(found.sidecar, found.lock);
                status.info("Recovered unsaved work. Save to keep it.");
            }
            RecoveryMessage::Discard => {
                remove_if_present(&found.sidecar);
                status.info("Discarded the autosave.");
            }
        }
    }
}

/// Run condition: no prompt is waiting on the user. Keyboard shortcuts
/// under a prompt would act on a map the user hasn't decided about yet.
pub fn no_prompt_open(
    recovery: Res<PendingRecovery>,
    discard: Res<super::file_actions::PendingDiscard>,
    import: Res<super::heightmap_io::PendingImport>,
) -> bool {
    !recovery.is_pending() && discard.0.is_none() && import.0.is_none()
}
