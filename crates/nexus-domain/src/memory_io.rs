//! Long-term memory file I/O operations.
//!
//! Handles reading, writing, listing, and deleting long-term memory
//! Markdown files on disk. Follows the same patterns as `soul_io.rs`.
//!
//! Memory files live at:
//! `~/.nexus42/creators/<creator_id>/memory/long-term/<slug>.md`
//!
//! All public functions that accept a `creator_id` validate it at the top
//! to prevent path-traversal attacks.

use crate::{is_valid_creator_id, DomainError, LongTermMemory};
use std::path::{Path, PathBuf};

/// Memory directory relative to the creator's home layout.
const MEMORY_SUBDIR: &str = "memory/long-term";

/// Validate that `creator_id` is safe to use in filesystem paths.
///
/// Rejects IDs containing path separators, `..` components, backslashes,
/// or control characters, and requires the standard `ctr_` prefix.
fn validate_creator_id(creator_id: &str) -> Result<(), DomainError> {
    if is_valid_creator_id(creator_id) {
        Ok(())
    } else {
        Err(DomainError::InvalidIdFormat(format!(
            "creator_id '{creator_id}' is not a valid CreatorId (must match ^ctr_[a-zA-Z0-9]+$ and contain no path separators or control characters)"
        )))
    }
}
#[must_use]
/// Resolve the memory directory path for a creator.
///
/// Returns: `<home>/.nexus42/creators/<creator_id>/memory/long-term/`
pub fn memory_dir(home: &Path, creator_id: &str) -> PathBuf {
    nexus_home_layout::nexus_root_from_home(home)
        .join("creators")
        .join(creator_id)
        .join(MEMORY_SUBDIR)
}

/// Resolve the full path for a memory file.
///
/// Returns: `<home>/.nexus42/creators/<creator_id>/memory/long-term/<slug>.md`
///
/// # Panics (defense-in-depth)
///
/// Does not validate `creator_id` or `slug` on its own — callers should
/// validate before calling. If called with malicious input, the path
/// may resolve outside the expected directory.
#[must_use]
pub fn memory_path(home: &Path, creator_id: &str, slug: &str) -> PathBuf {
    memory_dir(home, creator_id).join(format!("{slug}.md"))
}

/// List all memory slugs (filenames without `.md` extension) in the
/// memory directory for a creator.
///
/// Returns an empty list if the directory doesn't exist or contains
/// no `.md` files.
pub fn list_memories(home: &Path, creator_id: &str) -> Result<Vec<String>, DomainError> {
    validate_creator_id(creator_id)?;
    let dir = memory_dir(home, creator_id);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(&dir)
        .map_err(|e| DomainError::ValidationError(format!("cannot read memory directory: {e}")))?;
    let mut slugs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| {
            DomainError::ValidationError(format!("cannot read directory entry: {e}"))
        })?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "md" {
                    if let Some(stem) = path.file_stem() {
                        if let Some(name) = stem.to_str() {
                            slugs.push(name.to_string());
                        }
                    }
                }
            }
        }
    }
    slugs.sort();
    Ok(slugs)
}
///
/// # Errors
/// Returns `Err(DomainError::...)` if validation fails.
/// Load and parse a long-term memory file.
///
/// The file is read from `<memory_dir>/<slug>.md`, frontmatter is parsed,
/// and a `LongTermMemory` with the `source_path` set is returned.
pub fn load_memory(
    home: &Path,
    creator_id: &str,
    slug: &str,
) -> Result<LongTermMemory, DomainError> {
    validate_creator_id(creator_id)?;
    if !slug_is_safe(slug) {
        return Err(DomainError::ValidationError(format!(
            "slug '{}' is not path-safe (rejected: contains '..', '/', '\\', or control characters)",
            slug
        )));
    }
    let path = memory_path(home, creator_id, slug);
    if !path.exists() {
        return Err(DomainError::ValidationError(format!(
            "memory file not found: {}",
            path.display()
        )));
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| DomainError::ValidationError(format!("cannot read memory file: {e}")))?;
    let mut memory = LongTermMemory::parse(&content)
        .map_err(|e| DomainError::ValidationError(format!("cannot parse memory file: {e}")))?;
    memory.source_path = Some(path);
    Ok(memory)
}
///
/// # Errors
/// Returns `Err(DomainError::...)` if validation fails.
/// Save a long-term memory file to disk.
///
/// Creates the memory directory if it doesn't exist. Serializes the
/// frontmatter and body to the standard Markdown format and writes
/// to `<memory_dir>/<slug>.md`.
pub fn save_memory(
    home: &Path,
    creator_id: &str,
    slug: &str,
    memory: &LongTermMemory,
) -> Result<(), DomainError> {
    validate_creator_id(creator_id)?;
    if !slug_is_safe(slug) {
        return Err(DomainError::ValidationError(format!(
            "slug '{}' is not path-safe (rejected: contains '..', '/', '\\', or control characters)",
            slug
        )));
    }
    ensure_memory_dir(home, creator_id)?;
    let content = memory.render()?;
    let path = memory_path(home, creator_id, slug);
    std::fs::write(&path, &content)
        .map_err(|e| DomainError::ValidationError(format!("cannot write memory file: {e}")))?;
    Ok(())
}
///
/// # Errors
/// Returns `Err(DomainError::...)` if validation fails.
/// Delete a long-term memory file.
pub fn delete_memory(home: &Path, creator_id: &str, slug: &str) -> Result<(), DomainError> {
    validate_creator_id(creator_id)?;
    if !slug_is_safe(slug) {
        return Err(DomainError::ValidationError(format!(
            "slug '{}' is not path-safe (rejected: contains '..', '/', '\\', or control characters)",
            slug
        )));
    }
    let path = memory_path(home, creator_id, slug);
    if !path.exists() {
        return Err(DomainError::ValidationError(format!(
            "memory file not found: {}",
            path.display()
        )));
    }
    std::fs::remove_file(&path)
        .map_err(|e| DomainError::ValidationError(format!("cannot delete memory file: {e}")))?;
    Ok(())
}

