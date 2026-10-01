//! Water and lighting: the map's mood.

use bevy_egui::egui;
use lofi_world::{Lighting, Water};

use lofi_editor::ui::widgets::{slider, srgb_button};

pub(super) fn water_section(ui: &mut egui::Ui, water: &mut Water) {
    egui::CollapsingHeader::new("Water")
        .default_open(true)
        .show(ui, |ui| {
            ui.checkbox(&mut water.enabled, "Sea level");
            ui.add_enabled_ui(water.enabled, |ui| {
                egui::Grid::new("water").num_columns(2).show(ui, |ui| {
                    ui.label("Level");
                    ui.add(
                        egui::DragValue::new(&mut water.level)
                            .speed(0.1)
                            .suffix(" m"),
                    );
                    ui.end_row();
                    ui.label("Shallow");
                    srgb_button(ui, &mut water.shallow);
                    ui.end_row();
                    ui.label("Deep");
                    srgb_button(ui, &mut water.deep);
                    ui.end_row();
                    ui.label("Deep at");
                    ui.add(
                        egui::DragValue::new(&mut water.depth)
                            .speed(0.1)
                            .range(0.5..=200.0)
                            .clamp_existing_to_range(false)
                            .suffix(" m"),
                    );
                    ui.end_row();
                });
            });
        });
}

pub(super) fn lighting_section(ui: &mut egui::Ui, light: &mut Lighting) {
    egui::CollapsingHeader::new("Lighting")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("lighting").num_columns(2).show(ui, |ui| {
                ui.label("Sun bearing");
                ui.add(slider(&mut light.sun_azimuth, 0.0..=360.0).suffix("°"));
                ui.end_row();
                ui.label("Sun height");
                ui.add(slider(&mut light.sun_elevation, -10.0..=89.0).suffix("°"));
                ui.end_row();
                ui.label("Sun");
                ui.horizontal(|ui| {
                    srgb_button(ui, &mut light.sun_color);
                    ui.add(slider(&mut light.sun_strength, 0.0..=2.0));
                });
                ui.end_row();
                ui.label("Sky light");
                ui.horizontal(|ui| {
                    srgb_button(ui, &mut light.sky_ambient);
                    ui.add(slider(&mut light.ambient_strength, 0.0..=2.0));
                });
                ui.end_row();
                ui.label("Ground light");
                srgb_button(ui, &mut light.ground_ambient);
                ui.end_row();
                ui.label("Sky top");
                srgb_button(ui, &mut light.sky_zenith);
                ui.end_row();
                ui.label("Horizon / fog");
                srgb_button(ui, &mut light.sky_horizon);
                ui.end_row();
                ui.label("Fog from");
                let fog_end = light.fog_end;
                ui.add(slider(&mut light.fog_start, 0.0..=fog_end).suffix(" m"));
                ui.end_row();
                ui.label("Fog to");
                ui.add(
                    slider(&mut light.fog_end, 50.0..=20_000.0)
                        .logarithmic(true)
                        .suffix(" m"),
                );
                ui.end_row();
                ui.label("Valley fog");
                ui.add(slider(&mut light.height_fog_density, 0.0..=0.1).logarithmic(true))
                    .on_hover_text("Fog pooling in low ground.");
                ui.end_row();
                ui.label("Valley fog falloff");
                ui.add(slider(&mut light.height_fog_falloff, 0.001..=0.5).logarithmic(true));
                ui.end_row();
            });
            if ui.button("Reset lighting").clicked() {
                *light = Lighting::default();
            }
        });
}

#[cfg(test)]
mod tests {
    use lofi_editor::ui::widgets::draw_untouched;

    use super::*;

    #[test]
    fn drawing_leaves_out_of_range_settings_alone() {
        let mut water = Water {
            depth: 500.0,
            ..Water::default()
        };
        let mut light = Lighting {
            sun_azimuth: -90.0,
            fog_end: 40_000.0,
            ..Lighting::default()
        };
        let (water_before, light_before) = (water, light);
        draw_untouched(|ui| {
            water_section(ui, &mut water);
            // Collapsed by default; open it so its widgets draw.
            egui::CollapsingHeader::new("Lighting")
                .default_open(true)
                .show(ui, |_| {});
            lighting_section(ui, &mut light);
        });
        assert_eq!(water, water_before);
        assert_eq!(light, light_before);
    }
}
