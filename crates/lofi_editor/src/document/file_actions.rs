//! New, open, save and quit, with a guard against losing unsaved work.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use lofi_world::{
    FileStamp, GridSize, HeightmapFormat, MAP_FILE_EXTENSION, MAP_FORMAT_VERSION, MAX_CELL_SIZE,
    MIN_CELL_SIZE, WorldMap, load_map, save_map,
};

use super::autosave::Autosave;
use super::file_dialog::FileDialog;
use super::history::EditHistory;
use super::lock::{MapClaim, resolved};
use super::map_document::MapDocument;
use super::recovery::PendingRecovery;
use super::sidecar::{recovery_for_map, recovery_for_untitled};
use super::status::StatusLine;

/// New maps are capped below the format's limit. Past this, flat-shaded CPU
/// meshes get too heavy; see docs/ROADMAP.md.
pub const NEW_MAP_MAX_CHUNKS: u32 = 32;

#[derive(Clone, Debug)]
pub enum FileAction {
    New(NewMapSettings),
    /// Pick a file to open.
    Open,
    /// Open a file already picked. Comes from the Open dialog when the map
    /// changed while the dialog was up.
    OpenFile(PathBuf),
    Save,
    SaveAs,
    /// Pick a heightmap to replace the map's heights (undoably).
    ImportHeightmap,
    /// Pick where to write the map's heights.
    ExportHeightmap(HeightmapFormat),
    /// Pick a folder to write the game's cooked world into.
    Cook,
    Quit,
}

impl FileAction {
    /// Whether this throws away the current map.
    const fn discards(&self) -> bool {
        matches!(
            self,
            Self::New(_) | Self::Open | Self::OpenFile(_) | Self::Quit
        )
    }
}

#[derive(Message, Clone, Debug)]
pub struct FileActionMessage {
    pub action: FileAction,
    /// The user already agreed to lose unsaved changes.
    pub discard_confirmed: bool,
}

impl FileActionMessage {
    pub const fn new(action: FileAction) -> Self {
        Self {
            action,
            discard_confirmed: false,
        }
    }

    pub const fn confirmed(action: FileAction) -> Self {
        Self {
            action,
            discard_confirmed: true,
        }
    }
}

/// An action waiting on "discard unsaved changes?".
#[derive(Resource, Default)]
pub struct PendingDiscard(pub Option<FileAction>);

#[derive(Clone, Debug, PartialEq)]
pub struct NewMapSettings {
    pub name: String,
    pub chunks_x: u32,
    pub chunks_z: u32,
    pub cell_size: f32,
    pub base_height: f32,
}

impl Default for NewMapSettings {
    fn default() -> Self {
        Self {
            name: "Untitled".to_owned(),
            chunks_x: 8,
            chunks_z: 8,
            cell_size: 2.0,
            base_height: 4.0,
        }
    }
}

impl NewMapSettings {
    pub fn build(&self) -> WorldMap {
        let clamp = |c: u32| c.clamp(1, NEW_MAP_MAX_CHUNKS);
        let size = GridSize::new(clamp(self.chunks_x), clamp(self.chunks_z))
            .expect("clamped into the valid range");
        let name = if self.name.trim().is_empty() {
            "Untitled"
        } else {
            self.name.trim()
        };
        WorldMap::flat(
            name,
            size,
            self.cell_size.clamp(MIN_CELL_SIZE, MAX_CELL_SIZE),
            self.base_height,
        )
    }
}

