//! Long-term memory file I/O operations.
//!
//! Handles reading, writing, listing, and deleting long-term memory
//! Markdown files on disk. Path resolution dispatches through the closed
//! [`MemoryBearerRef`] (v1.184 P3).
//!
//! Memory files live at:
//! `~/.nexus42/creators/<creator_id>/memory/long-term/<slug>.md` (Creator)
//! or
//! `~/.nexus42/creators/<owner_creator_id>/characters/<character_id>/memory/long-term/<slug>.md`
//! (Character).
//!
//! All public functions validate the bearer at the top to prevent
//! path-traversal attacks.

use crate::bearer::MemoryBearerRef;
use crate::errors::MemoryError;
use crate::long_term_memory::LongTermMemory;
use std::path::{Path, PathBuf};

#[must_use]
/// Resolve the memory directory path for a bearer.
///
/// Returns `<home>/.nexus42/creators/<creator_id>/memory/long-term/` for the
/// Creator arm, or the Character arm's `…/characters/<character_id>/memory/long-term/`.
pub fn memory_dir(home: &Path, bearer: MemoryBearerRef<'_>) -> PathBuf {
    bearer.long_term_memory_dir(home)
}

/// Resolve the full path for a memory file.
///
/// Returns `<memory_dir>/<slug>.md`.
///
/// # Panics (defense-in-depth)
///
/// Does not validate the bearer or `slug` on its own — callers should
/// validate before calling (`slug_is_safe` / `bearer.validate()`).
#[must_use]
pub fn memory_path(home: &Path, bearer: MemoryBearerRef<'_>, slug: &str) -> PathBuf {
    bearer.long_term_memory_path(home, slug)
}

