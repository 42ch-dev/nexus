//! Experience aggregation from long-term memories (spec §4.3).
//!
//! Scans long-term memory MD files for experience-kind entries, optionally
//! uses ACP to generate aggregated experience text, and replaces the
//! `## Experience` section in SOUL.md.
//!
//! If ACP is unavailable, falls back to deterministic concatenation
//! (sort by recency + concat excerpts).

use crate::memory_io;
use crate::DomainError;
#[cfg(test)]
use crate::LongTermMemory;
use std::path::Path;

/// Memory kinds that contribute to the Experience section.
const EXPERIENCE_MEMORY_KINDS: &[&str] = &[
    "story_summary",
    "review_note",
    "character_note",
    "world_building",
    "plot_outline",
    "theme_analysis",
];

/// Maximum body characters included per memory in deterministic fallback.
const FALLBACK_EXCERPT_LENGTH: usize = 200;

/// Result of experience aggregation.
#[derive(Debug, Clone)]
pub struct AggregationResult {
    /// The generated markdown body for the `## Experience` section.
    pub experience_markdown: String,
    /// Number of experience-type memories found.
    pub memories_processed: usize,
    /// Whether ACP was used (true) or deterministic fallback (false).
    pub used_acp: bool,
}

/// Trait for LLM-assisted experience synthesis.
///
/// Implementations call an LLM (via ACP or other mechanism) to
/// synthesize experience entries into a coherent markdown section.
pub trait ExperienceSynthesizer: Send + Sync {
    /// Synthesize a list of experience memories into an aggregated markdown section.
    ///
    /// Returns the markdown body (without `## Experience` heading — the caller
    /// adds it).
    fn synthesize(
        &self,
        entries: &[ExperienceEntry],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, DomainError>> + Send + '_>>;
}

/// A single experience entry for synthesis.
#[derive(Debug, Clone)]
pub struct ExperienceEntry {
    /// Memory kind (e.g., "story_summary").
    pub memory_kind: String,
    /// Memory slug / filename stem.
    pub slug: String,
    /// Body text of the memory.
    pub body: String,
    /// Timestamp (ISO-8601).
    pub updated_at: String,
}

/// Aggregate experience from long-term memories and update SOUL.md.
///
/// Steps:
/// 1. List all long-term memories for the creator
/// 2. Filter to experience-kind memories
/// 3. Sort by recency (most recent first)
/// 4. Optionally call synthesizer to generate aggregated text
/// 5. Replace `## Experience` section in SOUL.md
///
/// If no synthesizer is provided, falls back to deterministic concat.
pub async fn aggregate_experience(
    home: &Path,
    creator_id: &str,
    synthesizer: Option<&dyn ExperienceSynthesizer>,
) -> Result<AggregationResult, DomainError> {
    let entries = collect_experience_entries(home, creator_id)?;

    let count = entries.len();

    let (experience_markdown, used_acp) =
        generate_experience_markdown(&entries, synthesizer).await;

    // Update SOUL.md
    let mut soul = crate::soul_io::load(home, creator_id)?;
    soul.set_experience(experience_markdown.clone());
    crate::soul_io::save(home, creator_id, &soul)?;

    Ok(AggregationResult {
        experience_markdown,
        memories_processed: count,
        used_acp,
    })
}

/// Aggregate experience without updating SOUL.md.
///
/// Returns the generated markdown and metadata, but does NOT write to disk.
/// Useful for preview or dry-run scenarios.
pub async fn aggregate_experience_preview(
    home: &Path,
    creator_id: &str,
    synthesizer: Option<&dyn ExperienceSynthesizer>,
) -> Result<AggregationResult, DomainError> {
    let entries = collect_experience_entries(home, creator_id)?;

    let count = entries.len();

    let (experience_markdown, used_acp) =
        generate_experience_markdown(&entries, synthesizer).await;

    Ok(AggregationResult {
        experience_markdown,
        memories_processed: count,
        used_acp,
    })
}

/// Collect and filter experience-kind memory entries for a creator.
///
/// Shared helper extracted to deduplicate between `aggregate_experience`
/// and `aggregate_experience_preview` (R4 pipeline).
///
/// Steps:
/// 1. List all long-term memories for the creator
/// 2. Load and filter to experience-kind memories
/// 3. Sort by recency (most recent first)
fn collect_experience_entries(
    home: &Path,
    creator_id: &str,
) -> Result<Vec<ExperienceEntry>, DomainError> {
    let slugs = memory_io::list_memories(home, creator_id)?;

    let mut experience_entries: Vec<ExperienceEntry> = Vec::new();
    for slug in &slugs {
        let memory = match memory_io::load_memory(home, creator_id, slug) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(slug = %slug, error = %e, "Skipping unreadable memory during experience aggregation");
                continue;
            }
        };

