//! Native open/save dialogs, run off the main thread so the editor keeps drawing.
//!
//! The editor stays usable behind a dialog, so the map can change while one
//! is up. An Open that finds new edits goes back through the unsaved-changes
//! prompt instead of dropping them.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, block_on, poll_once};
use lofi_world::{HeightmapFormat, MAP_FILE_EXTENSION};

use super::autosave::Autosave;
use super::cook::CookJob;
use super::file_actions::{FileAction, FileActionMessage, save_document};
use super::heightmap_io::{PendingImport, export_to, read_for_import};
use super::history::EditHistory;
use super::map_document::MapDocument;
use super::status::StatusLine;

enum DialogOutcome {
    Open(PathBuf),
    SaveAs(PathBuf),
    ImportHeightmap(PathBuf),
    ExportHeightmap(PathBuf, HeightmapFormat),
    Cook(PathBuf),
    Cancelled,
}

/// At most one dialog at a time; requests while one is up are ignored.
#[derive(Resource, Default)]
pub struct FileDialog {
    task: Option<Task<DialogOutcome>>,
    /// The map's revision when an Open dialog appeared.
    opened_at: u64,
}

impl FileDialog {
    pub const fn is_open(&self) -> bool {
        self.task.is_some()
    }

    /// `revision` is the map's now; any change by the time a file is picked
    /// means asking about unsaved changes again.
    pub fn open(&mut self, start_dir: Option<PathBuf>, revision: u64) {
        if self.is_open() {
            return;
        }
        self.opened_at = revision;
        let dialog = base_dialog(start_dir).set_title("Open map");
        self.task = Some(IoTaskPool::get().spawn(async move {
            match dialog.pick_file().await {
                Some(file) => DialogOutcome::Open(file.path().to_owned()),
                None => DialogOutcome::Cancelled,
            }
        }));
    }

    pub fn save_as(&mut self, start_dir: Option<PathBuf>, file_name: String) {
        if self.is_open() {
            return;
        }
        let dialog = base_dialog(start_dir)
            .set_title("Save map")
            .set_file_name(file_name);
        self.task = Some(IoTaskPool::get().spawn(async move {
            match dialog.save_file().await {
                Some(file) => DialogOutcome::SaveAs(file.path().to_owned()),
                None => DialogOutcome::Cancelled,
            }
        }));
    }
}

impl FileDialog {
    pub fn pick_heightmap(&mut self, start_dir: Option<PathBuf>) {
        if self.is_open() {
            return;
        }
        let dialog = in_dir(rfd::AsyncFileDialog::new(), start_dir)
            .set_title("Import heightmap")
            .add_filter("heightmap", &["png", "r16", "raw", "r32"]);
        self.task = Some(IoTaskPool::get().spawn(async move {
            match dialog.pick_file().await {
                Some(file) => DialogOutcome::ImportHeightmap(file.path().to_owned()),
                None => DialogOutcome::Cancelled,
            }
        }));
    }

    pub fn pick_cook_folder(&mut self, start_dir: Option<PathBuf>) {
        if self.is_open() {
            return;
        }
        let dialog =
            in_dir(rfd::AsyncFileDialog::new(), start_dir).set_title("Cook for game into…");
        self.task = Some(IoTaskPool::get().spawn(async move {
            match dialog.pick_folder().await {
                Some(folder) => DialogOutcome::Cook(folder.path().to_owned()),
                None => DialogOutcome::Cancelled,
            }
        }));
    }

    pub fn export_heightmap(
        &mut self,
        start_dir: Option<PathBuf>,
        file_name: String,
        format: HeightmapFormat,
    ) {
        if self.is_open() {
            return;
        }
        let dialog = in_dir(rfd::AsyncFileDialog::new(), start_dir)
            .set_title("Export heightmap")
            .add_filter(format.label(), &[format.extension()])
            .set_file_name(file_name);
        self.task = Some(IoTaskPool::get().spawn(async move {
            match dialog.save_file().await {
                Some(file) => DialogOutcome::ExportHeightmap(file.path().to_owned(), format),
                None => DialogOutcome::Cancelled,
            }
        }));
    }
}

fn base_dialog(start_dir: Option<PathBuf>) -> rfd::AsyncFileDialog {
    in_dir(rfd::AsyncFileDialog::new(), start_dir).add_filter("lofi map", &[MAP_FILE_EXTENSION])
}

fn in_dir(dialog: rfd::AsyncFileDialog, start_dir: Option<PathBuf>) -> rfd::AsyncFileDialog {
    match start_dir {
        Some(dir) => dialog.set_directory(dir),
        None => dialog,
    }
}

pub(super) fn poll_file_dialog(
    mut dialog: ResMut<FileDialog>,
    mut document: ResMut<MapDocument>,
    mut history: ResMut<EditHistory>,
    mut autosave: ResMut<Autosave>,
    mut status: ResMut<StatusLine>,
    mut import: ResMut<PendingImport>,
    mut cook: ResMut<CookJob>,
    mut file_actions: MessageWriter<FileActionMessage>,
) {
    let Some(task) = &mut dialog.task else {
        return;
    };
    let Some(outcome) = block_on(poll_once(task)) else {
        return;
    };
    dialog.task = None;
    match outcome {
        DialogOutcome::Open(path) => {
            let action = FileAction::OpenFile(path);
            // Unchanged since the dialog appeared: any unsaved-changes
            // question was already answered before it opened.
            file_actions.write(if document.revision() == dialog.opened_at {
                FileActionMessage::confirmed(action)
            } else {
                FileActionMessage::new(action)
            });
        }
        DialogOutcome::SaveAs(path) => {
            save_document(
                &mut document,
                &mut history,
                &mut autosave,
                path,
                &mut status,
            );
        }
        DialogOutcome::ImportHeightmap(path) => {
            read_for_import(&document, path, &mut import, &mut status);
        }
        DialogOutcome::ExportHeightmap(path, format) => {
            export_to(&document, format, &path, &mut status);
        }
        DialogOutcome::Cook(dir) => {
            // A snapshot: cloning a map copies pointers, not samples.
            let source = lofi_world::cook::CookSource {
                revision: document.revision(),
                unsaved: document.is_dirty(),
            };
            cook.start(document.map().clone(), source, dir, &mut status);
        }
        DialogOutcome::Cancelled => {}
    }
}
