//! SOUL.md file I/O operations.
//!
//! Handles reading, writing, and creating SOUL.md files on disk.
//! Uses `nexus_home_layout` for path resolution.

use crate::{DomainError, SoulDocument};
use std::path::{Path, PathBuf};

/// Resolve the SOUL.md path for a creator using the home layout.
pub fn soul_path(home: &Path, creator_id: &str) -> PathBuf {
    nexus_home_layout::creator_soul_md_path(home, creator_id)
}

/// Check if a SOUL.md exists for the given creator.
pub fn exists(home: &Path, creator_id: &str) -> bool {
    soul_path(home, creator_id).exists()
}

/// Read and parse SOUL.md for a creator.
pub fn load(home: &Path, creator_id: &str) -> Result<SoulDocument, DomainError> {
    let path = soul_path(home, creator_id);
    if !path.exists() {
        return Err(DomainError::SoulNotFound {
            creator_id: creator_id.to_string(),
            path: path.display().to_string(),
        });
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| DomainError::ValidationError(format!("cannot read SOUL.md: {e}")))?;
    let mut doc = SoulDocument::parse(&content)
        .map_err(|e| DomainError::ValidationError(format!("cannot parse SOUL.md: {e}")))?;
    doc.source_path = Some(path);
    Ok(doc)
}

/// Create a new SOUL.md for a creator. Fails if it already exists.
pub fn create(home: &Path, creator_id: &str) -> Result<SoulDocument, DomainError> {
    let path = soul_path(home, creator_id);
    if path.exists() {
        return Err(DomainError::ValidationError(format!(
            "SOUL.md already exists at {}",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| DomainError::ValidationError(format!("cannot create creator dir: {e}")))?;
    }
    let doc = SoulDocument::for_creator(creator_id);
    let content = doc.render();
    std::fs::write(&path, content)
        .map_err(|e| DomainError::ValidationError(format!("cannot write SOUL.md: {e}")))?;
    let mut loaded_doc = load(home, creator_id)?;
    loaded_doc.source_path = Some(path);
    Ok(loaded_doc)
}

/// Save an existing SOUL.md (overwrites). Must already exist.
pub fn save(home: &Path, creator_id: &str, doc: &SoulDocument) -> Result<(), DomainError> {
    let path = soul_path(home, creator_id);
    if !path.exists() {
        return Err(DomainError::SoulNotFound {
            creator_id: creator_id.to_string(),
            path: path.display().to_string(),
        });
    }
    let content = doc.render();
    std::fs::write(&path, content)
        .map_err(|e| DomainError::ValidationError(format!("cannot write SOUL.md: {e}")))?;
    Ok(())
}

/// Validate an existing SOUL.md (check sections and return parsed doc).
pub fn validate(home: &Path, creator_id: &str) -> Result<SoulDocument, DomainError> {
    let doc = load(home, creator_id)?;
    doc.validate()?;
    Ok(doc)
}

/// Delete SOUL.md for a creator.
pub fn delete(home: &Path, creator_id: &str) -> Result<(), DomainError> {
    let path = soul_path(home, creator_id);
    if !path.exists() {
        return Err(DomainError::SoulNotFound {
            creator_id: creator_id.to_string(),
            path: path.display().to_string(),
        });
    }
    std::fs::remove_file(&path)
        .map_err(|e| DomainError::ValidationError(format!("cannot delete SOUL.md: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fake_home() -> PathBuf {
        PathBuf::from("/tmp/test_soul_io")
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
}
