//! The reader must survive anything: every truncation and a sweep of
//! corrupted bytes must return an error or a map, never panic.

use std::panic::catch_unwind;
use std::path::PathBuf;

use bevy_math::Vec3;
use lofi_world::{
    AssetGuid, CellFlags, FileStamp, GridSize, MaterialId, Triangulation, VertexPaint, WorldMap,
    decode_map, encode_map, hash01,
};

fn random_map(chunks_x: u32, chunks_z: u32, seed: u32) -> WorldMap {
    let size = GridSize::new(chunks_x, chunks_z).expect("valid size");
    let mut map = WorldMap::flat(format!("map {seed}"), size, 1.5, 0.0);
    for (x, z) in size.full_rect().iter() {
        let r = |k: u32| hash01(x as i32, z as i32, seed.wrapping_mul(31).wrapping_add(k));
        map.terrain.set_height(x, z, (r(0) - 0.5) * 400.0);
        let pairs: Vec<_> = (0..4)
            .map(|i| (MaterialId(i), (r(u32::from(i) + 1) * 90.0) as u8))
            .collect();
        if let Some(paint) = VertexPaint::new(&pairs) {
            map.terrain
                .set_vertex_paint(x, z, paint)
                .expect("four materials fit");
        }
        map.terrain
            .set_tint(x, z, [5, 6, 7].map(|k| (r(k) * 255.0) as u8));
        if x < size.cells_x() && z < size.cells_z() {
            map.terrain.set_cell(x, z, CellFlags((r(8) * 255.0) as u8));
        }
    }
    if seed % 2 == 1 {
        map.terrain.set_triangulation(Triangulation::Checkerboard);
    }
    for i in 0..seed * 3 {
        let r = |k: u32| hash01(i as i32, k as i32, seed);
        map.objects.add(
            AssetGuid(u128::from(i) * 0x1_0000_0001),
            Vec3::new(r(1) * 50.0, r(2) * 10.0, r(3) * 50.0),
            r(4) * 6.0,
            0.5 + r(5),
        );
    }
    map.water.level = seed as f32;
    map.lighting.sun_azimuth = seed as f32 * 10.0;
    map
}

#[test]
fn random_maps_round_trip() {
    for (seed, (cx, cz)) in [(1, 1), (2, 3), (3, 1), (1, 4)].into_iter().enumerate() {
        let map = random_map(cx, cz, seed as u32);
        let stamp = FileStamp {
            revision: seed as u64 * 10,
            autosave_base: (seed % 2 == 0).then_some(seed as u64),
        };
        let loaded = decode_map(&encode_map(&map, stamp).expect("encodes")).expect("decodes");
        assert_eq!(loaded.map, map);
        assert_eq!(loaded.stamp, stamp);
    }
}

/// Where the v2 body checksum sits, and where the body starts.
const V2_CRC: std::ops::Range<usize> = 16..20;

/// `resign` recomputes the v2 body checksum after each change, so damage
/// reaches the metadata and section decoders instead of stopping at the
/// checksum.
fn assert_never_panics(bytes: &[u8], what: &str, resign: bool) {
    for len in 0..bytes.len() {
        let result = catch_unwind(|| decode_map(&bytes[..len]));
        match result {
            Ok(decoded) => assert!(decoded.is_err(), "{what}: truncated to {len} decoded"),
            Err(_) => panic!("{what}: panicked when truncated to {len} bytes"),
        }
    }
    let mut damaged = bytes.to_vec();
    for i in 0..bytes.len() {
        for mask in [0x01, 0x80, 0xff] {
            damaged[i] ^= mask;
            if resign && i >= V2_CRC.end {
                let crc = crc32fast::hash(&damaged[V2_CRC.end..]);
                damaged[V2_CRC].copy_from_slice(&crc.to_le_bytes());
            }
            assert!(
                catch_unwind(|| decode_map(&damaged)).is_ok(),
                "{what}: panicked with byte {i} ^ {mask:#x}"
            );
            damaged.copy_from_slice(bytes);
        }
    }
}

#[test]
fn current_format_never_panics_on_damage() {
    let bytes = encode_map(&random_map(2, 1, 9), FileStamp::default()).expect("encodes");
    assert_never_panics(&bytes, "v2", false);
    assert_never_panics(&bytes, "v2 re-signed", true);
}

#[test]
fn damage_never_loads_silently() {
    let bytes = encode_map(&random_map(1, 1, 4), FileStamp::default()).expect("encodes");
    let mut damaged = bytes.clone();
    for i in V2_CRC.start..bytes.len() {
        damaged[i] ^= 0x10;
        assert!(decode_map(&damaged).is_err(), "byte {i} damaged but loaded");
        damaged[i] ^= 0x10;
    }
}

#[test]
fn v1_reader_never_panics_on_damage() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/v1.lofimap");
    let bytes = std::fs::read(path).expect("v1 golden");
    assert_never_panics(&bytes, "v1", false);
}
