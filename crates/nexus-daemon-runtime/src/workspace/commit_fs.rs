//! Recoverable workspace commit filesystem primitives (v1.188 P3 L2).
//!
//! Descriptor-relative no-follow operations; stage/backup files live in the
//! same directory as their target. External writers are not excluded — OCC
//! detects third-state bytes at the mutation boundary.

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

/// Owner-read/write for newly created workspace files.
pub const CREATE_FILE_MODE: u32 = 0o600;

#[cfg(unix)]
fn open_nofollow(path: &Path, write: bool, create_new: bool, truncate: bool) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut opts = OpenOptions::new();
    opts.read(true);
    if write {
        opts.write(true);
    }
    if truncate {
        opts.truncate(true);
    }
    if create_new {
        opts.create_new(true);
    }
    opts.custom_flags(libc::O_NOFOLLOW).open(path)
}

#[cfg(not(unix))]
fn open_nofollow(path: &Path, write: bool, create_new: bool, truncate: bool) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    if write {
        opts.write(true).truncate(truncate);
    }
    if create_new {
        opts.create_new(true);
    }
    opts.open(path)
}

pub fn is_symlink(path: &Path) -> io::Result<bool> {
    Ok(fs::symlink_metadata(path)?.file_type().is_symlink())
}

pub fn fsync_file_and_parent(path: &Path) -> io::Result<()> {
    if path.exists() {
        let file = OpenOptions::new().write(true).open(path)?;
        file.sync_all()?;
    }
    if let Some(parent) = path.parent() {
        if parent.exists() {
            let dir = File::open(parent)?;
            dir.sync_all()?;
        }
    }
    Ok(())
}

pub fn hash_file(path: &Path) -> io::Result<String> {
    if is_symlink(path)? {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "symlink not allowed"));
    }
    let mut file = open_nofollow(path, false, false, false)?;
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

#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut sha = Sha256::new();
    sha.update(bytes);
    hex::encode(sha.finalize())
}

/// Write a new exclusive stage file (create-new, no-clobber).
pub fn write_stage_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_stage_file_with_mode(path, bytes, None)
}

pub fn write_stage_file_with_mode(path: &Path, bytes: &[u8], mode: Option<u32>) -> io::Result<()> {
    if path.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "stage exists"));
    }
    {
        let mut file = open_nofollow(path, true, true, true)?;
        #[cfg(unix)]
        if let Some(mode) = mode {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fsync_file_and_parent(path)?;
    Ok(())
}

pub fn backup_file(src: &Path, backup: &Path) -> io::Result<()> {
    if is_symlink(src)? {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "cannot backup symlink"));
    }
    if backup.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "backup exists"));
    }
    let mut src_file = open_nofollow(src, false, false, false)?;
    let mut bytes = Vec::new();
    src_file.read_to_end(&mut bytes)?;
    let mode = file_mode(src);
    write_stage_file_with_mode(backup, &bytes, mode)?;
    Ok(())
}

#[cfg(not(unix))]
fn file_mode(_path: &Path) -> Option<u32> {
    None
}

#[cfg(unix)]
fn file_mode(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).ok().map(|m| m.permissions().mode())
}

pub fn read_file_mode(path: &Path) -> Option<u32> {
    file_mode(path)
}

/// Atomic create via rename; fails if target exists (no-clobber).
pub fn atomic_create(target: &Path, stage: &Path) -> io::Result<()> {
    if target.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "target exists"));
    }
    fs::rename(stage, target)?;
    fsync_file_and_parent(target)?;
    Ok(())
}

/// Replace only when current hash matches `expected` (third-state guard).
pub fn atomic_replace_verified(target: &Path, stage: &Path, expected_hash: &str) -> io::Result<()> {
    let current = hash_file(target)?;
    if current != expected_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "third-state bytes at replace boundary",
        ));
    }
    fs::rename(stage, target)?;
    fsync_file_and_parent(target)?;
    Ok(())
}

pub fn atomic_delete_verified(target: &Path, expected_hash: &str) -> io::Result<()> {
    if is_symlink(target)? {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "cannot delete symlink"));
    }
    let current = hash_file(target)?;
    if current != expected_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "third-state bytes at delete boundary",
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

/// Restore preimage when current bytes match `post_hash` or target absent for create rollback.
pub fn restore_preimage_verified(
    target: &Path,
    backup: Option<&Path>,
    pre_hash: Option<&str>,
    post_hash: Option<&str>,
) -> io::Result<()> {
    if target.exists() {
        let current = hash_file(target)?;
        if let Some(post) = post_hash {
            if current != post {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "third-state during rollback",
                ));
            }
        } else if let Some(pre) = pre_hash {
            if current != pre {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected bytes during rollback",
                ));
            }
        }
    }
    match backup {
        Some(backup_path) if backup_path.exists() => {
            let mut file = open_nofollow(backup_path, false, false, false)?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            let stage = target.with_extension("nexus-restore-stage");
            if stage.exists() {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "restore stage exists"));
            }
            write_stage_file_with_mode(&stage, &bytes, file_mode(backup_path))?;
            if target.exists() {
                let pre = pre_hash.unwrap_or("");
                atomic_replace_verified(target, &stage, post_hash.unwrap_or(pre))?;
            } else {
                atomic_create(target, &stage)?;
            }
            Ok(())
        }
        _ => {
            if target.exists() {
                atomic_delete_verified(target, post_hash.unwrap_or(pre_hash.unwrap_or("")))?;
            }
            Ok(())
        }
    }
}

pub fn cleanup_temp(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

pub fn decode_base64(encoded: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|e| format!("invalid base64: {e}"))
}

pub fn require_parent_exists(target: &Path) -> io::Result<()> {
    match target.parent() {
        Some(parent) if parent.exists() => Ok(()),
        _ => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "parent directory must exist",
        )),
    }
}
