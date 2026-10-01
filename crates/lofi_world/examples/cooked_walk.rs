//! Walks across a cooked world the way a game would: tiles stream in around
//! the walker, and the cooked ground says where to stand.
//!
//! `cargo run -p lofi_world --example cooked_walk -- COOKED_DIR [RADIUS]`

use std::path::PathBuf;

use bevy_math::{Vec2, Vec3};
use lofi_world::cook::{CookError, CookedGround, CookedWorld};
use lofi_world::{Ground, GroundAt, Holes, RayTrace};

/// Metres between the walker's steps.
const STRIDE: f32 = 16.0;

fn main() -> Result<(), CookError> {
    let mut args = std::env::args_os().skip(1);
    let dir: PathBuf = args.next().map_or_else(|| "cooked".into(), Into::into);
    let radius: f32 = args
        .next()
        .and_then(|r| r.to_string_lossy().parse().ok())
        .unwrap_or(100.0);

    let world = CookedWorld::open(&dir)?;
    let layout = world.layout();
    let mut ground = world.ground();
    let extent = layout.extent();
    let steps = (extent.length() / STRIDE) as u32;
    println!(
        "{}: {:.0} × {:.0} m in {} tiles; keeping tiles within {radius:.0} m loaded",
        world.manifest.name,
        extent.x,
        extent.y,
        world.manifest.tiles.len()
    );

    let (mut walked, mut blocked, mut holes, mut loads) = (0, 0, 0, 0);
    for step in 0..=steps {
        // Corner to corner.
        let at = extent * (step as f32 / steps.max(1) as f32);
        loads += stream(&world, &mut ground, at, radius)?;
        match ground.ground_at(at) {
            GroundAt::Surface(point) if point.walkable => walked += 1,
            GroundAt::Surface(_) => blocked += 1,
            GroundAt::Hole => holes += 1,
            GroundAt::Unknown => println!("  nothing loaded under {at}"),
        }
    }
    println!(
        "{walked} steps on walkable ground, {blocked} too steep or flagged, {holes} over holes"
    );
    println!(
        "{loads} tile loads; {} tiles in memory at the end",
        ground.loaded().count()
    );

    // Where the walker ended up, looking back down towards the middle. The
    // ray says when it runs out of loaded ground, rather than calling that
    // a clear line.
    let Some(feet) = ground.point_at(extent) else {
        return Ok(());
    };
    let eye = feet.position + Vec3::Y * 30.0;
    let towards = Vec3::new(extent.x * 0.5, feet.position.y, extent.y * 0.5) - eye;
    print!("from 30 m over the far corner, looking back: ");
    match ground.trace_ray(eye, towards, 2000.0, Holes::Open) {
        RayTrace::Hit(hit) => println!(
            "ground {:.1} m away, {}",
            hit.distance,
            if hit.walkable {
                "walkable"
            } else {
                "not walkable"
            }
        ),
        RayTrace::Clear => println!("nothing in the way"),
        RayTrace::Unknown { distance } => {
            println!("clear for {distance:.1} m, then ground that isn't loaded");
        }
    }
    Ok(())
}

/// Loads what's near `at` and drops what isn't. Returns how many tiles it
/// read.
fn stream(
    world: &CookedWorld,
    ground: &mut CookedGround,
    at: Vec2,
    radius: f32,
) -> Result<u32, CookError> {
    let near = world.layout().tiles_within(at, radius);
    let far: Vec<_> = ground.loaded().filter(|t| !near.contains(t)).collect();
    for (x, z) in far {
        ground.remove(x, z);
    }
    let mut loads = 0;
    for (x, z) in near {
        if !ground.is_loaded(x, z) {
            ground.insert(world.load_tile(x, z)?)?;
            loads += 1;
        }
    }
    Ok(loads)
}
