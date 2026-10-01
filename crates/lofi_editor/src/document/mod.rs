mod autosave;
mod close_guard;
mod cook;
mod file_actions;
mod file_dialog;
mod heightmap_io;
mod history;
mod lock;
mod map_document;
mod plugin;
mod recovery;
mod sidecar;
mod startup;
mod status;
mod undo;

pub use file_actions::{
    FileAction, FileActionMessage, NEW_MAP_MAX_CHUNKS, NewMapSettings, PendingDiscard,
};
pub use heightmap_io::{Fit, ImportMessage, PendingImport};
pub use history::{EditHistory, MapFields};
pub use map_document::{MapDocument, PendingChanges, TerrainChange, TerrainChanges};
pub use plugin::DocumentPlugin;
pub use recovery::{PendingRecovery, RecoveryMessage, no_prompt_open};
pub use status::StatusLine;
pub use undo::HistoryMessage;
