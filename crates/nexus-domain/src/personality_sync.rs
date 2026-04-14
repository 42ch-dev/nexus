//! Personality push-down: SOUL → long-term memory sync.
//!
//! When a SOUL document's `## Personality` section is saved or modified,
//! the content is pushed down to a local personality-type long-term memory
//! file (`personality-core.md` in the creator's memory directory).
//!
//! This implements spec §4.2 (personality track → memory projection).
//! In V1.2, this is local-only (no sync outbox — that's T6/WS4).
//!
//! Key rules:
//! - SOUL is the anchor; its next push always wins.
//! - Personality is user-written, not session-derived → `source_session_ids` is empty.
//! - Re-push overwrites any existing personality memory.

use crate::memory_io::{load_memory, save_memory};
use crate::{DomainError, LongTermMemory, SoulDocument};
use std::path::Path;

/// Slug used for the personality memory file.
pub const PERSONALITY_MEMORY_SLUG: &str = "personality-core";

/// Push personality section from SOUL.md to long-term memory.
///
/// Creates or updates `personality-core.md` in the creator's
/// `memory/long-term/` directory.
///
/// If a personality memory already exists (e.g., edited outside SOUL),
/// this push **overwrites** it — SOUL's personality is the anchor.
///
/// # Arguments
///
/// * `home` - Creator home directory (e.g., `~`).
/// * `creator_id` - Validated creator ID (`ctr_` prefix).
/// * `soul` - Parsed `SoulDocument` containing personality content.
///
/// # Returns
///
/// The saved `LongTermMemory` with `source_path` set.
///
/// # Errors
///
/// * `InvalidIdFormat` if `creator_id` is not a valid creator ID.
/// * `ValidationError` if personality section is empty.
/// * I/O errors from directory creation or file write.
pub fn push_personality_to_memory(
    home: &Path,
    creator_id: &str,
    soul: &SoulDocument,
) -> Result<LongTermMemory, DomainError> {
    // 1. Extract personality content
    let personality_content = soul.personality.as_deref().unwrap_or("").trim();
    if personality_content.is_empty() {
        return Err(DomainError::ValidationError(
            "cannot push empty personality section to memory".to_string(),
        ));
    }

    // 2. Check if personality memory already exists to preserve memory_id
    let existing = load_memory(home, creator_id, PERSONALITY_MEMORY_SLUG).ok();

    // 3. Create or update LongTermMemory
    let mut memory = match existing {
        Some(mut existing_mem) => {
            // Preserve memory_id and source_session_ids from existing
            existing_mem.set_body(personality_content);
            // Personality is user-written, not session-derived:
            // clear any source_session_ids that may have been added externally
            existing_mem.frontmatter.source_session_ids.clear();
            existing_mem
        }
        None => {
            let mut new_mem = LongTermMemory::new("personality_core");
            new_mem.set_body(personality_content);
            new_mem
        }
    };

    // 4. Save to disk
    save_memory(home, creator_id, PERSONALITY_MEMORY_SLUG, &memory)?;

    // 5. Set source_path on returned memory for downstream use
    memory.source_path = Some(crate::memory_io::memory_path(
        home,
        creator_id,
        PERSONALITY_MEMORY_SLUG,
    ));

    Ok(memory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SoulDocument;
    use std::path::PathBuf;

    fn fake_home() -> PathBuf {
        let id = std::thread::current()
            .name()
            .unwrap_or("unknown")
            .replace("::", "_");
        PathBuf::from(format!("/tmp/test_personality_sync_{id}"))
    }

    fn cleanup(home: &Path) {
        let _ = std::fs::remove_dir_all(home);
    }

    fn make_soul_with_personality(personality: &str) -> SoulDocument {
        let mut soul = SoulDocument::for_creator("ctr_test");
        soul.set_personality(personality.to_string());
        soul.set_experience(String::new());
        soul
    }

    #[test]
    fn push_creates_personality_memory_file() {
        let home = fake_home();
        cleanup(&home);
        let soul = make_soul_with_personality("Bold and inventive voice.");

        let result = push_personality_to_memory(&home, "ctr_test", &soul);
        assert!(result.is_ok());

        let memory = result.unwrap();
        assert_eq!(memory.frontmatter.memory_kind, "personality_core");
        assert!(memory.body.contains("Bold and inventive voice."));
        assert!(memory.frontmatter.source_session_ids.is_empty());
        assert!(memory.source_path.is_some());
        assert!(memory
            .source_path
            .as_ref()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("personality-core.md"));

        // Verify file exists on disk
        assert!(memory.source_path.as_ref().unwrap().exists());
        cleanup(&home);
    }

    #[test]
    fn push_creates_valid_memory_with_frontmatter() {
        let home = fake_home();
        cleanup(&home);
        let soul = make_soul_with_personality("A thoughtful narrator.");

        let memory = push_personality_to_memory(&home, "ctr_test", &soul).unwrap();

        // Validate frontmatter fields
        assert!(
            memory.frontmatter.memory_id.starts_with("mem_"),
            "memory_id should start with mem_"
        );
        assert_eq!(
            memory.frontmatter.nexus_memory_version, 1,
            "nexus_memory_version should be 1"
        );
        assert!(
            !memory.frontmatter.updated_at.is_empty(),
            "updated_at should be set"
        );
        assert_eq!(
            memory.frontmatter.memory_kind, "personality_core",
            "memory_kind should be personality_core"
        );

        // Verify the file passes LongTermMemory validation
        let loaded = load_memory(&home, "ctr_test", PERSONALITY_MEMORY_SLUG).unwrap();
        assert!(loaded.validate().is_ok());
        cleanup(&home);
    }

    #[test]
    fn re_push_overwrites_existing_personality_memory() {
        let home = fake_home();
        cleanup(&home);

        // First push
        let soul1 = make_soul_with_personality("Original personality.");
        let mem1 = push_personality_to_memory(&home, "ctr_test", &soul1).unwrap();
        let original_id = mem1.frontmatter.memory_id.clone();

        // Second push with different content
        let soul2 = make_soul_with_personality("Updated personality v2.");
        let mem2 = push_personality_to_memory(&home, "ctr_test", &soul2).unwrap();

        // memory_id should be preserved (SOUL wins but keeps same memory)
        assert_eq!(
            mem2.frontmatter.memory_id, original_id,
            "re-push should preserve memory_id"
        );
        assert!(
            mem2.body.contains("Updated personality v2."),
            "body should have new content"
        );

        // Verify on disk
        let loaded = load_memory(&home, "ctr_test", PERSONALITY_MEMORY_SLUG).unwrap();
        assert!(loaded.body.contains("Updated personality v2."));
        assert_eq!(loaded.frontmatter.memory_id, original_id);
        cleanup(&home);
    }

    #[test]
    fn push_clears_source_session_ids_on_existing() {
        let home = fake_home();
        cleanup(&home);

        // First push
        let soul1 = make_soul_with_personality("Initial.");
        let _mem1 = push_personality_to_memory(&home, "ctr_test", &soul1).unwrap();

        // Manually add session IDs (simulating external edit)
        let mut loaded = load_memory(&home, "ctr_test", PERSONALITY_MEMORY_SLUG).unwrap();
        loaded.add_source_session("sess_external_1");
        loaded.add_source_session("sess_external_2");
        save_memory(&home, "ctr_test", PERSONALITY_MEMORY_SLUG, &loaded).unwrap();

        // Re-push should clear session IDs
        let soul2 = make_soul_with_personality("Re-pushed personality.");
        let mem2 = push_personality_to_memory(&home, "ctr_test", &soul2).unwrap();

        assert!(
            mem2.frontmatter.source_session_ids.is_empty(),
            "re-push should clear source_session_ids (personality is user-written)"
        );
        cleanup(&home);
    }

    #[test]
    fn push_rejects_empty_personality() {
        let home = fake_home();
        cleanup(&home);
        let soul = make_soul_with_personality("");

        let result = push_personality_to_memory(&home, "ctr_test", &soul);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("empty personality"), "err: {err}");
        cleanup(&home);
    }

    #[test]
    fn push_rejects_whitespace_only_personality() {
        let home = fake_home();
        cleanup(&home);
        let soul = make_soul_with_personality("   \n\t  \n  ");

        let result = push_personality_to_memory(&home, "ctr_test", &soul);
        assert!(result.is_err());
        cleanup(&home);
    }

    #[test]
    fn push_rejects_none_personality() {
        let home = fake_home();
        cleanup(&home);
        let mut soul = SoulDocument::for_creator("ctr_test");
        soul.personality = None;
        soul.set_experience(String::new());

        let result = push_personality_to_memory(&home, "ctr_test", &soul);
        assert!(result.is_err());
        cleanup(&home);
    }

    #[test]
    fn push_rejects_invalid_creator_id() {
        let home = fake_home();
        cleanup(&home);
        let soul = make_soul_with_personality("Test personality.");

        let result = push_personality_to_memory(&home, "../../etc", &soul);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"));
        cleanup(&home);
    }

    #[test]
    fn push_preserves_none_personality_as_empty_error() {
        let home = fake_home();
        cleanup(&home);
        let mut soul = SoulDocument::new();
        soul.personality = Some(String::new());
        soul.experience = Some(String::new());

        let result = push_personality_to_memory(&home, "ctr_test", &soul);
        assert!(result.is_err());
        cleanup(&home);
    }
}
