//! Map facts and the controls reference.

use bevy_egui::egui;
use lofi_editor::document::MapDocument;
use lofi_world::Triangulation;

/// The map's lowest and highest vertex, rescanned only when the map changes
/// and no stroke is under way: a big map has millions of them, and a stroke
/// changes the map every frame.
#[derive(Default)]
pub struct HeightRange {
    seen: Option<(u64, u64)>,
    range: (f32, f32),
    held: bool,
}

impl HeightRange {
    /// While held, the last range stands; it catches up once released.
    pub(super) const fn hold(&mut self, held: bool) {
        self.held = held;
    }

    fn of(&mut self, document: &MapDocument) -> (f32, f32) {
        let key = (document.generation(), document.revision());
        let stale = self.seen != Some(key);
        let first = self.seen.is_none_or(|(generation, _)| generation != key.0);
        if stale && (!self.held || first) {
            self.range = document.terrain().height_range();
            self.seen = Some(key);
        }
        self.range
    }
}

pub(super) fn map_section(
    ui: &mut egui::Ui,
    document: &MapDocument,
    heights: &mut HeightRange,
    name: &mut String,
    triangulation: &mut Triangulation,
) {
    egui::CollapsingHeader::new("Map")
        .default_open(true)
        .show(ui, |ui| {
            let terrain = document.terrain();
            egui::Grid::new("map_facts").num_columns(2).show(ui, |ui| {
                ui.label("Name");
                ui.text_edit_singleline(name);
                ui.end_row();
                ui.label("Diagonals")
                    .on_hover_text("How cells are cut into triangles, unless set per cell.");
                egui::ComboBox::from_id_salt("triangulation")
                    .selected_text(triangulation.label())
                    .show_ui(ui, |ui| {
                        for rule in [Triangulation::FollowTerrain, Triangulation::Checkerboard] {
                            ui.selectable_value(triangulation, rule, rule.label());
                        }
                    });
                ui.end_row();
                ui.label("Chunks");
                ui.label(format!(
                    "{} × {}",
                    terrain.size().chunks_x(),
                    terrain.size().chunks_z()
                ));
                ui.end_row();
                ui.label("Cell size");
                ui.label(format!("{} m", terrain.cell_size()));
                ui.end_row();
                let (low, high) = heights.of(document);
                ui.label("Heights");
                ui.label(format!("{low:.1} to {high:.1} m"));
                ui.end_row();
            });
        });
}

pub(super) fn controls_section(ui: &mut egui::Ui) {
    egui::CollapsingHeader::new("Controls")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("controls").num_columns(2).show(ui, |ui| {
                for (keys, action) in [
                    ("LMB drag", "Use tool"),
                    ("Ctrl + LMB", "Invert tool"),
                    ("RMB drag", "Look around"),
                    ("W A S D", "Fly"),
                    ("Q / E", "Down / up"),
                    ("Shift", "Fly faster"),
                    ("MMB drag", "Pan"),
                    ("Wheel", "Dolly"),
                    ("Alt + wheel", "Brush size"),
                    ("[  ]", "Brush size"),
                    ("1 – 8", "Pick tool"),
                    ("Shift / Ctrl drag", "Select: add / subtract"),
                    ("Esc, Ctrl + D", "Select none"),
                    ("F / G", "Flat shading / wireframe"),
                    ("Home", "Frame map"),
                    ("Ctrl + Z / Y", "Undo / redo"),
                ] {
                    ui.monospace(keys);
                    ui.label(action);
                    ui.end_row();
                }
            });
        });
}
