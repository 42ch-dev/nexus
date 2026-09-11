//! Manager-lifetime advisory lease beside the creator DB (v1.188 P3).

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
            use nix::fcntl::{flock, FlockArg};
            use std::os::unix::io::AsRawFd;
            flock(file.as_raw_fd(), FlockArg::LockExclusiveNonblock)
                .map_err(io::Error::from)?;
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
                use nix::fcntl::{flock, FlockArg};
                use std::os::unix::io::AsRawFd;
                let _ = flock(file.as_raw_fd(), FlockArg::Unlock);
            }
            drop(file);
        }
    }
}
