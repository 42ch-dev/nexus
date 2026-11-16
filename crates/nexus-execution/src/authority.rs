//! Manager-lifetime advisory lease beside the creator DB (v1.188 P3 L2).

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

/// Shared exclusive OS advisory lock held for the process lifetime of a workspace authority.
#[derive(Debug)]
pub struct WorkspaceAuthorityLease {
    path: PathBuf,
    /// The held lock file, taken exactly ONCE — by [`Self::release`] (a
    /// confirmed owner close) or by `Drop` (the last reference going away).
    ///
    /// Behind a mutex because the authority is shared: the owner that
    /// established the composition releases the lease at the END of its
    /// confirmed close, when the lock is no longer load-bearing, instead of
    /// waiting for every `Arc` clone — including the ones a settled owner's
    /// engine and ports still reference — to drop.
    file: Mutex<Option<File>>,
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
            file: Mutex::new(Some(file)),
        }))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Close the held lock file now, releasing the OS lease even while other
    /// handles to this lease are still referenced.
    ///
    /// A CONFIRMED owner close is the point where the composition that took
    /// the lease is over: its admission is fenced and every owned drive has
    /// joined, so the lock no longer fences a live writer and must not keep
    /// fencing the next owner of the same home. Idempotent: `false` means the
    /// lease was already released (or was never held).
    pub fn release(&self) -> bool {
        let file = self
            .file
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        // Explicit `flock` unlock, then the close: releasing here leaves the
        // same state as the dropped-lease path.
        let Some(file) = file else {
            return false;
        };
        unlock(&file);
        drop(file);
        true
    }
}

/// Release the OS lock a held lock file carries.
///
/// Closing the file descriptor is what releases the `flock`; the explicit
/// unlock keeps the released-by-us path identical to the dropped-lease path.
#[cfg(unix)]
fn unlock(file: &File) {
    use std::os::unix::io::AsRawFd;
    #[allow(deprecated)]
    let _ = nix::fcntl::flock(file.as_raw_fd(), nix::fcntl::FlockArg::Unlock);
}

#[cfg(not(unix))]
fn unlock(_file: &File) {}

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
        // The last reference going away is the OTHER release path: the file
        // still held here is closed (and its lock released) only when nothing
        // can use the lease any more.
        if let Some(file) = self
            .file
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            unlock(&file);
            drop(file);
        }
    }
}
