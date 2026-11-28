//! A bundle root pinned to an open directory descriptor.
//!
//! Confinement has to be anchored to the directory object, not to the pathname a
//! caller passed: if that pathname is renamed away or replaced by a symlink after
//! the root is canonicalised, a path-based reader confines to the replacement and
//! hands back prompt bytes from outside the bundle. The root is therefore opened
//! once, every asset read resolves relative to that descriptor, and the asset's
//! own descriptor path is compared with the root's.

use std::io::Read as _;
use std::path::{Path, PathBuf};

/// An opened bundle root; every asset read resolves inside this directory.
#[derive(Debug)]
pub struct BundleRoot {
    dir: std::fs::File,
    /// The root's path at open time. Only targets without `openat` need it, to
    /// name the assets they open by path.
    #[cfg(windows)]
    path: PathBuf,
}

impl BundleRoot {
    /// Open `root` as a directory handle that fixes the confinement boundary.
    ///
    /// # Errors
    ///
    /// Returns the open error when `root` is not a directory this process may
    /// traverse.
    pub fn open(root: &Path) -> std::io::Result<Self> {
        let dir = open_root_dir(root)?;
        #[cfg(windows)]
        let path = nexus_config_metadata::final_path(&dir)?;
        Ok(Self {
            dir,
            #[cfg(windows)]
            path,
        })
    }

    /// The directory's own current path, as its descriptor reports it.
    ///
    /// This is the location to record alongside decisions made through this
    /// root: it comes from the same descriptor the reads use, never from a
    /// second resolution of the caller's path.
    ///
    /// # Errors
    ///
    /// Returns the platform error when the descriptor's path cannot be read.
    #[cfg(target_os = "linux")]
    pub fn path(&self) -> std::io::Result<PathBuf> {
        let (path, unlinked) = descriptor_path(&self.dir)?;
        Ok(linked_path(path, unlinked))
    }

    /// # Errors
    ///
    /// Returns the platform error when the descriptor's path cannot be read.
    #[cfg(target_os = "macos")]
    pub fn path(&self) -> std::io::Result<PathBuf> {
        descriptor_path(&self.dir)
    }

    /// # Errors
    ///
    /// Returns the platform error when the descriptor's path cannot be read.
    #[cfg(windows)]
    pub fn path(&self) -> std::io::Result<PathBuf> {
        nexus_config_metadata::final_path(&self.dir)
    }

