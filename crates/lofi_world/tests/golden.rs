//! Every released file version must keep loading. Each version has a golden
//! file in `tests/golden/`, written once by that version and never rewritten.
//!
//! To add the current version's file after a format bump:
//! `cargo test -p lofi_world --test golden -- --ignored bless`

use std::path::PathBuf;

use bevy_math::Vec3;
use lofi_world::{
    AssetGuid, BUILTIN_ASSETS, CellFlags, Diagonal, FileStamp, GridSize, MAP_FORMAT_VERSION,
    Material, MaterialId, MaterialTable, Pattern, Triangulation, VertexPaint, WorldMap, load_map,
    save_map,
};

fn golden_path(version: u32) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("v{version}.lofimap"))
}

fn heights(x: u32, z: u32) -> f32 {
    ((x * 7 + z * 13) % 17) as f32 * 0.75 - 3.0
}

/// Weights for materials 0 to 3 at a vertex; every one is used somewhere.
fn weights(x: u32, z: u32) -> [u8; 4] {
    let a = ((x * 37 + z * 11) % 150) as u8;
    let b = ((x * 3 + z * 29) % 60) as u8;
    let c = ((x * 17 + z * 5) % 40) as u8;
    [a, b, c, 255 - a - b - c]
}

/// What the v1 and v2 golden files hold, in today's model: four map-wide
/// layers become materials 0 to 3, the fourth renamed "Snow", and cells keep
/// the checkerboard they always had.
fn legacy_golden() -> WorldMap {
    let size = GridSize::new(1, 1).expect("valid size");
    let mut map = WorldMap::flat("Golden", size, 2.5, 0.0);
    map.terrain.set_triangulation(Triangulation::Checkerboard);
    let material = |id, name: &str, color, pattern| Material {
        id: MaterialId(id),
        name: name.to_owned(),
        color,
        pattern,
        tile_size: 4.0,
    };
    map.materials = MaterialTable::new(
        vec![
            material(0, "Grass", [0.36, 0.49, 0.20], Pattern::Grass),
            material(1, "Dirt", [0.48, 0.37, 0.24], Pattern::Dirt),
            material(2, "Rock", [0.50, 0.48, 0.45], Pattern::Rock),
            material(3, "Snow", [0.9, 0.92, 0.95], Pattern::Sand),
        ],
        4,
        MaterialId(0),
    )
    .expect("valid table");
    for (x, z) in size.full_rect().iter() {
        map.terrain.set_height(x, z, heights(x, z));
        let pairs: Vec<_> = (0..4)
            .map(|i| (MaterialId(i as u16), weights(x, z)[i]))
            .collect();
        let paint = VertexPaint::new(&pairs).expect("four materials");
        map.terrain.set_vertex_paint(x, z, paint).expect("room");
    }
    map.water.enabled = false;
    map.water.level = -1.5;
    map
}

fn golden_stamp(version: u32) -> FileStamp {
    match version {
        1 => FileStamp::default(),
        _ => FileStamp {
            revision: 7,
            autosave_base: None,
        },
    }
}

#[test]
fn v1_loads() {
    let loaded = load_map(&golden_path(1)).expect("v1 golden loads");
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.map, legacy_golden());
    assert_eq!(loaded.stamp, golden_stamp(1));
}

#[test]
fn v2_loads() {
    let loaded = load_map(&golden_path(2)).expect("v2 golden loads");
    assert_eq!(loaded.version, 2);
    assert_eq!(loaded.map, legacy_golden());
    assert_eq!(loaded.stamp, golden_stamp(2));
}

#[test]
fn v3_loads() {
    let loaded = load_map(&golden_path(3)).expect("v3 golden loads");
    assert_eq!(loaded.version, 3);
    assert_eq!(loaded.map, current_golden());
    assert_eq!(loaded.stamp, golden_stamp(3));
}

/// The current version's golden map: every field off its default.
fn current_golden() -> WorldMap {
    let mut map = legacy_golden();
    let mud = map
        .materials
        .add("Mud", [0.3, 0.24, 0.18], Pattern::Dirt)
        .expect("room");
    // The chunk already blends four materials, so mud takes material 3's
    // place, leaving 3 in the table but unpainted.
    assert!(map.terrain.replace_material(MaterialId(3), mud));
    map.water.shallow = [0.2, 0.5, 0.5];
    map.lighting.sun_elevation = 22.0;
    map.lighting.height_fog_density = 0.02;
    let t = &mut map.terrain;
    t.set_triangulation(Triangulation::FollowTerrain);
    t.set_cell(3, 4, CellFlags::default().with_hole(true));
    t.set_cell(10, 2, CellFlags::default().with_diagonal(Diagonal::Anti));
    t.set_cell(11, 2, CellFlags::default().with_diagonal(Diagonal::Main));
    t.set_cell(
        20,
        30,
        CellFlags::default()
            .with_no_clutter(true)
            .with_unwalkable(true),
    );
    // Bits no version assigns yet must survive too.
    t.set_cell(31, 31, CellFlags(0b1110_0000));
    for (x, z) in GridSize::new(1, 1).expect("valid size").full_rect().iter() {
        let v = ((x * 5 + z * 3) % 64) as u8;
        t.set_tint(x, z, [96 + v, 128, 160 - v]);
    }
    map
}

/// v3 once objects arrived: the v3 golden plus objects, one of them deleted
/// so the next id runs ahead of the list.
fn objects_golden() -> WorldMap {
    let mut map = current_golden();
    let oak = BUILTIN_ASSETS[0].guid;
    map.objects
        .add(oak, Vec3::new(10.0, 2.5, 12.0), 0.75, 1.25)
        .expect("room");
    let gone = map
        .objects
        .add(oak, Vec3::new(20.0, 3.0, 4.0), 0.0, 1.0)
        .expect("room");
    // A GUID no catalog knows must survive too.
    map.objects
        .add(
            AssetGuid(0xdead_beef),
            Vec3::new(40.0, -1.0, 70.0),
            3.0,
            0.5,
        )
        .expect("room");
    map.objects.remove(gone);
    map
}

fn objects_golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/v3-objects.lofimap")
}

#[test]
fn v3_with_objects_loads() {
    let loaded = load_map(&objects_golden_path()).expect("v3 objects golden loads");
    assert_eq!(loaded.map, objects_golden());
    assert_eq!(loaded.map.objects.next_id(), 4);
}

#[test]
#[ignore = "writes a golden file; run once per format version"]
fn bless() {
    let path = golden_path(MAP_FORMAT_VERSION);
    assert!(
        !path.exists(),
        "{} exists; golden files are never rewritten",
        path.display()
    );
    std::fs::create_dir_all(path.parent().expect("has parent")).expect("golden dir");
    save_map(&current_golden(), golden_stamp(MAP_FORMAT_VERSION), &path).expect("writes golden");
}

#[test]
#[ignore = "writes a golden file; run once"]
fn bless_objects() {
    let path = objects_golden_path();
    assert!(
        !path.exists(),
        "{} exists; golden files are never rewritten",
        path.display()
    );
    save_map(&objects_golden(), golden_stamp(3), &path).expect("writes golden");
}
