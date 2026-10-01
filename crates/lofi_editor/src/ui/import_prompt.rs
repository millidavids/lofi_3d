//! "Import heightmap": the height range and how to fit it, before anything
//! changes. Modal, so the map can't be swapped out from under it.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::document::{Fit, ImportMessage, MapDocument, PendingImport};

pub(super) fn import_prompt(
    mut contexts: EguiContexts,
    document: Res<MapDocument>,
    mut pending: ResMut<PendingImport>,
    mut choice: MessageWriter<ImportMessage>,
) -> Result {
    let Some(draft) = pending.0.as_mut() else {
        return Ok(());
    };
    let ctx = contexts.ctx_mut()?;
    let size = document.terrain().size();
    let (width, height) = (size.verts_x(), size.verts_z());

    egui::Modal::new(egui::Id::new("import_heightmap")).show(ctx, |ui| {
        ui.set_max_width(440.0);
        ui.heading("Import heightmap");
        let name = draft
            .path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        ui.label(format!(
            "{name}: {}×{} samples. The map has {width}×{height} vertices.",
            draft.map.width, draft.map.height
        ));
        ui.add_space(6.0);

        if draft.needs_range() {
            let source = if draft.info.is_some() {
                "from the file"
            } else {
                "the file doesn't say; enter them"
            };
            ui.label(format!("Heights for black and white ({source}):"));
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.min)
                        .speed(0.5)
                        .prefix("lowest ")
                        .suffix(" m"),
                );
                ui.add(
                    egui::DragValue::new(&mut draft.max)
                        .speed(0.5)
                        .prefix("highest ")
                        .suffix(" m"),
                );
            });
        } else {
            ui.label("Heights are in metres already.");
        }
        if (draft.map.width, draft.map.height) != (width, height) {
            ui.label("The sizes differ:");
            ui.radio_value(&mut draft.fit, Fit::Resample, "Stretch to cover the map");
            ui.radio_value(
                &mut draft.fit,
                Fit::CropOrPad,
                "Place at the first corner, cutting or extending the edges",
            );
        }
        ui.checkbox(&mut draft.flip, "Flip vertically")
            .on_hover_text("Some tools put the first row at the far edge.");
        ui.weak("This replaces every height on the map. Undo brings them back.");

        let problem = draft.range_problem();
        if let Some(problem) = problem {
            ui.colored_label(ui.visuals().warn_fg_color, problem);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(problem.is_none(), egui::Button::new("Import"))
                .clicked()
            {
                choice.write(ImportMessage::Apply);
            }
            if ui.button("Cancel").clicked() {
                choice.write(ImportMessage::Cancel);
            }
        });
    });
    Ok(())
}
