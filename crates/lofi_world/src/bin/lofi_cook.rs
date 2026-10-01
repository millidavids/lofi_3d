//! Cooks a map for the game, or asks a cook about its ground:
//!
//! ```text
//! lofi_cook MAP.lofimap OUT_DIR [--tile-chunks N]
//! lofi_cook --probe X,Z OUT_DIR
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bevy_math::Vec2;
use lofi_world::cook::{CookSource, CookedWorld, DEFAULT_TILE_CHUNKS, cook_map};
use lofi_world::{Ground, GroundAt, load_map};

const USAGE: &str = "usage: lofi_cook MAP.lofimap OUT_DIR [--tile-chunks N]\n       \
                     lofi_cook --probe X,Z OUT_DIR";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("lofi_cook: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut tile_chunks = DEFAULT_TILE_CHUNKS;
    let mut probe_at = None;
    while let Some(arg) = args.next() {
        if arg == "--tile-chunks" {
            let n = args.next().ok_or(USAGE)?;
            tile_chunks = n
                .to_string_lossy()
                .parse()
                .map_err(|e| format!("--tile-chunks: {e}"))?;
        } else if arg == "--probe" {
            let at = args.next().ok_or(USAGE)?;
            probe_at = Some(parse_xz(&at.to_string_lossy())?);
        } else {
            paths.push(arg.into());
        }
    }
    if let Some(xz) = probe_at {
        let [dir] = &paths[..] else {
            return Err(USAGE.to_owned());
        };
        return probe(dir, xz);
    }
    let [map_path, out] = &paths[..] else {
        return Err(USAGE.to_owned());
    };
    let loaded = load_map(map_path).map_err(|e| format!("{}: {e}", map_path.display()))?;
    let source = CookSource {
        revision: loaded.stamp.revision,
        unsaved: false,
    };
    let manifest = cook_map(&loaded.map, out, tile_chunks, source).map_err(|e| e.to_string())?;
    let bytes: u64 = manifest.tiles.iter().map(|t| t.bytes).sum();
    println!(
        "cooked \"{}\" into {}: {} tiles of {tile_chunks}×{tile_chunks} chunks, {:.1} MB",
        manifest.name,
        out.display(),
        manifest.tiles.len(),
        bytes as f64 / 1e6
    );
    if let Some(bounds) = manifest.bounds() {
        println!(
            "ground from ({:.0}, {:.2}, {:.0}) to ({:.0}, {:.2}, {:.0})",
            bounds.min.x, bounds.min.y, bounds.min.z, bounds.max.x, bounds.max.y, bounds.max.z
        );
    }
    Ok(())
}

fn parse_xz(text: &str) -> Result<Vec2, String> {
    let bad = || format!("--probe: expected X,Z, got \"{text}\"");
    let (x, z) = text.split_once(',').ok_or_else(bad)?;
    let number = |s: &str| s.trim().parse::<f32>().map_err(|_| bad());
    Ok(Vec2::new(number(x)?, number(z)?))
}

/// Loads the tile under a point and says what the cooked ground is there.
fn probe(dir: &Path, xz: Vec2) -> Result<(), String> {
    let world = CookedWorld::open(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (tx, tz) = world.layout().tile_at(xz);
    let mut ground = world.ground();
    let tile = world.load_tile(tx, tz).map_err(|e| e.to_string())?;
    ground.insert(tile).map_err(|e| e.to_string())?;
    if let Some(bounds) = world.manifest.tile_bounds(tx, tz) {
        println!(
            "tile {tx}, {tz}: ({:.0}, {:.2}, {:.0}) to ({:.0}, {:.2}, {:.0})",
            bounds.min.x, bounds.min.y, bounds.min.z, bounds.max.x, bounds.max.y, bounds.max.z
        );
    }
    match ground.ground_at(xz) {
        GroundAt::Surface(point) => {
            let slope = point.normal.y.clamp(-1.0, 1.0).acos().to_degrees();
            println!(
                "ground at ({}, {}): height {:.2} m, normal ({:.3}, {:.3}, {:.3}), slope {slope:.1}°, {}",
                xz.x,
                xz.y,
                point.position.y,
                point.normal.x,
                point.normal.y,
                point.normal.z,
                if point.walkable {
                    "walkable"
                } else {
                    "not walkable"
                }
            );
        }
        GroundAt::Hole => println!("({}, {}) is over a hole", xz.x, xz.y),
        GroundAt::Unknown => println!("({}, {}) is off the map", xz.x, xz.y),
    }
    Ok(())
}
