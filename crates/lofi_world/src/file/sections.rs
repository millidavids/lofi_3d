//! Named, packed, checksummed sections: the bulk of v2 and later files.
//!
//! Rules for growing the format without a version bump:
//! - New metadata fields get `#[serde(default)]`. Older editors drop metadata
//!   they don't know when they save, so tool-owned data goes in a section.
//! - New data goes in a new section. Mark it critical only if an editor that
//!   ignores it would corrupt the map; older editors refuse those, and keep
//!   every other unknown section byte-for-byte.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::codec;
use super::error::MapFileError;

#[derive(Serialize, Deserialize)]
pub(super) struct SectionRecord {
    pub tag: String,
    pub critical: bool,
    pub codec: String,
    pub raw_len: u64,
    /// Of the unpacked bytes. ruzstd writes no frame checksum of its own.
    pub crc32: u32,
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

/// A section from a newer tool, kept byte-for-byte so saving doesn't drop it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownSection {
    tag: String,
    codec: String,
    raw_len: u64,
    crc32: u32,
    data: Arc<[u8]>,
}

impl UnknownSection {
    #[must_use]
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// A known section this build can't use after all (say, a newer layout
    /// inside it), kept as optional data. Re-packed, same bytes inside.
    pub(super) fn from_raw(tag: &str, raw: &[u8]) -> Self {
        let record = packed_optional(tag, raw);
        Self {
            tag: record.tag,
            codec: record.codec,
            raw_len: record.raw_len,
            crc32: record.crc32,
            data: record.data.into(),
        }
    }

    /// A section as it was stored, unread.
    fn kept(section: SectionRecord) -> Self {
        Self {
            tag: section.tag,
            codec: section.codec,
            raw_len: section.raw_len,
            crc32: section.crc32,
            data: section.data.into(),
        }
    }

    pub(super) fn to_record(&self) -> SectionRecord {
        SectionRecord {
            tag: self.tag.clone(),
            critical: false,
            codec: self.codec.clone(),
            raw_len: self.raw_len,
            crc32: self.crc32,
            data: self.data.to_vec(),
        }
    }
}

/// Packs `raw` (a multiple of 4 bytes) as a critical section.
pub(super) fn packed(tag: &str, raw: &[u8]) -> SectionRecord {
    SectionRecord {
        tag: tag.to_owned(),
        critical: true,
        codec: codec::SHUFFLE4_ZSTD.to_owned(),
        raw_len: raw.len() as u64,
        crc32: crc32fast::hash(raw),
        data: codec::encode(raw),
    }
}

/// Packs byte-sized data, whatever its length, as a critical section. Plain
/// zstd: there's nothing to shuffle.
pub(super) fn packed_bytes(tag: &str, raw: &[u8]) -> SectionRecord {
    SectionRecord {
        tag: tag.to_owned(),
        critical: true,
        codec: codec::ZSTD.to_owned(),
        raw_len: raw.len() as u64,
        crc32: crc32fast::hash(raw),
        data: codec::encode_plain(raw),
    }
}

/// Packs byte data as an optional section: a reader that doesn't know it
/// keeps it byte-for-byte rather than refusing the file.
pub(super) fn packed_optional(tag: &str, raw: &[u8]) -> SectionRecord {
    SectionRecord {
        critical: false,
        ..packed_bytes(tag, raw)
    }
}

/// What size a known section's unpacked bytes may be, checked before
/// anything is unpacked.
#[derive(Clone, Copy, Debug)]
pub(super) enum SectionSize {
    Exact(usize),
    /// Anything in `min..=max`; the reader checks the layout itself.
    Between(usize, usize),
}

impl SectionSize {
    fn allows(self, raw_len: u64) -> bool {
        match self {
            Self::Exact(n) => raw_len == n as u64,
            Self::Between(lo, hi) => (lo as u64..=hi as u64).contains(&raw_len),
        }
    }
}

/// Sections read back: the known ones unpacked, the rest kept.
pub(super) struct Unpacked {
    known: BTreeMap<&'static str, Vec<u8>>,
    pub unknown: Vec<UnknownSection>,
}

impl Unpacked {
    /// `known` names each tag this reader understands and the exact size its
    /// unpacked bytes must have, checked before anything is unpacked.
    pub fn read(
        sections: Vec<SectionRecord>,
        known: &[(&'static str, usize)],
    ) -> Result<Self, MapFileError> {
        let sized: Vec<_> = known
            .iter()
            .map(|&(tag, n)| (tag, SectionSize::Exact(n)))
            .collect();
        Self::read_sized(sections, &sized)
    }

    /// Like [`Self::read`], for sections whose size follows a rule.
    pub fn read_sized(
        sections: Vec<SectionRecord>,
        known: &[(&'static str, SectionSize)],
    ) -> Result<Self, MapFileError> {
        let mut out = Self {
            known: BTreeMap::new(),
            unknown: Vec::new(),
        };
        let mut seen = Vec::new();
        for section in sections {
            let Some(&(tag, size)) = known.iter().find(|(t, _)| *t == section.tag) else {
                if section.critical {
                    return Err(MapFileError::UnknownCritical(section.tag));
                }
                out.unknown.push(UnknownSection::kept(section));
                continue;
            };
            if seen.contains(&tag) {
                return Err(corrupt(format!("two \"{tag}\" sections")));
            }
            seen.push(tag);
            // An optional section packed in a way this build doesn't know is
            // a newer tool's: keep it as it is rather than refuse the map.
            if !section.critical
                && (!codec::is_known(&section.codec) || !size.allows(section.raw_len))
            {
                out.unknown.push(UnknownSection::kept(section));
                continue;
            }
            if !size.allows(section.raw_len) {
                return Err(corrupt(format!(
                    "\"{tag}\" holds {} bytes, expected {size:?}",
                    section.raw_len
                )));
            }
            let raw = codec::decode(&section.codec, &section.data, section.raw_len as usize)?;
            if crc32fast::hash(&raw) != section.crc32 {
                return Err(corrupt(format!("\"{tag}\" failed its checksum")));
            }
            out.known.insert(tag, raw);
        }
        Ok(out)
    }

    /// A known section's bytes; corrupt if the file lacks it.
    pub fn take(&mut self, tag: &str) -> Result<Vec<u8>, MapFileError> {
        self.take_optional(tag)
            .ok_or_else(|| corrupt(format!("no \"{tag}\" section")))
    }

    /// A known section's bytes, if the file has it.
    pub fn take_optional(&mut self, tag: &str) -> Option<Vec<u8>> {
        self.known.remove(tag)
    }
}

pub(super) fn corrupt(what: String) -> MapFileError {
    MapFileError::Corrupt(what)
}

/// Little-endian `f32`s.
pub(super) fn f32s(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect()
}
