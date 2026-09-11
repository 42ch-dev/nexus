//! Narrow descriptor-relative no-follow filesystem primitives for the
//! sealed deny_all home (v1.188 P0 T2 fix waves 2-3, architecture §3.4).
//!
//! The sealed child DSH_HOME is provisioned, written, revalidated and
//! DELETED without ever following a path component that could have been
//! swapped after validation: every ancestor of the selected home is
//! opened with `openat(O_NOFOLLOW | O_DIRECTORY)` starting from the
//! filesystem root (which also VALIDATES the full ancestor chain), the
//! exclusive leaf and every layout directory/file is created with
//! `mkdirat` / `openat(O_CREAT | O_EXCL | O_NOFOLLOW)` relative to the
//! pinned parent descriptor, modes are applied with `fchmod` on the
//! descriptor (umask-proof), and removal recurses with `unlinkat`
//! relative to the retained anchor descriptor — never a path-based
//! `remove_dir_all` on the security-sensitive lease. A swapped component
//! therefore cannot redirect a write or a delete: descriptors pin the
//! inode that was validated.
//!
//! # Trust model (exact)
//!
//! The boundary enforced here excludes mutation by OTHER users: every
//! anchor component must be non-group-writable and either
//! non-other-writable or a sticky world-writable system directory (like
//! `/tmp`, where the sticky bit already prevents other users from
//! removing/renaming entries they do not own); the selected home,
//! `nexus/` and the leaf must additionally be owned by the effective
//! uid. NO protection is claimed against malicious processes running as
//! the SAME uid — the OS grants them equivalent authority (architecture
//! §3.4's owner-only boundary).
//!
//! This is deliberately NOT a general filesystem abstraction: only the
//! handful of operations the sealed recipe needs, Unix-only. Callers on
//! unsupported targets fail sealed provisioning closed rather than
//! silently weakening it.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::io::Write;
use std::path::Path;

use rustix::fd::OwnedFd;
use rustix::fs::{AtFlags, Mode, OFlags};

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
fn open_dir_at<Fd: std::os::fd::AsFd>(dirfd: Fd, name: &OsStr) -> Result<OwnedFd, rustix::io::Errno> {
    rustix::fs::openat(dirfd, name, DIR_OPEN, Mode::empty())
}

/// Boundary check for one pinned directory (trust model above): never
/// group-writable; other-writable only when the sticky bit is set (the
/// `/tmp` shape, where the OS already prevents other users from
/// removing/renaming entries they do not own); when `require_owner`,
/// also owned by the effective uid (selected home / `nexus` / leaf —
/// anything another user owns can be mutated by them).
pub fn check_dir_boundary(fd: &OwnedFd, require_owner: bool) -> Result<(), String> {
    let stat = rustix::fs::fstat(fd)
        .map_err(|error| format!("could not stat a sealed-home directory ({})", errno_name(&error)))?;
    let mode = stat.st_mode & 0o7777;
    if mode & 0o020 != 0 {
        return Err("a sealed-home directory is group-writable".to_string());
    }
    if mode & 0o002 != 0 && mode & 0o1000 == 0 {
        return Err(
            "a sealed-home directory is world-writable without the sticky bit".to_string()
        );
    }
    if require_owner {
        let euid = rustix::process::geteuid().as_raw();
        if stat.st_uid != euid {
            return Err("a sealed-home directory is owned by another user".to_string());
        }
    }
    Ok(())
}

/// Inode identity of one pinned descriptor (dev + ino) for swap
/// revalidation.
pub fn inode_of(fd: &OwnedFd) -> Result<(u64, u64), String> {
    let stat = rustix::fs::fstat(fd)
        .map_err(|error| format!("could not stat a pinned descriptor ({})", errno_name(&error)))?;
    Ok((stat.st_dev as u64, stat.st_ino as u64))
}

/// Open `path` (absolute) as a directory, validating EVERY ancestor
/// component with descriptor-relative no-follow opens from the root and
/// enforcing the non-owner-mutation boundary on each. The returned
/// descriptor is pinned to the validated inode — a later component swap
/// cannot redirect operations performed through it.
pub fn open_dir_nofollow_absolute(path: &Path) -> Result<OwnedFd, String> {
    walk_absolute(path, false)
}

/// Like [`open_dir_nofollow_absolute`], but missing components are
/// created (`0o755`, umask-independent via `fchmod`).
pub fn ensure_dir_nofollow_absolute(path: &Path) -> Result<OwnedFd, String> {
    walk_absolute(path, true)
}

fn walk_absolute(path: &Path, create_missing: bool) -> Result<OwnedFd, String> {
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
            Err(rustix::io::Errno::NOENT) if create_missing => {
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
                    "a sealed-home ancestor is a symlink, missing, or not a directory ({})",
                    errno_name(&error)
                ));
            }
        };
        check_dir_boundary(&fd, false)?;
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

/// Open one file under `dirfd` no-follow (for revalidation reads/stats).
pub fn open_file_nofollow_at(dirfd: &OwnedFd, name: &str) -> Result<OwnedFd, String> {
    debug_assert!(!name.contains('/'));
    rustix::fs::openat(
        dirfd,
        name,
        OFlags::RDONLY.union(OFlags::NOFOLLOW).union(OFlags::CLOEXEC),
        Mode::empty(),
    )
    .map_err(|error| {
        format!(
            "could not open a sealed-home file no-follow ({})",
            errno_name(&error)
        )
    })
}

