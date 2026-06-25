//! Workspace-root path guard helper.
//!
//! Enforces the W-002 invariant: any file path resolved from a
//! user-supplied or DB-stored relative path must remain inside the active
//! workspace root. Used by chapter-content handlers and host-tool write paths
//! so both surfaces share the same canonicalize + component-wise prefix-check
//! implementation.

use crate::api::errors::NexusApiError;
use std::path::{Path, PathBuf};

/// Resolve a relative path under the workspace root and enforce the
/// W-002-style path guard: the resolved absolute path must remain inside
/// the canonical workspace root.
///
/// `must_exist` controls whether the target itself must exist (read paths) or
/// whether a missing-but-creatable target is allowed (write paths). For write
/// paths, the helper walks up to the nearest existing parent so that missing
/// intermediate directories are accepted as long as they would be created
/// inside the root.
///
/// # Errors
///
/// Returns `NexusApiError::BadRequest` with `CHAPTER_PATH_*` codes when the
/// path is empty, cannot be resolved, or escapes the workspace root. Callers
/// may map these to domain-specific errors if desired.
pub fn resolve_guarded_path(
    workspace_root: &Path,
    rel_path: &str,
    must_exist: bool,
) -> Result<PathBuf, NexusApiError> {
    if rel_path.is_empty() {
        return Err(NexusApiError::BadRequest {
            code: "CHAPTER_PATH_EMPTY".to_string(),
            message: "chapter path is empty".to_string(),
        });
    }

    let canonical_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());

    let joined = canonical_root.join(rel_path);

    if must_exist {
        let canonical_target = joined
            .canonicalize()
            .map_err(|e| NexusApiError::BadRequest {
                code: "CHAPTER_PATH_UNRESOLVABLE".to_string(),
                message: format!("cannot resolve chapter path '{rel_path}': {e}"),
            })?;
        // Component-wise comparison (Path::starts_with). A plain string prefix
        // match would let `/home/user-data/evil.md` slip past a `/home/user`
        // root because the string starts with "/home/user".
        if !canonical_target.starts_with(&canonical_root) {
            return Err(NexusApiError::BadRequest {
                code: "CHAPTER_PATH_FORBIDDEN".to_string(),
                message: format!("chapter path '{rel_path}' escapes workspace root"),
            });
        }
        Ok(canonical_target)
    } else {
        // For creatable targets, normalize the joined path and verify it stays
        // within the workspace root. We walk up to the nearest existing parent
        // so that missing intermediate directories are still allowed as long
        // as they would be created inside the root.
        let mut probe = joined.as_path();
        loop {
            if let Ok(canonical) = probe.canonicalize() {
                // Component-wise comparison (Path::starts_with) — see read branch.
                if !canonical.starts_with(&canonical_root) {
                    return Err(NexusApiError::BadRequest {
                        code: "CHAPTER_PATH_FORBIDDEN".to_string(),
                        message: format!("chapter path '{rel_path}' escapes workspace root"),
                    });
                }
                return Ok(joined);
            }
            match probe.parent() {
                Some(parent) => probe = parent,
                None => {
                    return Err(NexusApiError::BadRequest {
                        code: "CHAPTER_PATH_FORBIDDEN".to_string(),
                        message: format!(
                            "chapter path '{rel_path}' has no parent inside workspace root"
                        ),
                    });
                }
            }
        }
    }
}