    /// No confinement proof is wired for this target, so there is no path either.
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    pub fn path(&self) -> std::io::Result<PathBuf> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "cannot verify asset confinement on this platform",
        ))
    }

    /// Read a bundle-relative asset through one descriptor inside this root.
    ///
    /// The bytes come from the descriptor the confinement check approved, and
    /// the check is anchored to the root handle, so replacing the bundle's
    /// pathname afterwards cannot move the boundary.
    ///
    /// # Errors
    ///
    /// Returns the open/read error, a refusal for a non-regular target, or the
    /// sandbox diagnostic when the descriptor resolves outside the root.
    #[cfg(target_os = "linux")]
    pub fn read(&self, rel: &str) -> std::io::Result<Vec<u8>> {
        let file = match open_beneath(&self.dir, rel) {
            Ok(file) => file,
            // `BENEATH` refuses an absolute symlink target with `EXDEV` even
            // when it resolves inside the bundle; no resolve-flag set both
            // follows absolute targets and confines them (`BENEATH | IN_ROOT`
            // is rejected as mutually exclusive, and `IN_ROOT` alone rebases
            // the target at the root, so a genuine absolute in-bundle path
            // matches nothing). `NOSYS` covers kernels before 5.6.
            Err(rustix::io::Errno::XDEV | rustix::io::Errno::NOSYS) => {
                let file = openat_asset(&self.dir, rel)?;
                reject_non_regular(rel, &file)?;
                verify_beneath_root(&self.dir, rel, &file)?;
                return read_bytes(file);
            }
            Err(errno) => return Err(errno.into()),
        };
        reject_non_regular(rel, &file)?;
        read_bytes(file)
    }

    /// macOS proves containment by descriptor identity: the asset is opened
    /// relative to the root, then its containing directory is walked up via
    /// `openat(dir, "..")` — a real parent link from the vnode, not a name
    /// lookup — until an ancestor's `(st_dev, st_ino)` equals the root's. The
    /// opened object is accepted only if an `O_NOFOLLOW` reopen of it inside
    /// that proven directory has the same identity, so the bytes come from an
    /// inode reachable beneath the root.
    ///
    /// `F_GETPATH` supplies the location to start from, never the verdict: a
    /// name query cannot tell a rebased mount from the root, whereas the
    /// `..` chain is the filesystem's own ancestry.
    #[cfg(target_os = "macos")]
    pub fn read(&self, rel: &str) -> std::io::Result<Vec<u8>> {
        let file = openat_asset(&self.dir, rel)?;
        reject_non_regular(rel, &file)?;

        let path = descriptor_path(&file)?;
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(escape_ancestry(rel));
        };
        let parent = open_directory(parent)?;
        ensure_beneath_root(&self.dir, rel, &parent)?;

        // Tie the object to the proven directory by identity: the name only
        // locates a candidate, the inode decides whether it is the same object.
        let sibling = openat_leaf(&parent, name)?;
        if identity(&sibling)? != identity(&file)? {
            return Err(escape_ancestry(rel));
        }
        read_bytes(file)
    }

    /// Windows opens the asset by path; the root handle's own final path is the
    /// boundary, so an asset reached through a replaced bundle path is refused
    /// instead of read. Like the macOS arm this is a name comparison — Windows
    /// has no handle-ancestry query — so a reparse/junction redirection or a
    /// rename timed against the two lookups is outside what it can decide.
    #[cfg(windows)]
    pub fn read(&self, rel: &str) -> std::io::Result<Vec<u8>> {
        let file = std::fs::File::open(self.path.join(rel))?;
        reject_non_regular(rel, &file)?;
        if !nexus_config_metadata::final_path(&file)?
            .starts_with(nexus_config_metadata::final_path(&self.dir)?)
        {
            return Err(escape(rel));
        }
        read_bytes(file)
    }

    /// No confinement proof is wired for this target, so the read fails closed.
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    pub fn read(&self, _rel: &str) -> std::io::Result<Vec<u8>> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "cannot verify asset confinement on this platform",
        ))
    }
}

#[cfg(any(target_os = "linux", windows))]
fn escape(rel: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        format!("asset `{rel}` resolves outside the bundle sandbox (symlink escape)"),
    )
}

/// A FIFO, socket or device answers the open (`O_NONBLOCK` keeps it from
/// blocking) but is never an asset, and its descriptor name says nothing about
/// the bundle: refuse it before any path query or byte read.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn reject_non_regular(rel: &str, file: &std::fs::File) -> std::io::Result<()> {
    if file.metadata()?.is_file() {
        return Ok(());
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("asset `{rel}` is not a regular file"),
    ))
}

fn read_bytes(mut file: std::fs::File) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// `O_PATH` opens the directory without requiring read permission, so a bundle
/// root that is only searchable (`0711`) still imports; `openat2` resolution
/// needs execute permission on each component, not a listing.
#[cfg(target_os = "linux")]
fn open_root_dir(root: &Path) -> std::io::Result<std::fs::File> {
    use rustix::fs::{open, Mode, OFlags};

    open(
        root,
        OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map(std::fs::File::from)
    .map_err(std::io::Error::from)
}

/// `O_SEARCH` opens the directory for search only: Darwin refuses a plain
/// `O_RDONLY` directory open without read permission, while `openat` from an
/// `O_SEARCH` descriptor still resolves (verified on macOS).
#[cfg(target_os = "macos")]
fn open_root_dir(root: &Path) -> std::io::Result<std::fs::File> {
    use nix::fcntl::{open, OFlag};
    use nix::sys::stat::Mode;

    open(root, OFlag::O_SEARCH | OFlag::O_CLOEXEC, Mode::empty())
        .map(std::fs::File::from)
        .map_err(std::io::Error::from)
}

/// A directory handle needs backup semantics; `File::open` alone refuses one.
///
/// # Search-only roots
///
/// `read(true)` requests `GENERIC_READ`, which carries `FILE_LIST_DIRECTORY` for
/// a directory, so a root the caller may only traverse is refused here. A
/// minimal-rights `CreateFileW` handle would not help: `final_path` uses
/// `FILE_NAME_NORMALIZED`, and Microsoft documents that this normalization
/// queries every path component and fails with `ERROR_ACCESS_DENIED` when access
/// to any one of them is missing. Windows therefore has no search-only open for
/// this reader; the refusal is the documented boundary, not an oversight.
#[cfg(windows)]
fn open_root_dir(root: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(root)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn open_root_dir(_root: &Path) -> std::io::Result<std::fs::File> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "cannot verify asset confinement on this platform",
    ))
}

