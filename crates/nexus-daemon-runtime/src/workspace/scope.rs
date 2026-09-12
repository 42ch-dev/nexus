//! Opened-scope coordinate resolution (v1.188 P3 L2).

use std::path::{Path, PathBuf};

use super::bounds::validate_relative_path;
use super::session::{enforce_path_boundary, SessionError};

/// Canonical scope directory for a session (`relative_path` may be empty).
///
/// # Errors
///
/// Returns whichever error [`validate_relative_path`] or
/// [`enforce_path_boundary`] raises: [`SessionError::ManifestInvalid`] for a
/// malformed relative path, or the boundary violation when the joined path
/// escapes `canonical_root`.
pub fn scope_directory(
    canonical_root: &Path,
    scope_relative: &str,
) -> Result<PathBuf, SessionError> {
    let base = if scope_relative.is_empty() {
        canonical_root.to_path_buf()
    } else {
        validate_relative_path(scope_relative)?;
        let joined = canonical_root.join(scope_relative);
        enforce_path_boundary(&joined, canonical_root)?;
        joined
    };
    Ok(base)
}

/// Resolve a manifest `path` relative to the opened scope coordinate.
///
/// # Errors
///
/// Returns [`SessionError::ManifestInvalid`] when `change_path` fails
/// [`validate_relative_path`].
pub fn resolve_in_scope(scope_dir: &Path, change_path: &str) -> Result<PathBuf, SessionError> {
    validate_relative_path(change_path)?;
    let target = scope_dir.join(change_path);
    Ok(target)
}
