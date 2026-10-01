//! Version 3, the current format: a material table, per-chunk paint, cell
//! flags and vertex tint.
//!
//! Sections:
//! - `heights`: `f32` per vertex, row-major.
//! - `paint`: per chunk in row-major chunk order, four `u16` slot material
//!   ids (`0xFFFF` empty) then four weight bytes for each of the chunk's
//!   33×33 vertices.
//! - `cells`: one flag byte per cell, row-major over the whole map.
//! - `tint`: per vertex, row-major, as three planes: every red byte, then
//!   every green, then every blue.
//!
//! - `objects` (optional, so v3 readers that predate it keep it untouched):
//!   `format: u32` (1), `next_id: u64`, `count: u32`, then per object
//!   `id: u64`, `asset: u128`, position `3 × f32`, `yaw: f32`,
//!   `scale: f32`, all little-endian. Written once any object has existed.
//!   A newer `format`, or a codec or size this build doesn't know, is kept
//!   as unknown data and the objects left alone.
//!   New per-object data goes in a new section keyed by object id, not in
//!   these records.
//!
//! `cells` and `tint` came late in v3's development; files without them load
//! with clear cells and neutral tint. Meta's `triangulation` is a
//! [`Triangulation`] code, 0 (checkerboard) when absent.

use serde::{Deserialize, Serialize};

use super::error::MapFileError;
use super::format::{FileStamp, LoadedMap};
use super::records::{
    LightingRecord, MAX_REVISION, MaterialRecord, WaterRecord, grid_size, lighting_from,
    material_records, material_table, water_from,
};
use super::sections::{
    SectionRecord, SectionSize, UnknownSection, Unpacked, corrupt, f32s, packed, packed_bytes,
    packed_optional,
};
use crate::cells::{CellFlags, Triangulation};
use crate::chunk_data::Tint;
use crate::grid::GridSize;
use crate::map::WorldMap;
use crate::materials::{MaterialId, MaterialTable};
use crate::objects::{AssetGuid, MAX_OBJECTS, ObjectId, ObjectLayer, PlacedObject};
use crate::paint::{CHUNK_PAINT_VERTICES, ChunkPaint, SLOTS};
use crate::terrain::{Terrain, TerrainParts};

const HEIGHTS: &str = "heights";
const PAINT: &str = "paint";
const CELLS: &str = "cells";
const TINT: &str = "tint";
const OBJECTS: &str = "objects";
const OBJECTS_FORMAT: u32 = 1;
const OBJECTS_HEADER: usize = 16;
const OBJECT_RECORD: usize = 44;
const EMPTY_SLOT: u16 = u16::MAX;
const PAINT_CHUNK_BYTES: usize = SLOTS * 2 + CHUNK_PAINT_VERTICES * SLOTS;

