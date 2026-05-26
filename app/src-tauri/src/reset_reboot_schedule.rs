//! Windows-only fallback for `reset_local_data` (issue #1615).
//!
//! When the in-process `remove_dir_all` step fails because a third-party
//! process (anti-virus, file-indexer, sibling OpenHuman window) still holds
//! an open handle inside the `.openhuman` tree, Windows returns
//! `ERROR_SHARING_VIOLATION` (os error 32) / `ERROR_LOCK_VIOLATION` (33)
//! and the user is stuck — see PR #2395 / #1811, which surface a "close all
//! OpenHuman windows" prompt but cannot break a foreign lock.
//!
//! This module walks the still-present sub-tree depth-first and asks the
//! Windows Session Manager to delete each entry at next boot via
//! `MoveFileExW(src, NULL, MOVEFILE_DELAY_UNTIL_REBOOT)`. The session
//! manager requires that directories be empty when boot-time deletion
//! runs, so children are scheduled before their parent.
//!
//! Reference:
//!   https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw
//!
//! Privileges: `MoveFileExW(.., NULL, MOVEFILE_DELAY_UNTIL_REBOOT)` writes
//! to `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\PendingFileRenameOperations`
//! (the boot-time session manager reads from HKLM, not the per-user hive),
//! so the call **may fail for non-administrator users** with `ERROR_ACCESS_DENIED`.
//! That is by design — Microsoft documents the elevation requirement on the
//! `MOVEFILE_DELAY_UNTIL_REBOOT` flag — and the caller in `lib.rs` handles
//! the failure path gracefully: it preserves the original lock error plus
//! the schedule failure reason and falls back to the "close all OpenHuman
//! windows and try again" guidance from PR #2395 / #1811.

#![cfg(target_os = "windows")]

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT};

/// Tally of entries handed off to `MoveFileExW`, returned to the caller so
/// it can log and surface (e.g. "scheduled 142 files / 14 dirs for deletion
/// on next reboot") instead of just an opaque "ok".
///
/// `partial` is `true` when the walk aborted mid-tree (e.g. a directory
/// became unreadable, or an individual `MoveFileExW` call failed). In that
/// case `files` / `dirs` represent **only** what was queued before the
/// failure point — useful for support logs to distinguish "everything is
/// queued" from "some of the tree is queued but the rest still needs
/// manual cleanup." Pair with the `Result::Err` returned by
/// [`schedule_path_for_reboot_deletion`] for the cause.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RebootDeletionSchedule {
    pub files: u32,
    pub dirs: u32,
    pub partial: bool,
}

impl RebootDeletionSchedule {
    pub fn total(&self) -> u32 {
        self.files.saturating_add(self.dirs)
    }
}

/// Schedule `path` (and everything under it if it is a directory) for
/// deletion on the next reboot via `MoveFileExW(_, NULL, MOVEFILE_DELAY_UNTIL_REBOOT)`.
///
/// Strategy:
///   * Regular files / symlinks → scheduled directly.
///   * Directories → children scheduled first (depth-first), then the
///     directory itself once its contents are queued.
///
/// `path` not existing on disk yields `Err(RebootDeletionFailure { error: NotFound, .. })` —
/// callers can choose to treat that as a no-op since "nothing to remove" is
/// the same outcome.
///
/// On error the failure carries a partially-populated `RebootDeletionSchedule`
/// (`partial = true`) so the caller can surface "we queued N files and M
/// folders before scheduling failed" instead of just the bare io error.
/// The walk is depth-first, so the counts reflect entries queued *before*
/// the failing step.
pub fn schedule_path_for_reboot_deletion(
    path: &Path,
) -> Result<RebootDeletionSchedule, RebootDeletionFailure> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| RebootDeletionFailure {
        error,
        partial: RebootDeletionSchedule {
            partial: true,
            ..RebootDeletionSchedule::default()
        },
    })?;
    let mut summary = RebootDeletionSchedule::default();
    match schedule_inner(path, &metadata, &mut summary) {
        Ok(()) => Ok(summary),
        Err(error) => {
            summary.partial = true;
            Err(RebootDeletionFailure {
                error,
                partial: summary,
            })
        }
    }
}

/// Pair of `(io::Error, partial schedule)` returned when the depth-first
/// walk aborts mid-tree. The `partial` field records what was queued via
/// `MoveFileExW` *before* the failure point so the caller can include the
/// counts in user-facing copy and support logs ("123 files / 7 folders
/// were queued for the next reboot before scheduling failed: <reason>").
#[derive(Debug)]
pub struct RebootDeletionFailure {
    pub error: io::Error,
    pub partial: RebootDeletionSchedule,
}

