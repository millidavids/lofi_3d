//! The tool list, brush shape, each tool's own options, and the mask stack.

use bevy_egui::egui;
use lofi_world::{Mask, MaskKind, NEUTRAL_TINT};

use crate::sculpt::{BrushSettings, CellMode, MAX_RADIUS, MIN_RADIUS, Tool};
use crate::select::{SelectMode, SelectSettings, Selection};

/// Requests that need more than the brush settings to carry out.
pub(super) enum ToolAction {
    /// Paint the selected material wherever the masks pass.
    AutoPaint,
    ClearSelection,
}

pub(super) fn tools_section(
    ui: &mut egui::Ui,
    brush: &mut BrushSettings,
    select: &mut SelectSettings,
    selection: &Selection,
) -> Option<ToolAction> {
    let mut action = None;
    ui.heading("Terrain");
    egui::Grid::new("tools").num_columns(2).show(ui, |ui| {
        for (i, tool) in Tool::ALL.into_iter().enumerate() {
            ui.selectable_value(&mut brush.tool, tool, tool.label());
            ui.weak(format!("{}", i + 1));
            ui.end_row();
        }
    });
    ui.label(egui::RichText::new(brush.tool.hint()).small().weak());
    ui.add_space(6.0);

    match brush.tool {
        Tool::Tint => tint_options(ui, brush),
        Tool::Cells => {
            egui::ComboBox::from_label("Sets")
                .selected_text(brush.cell_mode.label())
                .show_ui(ui, |ui| {
                    for mode in CellMode::ALL {
                        ui.selectable_value(&mut brush.cell_mode, mode, mode.label());
                    }
                });
        }
        Tool::Select => {
            ui.horizontal(|ui| {
                for mode in SelectMode::ALL {
                    ui.selectable_value(&mut select.mode, mode, mode.label());
                }
            });
            if ui
                .add_enabled(selection.is_active(), egui::Button::new("Select none"))
                .on_hover_text("Ctrl+D or Esc")
                .clicked()
            {
                action = Some(ToolAction::ClearSelection);
            }
        }
        _ => {}
    }

    if brush.tool != Tool::Select {
        ui.add(
            egui::Slider::new(&mut brush.radius, MIN_RADIUS..=MAX_RADIUS)
                .logarithmic(true)
                .suffix(" m")
                .text("Radius  [ ]"),
        );
        if brush.tool != Tool::Cells {
            ui.add(egui::Slider::new(&mut brush.strength, 0.02..=1.0).text("Strength"));
            ui.add(egui::Slider::new(&mut brush.hardness, 0.0..=0.95).text("Hardness"));
        }
    }

    let limits = limits(brush, selection);
    if !limits.is_empty() {
        ui.label(
            egui::RichText::new(format!("Limited to {limits}."))
                .small()
                .color(ui.visuals().warn_fg_color),
        );
    }
    if masks_section(ui, brush) {
        action = Some(ToolAction::AutoPaint);
    }
    action
}

fn tint_options(ui: &mut egui::Ui, brush: &mut BrushSettings) {
    ui.horizontal(|ui| {
        // Brush state, not map data: a byte-exact round trip doesn't matter.
        ui.color_edit_button_srgb(&mut brush.tint)
            .on_hover_text("Mid grey changes nothing; lighter brightens, darker darkens.");
        ui.label("Tint");
        if ui.small_button("Neutral").clicked() {
            brush.tint = NEUTRAL_TINT;
        }
    });
}

/// What's narrowing the brush, in words, or empty.
fn limits(brush: &BrushSettings, selection: &Selection) -> String {
    let masks = brush.masks.0.iter().filter(|m| m.enabled).count();
    let mut parts = Vec::new();
    if masks > 0 {
        parts.push(format!("{masks} mask{}", if masks == 1 { "" } else { "s" }));
    }
    if selection.is_active() {
        parts.push("the selection".to_owned());
    }
    parts.join(" and ")
}

/// The mask stack. Returns whether auto-paint was asked for.
fn masks_section(ui: &mut egui::Ui, brush: &mut BrushSettings) -> bool {
    let mut auto_paint = false;
    egui::CollapsingHeader::new("Masks")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Every tool only touches ground that passes all of these.")
                    .small()
                    .weak(),
            );
            let mut remove = None;
            for (i, mask) in brush.masks.0.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    if mask_row(ui, mask) {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                brush.masks.0.remove(i);
            }
            ui.menu_button("Add mask", |ui| {
                for kind in MaskKind::DEFAULTS {
                    if ui.button(kind.label()).clicked() {
                        brush.masks.0.push(Mask::new(kind));
                    }
                }
            });
            ui.add_space(4.0);
            let can_paint = brush.tool == Tool::Paint && brush.masks.is_active();
            auto_paint = ui
                .add_enabled(can_paint, egui::Button::new("Paint where masks pass"))
                .on_hover_text(
                    "Paints the selected material over the whole map, or the selection, \
                     as strongly as the masks pass. Pick the Paint tool to use it.",
                )
                .clicked();
        });
    auto_paint
}

/// One mask's controls. Returns whether it should be removed.
fn mask_row(ui: &mut egui::Ui, mask: &mut Mask) -> bool {
    let mut remove = false;
    ui.horizontal(|ui| {
        ui.checkbox(&mut mask.enabled, mask.kind.label());
        ui.checkbox(&mut mask.invert, "invert");
        if ui.small_button("x").on_hover_text("Remove").clicked() {
            remove = true;
        }
    });
    ui.add_enabled_ui(mask.enabled, |ui| {
        ui.horizontal(|ui| {
            let speed = speed_of(mask.kind);
            let suffix = match mask.kind {
                MaskKind::Height { .. } => " m",
                MaskKind::Slope { .. } => "°",
                MaskKind::Curvature { .. } => " /m",
                MaskKind::Noise { .. } => "",
            };
            match &mut mask.kind {
                MaskKind::Height { min, max }
                | MaskKind::Slope { min, max }
                | MaskKind::Curvature { min, max } => {
                    ui.add(egui::DragValue::new(min).speed(speed).suffix(suffix));
                    ui.label("to");
                    ui.add(egui::DragValue::new(max).speed(speed).suffix(suffix));
                }
                MaskKind::Noise { scale, threshold } => {
                    ui.add(
                        egui::DragValue::new(scale)
                            .speed(0.5)
                            .range(1.0..=1000.0)
                            .prefix("size ")
                            .suffix(" m"),
                    );
                    ui.add(
                        egui::DragValue::new(threshold)
                            .speed(0.01)
                            .range(-1.0..=1.0)
                            .prefix("above "),
                    );
                }
            }
        });
        ui.add(
            egui::DragValue::new(&mut mask.fade)
                .speed(speed_of(mask.kind))
                .range(0.0..=f32::MAX)
                .prefix("soft edge "),
        );
    });
    ui.separator();
    remove
}

/// Drag speed, in the kind's units per pixel.
fn speed_of(kind: MaskKind) -> f32 {
    match kind {
        MaskKind::Height { .. } => 0.1,
        MaskKind::Slope { .. } => 0.5,
        MaskKind::Curvature { .. } => 0.005,
        MaskKind::Noise { .. } => 0.01,
    }
}