/// List all memory slugs (filenames without `.md` extension) in the
/// memory directory for a bearer.
///
/// Returns an empty list only when the directory is genuinely absent
/// (`ErrorKind::NotFound`) or contains no `.md` files. A permission, I/O, or
/// malformed-path failure is propagated as a [`MemoryError`], so callers
/// (e.g. the Character mind projection) can distinguish honest-empty absence
/// from an inability to read the directory and fail closed before host launch.
///
/// # Errors
///
/// Returns [`MemoryError`] on a permission, I/O, or malformed-path failure.
pub fn list_memories(home: &Path, bearer: MemoryBearerRef<'_>) -> Result<Vec<String>, MemoryError> {
    let dir = bearer.validated_long_term_memory_dir(home)?;
    // Attempt the read directly rather than gating on `Path::exists()`, which
    // returns false both for a missing path and for an unreadable path. A
    // missing directory is the only honest-empty case; metadata/permission
    // errors must surface to the caller.
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(MemoryError::ValidationError(format!(
                "cannot read memory directory: {e}"
            )));
        }
    };
    let mut slugs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| {
            MemoryError::ValidationError(format!("cannot read directory entry: {e}"))
        })?;
        let path = entry.path();
        // Resolve the entry type with a result-bearing metadata operation that
        // follows symlinks (matching the previous `Path::is_file()` semantics).
        // A per-entry permission/I/O/metadata failure is propagated rather than
        // silently dropping the file; an entry that vanished between `read_dir`
        // and this stat is treated as ordinary absence (skipped), and a
        // non-file entry (directory, FIFO, socket) remains excluded.
        let metadata = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(MemoryError::ValidationError(format!(
                    "cannot stat memory entry: {e}"
                )));
            }
        };
        if !metadata.is_file() {
            continue;
        }
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
    slugs.sort();
    Ok(slugs)
}
///
/// # Errors
/// Returns `Err(MemoryError::...)` if validation fails.
/// Load and parse a long-term memory file.
///
/// The file is read from `<memory_dir>/<slug>.md`, frontmatter is parsed,
/// and a `LongTermMemory` with the `source_path` set is returned.
pub fn load_memory(
    home: &Path,
    bearer: MemoryBearerRef<'_>,
    slug: &str,
) -> Result<LongTermMemory, MemoryError> {
    bearer.validate()?;
    if !slug_is_safe(slug) {
        return Err(MemoryError::ValidationError(format!(
            "slug '{slug}' is not path-safe (rejected: contains '..', '/', '\\', or control characters)"
        )));
    }
    let path = memory_path(home, bearer, slug);
    if !path.exists() {
        return Err(MemoryError::ValidationError(format!(
            "memory file not found: {}",
            path.display()
        )));
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| MemoryError::ValidationError(format!("cannot read memory file: {e}")))?;
    let mut memory = LongTermMemory::parse(&content)
        .map_err(|e| MemoryError::ValidationError(format!("cannot parse memory file: {e}")))?;
    memory.source_path = Some(path);
    Ok(memory)
}
///
/// # Errors
/// Returns `Err(MemoryError::...)` if validation fails.
/// Save a long-term memory file to disk.
///
/// Creates the memory directory if it doesn't exist. Serializes the
/// frontmatter and body to the standard Markdown format and writes
/// to `<memory_dir>/<slug>.md`.
///
/// Uses atomic write (write to `.tmp` then rename) to prevent corruption
/// on crash or disk-full (R-V133P4-05).
pub fn save_memory(
    home: &Path,
    bearer: MemoryBearerRef<'_>,
    slug: &str,
    memory: &LongTermMemory,
) -> Result<(), MemoryError> {
    bearer.validate()?;
    if !slug_is_safe(slug) {
        return Err(MemoryError::ValidationError(format!(
            "slug '{slug}' is not path-safe (rejected: contains '..', '/', '\\', or control characters)"
        )));
    }
    ensure_memory_dir(home, bearer)?;
    let content = memory.render()?;
    let path = memory_path(home, bearer, slug);

    // R-V133P4-05: Atomic write — write to temp file, then rename.
    // POSIX rename is atomic within a filesystem, preventing partial writes.
    let tmp_path = path.with_extension("md.tmp");
    std::fs::write(&tmp_path, &content)
        .map_err(|e| MemoryError::ValidationError(format!("cannot write temp memory file: {e}")))?;
    std::fs::rename(&tmp_path, &path).map_err(|e| {
        // Clean up temp file on rename failure (best effort).
        let _ = std::fs::remove_file(&tmp_path);
        MemoryError::ValidationError(format!("cannot rename temp memory file: {e}"))
    })?;
    Ok(())
}
///
/// # Errors
/// Returns `Err(MemoryError::...)` if validation fails.
/// Delete a long-term memory file.
pub fn delete_memory(
    home: &Path,
    bearer: MemoryBearerRef<'_>,
    slug: &str,
) -> Result<(), MemoryError> {
    bearer.validate()?;
    if !slug_is_safe(slug) {
        return Err(MemoryError::ValidationError(format!(
            "slug '{slug}' is not path-safe (rejected: contains '..', '/', '\\', or control characters)"
        )));
    }
    let path = memory_path(home, bearer, slug);
    if !path.exists() {
        return Err(MemoryError::ValidationError(format!(
            "memory file not found: {}",
            path.display()
        )));
    }
    std::fs::remove_file(&path)
        .map_err(|e| MemoryError::ValidationError(format!("cannot delete memory file: {e}")))?;
    Ok(())
}

