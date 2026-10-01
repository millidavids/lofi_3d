//! The material list: pick what the paint brush lays down, and edit the table.

use bevy_egui::egui;
use lofi_world::{MAX_TILE_SIZE, MIN_TILE_SIZE, MaterialId, MaterialTable, Pattern};

use crate::sculpt::BrushSettings;
use lofi_editor::ui::widgets::{slider, srgb_button};

/// Changes that touch the terrain as well as the table.
pub(super) enum MaterialAction {
    /// Repaint `id` as `replacement`, then drop it from the table.
    Delete {
        id: MaterialId,
        replacement: MaterialId,
    },
}

/// Edits `materials` in place; returns anything that also has to repaint terrain.
pub(super) fn materials_section(
    ui: &mut egui::Ui,
    brush: &mut BrushSettings,
    materials: &mut MaterialTable,
) -> Option<MaterialAction> {
    let mut action = None;
    let base = materials.base();
    let others: Vec<(MaterialId, String)> =
        materials.iter().map(|m| (m.id, m.name.clone())).collect();
    let mut new_base = None;

    egui::CollapsingHeader::new("Materials")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Pick what Paint lays down. Each chunk blends up to four.")
                    .small()
                    .weak(),
            );
            let ids: Vec<MaterialId> = materials.iter().map(|m| m.id).collect();
            for id in ids {
                let Some(material) = materials.get_mut(id) else {
                    continue;
                };
                ui.horizontal(|ui| {
                    ui.radio_value(&mut brush.material, id, "");
                    srgb_button(ui, &mut material.color);
                    ui.add(egui::TextEdit::singleline(&mut material.name).desired_width(96.0));
                    if id == base {
                        ui.weak("base")
                            .on_hover_text("Ctrl-paint erases back to this.");
                    }
                    ui.menu_button("...", |ui| {
                        egui::ComboBox::from_label("Pattern")
                            .selected_text(material.pattern.label())
                            .show_ui(ui, |ui| {
                                for pattern in Pattern::ALL {
                                    ui.selectable_value(
                                        &mut material.pattern,
                                        pattern,
                                        pattern.label(),
                                    );
                                }
                            });
                        ui.add(
                            slider(&mut material.tile_size, MIN_TILE_SIZE..=MAX_TILE_SIZE)
                                .logarithmic(true)
                                .suffix(" m")
                                .text("Tile size"),
                        );
                        if id != base && ui.button("Use as base").clicked() {
                            new_base = Some(id);
                        }
                        if id != base {
                            ui.menu_button("Delete, repainting as…", |ui| {
                                for (other, name) in others.iter().filter(|(o, _)| *o != id) {
                                    if ui.button(name).clicked() {
                                        action = Some(MaterialAction::Delete {
                                            id,
                                            replacement: *other,
                                        });
                                    }
                                }
                            });
                        }
                    });
                });
            }
            let add = ui.add_enabled(!materials.is_full(), egui::Button::new("Add material"));
            if add.clicked()
                && let Some(id) = materials.add(
                    &format!("Material {}", materials.len() + 1),
                    [0.5, 0.5, 0.5],
                    Pattern::Plain,
                )
            {
                brush.material = id;
            }
        });

    if let Some(id) = new_base {
        materials.set_base(id);
    }
    // A brush left on a material that's gone paints nothing useful.
    if materials.get(brush.material).is_none() {
        brush.material = materials.base();
    }
    action
}
