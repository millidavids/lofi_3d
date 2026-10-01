//! How section bytes are packed.
//!
//! The writer always byte-shuffles then zstd-compresses. Shuffling groups the
//! nth byte of every 4-byte value together, so the slow-changing high bytes of
//! heights and the mostly-zero paint channels compress far better.

use std::io::Read;

use ruzstd::decoding::StreamingDecoder;
use ruzstd::encoding::{CompressionLevel, compress_to_vec};

use super::error::MapFileError;

pub(super) const RAW: &str = "raw";
pub(crate) const ZSTD: &str = "zstd";
pub(super) const SHUFFLE4_ZSTD: &str = "shuffle4-zstd";

/// Packs with [`SHUFFLE4_ZSTD`]. `raw.len()` must be a multiple of 4.
pub(super) fn encode(raw: &[u8]) -> Vec<u8> {
    debug_assert!(raw.len().is_multiple_of(4));
    // ruzstd only implements its fastest level; the others are `unimplemented!`.
    compress_to_vec(shuffle4(raw).as_slice(), CompressionLevel::Fastest)
}

/// Packs with [`ZSTD`], for data with no 4-byte structure.
pub(crate) fn encode_plain(raw: &[u8]) -> Vec<u8> {
    compress_to_vec(raw, CompressionLevel::Fastest)
}

pub(super) fn is_known(codec: &str) -> bool {
    matches!(codec, RAW | ZSTD | SHUFFLE4_ZSTD)
}

/// Unpacks into exactly `raw_len` bytes, reading no more than that from the
/// decompressor, so a damaged file can't make it allocate without bound.
pub(crate) fn decode(codec: &str, data: &[u8], raw_len: usize) -> Result<Vec<u8>, MapFileError> {
    let corrupt = |what: String| MapFileError::Corrupt(what);
    let raw = match codec {
        RAW => data.to_vec(),
        ZSTD | SHUFFLE4_ZSTD => {
            let mut decoder =
                StreamingDecoder::new(data).map_err(|e| corrupt(format!("zstd: {e}")))?;
            let mut out = Vec::with_capacity(raw_len);
            (&mut decoder)
                .take(raw_len as u64 + 1)
                .read_to_end(&mut out)
                .map_err(|e| corrupt(format!("zstd: {e}")))?;
            out
        }
        other => return Err(corrupt(format!("unknown codec \"{other}\""))),
    };
    if raw.len() != raw_len {
        return Err(corrupt(format!(
            "unpacked to {} bytes, expected {raw_len}",
            raw.len()
        )));
    }
    if codec == SHUFFLE4_ZSTD {
        if !raw_len.is_multiple_of(4) {
            return Err(corrupt(format!("{raw_len} bytes can't be shuffled by 4")));
        }
        return Ok(unshuffle4(&raw));
    }
    Ok(raw)
}

fn shuffle4(raw: &[u8]) -> Vec<u8> {
    let n = raw.len() / 4;
    let mut out = vec![0; n * 4];
    for (i, value) in raw.as_chunks::<4>().0.iter().enumerate() {
        for (b, &byte) in value.iter().enumerate() {
            out[b * n + i] = byte;
        }
    }
    out
}

fn unshuffle4(shuffled: &[u8]) -> Vec<u8> {
    let n = shuffled.len() / 4;
    let mut out = vec![0; n * 4];
    for (i, value) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        for (b, byte) in value.iter_mut().enumerate() {
            *byte = shuffled[b * n + i];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        (0..4000u32)
            .flat_map(|i| (i as f32 * 0.1).to_le_bytes())
            .collect()
    }

    #[test]
    fn round_trips_and_compresses() {
        let raw = sample();
        let packed = encode(&raw);
        assert!(packed.len() < raw.len());
        assert_eq!(
            decode(SHUFFLE4_ZSTD, &packed, raw.len()).expect("decodes"),
            raw
        );
    }

    #[test]
    fn shuffle_is_invertible() {
        let raw = sample();
        assert_eq!(unshuffle4(&shuffle4(&raw)), raw);
    }

    #[test]
    fn wrong_sizes_are_corrupt_not_panics() {
        let raw = sample();
        let packed = encode(&raw);
        assert!(decode(SHUFFLE4_ZSTD, &packed, raw.len() - 4).is_err());
        assert!(decode(SHUFFLE4_ZSTD, &packed, raw.len() + 4).is_err());
        assert!(decode(SHUFFLE4_ZSTD, &packed[..packed.len() / 2], raw.len()).is_err());
        assert!(decode(RAW, &raw, raw.len() + 1).is_err());
        assert!(decode("lz9", &raw, raw.len()).is_err());
    }
}
