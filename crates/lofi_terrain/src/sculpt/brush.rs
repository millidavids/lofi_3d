//! Tool choice and brush shape.

use bevy::prelude::*;
use lofi_world::{Diagonal, MaskStack, MaterialId, Tint};

pub const MIN_RADIUS: f32 = 1.0;
pub const MAX_RADIUS: f32 = 256.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Raise,
    Smooth,
    Flatten,
    Noise,
    Paint,
    Tint,
    Cells,
    Select,
}

impl Tool {
    /// In shortcut order: key 1 is the first.
    pub const ALL: [Self; 8] = [
        Self::Raise,
        Self::Smooth,
        Self::Flatten,
        Self::Noise,
        Self::Paint,
        Self::Tint,
        Self::Cells,
        Self::Select,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Raise => "Raise / Lower",
            Self::Smooth => "Smooth",
            Self::Flatten => "Flatten",
            Self::Noise => "Noise",
            Self::Paint => "Paint",
            Self::Tint => "Tint",
            Self::Cells => "Cells",
            Self::Select => "Select",
        }
    }

    pub const fn hint(self) -> &'static str {
        match self {
            Self::Raise => "Drag to raise. Hold Ctrl to lower.",
            Self::Smooth => "Drag to soften bumps and hard edges.",
            Self::Flatten => "Drag to level toward the height where the stroke began.",
            Self::Noise => "Drag to roughen. Hold Ctrl to invert.",
            Self::Paint => {
                "Drag to paint the selected material. Hold Ctrl to erase to the base material."
            }
            Self::Tint => "Drag to shade the ground with the tint colour. Hold Ctrl to clear it.",
            Self::Cells => "Drag to set the chosen cell flag. Hold Ctrl to clear it.",
            Self::Select => {
                "Drag to select; Shift adds, Ctrl subtracts. Click or Esc to clear. \
                 Other tools only work inside a selection."
            }
        }
    }

    pub const fn cursor_color(self) -> Color {
        match self {
            Self::Raise => Color::srgb(1.0, 0.85, 0.3),
            Self::Smooth => Color::srgb(0.5, 0.85, 1.0),
            Self::Flatten => Color::srgb(0.95, 0.95, 0.95),
            Self::Noise => Color::srgb(1.0, 0.55, 0.3),
            Self::Paint => Color::srgb(0.6, 1.0, 0.5),
            Self::Tint => Color::srgb(0.95, 0.6, 1.0),
            Self::Cells => Color::srgb(1.0, 0.35, 0.35),
            Self::Select => Color::srgb(0.4, 0.9, 1.0),
        }
    }

    /// Whether the tool changes heights, which reach further when remeshing.
    pub const fn sculpts(self) -> bool {
        matches!(
            self,
            Self::Raise | Self::Smooth | Self::Flatten | Self::Noise
        )
    }
}

/// What the Cells tool sets (or, with Ctrl, clears).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CellMode {
    #[default]
    Hole,
    /// Forces a diagonal; clearing goes back to the map's rule.
    Diagonal(Diagonal),
    NoClutter,
    Unwalkable,
}

impl CellMode {
    pub const ALL: [Self; 5] = [
        Self::Hole,
        Self::Diagonal(Diagonal::Main),
        Self::Diagonal(Diagonal::Anti),
        Self::NoClutter,
        Self::Unwalkable,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Hole => "Hole",
            Self::Diagonal(Diagonal::Main | Diagonal::Auto) => "Diagonal ╲",
            Self::Diagonal(Diagonal::Anti) => "Diagonal ╱",
            Self::NoClutter => "No clutter",
            Self::Unwalkable => "Unwalkable",
        }
    }
}

#[derive(Resource, Clone, Debug)]
pub struct BrushSettings {
    pub tool: Tool,
    /// World metres.
    pub radius: f32,
    /// 0..1, scaled per tool.
    pub strength: f32,
    /// Share of the radius at full strength, 0..1.
    pub hardness: f32,
    /// What the paint tool lays down.
    pub material: MaterialId,
    /// What the tint tool shades toward; 128 is neutral.
    pub tint: Tint,
    pub cell_mode: CellMode,
    /// Limits every tool to ground of a given shape.
    pub masks: MaskStack,
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            tool: Tool::Raise,
            radius: 12.0,
            strength: 0.5,
            hardness: 0.3,
            // The starter set's dirt: the base is already everywhere.
            material: MaterialId(1),
            // A warm, slightly dark wash.
            tint: [140, 120, 100],
            cell_mode: CellMode::Hole,
            masks: MaskStack::default(),
        }
    }
}

/// 1 inside the hard core, easing to 0 at `radius`.
pub fn falloff(distance: f32, radius: f32, hardness: f32) -> f32 {
    if radius <= 0.0 || distance >= radius {
        return 0.0;
    }
    let t = distance / radius;
    let hardness = hardness.clamp(0.0, 0.99);
    if t <= hardness {
        return 1.0;
    }
    let s = (t - hardness) / (1.0 - hardness);
    1.0 - s * s * (3.0 - 2.0 * s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falloff_shape() {
        assert_eq!(falloff(0.0, 10.0, 0.0), 1.0);
        assert_eq!(falloff(10.0, 10.0, 0.0), 0.0);
        assert_eq!(falloff(4.0, 10.0, 0.5), 1.0);
        let mid = falloff(5.0, 10.0, 0.0);
        assert!((mid - 0.5).abs() < 1e-6);
        assert!(falloff(7.0, 10.0, 0.0) < mid);
    }

    #[test]
    fn falloff_handles_degenerate_input() {
        assert_eq!(falloff(0.0, 0.0, 0.5), 0.0);
        assert!(falloff(9.99, 10.0, 1.0).is_finite());
    }
}
