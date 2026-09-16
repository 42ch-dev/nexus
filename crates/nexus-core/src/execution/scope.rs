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

/// Require a session's persisted workspace root to match the active executor root.
///
/// Both paths are canonicalized before comparison so symlink-equivalent roots
/// match and foreign sessions cannot commit through a different authority.
///
/// # Errors
///
/// Returns [`SessionError::ActiveWorkspaceMismatch`] when the canonical roots
/// differ, or whichever error [`canonicalize_workspace_root`] reports.
pub async fn enforce_active_workspace_root(
    session_workspace_root: &str,
    active_workspace_root: &str,
) -> Result<(), SessionError> {
    let session_canonical =
        super::session::canonicalize_workspace_root(std::path::Path::new(session_workspace_root))
            .await?;
    let active_canonical =
        super::session::canonicalize_workspace_root(std::path::Path::new(active_workspace_root))
            .await?;
    if session_canonical != active_canonical {
        return Err(SessionError::ActiveWorkspaceMismatch {
            session_root: session_canonical.to_string_lossy().into_owned(),
            active_root: active_canonical.to_string_lossy().into_owned(),
        });
    }
    Ok(())
}
