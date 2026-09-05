//! SOUL.md file I/O operations.
//!
//! Handles reading, writing, and creating SOUL.md files on disk.
//! Path resolution dispatches through the closed [`MemoryBearerRef`]
//! (v1.184 P3): the Creator arm reproduces the legacy layout and bytes;
//! the Character arm uses the `nexus-home-layout` Character paths.
//!
//! All public functions validate the bearer at the top to prevent
//! path-traversal attacks (malicious ids like `../../etc/`).

use crate::bearer::MemoryBearerRef;
use crate::errors::MemoryError;
use crate::soul::SoulDocument;
use std::path::{Path, PathBuf};
#[must_use]
/// Resolve the SOUL.md path for a bearer using the home layout.
///
/// # Panics (defense-in-depth)
///
/// This function does **not** validate the bearer on its own — callers that
/// reach this through the public API should already have passed
/// `bearer.validate()`. If you call this directly with untrusted input, run
/// `bearer.validate()` first.
pub fn soul_path(home: &Path, bearer: MemoryBearerRef<'_>) -> PathBuf {
    bearer.soul_path(home)
}

/// Check if a SOUL.md exists for the given bearer.
#[must_use]
pub fn exists(home: &Path, bearer: MemoryBearerRef<'_>) -> bool {
    // Existence check: silently return false for invalid ids rather than
    // erroring, matching common "check then maybe create" patterns.
    if bearer.validate().is_err() {
        return false;
    }
    soul_path(home, bearer).exists()
}

