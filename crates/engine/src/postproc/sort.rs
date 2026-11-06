//! Moving finished files and folders without ever overwriting, and the Recycle Bin.

use crate::naming::{reserve_unique_dir, reserve_unique_path};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

/// Moves files to the Recycle Bin. Haul never deletes a user's file any other way.
pub trait Recycler: Send + Sync {
    fn recycle(&self, paths: &[PathBuf]) -> Result<(), String>;
}

/// The real Windows Recycle Bin.
pub struct RecycleBin;

impl Recycler for RecycleBin {
    fn recycle(&self, paths: &[PathBuf]) -> Result<(), String> {
        trash::delete_all(paths).map_err(|e| e.to_string())
    }
}

/// `ERROR_NOT_SAME_DEVICE`.
const NOT_SAME_DEVICE: i32 = 17;

fn crosses_devices(e: &io::Error) -> bool {
    e.kind() == ErrorKind::CrossesDevices || e.raw_os_error() == Some(NOT_SAME_DEVICE)
}

/// Total bytes under `p` (a file or a folder).
fn size_of(p: &Path) -> io::Result<u64> {
    let m = std::fs::symlink_metadata(p)?;
    if !m.is_dir() {
        return Ok(m.len());
    }
    let mut total = 0;
    for e in std::fs::read_dir(p)? {
        total += size_of(&e?.path())?;
    }
    Ok(total)
}

fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    if !std::fs::metadata(src)?.is_dir() {
        std::fs::copy(src, dst)?;
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        copy_tree(&e.path(), &dst.join(e.file_name()))?;
    }
    Ok(())
}

fn remove_tree(p: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(p)?.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) }
}

/// Moves a file or folder into `dest_dir` as `name`, or `name (n)` when taken. Never overwrites.
/// Returns where it ended up.
pub fn move_item(src: &Path, dest_dir: &Path, name: &str) -> io::Result<PathBuf> {
    move_item_with(src, dest_dir, name, |a, b| std::fs::rename(a, b))
}

/// `move_item` with the rename step injected. A rename that fails because the target is on
/// another drive falls back to copy, size check, then removing the source.
pub fn move_item_with(
    src: &Path,
    dest_dir: &Path,
    name: &str,
    rename: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<PathBuf> {
    let is_dir = std::fs::metadata(src)?.is_dir();
    if src.parent() == Some(dest_dir) && src.file_name().is_some_and(|n| n == name) {
        return Ok(src.to_path_buf());
    }
    std::fs::create_dir_all(dest_dir)?;
    // Reserve the name first, so two moves at once can't pick the same one.
    let target = if is_dir { reserve_unique_dir(dest_dir, name)? } else { reserve_unique_path(dest_dir, name)? };
    if is_dir {
        std::fs::remove_dir(&target)?;
    }
    let undo_reservation = |t: &Path| {
        let _ = if is_dir { std::fs::remove_dir_all(t) } else { std::fs::remove_file(t) };
    };
    match rename(src, &target) {
        Ok(()) => Ok(target),
        Err(e) if crosses_devices(&e) => {
            let want = size_of(src)?;
            let copied = copy_tree(src, &target).and_then(|()| size_of(&target));
            match copied {
                Ok(got) if got == want => {
                    remove_tree(src)?;
                    Ok(target)
                }
                Ok(got) => {
                    undo_reservation(&target);
                    Err(io::Error::other(format!("copy was {got} bytes, expected {want}")))
                }
                Err(e) => {
                    undo_reservation(&target);
                    Err(e)
                }
            }
        }
        Err(e) => {
            if !is_dir {
                undo_reservation(&target);
            }
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn sort_never_overwrites() {
        let t = tempfile::tempdir().unwrap();
        let (src_dir, dest) = (t.path().join("src"), t.path().join("dest"));
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("a.txt"), "old").unwrap();
        fs::write(src_dir.join("a.txt"), "new").unwrap();
        let got = move_item(&src_dir.join("a.txt"), &dest, "a.txt").unwrap();
        assert_eq!(got, dest.join("a (1).txt"));
        assert_eq!(fs::read_to_string(dest.join("a.txt")).unwrap(), "old");
        assert_eq!(fs::read_to_string(&got).unwrap(), "new");
        assert!(!src_dir.join("a.txt").exists());
    }

    #[test]
    fn move_renames_and_creates_dest() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("a.iso"), "x").unwrap();
        let got = move_item(&t.path().join("a.iso"), &t.path().join(r"Programs\ISO"), "renamed.iso").unwrap();
        assert_eq!(got, t.path().join(r"Programs\ISO\renamed.iso"));
        assert_eq!(fs::read_to_string(got).unwrap(), "x");
    }

    #[test]
    fn moves_folder_unique() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("pkg");
        fs::create_dir_all(src.join("inner")).unwrap();
        fs::write(src.join("inner/f.txt"), "f").unwrap();
        let dest = t.path().join("Video");
        fs::create_dir_all(dest.join("pkg")).unwrap();
        let got = move_item(&src, &dest, "pkg").unwrap();
        assert_eq!(got, dest.join("pkg (1)"));
        assert_eq!(fs::read_to_string(got.join("inner/f.txt")).unwrap(), "f");
        assert!(dest.join("pkg").is_dir());
        assert!(!src.exists());
    }

    #[test]
    fn cross_device_falls_back_to_copy() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("pkg");
        fs::create_dir_all(src.join("inner")).unwrap();
        fs::write(src.join("a.bin"), vec![7u8; 5000]).unwrap();
        fs::write(src.join("inner/b.txt"), "b").unwrap();
        fs::write(t.path().join("lone.txt"), "lone").unwrap();
        let cross = |_: &Path, _: &Path| Err(std::io::Error::from(std::io::ErrorKind::CrossesDevices));
        let dest = t.path().join("other-drive");
        let got = move_item_with(&src, &dest, "pkg", cross).unwrap();
        assert_eq!(fs::read(got.join("a.bin")).unwrap(), vec![7u8; 5000]);
        assert_eq!(fs::read_to_string(got.join("inner/b.txt")).unwrap(), "b");
        assert!(!src.exists());
        let got = move_item_with(&t.path().join("lone.txt"), &dest, "lone.txt", cross).unwrap();
        assert_eq!(fs::read_to_string(got).unwrap(), "lone");
        assert!(!t.path().join("lone.txt").exists());
    }

    #[test]
    fn other_rename_errors_leave_no_placeholder() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("a.txt"), "a").unwrap();
        let denied = |_: &Path, _: &Path| Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        let dest = t.path().join("d");
        assert!(move_item_with(&t.path().join("a.txt"), &dest, "a.txt", denied).is_err());
        assert!(!dest.join("a.txt").exists());
        assert!(t.path().join("a.txt").exists());
    }

    #[test]
    fn same_place_is_noop() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("a.txt"), "a").unwrap();
        assert_eq!(move_item(&t.path().join("a.txt"), t.path(), "a.txt").unwrap(), t.path().join("a.txt"));
        assert_eq!(fs::read_to_string(t.path().join("a.txt")).unwrap(), "a");
        assert!(!t.path().join("a (1).txt").exists());
    }

    #[test]
    fn missing_source_errs() {
        let t = tempfile::tempdir().unwrap();
        let e = move_item(&t.path().join("gone.txt"), &t.path().join("d"), "gone.txt").unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
        assert!(!t.path().join("d/gone.txt").exists());
    }
}
