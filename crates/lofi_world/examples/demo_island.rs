//! Writes a small painted island map to start from.
//!
//! `cargo run -p lofi_world --example demo_island -- [OUT.lofimap] [CHUNKS]`
//!
//! `CHUNKS` (default 8) sets the island's width in chunks, for measuring.

use std::path::PathBuf;

use bevy_math::{Vec2, Vec3Swizzles};
use std::f32::consts::TAU;

use bevy_math::Vec3;
use lofi_world::{
    BUILTIN_ASSETS, CellFlags, FileStamp, GridSize, MaterialId, Tint, VertexPaint, WorldMap, fbm,
    hash01, save_map,
};

// The starter material set.
const GRASS: MaterialId = MaterialId(0);
const DIRT: MaterialId = MaterialId(1);
const ROCK: MaterialId = MaterialId(2);
const SAND: MaterialId = MaterialId(3);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path: PathBuf = args
        .next()
        .map_or_else(|| "demo_island.lofimap".into(), PathBuf::from);
    let chunks: u32 = args.next().map_or(Ok(8), |s| s.parse())?;

    let size = GridSize::new(chunks, chunks).ok_or("bad size")?;
    let mut map = WorldMap::flat("Demo Island", size, 2.0, 0.0);
    let extent = map.terrain.extent();
    let center = extent * 0.5;

    for (x, z) in size.full_rect().iter() {
        let p = map.terrain.vertex_position(x, z).xz();
        // Smaller than the map, so deep water rings it before the open sea.
        map.terrain
            .set_height(x, z, island_height(p, center, extent.x * 0.42));
    }
    for (x, z) in size.full_rect().iter() {
        let p = map.terrain.vertex_position(x, z);
        let slope = 1.0 - map.terrain.normal(x, z).y;
        // Only four materials in play, so every chunk has room.
        map.terrain
            .set_vertex_paint(x, z, paint(p.xz(), p.y, slope))
            .map_err(|chunk| format!("chunk {chunk:?} is full"))?;
        map.terrain.set_tint(x, z, tint(p.xz()));
    }
    // A cave mouth: a few cells cut out of a hillside.
    let (cx, cz) = (size.cells_x() * 3 / 5, size.cells_z() / 2);
    for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1), (2, 1)] {
        map.terrain
            .set_cell(cx + dx, cz + dz, CellFlags::default().with_hole(true));
    }
    scatter_objects(&mut map);

    save_map(&map, FileStamp::default(), &path)?;
    println!("wrote {}", path.display());
    Ok(())
}

/// Woods in the grass, boulders on the steeps, and a hut by the cave.
fn scatter_objects(map: &mut WorldMap) {
    let [oak, pine, boulder, _, hut, _] = BUILTIN_ASSETS.map(|a| a.guid);
    let extent = map.terrain.extent();
    let tries = (extent.x * extent.y / 300.0) as i32;
    for i in 0..tries {
        let r = |k: i32| hash01(i, k, 77);
        let p = Vec2::new(r(1), r(2)) * extent;
        let Some(h) = map.terrain.height_at(p) else {
            continue;
        };
        let woods = fbm(p / 60.0 + 3.3);
        let (x, z) = (
            (p.x / map.terrain.cell_size()) as u32,
            (p.y / map.terrain.cell_size()) as u32,
        );
        let slope = 1.0 - map.terrain.normal(x, z).y;
        let asset = if slope > 0.2 && r(3) < 0.5 {
            boulder
        } else if h > 4.0 && slope < 0.15 && woods > 0.1 {
            if h > 18.0 || r(4) < 0.3 { pine } else { oak }
        } else {
            continue;
        };
        let place = Vec3::new(p.x, h, p.y);
        map.objects
            .add(asset, place, r(5) * TAU, 0.8 + r(6) * 0.5)
            .ok_or("object out of bounds")
            .ok();
    }
    let size = map.terrain.size();
    let door = Vec2::new(
        size.cells_x() as f32 * 0.6 + 6.0,
        size.cells_z() as f32 * 0.5 - 6.0,
    ) * map.terrain.cell_size();
    if let Some(h) = map.terrain.height_at(door) {
        map.objects.add(hut, Vec3::new(door.x, h, door.y), 0.6, 1.0);
    }
}

/// Rolling hills inside a coastline, sea floor outside it.
fn island_height(p: Vec2, center: Vec2, radius: f32) -> f32 {
    let warp = Vec2::new(fbm(p / 140.0), fbm(p / 140.0 + 31.7)) * 60.0;
    let d = ((p + warp) - center).length() / radius;
    let land = smoothstep(1.0, 0.55, d);
    let hills = fbm(p / 110.0) * 0.5 + 0.5;
    let ridge = 1.0 - fbm(p / 70.0 + 9.1).abs();
    -8.0 + land * (12.0 + 38.0 * hills * hills + 14.0 * ridge * ridge * hills)
}

/// Sand at the shore, rock on steep ground, dirt patches in the grass.
fn paint(p: Vec2, height: f32, slope: f32) -> VertexPaint {
    let sand = smoothstep(3.5, 1.0, height);
    let rock = smoothstep(0.18, 0.32, slope) * (1.0 - sand);
    let dirt = smoothstep(0.15, 0.45, fbm(p / 35.0 + 4.2)) * 0.8 * (1.0 - sand - rock);
    let grass = (1.0 - sand - rock - dirt).max(0.0);
    let weight = |v: f32| (v * 255.0).round() as u8;
    VertexPaint::new(&[
        (GRASS, weight(grass)),
        (DIRT, weight(dirt)),
        (ROCK, weight(rock)),
        (SAND, weight(sand)),
    ])
    .unwrap_or(VertexPaint::solid(GRASS))
}

/// Broad warm and cool washes, like hand-painted vertex colour.
fn tint(p: Vec2) -> Tint {
    let n = fbm(p / 90.0 + 17.0);
    let shift = |amount: f32| (128.0 + n * amount).round() as u8;
    [shift(14.0), shift(5.0), shift(-14.0)]
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