pub(super) fn handle_file_actions(
    mut messages: MessageReader<FileActionMessage>,
    mut document: ResMut<MapDocument>,
    mut history: ResMut<EditHistory>,
    mut pending: ResMut<PendingDiscard>,
    mut recovery: ResMut<PendingRecovery>,
    mut dialog: ResMut<FileDialog>,
    mut autosave: ResMut<Autosave>,
    mut status: ResMut<StatusLine>,
    mut exit: MessageWriter<AppExit>,
) {
    for message in messages.read() {
        let action = &message.action;
        if action.discards() && document.is_dirty() && !message.discard_confirmed {
            pending.0 = Some(action.clone());
            continue;
        }
        match action {
            FileAction::New(settings) => {
                autosave.retire();
                recovery.clear();
                document.replace(settings.build(), None, FileStamp::default());
                history.clear();
                status.info(format!("New map \"{}\"", document.map().name));
                // Starting untitled work is when earlier untitled work matters.
                if let Some(found) = recovery_for_untitled(autosave.owned()) {
                    recovery.offer(found, None);
                }
            }
            // The current map is only dropped once a file is picked and loads.
            FileAction::Open => dialog.open(start_dir(&document), document.revision()),
            FileAction::OpenFile(path) => open_document(
                &mut document,
                &mut history,
                &mut autosave,
                &mut recovery,
                path.clone(),
                &mut status,
            ),
            FileAction::Save => match document.path() {
                Some(path) => {
                    let path = path.to_owned();
                    save_document(
                        &mut document,
                        &mut history,
                        &mut autosave,
                        path,
                        &mut status,
                    );
                }
                None => dialog.save_as(start_dir(&document), suggested_file_name(&document)),
            },
            FileAction::SaveAs => {
                dialog.save_as(start_dir(&document), suggested_file_name(&document));
            }
            FileAction::ImportHeightmap => dialog.pick_heightmap(start_dir(&document)),
            FileAction::Cook => dialog.pick_cook_folder(start_dir(&document)),
            FileAction::ExportHeightmap(format) => dialog.export_heightmap(
                start_dir(&document),
                format!("{}.{}", suggested_stem(&document), format.extension()),
                *format,
            ),
            FileAction::Quit => {
                autosave.retire();
                exit.write(AppExit::Success);
            }
        }
    }
}

pub fn save_document(
    document: &mut MapDocument,
    history: &mut EditHistory,
    autosave: &mut Autosave,
    path: PathBuf,
    status: &mut StatusLine,
) {
    let path = resolved(&with_map_extension(path));
    // Saving over our own file: refuse if something else changed it since.
    let ours = document.claim().is_some_and(|c| c.path() == path);
    if ours && document.claim().is_some_and(MapClaim::changed_on_disk) {
        status.error(format!(
            "{} changed on disk since it was opened. Save As to keep both.",
            path.display()
        ));
        return;
    }
    // Saving to another file: it mustn't be open in another editor.
    let new_claim = match ours {
        true => None,
        false => match claim(&path) {
            Claim::Taken => {
                status.error(format!(
                    "{} is open in another lofi editor.",
                    path.display()
                ));
                return;
            }
            Claim::Got(claim) => Some(claim),
            Claim::Unlockable => None,
        },
    };
    match save_map(document.map(), document.stamp(), &path) {
        Ok(report) => {
            match report.backup_error {
                None => status.info(format!("Saved {}", path.display())),
                Some(err) => status.error(format!(
                    "Saved {}, but couldn't update its .bak: {err}",
                    path.display()
                )),
            }
            // The work is on disk now; its autosave is obsolete.
            autosave.retire();
            // So undo can always step back to exactly what was saved.
            history.seal();
            document.mark_saved(path);
            if let Some(claim) = new_claim {
                document.set_claim(claim);
            }
            if let Some(claim) = document.claim_mut() {
                claim.note_written();
            }
        }
        Err(err) => status.error(format!("Could not save {}: {err}", path.display())),
    }
}

enum Claim {
    Got(MapClaim),
    /// Another session has it.
    Taken,
    /// No lock file could be made there (say, a read-only folder): go on
    /// without one.
    Unlockable,
}

fn claim(path: &Path) -> Claim {
    match MapClaim::acquire(path) {
        Ok(Some(claim)) => Claim::Got(claim),
        Ok(None) => Claim::Taken,
        Err(err) => {
            warn!("No lock beside {}: {err}", path.display());
            Claim::Unlockable
        }
    }
}

