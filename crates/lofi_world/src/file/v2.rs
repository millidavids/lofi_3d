//! Version 2: four map-wide layers with weights per vertex, in sections.
//! Read-only; frozen. Converted to the current model on load.

use serde::Deserialize;

use super::error::MapFileError;
use super::format::{FileStamp, LoadedMap};
use super::records::{
    LayerRecord, MAX_REVISION, WaterRecord, grid_size, legacy_materials, water_from,
};
use super::sections::{SectionRecord, Unpacked, corrupt, f32s};
use crate::layers::{LAYER_COUNT, LayerWeights};
use crate::map::{Lighting, WorldMap};
use crate::objects::ObjectLayer;
use crate::terrain::Terrain;

const HEIGHTS: &str = "heights";
const WEIGHTS: &str = "weights";

#[derive(Deserialize)]
struct Body {
    meta: Meta,
    sections: Vec<SectionRecord>,
}

#[derive(Deserialize)]
struct Meta {
    name: String,
    chunks_x: u32,
    chunks_z: u32,
    cell_size: f32,
    layers: Vec<LayerRecord>,
    #[serde(default)]
    water: WaterRecord,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    autosave_base: Option<u64>,
}

pub(super) fn decode(body: &[u8]) -> Result<LoadedMap, MapFileError> {
    let Body { meta, sections } = rmp_serde::from_slice(body)?;
    let size = grid_size(meta.chunks_x, meta.chunks_z)?;
    if meta.revision > MAX_REVISION || meta.autosave_base.is_some_and(|b| b > MAX_REVISION) {
        return Err(corrupt(format!(
            "revision {} is out of range",
            meta.revision
        )));
    }
    let vertex_bytes = size.vertex_count() * 4;
    let mut unpacked = Unpacked::read(
        sections,
        &[(HEIGHTS, vertex_bytes), (WEIGHTS, vertex_bytes)],
    )?;
    let heights = f32s(&unpacked.take(HEIGHTS)?);
    let weights: Vec<LayerWeights> = unpacked
        .take(WEIGHTS)?
        .as_chunks::<LAYER_COUNT>()
        .0
        .to_vec();

    let (materials, ids) = legacy_materials(meta.layers);
    let terrain = Terrain::from_legacy(size, meta.cell_size, &heights, &weights, ids)
        .map_err(MapFileError::Invalid)?;

    Ok(LoadedMap {
        map: WorldMap {
            name: meta.name,
            terrain,
            materials,
            water: water_from(meta.water),
            lighting: Lighting::default(),
            objects: ObjectLayer::default(),
            unknown_sections: unpacked.unknown,
        },
        stamp: FileStamp {
            revision: meta.revision,
            autosave_base: meta.autosave_base,
        },
        version: 2,
    })
}