/// Check if a slug is path-safe (no `..`, `/`, `\`, null bytes, or control chars).
///
/// Re-exported from `long_term_memory::slug_is_safe` for convenience.
#[must_use]
///
/// # Errors
/// Returns `Err(DomainError::...)` if validation fails.
pub fn slug_is_safe(slug: &str) -> bool {
    crate::long_term_memory::slug_is_safe(slug)
}
///
/// # Errors
/// Returns `Err(DomainError::...)` if validation fails.
/// Ensure the memory directory exists for a creator.
///
/// Creates `<home>/.nexus42/creators/<creator_id>/memory/long-term/`
/// and all parent directories if they don't exist.
pub fn ensure_memory_dir(home: &Path, creator_id: &str) -> Result<(), DomainError> {
    validate_creator_id(creator_id)?;
    let dir = memory_dir(home, creator_id);
    std::fs::create_dir_all(&dir).map_err(|e| {
        DomainError::ValidationError(format!("cannot create memory directory: {e}"))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fake_home() -> PathBuf {
        let id = std::thread::current()
            .name()
            .unwrap_or("unknown")
            .replace("::", "_");
        PathBuf::from(format!("/tmp/test_memory_io_{id}"))
    }

    fn cleanup(home: &Path) {
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let home = fake_home();
        cleanup(&home);
        let mut mem = LongTermMemory::new("story_summary");
        mem.set_body("Chapter 1 analysis: the protagonist discovers the truth.");
        mem.add_source_session("sess_001");

        save_memory(&home, "ctr_test", "chapter1-analysis", &mem).unwrap();
        let loaded = load_memory(&home, "ctr_test", "chapter1-analysis").unwrap();

        assert_eq!(loaded.frontmatter.memory_id, mem.frontmatter.memory_id);
        assert_eq!(loaded.frontmatter.memory_kind, "story_summary");
        assert_eq!(loaded.body.trim(), mem.body.trim());
        assert_eq!(loaded.frontmatter.source_session_ids, vec!["sess_001"]);
        assert!(loaded.source_path.is_some());
        cleanup(&home);
    }

    #[test]
    fn list_memories_empty() {
        let home = fake_home();
        cleanup(&home);
        let slugs = list_memories(&home, "ctr_test").unwrap();
        assert!(slugs.is_empty());
        cleanup(&home);
    }

    #[test]
    fn list_memories_after_save() {
        let home = fake_home();
        cleanup(&home);
        let mem1 = LongTermMemory::new("story_summary");
        let mem2 = LongTermMemory::new("character_note");

        save_memory(&home, "ctr_test", "alpha-memory", &mem1).unwrap();
        save_memory(&home, "ctr_test", "beta-memory", &mem2).unwrap();

        let slugs = list_memories(&home, "ctr_test").unwrap();
        assert_eq!(slugs, vec!["alpha-memory", "beta-memory"]);
        cleanup(&home);
    }

    #[test]
    fn delete_memory_removes_file() {
        let home = fake_home();
        cleanup(&home);
        let mem = LongTermMemory::new("story_summary");
        save_memory(&home, "ctr_test", "to-delete", &mem).unwrap();

        assert!(load_memory(&home, "ctr_test", "to-delete").is_ok());
        delete_memory(&home, "ctr_test", "to-delete").unwrap();
        assert!(load_memory(&home, "ctr_test", "to-delete").is_err());
        cleanup(&home);
    }

    #[test]
    fn load_not_found() {
        let home = fake_home();
        let result = load_memory(&home, "ctr_test", "nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"), "err: {err}");
    }

    #[test]
    fn delete_not_found() {
        let home = fake_home();
        let result = delete_memory(&home, "ctr_test", "nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"), "err: {err}");
    }

    #[test]
    fn ensure_memory_dir_creates_dirs() {
        let home = fake_home();
        cleanup(&home);
        ensure_memory_dir(&home, "ctr_mkdir").unwrap();
        let dir = memory_dir(&home, "ctr_mkdir");
        assert!(dir.exists());
        cleanup(&home);
    }

    #[test]
    fn memory_dir_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            memory_dir(&home, "ctr_test"),
            PathBuf::from("/h/.nexus42/creators/ctr_test/memory/long-term")
        );
    }

    #[test]
    fn memory_path_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            memory_path(&home, "ctr_test", "my-slug"),
            PathBuf::from("/h/.nexus42/creators/ctr_test/memory/long-term/my-slug.md")
        );
    }

    // ── Path traversal rejection tests ─────────────────────────────

    #[test]
    fn load_rejects_path_traversal_creator_id() {
        let home = fake_home();
        let result = load_memory(&home, "../../etc/passwd", "safe-slug");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"));
    }

    #[test]
    fn save_rejects_path_traversal_creator_id() {
        let home = fake_home();
        let mem = LongTermMemory::new("story_summary");
        let result = save_memory(&home, "../../etc", "slug", &mem);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"));
    }

    #[test]
    fn delete_rejects_path_traversal_creator_id() {
        let home = fake_home();
        let result = delete_memory(&home, "ctr_../evil", "slug");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"));
    }

    #[test]
    fn list_rejects_path_traversal_creator_id() {
        let home = fake_home();
        let result = list_memories(&home, "../../etc");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"));
    }

    #[test]
    fn load_rejects_unsafe_slug() {
        let home = fake_home();
        let result = load_memory(&home, "ctr_test", "../etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not path-safe"));
    }

    #[test]
    fn save_rejects_unsafe_slug() {
        let home = fake_home();
        let mem = LongTermMemory::new("story_summary");
        let result = save_memory(&home, "ctr_test", "../evil", &mem);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not path-safe"));
    }

    #[test]
    fn delete_rejects_unsafe_slug() {
        let home = fake_home();
        let result = delete_memory(&home, "ctr_test", "..\\escape");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not path-safe"));
    }

    #[test]
    fn ensure_dir_rejects_path_traversal_creator_id() {
        let home = fake_home();
        let result = ensure_memory_dir(&home, "ctr_../escape");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"));
    }

    #[test]
    fn update_memory_overwrites() {
        let home = fake_home();
        cleanup(&home);
        let mut mem = LongTermMemory::new("story_summary");
        mem.set_body("Original content");
        save_memory(&home, "ctr_test", "update-test", &mem).unwrap();

        // Load, modify, save
        let mut loaded = load_memory(&home, "ctr_test", "update-test").unwrap();
        loaded.set_body("Updated content");
        save_memory(&home, "ctr_test", "update-test", &loaded).unwrap();

        let reloaded = load_memory(&home, "ctr_test", "update-test").unwrap();
        assert!(reloaded.body.contains("Updated content"));
        cleanup(&home);
    }

    #[test]
    fn save_creates_parent_dirs() {
        let home = fake_home();
        cleanup(&home);
        // Don't call ensure_memory_dir explicitly; save should handle it
        let mem = LongTermMemory::new("custom");
        assert!(save_memory(&home, "ctr_new", "auto-dir", &mem).is_ok());
        assert!(memory_path(&home, "ctr_new", "auto-dir").exists());
        cleanup(&home);
    }
}