/// Read and parse SOUL.md for a bearer.
pub fn load(home: &Path, bearer: MemoryBearerRef<'_>) -> Result<SoulDocument, MemoryError> {
    bearer.validate()?;
    let path = soul_path(home, bearer);
    if !path.exists() {
        return Err(MemoryError::SoulNotFound {
            creator_id: bearer.id().to_string(),
            path: path.display().to_string(),
        });
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| MemoryError::ValidationError(format!("cannot read SOUL.md: {e}")))?;
    let mut doc = SoulDocument::parse(&content)
        .map_err(|e| MemoryError::ValidationError(format!("cannot parse SOUL.md: {e}")))?;
    doc.source_path = Some(path);
    Ok(doc)
}
///
/// # Errors
/// Returns `Err(MemoryError::...)` if validation fails.
/// Create a new SOUL.md for a bearer. Fails if it already exists.
pub fn create(home: &Path, bearer: MemoryBearerRef<'_>) -> Result<SoulDocument, MemoryError> {
    bearer.validate()?;
    let path = soul_path(home, bearer);
    if path.exists() {
        return Err(MemoryError::ValidationError(format!(
            "SOUL.md already exists at {}",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| MemoryError::ValidationError(format!("cannot create creator dir: {e}")))?;
    }
    let doc = SoulDocument::for_creator(bearer.id());
    let content = doc.render();
    std::fs::write(&path, &content)
        .map_err(|e| MemoryError::ValidationError(format!("cannot write SOUL.md: {e}")))?;
    let mut loaded_doc = load(home, bearer)?;
    loaded_doc.source_path = Some(path);
    Ok(loaded_doc)
}
///
/// # Errors
/// Returns `Err(MemoryError::...)` if validation fails.
/// Save an existing SOUL.md (overwrites). Must already exist.
pub fn save(
    home: &Path,
    bearer: MemoryBearerRef<'_>,
    doc: &SoulDocument,
) -> Result<(), MemoryError> {
    bearer.validate()?;
    let path = soul_path(home, bearer);
    if !path.exists() {
        return Err(MemoryError::SoulNotFound {
            creator_id: bearer.id().to_string(),
            path: path.display().to_string(),
        });
    }
    let content = doc.render();
    std::fs::write(&path, content)
        .map_err(|e| MemoryError::ValidationError(format!("cannot write SOUL.md: {e}")))?;
    Ok(())
}
///
/// # Errors
/// Returns `Err(MemoryError::...)` if validation fails.
/// Validate an existing SOUL.md (check sections and return parsed doc).
pub fn validate(home: &Path, bearer: MemoryBearerRef<'_>) -> Result<SoulDocument, MemoryError> {
    bearer.validate()?;
    let doc = load(home, bearer)?;
    doc.validate()?;
    Ok(doc)
}
///
/// # Errors
/// Returns `Err(MemoryError::...)` if validation fails.
///
/// # Errors
/// Returns `Err(MemoryError::...)` if validation fails.
/// Delete SOUL.md for a bearer.
pub fn delete(home: &Path, bearer: MemoryBearerRef<'_>) -> Result<(), MemoryError> {
    bearer.validate()?;
    let path = soul_path(home, bearer);
    if !path.exists() {
        return Err(MemoryError::SoulNotFound {
            creator_id: bearer.id().to_string(),
            path: path.display().to_string(),
        });
    }
    std::fs::remove_file(&path)
        .map_err(|e| MemoryError::ValidationError(format!("cannot delete SOUL.md: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fake_home() -> PathBuf {
        // Each test gets a unique temp dir to avoid parallel test races.
        let id = std::thread::current()
            .name()
            .unwrap_or("unknown")
            .replace("::", "_");
        PathBuf::from(format!("/tmp/test_soul_io_{id}"))
    }

    fn cleanup(home: &Path) {
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn create_load_roundtrip() {
        let home = fake_home();
        cleanup(&home);
        let doc = create(&home, MemoryBearerRef::Creator("ctr_test")).unwrap();
        assert!(doc.personality.is_some());
        assert!(doc.experience.is_some());
        assert!(exists(&home, MemoryBearerRef::Creator("ctr_test")));

        let loaded = load(&home, MemoryBearerRef::Creator("ctr_test")).unwrap();
        assert_eq!(
            loaded.frontmatter.creator_id.as_deref().unwrap(),
            "ctr_test"
        );
        cleanup(&home);
    }

    #[test]
    fn load_not_found() {
        let home = fake_home();
        let result = load(&home, MemoryBearerRef::Creator("ctr_nonexistent"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"), "err: {err}");
        cleanup(&home);
    }

    #[test]
    fn create_already_exists() {
        let home = fake_home();
        cleanup(&home);
        create(&home, MemoryBearerRef::Creator("ctr_dup")).unwrap();
        let result = create(&home, MemoryBearerRef::Creator("ctr_dup"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("already exists"), "err: {err}");
        cleanup(&home);
    }

    #[test]
    fn save_update_and_reload() {
        let home = fake_home();
        cleanup(&home);
        create(&home, MemoryBearerRef::Creator("ctr_save")).unwrap();
        let mut doc = load(&home, MemoryBearerRef::Creator("ctr_save")).unwrap();
        doc.set_personality("Updated personality.".to_string());
        save(&home, MemoryBearerRef::Creator("ctr_save"), &doc).unwrap();
        let reloaded = load(&home, MemoryBearerRef::Creator("ctr_save")).unwrap();
        assert_eq!(
            reloaded.personality.as_deref().unwrap().trim(),
            "Updated personality."
        );
        cleanup(&home);
    }

    #[test]
    fn validate_ok() {
        let home = fake_home();
        cleanup(&home);
        create(&home, MemoryBearerRef::Creator("ctr_val")).unwrap();
        assert!(validate(&home, MemoryBearerRef::Creator("ctr_val")).is_ok());
        cleanup(&home);
    }

    // ── R1: path traversal rejection tests ─────────────────────────────

    #[test]
    fn load_rejects_path_traversal() {
        let home = fake_home();
        let result = load(&home, MemoryBearerRef::Creator("../../etc/passwd"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid ID format"), "err: {err}");
    }

    #[test]
    fn create_rejects_path_traversal() {
        let home = fake_home();
        cleanup(&home);
        let result = create(&home, MemoryBearerRef::Creator("../../etc/passwd"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid ID format"), "err: {err}");
        cleanup(&home);
    }

    #[test]
    fn save_rejects_path_traversal() {
        let home = fake_home();
        let result = save(&home, MemoryBearerRef::Creator("../../../tmp/evil"), &SoulDocument::for_creator("ctr_legit"));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"),);
    }

    #[test]
    fn validate_rejects_path_traversal() {
        let home = fake_home();
        let result = validate(&home, MemoryBearerRef::Creator("ctr_.._escape"));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"),);
    }

    #[test]
    fn delete_rejects_path_traversal() {
        let home = fake_home();
        let result = delete(&home, MemoryBearerRef::Creator("ctr_../evil"));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"),);
    }

    #[test]
    fn exists_returns_false_for_invalid_id() {
        let home = fake_home();
        assert!(!exists(&home, MemoryBearerRef::Creator("../../etc")));
        assert!(!exists(&home, MemoryBearerRef::Creator("ctr_../escape")));
    }
}
