//! Reading and writing map files on disk without ever losing one.

use std::fs::{self, File, Metadata};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::error::MapFileError;
use super::format::{FileStamp, LoadedMap, decode_map, encode_map};
use crate::map::WorldMap;

/// Far past any real map; refuses to read multi-gigabyte junk into memory.
const MAX_FILE_BYTES: u64 = 2 << 30;

/// What a save managed besides the save itself.
#[derive(Debug, Default)]
pub struct SaveReport {
    /// The map saved, but `.bak` couldn't be refreshed.
    pub backup_error: Option<String>,
}

pub fn load_map(path: &Path) -> Result<LoadedMap, MapFileError> {
    let len = fs::metadata(path)?.len();
    if len > MAX_FILE_BYTES {
        return Err(MapFileError::Invalid(format!(
            "{len} bytes is too big for a map"
        )));
    }
    decode_map(&fs::read(path)?)
}

/// Replaces `path` atomically and keeps the previous save as `<path>.bak`.
///
/// Refuses a read-only file: that usually means "not checked out", and a
/// rename would quietly replace it anyway.
pub fn save_map(map: &WorldMap, stamp: FileStamp, path: &Path) -> Result<SaveReport, MapFileError> {
    Ok(write_atomically(path, &encode_map(map, stamp)?, true)?)
}

/// Like [`save_map`] without the backup; an autosave is a backup already.
pub fn save_autosave(map: &WorldMap, stamp: FileStamp, path: &Path) -> Result<(), MapFileError> {
    write_atomically(path, &encode_map(map, stamp)?, false)?;
    Ok(())
}

/// Any other file the tools write, e.g. heightmap exports: same crash safety
/// as a map, no backup.
pub(crate) fn write_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomically(path, bytes, false).map(|_| ())
}

/// `<path>.bak`, where [`save_map`] keeps the previous save.
#[must_use]
pub fn backup_path(path: &Path) -> PathBuf {
    with_suffix(path, ".bak")
}

