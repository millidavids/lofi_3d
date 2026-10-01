use bevy::prelude::*;
use lofi_world::WorldMap;

use super::autosave::{Autosave, run_autosave};
use super::close_guard::quit_on_close_request;
use super::cook::{CookJob, poll_cook};
use super::file_actions::{FileActionMessage, NewMapSettings, PendingDiscard, handle_file_actions};
use super::file_dialog::{FileDialog, poll_file_dialog};
use super::heightmap_io::{ImportMessage, PendingImport, handle_import};
use super::history::EditHistory;
use super::map_document::{MapDocument, TerrainChanges};
use super::recovery::{PendingRecovery, RecoveryMessage, handle_recovery};
use super::startup::open_from_command_line;
use super::status::StatusLine;
use super::undo::{HistoryMessage, apply_history_messages};
use crate::schedule::EditorSystems;

pub struct DocumentPlugin;

impl Plugin for DocumentPlugin {
    fn build(&self, app: &mut App) {
        let map: WorldMap = NewMapSettings::default().build();
        app.insert_resource(MapDocument::new(map))
            .init_resource::<TerrainChanges>()
            .init_resource::<EditHistory>()
            .init_resource::<PendingDiscard>()
            .init_resource::<PendingRecovery>()
            .init_resource::<FileDialog>()
            .init_resource::<Autosave>()
            .init_resource::<StatusLine>()
            .init_resource::<PendingImport>()
            .init_resource::<CookJob>()
            .add_message::<ImportMessage>()
            .add_message::<FileActionMessage>()
            .add_message::<HistoryMessage>()
            .add_message::<RecoveryMessage>()
            .add_systems(Startup, open_from_command_line)
            .add_systems(
                Update,
                (
                    quit_on_close_request.in_set(EditorSystems::Input),
                    (
                        apply_history_messages,
                        handle_file_actions,
                        handle_recovery,
                        poll_file_dialog.run_if(|dialog: Res<FileDialog>| dialog.is_open()),
                        handle_import,
                        poll_cook.run_if(|job: Res<CookJob>| job.is_running()),
                        run_autosave,
                    )
                        .in_set(EditorSystems::Edit),
                ),
            );
    }
}
