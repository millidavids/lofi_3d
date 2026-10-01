//! The New Map window and the "discard unsaved changes?" prompt.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use lofi_world::{CHUNK_CELLS, MAX_CELL_SIZE, MIN_CELL_SIZE};

use crate::document::{
    FileAction, FileActionMessage, MapDocument, NEW_MAP_MAX_CHUNKS, NewMapSettings, PendingDiscard,
};

#[derive(Resource, Default)]
pub struct NewMapDialog {
    open: bool,
    settings: NewMapSettings,
}

impl NewMapDialog {
    pub const fn open(&mut self) {
        self.open = true;
    }
}

pub(super) fn new_map_window(
    mut contexts: EguiContexts,
    mut dialog: ResMut<NewMapDialog>,
    mut file_actions: MessageWriter<FileActionMessage>,
) -> Result {
    if !dialog.open {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let NewMapDialog { open, settings } = &mut *dialog;
    let mut create = false;
    let mut cancel = false;

    egui::Window::new("New map")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            egui::Grid::new("new_map").num_columns(2).show(ui, |ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut settings.name);
                ui.end_row();

                ui.label("Chunks X");
                ui.add(egui::Slider::new(
                    &mut settings.chunks_x,
                    1..=NEW_MAP_MAX_CHUNKS,
                ));
                ui.end_row();
                ui.label("Chunks Z");
                ui.add(egui::Slider::new(
                    &mut settings.chunks_z,
                    1..=NEW_MAP_MAX_CHUNKS,
                ));
                ui.end_row();

                ui.label("Cell size");
                ui.add(
                    egui::Slider::new(&mut settings.cell_size, MIN_CELL_SIZE..=MAX_CELL_SIZE)
                        .logarithmic(true)
                        .suffix(" m"),
                );
                ui.end_row();

                ui.label("Ground height");
                ui.add(
                    egui::DragValue::new(&mut settings.base_height)
                        .speed(0.1)
                        .suffix(" m"),
                );
                ui.end_row();
            });
            let cells = |chunks: u32| chunks * CHUNK_CELLS;
            let metres = |chunks: u32| cells(chunks) as f32 * settings.cell_size;
            ui.weak(format!(
                "{} × {} cells, {:.0} × {:.0} m",
                cells(settings.chunks_x),
                cells(settings.chunks_z),
                metres(settings.chunks_x),
                metres(settings.chunks_z),
            ));
            ui.separator();
            ui.horizontal(|ui| {
                create = ui.button("Create").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });

    if create {
        file_actions.write(FileActionMessage::new(FileAction::New(settings.clone())));
    }
    if create || cancel {
        *open = false;
    }
    Ok(())
}

pub(super) fn confirm_discard(
    mut contexts: EguiContexts,
    document: Res<MapDocument>,
    mut pending: ResMut<PendingDiscard>,
    mut file_actions: MessageWriter<FileActionMessage>,
) -> Result {
    let Some(action) = pending.0.clone() else {
        return Ok(());
    };
    let ctx = contexts.ctx_mut()?;
    let verb = match action {
        FileAction::New(_) => "Start a new map",
        FileAction::Open | FileAction::OpenFile(_) => "Open another map",
        FileAction::Quit => "Quit",
        FileAction::Save
        | FileAction::SaveAs
        | FileAction::ImportHeightmap
        | FileAction::ExportHeightmap(_)
        | FileAction::Cook => "Continue",
    };
    let mut decided = false;

    let modal = egui::Modal::new(egui::Id::new("confirm_discard")).show(ctx, |ui| {
        ui.heading("Unsaved changes");
        ui.label(format!(
            "\"{}\" has changes that aren't saved.",
            document.title()
        ));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(format!("{verb} and discard them")).clicked() {
                file_actions.write(FileActionMessage::confirmed(action.clone()));
                decided = true;
            }
            if ui.button("Cancel").clicked() {
                decided = true;
            }
        });
    });

    if decided || modal.should_close() {
        pending.0 = None;
    }
    Ok(())
}
