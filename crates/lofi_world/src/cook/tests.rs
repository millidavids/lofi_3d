use std::path::PathBuf;

use bevy_math::{Vec2, Vec3Swizzles};

use super::format::{TILE_MAGIC, WORLD_MAGIC, pack, unpack};
use super::write::sibling;
use super::*;
use crate::cells::CellFlags;
use crate::grid::{CHUNK_CELLS, GridSize};
use crate::map::WorldMap;
use crate::noise::{fbm, hash01};

pub(super) fn cook(
    map: &WorldMap,
    dir: &std::path::Path,
    tile_chunks: u32,
) -> Result<Manifest, CookError> {
    cook_map(map, dir, tile_chunks, CookSource::default())
}

pub(super) fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lofi_cook_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Hills over 3×2 chunks (not a multiple of the 2-chunk tiles used here),
/// a few holes and flags, some tint.
pub(super) fn map() -> WorldMap {
    let size = GridSize::new(3, 2).expect("valid size");
    let mut map = WorldMap::flat("cooked", size, 2.0, 0.0);
    for (x, z) in size.full_rect().iter() {
        let p = map.terrain.vertex_position(x, z).xz();
        map.terrain
            .set_height(x, z, fbm(p / 40.0) * 25.0 + fbm(p / 9.0) * 3.0);
        map.terrain.set_tint(x, z, [128, (x % 200) as u8, 90]);
    }
    map.terrain
        .set_cell(40, 10, CellFlags::default().with_hole(true));
    map.terrain
        .set_cell(70, 50, CellFlags::default().with_unwalkable(true));
    map
}

