//! v1.184 P3 Task 2 — Creator output byte-stability goldens.
//!
//! The golden strings below were captured from the pre-refactor Creator
//! pipeline (single-creator code path). The shared Creator/Character bearer
//! refactor must reproduce them byte-for-byte: SOUL.md creation, long-term
//! memory file format, promotion output shape, and deterministic experience
//! aggregation into SOUL.md.

#![allow(clippy::unwrap_used)]

use nexus_creator_memory::bearer::MemoryBearerRef;
use nexus_creator_memory::errors::MemoryError;
use nexus_creator_memory::long_term_memory::LongTermMemory;
use nexus_creator_memory::review::{
    promote_to_long_term, PendingReviewInput, SessionDigestSummarizer,
};
use nexus_creator_memory::{experience_aggregation, memory_io, soul_io};
use std::path::PathBuf;

const GOLDEN_HOME_ID: &str = "ctr_golden";

fn tmp(tag: &str) -> PathBuf {
    PathBuf::from(format!(
        "/tmp/test_creator_byte_stability_{tag}_{}",
        std::process::id()
    ))
}

const fn creator() -> MemoryBearerRef<'static> {
    MemoryBearerRef::Creator(GOLDEN_HOME_ID)
}

/// Bytes of a freshly created Creator SOUL.md (pre-refactor capture).
const GOLDEN_SOUL_CREATE: &str =
    "---\ncreator_id: ctr_golden\nschema_version: 1\n---\n\n## Personality\n\n\n\n## Experience\n\n\n";

/// Bytes of a long-term memory file with a fully fixed frontmatter
/// (pre-refactor capture).
const GOLDEN_MEMORY_SAVE: &str = "---\nnexus_memory_version: 1\nmemory_id: mem_0123456789abcdef0123456789abcdef\nmemory_kind: story_summary\nupdated_at: 2026-01-02T03:04:05Z\nsource_session_ids:\n- sess_1\n---\nBody text line one.\nSecond line.\n";

/// Bytes of SOUL.md after deterministic experience aggregation over two
/// fixed memories plus the fixed-frontmatter one (pre-refactor capture).
const GOLDEN_AGGREGATE_SOUL: &str = "---\ncreator_id: ctr_golden\nschema_version: 1\n---\n\n## Personality\n\n\n\n## Experience\n\n### Story Summary: adventure-story\n\nA grand adventure story about heroes.\n\n### Story Summary: fixed-slug\n\nBody text line one.\nSecond line.\n\n\n### Character Note: alice-note\n\nAlice is brave and cautious.\n";