        if EXPERIENCE_MEMORY_KINDS.contains(&memory.frontmatter.memory_kind.as_str()) {
            experience_entries.push(ExperienceEntry {
                memory_kind: memory.frontmatter.memory_kind.clone(),
                slug: memory.slug(),
                body: memory.body.clone(),
                updated_at: memory.frontmatter.updated_at.clone(),
            });
        }
    }

    // Sort by recency (most recent first)
    experience_entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    Ok(experience_entries)
}

/// Generate experience markdown from collected entries.
///
/// Shared helper extracted to deduplicate between `aggregate_experience`
/// and `aggregate_experience_preview` (R4 pipeline).
///
/// If a synthesizer is provided, uses it to generate aggregated text;
/// falls back to deterministic concatenation on failure.
async fn generate_experience_markdown(
    entries: &[ExperienceEntry],
    synthesizer: Option<&dyn ExperienceSynthesizer>,
) -> (String, bool) {
    if let Some(synth) = synthesizer {
        if !entries.is_empty() {
            match synth.synthesize(entries).await {
                Ok(text) => return (text, true),
                Err(_) => {
                    // ACP failed — fall back to deterministic
                    return (deterministic_concat(entries), false);
                }
            }
        }
    }
    (deterministic_concat(entries), false)
}

