//! Record types the format versions share, and their validation.

use serde::{Deserialize, Serialize};

use super::error::MapFileError;
use crate::grid::GridSize;
use crate::map::{Lighting, Water};
use crate::materials::{Material, MaterialId, MaterialTable, Pattern, clamp_tile_size};

/// Real revisions count edits and never get near this. Rejecting the rest
/// means `revision + 1` can't overflow once the map is being edited.
pub(super) const MAX_REVISION: u64 = 1 << 53;

/// A pre-v3 map-wide layer.
#[derive(Deserialize)]
pub(super) struct LayerRecord {
    name: String,
    color: [f32; 3],
}

#[derive(Serialize, Deserialize)]
pub(super) struct WaterRecord {
    enabled: bool,
    level: f32,
    #[serde(default = "default_shallow")]
    shallow: [f32; 3],
    #[serde(default = "default_deep")]
    deep: [f32; 3],
    #[serde(default = "default_depth")]
    depth: f32,
}

fn default_shallow() -> [f32; 3] {
    Water::default().shallow
}

fn default_deep() -> [f32; 3] {
    Water::default().deep
}

fn default_depth() -> f32 {
    Water::default().depth
}

impl Default for WaterRecord {
    fn default() -> Self {
        Water::default().into()
    }
}

impl From<Water> for WaterRecord {
    fn from(w: Water) -> Self {
        Self {
            enabled: w.enabled,
            level: w.level,
            shallow: w.shallow,
            deep: w.deep,
            depth: w.depth,
        }
    }
}

pub(super) fn water_from(r: WaterRecord) -> Water {
    let finite = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
    Water {
        enabled: r.enabled,
        level: finite(r.level, 0.0),
        shallow: color(r.shallow),
        deep: color(r.deep),
        depth: finite(r.depth, 8.0).clamp(0.1, 1_000.0),
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct MaterialRecord {
    id: u16,
    name: String,
    color: [f32; 3],
    /// A [`Pattern`] label; unknown ones fall back to plain.
    pattern: String,
    tile_size: f32,
}

pub(super) fn material_records(table: &MaterialTable) -> Vec<MaterialRecord> {
    table
        .iter()
        .map(|m| MaterialRecord {
            id: m.id.0,
            name: m.name.clone(),
            color: m.color,
            pattern: m.pattern.label().to_owned(),
            tile_size: m.tile_size,
        })
        .collect()
}

pub(super) fn material_table(
    records: Vec<MaterialRecord>,
    next_id: u16,
    base: u16,
) -> Result<MaterialTable, MapFileError> {
    let materials = records
        .into_iter()
        .map(|r| Material {
            id: MaterialId(r.id),
            name: r.name,
            color: r.color,
            pattern: Pattern::from_label(&r.pattern),
            tile_size: r.tile_size,
        })
        .collect();
    MaterialTable::new(materials, next_id, MaterialId(base)).map_err(MapFileError::Invalid)
}

/// Pre-v3 maps had four map-wide layers: they become materials 0 to 3.
pub(super) fn legacy_materials(layers: Vec<LayerRecord>) -> (MaterialTable, [MaterialId; 4]) {
    const PATTERNS: [Pattern; 4] = [Pattern::Grass, Pattern::Dirt, Pattern::Rock, Pattern::Sand];
    let starter = MaterialTable::starter();
    let mut layers = layers.into_iter();
    let materials = (0..4u16)
        .map(|i| {
            let fallback = starter
                .get(MaterialId(i))
                .expect("the starter set has four materials");
            let (name, color) = layers.next().map_or_else(
                || (fallback.name.clone(), fallback.color),
                |l| (l.name, l.color),
            );
            Material {
                id: MaterialId(i),
                name,
                color,
                pattern: PATTERNS[i as usize],
                tile_size: clamp_tile_size(4.0),
            }
        })
        .collect();
    let table = MaterialTable::new(materials, 4, MaterialId(0))
        .expect("four distinct ids below next_id, with the base among them");
    (table, [0, 1, 2, 3].map(MaterialId))
}

#[derive(Serialize, Deserialize)]
pub(super) struct LightingRecord {
    sun_azimuth: f32,
    sun_elevation: f32,
    sun_color: [f32; 3],
    sun_strength: f32,
    sky_ambient: [f32; 3],
    ground_ambient: [f32; 3],
    ambient_strength: f32,
    sky_zenith: [f32; 3],
    sky_horizon: [f32; 3],
    fog_start: f32,
    fog_end: f32,
    height_fog_density: f32,
    height_fog_falloff: f32,
}

impl Default for LightingRecord {
    fn default() -> Self {
        Lighting::default().into()
    }
}

impl From<Lighting> for LightingRecord {
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

pub(super) fn lighting_from(r: LightingRecord) -> Lighting {
    Lighting {
        sun_azimuth: r.sun_azimuth,
        sun_elevation: r.sun_elevation,
        sun_color: r.sun_color,
        sun_strength: r.sun_strength,
        sky_ambient: r.sky_ambient,
        ground_ambient: r.ground_ambient,
        ambient_strength: r.ambient_strength,
        sky_zenith: r.sky_zenith,
        sky_horizon: r.sky_horizon,
        fog_start: r.fog_start,
        fog_end: r.fog_end,
        height_fog_density: r.height_fog_density,
        height_fog_falloff: r.height_fog_falloff,
    }
    .sanitized()
}

pub(super) fn grid_size(chunks_x: u32, chunks_z: u32) -> Result<GridSize, MapFileError> {
    GridSize::new(chunks_x, chunks_z)
        .ok_or_else(|| MapFileError::Invalid(format!("size {chunks_x}x{chunks_z} chunks")))
}

fn color(c: [f32; 3]) -> [f32; 3] {
    c.map(|v| {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        }
    })
}
