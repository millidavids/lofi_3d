//! Reading a cooked world back, trusting nothing in it.

use std::path::{Component, Path, PathBuf};

use super::chunk::CookedChunk;
use super::format::{CookError, MANIFEST_FILE, TILE_MAGIC, WORLD_MAGIC, read_capped, unpack};
use super::ground::CookedGround;
use super::layout::TileLayout;
use super::manifest::{Manifest, TileEntry};
use super::tile::{CookedObject, CookedTile};

/// A cooked directory, opened.
#[derive(Debug)]
pub struct CookedWorld {
    dir: PathBuf,
    pub manifest: Manifest,
    layout: TileLayout,
}

impl CookedWorld {
    /// Reads the manifest.
    ///
    /// # Errors
    /// If it's missing, damaged or from a newer cooker.
    pub fn open(dir: &Path) -> Result<Self, CookError> {
        let bytes = read_capped(&dir.join(MANIFEST_FILE))?;
        let manifest: Manifest = unpack(WORLD_MAGIC, &bytes)?;
        let layout = manifest.layout()?;
        // Tiles are only ever read from inside the cooked folder.
        let inside = |file: &str| {
            Path::new(file)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        };
        let sound = |t: &TileEntry| inside(&t.file) && t.min_step <= t.max_step;
        if !manifest.tiles.iter().all(sound) {
            return Err(CookError::Corrupt("bad manifest".to_owned()));
        }
        Ok(Self {
            dir: dir.to_owned(),
            manifest,
            layout,
        })
    }

    #[must_use]
    pub const fn layout(&self) -> TileLayout {
        self.layout
    }

    /// Empty ground for this world, to load tiles into.
    #[must_use]
    pub fn ground(&self) -> CookedGround {
        CookedGround::new(self.layout)
    }

    /// Loads one tile, checking it against the manifest.
    ///
    /// # Errors
    /// If the tile isn't listed, or its file is missing or damaged.
    pub fn load_tile(&self, x: u32, z: u32) -> Result<CookedTile, CookError> {
        let entry = self
            .manifest
            .tiles
            .iter()
            .find(|t| t.x == x && t.z == z)
            .ok_or_else(|| CookError::Invalid(format!("no tile {x}, {z}")))?;
        let bytes = read_capped(&self.dir.join(&entry.file))?;
        if bytes.len() as u64 != entry.bytes || crc32fast::hash(&bytes) != entry.crc32 {
            return Err(CookError::Corrupt(format!(
                "{} doesn't match the manifest",
                entry.file
            )));
        }
        let tile: CookedTile = unpack(TILE_MAGIC, &bytes)?;
        let expected = self.layout.chunks_of(x, z);
        // Exactly the tile's chunks, in order: none missing, doubled or stray.
        if (tile.x, tile.z) != (x, z)
            || tile.cell_size != self.manifest.cell_size
            || !tile.chunks.iter().map(CookedChunk::coord).eq(expected)
        {
            return Err(CookError::Corrupt(format!(
                "{} is another tile",
                entry.file
            )));
        }
        // A manifest that understates a tile's height would cull real ground.
        let listed = (i64::from(entry.min_step), i64::from(entry.max_step));
        if tile.step_range() != Some(listed) {
            return Err(CookError::Corrupt(format!(
                "{} isn't as tall as the manifest says",
                entry.file
            )));
        }
        if !tile.objects.iter().all(CookedObject::is_sound) {
            return Err(CookError::Corrupt(format!(
                "{} has a misplaced object",
                entry.file
            )));
        }
        Ok(tile)
    }
}