/// Check if a slug is path-safe (no `..`, `/`, `\`, null bytes, or control chars).
///
/// Re-exported from `long_term_memory::slug_is_safe` for convenience.
#[must_use]
///
/// # Errors
/// Returns `Err(MemoryError::...)` if validation fails.
pub fn slug_is_safe(slug: &str) -> bool {
    crate::long_term_memory::slug_is_safe(slug)
}
///
/// # Errors
/// Returns `Err(MemoryError::...)` if validation fails.
/// Ensure the memory directory exists for a bearer.
///
/// Creates the bearer's `memory/long-term/` directory (and all parents) if
/// it doesn't exist.
pub fn ensure_memory_dir(home: &Path, bearer: MemoryBearerRef<'_>) -> Result<(), MemoryError> {
    bearer.validate()?;
    let dir = memory_dir(home, bearer);
    std::fs::create_dir_all(&dir).map_err(|e| {
        MemoryError::ValidationError(format!("cannot create memory directory: {e}"))
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

        save_memory(
            &home,
            MemoryBearerRef::Creator("ctr_test"),
            "chapter1-analysis",
            &mem,
        )
        .unwrap();
        let loaded = load_memory(
            &home,
            MemoryBearerRef::Creator("ctr_test"),
            "chapter1-analysis",
        )
        .unwrap();

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
        let slugs = list_memories(&home, MemoryBearerRef::Creator("ctr_test")).unwrap();
        assert!(slugs.is_empty());
        cleanup(&home);
    }

    #[test]
    fn list_memories_after_save() {
        let home = fake_home();
        cleanup(&home);
        let mem1 = LongTermMemory::new("story_summary");
        let mem2 = LongTermMemory::new("character_note");

        save_memory(
            &home,
            MemoryBearerRef::Creator("ctr_test"),
            "alpha-memory",
            &mem1,
        )
        .unwrap();
        save_memory(
            &home,
            MemoryBearerRef::Creator("ctr_test"),
            "beta-memory",
            &mem2,
        )
        .unwrap();

        let slugs = list_memories(&home, MemoryBearerRef::Creator("ctr_test")).unwrap();
        assert_eq!(slugs, vec!["alpha-memory", "beta-memory"]);
        cleanup(&home);
    }

    #[test]
    fn delete_memory_removes_file() {
        let home = fake_home();
        cleanup(&home);
        let mem = LongTermMemory::new("story_summary");
        save_memory(
            &home,
            MemoryBearerRef::Creator("ctr_test"),
            "to-delete",
            &mem,
        )
        .unwrap();

        assert!(load_memory(&home, MemoryBearerRef::Creator("ctr_test"), "to-delete").is_ok());
        delete_memory(&home, MemoryBearerRef::Creator("ctr_test"), "to-delete").unwrap();
        assert!(load_memory(&home, MemoryBearerRef::Creator("ctr_test"), "to-delete").is_err());
        cleanup(&home);
    }

    #[test]
    fn load_not_found() {
        let home = fake_home();
        let result = load_memory(&home, MemoryBearerRef::Creator("ctr_test"), "nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"), "err: {err}");
    }

    #[test]
    fn delete_not_found() {
        let home = fake_home();
        let result = delete_memory(&home, MemoryBearerRef::Creator("ctr_test"), "nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"), "err: {err}");
    }

    #[test]
    fn list_memories_missing_dir_is_honest_empty() {
        let home = fake_home();
        cleanup(&home);
        // Never create the memory directory: genuine absence is honest-empty.
        let slugs = list_memories(&home, MemoryBearerRef::Creator("ctr_test")).unwrap();
        assert!(slugs.is_empty());
        cleanup(&home);
    }

    #[test]
    fn list_memories_propagates_non_notfound_read_error() {
        let home = fake_home();
        cleanup(&home);
        // Place a regular file where the memory directory should be so
        // read_dir fails with NotADirectory (not NotFound) — a metadata/read
        // failure must propagate rather than be treated as absence.
        let dir = memory_dir(&home, MemoryBearerRef::Creator("ctr_test"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::remove_dir(&dir).unwrap();
        std::fs::write(&dir, "not a directory").unwrap();

        let result = list_memories(&home, MemoryBearerRef::Creator("ctr_test"));
        assert!(
            result.is_err(),
            "a non-directory/path metadata failure must propagate"
        );
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cannot read memory directory"), "err: {err}");
        cleanup(&home);
    }

    #[test]
    fn list_memories_excludes_non_file_entries() {
        let home = fake_home();
        cleanup(&home);
        let mut mem = LongTermMemory::new("story_summary");
        mem.set_body("Body.");
        save_memory(&home, MemoryBearerRef::Creator("ctr_test"), "keep-me", &mem).unwrap();

        // A directory named like a memory file and a non-md regular file must
        // both be excluded; only the regular `.md` file is collected.
        let dir = memory_dir(&home, MemoryBearerRef::Creator("ctr_test"));
        std::fs::create_dir_all(dir.join("subdir.md")).unwrap();
        std::fs::write(dir.join("note.txt"), "not a memory").unwrap();

        let slugs = list_memories(&home, MemoryBearerRef::Creator("ctr_test")).unwrap();
        assert_eq!(slugs, vec!["keep-me"]);
        cleanup(&home);
    }

    #[test]
    fn list_memories_propagates_per_entry_metadata_error() {
        let home = fake_home();
        cleanup(&home);
        let dir = memory_dir(&home, MemoryBearerRef::Creator("ctr_test"));
        std::fs::create_dir_all(&dir).unwrap();

        // A self-loop symlink makes the following `fs::metadata` fail with a
        // non-NotFound error (filesystem loop), which must propagate rather
        // than silently drop the entry.
        #[cfg(unix)]
        std::os::unix::fs::symlink("loop_self", dir.join("loop_self")).unwrap();

        #[cfg(unix)]
        {
            let result = list_memories(&home, MemoryBearerRef::Creator("ctr_test"));
            assert!(
                result.is_err(),
                "a per-entry metadata failure must propagate"
            );
            let err = result.unwrap_err().to_string();
            assert!(err.contains("cannot stat memory entry"), "err: {err}");
        }
        #[cfg(not(unix))]
        cleanup(&home);
        cleanup(&home);
    }

    #[test]
    fn ensure_memory_dir_creates_dirs() {
        let home = fake_home();
        cleanup(&home);
        ensure_memory_dir(&home, MemoryBearerRef::Creator("ctr_mkdir")).unwrap();
        let dir = memory_dir(&home, MemoryBearerRef::Creator("ctr_mkdir"));
        assert!(dir.exists());
        cleanup(&home);
    }

    #[test]
    fn memory_dir_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            memory_dir(&home, MemoryBearerRef::Creator("ctr_test")),
            PathBuf::from("/h/.nexus42/creators/ctr_test/memory/long-term")
        );
    }

    #[test]
    fn memory_path_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            memory_path(&home, MemoryBearerRef::Creator("ctr_test"), "my-slug"),
            PathBuf::from("/h/.nexus42/creators/ctr_test/memory/long-term/my-slug.md")
        );
    }

    // ── Path traversal rejection tests ─────────────────────────────

    #[test]
    fn load_rejects_path_traversal_creator_id() {
        let home = fake_home();
        let result = load_memory(
            &home,
            MemoryBearerRef::Creator("../../etc/passwd"),
            "safe-slug",
        );
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
        let result = save_memory(&home, MemoryBearerRef::Creator("../../etc"), "slug", &mem);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"));
    }

    #[test]
    fn delete_rejects_path_traversal_creator_id() {
        let home = fake_home();
        let result = delete_memory(&home, MemoryBearerRef::Creator("ctr_../evil"), "slug");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"));
    }

    #[test]
    fn list_rejects_path_traversal_creator_id() {
        let home = fake_home();
        let result = list_memories(&home, MemoryBearerRef::Creator("../../etc"));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"));
    }

    #[test]
    fn load_rejects_unsafe_slug() {
        let home = fake_home();
        let result = load_memory(&home, MemoryBearerRef::Creator("ctr_test"), "../etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not path-safe"));
    }

    #[test]
    fn save_rejects_unsafe_slug() {
        let home = fake_home();
        let mem = LongTermMemory::new("story_summary");
        let result = save_memory(&home, MemoryBearerRef::Creator("ctr_test"), "../evil", &mem);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not path-safe"));
    }

    #[test]
    fn delete_rejects_unsafe_slug() {
        let home = fake_home();
        let result = delete_memory(&home, MemoryBearerRef::Creator("ctr_test"), "..\\escape");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not path-safe"));
    }

    #[test]
    fn ensure_dir_rejects_path_traversal_creator_id() {
        let home = fake_home();
        let result = ensure_memory_dir(&home, MemoryBearerRef::Creator("ctr_../escape"));
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
        save_memory(
            &home,
            MemoryBearerRef::Creator("ctr_test"),
            "update-test",
            &mem,
        )
        .unwrap();

        // Load, modify, save
        let mut loaded =
            load_memory(&home, MemoryBearerRef::Creator("ctr_test"), "update-test").unwrap();
        loaded.set_body("Updated content");
        save_memory(
            &home,
            MemoryBearerRef::Creator("ctr_test"),
            "update-test",
            &loaded,
        )
        .unwrap();

        let reloaded =
            load_memory(&home, MemoryBearerRef::Creator("ctr_test"), "update-test").unwrap();
        assert!(reloaded.body.contains("Updated content"));
        cleanup(&home);
    }

    #[test]
    fn save_creates_parent_dirs() {
        let home = fake_home();
        cleanup(&home);
        // Don't call ensure_memory_dir explicitly; save should handle it
        let mem = LongTermMemory::new("custom");
        assert!(save_memory(&home, MemoryBearerRef::Creator("ctr_new"), "auto-dir", &mem).is_ok());
        assert!(memory_path(&home, MemoryBearerRef::Creator("ctr_new"), "auto-dir").exists());
        cleanup(&home);
    }
}
