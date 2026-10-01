//! Opens the map named on the command line, or offers back untitled work
//! that was never saved before the editor died.

use bevy::prelude::*;
use lofi_world::load_map;

use super::autosave::Autosave;
use super::file_actions::open_document;
use super::history::EditHistory;
use super::lock::resolved;
use super::map_document::MapDocument;
use super::recovery::PendingRecovery;
use super::sidecar::recovery_for_untitled;
use super::status::StatusLine;
use crate::launch::LaunchOptions;

pub(super) fn open_from_command_line(
    options: Res<LaunchOptions>,
    mut document: ResMut<MapDocument>,
    mut history: ResMut<EditHistory>,
    mut autosave: ResMut<Autosave>,
    mut recovery: ResMut<PendingRecovery>,
    mut status: ResMut<StatusLine>,
    mut exit: MessageWriter<AppExit>,
) {
    // A capture only reads: it takes no lock, so it works on a map that's
    // open in an editor, and it never stops for a recovery prompt.
    if options.screenshot.is_some() || options.measure {
        let Some(path) = options.map.as_deref() else {
            return;
        };
        match load_map(path) {
            Ok(loaded) => document.replace(loaded.map, Some(resolved(path)), loaded.stamp),
            Err(err) => {
                // Not a picture of the default map with a clean exit.
                eprintln!("Could not open {}: {err}", path.display());
                exit.write(AppExit::from_code(1));
            }
        }
        return;
    }
    if let Some(path) = options.map.clone() {
        open_document(
            &mut document,
            &mut history,
            &mut autosave,
            &mut recovery,
            path,
            &mut status,
        );
    }
    // Nothing opened (no argument, or it failed to load): still untitled.
    if document.path().is_none()
        && let Some(found) = recovery_for_untitled(autosave.owned())
    {
        recovery.offer(found, None);
    }
}