#[test]
fn creator_soul_create_bytes_are_byte_stable() {
    let home = tmp("soul");
    let _ = std::fs::remove_dir_all(&home);

    soul_io::create(&home, creator()).unwrap();
    let bytes = std::fs::read_to_string(soul_io::soul_path(&home, creator())).unwrap();
    assert_eq!(bytes, GOLDEN_SOUL_CREATE, "SOUL.md create bytes drifted");

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn creator_memory_save_bytes_are_byte_stable() {
    let home = tmp("memory");
    let _ = std::fs::remove_dir_all(&home);

    let mut mem = LongTermMemory::new("story_summary");
    mem.frontmatter.memory_id = "mem_0123456789abcdef0123456789abcdef".to_string();
    mem.frontmatter.updated_at = "2026-01-02T03:04:05Z".to_string();
    mem.frontmatter.source_session_ids = vec!["sess_1".to_string()];
    mem.body = "Body text line one.\nSecond line.\n".to_string();

    memory_io::save_memory(&home, creator(), "fixed-slug", &mem).unwrap();
    let bytes =
        std::fs::read_to_string(memory_io::memory_path(&home, creator(), "fixed-slug")).unwrap();
    assert_eq!(bytes, GOLDEN_MEMORY_SAVE, "memory file bytes drifted");

    let _ = std::fs::remove_dir_all(&home);
}

#[tokio::test]
async fn creator_experience_aggregation_bytes_are_byte_stable() {
    let home = tmp("aggregate");
    let _ = std::fs::remove_dir_all(&home);

    soul_io::create(&home, creator()).unwrap();

    let mut fixed = LongTermMemory::new("story_summary");
    fixed.frontmatter.memory_id = "mem_0123456789abcdef0123456789abcdef".to_string();
    fixed.frontmatter.updated_at = "2026-01-02T03:04:05Z".to_string();
    fixed.frontmatter.source_session_ids = vec!["sess_1".to_string()];
    fixed.body = "Body text line one.\nSecond line.\n".to_string();
    memory_io::save_memory(&home, creator(), "fixed-slug", &fixed).unwrap();

    let mut m1 = LongTermMemory::new("story_summary");
    m1.frontmatter.memory_id = "mem_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
    m1.frontmatter.updated_at = "2026-02-01T00:00:00Z".to_string();
    m1.body = "A grand adventure story about heroes.".to_string();
    memory_io::save_memory(&home, creator(), "adventure-story", &m1).unwrap();

    let mut m2 = LongTermMemory::new("character_note");
    m2.frontmatter.memory_id = "mem_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
    m2.frontmatter.updated_at = "2026-01-01T00:00:00Z".to_string();
    m2.body = "Alice is brave and cautious.".to_string();
    memory_io::save_memory(&home, creator(), "alice-note", &m2).unwrap();

    let result = experience_aggregation::aggregate_experience(&home, creator(), None)
        .await
        .unwrap();
    assert!(!result.used_acp, "no synthesizer → deterministic fallback");
    assert_eq!(result.memories_processed, 3);

    let bytes = std::fs::read_to_string(soul_io::soul_path(&home, creator())).unwrap();
    assert_eq!(
        bytes, GOLDEN_AGGREGATE_SOUL,
        "aggregation SOUL.md bytes drifted"
    );

    let _ = std::fs::remove_dir_all(&home);
}

struct FixedSummarizer;

impl SessionDigestSummarizer for FixedSummarizer {
    async fn summarize(
        &self,
        _session_id: &str,
        _task_kind: &str,
        _raw_digest: &str,
        _scope_id: Option<&str>,
    ) -> Result<String, MemoryError> {
        Ok("Fixed summary body.".to_string())
    }
}

/// Promotion output shape golden: the only volatile fields are the minted
/// `memory_id` and `updated_at`; everything else is pinned byte-for-byte.
#[tokio::test]
async fn creator_promote_output_shape_is_stable() {
    let home = tmp("promote");
    let _ = std::fs::remove_dir_all(&home);

    let input = PendingReviewInput {
        pending_id: "pending_1".to_string(),
        session_id: "sess_promote_1".to_string(),
        bearer_id: GOLDEN_HOME_ID.to_string(),
        scope_id: None,
        task_kind: "brainstorm".to_string(),
        raw_digest: "Discussed three major themes for the novel at length.".to_string(),
        created_at: "2026-03-01T00:00:00Z".to_string(),
    };

    let memory = promote_to_long_term(&home, creator(), &input, &FixedSummarizer)
        .await
        .unwrap();

    // Volatile field shapes are still constrained.
    let id_suffix = memory
        .frontmatter
        .memory_id
        .strip_prefix("mem_")
        .expect("memory_id keeps mem_ prefix");
    assert_eq!(id_suffix.len(), 32, "memory_id keeps 32-hex shape");
    assert!(id_suffix.chars().all(|c| c.is_ascii_hexdigit()));
    let ts = &memory.frontmatter.updated_at;
    assert!(
        ts.len() >= 20 && ts.contains('T'),
        "RFC3339 timestamp: {ts}"
    );

    // Slug derivation is pinned: `mem_<id>` → `memory-<id>`.
    let expected_slug = memory.frontmatter.memory_id.replace("mem_", "memory-");
    let file = memory_io::memory_path(&home, creator(), &expected_slug);
    assert!(file.exists(), "promoted memory file at {file:?}");

    let mut normalized = memory_io::load_memory(&home, creator(), &expected_slug)
        .unwrap_or_else(|e| panic!("reload promoted memory: {e}"));
    assert_eq!(normalized.frontmatter.memory_kind, "story_summary");
    assert_eq!(
        normalized.frontmatter.source_session_ids,
        vec!["sess_promote_1".to_string()]
    );
    normalized.frontmatter.memory_id = "mem_<ID>".to_string();
    normalized.frontmatter.updated_at = "<TS>".to_string();
    let bytes = normalized.render().unwrap();
    let expected = "---\nnexus_memory_version: 1\nmemory_id: mem_<ID>\nmemory_kind: story_summary\nupdated_at: <TS>\nsource_session_ids:\n- sess_promote_1\n---\nFixed summary body.";
    assert_eq!(bytes, expected, "promotion output shape drifted");

    let _ = std::fs::remove_dir_all(&home);
}