/// Loads `path` in place of the current map, and offers back any unsaved
/// work an autosave kept for it.
pub fn open_document(
    document: &mut MapDocument,
    history: &mut EditHistory,
    autosave: &mut Autosave,
    recovery: &mut PendingRecovery,
    path: PathBuf,
    status: &mut StatusLine,
) {
    let path = resolved(&path);
    // Reopening our own file keeps the lock we hold; anything else needs one.
    let reopening = document.claim().is_some_and(|c| c.path() == path);
    let new_claim = if reopening {
        None
    } else {
        match claim(&path) {
            Claim::Taken => {
                status.error(format!(
                    "{} is open in another lofi editor.",
                    path.display()
                ));
                return;
            }
            Claim::Got(claim) => Some(claim),
            Claim::Unlockable => None,
        }
    };
    let loaded = match load_map(&path) {
        Ok(loaded) => loaded,
        Err(err) => {
            status.error(format!("Could not open {}: {err}", path.display()));
            return;
        }
    };
    if loaded.version < MAP_FORMAT_VERSION {
        status.info(format!(
            "Opened {} (format v{}; saving converts it to v{MAP_FORMAT_VERSION})",
            path.display(),
            loaded.version
        ));
    } else {
        status.info(format!("Opened {}", path.display()));
    }
    autosave.retire();
    recovery.clear();
    if let Some(found) = recovery_for_map(&path, loaded.stamp.revision) {
        recovery.offer(found, Some(path.clone()));
    }
    document.replace(loaded.map, Some(path), loaded.stamp);
    if let Some(claim) = new_claim {
        document.set_claim(claim);
    } else if let Some(claim) = document.claim_mut() {
        claim.note_written();
    }
    history.clear();
}

/// Appends `.lofimap` unless the name already ends with it.
fn with_map_extension(path: PathBuf) -> PathBuf {
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(MAP_FILE_EXTENSION))
    {
        return path;
    }
    let mut name = path.into_os_string();
    name.push(".");
    name.push(MAP_FILE_EXTENSION);
    PathBuf::from(name)
}

fn start_dir(document: &MapDocument) -> Option<PathBuf> {
    document.path().and_then(|p| p.parent()).map(PathBuf::from)
}

fn suggested_file_name(document: &MapDocument) -> String {
    format!("{}.{MAP_FILE_EXTENSION}", suggested_stem(document))
}

