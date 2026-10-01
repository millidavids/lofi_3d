//! What every cooked file shares: names, limits, errors and the header.

use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::file::codec;

/// Height quantum: 1 cm, the same everywhere, so chunk seams can't crack.
pub const HEIGHT_STEP: f32 = 0.01;
/// Tiles are this many chunks on a side unless asked otherwise: 256 m at
/// 2 m cells.
pub const DEFAULT_TILE_CHUNKS: u32 = 4;
pub const MAX_TILE_CHUNKS: u32 = 16;
pub const MANIFEST_FILE: &str = "world.lofiworld";
pub const TILE_DIR: &str = "tiles";
/// 2: the manifest says how tall each tile is.
pub const COOK_VERSION: u32 = 2;
/// The oldest reader that can read what this writes.
const MIN_READER: u32 = 2;
/// The oldest cook this reads. Older ones are cooked again, not migrated.
const OLDEST_READ: u32 = 2;
pub(super) const WORLD_MAGIC: [u8; 8] = *b"LOFIWRLD";
pub(super) const TILE_MAGIC: [u8; 8] = *b"LOFITILE";
/// Largest unpacked body read: a 16×16-chunk tile is about 15 MB.
const MAX_BODY: u64 = 256 << 20;

#[derive(Debug, thiserror::Error)]
pub enum CookError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("can't cook: {0}")]
    Invalid(String),
    #[error("not a cooked world file")]
    NotCooked,
    #[error("cooked by a newer version ({found}); this reads up to {supported}")]
    TooNew { found: u32, supported: u32 },
    #[error("cooked by an older version ({found}); cook the map again")]
    TooOld { found: u32 },
    #[error("damaged: {0}")]
    Corrupt(String),
}

pub(super) fn read_capped(path: &Path) -> Result<Vec<u8>, CookError> {
    let len = std::fs::metadata(path)?.len();
    if len > MAX_BODY {
        return Err(CookError::Corrupt(format!("{} is too big", path.display())));
    }
    Ok(std::fs::read(path)?)
}

/// Header, then the zstd'd MessagePack body.
pub(super) fn pack<T: Serialize>(magic: [u8; 8], value: &T) -> Result<Vec<u8>, CookError> {
    let raw = rmp_serde::to_vec_named(value).map_err(|e| CookError::Invalid(e.to_string()))?;
    let packed = codec::encode_plain(&raw);
    let mut bytes = Vec::with_capacity(28 + packed.len());
    bytes.extend_from_slice(&magic);
    bytes.extend_from_slice(&COOK_VERSION.to_le_bytes());
    bytes.extend_from_slice(&MIN_READER.to_le_bytes());
    bytes.extend_from_slice(&crc32fast::hash(&packed).to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&packed);
    Ok(bytes)
}

pub(super) fn unpack<T: DeserializeOwned>(magic: [u8; 8], bytes: &[u8]) -> Result<T, CookError> {
    let (found, rest) = bytes.split_first_chunk::<8>().ok_or(CookError::NotCooked)?;
    if *found != magic {
        return Err(CookError::NotCooked);
    }
    let truncated = || CookError::Corrupt("truncated header".to_owned());
    let (version, rest) = rest.split_first_chunk::<4>().ok_or_else(truncated)?;
    let (min_reader, rest) = rest.split_first_chunk::<4>().ok_or_else(truncated)?;
    let (crc, rest) = rest.split_first_chunk::<4>().ok_or_else(truncated)?;
    let (raw_len, packed) = rest.split_first_chunk::<8>().ok_or_else(truncated)?;
    let found = u32::from_le_bytes(*version);
    if u32::from_le_bytes(*min_reader) > COOK_VERSION {
        return Err(CookError::TooNew {
            found,
            supported: COOK_VERSION,
        });
    }
    if found < OLDEST_READ {
        return Err(CookError::TooOld { found });
    }
    if crc32fast::hash(packed) != u32::from_le_bytes(*crc) {
        return Err(CookError::Corrupt("failed its checksum".to_owned()));
    }
    let raw_len = u64::from_le_bytes(*raw_len);
    if raw_len > MAX_BODY {
        return Err(CookError::Corrupt(format!("claims {raw_len} bytes")));
    }
    let raw = codec::decode(codec::ZSTD, packed, raw_len as usize)
        .map_err(|e| CookError::Corrupt(e.to_string()))?;
    rmp_serde::from_slice(&raw).map_err(|e| CookError::Corrupt(e.to_string()))
}
