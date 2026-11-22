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
pub enum Stage {
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
pub struct AtomicWriteError {
    pub stage: Stage,
    pub source: io::Error,
}

impl AtomicWriteError {
    fn at(stage: Stage) -> impl FnOnce(io::Error) -> Self {
        move |source| Self { stage, source }
    }

    /// Discard the stage tag; used by callers that report a plain
    /// `io::Result` or fold every stage into one error code.
    pub fn into_io(self) -> io::Error {
        self.source
    }
}

impl std::fmt::Display for AtomicWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(f)
    }
}

/// Stage one: write `content` to `tmp_path` and fsync it.
///
/// `tmp_path` must be a sibling of the eventual target on the same
/// filesystem; it is created and owned by the caller, and is left in place
/// on failure for the caller's cleanup policy to handle. Nothing about the
/// target changes in this stage, so an async caller dropped before its
/// [`commit`] leaves the previous content intact.
pub fn prepare_temp(tmp_path: &Path, content: &[u8]) -> Result<(), AtomicWriteError> {
    std::fs::write(tmp_path, content).map_err(AtomicWriteError::at(Stage::Write))?;

    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(tmp_path)
        .map_err(AtomicWriteError::at(Stage::TempOpen))?;
    file.sync_all()
        .map_err(AtomicWriteError::at(Stage::TempSync))?;

    Ok(())
}

/// Stage two: rename `tmp_path` over `target_path`, then fsync the renamed
/// file (when `fsync_final`) and the parent directory.
///
/// `tmp_path` must be the file [`prepare_temp`] already wrote and fsynced.
/// This stage runs synchronously on the caller: it is not interruptible
/// once entered, and on failure the temp file is left in place for the
/// caller's cleanup policy.
pub fn commit(
    target_path: &Path,
    tmp_path: &Path,
    fsync_final: bool,
) -> Result<(), AtomicWriteError> {
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
        dir.sync_all()
            .map_err(AtomicWriteError::at(Stage::DirSync))?;
    }

    Ok(())
}

/// Write `content` to `tmp_path`, fsync it, rename it over `target_path`,
/// then fsync the renamed file (when `fsync_final`) and the parent directory.
///
/// `tmp_path` must be a sibling of `target_path` on the same filesystem; it
/// is created and owned by the caller, and is left in place on failure for
/// the caller's cleanup policy to handle.
///
/// This is the convenience composition of [`prepare_temp`] and [`commit`]
/// for callers with no cancellation point between the two stages.
pub fn atomic_write(
    target_path: &Path,
    tmp_path: &Path,
    content: &[u8],
    fsync_final: bool,
) -> Result<(), AtomicWriteError> {
    prepare_temp(tmp_path, content)?;
    commit(target_path, tmp_path, fsync_final)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Negative control for the two-phase kernel: `prepare_temp` alone never
    /// touches the target — the rename only happens in `commit`. Mirrors the
    /// `content` cancellation contract, where a dropped future stops between
    /// the two stages.
    #[test]
    fn prepare_without_commit_leaves_target_bytes_intact() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = dir.path().join("chapter.md");
        let tmp = dir.path().join("chapter.md.tmp.1.1");
        std::fs::write(&target, b"old content").expect("seed target");

        prepare_temp(&tmp, b"new content").expect("prepare succeeds");

        assert_eq!(
            std::fs::read(&target).expect("read target"),
            b"old content",
            "prepare alone must not replace the target"
        );
        assert_eq!(
            std::fs::read(&tmp).expect("read temp"),
            b"new content",
            "prepare stages the new bytes in the temp file"
        );

        commit(&target, &tmp, true).expect("commit succeeds");
        assert_eq!(
            std::fs::read(&target).expect("read target"),
            b"new content",
            "commit replaces the target"
        );
        assert!(!tmp.exists(), "commit consumes the temp file");
    }
}
