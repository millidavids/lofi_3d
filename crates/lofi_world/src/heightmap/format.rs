//! The file formats, the sidecar, and the limits on what's read.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Largest side read: twice the biggest map (2049 vertices across), so a
/// source can still be downsampled to fit.
pub const MAX_SIDE: u32 = 4097;
/// Largest file read, checked before reading it.
pub(super) const MAX_FILE_BYTES: u64 = MAX_SIDE as u64 * MAX_SIDE as u64 * 4 + (1 << 20);
/// A sidecar is a few lines of JSON.
pub(super) const MAX_SIDECAR_BYTES: u64 = 64 * 1024;
/// PNG text chunk holding "min max cell_size".
pub(super) const PNG_RANGE_KEY: &str = "lofi-range";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeightmapFormat {
    Png16,
    R16,
    R32,
}

impl HeightmapFormat {
    pub const ALL: [Self; 3] = [Self::Png16, Self::R16, Self::R32];

    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png16 => "png",
            Self::R16 => "r16",
            Self::R32 => "r32",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Png16 => "16-bit PNG",
            Self::R16 => "R16 (raw 16-bit)",
            Self::R32 => "R32 (raw float, lossless)",
        }
    }

    /// By file extension, ignoring case. `.raw` is taken as R16.
    #[must_use]
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "png" => Some(Self::Png16),
            "r16" | "raw" => Some(Self::R16),
            "r32" => Some(Self::R32),
            _ => None,
        }
    }
}

/// The sidecar: everything needed to turn the pixels back into metres.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HeightmapInfo {
    pub format: HeightmapFormat,
    pub width: u32,
    pub height: u32,
    /// Metres at sample 0 and at 65535 (PNG, R16). Informational for R32.
    pub min: f32,
    pub max: f32,
    /// Metres between samples in the map it came from.
    pub cell_size: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum HeightmapError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("not a heightmap: {0}")]
    Invalid(String),
    #[error("PNG: {0}")]
    Png(String),
}

/// `<file>.json`.
#[must_use]
pub fn sidecar_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".json");
    PathBuf::from(name)
}

pub(super) fn check_size(width: u32, height: u32) -> Result<(), HeightmapError> {
    if width < 2 || height < 2 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(HeightmapError::Invalid(format!(
            "{width}×{height} is outside 2..={MAX_SIDE} on a side"
        )));
    }
    Ok(())
}