/// Deterministic fallback: concatenate experience memories sorted by recency.
///
/// Each memory gets a `### <memory_kind>: <slug>` heading.
/// Body is truncated to `FALLBACK_EXCERPT_LENGTH` chars with "...".
fn deterministic_concat(entries: &[ExperienceEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut parts = Vec::new();
    for entry in entries {
        let kind_label = format_kind_label(&entry.memory_kind);
        let body_excerpt = truncate_body(&entry.body, FALLBACK_EXCERPT_LENGTH);
        parts.push(format!("### {kind_label}: {}", entry.slug));
        parts.push(body_excerpt);
    }

    parts.join("\n\n")
}

/// Format a memory_kind snake_case string into a human-readable label.
fn format_kind_label(kind: &str) -> String {
    kind.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Truncate body text to approximately `max_chars` characters.
fn truncate_body(body: &str, max_chars: usize) -> String {
    if body.len() <= max_chars {
        return body.to_string();
    }
    let end = &body[..max_chars];
    // Break at last space before limit
    if let Some(pos) = end.rfind(' ') {
        format!("{}...", &body[..pos])
    } else {
        format!("{}...", end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(kind: &str, slug: &str, body: &str, updated_at: &str) -> ExperienceEntry {
        ExperienceEntry {
            memory_kind: kind.to_string(),
            slug: slug.to_string(),
            body: body.to_string(),
            updated_at: updated_at.to_string(),
        }
    }

    #[test]
    fn deterministic_concat_empty() {
        let result = deterministic_concat(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn deterministic_concat_single_entry() {
        let entries = vec![make_entry(
            "story_summary",
            "chapter1",
            "This is a story summary about chapter one.",
            "2026-04-14T00:00:00Z",
        )];
        let result = deterministic_concat(&entries);
        assert!(result.contains("### Story Summary: chapter1"));
        assert!(result.contains("This is a story summary"));
    }

    #[test]
    fn deterministic_concat_multiple_entries() {
        let entries = vec![
            make_entry(
                "character_note",
                "alice",
                "Alice is the protagonist.",
                "2026-04-14T10:00:00Z",
            ),
            make_entry(
                "world_building",
                "setting",
                "The story takes place in a futuristic city.",
                "2026-04-13T00:00:00Z",
            ),
        ];
        let result = deterministic_concat(&entries);
        // Most recent first
        let alice_pos = result.find("alice").expect("should contain alice");
        let setting_pos = result.find("setting").expect("should contain setting");
        assert!(
            alice_pos < setting_pos,
            "more recent entry should come first"
        );
        assert!(result.contains("### Character Note: alice"));
        assert!(result.contains("### World Building: setting"));
    }

    #[test]
    fn deterministic_concat_truncates_long_body() {
        let long_body = "A".repeat(500);
        let entries = vec![make_entry(
            "story_summary",
            "long-story",
            &long_body,
            "2026-04-14T00:00:00Z",
        )];
        let result = deterministic_concat(&entries);
        // Should be truncated with "..."
        assert!(result.ends_with("..."));
        // Should not include the full body
        assert!(result.len() < long_body.len() + 50);
    }

    #[test]
    fn deterministic_concat_short_body_not_truncated() {
        let entries = vec![make_entry(
            "review_note",
            "short-review",
            "Short note.",
            "2026-04-14T00:00:00Z",
        )];
        let result = deterministic_concat(&entries);
        assert!(!result.contains("..."));
        assert!(result.contains("Short note."));
    }

    #[test]
    fn format_kind_label_converts_snake_case() {
        assert_eq!(format_kind_label("story_summary"), "Story Summary");
        assert_eq!(format_kind_label("character_note"), "Character Note");
        assert_eq!(format_kind_label("world_building"), "World Building");
        assert_eq!(format_kind_label("plot_outline"), "Plot Outline");
        assert_eq!(format_kind_label("theme_analysis"), "Theme Analysis");
        assert_eq!(format_kind_label("review_note"), "Review Note");
        assert_eq!(format_kind_label("custom"), "Custom");
    }

    #[test]
    fn truncate_body_short_text() {
        assert_eq!(truncate_body("hello", 100), "hello");
    }

    #[test]
    fn truncate_body_exact_length() {
        assert_eq!(truncate_body("hello", 5), "hello");
    }

    #[test]
    fn truncate_body_long_text() {
        let text = "one two three four five six seven eight nine ten";
        let result = truncate_body(text, 20);
        assert!(result.ends_with("..."));
        assert!(result.len() < text.len());
    }

    #[test]
    fn experience_kinds_includes_all_expected() {
        let expected = [
            "story_summary",
            "review_note",
            "character_note",
            "world_building",
            "plot_outline",
            "theme_analysis",
        ];
        for kind in &expected {
            assert!(
                EXPERIENCE_MEMORY_KINDS.contains(kind),
                "EXPERIENCE_MEMORY_KINDS should include '{kind}'"
            );
        }
    }

    #[test]
    fn experience_kinds_excludes_non_experience() {
        assert!(
            !EXPERIENCE_MEMORY_KINDS.contains(&"personality_core"),
            "personality_core should not be in experience kinds"
        );
        assert!(
            !EXPERIENCE_MEMORY_KINDS.contains(&"research_material"),
            "research_material should not be in experience kinds"
        );
    }

    #[tokio::test]
    async fn aggregate_experience_fallback_no_memories() {
        let home = std::path::PathBuf::from("/tmp/test_agg_exp_empty");
        let _ = std::fs::remove_dir_all(&home);

        // Create soul first
        crate::soul_io::create(&home, "ctr_test").unwrap();

        let result = aggregate_experience(&home, "ctr_test", None).await.unwrap();
        assert_eq!(result.memories_processed, 0);
        assert!(!result.used_acp);
        assert!(result.experience_markdown.is_empty());

        // Verify soul still has empty experience
        let soul = crate::soul_io::load(&home, "ctr_test").unwrap();
        assert_eq!(soul.experience.as_deref().unwrap_or(""), "");

        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn aggregate_experience_deterministic_fallback() {
        let home = std::path::PathBuf::from("/tmp/test_agg_exp_determ");
        let _ = std::fs::remove_dir_all(&home);

        // Create soul
        crate::soul_io::create(&home, "ctr_test").unwrap();

        // Create experience memories
        let mut mem1 = LongTermMemory::new("story_summary");
        mem1.set_body("A grand adventure story about heroes saving the world from darkness.");
        memory_io::save_memory(&home, "ctr_test", "adventure-story", &mem1).unwrap();

        let mut mem2 = LongTermMemory::new("character_note");
        mem2.set_body(
            "Alice is a brave and resourceful protagonist who overcomes great obstacles.",
        );
        memory_io::save_memory(&home, "ctr_test", "alice-note", &mem2).unwrap();

        // Create a non-experience memory (should be ignored)
        let mut mem3 = LongTermMemory::new("research_material");
        mem3.set_body("Research on medieval castles.");
        memory_io::save_memory(&home, "ctr_test", "castle-research", &mem3).unwrap();

        let result = aggregate_experience(&home, "ctr_test", None).await.unwrap();

        assert_eq!(result.memories_processed, 2);
        assert!(!result.used_acp);
        assert!(result.experience_markdown.contains("Story Summary"));
        assert!(result.experience_markdown.contains("Character Note"));
        assert!(!result.experience_markdown.contains("research"));

        // Verify SOUL.md was updated
        let soul = crate::soul_io::load(&home, "ctr_test").unwrap();
        let exp = soul.experience.as_deref().unwrap_or("");
        assert!(exp.contains("adventure-story"));
        assert!(exp.contains("alice-note"));

        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn aggregate_experience_with_synthesizer_success() {
        struct MockSynthesizer;
        impl ExperienceSynthesizer for MockSynthesizer {
            fn synthesize(
                &self,
                entries: &[ExperienceEntry],
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<String, DomainError>> + Send + '_>,
            > {
                let result = format!(
                    "Aggregated from {} memories: {}",
                    entries.len(),
                    entries
                        .iter()
                        .map(|e| e.slug.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                Box::pin(async move { Ok(result) })
            }
        }

        let home = std::path::PathBuf::from("/tmp/test_agg_exp_acp");
        let _ = std::fs::remove_dir_all(&home);

        crate::soul_io::create(&home, "ctr_test").unwrap();

        let mut mem = LongTermMemory::new("story_summary");
        mem.set_body("An epic tale of courage.");
        memory_io::save_memory(&home, "ctr_test", "epic-tale", &mem).unwrap();

        let synth = MockSynthesizer;
        let result = aggregate_experience(&home, "ctr_test", Some(&synth))
            .await
            .unwrap();

        assert!(result.used_acp);
        assert_eq!(result.memories_processed, 1);
        assert!(result.experience_markdown.contains("epic-tale"));

        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn aggregate_experience_synthesizer_failure_falls_back() {
        struct FailingSynthesizer;
        impl ExperienceSynthesizer for FailingSynthesizer {
            fn synthesize(
                &self,
                _entries: &[ExperienceEntry],
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<String, DomainError>> + Send + '_>,
            > {
                Box::pin(
                    async move { Err(DomainError::ValidationError("ACP unavailable".to_string())) },
                )
            }
        }

        let home = std::path::PathBuf::from("/tmp/test_agg_exp_acp_fail");
        let _ = std::fs::remove_dir_all(&home);

        crate::soul_io::create(&home, "ctr_test").unwrap();

        let mut mem = LongTermMemory::new("story_summary");
        mem.set_body("A test story summary.");
        memory_io::save_memory(&home, "ctr_test", "test-story", &mem).unwrap();

        let synth = FailingSynthesizer;
        let result = aggregate_experience(&home, "ctr_test", Some(&synth))
            .await
            .unwrap();

        assert!(!result.used_acp);
        assert_eq!(result.memories_processed, 1);
        // Should have deterministic fallback output
        assert!(result.experience_markdown.contains("Story Summary"));

        let _ = std::fs::remove_dir_all(&home);
    }

    // ── E2E-style tests (T5.17) ─────────────────────────────────────

    /// Full pipeline: SOUL init → create memories → aggregate experience → assemble context
    #[tokio::test]
    async fn e2e_soul_init_memories_aggregate_assemble() {
        let home = std::path::PathBuf::from("/tmp/test_e2e_full_pipeline");
        let _ = std::fs::remove_dir_all(&home);

        let creator_id = "ctr_e2e";

        // 1. Initialize SOUL
        let soul = crate::soul_io::create(&home, creator_id).unwrap();
        soul.validate().unwrap();

        // 2. Set personality
        let mut soul = crate::soul_io::load(&home, creator_id).unwrap();
        soul.set_personality("A bold and creative writer focused on sci-fi.".to_string());
        crate::soul_io::save(&home, creator_id, &soul).unwrap();

        // 3. Push personality to memory
        let personality_mem =
            crate::personality_sync::push_personality_to_memory(&home, creator_id, &soul).unwrap();
        assert_eq!(personality_mem.frontmatter.memory_kind, "personality_core");

        // 4. Create experience-type memories
        let mut story = LongTermMemory::new("story_summary");
        story.set_body("Wrote a cyberpunk novelette about AI consciousness.");
        memory_io::save_memory(&home, creator_id, "cyberpunk-story", &story).unwrap();

        let mut char_note = LongTermMemory::new("character_note");
        char_note.set_body("Developed Zara, a hacker protagonist with trust issues.");
        memory_io::save_memory(&home, creator_id, "zara-character", &char_note).unwrap();

        // 5. Aggregate experience (deterministic)
        let result = aggregate_experience(&home, creator_id, None).await.unwrap();
        assert_eq!(result.memories_processed, 2);
        assert!(!result.used_acp);
        assert!(result.experience_markdown.contains("cyberpunk-story"));
        assert!(result.experience_markdown.contains("zara-character"));

        // 6. Assemble context (Stage-0)
        let slugs = memory_io::list_memories(&home, creator_id).unwrap();
        let mut memories = Vec::new();
        for slug in &slugs {
            if let Ok(mem) = memory_io::load_memory(&home, creator_id, slug) {
                memories.push(mem);
            }
        }

        let reloaded_soul = crate::soul_io::load(&home, creator_id).unwrap();
        let assembly = crate::Stage0Assembly {
            personality: reloaded_soul.personality.clone().unwrap_or_default(),
            experience: reloaded_soul.experience.clone().unwrap_or_default(),
            long_term_memories: memories,
            fragment_keywords: vec![],
            system_prefix: String::new(),
            user_prompt: "Write chapter 1".to_string(),
            max_tokens: None,
        };

        let output = assembly.assemble();

        // Verify §9.2 ordering
        let personality_pos = output.find("## Personality").expect("personality heading");
        let mem_pos = output.find("### Memory:").expect("memory heading");
        let exp_pos = output.find("## Experience").expect("experience heading");
        let prompt_pos = output.find("Write chapter 1").expect("user prompt");

        assert!(
            personality_pos < mem_pos,
            "§9.2: personality before memories"
        );
        assert!(mem_pos < exp_pos, "§9.2: memories before experience");
        assert!(exp_pos < prompt_pos, "§9.2: experience before prompt");

        // Verify personality content present
        assert!(output.contains("bold and creative"));

        // 7. Verify personality push-down → memory file exists
        let loaded_personality =
            memory_io::load_memory(&home, creator_id, "personality-core").unwrap();
        assert!(loaded_personality.body.contains("bold and creative"));

        let _ = std::fs::remove_dir_all(&home);
    }

    /// Verify §9.2 ordering with multiple memory kinds
    #[tokio::test]
    async fn e2e_verify_section_ordering_with_multiple_kinds() {
        let home = std::path::PathBuf::from("/tmp/test_e2e_ordering");
        let _ = std::fs::remove_dir_all(&home);

        let creator_id = "ctr_order";

        // Create SOUL
        let mut soul = crate::soul_io::create(&home, creator_id).unwrap();
        soul.set_personality("Test personality.".to_string());
        soul.set_experience("Test experience.".to_string());
        crate::soul_io::save(&home, creator_id, &soul).unwrap();

        // Create memories of different kinds
        let kinds = vec![
            ("world_building", "wb-map", "World map notes."),
            ("character_note", "cn-hero", "Hero backstory."),
            ("story_summary", "ss-plot", "Plot outline."),
        ];
        for (kind, slug, body) in &kinds {
            let mut mem = LongTermMemory::new(kind);
            mem.set_body(body);
            memory_io::save_memory(&home, creator_id, slug, &mem).unwrap();
        }

        // Assemble
        let slugs = memory_io::list_memories(&home, creator_id).unwrap();
        let mut memories = Vec::new();
        for slug in &slugs {
            if let Ok(mem) = memory_io::load_memory(&home, creator_id, slug) {
                memories.push(mem);
            }
        }

        let soul = crate::soul_io::load(&home, creator_id).unwrap();
        let assembly = crate::Stage0Assembly {
            personality: soul.personality.clone().unwrap_or_default(),
            experience: soul.experience.clone().unwrap_or_default(),
            long_term_memories: memories,
            fragment_keywords: vec!["keyword1".to_string()],
            system_prefix: String::new(),
            user_prompt: String::new(),
            max_tokens: None,
        };

        let output = assembly.assemble();

        // Verify all sections present in correct order
        let pers = output.find("## Personality").unwrap();
        let mem = output.find("### Memory:").unwrap();
        let kw = output.find("### Fragment keywords").unwrap();
        let exp = output.find("## Experience").unwrap();

        assert!(pers < mem);
        assert!(mem < kw);
        assert!(kw < exp);
        assert!(output.contains("keyword1"));

        let _ = std::fs::remove_dir_all(&home);
    }

    /// Verify experience aggregation → SOUL.md updated
    #[tokio::test]
    async fn e2e_experience_aggregation_updates_soul() {
        let home = std::path::PathBuf::from("/tmp/test_e2e_agg_soul");
        let _ = std::fs::remove_dir_all(&home);

        let creator_id = "ctr_aggsoul";

        // Create SOUL with empty experience
        let soul = crate::soul_io::create(&home, creator_id).unwrap();
        let exp_before = soul.experience.as_deref().unwrap_or("");
        assert!(exp_before.is_empty());

        // Create experience memories
        let mut mem1 = LongTermMemory::new("theme_analysis");
        mem1.set_body("Themes of isolation and connection in modern fiction.");
        memory_io::save_memory(&home, creator_id, "isolation-theme", &mem1).unwrap();

        // Aggregate
        let result = aggregate_experience(&home, creator_id, None).await.unwrap();
        assert_eq!(result.memories_processed, 1);
        assert!(result.experience_markdown.contains("isolation-theme"));

        // Verify SOUL.md was updated on disk
        let reloaded = crate::soul_io::load(&home, creator_id).unwrap();
        let exp_after = reloaded.experience.as_deref().unwrap_or("");
        assert!(!exp_after.is_empty());
        assert!(exp_after.contains("Theme Analysis"));

        let _ = std::fs::remove_dir_all(&home);
    }

    /// Verify personality push-down → verify memory file created
    #[tokio::test]
    async fn e2e_personality_push_creates_memory_file() {
        let home = std::path::PathBuf::from("/tmp/test_e2e_push");
        let _ = std::fs::remove_dir_all(&home);

        let creator_id = "ctr_pushtest";

        // Create SOUL with personality
        let mut soul = crate::soul_io::create(&home, creator_id).unwrap();
        soul.set_personality("A minimalist prose style with sharp dialogue.".to_string());
        crate::soul_io::save(&home, creator_id, &soul).unwrap();

        // Push personality to memory
        let memory =
            crate::personality_sync::push_personality_to_memory(&home, creator_id, &soul).unwrap();

        // Verify file exists and is valid
        let loaded = memory_io::load_memory(&home, creator_id, "personality-core").unwrap();
        assert_eq!(loaded.frontmatter.memory_kind, "personality_core");
        assert!(loaded.body.contains("minimalist prose style"));
        assert!(loaded.validate().is_ok());

        // Re-push should preserve memory_id
        let memory2 =
            crate::personality_sync::push_personality_to_memory(&home, creator_id, &soul).unwrap();
        assert_eq!(
            memory.frontmatter.memory_id, memory2.frontmatter.memory_id,
            "re-push should preserve memory_id"
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    /// Verify token truncation preserves personality
    #[tokio::test]
    async fn e2e_token_truncation_preserves_personality() {
        let home = std::path::PathBuf::from("/tmp/test_e2e_truncate");
        let _ = std::fs::remove_dir_all(&home);

        let creator_id = "ctr_truncate";

        // Create SOUL
        let mut soul = crate::soul_io::create(&home, creator_id).unwrap();
        soul.set_personality("Critical personality text.".to_string());
        soul.set_experience("E".repeat(500));
        crate::soul_io::save(&home, creator_id, &soul).unwrap();

        // Create a large memory
        let mut mem = LongTermMemory::new("story_summary");
        mem.set_body(&"M".repeat(1000));
        memory_io::save_memory(&home, creator_id, "big-mem", &mem).unwrap();

        // Load memories
        let slugs = memory_io::list_memories(&home, creator_id).unwrap();
        let mut memories = Vec::new();
        for slug in &slugs {
            if let Ok(m) = memory_io::load_memory(&home, creator_id, slug) {
                memories.push(m);
            }
        }

        // Assemble with tiny budget
        let soul = crate::soul_io::load(&home, creator_id).unwrap();
        let assembly = crate::Stage0Assembly {
            personality: soul.personality.clone().unwrap_or_default(),
            experience: soul.experience.clone().unwrap_or_default(),
            long_term_memories: memories,
            fragment_keywords: vec![],
            system_prefix: String::new(),
            user_prompt: "Do task".to_string(),
            max_tokens: Some(15),
        };

        let output = assembly.assemble_with_truncation();

        // Personality MUST be preserved (spec §9.3)
        assert!(
            output.contains("Critical personality text."),
            "Personality should never be truncated: {output}"
        );

        let _ = std::fs::remove_dir_all(&home);
    }
}
