//! Version 1: everything in one MessagePack record. Read-only; frozen.
//! Converted to the current model on load.

use serde::Deserialize;

use super::error::MapFileError;
use super::format::{FileStamp, LoadedMap};
use super::records::{LayerRecord, WaterRecord, grid_size, legacy_materials, water_from};
use super::sections::corrupt;
use crate::layers::{LAYER_COUNT, LayerWeights};
use crate::map::{Lighting, WorldMap};
use crate::objects::ObjectLayer;
use crate::terrain::Terrain;

#[derive(Deserialize)]
struct RecordV1 {
    name: String,
    chunks_x: u32,
    chunks_z: u32,
    cell_size: f32,
    heights: Vec<f32>,
    /// `LAYER_COUNT` bytes per vertex, row-major like `heights`.
    #[serde(with = "serde_bytes")]
    weights: Vec<u8>,
    layers: Vec<LayerRecord>,
    #[serde(default)]
    water: WaterRecord,
}

pub(super) fn decode(body: &[u8]) -> Result<LoadedMap, MapFileError> {
    let record: RecordV1 = rmp_serde::from_slice(body)?;
    let size = grid_size(record.chunks_x, record.chunks_z)?;
    let (weights, rest) = record.weights.as_chunks::<LAYER_COUNT>();
    if !rest.is_empty() {
        return Err(corrupt("truncated layer weights".to_owned()));
    }
    let weights: Vec<LayerWeights> = weights.to_vec();
    let (materials, ids) = legacy_materials(record.layers);
    let terrain = Terrain::from_legacy(size, record.cell_size, &record.heights, &weights, ids)
        .map_err(MapFileError::Invalid)?;

    Ok(LoadedMap {
        map: WorldMap {
            name: record.name,
            terrain,
            materials,
            water: water_from(record.water),
            lighting: Lighting::default(),
            objects: ObjectLayer::default(),
            unknown_sections: Vec::new(),
        },
        stamp: FileStamp::default(),
        version: 1,
    })
}
