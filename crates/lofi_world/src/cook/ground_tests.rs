//! The cooked terrain describing itself: its ground, its boxes, where its
//! tiles are.

use bevy_math::{Vec2, Vec3, Vec3Swizzles};

use super::format::{WORLD_MAGIC, pack};
use super::tests::{cook, dir, map};
use super::*;
use crate::grid::{CHUNK_CELLS, ChunkCoord};
use crate::ground::tests::brute_force;
use crate::ground::{Bounds, Ground, GroundAt, Holes, RayTrace};
use crate::masks::WALKABLE_SLOPE_DEGREES;
use crate::noise::hash01;

/// Every tile of a cooked world, loaded.
fn whole(world: &CookedWorld) -> CookedGround {
    let mut ground = world.ground();
    for (x, z) in world.layout().tiles() {
        ground
            .insert(world.load_tile(x, z).expect("loads"))
            .expect("fits");
    }
    ground
}

#[test]
fn cooked_ground_answers_as_the_editor_does() {
    let map = map();
    let d = dir("ground");
    cook(&map, &d, 2).expect("cooks");
    let ground = whole(&CookedWorld::open(&d).expect("opens"));
    let terrain = &map.terrain;
    let limit = WALKABLE_SLOPE_DEGREES.to_radians().cos();
    let extent = terrain.extent();
    for i in 0..3000 {
        let xz = Vec2::new(hash01(i, 1, 8), hash01(i, 2, 8)) * extent;
        match (terrain.ground_at(xz), ground.ground_at(xz)) {
            (GroundAt::Hole, GroundAt::Hole) => {}
            (GroundAt::Surface(editor), GroundAt::Surface(cooked)) => {
                let off = (editor.position.y - cooked.position.y).abs();
                assert!(off <= HEIGHT_STEP * 0.5 + 1e-3, "{xz}: {off} m apart");
                // A centimetre across a 2 m cell tilts a face under a degree.
                assert!(editor.normal.dot(cooked.normal) > 0.9998, "{xz}");
                if (editor.normal.y - limit).abs() > 0.02 {
                    assert_eq!(editor.walkable, cooked.walkable, "{xz}");
                }
                assert_eq!(editor.no_clutter, cooked.no_clutter);
                // Straight down, the two can't be far apart either.
                let above = Vec3::new(xz.x, 500.0, xz.y);
                let hit = ground
                    .cast_ray(above, Vec3::NEG_Y, 1000.0, Holes::Open)
                    .expect("ground below");
                assert!((hit.point.y - editor.position.y).abs() <= HEIGHT_STEP * 0.5 + 2e-3);
            }
            other => panic!("{xz}: {other:?}"),
        }
    }
    assert_eq!(ground.ground_at(Vec2::splat(-1.0)), GroundAt::Unknown);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn cooked_rays_cross_tile_seams() {
    let map = map();
    let d = dir("rays");
    // One chunk per tile: every chunk border is a tile seam.
    cook(&map, &d, 1).expect("cooks");
    let ground = whole(&CookedWorld::open(&d).expect("opens"));
    let extent = map.terrain.extent();
    let r = |i: i32, k: i32| hash01(i, k, 55);
    let mut hits = 0;
    for i in 0..2000 {
        let origin = Vec3::new(
            (r(i, 0) * 1.4 - 0.2) * extent.x,
            r(i, 1) * 70.0 - 5.0,
            (r(i, 2) * 1.4 - 0.2) * extent.y,
        );
        let dir = Vec3::new(r(i, 3) - 0.5, r(i, 4) * 0.5 - 0.4, r(i, 5) - 0.5).normalize();
        let found = ground
            .cast_ray(origin, dir, 400.0, Holes::Open)
            .map(|h| h.distance);
        let expected = brute_force(&ground, origin, dir, 400.0, Holes::Open);
        match (found, expected) {
            (Some(a), Some(b)) => {
                assert!((a - b).abs() < 1e-3, "ray {i}: {a} vs {b}");
                hits += 1;
            }
            (None, None) => {}
            other => panic!("ray {i}: {other:?}"),
        }
    }
    assert!(hits > 500, "only {hits} rays hit");
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn ground_that_is_not_loaded_is_unknown_not_a_hole() {
    let map = map();
    let d = dir("partial");
    cook(&map, &d, 1).expect("cooks");
    let world = CookedWorld::open(&d).expect("opens");
    let mut ground = world.ground();
    ground
        .insert(world.load_tile(0, 0).expect("loads"))
        .expect("fits");
    assert!(ground.is_loaded(0, 0) && !ground.is_loaded(1, 0));

    assert!(matches!(
        ground.ground_at(Vec2::new(30.0, 30.0)),
        GroundAt::Surface(_)
    ));
    assert_eq!(ground.ground_at(Vec2::new(100.0, 30.0)), GroundAt::Unknown);
    // The border vertices are in the loaded tile, so its far edge is ground.
    assert!(ground.point_at(Vec2::new(64.0, 30.0)).is_some());
    assert!(ground.vertex_height(CHUNK_CELLS, 5).is_some());
    assert!(ground.vertex_height(CHUNK_CELLS + 1, 5).is_none());

    let loaded = ChunkCoord { x: 0, z: 0 };
    let missing = ChunkCoord { x: 1, z: 0 };
    assert!(ground.chunk_bounds(loaded).is_some());
    assert!(ground.chunk_collision(missing).is_none());
    // A ray over the missing tile can't say what's there: not a hit, and
    // not a clear line either.
    let above = Vec3::new(100.0, 500.0, 30.0);
    let down = |from: Vec3| ground.trace_ray(from, Vec3::NEG_Y, 1000.0, Holes::Solid);
    assert_eq!(down(above), RayTrace::Unknown { distance: 0.0 });
    assert!(
        ground
            .cast_ray(above, Vec3::NEG_Y, 1000.0, Holes::Open)
            .is_none()
    );
    // Straight down the loaded tile's far edge is its ground, as
    // `ground_at` says.
    assert!(matches!(
        down(Vec3::new(64.0, 500.0, 30.0)),
        RayTrace::Hit(_)
    ));
    assert!(matches!(
        down(Vec3::new(30.0, 500.0, 64.0)),
        RayTrace::Hit(_)
    ));

    assert!(ground.remove(0, 0).is_some());
    assert_eq!(ground.ground_at(Vec2::new(30.0, 30.0)), GroundAt::Unknown);
    let _ = std::fs::remove_dir_all(d);
}

/// Line of sight mustn't change as tiles stream in: over ground that's only
/// partly loaded, a ray gives the answer the whole world would, or says it
/// doesn't know. Never a different hit, never "clear" through a hill.
#[test]
fn a_ray_over_partly_loaded_ground_never_gives_a_wrong_answer() {
    let map = map();
    let d = dir("streaming");
    cook(&map, &d, 1).expect("cooks");
    let world = CookedWorld::open(&d).expect("opens");
    let full = whole(&world);
    let mut partial = world.ground();
    for (x, z) in [(2, 0), (0, 1)] {
        partial
            .insert(world.load_tile(x, z).expect("loads"))
            .expect("fits");
    }
    let extent = map.terrain.extent();
    let r = |i: i32, k: i32| hash01(i, k, 61);
    let (mut same, mut unknown) = (0, 0);
    for i in 0..5000 {
        let origin = Vec3::new(r(i, 0) * extent.x, 5.0 + r(i, 1) * 60.0, r(i, 2) * extent.y);
        let dir = Vec3::new(r(i, 3) - 0.5, r(i, 4) * 0.4 - 0.35, r(i, 5) - 0.5).normalize();
        let whole_world = full.trace_ray(origin, dir, 400.0, Holes::Open);
        match partial.trace_ray(origin, dir, 400.0, Holes::Open) {
            RayTrace::Unknown { distance } => {
                // And it stopped before anything the whole world would hit.
                if let RayTrace::Hit(hit) = whole_world {
                    assert!(distance <= hit.distance + 1e-3, "ray {i}");
                }
                unknown += 1;
            }
            known => {
                assert_eq!(known, whole_world, "ray {i}");
                same += 1;
            }
        }
    }
    assert!(
        same > 300 && unknown > 300,
        "{same} known, {unknown} unknown"
    );
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_tile_from_another_world_is_not_taken_in() {
    let map = map();
    let (ones, twos) = (dir("ones"), dir("twos"));
    cook(&map, &ones, 1).expect("cooks");
    cook(&map, &twos, 2).expect("cooks");
    let small = CookedWorld::open(&ones).expect("opens");
    let mut ground = CookedWorld::open(&twos).expect("opens").ground();
    let tile = small.load_tile(1, 0).expect("loads");
    assert!(matches!(ground.insert(tile), Err(CookError::Invalid(_))));
    assert_eq!(ground.loaded().count(), 0);
    let _ = std::fs::remove_dir_all(ones);
    let _ = std::fs::remove_dir_all(twos);
}

#[test]
fn the_manifest_alone_places_and_boxes_every_tile() {
    let map = map();
    let d = dir("bounds");
    let manifest = cook(&map, &d, 2).expect("cooks");
    let world = CookedWorld::open(&d).expect("opens");
    let ground = whole(&world);
    let layout = manifest.layout().expect("valid");
    assert_eq!(layout.tile_counts(), (2, 1));

    for (x, z) in layout.tiles() {
        let listed = manifest.tile_bounds(x, z).expect("listed");
        // What the tile's own chunks say, once loaded.
        let scanned = layout
            .chunks_of(x, z)
            .map(|c| ground.chunk_bounds(c).expect("loaded"))
            .reduce(Bounds::union)
            .expect("has chunks");
        assert!(listed.min.distance(scanned.min) < 1e-3, "tile {x}, {z}");
        assert!(listed.max.distance(scanned.max) < 1e-3, "tile {x}, {z}");
        // And the editor's vertices are inside it, give or take a step.
        for chunk in layout.chunks_of(x, z) {
            for (vx, vz) in chunk.vertex_rect().iter() {
                let p = map.terrain.vertex_position(vx, vz);
                assert!(p.y >= listed.min.y - HEIGHT_STEP && p.y <= listed.max.y + HEIGHT_STEP);
                assert!(p.xz().cmpge(listed.min.xz()).all() && p.xz().cmple(listed.max.xz()).all());
            }
        }
    }
    let all = manifest.bounds().expect("has tiles");
    assert_eq!(all.min.xz(), Vec2::ZERO);
    assert_eq!(all.max.xz(), map.terrain.extent());
    assert!(manifest.tile_bounds(2, 0).is_none());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_manifest_that_lies_about_a_tiles_height_is_caught() {
    let map = map();
    let d = dir("liar");
    let mut manifest = cook(&map, &d, 2).expect("cooks");
    manifest.tiles[0].max_step -= 50;
    let bytes = pack(WORLD_MAGIC, &manifest).expect("packs");
    std::fs::write(d.join(MANIFEST_FILE), bytes).expect("write");
    let world = CookedWorld::open(&d).expect("opens");
    assert!(matches!(world.load_tile(0, 0), Err(CookError::Corrupt(_))));
    assert!(world.load_tile(1, 0).is_ok());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn an_older_cook_asks_to_be_cooked_again() {
    let map = map();
    let d = dir("old");
    cook(&map, &d, 2).expect("cooks");
    let path = d.join(MANIFEST_FILE);
    let mut bytes = std::fs::read(&path).expect("read");
    // Version and minimum reader, after the 8-byte magic.
    bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
    std::fs::write(&path, bytes).expect("write");
    assert!(matches!(
        CookedWorld::open(&d),
        Err(CookError::TooOld { found: 1 })
    ));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn cooked_collision_has_the_editors_triangles() {
    let map = map();
    let d = dir("mesh");
    cook(&map, &d, 2).expect("cooks");
    let ground = whole(&CookedWorld::open(&d).expect("opens"));
    for chunk in map.terrain.size().chunks() {
        let editor = map.terrain.chunk_collision(chunk).expect("on the map");
        let cooked = ground.chunk_collision(chunk).expect("loaded");
        assert_eq!(editor.indices, cooked.indices, "{chunk:?}");
        for (a, b) in editor.positions.iter().zip(&cooked.positions) {
            assert_eq!((a[0], a[2]), (b[0], b[2]));
            assert!((a[1] - b[1]).abs() <= HEIGHT_STEP * 0.5 + 1e-3);
        }
    }
    // The hole at cell (40, 10) is missing from chunk (1, 0).
    let holed = ground
        .chunk_collision(ChunkCoord { x: 1, z: 0 })
        .expect("loaded");
    assert_eq!(
        holed.indices.len(),
        ((CHUNK_CELLS * CHUNK_CELLS - 1) * 6) as usize
    );
    let _ = std::fs::remove_dir_all(d);
}
