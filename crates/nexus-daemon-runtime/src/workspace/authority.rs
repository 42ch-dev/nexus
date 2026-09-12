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
                    return Err(io::Error::other(e));
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
