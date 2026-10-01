//! Widgets that leave a stored value alone until the user changes it.
//!
//! egui's colour buttons round-trip through HSV every time they draw, and its
//! sliders clamp by default. Either can rewrite a value just by showing it,
//! which would mark a freshly opened map unsaved.

use bevy_egui::egui;

/// An sRGB colour, shown as sRGB, written back only when edited.
pub fn srgb_button(ui: &mut egui::Ui, color: &mut [f32; 3]) -> egui::Response {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut shown = egui::Color32::from_rgb(byte(color[0]), byte(color[1]), byte(color[2]));
    let response = egui::color_picker::color_edit_button_srgba(
        ui,
        &mut shown,
        egui::color_picker::Alpha::Opaque,
    );
    if response.changed() {
        *color = [shown.r(), shown.g(), shown.b()].map(|b| f32::from(b) / 255.0);
    }
    response
}

/// A slider that clamps what the user enters, not what the map holds: files
/// may carry values outside the slider's range.
pub fn slider<'a>(value: &'a mut f32, range: std::ops::RangeInclusive<f32>) -> egui::Slider<'a> {
    egui::Slider::new(value, range).clamping(egui::SliderClamping::Edits)
}

/// Draws a few headless frames with nothing touched, for tests of panels.
pub fn draw_untouched(mut show: impl FnMut(&mut egui::Ui)) {
    let ctx = egui::Context::default();
    for _ in 0..3 {
        // No renderer here to take the font atlas.
        ctx.run_ui(egui::RawInput::default(), &mut show)
            .textures_delta
            .clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_leaves_colours_alone() {
        // Values that don't survive egui's HSV round trip.
        let original = [0.36, 0.49, 0.2];
        let mut color = original;
        draw_untouched(|ui| {
            srgb_button(ui, &mut color);
        });
        assert_eq!(color, original);
    }
}
