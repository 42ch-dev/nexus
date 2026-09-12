//! Manager-lifetime advisory lease beside the creator DB (v1.188 P3 L2).

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Shared exclusive OS advisory lock held for the process lifetime of a workspace authority.
#[derive(Debug)]
pub struct WorkspaceAuthorityLease {
    path: PathBuf,
    file: Option<File>,
}

impl WorkspaceAuthorityLease {
    /// Acquire an exclusive lock file adjacent to the canonical creator DB.
    ///
    /// # Errors
    ///
    /// Returns the [`io::Error`] from creating or opening the lock file, or
    /// from the non-blocking `flock` when another manager already holds the
    /// lease (the caller sees `WouldBlock`/`EAGAIN`).
    pub fn acquire(db_path: &Path) -> io::Result<Arc<Self>> {
        let path = db_path.with_extension("workspace_authority.lock");
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            #[allow(deprecated)]
            {
                if let Err(e) = nix::fcntl::flock(
                    file.as_raw_fd(),
                    nix::fcntl::FlockArg::LockExclusiveNonblock,
                ) {
                    return Err(lock_conflict_error(e));
                }
            }
        }
        Ok(Arc::new(Self {
            path,
            file: Some(file),
        }))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Classify a non-blocking `flock` failure as a held lease.
///
/// A held lease surfaces as `EWOULDBLOCK`/`EAGAIN` on Linux and as `EACCES` on
/// some BSD paths, so every "busy" errno collapses to ONE platform-independent
/// [`io::ErrorKind::WouldBlock`] error instead of leaking a raw errno string.
/// Locking semantics are unchanged: the lock is still non-blocking, exclusive,
/// and reported as an error to the caller.
#[cfg(unix)]
fn lock_conflict_error(err: nix::errno::Errno) -> io::Error {
    // `EWOULDBLOCK` is `EAGAIN` on both target platforms, so `EAGAIN` already
    // spells both; `EACCES` covers the BSD lock-conflict variant.
    let raw = err as i32;
    if raw == nix::errno::Errno::EAGAIN as i32 || raw == nix::errno::Errno::EACCES as i32 {
        io::Error::new(
            io::ErrorKind::WouldBlock,
            format!("workspace authority lease is already held: {err}"),
        )
    } else {
        io::Error::other(err)
    }
}

impl Drop for WorkspaceAuthorityLease {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            #[cfg(unix)]
            {
                use std::os::unix::io::AsRawFd;
                #[allow(deprecated)]
                {
                    let _ = nix::fcntl::flock(file.as_raw_fd(), nix::fcntl::FlockArg::Unlock);
                }
            }
            drop(file);
        }
    }
}
