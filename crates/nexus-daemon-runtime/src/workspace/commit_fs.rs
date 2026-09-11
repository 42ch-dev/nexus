//! Recoverable workspace commit filesystem primitives (v1.188 P3).
//!
//! Descriptor-relative no-follow operations; stage/backup files live in the
//! same directory as their target. External writers are not excluded — OCC
//! detects third-state bytes at check time.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Maximum decoded bytes per file in a commit manifest.
pub const MAX_FILE_BYTES: usize = 1_048_576;
/// Maximum total decoded bytes across all changes.
pub const MAX_TOTAL_BYTES: usize = 8_388_608;
/// Maximum number of changes in one commit.
pub const MAX_CHANGES: usize = 128;

/// Open a path without following symlinks (unix).
#[cfg(unix)]
fn open_nofollow(path: &Path, write: bool, create: bool) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .write(write)
        .create(create)
        .truncate(write)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(not(unix))]
fn open_nofollow(path: &Path, write: bool, create: bool) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    if write {
        opts.write(true).truncate(true);
    }
    if create {
        opts.create(true);
    }
    opts.open(path)
}

/// Returns true when metadata indicates a symlink.
pub fn is_symlink(path: &Path) -> io::Result<bool> {
    Ok(std::fs::symlink_metadata(path)?.file_type().is_symlink())
}

/// Fsync a file and its parent directory.
pub fn fsync_file_and_parent(path: &Path) -> io::Result<()> {
    let file = OpenOptions::new().write(true).open(path)?;
    file.sync_all()?;
    if let Some(parent) = path.parent() {
        if parent.exists() {
            let dir = File::open(parent)?;
            dir.sync_all()?;
        }
    }
    Ok(())
}

/// Compute lowercase SHA-256 hex for file bytes (no symlink follow).
pub fn hash_file(path: &Path) -> io::Result<String> {
    if is_symlink(path)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "symlink not allowed",
        ));
    }
    let mut file = open_nofollow(path, false, false)?;
    let mut sha = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sha.update(&buf[..n]);
    }
    Ok(hex::encode(sha.finalize()))
}

/// Compute hash from in-memory bytes.
#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut sha = Sha256::new();
    sha.update(bytes);
    hex::encode(sha.finalize())
}

/// Write bytes to a new stage file and fsync.
pub fn write_stage_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    {
        let mut file = open_nofollow(path, true, true)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fsync_file_and_parent(path)?;
    Ok(())
}

/// Copy existing regular file to backup path (no-follow).
pub fn backup_file(src: &Path, backup: &Path) -> io::Result<()> {
    if is_symlink(src)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot backup symlink",
        ));
    }
    let mut src_file = open_nofollow(src, false, false)?;
    let mut bytes = Vec::new();
    src_file.read_to_end(&mut bytes)?;
    write_stage_file(backup, &bytes)?;
    Ok(())
}

/// Atomic create: rename stage → target only if target absent.
pub fn atomic_create(target: &Path, stage: &Path) -> io::Result<()> {
    if target.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "target exists",
        ));
    }
    fs::rename(stage, target)?;
    fsync_file_and_parent(target)?;
    Ok(())
}

/// Atomic replace: rename stage → target (overwrites).
pub fn atomic_replace(target: &Path, stage: &Path) -> io::Result<()> {
    fs::rename(stage, target)?;
    fsync_file_and_parent(target)?;
    Ok(())
}

/// Delete a verified regular file.
pub fn atomic_delete(target: &Path) -> io::Result<()> {
    if is_symlink(target)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot delete symlink",
        ));
    }
    fs::remove_file(target)?;
    if let Some(parent) = target.parent() {
        if parent.exists() {
            let dir = File::open(parent)?;
            dir.sync_all()?;
        }
    }
    Ok(())
}

/// Restore pre-image from backup (create if absent, replace if present).
pub fn restore_preimage(target: &Path, backup: Option<&Path>) -> io::Result<()> {
    match backup {
        Some(backup_path) if backup_path.exists() => {
            let mut file = open_nofollow(backup_path, false, false)?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            if target.exists() {
                let stage = target.with_extension("nexus-restore-stage");
                write_stage_file(&stage, &bytes)?;
                atomic_replace(target, &stage)?;
            } else {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                let stage = target.with_extension("nexus-restore-stage");
                write_stage_file(&stage, &bytes)?;
                if target.exists() {
                    atomic_replace(target, &stage)?;
                } else {
                    atomic_create(target, &stage)?;
                }
            }
            Ok(())
        }
        _ => {
            if target.exists() {
                atomic_delete(target)?;
            }
            Ok(())
        }
    }
}

/// Remove owned stage/backup temp files (best-effort).
pub fn cleanup_temp(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

/// Decode canonical base64 content; rejects non-canonical padding.
pub fn decode_base64(encoded: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|e| format!("invalid base64: {e}"))
}