#[derive(Serialize, Deserialize)]
pub(super) struct Body {
    pub meta: Meta,
    pub sections: Vec<SectionRecord>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Meta {
    name: String,
    chunks_x: u32,
    chunks_z: u32,
    cell_size: f32,
    materials: Vec<MaterialRecord>,
    next_material_id: u16,
    base_material: u16,
    #[serde(default)]
    water: WaterRecord,
    #[serde(default)]
    lighting: LightingRecord,
    #[serde(default)]
    triangulation: u8,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub autosave_base: Option<u64>,
}

pub(super) fn encode(map: &WorldMap, stamp: FileStamp) -> Result<Vec<u8>, MapFileError> {
    let terrain = &map.terrain;
    let heights: Vec<u8> = terrain
        .heights_row_major()
        .iter()
        .flat_map(|h| h.to_le_bytes())
        .collect();
    let cells: Vec<u8> = terrain.cells_row_major().iter().map(|c| c.0).collect();
    let tints = terrain.tints_row_major();
    let tint_planes: Vec<u8> = (0..3)
        .flat_map(|channel| tints.iter().map(move |t| t[channel]))
        .collect();
    let mut sections = vec![
        packed(HEIGHTS, &heights),
        packed(PAINT, &paint_bytes(terrain)),
        packed_bytes(CELLS, &cells),
        packed_bytes(TINT, &tint_planes),
    ];
    // Once any object has existed, keep `next_id` even with none left, so
    // ids are never reused. Foreign objects travel in `unknown_sections`.
    if !map.objects.is_foreign() && map.objects.next_id() > ObjectLayer::default().next_id() {
        sections.push(packed_optional(OBJECTS, &object_bytes(&map.objects)));
    }
    sections.extend(map.unknown_sections.iter().map(|s| s.to_record()));

    let body = Body {
        meta: Meta {
            name: map.name.clone(),
            chunks_x: terrain.size().chunks_x(),
            chunks_z: terrain.size().chunks_z(),
            cell_size: terrain.cell_size(),
            materials: material_records(&map.materials),
            next_material_id: map.materials.next_id(),
            base_material: map.materials.base().0,
            water: map.water.into(),
            lighting: map.lighting.into(),
            // A rule added later must raise the file's minimum reader, but
            // only in files that use it.
            triangulation: terrain.triangulation().code(),
            revision: stamp.revision,
            autosave_base: stamp.autosave_base,
        },
        sections,
    };
    Ok(rmp_serde::to_vec_named(&body)?)
}

pub(super) fn decode(body: &[u8], version: u32) -> Result<LoadedMap, MapFileError> {
    let Body { meta, sections } = rmp_serde::from_slice(body)?;
    let size = grid_size(meta.chunks_x, meta.chunks_z)?;
    if meta.revision > MAX_REVISION || meta.autosave_base.is_some_and(|b| b > MAX_REVISION) {
        return Err(corrupt(format!(
            "revision {} is out of range",
            meta.revision
        )));
    }
    let triangulation = Triangulation::from_code(meta.triangulation)
        .ok_or_else(|| corrupt(format!("unknown triangulation rule {}", meta.triangulation)))?;
    let materials = material_table(meta.materials, meta.next_material_id, meta.base_material)?;
    let cell_count = size.cells_x() as usize * size.cells_z() as usize;
    let mut unpacked = Unpacked::read_sized(
        sections,
        &[
            (HEIGHTS, SectionSize::Exact(size.vertex_count() * 4)),
            (
                PAINT,
                SectionSize::Exact(size.chunk_count() * PAINT_CHUNK_BYTES),
            ),
            (CELLS, SectionSize::Exact(cell_count)),
            (TINT, SectionSize::Exact(size.vertex_count() * 3)),
            // Any layout, newer ones too, but never more than a full layer
            // of today's records: the format is read before the layout.
            (
                OBJECTS,
                SectionSize::Between(4, OBJECTS_HEADER + MAX_OBJECTS * OBJECT_RECORD),
            ),
        ],
    )?;
    let objects = match unpacked.take_optional(OBJECTS) {
        Some(bytes) if objects_format(&bytes) > OBJECTS_FORMAT => {
            unpacked
                .unknown
                .push(UnknownSection::from_raw(OBJECTS, &bytes));
            ObjectLayer::foreign()
        }
        Some(bytes) => objects_from(&bytes)?,
        // Set aside unread: packed in a way this build doesn't know.
        None if unpacked.unknown.iter().any(|s| s.tag() == OBJECTS) => ObjectLayer::foreign(),
        None => ObjectLayer::default(),
    };
    let heights = f32s(&unpacked.take(HEIGHTS)?);
    let paints = paints_from(&unpacked.take(PAINT)?, &materials)?;
    let cells: Option<Vec<CellFlags>> = unpacked
        .take_optional(CELLS)
        .map(|bytes| bytes.into_iter().map(CellFlags).collect());
    let tints = unpacked
        .take_optional(TINT)
        .map(|planes| tints_from(&planes));
    let terrain = Terrain::from_parts(
        size,
        meta.cell_size,
        TerrainParts {
            heights: &heights,
            paints,
            cells: cells.as_deref(),
            tints: tints.as_deref(),
            triangulation,
        },
    )
    .map_err(MapFileError::Invalid)?;

    Ok(LoadedMap {
        map: WorldMap {
            name: meta.name,
            terrain,
            materials,
            water: water_from(meta.water),
            lighting: lighting_from(meta.lighting),
            objects,
            unknown_sections: unpacked.unknown,
        },
        stamp: FileStamp {
            revision: meta.revision,
            autosave_base: meta.autosave_base,
        },
        version,
    })
}

fn paint_bytes(terrain: &Terrain) -> Vec<u8> {
    let size: GridSize = terrain.size();
    let mut bytes = Vec::with_capacity(size.chunk_count() * PAINT_CHUNK_BYTES);
    for chunk in size.chunks() {
        let paint = terrain.chunk_paint(chunk);
        for slot in paint.slots() {
            bytes.extend_from_slice(&slot.map_or(EMPTY_SLOT, |m| m.0).to_le_bytes());
        }
        bytes.extend_from_slice(paint.weights().as_flattened());
    }
    bytes
}

fn object_bytes(layer: &ObjectLayer) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(OBJECTS_HEADER + layer.len() * OBJECT_RECORD);
    bytes.extend_from_slice(&OBJECTS_FORMAT.to_le_bytes());
    bytes.extend_from_slice(&layer.next_id().to_le_bytes());
    bytes.extend_from_slice(&(layer.len() as u32).to_le_bytes());
    for o in layer.iter() {
        bytes.extend_from_slice(&o.id.0.to_le_bytes());
        bytes.extend_from_slice(&o.asset.0.to_le_bytes());
        for v in [o.position.x, o.position.y, o.position.z, o.yaw, o.scale] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    bytes
}

/// The section's layout version; the length was checked before unpacking.
fn objects_format(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().unwrap_or_default())
}

