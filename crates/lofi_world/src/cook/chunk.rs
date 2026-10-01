//! One chunk as the game reads it: quantised heights, baked triangulation
//! and flags, normals, paint and light, all self-contained. `bake` makes
//! one from the terrain; `lookup` reads one.

use serde::{Deserialize, Serialize};

use super::CookError;
use crate::grid::CHUNK_CELLS;
use crate::paint::CHUNK_PAINT_VERTICES;

/// Vertices per chunk side.
pub(super) const SIDE: u32 = CHUNK_CELLS + 1;
pub(super) const VERTICES: usize = CHUNK_PAINT_VERTICES;
pub(super) const CELL_BITS: usize = (CHUNK_CELLS * CHUNK_CELLS) as usize / 8;
pub(super) const EMPTY_SLOT: u16 = u16::MAX;

/// A cooked chunk, as stored. Every vertex of its 33×33 rect is here, so a
/// chunk renders and collides without its neighbours.
///
/// Its blobs' sizes are checked as it's read, however it's read, so the
/// accessors can't index past them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Unchecked")]
pub struct CookedChunk {
    pub x: u32,
    pub z: u32,
    /// The lowest height, in [`super::HEIGHT_STEP`]s.
    pub(super) base: i32,
    /// Little-endian steps above `base` per vertex: `u16`, or `u32` when
    /// `wide`. One global step, so a vertex two chunks share quantises the
    /// same in both.
    #[serde(with = "serde_bytes")]
    pub(super) heights: Vec<u8>,
    /// The chunk spans more than 655.35 m, so heights take four bytes.
    #[serde(default)]
    pub(super) wide: bool,
    /// One bit per cell, row-major, least significant bit first: cut along
    /// (0, 0)–(1, 1). The other bitsets are laid out the same.
    #[serde(with = "serde_bytes")]
    pub(super) split: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub(super) hole: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub(super) no_clutter: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub(super) unwalkable: Vec<u8>,
    /// Two `i8` per vertex, octahedral.
    #[serde(with = "serde_bytes")]
    pub(super) normals: Vec<u8>,
    /// Material ids, `0xFFFF` for none.
    pub slots: [u16; 4],
    /// Four weights per vertex, in `slots` order.
    #[serde(with = "serde_bytes")]
    pub(super) weights: Vec<u8>,
    /// Tint r, g, b (128 neutral) and baked AO (255 open sky) per vertex.
    #[serde(with = "serde_bytes")]
    pub(super) light: Vec<u8>,
}

/// A chunk as it comes off disk: [`CookedChunk`]'s fields, sizes unchecked.
#[derive(Deserialize)]
struct Unchecked {
    x: u32,
    z: u32,
    base: i32,
    #[serde(with = "serde_bytes")]
    heights: Vec<u8>,
    #[serde(default)]
    wide: bool,
    #[serde(with = "serde_bytes")]
    split: Vec<u8>,
    #[serde(with = "serde_bytes")]
    hole: Vec<u8>,
    #[serde(with = "serde_bytes")]
    no_clutter: Vec<u8>,
    #[serde(with = "serde_bytes")]
    unwalkable: Vec<u8>,
    #[serde(with = "serde_bytes")]
    normals: Vec<u8>,
    slots: [u16; 4],
    #[serde(with = "serde_bytes")]
    weights: Vec<u8>,
    #[serde(with = "serde_bytes")]
    light: Vec<u8>,
}

impl TryFrom<Unchecked> for CookedChunk {
    type Error = CookError;

    fn try_from(read: Unchecked) -> Result<Self, CookError> {
        let Unchecked {
            x,
            z,
            base,
            heights,
            wide,
            split,
            hole,
            no_clutter,
            unwalkable,
            normals,
            slots,
            weights,
            light,
        } = read;
        let chunk = Self {
            x,
            z,
            base,
            heights,
            wide,
            split,
            hole,
            no_clutter,
            unwalkable,
            normals,
            slots,
            weights,
            light,
        };
        chunk.validate()?;
        Ok(chunk)
    }
}

impl CookedChunk {
    /// Checks every blob's size, so lookups can't go out of bounds.
    fn validate(&self) -> Result<(), CookError> {
        let sizes = [
            (
                "heights",
                self.heights.len(),
                VERTICES * if self.wide { 4 } else { 2 },
            ),
            ("split", self.split.len(), CELL_BITS),
            ("hole", self.hole.len(), CELL_BITS),
            ("no_clutter", self.no_clutter.len(), CELL_BITS),
            ("unwalkable", self.unwalkable.len(), CELL_BITS),
            ("normals", self.normals.len(), VERTICES * 2),
            ("weights", self.weights.len(), VERTICES * 4),
            ("light", self.light.len(), VERTICES * 4),
        ];
        for (what, found, expected) in sizes {
            if found != expected {
                return Err(CookError::Corrupt(format!(
                    "chunk {}, {}: {what} is {found} bytes, expected {expected}",
                    self.x, self.z
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bevy_math::Vec2;

    use super::*;

    /// A chunk as someone else might write it, blob sizes theirs to choose.
    #[derive(Serialize)]
    struct Forged {
        x: u32,
        z: u32,
        base: i32,
        #[serde(with = "serde_bytes")]
        heights: Vec<u8>,
        wide: bool,
        #[serde(with = "serde_bytes")]
        split: Vec<u8>,
        #[serde(with = "serde_bytes")]
        hole: Vec<u8>,
        #[serde(with = "serde_bytes")]
        no_clutter: Vec<u8>,
        #[serde(with = "serde_bytes")]
        unwalkable: Vec<u8>,
        #[serde(with = "serde_bytes")]
        normals: Vec<u8>,
        slots: [u16; 4],
        #[serde(with = "serde_bytes")]
        weights: Vec<u8>,
        #[serde(with = "serde_bytes")]
        light: Vec<u8>,
    }

    fn forged() -> Forged {
        Forged {
            x: 0,
            z: 0,
            base: 0,
            heights: vec![0; VERTICES * 2],
            wide: false,
            split: vec![0; CELL_BITS],
            hole: vec![0; CELL_BITS],
            no_clutter: vec![0; CELL_BITS],
            unwalkable: vec![0; CELL_BITS],
            normals: vec![0; VERTICES * 2],
            slots: [0, EMPTY_SLOT, EMPTY_SLOT, EMPTY_SLOT],
            weights: vec![0; VERTICES * 4],
            light: vec![0; VERTICES * 4],
        }
    }

    fn read(chunk: &Forged) -> Result<CookedChunk, rmp_serde::decode::Error> {
        rmp_serde::from_slice(&rmp_serde::to_vec_named(chunk).expect("encodes"))
    }

    #[test]
    fn a_chunk_is_checked_as_it_is_read_however_it_is_read() {
        let good = read(&forged()).expect("the right sizes read");
        assert_eq!(good.height_at_cells(Vec2::splat(31.9)), Some(0.0));

        // Each would send an accessor past the end of its blob.
        let mut short_hole = forged();
        short_hole.hole.truncate(3);
        assert!(read(&short_hole).is_err());
        let mut narrow = forged();
        narrow.wide = true;
        assert!(read(&narrow).is_err(), "wide, with two bytes a height");
        let mut long = forged();
        long.light.push(0);
        assert!(read(&long).is_err());
    }
}
