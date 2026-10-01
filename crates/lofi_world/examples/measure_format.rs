//! Prints file size and encode/decode time for big painted maps.
//!
//! `cargo run --release -p lofi_world --example measure_format`

use std::time::Instant;

use bevy_math::{Vec2, Vec3Swizzles};
use lofi_world::{
    FileStamp, GridSize, MaterialId, VertexPaint, WorldMap, decode_map, encode_map, fbm,
};

fn main() {
    for chunks in [8, 32, 64] {
        let map = painted_hills(chunks);
        let verts = map.terrain.size().vertex_count();

        let start = Instant::now();
        let bytes = encode_map(&map, FileStamp::default()).expect("encodes");
        let encode = start.elapsed();
        let start = Instant::now();
        let loaded = decode_map(&bytes).expect("decodes");
        let decode = start.elapsed();
        assert_eq!(loaded.map, map);

        let raw = verts * 8;
        println!(
            "{chunks:>2}x{chunks:<2} chunks  {verts:>9} verts  raw {:>6.1} MB  file {:>6.1} MB ({:>4.1}%)  encode {:>6.0} ms  decode {:>6.0} ms",
            mb(raw),
            mb(bytes.len()),
            100.0 * bytes.len() as f64 / raw as f64,
            encode.as_secs_f64() * 1e3,
            decode.as_secs_f64() * 1e3,
        );
    }
}

/// Hills with slope- and height-driven paint, like a sculpted zone.
fn painted_hills(chunks: u32) -> WorldMap {
    let size = GridSize::new(chunks, chunks).expect("valid size");
    let mut map = WorldMap::flat("measure", size, 2.0, 0.0);
    for (x, z) in size.full_rect().iter() {
        let p = map.terrain.vertex_position(x, z).xz();
        let h = fbm(p / 120.0) * 40.0 + fbm(p / 30.0) * 6.0;
        map.terrain.set_height(x, z, h);
    }
    for (x, z) in size.full_rect().iter() {
        let p = map.terrain.vertex_position(x, z);
        let slope = 1.0 - map.terrain.normal(x, z).y;
        let rock = ((slope - 0.1) * 6.0).clamp(0.0, 1.0);
        let dirt = (fbm(p.xz() / 35.0 + Vec2::splat(4.2)) * 2.0).clamp(0.0, 1.0) * (1.0 - rock);
        let sand = ((2.0 - p.y) * 0.5).clamp(0.0, 1.0) * (1.0 - rock);
        let grass = (1.0 - rock - dirt - sand).max(0.0);
        let pairs: Vec<_> = [grass, dirt, rock, sand]
            .into_iter()
            .enumerate()
            .map(|(i, v)| (MaterialId(i as u16), (v * 255.0) as u8))
            .collect();
        if let Some(paint) = VertexPaint::new(&pairs) {
            map.terrain
                .set_vertex_paint(x, z, paint)
                .expect("four materials fit");
        }
    }
    map
}

fn mb(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}
