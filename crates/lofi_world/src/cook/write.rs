//! Cooking a map into a folder, and swapping it in without losing the cook
//! that was there.

use std::path::{Path, PathBuf};

use super::chunk::CookedChunk;
use super::format::{
    CookError, HEIGHT_STEP, MANIFEST_FILE, MAX_TILE_CHUNKS, TILE_DIR, TILE_MAGIC, WORLD_MAGIC, pack,
};
use bevy_math::Vec3Swizzles;

use super::layout::TileLayout;
use super::manifest::{CookSource, CookedMaterial, CookedWater, Manifest, TileEntry};
use super::tile::{CookedObject, CookedTile};
use crate::file::write_file;
use crate::map::WorldMap;

/// Cooks `map` into `dir`. Everything is written to a sibling folder first,
/// then swapped in, so an interrupted cook never leaves a manifest that
/// disagrees with its tiles. `dir` must be missing, empty, or an earlier
/// cook; anything else in it is refused, never deleted.
///
/// # Errors
/// If `dir` holds other files, or a file can't be written.
pub fn cook_map(
    map: &WorldMap,
    dir: &Path,
    tile_chunks: u32,
    source: CookSource,
) -> Result<Manifest, CookError> {
    if !(1..=MAX_TILE_CHUNKS).contains(&tile_chunks) {
        return Err(CookError::Invalid(format!(
            "tiles of {tile_chunks} chunks; use 1 to {MAX_TILE_CHUNKS}"
        )));
    }
    if map.objects.is_foreign() {
        // They'd cook to nothing, and the game would ship without them.
        return Err(CookError::Invalid(
            "the map's objects were saved by a newer tool; cook with that version".to_owned(),
        ));
    }
    // The real folder, so a link to it is cooked into rather than replaced.
    let dir = &dir.canonicalize().or_else(|_| std::path::absolute(dir))?;
    previous_cook(dir)?;
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _lock = lock_cook(dir)?;
    let staging = sibling(dir, "cooking")?;
    remove_cook(&staging)?;
    let swapped = write_cook(map, &staging, tile_chunks, source).and_then(|manifest| {
        swap_in(&staging, dir)?;
        Ok(manifest)
    });
    if swapped.is_err() {
        let _ = remove_cook(&staging);
    }
    swapped
}

/// Puts the finished cook at `dir`. An earlier cook there is moved aside
/// first and deleted only once the new one is in place, so a failure leaves
/// one whole cook, never neither.
fn swap_in(staging: &Path, dir: &Path) -> Result<(), CookError> {
    if !dir.exists() {
        return Ok(std::fs::rename(staging, dir)?);
    }
    let aside = sibling(dir, "cooked-old")?;
    remove_cook(&aside)?;
    std::fs::rename(dir, &aside)?;
    if let Err(err) = std::fs::rename(staging, dir) {
        let _ = std::fs::rename(&aside, dir);
        return Err(err.into());
    }
    // The new cook is in place; a leftover old one is cleared by the next.
    let _ = remove_cook(&aside);
    Ok(())
}

/// Held while cooking into `dir`, so two cooks don't share its staging
/// folder. The lock file stays, like the editor's map locks.
fn lock_cook(dir: &Path) -> Result<std::fs::File, CookError> {
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(sibling(dir, "cooking.lock")?)?;
    match lock.try_lock() {
        Ok(()) => Ok(lock),
        Err(std::fs::TryLockError::WouldBlock) => Err(CookError::Invalid(format!(
            "another cook is writing {}",
            dir.display()
        ))),
        Err(std::fs::TryLockError::Error(err)) => Err(err.into()),
    }
}