impl std::fmt::Display for RebootDeletionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl std::error::Error for RebootDeletionFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

fn schedule_inner(
    path: &Path,
    metadata: &std::fs::Metadata,
    summary: &mut RebootDeletionSchedule,
) -> io::Result<()> {
    // Symlinked directories must NOT be descended into — the lock lives
    // on the link target, not the link itself, and following would queue
    // unrelated paths for deletion. Treat symlinks (file or dir) as a
    // single leaf entry.
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let child_meta = entry.metadata()?;
            schedule_inner(&entry.path(), &child_meta, summary)?;
        }
        schedule_one(path)?;
        summary.dirs = summary.dirs.saturating_add(1);
    } else {
        schedule_one(path)?;
        summary.files = summary.files.saturating_add(1);
    }
    Ok(())
}

fn schedule_one(path: &Path) -> io::Result<()> {
    // `MoveFileExW + MOVEFILE_DELAY_UNTIL_REBOOT` requires absolute paths —
    // the session manager runs at boot before any working directory is
    // established, so a relative path cannot be resolved. The call sites
    // in `reset_local_data` already resolve paths via the core's
    // `config_get_data_paths` RPC (which returns absolute paths) so this
    // is currently a no-op in release builds; the assert catches a future
    // regression that wires a different caller in without thinking.
    debug_assert!(
        path.is_absolute(),
        "MoveFileExW + DELAY_UNTIL_REBOOT requires an absolute path, got {}",
        path.display()
    );
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 buffer that outlives the
    // call. The destination pointer is `NULL`, which (combined with
    // `MOVEFILE_DELAY_UNTIL_REBOOT`) tells Windows to delete (rather than
    // rename) the source at the next boot. `MoveFileExW` returns BOOL —
    // non-zero on success.
    let ok = unsafe { MoveFileExW(wide.as_ptr(), std::ptr::null(), MOVEFILE_DELAY_UNTIL_REBOOT) };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Each test inspects the OS-wide `PendingFileRenameOperations` registry
    // value indirectly via `MoveFileExW` success/failure — serialize tests
    // so concurrent calls don't interleave with each other in unexpected
    // ways. Cargo runs unit tests in threads within the same process.
    static SCHEDULE_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn schedule_walks_files_then_dirs() {
        let _g = SCHEDULE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("reset-target");
        std::fs::create_dir_all(root.join("nested")).expect("mkdir nested");
        std::fs::write(root.join("a.txt"), b"a").expect("write a.txt");
        std::fs::write(root.join("nested").join("b.txt"), b"b").expect("write b.txt");

        let summary = schedule_path_for_reboot_deletion(&root).expect("schedule");
        // root + nested == 2 dirs; a.txt + nested/b.txt == 2 files
        assert_eq!(summary.files, 2, "expected 2 files queued, got {summary:?}");
        assert_eq!(summary.dirs, 2, "expected 2 dirs queued, got {summary:?}");
        assert_eq!(summary.total(), 4);
    }

    #[test]
    fn schedule_single_file_reports_one_file() {
        let _g = SCHEDULE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("solo.txt");
        std::fs::write(&file, b"x").expect("write solo.txt");

        let summary = schedule_path_for_reboot_deletion(&file).expect("schedule");
        assert_eq!(
            summary,
            RebootDeletionSchedule {
                files: 1,
                dirs: 0,
                partial: false,
            }
        );
    }

    #[test]
    fn schedule_missing_path_yields_not_found() {
        let _g = SCHEDULE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("does-not-exist");

        let failure = schedule_path_for_reboot_deletion(&missing).expect_err("missing");
        assert_eq!(failure.error.kind(), io::ErrorKind::NotFound);
        // Nothing scheduled, but partial flag still reports "did not
        // complete" so callers can distinguish from a clean success.
        assert!(failure.partial.partial);
        assert_eq!(failure.partial.files, 0);
        assert_eq!(failure.partial.dirs, 0);
    }

    #[test]
    fn schedule_empty_dir_counts_one_dir() {
        let _g = SCHEDULE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        let empty = dir.path().join("empty-target");
        std::fs::create_dir(&empty).expect("mkdir empty-target");

        let summary = schedule_path_for_reboot_deletion(&empty).expect("schedule");
        assert_eq!(
            summary,
            RebootDeletionSchedule {
                files: 0,
                dirs: 1,
                partial: false,
            }
        );
    }
}