/// `O_NONBLOCK` is what keeps a FIFO substitution from blocking the open before
/// the descriptor can be inspected; `O_CLOEXEC` keeps it out of child processes.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn openat_asset(dir: &std::fs::File, rel: &str) -> std::io::Result<std::fs::File> {
    use nix::fcntl::{openat, OFlag};
    use nix::sys::stat::Mode;

    openat(
        dir,
        rel,
        OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map(std::fs::File::from)
    .map_err(std::io::Error::from)
}

/// The kernel proves confinement: `openat2` resolves `rel` relative to the root
/// descriptor and fails with `EXDEV` for any resolution that leaves it.
///
/// `BENEATH`, not `NO_SYMLINKS`: the validation pass accepts in-bundle symlinks,
/// so a relative link that stays inside must resolve here.
#[cfg(target_os = "linux")]
fn open_beneath(dir: &std::fs::File, rel: &str) -> Result<std::fs::File, rustix::io::Errno> {
    use rustix::fs::{openat2, Mode, OFlags, ResolveFlags};

    let fd = openat2(
        dir,
        rel,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::BENEATH,
    )?;
    Ok(std::fs::File::from(fd))
}

/// Descriptor-path confinement for the cases the kernel declines: `/proc/self/fd`
/// names the object each descriptor is bound to, and a descriptor's link count —
/// never the shape of the name — says whether it still has a directory entry.
///
/// This comparison is by name, not a handle-ancestry proof: both paths are read
/// from their own descriptors, so a rename racing the pair could align them.
/// It only has to catch a *resolution that escaped* the root (the open itself
/// was already relative to the root descriptor), and Linux exposes no
/// handle-ancestry query to do better; `openat2(RESOLVE_BENEATH)` is the only
/// such proof and it is the primary arm.
#[cfg(target_os = "linux")]
fn verify_beneath_root(
    root: &std::fs::File,
    rel: &str,
    file: &std::fs::File,
) -> std::io::Result<()> {
    let (root_path, root_unlinked) = descriptor_path(root)?;
    let (asset_path, asset_unlinked) = descriptor_path(file)?;
    if !linked_path(asset_path, asset_unlinked).starts_with(linked_path(root_path, root_unlinked)) {
        return Err(escape(rel));
    }
    Ok(())
}

/// The descriptor's own path behind `/proc/self/fd`, plus whether that link has
/// no directory entry left.
#[cfg(target_os = "linux")]
fn descriptor_path(file: &std::fs::File) -> std::io::Result<(PathBuf, bool)> {
    use std::os::fd::AsRawFd as _;
    use std::os::unix::fs::MetadataExt as _;

    let unlinked = file.metadata()?.nlink() == 0;
    let path =
        std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd())).map_err(|error| {
            match error.kind() {
                std::io::ErrorKind::NotFound => std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "cannot verify asset confinement on this platform: \
                 openat2 declined the resolution and /proc/self/fd is unavailable",
                ),
                _ => error,
            }
        })?;
    Ok((path, unlinked))
}

/// The kernel appends ` (deleted)` only for a link with no directory entry left,
/// which is exactly `st_nlink == 0`.
#[cfg(target_os = "linux")]
fn linked_path(path: PathBuf, unlinked: bool) -> PathBuf {
    use std::os::unix::ffi::OsStrExt as _;

    if !unlinked {
        return path;
    }
    match path.as_os_str().as_bytes().strip_suffix(b" (deleted)") {
        Some(stripped) => PathBuf::from(std::ffi::OsStr::from_bytes(stripped)),
        None => path,
    }
}

