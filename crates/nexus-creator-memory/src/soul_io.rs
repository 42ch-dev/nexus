//! SOUL.md file I/O operations.
//!
//! Handles reading, writing, and creating SOUL.md files on disk.
//! Uses `nexus_home_layout` for path resolution.
//!
//! All public functions that accept a `creator_id` validate it at the top
//! to prevent path-traversal attacks (malicious IDs like `../../etc/`).

use crate::errors::MemoryError;
use crate::soul::SoulDocument;
use nexus_creator::is_valid_creator_id;
use std::path::{Path, PathBuf};

/// Validate that `creator_id` is safe to use in filesystem paths.
///
/// Rejects IDs containing path separators, `..` components, backslashes,
/// or control characters, and requires the standard `ctr_` prefix.
fn validate_creator_id(creator_id: &str) -> Result<(), MemoryError> {
    if is_valid_creator_id(creator_id) {
        Ok(())
    } else {
        Err(MemoryError::InvalidIdFormat(format!(
            "creator_id '{creator_id}' is not a valid CreatorId (must match ^ctr_[a-zA-Z0-9]+$ and contain no path separators or control characters)"
        )))
    }
}
#[must_use]
/// Resolve the SOUL.md path for a creator using the home layout.
///
/// # Panics (defense-in-depth)
///
/// This function does **not** validate `creator_id` on its own — callers
/// that reach this through the public API should already have passed
/// `validate_creator_id()`. If you call this directly with untrusted input,
/// run `validate_creator_id()` first.
pub fn soul_path(home: &Path, creator_id: &str) -> PathBuf {
    nexus_home_layout::creator_soul_md_path(home, creator_id)
}

/// Check if a SOUL.md exists for the given creator.
#[must_use]
pub fn exists(home: &Path, creator_id: &str) -> bool {
    // Existence check: silently return false for invalid IDs rather than
    // erroring, matching common "check then maybe create" patterns.
    if !is_valid_creator_id(creator_id) {
        return false;
    }
    soul_path(home, creator_id).exists()
}

/// Read and parse SOUL.md for a creator.
pub fn load(home: &Path, creator_id: &str) -> Result<SoulDocument, MemoryError> {
    validate_creator_id(creator_id)?;
    let path = soul_path(home, creator_id);
    if !path.exists() {
        return Err(MemoryError::SoulNotFound {
            creator_id: creator_id.to_string(),
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
/// Create a new SOUL.md for a creator. Fails if it already exists.
pub fn create(home: &Path, creator_id: &str) -> Result<SoulDocument, MemoryError> {
    validate_creator_id(creator_id)?;
    let path = soul_path(home, creator_id);
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
    let doc = SoulDocument::for_creator(creator_id);
    let content = doc.render();
    std::fs::write(&path, &content)
        .map_err(|e| MemoryError::ValidationError(format!("cannot write SOUL.md: {e}")))?;
    let mut loaded_doc = load(home, creator_id)?;
    loaded_doc.source_path = Some(path);
    Ok(loaded_doc)
}
///
/// # Errors
/// Returns `Err(MemoryError::...)` if validation fails.
/// Save an existing SOUL.md (overwrites). Must already exist.
pub fn save(home: &Path, creator_id: &str, doc: &SoulDocument) -> Result<(), MemoryError> {
    validate_creator_id(creator_id)?;
    let path = soul_path(home, creator_id);
    if !path.exists() {
        return Err(MemoryError::SoulNotFound {
            creator_id: creator_id.to_string(),
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
pub fn validate(home: &Path, creator_id: &str) -> Result<SoulDocument, MemoryError> {
    validate_creator_id(creator_id)?;
    let doc = load(home, creator_id)?;
    doc.validate()?;
    Ok(doc)
}
///
/// # Errors
/// Returns `Err(MemoryError::...)` if validation fails.
///
/// # Errors
/// Returns `Err(MemoryError::...)` if validation fails.
/// Delete SOUL.md for a creator.
pub fn delete(home: &Path, creator_id: &str) -> Result<(), MemoryError> {
    validate_creator_id(creator_id)?;
    let path = soul_path(home, creator_id);
    if !path.exists() {
        return Err(MemoryError::SoulNotFound {
            creator_id: creator_id.to_string(),
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
        let doc = create(&home, "ctr_test").unwrap();
        assert!(doc.personality.is_some());
        assert!(doc.experience.is_some());
        assert!(exists(&home, "ctr_test"));

        let loaded = load(&home, "ctr_test").unwrap();
        assert_eq!(
            loaded.frontmatter.creator_id.as_deref().unwrap(),
            "ctr_test"
        );
        cleanup(&home);
    }

    #[test]
    fn load_not_found() {
        let home = fake_home();
        let result = load(&home, "ctr_nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"), "err: {err}");
        cleanup(&home);
    }

    #[test]
    fn create_already_exists() {
        let home = fake_home();
        cleanup(&home);
        create(&home, "ctr_dup").unwrap();
        let result = create(&home, "ctr_dup");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("already exists"), "err: {err}");
        cleanup(&home);
    }

    #[test]
    fn save_update_and_reload() {
        let home = fake_home();
        cleanup(&home);
        create(&home, "ctr_save").unwrap();
        let mut doc = load(&home, "ctr_save").unwrap();
        doc.set_personality("Updated personality.".to_string());
        save(&home, "ctr_save", &doc).unwrap();
        let reloaded = load(&home, "ctr_save").unwrap();
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
        create(&home, "ctr_val").unwrap();
        assert!(validate(&home, "ctr_val").is_ok());
        cleanup(&home);
    }

    // ── R1: path traversal rejection tests ─────────────────────────────

    #[test]
    fn load_rejects_path_traversal() {
        let home = fake_home();
        let result = load(&home, "../../etc/passwd");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid ID format"), "err: {err}");
    }

    #[test]
    fn create_rejects_path_traversal() {
        let home = fake_home();
        cleanup(&home);
        let result = create(&home, "../../etc/passwd");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid ID format"), "err: {err}");
        cleanup(&home);
    }

    #[test]
    fn save_rejects_path_traversal() {
        let home = fake_home();
        let result = save(
            &home,
            "../../../tmp/evil",
            &SoulDocument::for_creator("ctr_legit"),
        );
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"),);
    }

    #[test]
    fn validate_rejects_path_traversal() {
        let home = fake_home();
        let result = validate(&home, "ctr_.._escape");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"),);
    }

    #[test]
    fn delete_rejects_path_traversal() {
        let home = fake_home();
        let result = delete(&home, "ctr_../evil");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"),);
    }

    #[test]
    fn exists_returns_false_for_invalid_id() {
        let home = fake_home();
        assert!(!exists(&home, "../../etc"));
        assert!(!exists(&home, "ctr_../escape"));
    }
}
