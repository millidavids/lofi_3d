//! Heightmap import and export. Export writes straight away; import reads
//! the file, then waits on a prompt for the height range and, if the sizes
//! differ, how to fit it.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use lofi_world::{
    HEIGHT_LIMIT, Heightmap, HeightmapFormat, HeightmapInfo, Units, export_heightmap,
    read_heightmap,
};

use super::history::EditHistory;
use super::map_document::{MapDocument, TerrainChanges};
use super::status::StatusLine;

/// How a heightmap of another size meets the map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fit {
    /// Stretched or shrunk to cover the whole map.
    #[default]
    Resample,
    /// Placed at the map's first corner, cut off or edge-extended.
    CropOrPad,
}

/// A heightmap read and waiting on the import prompt.
#[derive(Clone, Debug)]
pub struct ImportDraft {
    pub path: PathBuf,
    pub map: Heightmap,
    pub info: Option<HeightmapInfo>,
    /// Metres for sample 0 and for the top sample, when the file has no
    /// metres of its own.
    pub min: f32,
    pub max: f32,
    pub fit: Fit,
    pub flip: bool,
    /// The map this was meant for; another map cancels it.
    generation: u64,
}

impl ImportDraft {
    /// Whether the user must say what the samples mean in metres.
    pub fn needs_range(&self) -> bool {
        self.map.units == Units::Normalized
    }

    /// What's wrong with the range as entered, if anything.
    pub fn range_problem(&self) -> Option<&'static str> {
        if !self.needs_range() {
            return None;
        }
        if !self.min.is_finite() || !self.max.is_finite() {
            return Some("Heights must be numbers.");
        }
        if self.min >= self.max {
            return Some("The lowest height must be below the highest.");
        }
        if self.min < -HEIGHT_LIMIT || self.max > HEIGHT_LIMIT {
            return Some("Heights must stay within ±4096 m.");
        }
        None
    }
}

#[derive(Resource, Default)]
pub struct PendingImport(pub Option<ImportDraft>);

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportMessage {
    Apply,
    Cancel,
}

/// Writes the map's heights; reports in the status line.
pub(super) fn export_to(
    document: &MapDocument,
    format: HeightmapFormat,
    path: &Path,
    status: &mut StatusLine,
) {
    let adjusted = with_extension(path, format);
    // The save dialog asked about overwriting the name as typed, not this one.
    if adjusted != path && adjusted.exists() {
        status.error(format!(
            "{} already exists; pick it in the dialog to replace it.",
            adjusted.display()
        ));
        return;
    }
    let path = adjusted;
    match export_heightmap(document.terrain(), format, &path) {
        Ok(info) => status.info(format!(
            "Exported {} ({}×{}, {:.1} to {:.1} m)",
            path.display(),
            info.width,
            info.height,
            info.min,
            info.max
        )),
        Err(err) => status.error(format!("Could not export {}: {err}", path.display())),
    }
}

/// Reads a heightmap and puts it up for the import prompt.
pub(super) fn read_for_import(
    document: &MapDocument,
    path: PathBuf,
    pending: &mut PendingImport,
    status: &mut StatusLine,
) {
    match read_heightmap(&path) {
        Ok((map, info)) => {
            // A plausible range to start from when the file doesn't say.
            let (min, max) = info.map_or((0.0, 100.0), |i| (i.min, i.max));
            pending.0 = Some(ImportDraft {
                path,
                map,
                info,
                min,
                max,
                fit: Fit::default(),
                flip: false,
                generation: document.generation(),
            });
        }
        Err(err) => status.error(format!("Could not import {}: {err}", path.display())),
    }
}

pub(super) fn handle_import(
    mut messages: MessageReader<ImportMessage>,
    mut pending: ResMut<PendingImport>,
    mut document: ResMut<MapDocument>,
    mut history: ResMut<EditHistory>,
    mut changes: ResMut<TerrainChanges>,
    mut status: ResMut<StatusLine>,
) {
    if pending
        .0
        .as_ref()
        .is_some_and(|draft| draft.generation != document.generation())
    {
        pending.0 = None;
        status.info("Import cancelled: another map was opened.");
    }
    for message in messages.read() {
        let Some(draft) = pending.0.take() else {
            continue;
        };
        if *message == ImportMessage::Cancel {
            continue;
        }
        if let Some(problem) = draft.range_problem() {
            status.error(problem);
            pending.0 = Some(draft);
            continue;
        }
        if history.is_recording() {
            status.error("Finish the stroke first.");
            pending.0 = Some(draft);
            continue;
        }
        apply(
            &draft,
            &mut document,
            &mut history,
            &mut changes,
            &mut status,
        );
    }
}