/// macOS names the descriptor's own path through `fcntl(F_GETPATH)`.
#[cfg(target_os = "macos")]
fn descriptor_path(file: &std::fs::File) -> std::io::Result<PathBuf> {
    use nix::fcntl::{fcntl, FcntlArg};

    let mut path = PathBuf::new();
    fcntl(file, FcntlArg::F_GETPATH(&mut path)).map_err(std::io::Error::from)?;
    Ok(path)
}

/// How many `..` steps the ancestry proof may take before refusing.
#[cfg(target_os = "macos")]
const MAX_ANCESTOR_LEVELS: usize = 4096;

/// The identity pair a descriptor reports. Names are never compared: a mount or
/// a rename can move a name, but the inode a descriptor is bound to stays put.
#[cfg(target_os = "macos")]
fn identity(file: &std::fs::File) -> std::io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt as _;

    let metadata = file.metadata()?;
    Ok((metadata.dev(), metadata.ino()))
}

/// Refusal from the ancestry proof. Distinct from the path-level wording so a
/// test can tell which rule decided, while still carrying the sandbox phrase.
#[cfg(target_os = "macos")]
fn escape_ancestry(rel: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        format!(
            "asset `{rel}` resolves outside the bundle sandbox \
             (descriptor ancestry does not reach the root)"
        ),
    )
}

/// Open a directory for search only; a bundle root that permits traversal but
/// not a listing is still usable.
#[cfg(target_os = "macos")]
fn open_directory(path: &Path) -> std::io::Result<std::fs::File> {
    use nix::fcntl::{open, OFlag};
    use nix::sys::stat::Mode;

    open(path, OFlag::O_SEARCH | OFlag::O_CLOEXEC, Mode::empty())
        .map(std::fs::File::from)
        .map_err(std::io::Error::from)
}

/// Open a directory relative to an open directory descriptor. Search-only, like
/// the root itself: an ancestor the bundle may only be traversed through must
/// not need a listing for the `..` walk.
#[cfg(target_os = "macos")]
fn openat_directory(dir: &std::fs::File, name: &str) -> std::io::Result<std::fs::File> {
    use nix::fcntl::{openat, OFlag};
    use nix::sys::stat::Mode;

    openat(dir, name, OFlag::O_SEARCH | OFlag::O_CLOEXEC, Mode::empty())
        .map(std::fs::File::from)
        .map_err(std::io::Error::from)
}

/// Open a leaf inside an open directory without following a link at that name.
#[cfg(target_os = "macos")]
fn openat_leaf(dir: &std::fs::File, name: &std::ffi::OsStr) -> std::io::Result<std::fs::File> {
    use nix::fcntl::{openat, OFlag};
    use nix::sys::stat::Mode;

    openat(
        dir,
        name,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map(std::fs::File::from)
    .map_err(std::io::Error::from)
}

/// Walk `..` from `start` until an ancestor is the root directory by identity.
///
/// `..` from a directory descriptor is the filesystem's real parent link, so
/// meeting the root's `(st_dev, st_ino)` proves the directory really is beneath
/// it — a name prefix cannot: a mount at a path under the root, or a rename
/// between two lookups, would satisfy the name and not the ancestry.
#[cfg(target_os = "macos")]
fn ensure_beneath_root(
    root: &std::fs::File,
    rel: &str,
    start: &std::fs::File,
) -> std::io::Result<()> {
    let root_identity = identity(root)?;
    let mut current = start.try_clone()?;
    for _ in 0..MAX_ANCESTOR_LEVELS {
        let current_identity = identity(&current)?;
        if current_identity == root_identity {
            return Ok(());
        }
        let parent = openat_directory(&current, "..")?;
        let parent_identity = identity(&parent)?;
        if parent_identity == current_identity {
            // The filesystem root, one whose `..` is itself: no bundle root here.
            return Err(escape_ancestry(rel));
        }
        current = parent;
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("asset `{rel}` confinement walk exceeded {MAX_ANCESTOR_LEVELS} ancestor levels"),
    ))
}
