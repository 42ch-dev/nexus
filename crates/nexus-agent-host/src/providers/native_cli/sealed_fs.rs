//! Narrow descriptor-relative no-follow filesystem primitives for the
//! sealed deny_all home (v1.188 P0 T2 fix wave 2, architecture §3.4).
//!
//! The sealed child DSH_HOME must be provisioned and written WITHOUT ever
//! following a path component that could have been swapped after
//! validation: every ancestor of the selected home is opened with
//! `openat(O_NOFOLLOW | O_DIRECTORY)` starting from the filesystem root
//! (which also VALIDATES the full ancestor chain), the exclusive leaf and
//! every layout directory/file is created with `mkdirat` /
//! `openat(O_CREAT | O_EXCL | O_NOFOLLOW)` relative to the pinned parent
//! descriptor, and modes are applied with `fchmod` on the descriptor
//! (umask-proof). A swapped component therefore cannot redirect a write:
//! descriptors pin the inode that was validated, and `O_NOFOLLOW` +
//! exclusive creates fail closed on any symlink or pre-existing entry.
//!
//! This is deliberately NOT a general filesystem abstraction: only the
//! handful of operations the sealed recipe needs, Unix-only. Callers on
//! unsupported targets fail sealed provisioning closed rather than
//! silently weakening it.

use std::ffi::OsStr;
use std::io::Write;
use std::path::Path;

use rustix::fd::OwnedFd;
use rustix::fs::{Mode, OFlags};

const DIR_OPEN: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const FILE_CREATE: OFlags = OFlags::WRONLY
    .union(OFlags::CREATE)
    .union(OFlags::EXCL)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

fn errno_name(error: &rustix::io::Errno) -> String {
    format!("{error:?}")
}

/// Open one directory component relative to `dirfd`, never following a
/// symlink. `NotFound` propagates for the caller's create-or-open choice.
fn open_dir_at(dirfd: &OwnedFd, name: &OsStr) -> Result<OwnedFd, rustix::io::Errno> {
    rustix::fs::openat(dirfd, name, DIR_OPEN, Mode::empty())
}

/// Open `path` (absolute) as a directory, validating EVERY ancestor
/// component with descriptor-relative no-follow opens from the root;
/// missing tail components are created (`0o755`, umask-independent via
/// `fchmod`). The returned descriptor is pinned to the validated inode —
/// a later component swap cannot redirect operations performed through
/// it. `..`/`.` components are resolved by the kernel relative to the
/// descriptor and remain no-follow safe.
pub fn ensure_dir_nofollow_absolute(path: &Path) -> Result<OwnedFd, String> {
    debug_assert!(path.is_absolute());
    let mut fd = rustix::fs::open("/", DIR_OPEN, Mode::empty())
        .map_err(|error| format!("could not open the filesystem root ({})", errno_name(&error)))?;
    for component in path.components() {
        let name = match component {
            std::path::Component::RootDir => continue,
            std::path::Component::Normal(name) => name,
            // Kernel-resolved relative to the pinned descriptor; safe.
            std::path::Component::CurDir => continue,
            std::path::Component::ParentDir => std::ffi::OsStr::new(".."),
            std::path::Component::Prefix(_) => {
                return Err("path prefixes are unsupported".to_string());
            }
        };
        fd = match open_dir_at(&fd, name) {
            Ok(next) => next,
            Err(rustix::io::Errno::NOENT) => {
                rustix::fs::mkdirat(&fd, name, Mode::from_bits_truncate(0o755)).map_err(
                    |error| {
                        format!(
                            "could not create a sealed-home ancestor ({})",
                            errno_name(&error)
                        )
                    },
                )?;
                // A second no-follow open catches a swapped/symlinked
                // entry even if the create raced one.
                let next = open_dir_at(&fd, name).map_err(|error| {
                    format!(
                        "a sealed-home ancestor is not a plain directory ({})",
                        errno_name(&error)
                    )
                })?;
                rustix::fs::fchmod(&next, Mode::from_bits_truncate(0o755)).map_err(|error| {
                    format!(
                        "could not set a sealed-home ancestor owner ({})",
                        errno_name(&error)
                    )
                })?;
                next
            }
            Err(error) => {
                return Err(format!(
                    "a sealed-home ancestor is a symlink or not a directory ({})",
                    errno_name(&error)
                ));
            }
        };
    }
    Ok(fd)
}