/// fsync a directory descriptor so the entries created beneath it are
/// durable (where the platform applies directory fsync).
pub fn fsync_dir(fd: &OwnedFd) -> Result<(), String> {
    rustix::fs::fsync(fd)
        .map_err(|error| format!("could not fsync a sealed-home directory ({})", errno_name(&error)))
}

/// Recursively delete directory `name` below `dirfd` — descriptor-rooted
/// and no-follow all the way down: subdirectories are re-opened with
/// `openat(O_NOFOLLOW | O_DIRECTORY)` before recursion (a swapped
/// symlink is unlinked as a file, never followed), entries are removed
/// with `unlinkat`, and the emptied directory with
/// `unlinkat(AT_REMOVEDIR)`. A security-sensitive retained lease is
/// never deleted through a path-based `remove_dir_all`.
pub fn remove_tree_at(dirfd: &OwnedFd, name: &str) -> Result<(), String> {
    debug_assert!(!name.contains('/'));
    match open_dir_at(dirfd, OsStr::new(name)) {
        Ok(fd) => {
            empty_dir_at(&fd)?;
            rustix::fs::unlinkat(dirfd, name, AtFlags::REMOVEDIR).map_err(|error| {
                format!(
                    "could not remove the emptied retained lease ({})",
                    errno_name(&error)
                )
            })
        }
        // A swapped lexical leaf that is now a symlink or file: unlink the
        // anchor entry directly — never follow it to the target.
        Err(rustix::io::Errno::LOOP | rustix::io::Errno::NOTDIR) => {
            rustix::fs::unlinkat(dirfd, name, AtFlags::empty()).map_err(|error| {
                format!(
                    "could not unlink the swapped retained lease entry ({})",
                    errno_name(&error)
                )
            })
        }
        Err(error) => Err(format!(
            "could not open the retained lease no-follow ({})",
            errno_name(&error)
        )),
    }
}

/// Remove every entry inside the pinned directory `fd`, recursing only
/// into entries that re-open as real directories no-follow.
fn empty_dir_at(fd: &OwnedFd) -> Result<(), String> {
    let dir = rustix::fs::Dir::read_from(fd)
        .map_err(|error| format!("could not read a retained lease directory ({})", errno_name(&error)))?;
    for entry in dir {
        let entry = entry
            .map_err(|error| format!("could not read a retained lease entry ({})", errno_name(&error)))?;
        let name = entry.file_name();
        if name.to_bytes() == b"." || name.to_bytes() == b".." {
            continue;
        }
        match open_dir_at(fd, OsStr::from_bytes(name.to_bytes())) {
            // A real directory: empty it recursively, then remove it.
            Ok(sub_fd) => {
                empty_dir_at(&sub_fd)?;
                rustix::fs::unlinkat(fd, name, AtFlags::REMOVEDIR).map_err(|error| {
                    format!(
                        "could not remove a retained lease subdirectory ({})",
                        errno_name(&error)
                    )
                })?;
            }
            // A symlink or non-directory entry: unlink it directly —
            // never followed (ELOOP/ENOTDIR from the no-follow open).
            Err(rustix::io::Errno::LOOP | rustix::io::Errno::NOTDIR) => {
                rustix::fs::unlinkat(fd, name, AtFlags::empty()).map_err(|error| {
                    format!(
                        "could not unlink a retained lease entry ({})",
                        errno_name(&error)
                    )
                })?;
            }
            Err(error) => {
                return Err(format!(
                    "could not inspect a retained lease entry ({})",
                    errno_name(&error)
                ));
            }
        }
    }
    Ok(())
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
            open_dir_nofollow_absolute(&canonical_temp.join("link")).is_err(),
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

    #[test]
    fn remove_tree_is_anchored_and_never_follows_symlinks() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let canonical_temp = std::fs::canonicalize(temp_dir.path()).expect("canonical temp");
        let fd = ensure_dir_nofollow_absolute(&canonical_temp).expect("open temp");
        let leaf = mkdir_exclusive_at(&fd, "leaf", 0o700).expect("create leaf");
        let sub = mkdir_exclusive_at(&leaf, "sub", 0o700).expect("create sub");
        write_file_exclusive_at(&sub, "state", b"x").expect("write nested");
        write_file_exclusive_at(&leaf, "patch", b"y").expect("write file");
        // A symlink inside the tree is unlinked, never followed.
        let outside = temp_dir.path().join("outside");
        std::fs::create_dir_all(&outside).expect("outside dir");
        std::fs::write(outside.join("keep"), b"keep").expect("keep file");
        std::os::unix::fs::symlink(&outside, temp_dir.path().join("leaf").join("link"))
            .expect("symlink inside tree");

        remove_tree_at(&fd, "leaf").expect("anchored removal");
        assert!(
            !temp_dir.path().join("leaf").exists(),
            "the pinned leaf is removed"
        );
        assert_eq!(
            std::fs::read_to_string(outside.join("keep")).expect("keep survives"),
            "keep",
            "the symlink target is never followed or deleted"
        );
    }

    #[test]
    fn boundary_rejects_world_writable_without_sticky() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let canonical_temp = std::fs::canonicalize(temp_dir.path()).expect("canonical temp");
        let fd = ensure_dir_nofollow_absolute(&canonical_temp).expect("open temp");
        let loose = mkdir_exclusive_at(&fd, "loose", 0o700).expect("create dir");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            temp_dir.path().join("loose"),
            std::fs::Permissions::from_mode(0o777),
        )
        .expect("chmod 0777");
        assert!(
            check_dir_boundary(&loose, false).is_err(),
            "a non-sticky world-writable directory must fail the boundary"
        );
    }
}