#[test]
fn every_tile_round_trips_and_reads_back() {
    let map = map();
    let d = dir("round_trip");
    let manifest = cook(&map, &d, 2).expect("cooks");
    // 3×2 chunks in 2×2 tiles: 2×1 tiles, the second one chunk wide.
    assert_eq!(manifest.tiles.len(), 2);
    let world = CookedWorld::open(&d).expect("opens");
    assert_eq!(world.manifest, manifest);
    let edge = world.load_tile(1, 0).expect("loads");
    assert_eq!(edge.chunks.len(), 2, "one chunk across, two down");
    assert!(world.load_tile(2, 0).is_err());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn collision_matches_the_editor_surface() {
    let map = map();
    let d = dir("collision");
    cook(&map, &d, 2).expect("cooks");
    let world = CookedWorld::open(&d).expect("opens");
    let tiles: Vec<CookedTile> = world
        .manifest
        .tiles
        .iter()
        .map(|t| world.load_tile(t.x, t.z).expect("loads"))
        .collect();
    let extent = map.terrain.extent();
    for i in 0..3000 {
        let p = Vec2::new(hash01(i, 1, 3), hash01(i, 2, 3)) * extent;
        let editor = map.terrain.height_at(p).expect("on map");
        let cooked = tiles.iter().find_map(|t| t.height_at(p));
        if map.terrain.is_hole_at(p) {
            assert!(cooked.is_none(), "hole at {p}");
            continue;
        }
        let cooked = cooked.expect("some tile holds it");
        assert!(
            (cooked - editor).abs() <= HEIGHT_STEP * 0.5 + 1e-3,
            "{p}: {cooked} vs {editor}"
        );
    }
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn shared_vertices_match_across_chunks() {
    let map = map();
    let d = dir("seams");
    cook(&map, &d, 4).expect("cooks");
    let world = CookedWorld::open(&d).expect("opens");
    let tile = world.load_tile(0, 0).expect("loads");
    let chunk = |x: u32, z: u32| {
        tile.chunks
            .iter()
            .find(|c| (c.x, c.z) == (x, z))
            .expect("present")
    };
    let (left, right) = (chunk(0, 0), chunk(1, 0));
    for lz in 0..=CHUNK_CELLS {
        assert_eq!(left.height(CHUNK_CELLS, lz), right.height(0, lz));
    }
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn cooking_is_deterministic() {
    let map = map();
    let (a, b) = (dir("det_a"), dir("det_b"));
    cook(&map, &a, 2).expect("cooks");
    cook(&map, &b, 2).expect("cooks");
    for file in [
        MANIFEST_FILE,
        "tiles/t_0_0.lofitile",
        "tiles/t_1_0.lofitile",
    ] {
        assert_eq!(
            std::fs::read(a.join(file)).expect("a"),
            std::fs::read(b.join(file)).expect("b"),
            "{file}"
        );
    }
    let _ = std::fs::remove_dir_all(a);
    let _ = std::fs::remove_dir_all(b);
}

#[test]
fn a_chunk_too_tall_for_sixteen_bits_goes_wide() {
    let mut map = WorldMap::flat("tall", GridSize::new(1, 1).expect("valid size"), 2.0, 0.0);
    map.terrain.set_height(3, 3, 700.0);
    map.terrain.set_height(4, 3, -300.0);
    let d = dir("tall");
    cook(&map, &d, 1).expect("cooks");
    let tile = CookedWorld::open(&d)
        .expect("opens")
        .load_tile(0, 0)
        .expect("loads");
    assert_eq!(tile.chunks[0].height(3, 3), 700.0);
    assert_eq!(tile.chunks[0].height(4, 3), -300.0);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn objects_land_in_the_tile_under_them() {
    let mut map = map();
    let tree = crate::objects::AssetGuid(0x0123_4567_89ab_cdef_0011_2233_4455_6677);
    let inside = map
        .objects
        .add(tree, bevy_math::Vec3::new(140.0, 3.0, 20.0), 1.0, 2.0)
        .expect("room");
    // Past the far corner: the nearest edge tile takes it.
    let outside = map
        .objects
        .add(tree, bevy_math::Vec3::new(900.0, 0.0, -50.0), 0.0, 1.0)
        .expect("room");
    let d = dir("objects");
    cook(&map, &d, 2).expect("cooks");
    let world = CookedWorld::open(&d).expect("opens");
    // Tiles are 128 m: x = 140 is in tile 1.
    let edge = world.load_tile(1, 0).expect("loads");
    let ids: Vec<u64> = edge.objects.iter().map(|o| o.id).collect();
    assert_eq!(ids, vec![inside.0, outside.0]);
    let o = edge.objects[0];
    assert_eq!(
        (u128::from(o.asset[0]) << 64) | u128::from(o.asset[1]),
        tree.0
    );
    assert_eq!(o.position, [140.0, 3.0, 20.0]);
    assert!(world.load_tile(0, 0).expect("loads").objects.is_empty());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn recooking_replaces_an_earlier_cook_and_nothing_else() {
    let map = map();
    let d = dir("recook");
    cook(&map, &d, 1).expect("cooks");
    assert_eq!(
        std::fs::read_dir(d.join(TILE_DIR)).expect("tiles").count(),
        6
    );
    // Bigger tiles: the six old files go, two new ones come.
    cook(&map, &d, 2).expect("recooks");
    assert_eq!(
        std::fs::read_dir(d.join(TILE_DIR)).expect("tiles").count(),
        2
    );

    // A folder with someone's own files in it is refused and left alone.
    std::fs::write(d.join("notes.txt"), "mine").expect("write");
    assert!(matches!(cook(&map, &d, 2), Err(CookError::Invalid(_))));
    assert!(d.join("notes.txt").exists());
    assert!(d.join(MANIFEST_FILE).exists());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_recook_that_cannot_swap_in_keeps_the_old_cook() {
    let map = map();
    let d = dir("keep");
    cook(&map, &d, 2).expect("cooks");
    let before = std::fs::read(d.join(MANIFEST_FILE)).expect("manifest");
    // Something of the user's where the old cook is moved aside to.
    let aside = sibling(&d, "cooked-old").expect("named");
    std::fs::create_dir_all(&aside).expect("mkdir");
    std::fs::write(aside.join("notes.txt"), "mine").expect("write");

    assert!(cook(&map, &d, 1).is_err());
    assert_eq!(
        std::fs::read(d.join(MANIFEST_FILE)).expect("still there"),
        before
    );
    assert!(
        CookedWorld::open(&d)
            .expect("opens")
            .load_tile(0, 0)
            .is_ok()
    );
    assert!(aside.join("notes.txt").exists(), "left alone");
    assert!(!sibling(&d, "cooking").expect("named").exists(), "tidied");
    let _ = std::fs::remove_dir_all(aside);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn staging_only_ever_loses_a_cooks_own_files() {
    let map = map();
    let d = dir("staging");
    let staging = sibling(&d, "cooking").expect("named");
    let _ = std::fs::remove_dir_all(&staging);
    // A cook that died half way: tiles, no manifest.
    std::fs::create_dir_all(staging.join(TILE_DIR)).expect("mkdir");
    std::fs::write(staging.join("tiles/t_9_9.lofitile"), "stale").expect("write");
    cook(&map, &d, 2).expect("clears the leftover");
    assert!(!d.join("tiles/t_9_9.lofitile").exists());

    std::fs::create_dir_all(&staging).expect("mkdir");
    std::fs::write(staging.join("notes.txt"), "mine").expect("write");
    assert!(matches!(cook(&map, &d, 2), Err(CookError::Invalid(_))));
    assert!(staging.join("notes.txt").exists());
    let _ = std::fs::remove_dir_all(staging);
    let _ = std::fs::remove_dir_all(d);
}

#[cfg(unix)]
#[test]
fn cooking_through_a_link_fills_the_folder_it_points_at() {
    let map = map();
    let real = dir("link_real");
    let link = dir("link");
    let _ = std::fs::remove_file(&link);
    std::fs::create_dir_all(&real).expect("mkdir");
    std::os::unix::fs::symlink(&real, &link).expect("link");
    cook(&map, &link, 2).expect("cooks");
    cook(&map, &link, 1).expect("recooks");
    assert!(link.symlink_metadata().expect("there").is_symlink());
    assert!(real.join(MANIFEST_FILE).exists());
    let _ = std::fs::remove_file(link);
    let _ = std::fs::remove_dir_all(real);
}

#[test]
fn objects_this_build_cannot_read_are_not_cooked_away() {
    let mut map = map();
    map.objects = crate::objects::ObjectLayer::foreign();
    let d = dir("foreign");
    assert!(matches!(cook(&map, &d, 2), Err(CookError::Invalid(_))));
    assert!(!d.exists());
}

#[test]
fn the_far_edge_is_on_the_map_at_any_cell_size() {
    for cell_size in [0.5, 0.7, 1.1, 1.3, 2.0, 3.3] {
        let size = GridSize::new(3, 2).expect("valid size");
        let map = WorldMap::flat("edge", size, cell_size, 5.0);
        let d = dir("edge");
        cook(&map, &d, 2).expect("cooks");
        let world = CookedWorld::open(&d).expect("opens");
        let tile = world.load_tile(1, 0).expect("loads");
        let extent = map.terrain.extent();
        for p in [
            extent,
            Vec2::new(extent.x, 1.0),
            Vec2::new(extent.x - 1.0, extent.y),
        ] {
            assert!(map.terrain.height_at(p).is_some(), "{cell_size}: {p}");
            assert_eq!(tile.height_at(p), Some(5.0), "{cell_size}: {p}");
        }
        let _ = std::fs::remove_dir_all(d);
    }
}

#[test]
fn a_manifest_cannot_point_outside_its_folder() {
    let map = map();
    let d = dir("escape");
    let mut manifest = cook(&map, &d, 2).expect("cooks");
    for file in ["/etc/hostname", "../t_0_0.lofitile", "tiles/../../x"] {
        manifest.tiles[0].file = file.to_owned();
        let bytes = pack(WORLD_MAGIC, &manifest).expect("packs");
        std::fs::write(d.join(MANIFEST_FILE), bytes).expect("write");
        assert!(
            matches!(CookedWorld::open(&d), Err(CookError::Corrupt(_))),
            "{file}"
        );
    }
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_tile_must_hold_exactly_its_chunks() {
    let map = map();
    let d = dir("chunks");
    let manifest = cook(&map, &d, 2).expect("cooks");
    let path = d.join(&manifest.tiles[0].file);
    let good: CookedTile = unpack(TILE_MAGIC, &std::fs::read(&path).expect("read")).expect("ok");
    let rewrite = |tile: &CookedTile| {
        let bytes = pack(TILE_MAGIC, tile).expect("packs");
        std::fs::write(&path, &bytes).expect("write");
        let mut manifest = manifest.clone();
        manifest.tiles[0].bytes = bytes.len() as u64;
        manifest.tiles[0].crc32 = crc32fast::hash(&bytes);
        std::fs::write(
            d.join(MANIFEST_FILE),
            pack(WORLD_MAGIC, &manifest).expect("packs"),
        )
        .expect("write");
        CookedWorld::open(&d).expect("opens").load_tile(0, 0)
    };
    assert!(rewrite(&good).is_ok());

    let mut doubled = good.clone();
    doubled.chunks[1] = doubled.chunks[0].clone();
    assert!(rewrite(&doubled).is_err(), "the same chunk twice");
    let mut stray = good.clone();
    stray.chunks[0].x = 40;
    assert!(rewrite(&stray).is_err(), "a chunk from elsewhere");
    let mut short = good.clone();
    short.chunks.pop();
    assert!(rewrite(&short).is_err(), "a chunk missing");
    let mut lost = good;
    lost.objects.push(CookedObject {
        id: 1,
        asset: [0, 1],
        position: [f32::NAN, 0.0, 0.0],
        yaw: 0.0,
        scale: 1.0,
    });
    assert!(rewrite(&lost).is_err(), "an object nowhere");
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn the_manifest_records_its_source() {
    let map = map();
    let d = dir("source");
    let source = CookSource {
        revision: 41,
        unsaved: true,
    };
    let manifest = cook_map(&map, &d, 2, source).expect("cooks");
    assert_eq!(
        (manifest.source_revision, manifest.source_unsaved),
        (41, true)
    );
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn damaged_tiles_error_without_panicking() {
    let map = map();
    let d = dir("damaged");
    cook(&map, &d, 2).expect("cooks");
    let path = d.join("tiles/t_0_0.lofitile");
    let good = std::fs::read(&path).expect("read");
    let world = CookedWorld::open(&d).expect("opens");
    for cut in [0, 7, 20, good.len() / 2, good.len() - 1] {
        std::fs::write(&path, &good[..cut]).expect("write");
        assert!(world.load_tile(0, 0).is_err(), "cut to {cut}");
    }
    for i in (0..good.len()).step_by(97) {
        let mut bad = good.clone();
        bad[i] ^= 0x5a;
        std::fs::write(&path, &bad).expect("write");
        assert!(world.load_tile(0, 0).is_err(), "byte {i} flipped");
        // The bytes alone, past the manifest's checks, must not panic either.
        let _ = unpack::<CookedTile>(TILE_MAGIC, &bad);
    }
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn baked_flags_and_light_come_through() {
    let map = map();
    let d = dir("flags");
    cook(&map, &d, 4).expect("cooks");
    let world = CookedWorld::open(&d).expect("opens");
    let tile = world.load_tile(0, 0).expect("loads");
    let chunk = |x: u32, z: u32| {
        tile.chunks
            .iter()
            .find(|c| (c.x, c.z) == (x, z))
            .expect("present")
    };
    // Cell (40, 10) is chunk (1, 0)'s (8, 10); (70, 50) is chunk (2, 1)'s (6, 18).
    assert!(chunk(1, 0).is_hole(8, 10));
    assert!(chunk(2, 1).unwalkable(6, 18));
    assert!(!chunk(2, 1).no_clutter(6, 18));
    let terrain = &map.terrain;
    assert_eq!(
        chunk(1, 0).splits_on_main(3, 4),
        terrain.splits_on_main_diagonal(35, 4)
    );
    assert_eq!(chunk(0, 0).light(5, 0)[..3], terrain.tint(5, 0));
    let n = chunk(0, 0).normal(7, 7);
    assert!(n.angle_between(terrain.normal(7, 7)) < 1.5_f32.to_radians());
    let _ = std::fs::remove_dir_all(d);
}