/// Open directory `name` under `dirfd` no-follow, creating it with
/// `mode` (umask-independent via `fchmod`) when absent. A symlinked or
/// non-directory entry fails closed.
pub fn ensure_dir_at(dirfd: &OwnedFd, name: &str, mode: u16) -> Result<OwnedFd, String> {
    debug_assert!(!name.contains('/'));
    let name = OsStr::new(name);
    match open_dir_at(dirfd, name) {
        Ok(fd) => Ok(fd),
        Err(rustix::io::Errno::NOENT) => {
            rustix::fs::mkdirat(dirfd, name, Mode::from_bits_truncate(mode)).map_err(|error| {
                format!(
                    "could not create a sealed-home directory ({})",
                    errno_name(&error)
                )
            })?;
            // A second no-follow open catches a swapped/symlinked entry
            // even if the create raced one.
            let fd = open_dir_at(dirfd, name).map_err(|error| {
                format!(
                    "a freshly created sealed-home directory was swapped ({})",
                    errno_name(&error)
                )
            })?;
            rustix::fs::fchmod(&fd, Mode::from_bits_truncate(mode)).map_err(|error| {
                format!(
                    "could not set sealed-home directory permissions ({})",
                    errno_name(&error)
                )
            })?;
            Ok(fd)
        }
        Err(error) => Err(format!(
            "a sealed-home directory is a symlink or not a directory ({})",
            errno_name(&error)
        )),
    }
}


/// Exclusively create directory `name` under `dirfd` (fails if ANY entry
/// already exists — no check-then-create window), apply `mode` via
/// `fchmod` (umask-proof), and return its descriptor. A symlink raced in
/// after the create is rejected by the no-follow reopen.
pub fn mkdir_exclusive_at(dirfd: &OwnedFd, name: &str, mode: u16) -> Result<OwnedFd, String> {
    debug_assert!(!name.contains('/'));
    rustix::fs::mkdirat(dirfd, name, Mode::from_bits_truncate(mode)).map_err(|error| {
        format!(
            "could not exclusively create a sealed-home directory ({})",
            errno_name(&error)
        )
    })?;
    let fd = open_dir_at(dirfd, OsStr::new(name)).map_err(|error| {
        format!(
            "a freshly created sealed-home directory was swapped ({})",
            errno_name(&error)
        )
    })?;
    rustix::fs::fchmod(&fd, Mode::from_bits_truncate(mode)).map_err(|error| {
        format!(
            "could not set sealed-home directory permissions ({})",
            errno_name(&error)
        )
    })?;
    Ok(fd)
}

/// Exclusively create file `name` under `dirfd` with owner-only content
/// (`0o600` via `fchmod`), write `contents`, and fsync it. A pre-existing
/// entry or a raced symlink fails closed.
pub fn write_file_exclusive_at(
    dirfd: &OwnedFd,
    name: &str,
    contents: &[u8],
) -> Result<(), String> {
    debug_assert!(!name.contains('/'));
    let fd = rustix::fs::openat(dirfd, name, FILE_CREATE, Mode::from_bits_truncate(0o600))
        .map_err(|error| {
            format!(
                "could not exclusively create a sealed-home file ({})",
                errno_name(&error)
            )
        })?;
    rustix::fs::fchmod(&fd, Mode::from_bits_truncate(0o600)).map_err(|error| {
        format!(
            "could not set sealed-home file permissions ({})",
            errno_name(&error)
        )
    })?;
    let mut file = std::fs::File::from(fd);
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|_io| "could not write/fsync a sealed-home file".to_string())
}

/// fsync a directory descriptor so the entries created beneath it are
/// durable (where the platform applies directory fsync).
pub fn fsync_dir(fd: &OwnedFd) -> Result<(), String> {
    rustix::fs::fsync(fd)
        .map_err(|error| format!("could not fsync a sealed-home directory ({})", errno_name(&error)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestor_symlink_is_rejected_without_writes() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        // macOS tempdirs live under the symlinked `/var`: callers resolve
        // the anchor once (as `provision` does) before the no-follow pin.
        let canonical_temp = std::fs::canonicalize(temp_dir.path()).expect("canonical temp");
        let elsewhere = temp_dir.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("elsewhere");
        std::os::unix::fs::symlink(&elsewhere, temp_dir.path().join("link")).expect("symlink");
        assert!(
            ensure_dir_nofollow_absolute(&canonical_temp.join("link")).is_err(),
            "a symlinked component must be rejected"
        );
        assert!(
            std::fs::read_dir(&elsewhere)
                .expect("read elsewhere")
                .next()
                .is_none(),
            "nothing may be written through the swap"
        );
    }

    #[test]
    fn exclusive_create_rejects_existing_entries() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let canonical_temp = std::fs::canonicalize(temp_dir.path()).expect("canonical temp");
        let fd = ensure_dir_nofollow_absolute(&canonical_temp).expect("open temp");
        let _leaf = mkdir_exclusive_at(&fd, "leaf", 0o700).expect("create leaf");
        assert!(
            mkdir_exclusive_at(&fd, "leaf", 0o700).is_err(),
            "a second create of the same leaf must fail"
        );
        write_file_exclusive_at(&fd, "state", b"x").expect("write file");
        assert!(
            write_file_exclusive_at(&fd, "state", b"y").is_err(),
            "a pre-existing file must fail the exclusive create"
        );
        let mode = std::fs::metadata(temp_dir.path().join("state"))
            .expect("stat")
            .permissions();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(mode.mode() & 0o777, 0o600, "files are owner-only");
        let dir_mode = std::fs::metadata(temp_dir.path().join("leaf"))
            .expect("stat leaf")
            .permissions();
        assert_eq!(dir_mode.mode() & 0o777, 0o700, "leaf dirs are owner-only");
    }
}
