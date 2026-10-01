//! Prints how long chunk meshing, AO baking and a long ray take, on one
//! thread.
//!
//! `cargo run --release -p lofi_world --example measure_mesh`

use std::time::Instant;

use bevy_math::{Vec3, Vec3Swizzles};
use lofi_world::{GridSize, Ground, Holes, WorldMap, build_chunk_mesh, chunk_ao, fbm};

fn main() {
    let size = GridSize::new(8, 8).expect("valid size");
    let mut map = WorldMap::flat("measure", size, 2.0, 0.0);
    for (x, z) in size.full_rect().iter() {
        let p = map.terrain.vertex_position(x, z).xz();
        map.terrain
            .set_height(x, z, fbm(p / 80.0) * 30.0 + fbm(p / 15.0) * 3.0);
    }
    let chunks: Vec<_> = size.chunks().collect();

    let start = Instant::now();
    let mut sink = 0;
    for &chunk in &chunks {
        sink += build_chunk_mesh(&map.terrain, chunk).indices.len();
    }
    let mesh = start.elapsed().as_secs_f64() * 1e3 / chunks.len() as f64;

    let start = Instant::now();
    for &chunk in &chunks {
        sink += chunk_ao(&map.terrain, chunk).len();
    }
    let ao = start.elapsed().as_secs_f64() * 1e3 / chunks.len() as f64;

    println!("per chunk: mesh {mesh:.3} ms, ao {ao:.3} ms ({sink} items)");
    for n in [32u32, 64] {
        let count = f64::from(n * n);
        println!("{n}x{n} map, one thread: {:.0} ms", (mesh + ao) * count);
    }

    // About the longest ray a cursor casts: from above one corner down to
    // the far one, low over the hills on the way.
    let extent = map.terrain.extent();
    let (low, top) = map.terrain.height_range();
    let from = Vec3::new(0.0, top + 0.5, 0.0);
    let to = Vec3::new(extent.x, low, extent.y);
    let rays = 200;
    let start = Instant::now();
    let mut hits = 0;
    for i in 0..rays {
        let aim = to + Vec3::Y * (i as f32 * 0.01);
        let hit = map
            .terrain
            .cast_ray(from, aim - from, 10_000.0, Holes::Open);
        hits += usize::from(hit.is_some());
    }
    let ray = start.elapsed().as_secs_f64() * 1e6 / f64::from(rays);
    println!("corner-to-corner ray across 8x8 chunks: {ray:.0} us ({hits} hit)");
}