/// Deletes the cook in `dir`, by name only, and the folder if that emptied
/// it. Anything else in there is refused and left.
fn remove_cook(dir: &Path) -> Result<(), CookError> {
    let mut files = previous_cook(dir)?;
    // Manifest first: what's left mid-way never claims to be a whole cook.
    files.sort_by_key(|file| file.file_name() != Some(MANIFEST_FILE.as_ref()));
    for file in files {
        std::fs::remove_file(file)?;
    }
    for folder in [dir.join(TILE_DIR), dir.to_owned()] {
        match std::fs::remove_dir(&folder) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

/// The files of an earlier cook in `dir`, or an error if `dir` holds
/// anything else.
fn previous_cook(dir: &Path) -> Result<Vec<PathBuf>, CookError> {
    let refuse = || {
        CookError::Invalid(format!(
            "{} has other files in it; pick an empty folder or an earlier cook",
            dir.display()
        ))
    };
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name == MANIFEST_FILE {
            files.push(path);
        } else if name == TILE_DIR && path.is_dir() {
            for tile in std::fs::read_dir(&path)? {
                let tile = tile?.path();
                let tile_name = tile
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                if !(tile_name.starts_with("t_") && tile_name.ends_with(".lofitile")) {
                    return Err(refuse());
                }
                files.push(tile);
            }
        } else {
            return Err(refuse());
        }
    }
    Ok(files)
}

/// `.<name>.<suffix>` beside `dir`.
pub(super) fn sibling(dir: &Path, suffix: &str) -> Result<PathBuf, CookError> {
    let name = dir
        .file_name()
        .ok_or_else(|| CookError::Invalid(format!("{} isn't a folder name", dir.display())))?;
    let mut hidden = std::ffi::OsString::from(".");
    hidden.push(name);
    hidden.push(".");
    hidden.push(suffix);
    Ok(dir.with_file_name(hidden))
}

fn write_cook(
    map: &WorldMap,
    dir: &Path,
    tile_chunks: u32,
    source: CookSource,
) -> Result<Manifest, CookError> {
    let terrain = &map.terrain;
    let size = terrain.size();
    let layout = TileLayout::new(size, terrain.cell_size(), tile_chunks)
        .ok_or_else(|| CookError::Invalid(format!("tiles of {tile_chunks} chunks")))?;
    std::fs::create_dir_all(dir.join(TILE_DIR))?;
    let (tiles_x, tiles_z) = layout.tile_counts();
    let mut objects_by_tile = vec![Vec::new(); (tiles_x * tiles_z) as usize];
    for o in map.objects.iter() {
        let (tx, tz) = layout.tile_at(o.position.xz());
        objects_by_tile[(tz * tiles_x + tx) as usize].push(CookedObject {
            id: o.id.0,
            asset: [(o.asset.0 >> 64) as u64, o.asset.0 as u64],
            position: o.position.to_array(),
            yaw: o.yaw,
            scale: o.scale,
        });
    }
    let mut tiles = Vec::new();
    for ((tx, tz), objects) in layout.tiles().zip(objects_by_tile) {
        let chunks = layout
            .chunks_of(tx, tz)
            .map(|c| CookedChunk::cook(terrain, c))
            .collect::<Result<Vec<_>, _>>()?;
        let tile = CookedTile {
            x: tx,
            z: tz,
            cell_size: terrain.cell_size(),
            chunks,
            objects,
        };
        // Heights are clamped far inside what an i32 of steps holds.
        let (min_step, max_step) = tile.step_range().unwrap_or_default();
        let bytes = pack(TILE_MAGIC, &tile)?;
        let file = format!("{TILE_DIR}/t_{tx}_{tz}.lofitile");
        write_file(&dir.join(&file), &bytes)?;
        tiles.push(TileEntry {
            x: tx,
            z: tz,
            file,
            bytes: bytes.len() as u64,
            crc32: crc32fast::hash(&bytes),
            min_step: min_step as i32,
            max_step: max_step as i32,
        });
    }
    let manifest = Manifest {
        name: map.name.clone(),
        chunks_x: size.chunks_x(),
        chunks_z: size.chunks_z(),
        cell_size: terrain.cell_size(),
        tile_chunks,
        height_step: HEIGHT_STEP,
        materials: map
            .materials
            .iter()
            .map(|m| CookedMaterial {
                id: m.id.0,
                name: m.name.clone(),
                color: m.color,
                pattern: m.pattern.label().to_owned(),
                tile_size: m.tile_size,
            })
            .collect(),
        base_material: map.materials.base().0,
        water: CookedWater {
            enabled: map.water.enabled,
            level: map.water.level,
            shallow: map.water.shallow,
            deep: map.water.deep,
            depth: map.water.depth,
        },
        lighting: map.lighting.into(),
        tiles,
        source_revision: source.revision,
        source_unsaved: source.unsaved,
    };
    // Last, so a manifest only ever lists tiles already written.
    write_file(&dir.join(MANIFEST_FILE), &pack(WORLD_MAGIC, &manifest)?)?;
    Ok(manifest)
}
