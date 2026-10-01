//! The file header and version dispatch.
//!
//! Header: 8-byte magic, then little-endian `u32` format version. From
//! version 2 on, two more `u32`s follow:
//! - the minimum reader version: a newer writer that stays readable by older
//!   builds keeps it low, and a build refuses files whose minimum is above
//!   what it supports;
//! - a crc32 of the body, so damaged metadata can't load as a valid map.

use super::error::MapFileError;
use super::{v1, v2, v3};
use crate::map::WorldMap;

pub const MAP_FILE_EXTENSION: &str = "lofimap";
pub const MAP_FORMAT_VERSION: u32 = 3;
const MIN_READER_VERSION: u32 = 3;
const MAGIC: [u8; 8] = *b"LOFIMAP\0";

/// Editing bookkeeping stored alongside a map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileStamp {
    /// Counts edits. Lets an autosave prove it is newer than the file it came from.
    pub revision: u64,
    /// Set only in autosaves: the revision of the file they were taken from.
    pub autosave_base: Option<u64>,
}

#[derive(Debug)]
pub struct LoadedMap {
    pub map: WorldMap,
    pub stamp: FileStamp,
    /// The version the file was written in.
    pub version: u32,
}

/// Always writes the current version.
pub fn encode_map(map: &WorldMap, stamp: FileStamp) -> Result<Vec<u8>, MapFileError> {
    let body = v3::encode(map, stamp)?;
    let mut bytes = Vec::with_capacity(20 + body.len());
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&MAP_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&MIN_READER_VERSION.to_le_bytes());
    bytes.extend_from_slice(&crc32fast::hash(&body).to_le_bytes());
    bytes.extend_from_slice(&body);
    Ok(bytes)
}

/// Reads any version this build knows. Never panics on bad input.
pub fn decode_map(bytes: &[u8]) -> Result<LoadedMap, MapFileError> {
    let (magic, rest) = bytes
        .split_first_chunk::<8>()
        .ok_or(MapFileError::NotAMap)?;
    if *magic != MAGIC {
        return Err(MapFileError::NotAMap);
    }
    let (version, rest) = split_u32(rest).ok_or(MapFileError::NotAMap)?;
    if version == 0 {
        return Err(MapFileError::NotAMap);
    }
    if version == 1 {
        return v1::decode(rest);
    }
    let truncated = || MapFileError::Corrupt("truncated header".to_owned());
    let (min_reader, rest) = split_u32(rest).ok_or_else(truncated)?;
    if min_reader > MAP_FORMAT_VERSION {
        return Err(MapFileError::TooNew {
            found: version,
            supported: MAP_FORMAT_VERSION,
        });
    }
    let (crc, body) = split_u32(rest).ok_or_else(truncated)?;
    if crc32fast::hash(body) != crc {
        return Err(MapFileError::Corrupt("failed its checksum".to_owned()));
    }
    match version {
        2 => v2::decode(body),
        // Newer versions that still let us read them use the v3 layout.
        _ => v3::decode(body, version),
    }
}

fn split_u32(bytes: &[u8]) -> Option<(u32, &[u8])> {
    let (value, rest) = bytes.split_first_chunk::<4>()?;
    Some((u32::from_le_bytes(*value), rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::GridSize;

    fn map() -> WorldMap {
        WorldMap::flat("t", GridSize::new(1, 1).expect("valid size"), 2.0, 0.0)
    }

    #[test]
    fn rejects_other_files() {
        assert!(matches!(
            decode_map(b"PNG\x89 not a map at all"),
            Err(MapFileError::NotAMap)
        ));
        assert!(matches!(decode_map(&[]), Err(MapFileError::NotAMap)));
    }

    #[test]
    fn newer_files_load_unless_they_say_otherwise() {
        let mut bytes = encode_map(&map(), FileStamp::default()).expect("encodes");
        bytes[8..12].copy_from_slice(&(MAP_FORMAT_VERSION + 1).to_le_bytes());
        let loaded = decode_map(&bytes).expect("still readable");
        assert_eq!(loaded.version, MAP_FORMAT_VERSION + 1);

        bytes[12..16].copy_from_slice(&(MAP_FORMAT_VERSION + 1).to_le_bytes());
        assert!(matches!(
            decode_map(&bytes),
            Err(MapFileError::TooNew { .. })
        ));
    }

    #[test]
    fn damaged_metadata_fails_the_checksum() {
        let mut bytes = encode_map(&map(), FileStamp::default()).expect("encodes");
        // The map's name, "t", is the first one-letter string in the body.
        let name = 20
            + bytes[20..]
                .windows(2)
                .position(|w| w == b"\xa1t")
                .expect("name is in the body");
        bytes[name + 1] = b'u';
        assert!(matches!(decode_map(&bytes), Err(MapFileError::Corrupt(_))));
    }
}
