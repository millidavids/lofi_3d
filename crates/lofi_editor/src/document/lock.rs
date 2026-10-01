//! One editor per file at a time, enforced: a session holds an exclusive OS
//! lock on a small `.<name>.lock` file beside each map or autosave it owns.
//! The OS drops the lock if the editor dies, so a crash never leaves a map
//! claimed. Lock files are left in place: deleting one could let two
//! sessions lock different files for the same map.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// One spelling per file: absolute, links followed. The same map reached by
/// another route is then still ours, and its lock sits beside the real file.
pub fn resolved(path: &Path) -> PathBuf {
    if let Ok(real) = path.canonicalize() {
        return real;
    }
    // Not there yet (Save As): resolve the folder, keep the name.
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    let folder = absolute.parent().and_then(|dir| dir.canonicalize().ok());
    match (folder, absolute.file_name()) {
        (Some(folder), Some(name)) => folder.join(name),
        _ => absolute,
    }
}

/// Held until dropped.
#[derive(Debug)]
pub struct FileLock {
    _file: File,
    target: PathBuf,
}

impl FileLock {
    /// `.<name>.lock` beside `target`.
    fn lock_path(target: &Path) -> PathBuf {
        let mut name = std::ffi::OsString::from(".");
        name.push(target.file_name().unwrap_or_default());
        name.push(".lock");
        target.with_file_name(name)
    }

    /// The lock on `target`, or `None` if another session holds it.
    ///
    /// # Errors
    /// If the lock file can't be created, e.g. a read-only folder.
    pub fn try_acquire(target: &Path) -> io::Result<Option<Self>> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(Self::lock_path(target))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self {
                _file: file,
                target: target.to_owned(),
            })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(e)) => Err(e),
        }
    }

    pub fn target(&self) -> &Path {
        &self.target
    }

    /// The file a lock file guards, if `lock` is named like one.
    pub fn target_of(lock: &Path) -> Option<PathBuf> {
        let name = lock.file_name()?.to_str()?;
        let target = name.strip_prefix('.')?.strip_suffix(".lock")?;
        Some(lock.with_file_name(target))
    }
}

/// This session's hold on the map file it's editing, and the file's time
/// as last seen, to notice changes made behind the editor's back.
#[derive(Debug)]
pub struct MapClaim {
    lock: FileLock,
    modified: Option<SystemTime>,
}

impl MapClaim {
    /// Locks `path` for this session. `Ok(None)` if another session has it.
    ///
    /// # Errors
    /// If the lock file can't be created.
    pub fn acquire(path: &Path) -> io::Result<Option<Self>> {
        Ok(FileLock::try_acquire(path)?.map(|lock| Self {
            modified: modified(path),
            lock,
        }))
    }

    pub fn path(&self) -> &Path {
        self.lock.target()
    }

    /// Whether the file changed on disk since it was opened or last saved.
    pub fn changed_on_disk(&self) -> bool {
        modified(self.path()) != self.modified
    }

    /// After this session wrote the file.
    pub fn note_written(&mut self) {
        self.modified = modified(self.path());
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lofi_lock_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn one_holder_at_a_time() {
        let d = dir("one");
        let map = d.join("zone.lofimap");
        let first = FileLock::try_acquire(&map).expect("io").expect("free");
        assert!(FileLock::try_acquire(&map).expect("io").is_none(), "held");
        drop(first);
        assert!(FileLock::try_acquire(&map).expect("io").is_some());
        assert_eq!(
            FileLock::target_of(&FileLock::lock_path(&map)),
            Some(map.clone())
        );
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn every_spelling_of_a_file_resolves_to_one() {
        let d = dir("spelling");
        std::fs::create_dir_all(d.join("sub")).expect("mkdir");
        let map = d.join("zone.lofimap");
        std::fs::write(&map, b"one").expect("write");
        let real = map.canonicalize().expect("there");
        assert_eq!(resolved(&d.join("sub/../zone.lofimap")), real);
        assert_eq!(resolved(&d.join("./zone.lofimap")), real);
        // Not written yet: the folder still resolves.
        assert_eq!(
            resolved(&d.join("sub/../new.lofimap")),
            real.with_file_name("new.lofimap")
        );
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn claims_notice_changes_on_disk() {
        let d = dir("claim");
        let map = d.join("zone.lofimap");
        std::fs::write(&map, b"one").expect("write");
        let mut claim = MapClaim::acquire(&map).expect("io").expect("free");
        assert!(!claim.changed_on_disk());
        // Make sure the timestamp moves even on coarse filesystems.
        let later = SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&map)
            .and_then(|f| f.set_modified(later))
            .expect("touch");
        assert!(claim.changed_on_disk());
        claim.note_written();
        assert!(!claim.changed_on_disk());
        let _ = std::fs::remove_dir_all(d);
    }
}
