//! Where autosaves live, and which ones hold work worth offering back.
//!
//! A saved map autosaves next to itself as `<file>.autosave`. Untitled maps
//! autosave to the app-data directory under a name unique to the session,
//! so two editors never write the same file.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use lofi_world::{FileStamp, LoadedMap, MAP_FILE_EXTENSION, load_map};

use super::lock::FileLock;

const SUFFIX: &str = ".autosave";
const UNTITLED_PREFIX: &str = "untitled-";
const SWEEP_AFTER: Duration = Duration::from_secs(600);

/// An autosave worth offering back.
pub struct Found {
    pub loaded: LoadedMap,
    pub sidecar: PathBuf,
    /// The map on disk changed after the autosave was taken, so recovering
    /// replaces those changes.
    pub file_changed: bool,
    /// Held from the moment an untitled autosave is offered, so no other
    /// session offers it too. A map's autosave is covered by the map's lock.
    pub lock: Option<FileLock>,
}

pub fn sidecar_for(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(SUFFIX);
    name.into()
}

/// A new sidecar name for an untitled map: time, process and a counter.
pub fn new_untitled_sidecar() -> Option<PathBuf> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    Some(untitled_dir()?.join(format!(
        "{UNTITLED_PREFIX}{secs}-{pid}-{n}.{MAP_FILE_EXTENSION}{SUFFIX}"
    )))
}

/// The autosave beside the map at `path`, if it has work the file doesn't.
/// One with nothing newer is deleted; anything else is left for the user.
pub fn recovery_for_map(path: &Path, file_revision: u64) -> Option<Found> {
    let sidecar = sidecar_for(path);
    let loaded = read(&sidecar)?;
    match classify(loaded.stamp, file_revision) {
        Verdict::Offer { file_changed } => Some(Found {
            loaded,
            sidecar,
            file_changed,
            lock: None,
        }),
        Verdict::Stale => {
            remove_if_present(&sidecar);
            None
        }
        Verdict::NotAnAutosave => None,
    }
}

/// The newest untitled autosave another session left behind. `except` is
/// this session's own; ones a running session still holds are passed by.
pub fn recovery_for_untitled(except: Option<&Path>) -> Option<Found> {
    recovery_in(&untitled_dir()?, except)
}

fn recovery_in(dir: &Path, except: Option<&Path>) -> Option<Found> {
    let entries: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    sweep_locks(&entries);
    let mut candidates: Vec<(SystemTime, PathBuf)> = entries
        .into_iter()
        .filter(|path| is_untitled_sidecar(path) && Some(path.as_path()) != except)
        .filter_map(|path| Some((fs::metadata(&path).ok()?.modified().ok()?, path)))
        .collect();
    // Newest first.
    candidates.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    candidates.into_iter().find_map(|(_, sidecar)| {
        // Taken now and kept while it's on offer. A folder that can't hold
        // lock files goes without.
        let lock = match FileLock::try_acquire(&sidecar) {
            Ok(Some(lock)) => Some(lock),
            Ok(None) => return None,
            Err(_) => None,
        };
        let loaded = read(&sidecar)?;
        loaded.stamp.autosave_base.is_some().then_some(Found {
            loaded,
            sidecar,
            file_changed: false,
            lock,
        })
    })
}

/// Deletes lock files whose autosave is long gone, so they don't pile up:
/// one is left by every session that autosaved untitled work.
fn sweep_locks(entries: &[PathBuf]) {
    for lock in entries {
        let Some(target) = FileLock::target_of(lock) else {
            continue;
        };
        // A session makes its lock file a moment before it locks it and
        // first writes its autosave; leave young ones alone.
        let old = fs::metadata(lock)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > SWEEP_AFTER);
        if is_untitled_sidecar(&target)
            && !target.exists()
            && old
            && let Ok(Some(_held)) = FileLock::try_acquire(&target)
        {
            remove_if_present(lock);
        }
    }
}

pub fn remove_if_present(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => warn!("Could not remove {}: {err}", path.display()),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Offer {
        file_changed: bool,
    },
    /// Holds nothing the file doesn't.
    Stale,
    /// A map file that happens to sit where a sidecar would. Not ours.
    NotAnAutosave,
}