fn apply(
    draft: &ImportDraft,
    document: &mut MapDocument,
    history: &mut EditHistory,
    changes: &mut TerrainChanges,
    status: &mut StatusLine,
) {
    let size = document.terrain().size();
    let (width, height) = (size.verts_x(), size.verts_z());
    let mut map = draft.map.clone().into_metres(draft.min, draft.max);
    if draft.flip {
        map = map.flipped();
    }
    if (map.width, map.height) != (width, height) {
        map = match draft.fit {
            Fit::Resample => map.resample(width, height),
            Fit::CropOrPad => map.crop_or_pad(width, height),
        };
    }
    let clamped = map
        .samples
        .iter()
        .filter(|h| h.abs() > HEIGHT_LIMIT)
        .count();

    let full = size.full_rect();
    history.begin("Import heightmap");
    history.capture(document.terrain(), full);
    document.edit_terrain(|terrain| {
        let mut changed = false;
        for ((x, z), &h) in full.iter().zip(&map.samples) {
            changed |= terrain.set_height(x, z, h);
        }
        changed
    });
    history.commit(document.map());
    changes.mark_heights(full);

    let name = draft.path.file_name().map_or_else(
        || draft.path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    if clamped > 0 {
        status.error(format!(
            "Imported {name}; {clamped} heights were past ±4096 m and were clamped."
        ));
    } else {
        status.info(format!("Imported {name}"));
    }
}

/// Adds the format's extension unless the name already has it.
fn with_extension(path: &Path, format: HeightmapFormat) -> PathBuf {
    if HeightmapFormat::from_path(path) == Some(format) {
        return path.to_owned();
    }
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(format.extension());
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use lofi_world::{GridSize, WorldMap};

    use super::*;

    fn draft(map: Heightmap, generation: u64) -> ImportDraft {
        ImportDraft {
            path: "h.png".into(),
            map,
            info: None,
            min: -10.0,
            max: 30.0,
            fit: Fit::Resample,
            flip: false,
            generation,
        }
    }

    fn ramp(side: u32) -> Heightmap {
        Heightmap {
            width: side,
            height: side,
            samples: (0..side * side)
                .map(|i| (i % side) as f32 / (side - 1) as f32)
                .collect(),
            units: Units::Normalized,
        }
    }

    #[test]
    fn importing_maps_the_range_and_undoes() {
        let mut document = MapDocument::new(WorldMap::flat(
            "t",
            GridSize::new(1, 1).expect("valid size"),
            2.0,
            0.0,
        ));
        let mut history = EditHistory::default();
        let mut changes = TerrainChanges::default();
        let mut status = StatusLine::default();
        // 65 samples onto 33 vertices: resampled.
        let d = draft(ramp(65), document.generation());
        apply(&d, &mut document, &mut history, &mut changes, &mut status);
        let t = document.terrain();
        assert!((t.height(0, 5) + 10.0).abs() < 1e-3);
        assert!((t.height(32, 5) - 30.0).abs() < 1e-3);
        assert!(changes.take().heights.is_some());
        assert_eq!(history.undo_label(), Some("Import heightmap"));
        history.undo(document.map_mut());
        assert_eq!(document.terrain().height(32, 5), 0.0);
    }

    #[test]
    fn bad_ranges_are_caught_before_importing() {
        let mut d = draft(ramp(3), 0);
        assert_eq!(d.range_problem(), None);
        d.max = d.min;
        assert!(d.range_problem().is_some());
        d.max = f32::NAN;
        assert!(d.range_problem().is_some());
        d.max = 1e6;
        assert!(d.range_problem().is_some());
    }

    #[test]
    fn extensions_are_added_once() {
        assert_eq!(
            with_extension(Path::new("a/b"), HeightmapFormat::R16),
            PathBuf::from("a/b.r16")
        );
        assert_eq!(
            with_extension(Path::new("a/b.PNG"), HeightmapFormat::Png16),
            PathBuf::from("a/b.PNG")
        );
    }
}
