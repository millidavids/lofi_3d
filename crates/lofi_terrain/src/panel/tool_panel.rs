//! Left panel: tools, materials, water, lighting and map facts.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use lofi_editor::document::{EditHistory, MapDocument, MapFields, StatusLine, TerrainChanges};
use lofi_editor::ui::UiLayout;

use super::environment::{lighting_section, water_section};
use super::info::{HeightRange, controls_section, map_section};
use super::materials::{MaterialAction, materials_section};
use super::tools::{ToolAction, tools_section};
use crate::sculpt::{BrushSettings, auto_paint};
use crate::select::{SelectSettings, Selection};

const PANEL_WIDTH: f32 = 270.0;

pub fn tool_panel(
    mut contexts: EguiContexts,
    mut layout: ResMut<UiLayout>,
    mut brush: ResMut<BrushSettings>,
    mut select: ResMut<SelectSettings>,
    mut selection: ResMut<Selection>,
    mut document: ResMut<MapDocument>,
    mut history: ResMut<EditHistory>,
    mut changes: ResMut<TerrainChanges>,
    mut status: ResMut<StatusLine>,
    mut heights: Local<HeightRange>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut root = layout.root(ctx, "tool_panel_root");
    // Edit copies and write back only on change: touching the document marks
    // it unsaved.
    let map = document.map();
    let (mut materials, mut water, mut lighting, mut name, mut triangulation) = (
        map.materials.clone(),
        map.water,
        map.lighting,
        map.name.clone(),
        map.terrain.triangulation(),
    );
    let mut action = None;
    let mut tool_action = None;
    // A stroke can't be held while clicking the panel, but stay safe: a
    // second recording would overwrite the first.
    let recording = history.is_recording();
    heights.hold(recording);

    egui::Panel::left("tool_panel")
        .default_size(PANEL_WIDTH)
        .show(&mut root, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                tool_action = tools_section(ui, &mut brush, &mut select, &selection);
                ui.separator();
                action = materials_section(ui, &mut brush, &mut materials);
                water_section(ui, &mut water);
                lighting_section(ui, &mut lighting);
                map_section(ui, &document, &mut heights, &mut name, &mut triangulation);
                controls_section(ui);
            });
        });
    layout.claim(&root);

    match tool_action {
        Some(ToolAction::ClearSelection) => selection.clear(),
        Some(ToolAction::AutoPaint) if !recording => {
            let rect = selection
                .vertex_rect()
                .unwrap_or_else(|| document.terrain().size().full_rect());
            history.begin("Auto-paint");
            history.capture(document.terrain(), rect);
            let mut outcome = Default::default();
            document.edit_terrain(|terrain| {
                outcome = auto_paint(terrain, &brush.masks, &selection, brush.material, rect);
                outcome.changed
            });
            history.commit(document.map());
            changes.mark_surface(rect);
            if outcome.refused > 0 {
                status.error(format!(
                    "Skipped {} vertices in chunks that already blend four materials.",
                    outcome.refused
                ));
            } else if !outcome.changed {
                status.info("Nothing to paint: the masks pass nowhere new.");
            }
        }
        Some(ToolAction::AutoPaint) => {}
        None => {}
    }

    let map = document.map();
    let changed = [
        ("Materials", materials != map.materials),
        ("Water", water != map.water),
        ("Lighting", lighting != map.lighting),
        ("Rename", name != map.name),
        ("Diagonals", triangulation != map.terrain.triangulation()),
    ]
    .into_iter()
    .find_map(|(label, changed)| changed.then_some(label));
    if let Some(label) = changed {
        history.record_map_edit(label, MapFields::of(document.map()));
        let map = document.map_mut();
        map.materials = materials;
        map.water = water;
        map.lighting = lighting;
        map.name = name;
        map.terrain.set_triangulation(triangulation);
    }

    if let Some(MaterialAction::Delete { id, replacement }) = action
        && !recording
    {
        let full = document.terrain().size().full_rect();
        history.begin("Delete material");
        history.capture(document.terrain(), full);
        history.capture_fields(document.map());
        let map = document.map_mut();
        map.terrain.replace_material(id, replacement);
        map.materials.remove(id);
        history.commit(document.map());
        changes.mark_all_surfaces();
        if brush.material == id {
            brush.material = replacement;
        }
    }
    // The shell seals the undo step once the panels go idle.
    Ok(())
}