/// The map's name made safe for a file name.
fn suggested_stem(document: &MapDocument) -> String {
    document
        .map()
        .name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file-action system and what it touches, on a map with unsaved edits.
    fn dirty_app() -> App {
        let mut app = App::new();
        let mut document = MapDocument::new(NewMapSettings::default().build());
        document.map_mut().name = "edited".to_owned();
        app.insert_resource(document)
            .init_resource::<EditHistory>()
            .init_resource::<PendingDiscard>()
            .init_resource::<PendingRecovery>()
            .init_resource::<FileDialog>()
            .init_resource::<Autosave>()
            .init_resource::<StatusLine>()
            .add_message::<FileActionMessage>()
            .add_message::<AppExit>()
            .add_systems(Update, handle_file_actions);
        app
    }

    #[test]
    fn opening_a_picked_file_over_unsaved_work_asks_first() {
        let mut app = dirty_app();
        let path = PathBuf::from("/nonexistent/zone.lofimap");
        app.world_mut()
            .write_message(FileActionMessage::new(FileAction::OpenFile(path)));
        app.update();

        let pending = &app.world().resource::<PendingDiscard>().0;
        assert!(matches!(pending, Some(FileAction::OpenFile(_))));
        assert_eq!(app.world().resource::<MapDocument>().map().name, "edited");
    }

    #[test]
    fn confirmed_open_replaces_the_map() {
        let dir = std::env::temp_dir().join(format!("lofi_open_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("zone.lofimap");
        let map = NewMapSettings {
            name: "Zone".to_owned(),
            ..NewMapSettings::default()
        }
        .build();
        save_map(&map, FileStamp::default(), &path).expect("saves");

        let mut app = dirty_app();
        app.world_mut()
            .write_message(FileActionMessage::confirmed(FileAction::OpenFile(
                path.clone(),
            )));
        app.update();

        let document = app.world().resource::<MapDocument>();
        assert_eq!(document.map().name, "Zone");
        assert_eq!(document.path(), Some(resolved(&path).as_path()));
        assert!(!document.is_dirty());
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn our_own_map_under_another_spelling_is_still_ours() {
        let dir = std::env::temp_dir().join(format!("lofi_spelt_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).expect("temp dir");
        let path = dir.join("zone.lofimap");
        save_map(
            &NewMapSettings::default().build(),
            FileStamp::default(),
            &path,
        )
        .expect("saves");

        let mut document = MapDocument::new(NewMapSettings::default().build());
        let mut history = EditHistory::default();
        let mut autosave = Autosave::default();
        let mut recovery = PendingRecovery::default();
        let mut status = StatusLine::default();
        for spelling in [dir.join("sub/../zone.lofimap"), path.clone()] {
            open_document(
                &mut document,
                &mut history,
                &mut autosave,
                &mut recovery,
                spelling,
                &mut status,
            );
            assert!(status.text().starts_with("Opened"), "{}", status.text());
        }

        // Saving to it by yet another spelling is saving our own file.
        document.map_mut().name = "Renamed".to_owned();
        save_document(
            &mut document,
            &mut history,
            &mut autosave,
            dir.join("./zone.lofimap"),
            &mut status,
        );
        assert!(status.text().starts_with("Saved"), "{}", status.text());
        assert_eq!(document.path(), Some(resolved(&path).as_path()));
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn a_map_open_in_another_session_is_refused() {
        let dir = std::env::temp_dir().join(format!("lofi_locked_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("zone.lofimap");
        save_map(
            &NewMapSettings::default().build(),
            FileStamp::default(),
            &path,
        )
        .expect("saves");
        // Another editor has it.
        let other = MapClaim::acquire(&path).expect("io").expect("free");

        let mut app = dirty_app();
        app.world_mut()
            .write_message(FileActionMessage::confirmed(FileAction::OpenFile(
                path.clone(),
            )));
        app.update();
        let document = app.world().resource::<MapDocument>();
        assert_eq!(document.map().name, "edited", "kept the current map");
        assert!(
            app.world()
                .resource::<StatusLine>()
                .text()
                .contains("another lofi editor")
        );

        // Once it's let go, it opens, and this session holds it.
        drop(other);
        app.world_mut()
            .write_message(FileActionMessage::confirmed(FileAction::OpenFile(
                path.clone(),
            )));
        app.update();
        let document = app.world().resource::<MapDocument>();
        assert_eq!(document.path(), Some(resolved(&path).as_path()));
        assert!(MapClaim::acquire(&path).expect("io").is_none(), "held now");
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn map_extension_is_added_once() {
        assert_eq!(
            with_map_extension("a/zone".into()),
            PathBuf::from("a/zone.lofimap")
        );
        assert_eq!(
            with_map_extension("zone.v2".into()),
            PathBuf::from("zone.v2.lofimap")
        );
        assert_eq!(
            with_map_extension("zone.LOFIMAP".into()),
            PathBuf::from("zone.LOFIMAP")
        );
    }

    #[test]
    fn new_map_settings_are_clamped() {
        let map = NewMapSettings {
            name: "  ".to_owned(),
            chunks_x: 0,
            chunks_z: 999,
            cell_size: 100.0,
            base_height: 1.0,
        }
        .build();
        assert_eq!(map.name, "Untitled");
        assert_eq!(map.terrain.size().chunks_x(), 1);
        assert_eq!(map.terrain.size().chunks_z(), NEW_MAP_MAX_CHUNKS);
        assert_eq!(map.terrain.cell_size(), MAX_CELL_SIZE);
    }
}
