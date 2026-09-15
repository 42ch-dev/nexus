//! Workspace-root path guard helper.
//!
//! Enforces the W-002 invariant: any file path resolved from a
//! user-supplied or DB-stored relative path must remain inside the active
//! workspace root. Used by chapter-content handlers and host-tool write paths.
//!
//! The check itself has a single authority: [`nexus_core::resolve_guarded_path`]
//! (core `content.rs` module). This boundary module only delegates and maps the
//! core taxonomy onto [`NexusApiError`], so a security fix cannot diverge
//! between the core content services and the daemon fs/* consumers.

use crate::api::errors::NexusApiError;
use std::path::{Path, PathBuf};

/// Map the core guard taxonomy onto the legacy daemon classification.
///
/// `InvalidInput { field, reason }` becomes `BadRequest { code, message }`
/// (the legacy `chapter_path_*` wire shape) and the async wrapper's
/// `PATH_GUARD_PANIC` carrier is split back into the legacy
/// `Internal { code, message }` form. Everything else keeps the shared
/// generic conversion.
fn map_guard_error(error: nexus_core::CoreError) -> NexusApiError {
    match error {
        nexus_core::CoreError::InvalidInput { field, reason } => {
            NexusApiError::BadRequest { code: field, message: reason }
        }
        nexus_core::CoreError::Internal { category } => match category.split_once(": ") {
            Some(("PATH_GUARD_PANIC", message)) => NexusApiError::Internal {
                code: "PATH_GUARD_PANIC".to_string(),
                message: message.to_string(),
            },
            _ => nexus_core::CoreError::Internal { category }.into(),
        },
        other => other.into(),
    }
}

/// Async wrapper around [`nexus_core::resolve_guarded_path`] that runs the
/// blocking `std::fs::canonicalize` syscalls on the tokio blocking pool.
///
/// V1.88 T3 (R-V187-QC3-P001): the wrapper is shared by fs/* tools and
/// manuscript/chapter/outline handlers so all path-guard checks stay
/// non-blocking for the async runtime.
///
/// # Errors
///
/// Propagates the same [`NexusApiError`] variants as
/// [`resolve_guarded_path`] (e.g. `BadRequest` with `chapter_path_*` codes),
/// plus an `Internal` `PATH_GUARD_PANIC` if the blocking task panics.
pub async fn resolve_guarded_path_async(
    workspace_root: PathBuf,
    rel_path: String,
    must_exist: bool,
) -> Result<PathBuf, NexusApiError> {
    nexus_core::resolve_guarded_path_async(workspace_root, rel_path, must_exist)
        .await
        .map_err(map_guard_error)
}

/// Resolve a relative path under the workspace root and enforce the
/// W-002-style path guard (see [`nexus_core::resolve_guarded_path`] for the
/// canonical semantics: canonicalize + component-wise containment, with a
/// creatable walk-up probe on write paths).
///
/// # Errors
///
/// Returns `NexusApiError::BadRequest` with `chapter_path_*` codes when the
/// path is empty, cannot be resolved, or escapes the workspace root — the
/// legacy wire shape, mapped from the shared core error at this boundary.
pub fn resolve_guarded_path(
    workspace_root: &Path,
    rel_path: &str,
    must_exist: bool,
) -> Result<PathBuf, NexusApiError> {
    nexus_core::resolve_guarded_path(workspace_root, rel_path, must_exist)
        .map_err(map_guard_error)
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

    /// W-002 equivalence regression: the daemon boundary (this module,
    /// `NexusApiError`) and the core authority (`nexus_core`,
    /// `CoreError`) agree on traversal, symlink-escape and creatable-parent
    /// cases — same verdict and same legacy `chapter_path_*` code on both
    /// sides of the delegation.
    #[tokio::test]
    async fn path_guard_boundaries_stay_equivalent() {
        fn core_code(result: Result<PathBuf, nexus_core::CoreError>) -> String {
            match result {
                Ok(_) => "ok".to_string(),
                Err(nexus_core::CoreError::InvalidInput { field, .. }) => field,
                Err(other) => panic!("unexpected core error: {other:?}"),
            }
        }
        fn daemon_code(result: Result<PathBuf, NexusApiError>) -> String {
            match result {
                Ok(_) => "ok".to_string(),
                Err(NexusApiError::BadRequest { code, .. }) => code,
                Err(other) => panic!("unexpected daemon error: {other:?}"),
            }
        }

        let base = tempfile::tempdir().unwrap().keep();
        let root = base.join("creative");
        std::fs::create_dir_all(&root).unwrap();

        // (1) traversal: `..` into a prefix-confusion sibling (read path).
        let evil_dir = base.join("creative-evil");
        std::fs::create_dir_all(&evil_dir).unwrap();
        std::fs::write(evil_dir.join("evil.md"), "stolen").unwrap();

        // (2) symlink escape: in-root link pointing outside the root.
        let outside = base.join("outside-target.md");
        std::fs::write(&outside, "secret").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join("link.md")).unwrap();

        // (3) creatable-parent: missing intermediate dirs inside the root
        // (write path) must stay accepted.
        let cases: &[(&str, &str, bool)] = &[
            ("traversal", "../creative-evil/evil.md", true),
            ("symlink", "link.md", true),
            ("creatable-parent", "Works/w/Outlines/v2/ch01.md", false),
        ];
        for (name, rel, must_exist) in cases {
            let core =
                nexus_core::resolve_guarded_path(&root, rel, *must_exist);
            let daemon = resolve_guarded_path(&root, rel, *must_exist);
            assert_eq!(core_code(core), daemon_code(daemon), "case {name}");
        }

        // The rejected cases must actually be rejections, not accidental
        // `ok`/`ok` agreement, and the creatable write must be permitted.
        assert_eq!(
            daemon_code(resolve_guarded_path(&root, "../creative-evil/evil.md", true)),
            "chapter_path_forbidden"
        );
        #[cfg(unix)]
        assert_eq!(
            daemon_code(resolve_guarded_path(&root, "link.md", true)),
            "chapter_path_forbidden"
        );
        assert_eq!(
            daemon_code(resolve_guarded_path(&root, "Works/w/Outlines/v2/ch01.md", false)),
            "ok"
        );

        // The async daemon wrapper delegates to the core async wrapper.
        assert!(
            resolve_guarded_path_async(
                root.clone(),
                "../creative-evil/evil.md".to_string(),
                true
            )
            .await
            .is_err()
        );
    }
}
