//! Left panel: the tool, the asset catalog, and the selected object.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use lofi_editor::document::{EditHistory, MapDocument};
use lofi_editor::ui::UiLayout;
use lofi_world::{BUILTIN_ASSETS, MAX_OBJECT_SCALE, MIN_OBJECT_SCALE, asset};

use crate::tool::{DoodadSettings, DoodadTool, Edit, SelectedObject, apply_edit, edit_objects};

const PANEL_WIDTH: f32 = 270.0;

pub fn doodad_panel(
    mut contexts: EguiContexts,
    mut layout: ResMut<UiLayout>,
    mut settings: ResMut<DoodadSettings>,
    mut selected: ResMut<SelectedObject>,
    mut document: ResMut<MapDocument>,
    mut history: ResMut<EditHistory>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut root = layout.root(ctx, "doodad_panel_root");
    let mut action: Option<(&'static str, Edit)> = None;
    // Edit a copy; write back only on change.
    let current = selected
        .0
        .and_then(|id| document.map().objects.get(id).copied());
    let mut edited = current;
    let foreign = document.map().objects.is_foreign();
    let count = document.map().objects.len();

    egui::Panel::left("doodad_panel")
        .default_size(PANEL_WIDTH)
        .show(&mut root, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Objects");
                if foreign {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "These objects were saved by a newer tool. They're kept as they \
                         are, but can't be edited here.",
                    );
                }
                egui::Grid::new("doodad_tools")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (i, tool) in DoodadTool::ALL.into_iter().enumerate() {
                            ui.selectable_value(&mut settings.tool, tool, tool.label());
                            ui.weak(format!("{}", i + 1));
                            ui.end_row();
                        }
                    });
                ui.label(egui::RichText::new(settings.tool.hint()).small().weak());
                ui.add_space(6.0);

                egui::CollapsingHeader::new("Assets")
                    .default_open(true)
                    .show(ui, |ui| {
                        for a in &BUILTIN_ASSETS {
                            ui.horizontal(|ui| {
                                let [r, g, b] = a.color.map(|c| (c * 255.0).round() as u8);
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(14.0, 14.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    rect,
                                    2.0,
                                    egui::Color32::from_rgb(r, g, b),
                                );
                                ui.selectable_value(&mut settings.asset, a.guid, a.name);
                            });
                        }
                        ui.checkbox(&mut settings.random_yaw, "Random turn");
                        ui.add(
                            egui::Slider::new(&mut settings.scale, 0.25..=4.0)
                                .logarithmic(true)
                                .text("Scale"),
                        );
                    });

                egui::CollapsingHeader::new("Selected")
                    .default_open(true)
                    .show(ui, |ui| match edited.as_mut() {
                        None => {
                            ui.weak("Nothing selected.");
                        }
                        Some(o) => {
                            let catalog = asset(o.asset);
                            ui.label(catalog.name);
                            ui.weak(format!("id {}", o.id.0));
                            ui.weak(format!("asset {}", o.asset)).on_hover_text(
                                "Objects name their asset by this id, never by file, so \
                                 assets can move without breaking maps.",
                            );
                            ui.add_enabled_ui(!foreign, |ui| {
                                egui::Grid::new("selected").num_columns(2).show(ui, |ui| {
                                    for (label, v) in [
                                        ("x", &mut o.position.x),
                                        ("y", &mut o.position.y),
                                        ("z", &mut o.position.z),
                                    ] {
                                        ui.label(label);
                                        ui.add(egui::DragValue::new(v).speed(0.1).suffix(" m"));
                                        ui.end_row();
                                    }
                                    ui.label("Turn");
                                    let mut degrees = o.yaw.to_degrees();
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut degrees)
                                                .speed(1.0)
                                                .suffix("°"),
                                        )
                                        .changed()
                                    {
                                        o.yaw = degrees.to_radians();
                                    }
                                    ui.end_row();
                                    ui.label("Scale");
                                    ui.add(
                                        egui::DragValue::new(&mut o.scale)
                                            .speed(0.01)
                                            .range(MIN_OBJECT_SCALE..=MAX_OBJECT_SCALE),
                                    );
                                    ui.end_row();
                                });
                                ui.horizontal(|ui| {
                                    if ui.button("Drop to ground").on_hover_text("End").clicked() {
                                        action = Some(("Drop to ground", Edit::Drop));
                                    }
                                    if ui.button("Delete").on_hover_text("Delete").clicked() {
                                        action = Some(("Delete object", Edit::Delete));
                                    }
                                });
                            });
                        }
                    });
                ui.separator();
                ui.weak(format!("{count} objects on this map"));
            });
        });
    layout.claim(&root);

    if history.is_recording() || foreign {
        return Ok(());
    }
    if let (Some(before), Some(after)) = (current, edited)
        && before != after
    {
        // Merges while a field is dragged; the shell seals when it's let go.
        edit_objects(&mut document, &mut history, "Edit object", |objects| {
            objects.update(before.id, |o| *o = after).then_some(())
        });
    }
    if let (Some((label, edit)), Some(o)) = (action, current) {
        apply_edit(&mut document, &mut history, o.id, label, edit);
        if matches!(edit, Edit::Delete) {
            selected.0 = None;
        }
    }
    Ok(())
}