/// Today's layout; newer ones were set aside by the caller.
fn objects_from(bytes: &[u8]) -> Result<ObjectLayer, MapFileError> {
    if bytes.len() < OBJECTS_HEADER {
        return Err(corrupt(format!("objects is {} bytes", bytes.len())));
    }
    let (header, records) = bytes.split_at(OBJECTS_HEADER);
    let format = objects_format(header);
    if format == 0 {
        return Err(corrupt("objects format 0".to_owned()));
    }
    let next_id = u64::from_le_bytes(header[4..12].try_into().unwrap_or_default());
    let count = u32::from_le_bytes(header[12..].try_into().unwrap_or_default()) as usize;
    if count * OBJECT_RECORD != records.len() {
        return Err(corrupt(format!(
            "objects says {count}, holds {}",
            records.len() / OBJECT_RECORD
        )));
    }
    let f32_at =
        |r: &[u8], i: usize| f32::from_le_bytes(r[i..i + 4].try_into().unwrap_or_default());
    let objects = records
        .chunks(OBJECT_RECORD)
        .map(|r| PlacedObject {
            id: ObjectId(u64::from_le_bytes(r[..8].try_into().unwrap_or_default())),
            asset: AssetGuid(u128::from_le_bytes(r[8..24].try_into().unwrap_or_default())),
            position: bevy_math::Vec3::new(f32_at(r, 24), f32_at(r, 28), f32_at(r, 32)),
            yaw: f32_at(r, 36),
            scale: f32_at(r, 40),
        })
        .collect();
    ObjectLayer::from_parts(next_id, objects).map_err(corrupt)
}

/// Interleaves the three planes back into per-vertex tints. The length was
/// checked against the grid before unpacking.
fn tints_from(planes: &[u8]) -> Vec<Tint> {
    let n = planes.len() / 3;
    (0..n)
        .map(|i| [planes[i], planes[n + i], planes[2 * n + i]])
        .collect()
}

