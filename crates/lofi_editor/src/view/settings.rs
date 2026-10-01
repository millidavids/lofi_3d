//! How the map is drawn. Editor preferences, not part of the map file.

use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shading {
    /// Normals interpolate across triangles. WoW Classic terrain was smooth.
    #[default]
    Smooth,
    /// One normal per triangle: the faceted low-poly look.
    Flat,
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct ViewSettings {
    pub shading: Shading,
    pub wireframe: bool,
    /// Off shows the whole map regardless of the map's fog settings.
    pub fog: bool,
    /// Texels drawn as crisp squares up close rather than blurred.
    pub crisp_texels: bool,
    pub overlays: Overlays,
}

/// Editing aids drawn over the terrain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Overlays {
    /// Ground too steep to walk on, in red.
    pub slope: bool,
    pub chunk_grid: bool,
    /// Chunks already blending four materials, outlined.
    pub full_chunks: bool,
    /// Cells flagged unwalkable or no-clutter, outlined near the camera.
    pub cell_flags: bool,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            shading: Shading::Smooth,
            wireframe: false,
            fog: true,
            crisp_texels: true,
            overlays: Overlays::default(),
        }
    }
}
