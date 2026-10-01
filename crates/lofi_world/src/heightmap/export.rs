//! Writing the terrain's heights out, with the sidecar.

use std::path::Path;

use super::format::{HeightmapError, HeightmapFormat, HeightmapInfo, PNG_RANGE_KEY, sidecar_path};
use crate::file::write_file;
use crate::terrain::Terrain;

/// Writes the terrain's heights and the sidecar.
///
/// # Errors
/// If either file can't be written.
pub fn export_heightmap(
    terrain: &Terrain,
    format: HeightmapFormat,
    path: &Path,
) -> Result<HeightmapInfo, HeightmapError> {
    let size = terrain.size();
    let (width, height) = (size.verts_x(), size.verts_z());
    let heights = terrain.heights_row_major();
    let (low, high) = terrain.height_range();
    // A flat map still needs a range to divide by.
    let max = if high - low > 1e-3 { high } else { low + 1.0 };
    let info = HeightmapInfo {
        format,
        width,
        height,
        min: low,
        max,
        cell_size: terrain.cell_size(),
    };
    let to_u16 = |h: f32| {
        ((h - low) / (max - low) * 65535.0)
            .round()
            .clamp(0.0, 65535.0) as u16
    };
    let bytes = match format {
        HeightmapFormat::R32 => heights.iter().flat_map(|h| h.to_le_bytes()).collect(),
        HeightmapFormat::R16 => heights
            .iter()
            .flat_map(|&h| to_u16(h).to_le_bytes())
            .collect(),
        HeightmapFormat::Png16 => {
            // PNG samples are big-endian.
            let data: Vec<u8> = heights
                .iter()
                .flat_map(|&h| to_u16(h).to_be_bytes())
                .collect();
            encode_png(&info, &data)?
        }
    };
    let json =
        serde_json::to_vec_pretty(&info).map_err(|e| HeightmapError::Invalid(e.to_string()))?;
    // An old sidecar beside a new image would give the wrong range if the
    // new sidecar failed to write, so it goes first.
    let side = sidecar_path(path);
    match std::fs::remove_file(&side) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    write_file(path, &bytes)?;
    write_file(&side, &json)?;
    Ok(info)
}

fn encode_png(info: &HeightmapInfo, data: &[u8]) -> Result<Vec<u8>, HeightmapError> {
    let png_error = |e: png::EncodingError| HeightmapError::Png(e.to_string());
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, info.width, info.height);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Sixteen);
    encoder
        .add_text_chunk(
            PNG_RANGE_KEY.to_owned(),
            format!("{} {} {}", info.min, info.max, info.cell_size),
        )
        .map_err(png_error)?;
    let mut writer = encoder.write_header().map_err(png_error)?;
    writer.write_image_data(data).map_err(png_error)?;
    writer.finish().map_err(png_error)?;
    Ok(out)
}
