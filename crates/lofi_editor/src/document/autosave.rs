//! Periodic autosave of unsaved work to a sidecar file (see `sidecar`).
//!
//! Saving for real, or throwing the work away, deletes the sidecar. One left
//! behind means the editor died with unsaved work.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, block_on, poll_once};
use lofi_world::save_autosave;

use super::lock::FileLock;
use super::map_document::MapDocument;
use super::sidecar::{new_untitled_sidecar, remove_if_present, sidecar_for};
use super::status::StatusLine;

const INTERVAL: Duration = Duration::from_secs(120);

#[derive(Resource, Default)]
pub struct Autosave {
    /// Resolves to the revision it wrote.
    task: Option<Task<Result<u64, String>>>,
    since_last: Duration,
    /// Revision last written, so an idle editor doesn't rewrite the same state.
    written: Option<u64>,
    /// The sidecar holding the current map's unsaved work. Only this one is
    /// ever written or deleted: any other may be another session's work.
    owned: Option<PathBuf>,
    /// Held while `owned` is set, so other sessions' recovery passes it by.
    owned_lock: Option<FileLock>,
    /// A sidecar we refused to overwrite, so the warning shows once.
    blocked: Option<PathBuf>,
}

impl Autosave {
    /// Stops autosaving the current map and deletes its sidecar. Waits for a
    /// write in flight first: finishing after the delete would bring back
    /// work the user chose to throw away. Call when the map is saved or
    /// discarded.
    pub fn retire(&mut self) {
        if let Some(task) = self.task.take() {
            let _ = block_on(task);
        }
        if let Some(sidecar) = self.owned.take() {
            remove_if_present(&sidecar);
        }
        *self = Self::default();
    }

    /// Takes over a recovered sidecar as the current map's, with the lock
    /// taken when it was offered, if there was one.
    pub fn adopt(&mut self, sidecar: PathBuf, lock: Option<FileLock>) {
        self.retire();
        self.owned_lock = lock.or_else(|| FileLock::try_acquire(&sidecar).ok().flatten());
        self.owned = Some(sidecar);
    }

    /// Makes `sidecar` this session's. `false` if another session holds it.
    fn own(&mut self, sidecar: PathBuf) -> bool {
        if self.owned.as_ref() != Some(&sidecar) {
            match FileLock::try_acquire(&sidecar) {
                Ok(None) => return false,
                // A folder that can't hold lock files goes without.
                lock => self.owned_lock = lock.ok().flatten(),
            }
        }
        self.owned = Some(sidecar);
        true
    }

    /// Says once that `sidecar` belongs to someone else.
    fn block(&mut self, sidecar: PathBuf, status: &mut StatusLine) {
        if self.blocked.as_ref() != Some(&sidecar) {
            status.error(format!(
                "Autosave is off: {} holds unsaved work from another session. \
                 Reopen the map to recover or discard it.",
                sidecar.display()
            ));
            self.blocked = Some(sidecar);
        }
    }

    pub fn owned(&self) -> Option<&Path> {
        self.owned.as_deref()
    }
}

pub(super) fn run_autosave(
    time: Res<Time>,
    document: Res<MapDocument>,
    mut autosave: ResMut<Autosave>,
    mut status: ResMut<StatusLine>,
) {
    if let Some(task) = &mut autosave.task {
        let Some(result) = block_on(poll_once(task)) else {
            return;
        };
        autosave.task = None;
        match result {
            Ok(revision) => autosave.written = Some(revision),
            // `written` stays put, so the next interval tries again.
            Err(err) => status.error(format!("Autosave failed: {err}")),
        }
    }

    autosave.since_last += time.delta();
    if autosave.since_last < INTERVAL {
        return;
    }
    autosave.since_last = Duration::ZERO;
    if !document.is_dirty() || autosave.written == Some(document.revision()) {
        return;
    }
    let path = match (&autosave.owned, document.path()) {
        (Some(owned), _) => owned.clone(),
        (None, Some(map)) => sidecar_for(map),
        (None, None) => {
            let Some(path) = new_untitled_sidecar() else {
                return;
            };
            path
        }
    };
    let ours = autosave.owned.as_ref() == Some(&path);
    if !ours && (path.try_exists().unwrap_or(true) || !autosave.own(path.clone())) {
        autosave.block(path, &mut status);
        return;
    }

    // A copy-on-write snapshot: pointer copies, taken in microseconds.
    let map = document.map().clone();
    let stamp = document.autosave_stamp();
    autosave.task = Some(IoTaskPool::get().spawn(async move {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        save_autosave(&map, stamp, &path)
            .map(|()| stamp.revision)
            .map_err(|e| e.to_string())
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::sidecar::recovery_for_map;
    use lofi_world::{FileStamp, GridSize, WorldMap, save_map};

    /// Through real files: a crash leaves a sidecar, the next open finds it,
    /// retiring deletes only what this session owns.
    #[test]
    fn sidecar_lifecycle_on_disk() {
        let dir = std::env::temp_dir().join(format!("lofi_autosave_{}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("zone.lofimap");
        let sidecar = sidecar_for(&path);
        let map = WorldMap::flat("zone", GridSize::new(1, 1).expect("valid size"), 2.0, 0.0);
        let stamp = |revision, autosave_base| FileStamp {
            revision,
            autosave_base,
        };
        save_map(&map, stamp(5, None), &path).expect("saves");

        save_autosave(&map, stamp(9, Some(5)), &sidecar).expect("autosaves");
        let found = recovery_for_map(&path, 5).expect("offered");
        assert!(!found.file_changed);
        let found = recovery_for_map(&path, 7).expect("still offered");
        assert!(found.file_changed, "the file moved on since");

        // Not ours: retiring leaves it.
        Autosave::default().retire();
        assert!(sidecar.exists());

        // Adopted: retiring deletes it.
        let mut state = Autosave::default();
        state.adopt(sidecar.clone(), None);
        state.retire();
        assert!(!sidecar.exists());

        // Nothing newer than the file: cleaned up on sight.
        save_autosave(&map, stamp(5, Some(5)), &sidecar).expect("autosaves");
        assert!(recovery_for_map(&path, 5).is_none());
        assert!(!sidecar.exists());

        fs::remove_dir_all(&dir).expect("cleanup");
    }
}