fn classify(autosave: FileStamp, file_revision: u64) -> Verdict {
    match autosave.autosave_base {
        None => Verdict::NotAnAutosave,
        Some(base) if base != file_revision => Verdict::Offer { file_changed: true },
        Some(_) if autosave.revision > file_revision => Verdict::Offer {
            file_changed: false,
        },
        Some(_) => Verdict::Stale,
    }
}

/// `None` when missing or unreadable. Unreadable ones are left alone: a newer
/// build may have written them.
fn read(sidecar: &Path) -> Option<LoadedMap> {
    if !sidecar.try_exists().unwrap_or(false) {
        return None;
    }
    load_map(sidecar)
        .inspect_err(|err| warn!("Ignoring autosave {}: {err}", sidecar.display()))
        .ok()
}

fn untitled_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("lofi_3d").join("autosave"))
}

fn is_untitled_sidecar(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(UNTITLED_PREFIX) && name.ends_with(SUFFIX))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn autosave(revision: u64, base: u64) -> FileStamp {
        FileStamp {
            revision,
            autosave_base: Some(base),
        }
    }

    #[test]
    fn what_gets_offered() {
        let offer = |file_changed| Verdict::Offer { file_changed };
        assert_eq!(classify(autosave(9, 5), 5), offer(false));
        assert_eq!(classify(autosave(9, 5), 7), offer(true), "file moved on");
        assert_eq!(classify(autosave(5, 5), 5), Verdict::Stale);
        let saved = FileStamp {
            revision: 9,
            autosave_base: None,
        };
        assert_eq!(classify(saved, 5), Verdict::NotAnAutosave);
    }

    #[test]
    fn an_untitled_autosave_is_offered_to_one_session_at_a_time() {
        use lofi_world::{GridSize, WorldMap, save_autosave};

        let dir = std::env::temp_dir().join(format!("lofi_untitled_{}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        let sidecar = dir.join("untitled-1-2-3.lofimap.autosave");
        let map = WorldMap::flat("lost", GridSize::new(1, 1).expect("valid size"), 2.0, 0.0);
        save_autosave(&map, autosave(4, 0), &sidecar).expect("autosaves");

        let first = recovery_in(&dir, None).expect("offered");
        assert_eq!(first.sidecar, sidecar);
        assert!(first.lock.is_some());
        assert!(recovery_in(&dir, None).is_none(), "on offer elsewhere");
        drop(first);
        assert!(recovery_in(&dir, None).is_some(), "declined: free again");
        assert!(recovery_in(&dir, Some(&sidecar)).is_none(), "our own");
        fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn only_stale_locks_without_an_autosave_are_swept() {
        let dir = std::env::temp_dir().join(format!("lofi_sweep_{}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        let lock_for = |name: &str, age: Duration| {
            let lock = dir.join(format!(".{name}.lock"));
            let file = fs::File::create(&lock).expect("create");
            file.set_modified(SystemTime::now() - age).expect("touch");
            lock
        };
        let hour = Duration::from_secs(3600);
        let stale = lock_for("untitled-1-1-0.lofimap.autosave", hour);
        let young = lock_for("untitled-2-2-0.lofimap.autosave", Duration::ZERO);
        let in_use = lock_for("untitled-3-3-0.lofimap.autosave", hour);
        fs::write(dir.join("untitled-3-3-0.lofimap.autosave"), b"work").expect("write");
        let held = lock_for("untitled-4-4-0.lofimap.autosave", hour);
        let _session = FileLock::try_acquire(&dir.join("untitled-4-4-0.lofimap.autosave"))
            .expect("io")
            .expect("free");
        let a_maps = lock_for("zone.lofimap", hour);

        let entries: Vec<PathBuf> = fs::read_dir(&dir)
            .expect("reads")
            .map(|e| e.expect("entry").path())
            .collect();
        sweep_locks(&entries);
        assert!(!stale.exists());
        for kept in [young, in_use, held, a_maps] {
            assert!(kept.exists(), "{}", kept.display());
        }
        fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn names() {
        assert_eq!(
            sidecar_for(Path::new("maps/zone.lofimap")),
            PathBuf::from("maps/zone.lofimap.autosave")
        );
        if let (Some(a), Some(b)) = (new_untitled_sidecar(), new_untitled_sidecar()) {
            assert_ne!(a, b);
            assert!(is_untitled_sidecar(&a));
        }
        assert!(!is_untitled_sidecar(Path::new("zone.lofimap.autosave")));
    }
}
