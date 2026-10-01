//! Reading a heightmap, checking sizes against the file before trusting them.

use std::io::{BufReader, Cursor};
use std::path::Path;

use super::format::{
    HeightmapError, HeightmapFormat, HeightmapInfo, MAX_FILE_BYTES, MAX_SIDECAR_BYTES,
    PNG_RANGE_KEY, check_size, sidecar_path,
};
use super::samples::{Heightmap, Units};

/// Reads a heightmap and its sidecar, if there is one.
///
/// Raw files take their size from the sidecar, or are assumed square.
///
/// # Errors
/// If the file is unreadable, too big, or not a heightmap this reads.
pub fn read_heightmap(path: &Path) -> Result<(Heightmap, Option<HeightmapInfo>), HeightmapError> {
    let format = HeightmapFormat::from_path(path).ok_or_else(|| {
        HeightmapError::Invalid("expected a .png, .r16, .raw or .r32 file".to_owned())
    })?;
    let info = read_sidecar(path)?;
    let len = std::fs::metadata(path)?.len();
    if len > MAX_FILE_BYTES {
        return Err(HeightmapError::Invalid(format!("{len} bytes is too big")));
    }
    if let Some(info) = info
        && format != HeightmapFormat::Png16
    {
        // Check the sidecar's size against the file before reading it.
        check_size(info.width, info.height)?;
        let per_sample: u64 = if format == HeightmapFormat::R32 { 4 } else { 2 };
        let expected = u64::from(info.width) * u64::from(info.height) * per_sample;
        if expected != len {
            return Err(HeightmapError::Invalid(format!(
                "{len} bytes, but the sidecar says {}×{} ({expected} bytes)",
                info.width, info.height
            )));
        }
    }
    let bytes = std::fs::read(path)?;
    match format {
        HeightmapFormat::Png16 => {
            let (map, range) = decode_png(&bytes)?;
            // The sidecar wins; the PNG's own note is the fallback.
            let info = info.or_else(|| {
                range.map(|(min, max, cell_size)| HeightmapInfo {
                    format,
                    width: map.width,
                    height: map.height,
                    min,
                    max,
                    cell_size,
                })
            });
            Ok((map, info))
        }
        HeightmapFormat::R16 | HeightmapFormat::R32 => {
            Ok((decode_raw(&bytes, format, info)?, info))
        }
    }
}

fn read_sidecar(path: &Path) -> Result<Option<HeightmapInfo>, HeightmapError> {
    let side = sidecar_path(path);
    let len = match std::fs::metadata(&side) {
        Ok(meta) => meta.len(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if len > MAX_SIDECAR_BYTES {
        return Err(HeightmapError::Invalid(format!(
            "{} is {len} bytes; a sidecar is a few lines",
            side.display()
        )));
    }
    let json = std::fs::read(&side)?;
    let info: HeightmapInfo = serde_json::from_slice(&json)
        .map_err(|e| HeightmapError::Invalid(format!("{}: {e}", side.display())))?;
    if !(info.min.is_finite() && info.max.is_finite() && info.cell_size.is_finite()) {
        return Err(HeightmapError::Invalid(format!(
            "{}: range isn't finite",
            side.display()
        )));
    }
    Ok(Some(info))
}

fn decode_raw(
    bytes: &[u8],
    format: HeightmapFormat,
    info: Option<HeightmapInfo>,
) -> Result<Heightmap, HeightmapError> {
    let per_sample = if format == HeightmapFormat::R32 { 4 } else { 2 };
    let count = bytes.len() / per_sample;
    if !bytes.len().is_multiple_of(per_sample) {
        return Err(HeightmapError::Invalid(format!(
            "{} bytes isn't a whole number of samples",
            bytes.len()
        )));
    }
    let (width, height) = match info {
        Some(info) => (info.width, info.height),
        None => {
            let side = (count as f64).sqrt().round() as u32;
            if (side as usize) * (side as usize) != count {
                return Err(HeightmapError::Invalid(
                    "not square, and there's no .json sidecar giving its size".to_owned(),
                ));
            }
            (side, side)
        }
    };
    check_size(width, height)?;
    if width as usize * height as usize != count {
        return Err(HeightmapError::Invalid(format!(
            "{count} samples, but the sidecar says {width}×{height}"
        )));
    }
    let (samples, units) = if format == HeightmapFormat::R32 {
        let metres: Vec<f32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        // Past this, resampling arithmetic overflows; nothing real is that tall.
        if metres.iter().any(|h| !h.is_finite() || h.abs() > 1e6) {
            return Err(HeightmapError::Invalid(
                "heights are NaN, infinite or past a million metres".to_owned(),
            ));
        }
        (metres, Units::Metres)
    } else {
        let unit = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(u16::from_le_bytes(*b)) / 65535.0)
            .collect();
        (unit, Units::Normalized)
    };
    Ok(Heightmap {
        width,
        height,
        samples,
        units,
    })
}

/// The image, and the range from its text chunk if it has one.
type DecodedPng = (Heightmap, Option<(f32, f32, f32)>);

fn decode_png(bytes: &[u8]) -> Result<DecodedPng, HeightmapError> {
    let png_error = |e: png::DecodingError| HeightmapError::Png(e.to_string());
    let limits = png::Limits {
        bytes: MAX_FILE_BYTES as usize,
    };
    let mut decoder = png::Decoder::new_with_limits(BufReader::new(Cursor::new(bytes)), limits);
    // Palettes and sub-byte greys become plain bytes; 16-bit stays 16-bit.
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(png_error)?;
    let (width, height) = (reader.info().width, reader.info().height);
    check_size(width, height)?;
    let range = reader
        .info()
        .uncompressed_latin1_text
        .iter()
        .find(|chunk| chunk.keyword == PNG_RANGE_KEY)
        .and_then(|chunk| parse_range(&chunk.text));
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| HeightmapError::Png("image too large".to_owned()))?;
    let mut buf = vec![0; size];
    let frame = reader.next_frame(&mut buf).map_err(png_error)?;
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        other => {
            return Err(HeightmapError::Invalid(format!(
                "{other:?} PNG; heightmaps are greyscale"
            )));
        }
    };
    let wide = frame.bit_depth == png::BitDepth::Sixteen;
    let sample_bytes = if wide { 2 } else { 1 };
    let mut samples = Vec::with_capacity(width as usize * height as usize);
    for row in buf.chunks(frame.line_size).take(height as usize) {
        for x in 0..width as usize {
            let at = x * channels * sample_bytes;
            let v = if wide {
                f32::from(u16::from_be_bytes([row[at], row[at + 1]])) / 65535.0
            } else {
                f32::from(row[at]) / 255.0
            };
            samples.push(v);
        }
    }
    let map = Heightmap {
        width,
        height,
        samples,
        units: Units::Normalized,
    };
    Ok((map, range))
}

/// "min max cell_size", all finite, or nothing.
fn parse_range(text: &str) -> Option<(f32, f32, f32)> {
    let numbers: Vec<f32> = text
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let [min, max, cell] = numbers[..] else {
        return None;
    };
    [min, max, cell]
        .iter()
        .all(|v| v.is_finite())
        .then_some((min, max, cell))
}
