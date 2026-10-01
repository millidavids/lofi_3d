//! `world.lofiworld`: what the whole cooked world shares, and its tile index.

use serde::{Deserialize, Serialize};

use bevy_math::Vec3;

use super::format::{CookError, HEIGHT_STEP};
use super::layout::TileLayout;
use crate::grid::GridSize;
use crate::ground::Bounds;
use crate::map::Lighting;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub chunks_x: u32,
    pub chunks_z: u32,
    pub cell_size: f32,
    pub tile_chunks: u32,
    pub height_step: f32,
    pub materials: Vec<CookedMaterial>,
    pub base_material: u16,
    pub water: CookedWater,
    pub lighting: CookedLighting,
    pub tiles: Vec<TileEntry>,
    /// The map's edit revision when it was cooked.
    pub source_revision: u64,
    /// Cooked with edits that weren't saved to the map file yet.
    pub source_unsaved: bool,
}

impl Manifest {
    /// How the world is cut into tiles and where each one is.
    ///
    /// # Errors
    /// If the manifest's sizes aren't ones a cook writes.
    pub fn layout(&self) -> Result<TileLayout, CookError> {
        GridSize::new(self.chunks_x, self.chunks_z)
            .and_then(|size| TileLayout::new(size, self.cell_size, self.tile_chunks))
            .ok_or_else(|| CookError::Corrupt("bad manifest".to_owned()))
    }

    /// The box around a tile, from the manifest alone: enough to cull, to
    /// decide what to stream, or to seed a broad phase before any tile is
    /// read. `None` for a tile the manifest doesn't list.
    #[must_use]
    pub fn tile_bounds(&self, x: u32, z: u32) -> Option<Bounds> {
        let entry = self.tiles.iter().find(|t| t.x == x && t.z == z)?;
        let (low, high) = self.layout().ok()?.tile_rect(x, z)?;
        Some(Bounds {
            min: Vec3::new(low.x, entry.min_step as f32 * HEIGHT_STEP, low.y),
            max: Vec3::new(high.x, entry.max_step as f32 * HEIGHT_STEP, high.y),
        })
    }

    /// The box around the whole world.
    #[must_use]
    pub fn bounds(&self) -> Option<Bounds> {
        self.tiles
            .iter()
            .filter_map(|t| self.tile_bounds(t.x, t.z))
            .reduce(Bounds::union)
    }
}

/// Where a cook came from, for the manifest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CookSource {
    pub revision: u64,
    pub unsaved: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CookedMaterial {
    pub id: u16,
    pub name: String,
    /// sRGB.
    pub color: [f32; 3],
    pub pattern: String,
    pub tile_size: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CookedWater {
    pub enabled: bool,
    pub level: f32,
    pub shallow: [f32; 3],
    pub deep: [f32; 3],
    pub depth: f32,
}

/// The map's light table, as stored by the map.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CookedLighting {
    pub sun_azimuth: f32,
    pub sun_elevation: f32,
    pub sun_color: [f32; 3],
    pub sun_strength: f32,
    pub sky_ambient: [f32; 3],
    pub ground_ambient: [f32; 3],
    pub ambient_strength: f32,
    pub sky_zenith: [f32; 3],
    pub sky_horizon: [f32; 3],
    pub fog_start: f32,
    pub fog_end: f32,
    pub height_fog_density: f32,
    pub height_fog_falloff: f32,
}

impl From<Lighting> for CookedLighting {
    fn from(l: Lighting) -> Self {
        Self {
            sun_azimuth: l.sun_azimuth,
            sun_elevation: l.sun_elevation,
            sun_color: l.sun_color,
            sun_strength: l.sun_strength,
            sky_ambient: l.sky_ambient,
            ground_ambient: l.ground_ambient,
            ambient_strength: l.ambient_strength,
            sky_zenith: l.sky_zenith,
            sky_horizon: l.sky_horizon,
            fog_start: l.fog_start,
            fog_end: l.fog_end,
            height_fog_density: l.height_fog_density,
            height_fog_falloff: l.height_fog_falloff,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileEntry {
    pub x: u32,
    pub z: u32,
    /// Relative to the cooked directory.
    pub file: String,
    pub bytes: u64,
    pub crc32: u32,
    /// The tile's lowest and highest ground, in [`HEIGHT_STEP`]s. Checked
    /// against the tile when it loads.
    pub min_step: i32,
    pub max_step: i32,
}
