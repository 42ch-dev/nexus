//! Private durability kernel shared by the `content`, `creators`, and
//! `presets` atomic writers (I-000564).
//!
//! The caller owns the sibling temp path (naming and lifetime) and every
//! error-code mapping. The kernel performs exactly:
//! write temp → fsync temp → rename over target → optional fsync of the
//! renamed file → fsync the parent directory. It never removes a pre-existing
//! target except through the rename itself, and it does not clean up the
//! caller's temp path on failure — each wrapper keeps its own rollback
//! policy. The stage-tagged error lets callers keep their legacy error codes.

use std::io;
use std::path::Path;

/// Which stage of the atomic write produced the error.
#[derive(Debug)]
pub(crate) enum Stage {
    /// Writing the caller's temp file failed.
    Write,
    /// Opening the temp file for fsync failed.
    TempOpen,
    /// `sync_all` on the temp file failed.
    TempSync,
    /// Renaming the temp file over the target failed.
    Rename,
    /// Opening or `sync_all` on the renamed target failed
    /// (only when final-file fsync is enabled).
    FinalSync,
    /// `sync_all` on the target's parent directory failed.
    DirSync,
    /// Opening the target's parent directory failed.
    DirOpen,
}

/// An I/O failure tagged with the kernel stage that produced it.
#[derive(Debug)]
pub(crate) struct AtomicWriteError {
    pub stage: Stage,
    pub source: io::Error,
}

impl AtomicWriteError {
    fn at(stage: Stage) -> impl FnOnce(io::Error) -> Self {
        move |source| Self { stage, source }
    }

    /// Discard the stage tag; used by callers that report a plain
    /// `io::Result` or fold every stage into one error code.
    pub(crate) fn into_io(self) -> io::Error {
        self.source
    }
}

impl std::fmt::Display for AtomicWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(f)
    }
}

/// Write `content` to `tmp_path`, fsync it, rename it over `target_path`,
/// then fsync the renamed file (when `fsync_final`) and the parent directory.
///
/// `tmp_path` must be a sibling of `target_path` on the same filesystem; it
/// is created and owned by the caller, and is left in place on failure for
/// the caller's cleanup policy to handle.
pub(crate) fn atomic_write(
    target_path: &Path,
    tmp_path: &Path,
    content: &[u8],
    fsync_final: bool,
) -> Result<(), AtomicWriteError> {
    std::fs::write(tmp_path, content).map_err(AtomicWriteError::at(Stage::Write))?;

    let file = std::fs::File::open(tmp_path).map_err(AtomicWriteError::at(Stage::TempOpen))?;
    file.sync_all()
        .map_err(AtomicWriteError::at(Stage::TempSync))?;
    drop(file);

    std::fs::rename(tmp_path, target_path).map_err(AtomicWriteError::at(Stage::Rename))?;

    if fsync_final {
        let final_file =
            std::fs::File::open(target_path).map_err(AtomicWriteError::at(Stage::FinalSync))?;
        final_file
            .sync_all()
            .map_err(AtomicWriteError::at(Stage::FinalSync))?;
    }

    if let Some(parent) = target_path.parent() {
        let dir = std::fs::File::open(parent).map_err(AtomicWriteError::at(Stage::DirOpen))?;
        dir.sync_all().map_err(AtomicWriteError::at(Stage::DirSync))?;
    }

    Ok(())
}
