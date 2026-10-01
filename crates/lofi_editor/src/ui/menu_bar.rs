//! File, Edit and View menus.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use lofi_world::HeightmapFormat;

use super::dialogs::NewMapDialog;
use super::layout::UiLayout;
use crate::camera::FrameMapMessage;
use crate::document::{EditHistory, FileAction, FileActionMessage, HistoryMessage};
use crate::view::{Shading, ViewSettings};

pub(super) fn menu_bar(
    mut contexts: EguiContexts,
    mut layout: ResMut<UiLayout>,
    history: Res<EditHistory>,
    mut view: ResMut<ViewSettings>,
    mut new_map: ResMut<NewMapDialog>,
    mut file_actions: MessageWriter<FileActionMessage>,
    mut history_messages: MessageWriter<HistoryMessage>,
    mut frame: MessageWriter<FrameMapMessage>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut root = layout.root(ctx, "menu_bar_root");
    let mut settings = view.clone();
    let mut file = |action| {
        file_actions.write(FileActionMessage::new(action));
    };

    egui::Panel::top("menu_bar").show(&mut root, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if item(ui, "New…", "Ctrl+N").clicked() {
                    new_map.open();
                }
                if item(ui, "Open…", "Ctrl+O").clicked() {
                    file(FileAction::Open);
                }
                ui.separator();
                if item(ui, "Save", "Ctrl+S").clicked() {
                    file(FileAction::Save);
                }
                if item(ui, "Save As…", "Ctrl+Shift+S").clicked() {
                    file(FileAction::SaveAs);
                }
                ui.separator();
                if ui.button("Import heightmap…").clicked() {
                    file(FileAction::ImportHeightmap);
                }
                ui.menu_button("Export heightmap", |ui| {
                    for format in HeightmapFormat::ALL {
                        if ui.button(format.label()).clicked() {
                            file(FileAction::ExportHeightmap(format));
                        }
                    }
                });
                if ui
                    .button("Cook for game…")
                    .on_hover_text("Writes the tiles the game streams, into a folder.")
                    .clicked()
                {
                    file(FileAction::Cook);
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    file(FileAction::Quit);
                }
            });

            ui.menu_button("Edit", |ui| {
                let undo = history.undo_label();
                let label = undo.map_or_else(|| "Undo".to_owned(), |l| format!("Undo {l}"));
                if ui
                    .add_enabled(
                        undo.is_some(),
                        egui::Button::new(label).shortcut_text("Ctrl+Z"),
                    )
                    .clicked()
                {
                    history_messages.write(HistoryMessage::Undo);
                }
                let redo = history.redo_label();
                let label = redo.map_or_else(|| "Redo".to_owned(), |l| format!("Redo {l}"));
                if ui
                    .add_enabled(
                        redo.is_some(),
                        egui::Button::new(label).shortcut_text("Ctrl+Y"),
                    )
                    .clicked()
                {
                    history_messages.write(HistoryMessage::Redo);
                }
            });

            ui.menu_button("View", |ui| {
                let mut flat = settings.shading == Shading::Flat;
                if ui.checkbox(&mut flat, "Flat shading (F)").changed() {
                    settings.shading = if flat { Shading::Flat } else { Shading::Smooth };
                }
                ui.checkbox(&mut settings.wireframe, "Wireframe (G)");
                ui.checkbox(&mut settings.crisp_texels, "Crisp texels");
                ui.checkbox(&mut settings.fog, "Fog")
                    .on_hover_text("Off shows the whole map. The map's own fog is under Lighting.");
                ui.separator();
                ui.label(egui::RichText::new("Overlays").small().weak());
                let overlays = &mut settings.overlays;
                ui.checkbox(&mut overlays.slope, "Too steep to walk")
                    .on_hover_text("Red where the ground is steeper than 50°.");
                ui.checkbox(&mut overlays.chunk_grid, "Chunk grid");
                ui.checkbox(&mut overlays.full_chunks, "Chunks with four materials")
                    .on_hover_text("These can't take another material.");
                ui.checkbox(&mut overlays.cell_flags, "Cell flags")
                    .on_hover_text("Unwalkable and no-clutter cells near the camera.");
                ui.separator();
                if item(ui, "Frame map", "Home").clicked() {
                    frame.write(FrameMapMessage);
                }
            });
        });
    });

    layout.claim(&root);
    // Only write back real changes, so change detection means something.
    view.set_if_neq(settings);
    Ok(())
}

fn item(ui: &mut egui::Ui, label: &str, shortcut: &str) -> egui::Response {
    ui.add(egui::Button::new(label).shortcut_text(shortcut))
}