/// Write a temp file, flush it to disk, back up the old file by copying (so
/// the map never goes missing), then rename over it. A crash at any point
/// leaves either the old map or the new one in place.
fn write_atomically(path: &Path, bytes: &[u8], backup: bool) -> io::Result<SaveReport> {
    // Write through symlinks to the real file, so the link survives.
    let target = match fs::canonicalize(path) {
        Ok(target) => target,
        Err(e) if e.kind() == io::ErrorKind::NotFound => path.to_owned(),
        Err(e) => return Err(e),
    };
    let existing = metadata_if_present(&target)?;
    if existing
        .as_ref()
        .is_some_and(|m| m.permissions().readonly())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} is read-only", target.display()),
        ));
    }

    let tmp = with_suffix(&target, ".tmp");
    let result = (|| {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        // Keep the old file's permissions, e.g. group-writable shared maps.
        if let Some(meta) = &existing {
            fs::set_permissions(&tmp, meta.permissions())?;
        }

        let mut report = SaveReport::default();
        if backup && existing.is_some() {
            report.backup_error = write_backup(&target).err().map(|e| e.to_string());
        }
        retry_while_busy(|| fs::rename(&tmp, &target))?;
        sync_parent_dir(&target)?;
        Ok(report)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// A fresh file each time, not `fs::copy`: a copy inherits the source's
/// permissions, and one read-only `.bak` would then block every later save.
fn write_backup(target: &Path) -> io::Result<()> {
    let bak = backup_path(target);
    match fs::remove_file(&bak) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let mut source = retry_while_busy(|| File::open(target))?;
    let mut copy = File::create(&bak)?;
    io::copy(&mut source, &mut copy)?;
    copy.sync_all()
}

fn metadata_if_present(path: &Path) -> io::Result<Option<Metadata>> {
    match fs::metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// On Windows, antivirus and sync clients briefly hold files open. Give them
/// a moment rather than failing the save.
fn retry_while_busy<T>(mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    const ATTEMPTS: u64 = 5;
    let mut attempt = 0;
    loop {
        match op() {
            Err(e) if cfg!(windows) && is_busy(&e) && attempt + 1 < ATTEMPTS => {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(50 * attempt));
            }
            result => return result,
        }
    }
}

/// Access denied, sharing violation (32) or lock violation (33). Rust files
/// the last two under "uncategorized", so match the raw codes.
fn is_busy(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::PermissionDenied || matches!(e.raw_os_error(), Some(32 | 33))
}

/// Makes the rename itself durable. Unix only: Windows can't open a directory
/// for flushing, and NTFS journals renames.
#[cfg(unix)]
fn sync_parent_dir(path: &Path) -> io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    File::open(dir)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_dir(_: &Path) -> io::Result<()> {
    Ok(())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::GridSize;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("lofi_world_{name}_{}", std::process::id()));
            fs::create_dir_all(&dir).expect("temp dir");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn map(height: f32) -> WorldMap {
        WorldMap::flat("t", GridSize::new(1, 1).expect("valid size"), 2.0, height)
    }

    fn save(path: &Path, height: f32) -> Result<SaveReport, MapFileError> {
        save_map(&map(height), FileStamp::default(), path)
    }

    #[test]
    fn saves_keep_the_previous_one_as_backup() {
        let dir = TempDir::new("backup");
        let path = dir.0.join("zone.lofimap");
        save(&path, 1.0).expect("first save");
        assert!(!backup_path(&path).exists(), "nothing to back up yet");
        let report = save(&path, 2.0).expect("second save");
        assert!(report.backup_error.is_none());

        assert_eq!(load_map(&path).expect("loads").map, map(2.0));
        assert_eq!(load_map(&backup_path(&path)).expect("loads").map, map(1.0));
        assert!(!with_suffix(&path, ".tmp").exists());
    }

    #[test]
    fn autosaves_make_no_backup() {
        let dir = TempDir::new("autosave");
        let path = dir.0.join("zone.lofimap.autosave");
        save_autosave(&map(1.0), FileStamp::default(), &path).expect("saves");
        save_autosave(&map(2.0), FileStamp::default(), &path).expect("saves");
        assert!(!backup_path(&path).exists());
    }

    #[test]
    fn a_failed_save_leaves_nothing_behind() {
        let dir = TempDir::new("failed");
        let path = dir.0.join("missing_dir").join("zone.lofimap");
        assert!(save(&path, 1.0).is_err());
        assert!(!with_suffix(&path, ".tmp").exists());
    }

    #[test]
    fn read_only_maps_are_refused() {
        let dir = TempDir::new("readonly");
        let path = dir.0.join("zone.lofimap");
        save(&path, 1.0).expect("first save");
        let mut perms = fs::metadata(&path).expect("exists").permissions();
        perms.set_readonly(true);
        fs::set_permissions(&path, perms).expect("chmod");

        let err = save(&path, 2.0).expect_err("refused");
        assert!(err.to_string().contains("read-only"), "{err}");
        assert_eq!(load_map(&path).expect("loads").map, map(1.0));
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_backup_does_not_block_saving() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("readonly_bak");
        let path = dir.0.join("zone.lofimap");
        save(&path, 1.0).expect("first save");
        save(&path, 2.0).expect("second save");
        fs::set_permissions(backup_path(&path), fs::Permissions::from_mode(0o444)).expect("chmod");
        let report = save(&path, 3.0).expect("saves anyway");
        assert!(report.backup_error.is_none(), "stale .bak is replaced");
        assert_eq!(load_map(&backup_path(&path)).expect("loads").map, map(2.0));
    }

    #[cfg(unix)]
    #[test]
    fn saving_through_a_symlink_keeps_the_link_and_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("symlink");
        let real = dir.0.join("real.lofimap");
        let link = dir.0.join("link.lofimap");
        save(&real, 1.0).expect("first save");
        fs::set_permissions(&real, fs::Permissions::from_mode(0o660)).expect("chmod");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");

        save(&link, 2.0).expect("saves through the link");
        assert!(fs::symlink_metadata(&link).expect("exists").is_symlink());
        assert_eq!(load_map(&real).expect("loads").map, map(2.0));
        let mode = fs::metadata(&real).expect("exists").permissions().mode();
        assert_eq!(mode & 0o777, 0o660);
    }
}
