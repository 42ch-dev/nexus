//! Workspace-root path guard helper.
//!
//! Enforces the W-002 invariant: any file path resolved from a
//! user-supplied or DB-stored relative path must remain inside the active
//! workspace root. Used by chapter-content handlers and host-tool write paths
//! so both surfaces share the same canonicalize + component-wise prefix-check
//! implementation.

use crate::api::errors::NexusApiError;
use std::path::{Path, PathBuf};

/// Async wrapper around [`resolve_guarded_path`] that runs the blocking
/// `std::fs::canonicalize` syscalls on the tokio blocking pool.
///
/// V1.88 T3 (R-V187-QC3-P001): the wrapper is shared by fs/* tools and
/// manuscript/chapter/outline handlers so all path-guard checks stay
/// non-blocking for the async runtime.
///
/// # Errors
///
/// Propagates the same [`NexusApiError`] variants as [`resolve_guarded_path`]
/// (e.g. `BadRequest` with `chapter_path_*` codes), plus an `Internal`
/// `PATH_GUARD_PANIC` if the blocking task panics.
pub async fn resolve_guarded_path_async(
    workspace_root: PathBuf,
    rel_path: String,
    must_exist: bool,
) -> Result<PathBuf, NexusApiError> {
    tokio::task::spawn_blocking(move || {
        resolve_guarded_path(&workspace_root, &rel_path, must_exist)
    })
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "PATH_GUARD_PANIC".into(),
        message: format!("path guard task panicked: {e}"),
    })?
}

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
/// # TOCTOU note
///
/// There is a small race window between canonicalizing the workspace root and
/// canonicalizing the requested path: a local attacker with filesystem access
/// could replace either path during that window. Per the V1.86 trust-boundary
/// spec, this is "racy-correct" rather than "racy-incorrect" for the
/// single-user local daemon context: the practical risk is bounded by that
/// threat model, while adversarial multi-user FS access is out of scope
/// (`R-V166-QC2-TOCTOU`).
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
            code: "chapter_path_empty".to_string(),
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
                code: "chapter_path_unresolvable".to_string(),
                message: format!("cannot resolve chapter path '{rel_path}': {e}"),
            })?;
        // Component-wise comparison (Path::starts_with). A plain string prefix
        // match would let `/home/user-data/evil.md` slip past a `/home/user`
        // root because the string starts with "/home/user".
        if !canonical_target.starts_with(&canonical_root) {
            return Err(NexusApiError::BadRequest {
                code: "chapter_path_forbidden".to_string(),
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
                        code: "chapter_path_forbidden".to_string(),
                        message: format!("chapter path '{rel_path}' escapes workspace root"),
                    });
                }
                return Ok(joined);
            }
            match probe.parent() {
                Some(parent) => probe = parent,
                None => {
                    return Err(NexusApiError::BadRequest {
                        code: "chapter_path_forbidden".to_string(),
                        message: format!(
                            "chapter path '{rel_path}' has no parent inside workspace root"
                        ),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolve_guarded_path_async_accepts_inside_and_rejects_escape() {
        let root = tempfile::tempdir().unwrap().path().to_path_buf();
        let nested = root.join("Works/test/Outlines");
        std::fs::create_dir_all(&nested).unwrap();
        let file = nested.join("ch01.md");
        std::fs::write(&file, "x").unwrap();

        assert!(
            resolve_guarded_path_async(
                root.clone(),
                "Works/test/Outlines/ch01.md".to_string(),
                true
            )
            .await
            .is_ok(),
            "inside path should be accepted"
        );
        assert!(
            resolve_guarded_path_async(root, "../escape.md".to_string(), true)
                .await
                .is_err(),
            "escape path should be rejected"
        );
    }

    /// Regression: a sibling directory whose name extends the workspace-root
    /// name (e.g. root `…/creative`, sibling `…/creative-evil`) must NOT pass
    /// the async guard via a `..` traversal. Covers both the read path
    /// (`must_exist = true`) and the write path (`must_exist = false`).
    #[tokio::test]
    async fn resolve_guarded_path_async_rejects_prefix_confusion_sibling() {
        let base = tempfile::tempdir().unwrap().path().to_path_buf();
        let root = base.join("creative");
        std::fs::create_dir_all(&root).unwrap();
        let evil_dir = base.join("creative-evil");
        std::fs::create_dir_all(&evil_dir).unwrap();
        std::fs::write(evil_dir.join("evil.md"), "stolen").unwrap();

        assert!(
            resolve_guarded_path_async(root.clone(), "../creative-evil/evil.md".to_string(), true)
                .await
                .is_err(),
            "prefix-confusion sibling must be rejected on the read path"
        );
        assert!(
            resolve_guarded_path_async(
                root.clone(),
                "../creative-evil/newfile.md".to_string(),
                false
            )
            .await
            .is_err(),
            "prefix-confusion sibling must be rejected on the write path"
        );
        assert!(
            resolve_guarded_path_async(root, "Outlines/ch01.md".to_string(), false)
                .await
                .is_ok(),
            "inside-root creatable path should be accepted"
        );
    }
}