fn paints_from(bytes: &[u8], materials: &MaterialTable) -> Result<Vec<ChunkPaint>, MapFileError> {
    bytes
        .as_chunks::<PAINT_CHUNK_BYTES>()
        .0
        .iter()
        .map(|chunk| {
            let (slot_bytes, weights) = chunk.split_at(SLOTS * 2);
            let mut slots = [None; SLOTS];
            for (slot, b) in slots.iter_mut().zip(slot_bytes.as_chunks::<2>().0) {
                let id = u16::from_le_bytes(*b);
                if id != EMPTY_SLOT {
                    if materials.get(MaterialId(id)).is_none() {
                        return Err(corrupt(format!("paint uses unknown material {id}")));
                    }
                    *slot = Some(MaterialId(id));
                }
            }
            let weights = weights.as_chunks::<SLOTS>().0.to_vec();
            ChunkPaint::from_parts(slots, weights).map_err(corrupt)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::GridSize;
    use crate::paint::VertexPaint;

    fn map() -> WorldMap {
        let mut map = WorldMap::flat("t", GridSize::new(2, 1).expect("valid size"), 2.0, 1.0);
        map.terrain.set_height(3, 4, 9.0);
        map.terrain
            .set_vertex_paint(32, 5, VertexPaint::solid(MaterialId(2)))
            .expect("room");
        map.water.level = -3.0;
        map.lighting.sun_elevation = 25.0;
        map.terrain
            .set_cell(40, 7, CellFlags::default().with_hole(true));
        // Reserved bits from some newer tool.
        map.terrain.set_cell(0, 31, CellFlags(0b1010_0100));
        map.terrain.set_tint(32, 32, [90, 128, 200]);
        map.terrain.set_tint(64, 0, [1, 2, 3]);
        map
    }

    fn body(map: &WorldMap) -> Body {
        rmp_serde::from_slice(&encode(map, FileStamp::default()).expect("encodes"))
            .expect("decodes")
    }

    fn reencode(body: &Body) -> Vec<u8> {
        rmp_serde::to_vec_named(body).expect("encodes")
    }

    fn extra(critical: bool) -> SectionRecord {
        SectionRecord {
            tag: "doodads".to_owned(),
            critical,
            codec: "lz-from-the-future".to_owned(),
            raw_len: 3,
            crc32: 7,
            data: vec![1, 2, 3],
        }
    }

    #[test]
    fn round_trips_paint_and_settings() {
        let original = map();
        let stamp = FileStamp {
            revision: 41,
            autosave_base: Some(40),
        };
        let loaded = decode(&encode(&original, stamp).expect("encodes"), 3).expect("decodes");
        assert_eq!(loaded.map, original);
        assert_eq!(loaded.stamp, stamp);
        assert_eq!(
            loaded.map.terrain.triangulation(),
            Triangulation::FollowTerrain
        );

        let mut checker = map();
        checker
            .terrain
            .set_triangulation(Triangulation::Checkerboard);
        let loaded = decode(&encode(&checker, stamp).expect("encodes"), 3).expect("decodes");
        assert_eq!(loaded.map, checker);
    }

    #[test]
    fn objects_round_trip_and_ids_stay_spent() {
        let mut m = map();
        let tree = AssetGuid(0x1234_5678_9abc_def0_1234_5678_9abc_def0);
        let a = m
            .objects
            .add(tree, bevy_math::Vec3::new(1.0, 2.0, 3.0), 0.5, 1.5)
            .expect("room");
        let b = m
            .objects
            .add(AssetGuid(7), bevy_math::Vec3::ONE, 0.0, 1.0)
            .expect("room");
        let decoded = |m: &WorldMap| {
            decode(&encode(m, FileStamp::default()).expect("encodes"), 3)
                .expect("decodes")
                .map
        };
        assert_eq!(decoded(&m), m);

        // All gone: the section stays, so the next id isn't handed out twice.
        m.objects.remove(a);
        m.objects.remove(b);
        let mut back = decoded(&m);
        assert!(back.objects.is_empty());
        let c = back
            .objects
            .add(tree, bevy_math::Vec3::ZERO, 0.0, 1.0)
            .expect("room");
        assert!(c > b);
    }

    #[test]
    fn objects_from_a_newer_format_are_kept_untouched() {
        let mut m = map();
        m.objects.add(AssetGuid(1), bevy_math::Vec3::ZERO, 0.0, 1.0);
        let mut b = body(&m);
        // Pretend a later tool wrote format 2, with a layout we can't parse.
        let future = {
            let mut bytes = 2u32.to_le_bytes().to_vec();
            bytes.extend_from_slice(&[9; 23]);
            bytes
        };
        let i = b
            .sections
            .iter()
            .position(|s| s.tag == OBJECTS)
            .expect("present");
        b.sections[i] = packed_optional(OBJECTS, &future);
        let loaded = decode(&reencode(&b), 3).expect("loads anyway").map;
        assert!(loaded.objects.is_foreign());
        assert!(loaded.objects.is_empty());

        // Saving writes the newer section back, once, with the same bytes.
        let resaved = body(&loaded);
        let kept: Vec<_> = resaved
            .sections
            .iter()
            .filter(|s| s.tag == OBJECTS)
            .collect();
        assert_eq!(kept.len(), 1);
        let again = decode(&reencode(&resaved), 3).expect("loads").map;
        assert_eq!(again.unknown_sections, loaded.unknown_sections);
    }

    #[test]
    fn objects_packed_in_a_way_we_cannot_read_are_kept_too() {
        let mut m = map();
        m.objects.add(AssetGuid(1), bevy_math::Vec3::ZERO, 0.0, 1.0);
        let at = |b: &Body| {
            b.sections
                .iter()
                .position(|s| s.tag == OBJECTS)
                .expect("present")
        };
        let mut newer_codec = body(&m);
        let i = at(&newer_codec);
        newer_codec.sections[i].codec = "lz-from-the-future".to_owned();
        let mut too_big = body(&m);
        let i = at(&too_big);
        too_big.sections[i].raw_len = u64::MAX;

        for b in [newer_codec, too_big] {
            let stored = b.sections[at(&b)].data.clone();
            let loaded = decode(&reencode(&b), 3).expect("loads anyway").map;
            assert!(loaded.objects.is_foreign());
            let resaved = body(&loaded);
            let kept: Vec<_> = resaved
                .sections
                .iter()
                .filter(|s| s.tag == OBJECTS)
                .collect();
            assert_eq!(kept.len(), 1);
            assert_eq!(kept[0].data, stored, "byte for byte");
        }

        // Twice is still corrupt, readable or not.
        let mut twice = body(&m);
        let mut copy = packed_optional(OBJECTS, &object_bytes(&m.objects));
        copy.codec = "lz-from-the-future".to_owned();
        twice.sections.push(copy);
        assert!(matches!(
            decode(&reencode(&twice), 3),
            Err(MapFileError::Corrupt(_))
        ));
    }

    #[test]
    fn maps_without_objects_write_no_section() {
        let b = body(&map());
        assert!(b.sections.iter().all(|s| s.tag != OBJECTS));
    }

    #[test]
    fn the_objects_section_is_optional_and_checked() {
        let mut m = map();
        m.objects.add(AssetGuid(1), bevy_math::Vec3::ZERO, 0.0, 1.0);
        let b = body(&m);
        let section = b
            .sections
            .iter()
            .find(|s| s.tag == OBJECTS)
            .expect("present");
        assert!(
            !section.critical,
            "older v3 readers must keep it, not refuse"
        );

        // A count that disagrees with the records.
        let mut bytes = object_bytes(&m.objects);
        bytes[12] = 2;
        assert!(objects_from(&bytes).is_err());
        // Records cut short.
        assert!(objects_from(&object_bytes(&m.objects)[..30]).is_err());
        // A scale nobody could have saved.
        let mut bytes = object_bytes(&m.objects);
        let at = OBJECTS_HEADER + 40;
        bytes[at..at + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(objects_from(&bytes).is_err());
    }

    #[test]
    fn files_from_before_cells_and_tint_load_with_defaults() {
        let mut b = body(&map());
        b.sections.retain(|s| s.tag != CELLS && s.tag != TINT);
        b.meta.triangulation = 0;
        let loaded = decode(&reencode(&b), 3).expect("decodes").map;
        let t = &loaded.terrain;
        assert_eq!(t.triangulation(), Triangulation::Checkerboard);
        assert!(
            t.cells_row_major()
                .iter()
                .all(|c| *c == CellFlags::default())
        );
        assert!(
            t.tints_row_major()
                .iter()
                .all(|c| *c == crate::chunk_data::NEUTRAL_TINT)
        );
        assert_eq!(t.height(3, 4), 9.0, "the rest still loads");
    }

    #[test]
    fn unknown_triangulation_is_corrupt() {
        let mut b = body(&map());
        b.meta.triangulation = 9;
        assert!(matches!(
            decode(&reencode(&b), 3),
            Err(MapFileError::Corrupt(_))
        ));
    }

    #[test]
    fn short_cell_and_tint_sections_are_corrupt() {
        for tag in [CELLS, TINT] {
            let mut b = body(&map());
            let s = b
                .sections
                .iter_mut()
                .find(|s| s.tag == tag)
                .expect("present");
            s.raw_len -= 1;
            assert!(
                matches!(decode(&reencode(&b), 3), Err(MapFileError::Corrupt(_))),
                "{tag}"
            );
        }
    }

    #[test]
    fn unknown_optional_sections_survive_a_resave() {
        let mut b = body(&map());
        b.sections.push(extra(false));
        let loaded = decode(&reencode(&b), 3).expect("decodes");
        assert_eq!(loaded.map.unknown_sections[0].tag(), "doodads");
        let resaved = body(&loaded.map);
        let kept = resaved
            .sections
            .iter()
            .find(|s| s.tag == "doodads")
            .expect("kept");
        assert_eq!(
            (kept.codec.as_str(), kept.data.as_slice()),
            ("lz-from-the-future", &[1, 2, 3][..])
        );
    }

    #[test]
    fn unknown_critical_sections_refuse_to_load() {
        let mut b = body(&map());
        b.sections.push(extra(true));
        assert!(matches!(
            decode(&reencode(&b), 3),
            Err(MapFileError::UnknownCritical(tag)) if tag == "doodads"
        ));
    }

    #[test]
    fn damaged_sections_are_corrupt() {
        let mut b = body(&map());
        b.sections[0].raw_len = u64::MAX;
        assert!(matches!(
            decode(&reencode(&b), 3),
            Err(MapFileError::Corrupt(_))
        ));

        let mut b = body(&map());
        b.sections[1].crc32 ^= 1;
        assert!(matches!(
            decode(&reencode(&b), 3),
            Err(MapFileError::Corrupt(_))
        ));

        let mut b = body(&map());
        b.sections.remove(1);
        assert!(matches!(
            decode(&reencode(&b), 3),
            Err(MapFileError::Corrupt(_))
        ));
    }

    #[test]
    fn paint_naming_a_missing_material_is_corrupt() {
        let mut m = map();
        m.terrain
            .set_vertex_paint(1, 1, VertexPaint::solid(MaterialId(4)))
            .expect("room");
        m.materials.remove(MaterialId(4));
        assert!(matches!(
            decode(&encode(&m, FileStamp::default()).expect("encodes"), 3),
            Err(MapFileError::Corrupt(_))
        ));
    }
}
