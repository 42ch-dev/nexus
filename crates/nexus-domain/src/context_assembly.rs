//! Context assembly for ACP sessions (spec §9, §9.2).
//!
//! Two assembly strategies:
//!
//! **Stage-0** (`local_only` mode, ADR-017):
//! Assembles the final context package from local sources only. Combines
//! SOUL.md sections, long-term memories, fragment keywords, and the user
//! prompt into a single ordered context string.
//!
//! Assembly ordering (spec §9.2):
//! 1. System/policy prefix (runtime injection)
//! 2. `## Personality` (verbatim from SOUL.md)
//! 3. Long-term memory Markdown (sorted by memory_kind, then recency)
//! 4. `### Fragment keywords (deduped)` (omitted if empty)
//! 5. `## Experience` (aggregated result from SOUL.md)
//! 6. User task prompt
//!
//! **Two-Stage** (`local_first` / `cloud_enhanced` modes):
//! Stage-1 calls platform `context/assemble` API; Stage-2 merges the
//! platform response with local SOUL, memories, and fragments.
//!
//! Assembly ordering (spec §9.2 two-stage):
//! 1. System/policy prefix
//! 2. `## Personality`
//! 3. Long-term memories (local)
//! 4. Fragment keywords (local)
//! 5. Memory items from Stage-1 (deduped with local)
//! 6. KB + Timeline from Stage-1
//! 7. `## Experience`
//! 8. User prompt
//!
//! Token budget / truncation (spec §9.3):
//! - Uses chars/4 heuristic for token estimation
//! - Personality section is NEVER truncated
//! - Truncatable sections are dropped from the end when budget exceeded

use crate::runtime_mode::DomainRuntimeMode;
use crate::LongTermMemory;
use std::collections::{BTreeSet, HashSet};

/// Section heading for fragment keywords.
const FRAGMENT_KEYWORDS_HEADING: &str = "### Fragment keywords (deduped)";

/// Section heading for personality (matches SOUL.md).
const PERSONALITY_HEADING: &str = "## Personality";

/// Section heading for experience (matches SOUL.md).
const EXPERIENCE_HEADING: &str = "## Experience";

/// Stage-0 context assembly for `local_only` mode.
///
/// Collects all local context sources and assembles them in the
/// spec-defined order (§9.2).
#[derive(Default)]
pub struct Stage0Assembly {
    /// Personality section content (verbatim from SOUL.md `## Personality`).
    pub personality: String,
    /// Experience section content (aggregated from SOUL.md `## Experience`).
    pub experience: String,
    /// Long-term memories to include, sorted by kind then recency.
    pub long_term_memories: Vec<LongTermMemory>,
    /// Fragment keywords (deduped union from memory_fragments).
    pub fragment_keywords: Vec<String>,
    /// System/policy prefix (runtime injection).
    pub system_prefix: String,
    /// Current user task prompt.
    pub user_prompt: String,
    /// Optional token budget for the assembled context.
    pub max_tokens: Option<usize>,
}

impl Stage0Assembly {
    /// Assemble the full context string without truncation.
    ///
    /// Follows spec §9.2 ordering exactly.
    pub fn assemble(&self) -> String {
        let mut parts = Vec::new();

        // 1. System/policy prefix
        if !self.system_prefix.is_empty() {
            parts.push(self.system_prefix.clone());
        }

        // 2. ## Personality
        if !self.personality.is_empty() {
            parts.push(format!("{PERSONALITY_HEADING}\n\n{}\n", self.personality));
        }

        // 3. Long-term memories (sorted by kind, then recency)
        let sorted = self.sorted_memories();
        for mem in &sorted {
            let title = format!("### Memory: {}", mem.frontmatter.memory_id);
            parts.push(format!("{title}\n\n{}\n", mem.body));
        }

        // 4. Fragment keywords (deduped, omit if empty)
        let keywords = self.deduped_keywords();
        if !keywords.is_empty() {
            parts.push(format!(
                "{FRAGMENT_KEYWORDS_HEADING}\n\n{}\n",
                keywords.join(", ")
            ));
        }

        // 5. ## Experience
        if !self.experience.is_empty() {
            parts.push(format!("{EXPERIENCE_HEADING}\n\n{}\n", self.experience));
        }

        // 6. User prompt
        if !self.user_prompt.is_empty() {
            parts.push(self.user_prompt.clone());
        }

        parts.join("\n")
    }

    /// Assemble with token budget truncation.
    ///
    /// When `max_tokens` is set, sections are truncated to fit within
    /// the budget. Personality and system prefix are never truncated.
    /// If the budget cannot accommodate non-truncatable sections alone,
    /// they are still included (we never drop Personality).
    pub fn assemble_with_truncation(&self) -> String {
        let budget = match self.max_tokens {
            Some(b) => b,
            None => return self.assemble(),
        };

        let sorted = self.sorted_memories();
        let keywords = self.deduped_keywords();

        // Build sections in order: (heading, content, non_truncatable)
        let mut sections: Vec<(String, String, bool)> = Vec::new();

        // 1. System prefix (non-truncatable)
        if !self.system_prefix.is_empty() {
            sections.push((String::new(), self.system_prefix.clone(), true));
        }

        // 2. Personality (non-truncatable — spec §9.3)
        if !self.personality.is_empty() {
            sections.push((
                format!("{PERSONALITY_HEADING}\n\n"),
                self.personality.clone(),
                true,
            ));
        }

        // 3. Long-term memories (truncatable)
        for mem in &sorted {
            let heading = format!("### Memory: {}\n\n", mem.frontmatter.memory_id);
            sections.push((heading, mem.body.clone(), false));
        }

        // 4. Fragment keywords (truncatable)
        if !keywords.is_empty() {
            let content = keywords.join(", ");
            sections.push((format!("{FRAGMENT_KEYWORDS_HEADING}\n\n"), content, false));
        }

        // 5. Experience (truncatable)
        if !self.experience.is_empty() {
            sections.push((
                format!("{EXPERIENCE_HEADING}\n\n"),
                self.experience.clone(),
                false,
            ));
        }

        // 6. User prompt (non-truncatable)
        if !self.user_prompt.is_empty() {
            sections.push((String::new(), self.user_prompt.clone(), true));
        }

        let included = truncate_with_budget(&sections, budget);

        // Reassemble
        let parts: Vec<String> = included
            .into_iter()
            .map(|(heading, content)| {
                if heading.is_empty() {
                    content
                } else {
                    format!("{heading}{content}")
                }
            })
            .collect();

        parts.join("\n\n")
    }

    /// Sort memories by `memory_kind` (alphabetical), then by `updated_at`
    /// descending (most recent first).
    fn sorted_memories(&self) -> Vec<&LongTermMemory> {
        let mut sorted: Vec<&LongTermMemory> = self.long_term_memories.iter().collect();
        sorted.sort_by(|a, b| {
            let kind_cmp = a.frontmatter.memory_kind.cmp(&b.frontmatter.memory_kind);
            if kind_cmp != std::cmp::Ordering::Equal {
                return kind_cmp;
            }
            // Most recent first (reverse chronological)
            b.frontmatter.updated_at.cmp(&a.frontmatter.updated_at)
        });
        sorted
    }

    /// Deduplicate fragment keywords using a BTreeSet for deterministic ordering.
    fn deduped_keywords(&self) -> Vec<String> {
        let set: BTreeSet<String> = self
            .fragment_keywords
            .iter()
            .map(|k| k.trim().to_lowercase())
            .filter(|k| !k.is_empty())
            .collect();
        set.into_iter().collect()
    }
}

/// Estimate token count using the chars/4 heuristic.
///
/// This is a rough approximation; actual tokenization depends on the
/// tokenizer used by the LLM. Suitable for budget estimation.
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// Truncate sections to fit within a token budget.
///
/// Each section is a tuple of `(heading, content, non_truncatable)`:
/// - `heading`: section heading text (included in token count)
/// - `content`: section body text
/// - `non_truncatable`: if `true`, the section is always included
///
/// Non-truncatable sections are always included regardless of budget.
/// Truncatable sections are included greedily from the start; if a
/// section would exceed the remaining budget, it is skipped entirely.
/// The last fitting truncatable section may be partially truncated.
///
/// Returns the list of `(heading, content)` tuples that fit.
pub fn truncate_with_budget(
    sections: &[(String, String, bool)],
    budget: usize,
) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut used_tokens: usize = 0;

    // First pass: include all non-truncatable sections
    for (heading, content, non_truncatable) in sections {
        if *non_truncatable {
            let section_text = format!("{heading}{content}");
            let tokens = estimate_tokens(&section_text);
            result.push((heading.clone(), content.clone()));
            used_tokens += tokens;
        }
    }

    // Second pass: greedily add truncatable sections
    for (heading, content, non_truncatable) in sections {
        if *non_truncatable {
            continue;
        }
        let full_section = format!("{heading}{content}");
        let full_tokens = estimate_tokens(&full_section);
        let remaining = budget.saturating_sub(used_tokens);

        if full_tokens <= remaining {
            result.push((heading.clone(), content.clone()));
            used_tokens += full_tokens;
        } else if remaining > 0 {
            // Partially truncate: estimate how many chars fit
            let target_chars = remaining * 4;
            // Include heading plus as much content as fits
            let heading_chars = heading.len();
            let content_chars = target_chars.saturating_sub(heading_chars);
            let truncated_content = truncate_to_char_count(content, content_chars);
            let actual_section = format!("{heading}{truncated_content}");
            let actual_tokens = estimate_tokens(&actual_section);
            if actual_tokens <= remaining || result.is_empty() {
                result.push((heading.clone(), truncated_content));
                used_tokens += actual_tokens;
            }
        }
    }

    result
}

/// Truncate text to approximately `max_chars` characters at a word boundary.
fn truncate_to_char_count(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if text.len() <= max_chars {
        return text.to_string();
    }
    // Find the last space before max_chars to break at a word boundary
    let truncate_at = text[..max_chars.min(text.len())].rfind(' ');
    match truncate_at {
        Some(pos) if pos > 0 => format!("{}…", &text[..pos]),
        _ => format!("{}…", &text[..max_chars.min(text.len())]),
    }
}

// ── Two-Stage Assembly (local_first / cloud_enhanced) ────────────

/// Section heading for platform memory items.
const PLATFORM_MEMORY_HEADING: &str = "### Platform Memory Items";

/// Section heading for knowledge base entries.
const KB_HEADING: &str = "### Knowledge Base";

/// Section heading for timeline events.
const TIMELINE_HEADING: &str = "### Timeline Events";

/// Two-stage context assembly for `local_first` / `cloud_enhanced` modes.
///
/// Stage-1: Call platform `context/assemble` API.
/// Stage-2: Merge platform response with local SOUL + memories + fragments.
///
/// Spec: `creator-memory-soul-lifecycle-v1.md` §9
pub struct TwoStageAssembly {
    /// Stage-1 response from platform (optional — may fail or return empty).
    pub stage1_response: Option<AssembleResponse>,
    /// Local SOUL personality section.
    pub personality: String,
    /// Local SOUL experience section.
    pub experience: String,
    /// Local long-term memories.
    pub long_term_memories: Vec<LongTermMemory>,
    /// Local fragment keywords (union from memory_fragments).
    pub fragment_keywords: Vec<String>,
    /// User task prompt.
    pub user_prompt: String,
    /// System/policy prefix.
    pub system_prefix: String,
    /// Token budget.
    pub max_tokens: Option<usize>,
    /// Current runtime mode (for merge rules).
    pub runtime_mode: DomainRuntimeMode,
}

/// Response from platform `context/assemble` API (Stage-1).
///
/// Minimal wire shape for V1.2.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AssembleResponse {
    /// Memory items from platform vector search (optional).
    pub memory_items: Vec<MemoryItemRef>,
    /// Knowledge base entries (optional).
    pub kb: Vec<KbEntry>,
    /// Timeline events (optional).
    pub timeline: Vec<TimelineEventRef>,
    /// Assembly metadata from platform.
    pub metadata: AssembleMetadata,
}

/// Reference to a memory item (from platform).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryItemRef {
    pub memory_id: String,
    pub content_summary: String,
    pub relevance_score: Option<f32>,
}

/// Knowledge base entry reference.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KbEntry {
    pub entry_id: String,
    pub title: String,
    pub content: String,
}

/// Timeline event reference.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimelineEventRef {
    pub event_id: String,
    pub event_type: String,
    pub timestamp: String,
}

/// Assembly metadata from platform.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AssembleMetadata {
    pub assembled_at: String,
    pub token_count_estimate: Option<u32>,
}

impl TwoStageAssembly {
    /// Assemble the final context (Stage-2 merge).
    ///
    /// Merge rules (spec §9.2):
    /// 1. System prefix
    /// 2. Personality (SOUL)
    /// 3. Long-term memories (local)
    /// 4. Fragment keywords (local)
    /// 5. Memory items from Stage-1 (deduped: local wins for same `memory_id`)
    /// 6. KB + Timeline from Stage-1 (if available)
    /// 7. Experience (SOUL)
    /// 8. User prompt
    pub fn assemble(&self) -> String {
        let mut parts = Vec::new();

        // 1. System/policy prefix
        if !self.system_prefix.is_empty() {
            parts.push(self.system_prefix.clone());
        }

        // 2. ## Personality
        if !self.personality.is_empty() {
            parts.push(format!("{PERSONALITY_HEADING}\n\n{}\n", self.personality));
        }

        // 3. Long-term memories (local, sorted by kind then recency)
        let sorted = self.sorted_memories();
        for mem in &sorted {
            let title = format!("### Memory: {}", mem.frontmatter.memory_id);
            parts.push(format!("{title}\n\n{}\n", mem.body));
        }

        // 4. Fragment keywords (deduped, omit if empty)
        let keywords = self.deduped_keywords();
        if !keywords.is_empty() {
            parts.push(format!(
                "{FRAGMENT_KEYWORDS_HEADING}\n\n{}\n",
                keywords.join(", ")
            ));
        }

        // 5–6. Platform data from Stage-1 (if available)
        if let Some(ref response) = self.stage1_response {
            // 5. Memory items (deduped with local per §9.1.1)
            let deduped_items = self.deduped_platform_memories(&response.memory_items);
            if !deduped_items.is_empty() {
                let mem_lines: Vec<String> = deduped_items
                    .iter()
                    .map(|item| {
                        let score = item
                            .relevance_score
                            .map(|s| format!(" [relevance: {s:.2}]"))
                            .unwrap_or_default();
                        format!(
                            "- {} ({}): {}{score}",
                            item.memory_id, item.memory_id, item.content_summary
                        )
                    })
                    .collect();
                parts.push(format!(
                    "{PLATFORM_MEMORY_HEADING}\n\n{}\n",
                    mem_lines.join("\n")
                ));
            }

            // 6a. Knowledge base entries
            if !response.kb.is_empty() {
                let kb_lines: Vec<String> = response
                    .kb
                    .iter()
                    .map(|entry| {
                        format!(
                            "- **{}** ({}): {}",
                            entry.title, entry.entry_id, entry.content
                        )
                    })
                    .collect();
                parts.push(format!("{KB_HEADING}\n\n{}\n", kb_lines.join("\n")));
            }

            // 6b. Timeline events
            if !response.timeline.is_empty() {
                let tl_lines: Vec<String> = response
                    .timeline
                    .iter()
                    .map(|evt| {
                        format!(
                            "- [{}] {} ({})",
                            evt.timestamp, evt.event_type, evt.event_id
                        )
                    })
                    .collect();
                parts.push(format!("{TIMELINE_HEADING}\n\n{}\n", tl_lines.join("\n")));
            }
        }

        // 7. ## Experience
        if !self.experience.is_empty() {
            parts.push(format!("{EXPERIENCE_HEADING}\n\n{}\n", self.experience));
        }

        // 8. User prompt
        if !self.user_prompt.is_empty() {
            parts.push(self.user_prompt.clone());
        }

        parts.join("\n")
    }

    /// Assemble with fallback to Stage0Assembly if Stage-1 failed.
    ///
    /// When `stage1_response` is `None`, returns Stage0-style output
    /// (local data only, no platform sections).
    pub fn assemble_with_fallback(&self) -> String {
        if self.stage1_response.is_none() {
            return self.assemble_stage0_fallback();
        }
        self.assemble()
    }

    /// Fallback assembly when platform unavailable (Stage0 ordering).
    ///
    /// Reuses Stage0Assembly ordering:
    /// system → personality → memories → keywords → experience → prompt
    fn assemble_stage0_fallback(&self) -> String {
        let mut parts = Vec::new();

        // 1. System/policy prefix
        if !self.system_prefix.is_empty() {
            parts.push(self.system_prefix.clone());
        }

        // 2. ## Personality
        if !self.personality.is_empty() {
            parts.push(format!("{PERSONALITY_HEADING}\n\n{}\n", self.personality));
        }

        // 3. Long-term memories (sorted by kind then recency)
        let sorted = self.sorted_memories();
        for mem in &sorted {
            let title = format!("### Memory: {}", mem.frontmatter.memory_id);
            parts.push(format!("{title}\n\n{}\n", mem.body));
        }

        // 4. Fragment keywords (deduped, omit if empty)
        let keywords = self.deduped_keywords();
        if !keywords.is_empty() {
            parts.push(format!(
                "{FRAGMENT_KEYWORDS_HEADING}\n\n{}\n",
                keywords.join(", ")
            ));
        }

        // 5. ## Experience
        if !self.experience.is_empty() {
            parts.push(format!("{EXPERIENCE_HEADING}\n\n{}\n", self.experience));
        }

        // 6. User prompt
        if !self.user_prompt.is_empty() {
            parts.push(self.user_prompt.clone());
        }

        parts.join("\n")
    }

    /// Dedup platform memory items against local memories (spec §9.1.1).
    ///
    /// If a local long-term memory and a platform `memory_item` share the
    /// same `memory_id`, the platform item is excluded (local wins).
    fn deduped_platform_memories<'a>(&self, items: &'a [MemoryItemRef]) -> Vec<&'a MemoryItemRef> {
        let local_ids: HashSet<&str> = self
            .long_term_memories
            .iter()
            .map(|m| m.frontmatter.memory_id.as_str())
            .collect();
        items
            .iter()
            .filter(|item| !local_ids.contains(item.memory_id.as_str()))
            .collect()
    }

    /// Sort memories by `memory_kind` (alphabetical), then by `updated_at`
    /// descending (most recent first).
    fn sorted_memories(&self) -> Vec<&LongTermMemory> {
        let mut sorted: Vec<&LongTermMemory> = self.long_term_memories.iter().collect();
        sorted.sort_by(|a, b| {
            let kind_cmp = a.frontmatter.memory_kind.cmp(&b.frontmatter.memory_kind);
            if kind_cmp != std::cmp::Ordering::Equal {
                return kind_cmp;
            }
            // Most recent first (reverse chronological)
            b.frontmatter.updated_at.cmp(&a.frontmatter.updated_at)
        });
        sorted
    }

    /// Deduplicate fragment keywords using a BTreeSet for deterministic ordering.
    fn deduped_keywords(&self) -> Vec<String> {
        let set: BTreeSet<String> = self
            .fragment_keywords
            .iter()
            .map(|k| k.trim().to_lowercase())
            .filter(|k| !k.is_empty())
            .collect();
        set.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LongTermMemory;

    fn make_memory(kind: &str, body: &str, updated_at: &str) -> LongTermMemory {
        let mut mem = LongTermMemory::new(kind);
        mem.set_body(body);
        mem.frontmatter.updated_at = updated_at.to_string();
        mem
    }

    #[test]
    fn assemble_basic_ordering() {
        let asm = Stage0Assembly {
            system_prefix: "You are a helpful assistant.".to_string(),
            personality: "Creative and bold.".to_string(),
            experience: "10 years of writing.".to_string(),
            long_term_memories: vec![make_memory(
                "story_summary",
                "Plot analysis.",
                "2026-04-14T00:00:00Z",
            )],
            fragment_keywords: vec!["character".to_string(), "plot".to_string()],
            user_prompt: "Write chapter 3.".to_string(),
            max_tokens: None,
        };

        let output = asm.assemble();

        // Verify ordering: system → personality → memories → keywords → experience → prompt
        let sys_pos = output.find("You are a helpful assistant.").unwrap();
        let pers_pos = output.find(PERSONALITY_HEADING).unwrap();
        let mem_pos = output.find("### Memory:").unwrap();
        let kw_pos = output.find(FRAGMENT_KEYWORDS_HEADING).unwrap();
        let exp_pos = output.find(EXPERIENCE_HEADING).unwrap();
        let prompt_pos = output.find("Write chapter 3.").unwrap();

        assert!(sys_pos < pers_pos, "system should come before personality");
        assert!(
            pers_pos < mem_pos,
            "personality should come before memories"
        );
        assert!(mem_pos < kw_pos, "memories should come before keywords");
        assert!(kw_pos < exp_pos, "keywords should come before experience");
        assert!(exp_pos < prompt_pos, "experience should come before prompt");
    }

    #[test]
    fn assemble_omits_empty_sections() {
        let asm = Stage0Assembly {
            system_prefix: String::new(),
            personality: String::new(),
            experience: "Some experience.".to_string(),
            long_term_memories: Vec::new(),
            fragment_keywords: Vec::new(),
            user_prompt: "Task.".to_string(),
            max_tokens: None,
        };

        let output = asm.assemble();

        // Empty sections should not produce headings
        assert!(!output.contains(PERSONALITY_HEADING));
        assert!(!output.contains(FRAGMENT_KEYWORDS_HEADING));
        assert!(output.contains(EXPERIENCE_HEADING));
        assert!(output.contains("Task."));
    }

    #[test]
    fn assemble_keywords_section_omitted_when_empty() {
        let asm = Stage0Assembly {
            system_prefix: String::new(),
            personality: "A writer.".to_string(),
            experience: String::new(),
            long_term_memories: Vec::new(),
            fragment_keywords: vec![],
            user_prompt: String::new(),
            max_tokens: None,
        };

        let output = asm.assemble();
        assert!(
            !output.contains(FRAGMENT_KEYWORDS_HEADING),
            "keywords section should be omitted when empty"
        );
    }

    #[test]
    fn assemble_keywords_deduped() {
        let asm = Stage0Assembly {
            system_prefix: String::new(),
            personality: String::new(),
            experience: String::new(),
            long_term_memories: Vec::new(),
            fragment_keywords: vec![
                "plot".to_string(),
                "character".to_string(),
                "plot".to_string(),        // duplicate
                "CHARACTER".to_string(),   // case-insensitive duplicate
                "  setting  ".to_string(), // whitespace trim
            ],
            user_prompt: String::new(),
            max_tokens: None,
        };

        let output = asm.assemble();
        // Should contain each keyword once
        assert!(output.contains("plot"));
        assert!(output.contains("character"));
        assert!(output.contains("setting"));
        // The keywords section heading should be present
        assert!(output.contains(FRAGMENT_KEYWORDS_HEADING));
    }

    #[test]
    fn assemble_memories_sorted_by_kind_then_recency() {
        let mem1 = make_memory("world_building", "World A content", "2026-04-10T00:00:00Z");
        let mem2 = make_memory("character_note", "Alice is kind", "2026-04-13T00:00:00Z");
        let mem3 = make_memory("character_note", "Bob is brave", "2026-04-14T00:00:00Z");
        let mem4 = make_memory("story_summary", "Summary content", "2026-04-12T00:00:00Z");

        let asm = Stage0Assembly {
            system_prefix: String::new(),
            personality: String::new(),
            experience: String::new(),
            long_term_memories: vec![mem4, mem1, mem3, mem2], // shuffled
            fragment_keywords: Vec::new(),
            user_prompt: String::new(),
            max_tokens: None,
        };

        let output = asm.assemble();

        // Order should be: character_note (Bob newer → first), character_note (Alice older),
        // story_summary, world_building
        let bob_pos = output.find("Bob is brave").unwrap();
        let alice_pos = output.find("Alice is kind").unwrap();
        let summary_pos = output.find("Summary content").unwrap();
        let world_pos = output.find("World A content").unwrap();

        assert!(
            bob_pos < alice_pos,
            "Bob (newer) should come before Alice (older) within same kind"
        );
        assert!(
            bob_pos < summary_pos,
            "character_note should come before story_summary"
        );
        assert!(
            summary_pos < world_pos,
            "story_summary should come before world_building"
        );
    }

    #[test]
    fn assemble_no_truncation_when_no_budget() {
        let asm = Stage0Assembly {
            system_prefix: "System.".to_string(),
            personality: "Pers.".to_string(),
            experience: "Exp.".to_string(),
            long_term_memories: Vec::new(),
            fragment_keywords: vec!["kw".to_string()],
            user_prompt: "Prompt.".to_string(),
            max_tokens: None,
        };

        assert_eq!(asm.assemble(), asm.assemble_with_truncation());
    }

    #[test]
    fn assemble_with_truncation_keeps_personality() {
        // Very small budget — everything should be truncated but personality must remain
        let asm = Stage0Assembly {
            system_prefix: String::new(),
            personality: "Important personality.".to_string(),
            experience: "A".to_string(),
            long_term_memories: vec![make_memory(
                "story_summary",
                &"B".repeat(1000),
                "2026-04-14T00:00:00Z",
            )],
            fragment_keywords: vec!["kw".to_string()],
            user_prompt: String::new(),
            max_tokens: Some(20), // very small
        };

        let output = asm.assemble_with_truncation();
        assert!(
            output.contains("Important personality."),
            "Personality should never be truncated: {output}"
        );
        assert!(output.contains(PERSONALITY_HEADING));
    }

    #[test]
    fn assemble_with_truncation_drops_later_sections() {
        let asm = Stage0Assembly {
            system_prefix: "System.".to_string(),
            personality: "Pers.".to_string(),
            experience: "E".repeat(200),
            long_term_memories: vec![make_memory(
                "story_summary",
                &"M".repeat(200),
                "2026-04-14T00:00:00Z",
            )],
            fragment_keywords: vec!["kw".to_string()],
            user_prompt: "Prompt.".to_string(),
            max_tokens: Some(100), // tight budget
        };

        let output = asm.assemble_with_truncation();
        // System, personality, and prompt are non-truncatable
        assert!(output.contains("System."));
        assert!(output.contains(PERSONALITY_HEADING));
        assert!(output.contains("Prompt."));
    }

    // ── estimate_tokens tests ────────────────────────────────────────

    #[test]
    fn estimate_tokens_basic() {
        // "Hello" = 5 chars → ceil(5/4) = 2 tokens
        assert_eq!(estimate_tokens("Hello"), 2);
    }

    #[test]
    fn estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn estimate_tokens_exact_multiple() {
        // 8 chars → 8/4 = 2 tokens
        assert_eq!(estimate_tokens("abcdefgh"), 2);
    }

    // ── truncate_with_budget tests ───────────────────────────────────

    #[test]
    fn truncate_includes_all_when_budget_sufficient() {
        let sections = vec![
            ("## A\n".to_string(), "Body A.".to_string(), false),
            ("## B\n".to_string(), "Body B.".to_string(), false),
        ];
        let result = truncate_with_budget(&sections, 1000);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn truncate_drops_truncatable_sections() {
        let sections = vec![
            ("## A\n".to_string(), "Body A.".to_string(), false),
            ("## B\n".to_string(), "Body B.".to_string(), false),
        ];
        let result = truncate_with_budget(&sections, 5); // very tight
                                                         // May include 0 or 1 sections depending on heading overhead
        assert!(result.len() <= 2);
    }

    #[test]
    fn truncate_never_drops_non_truncatable() {
        let sections = vec![
            ("System: ".to_string(), "policy".to_string(), true),
            ("## Mem\n".to_string(), "x".repeat(1000), false),
            ("## Exp\n".to_string(), "x".repeat(1000), false),
            ("Prompt: ".to_string(), "do task".to_string(), true),
        ];
        let result = truncate_with_budget(&sections, 10); // very small
                                                          // Both non-truncatable must be present
        let headings: Vec<&str> = result.iter().map(|(h, _)| h.as_str()).collect();
        assert!(headings.iter().any(|h| h.contains("System")));
        assert!(headings.iter().any(|h| h.contains("Prompt")));
    }

    #[test]
    fn truncate_partial_section() {
        let long_body = "word ".repeat(200); // 1000 chars ≈ 250 tokens
        let sections = vec![
            ("Non-trunc\n".to_string(), "keep me".to_string(), true), // ~5 tokens
            ("Trunc\n".to_string(), long_body.clone(), false),        // ~253 tokens
        ];
        // Budget should fit non-trunc + partial trunc (e.g., 30 tokens)
        let result = truncate_with_budget(&sections, 30);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].1, "keep me");
        // The truncatable section should be shorter than original
        assert!(result[1].1.len() < long_body.len());
    }

    #[test]
    fn truncate_empty_sections() {
        let result = truncate_with_budget(&[], 100);
        assert!(result.is_empty());
    }

    #[test]
    fn truncate_to_char_count_basic() {
        assert_eq!(truncate_to_char_count("hello world", 5), "hello…");
    }

    #[test]
    fn truncate_to_char_count_shorter_than_limit() {
        assert_eq!(truncate_to_char_count("hi", 10), "hi");
    }

    #[test]
    fn truncate_to_char_count_zero() {
        assert_eq!(truncate_to_char_count("text", 0), "");
    }

    #[test]
    fn truncate_to_char_count_word_boundary() {
        let text = "one two three four five";
        let result = truncate_to_char_count(text, 13); // "one two thre"
        assert!(
            result.contains("one two"),
            "should keep complete words: {result}"
        );
        assert!(result.ends_with('…'));
    }

    // ── TwoStageAssembly tests ───────────────────────────────────────

    fn make_two_stage_with_stage1() -> TwoStageAssembly {
        use crate::runtime_mode::DomainRuntimeMode;
        use nexus_contracts::RuntimeMode;

        TwoStageAssembly {
            stage1_response: Some(AssembleResponse {
                memory_items: vec![
                    MemoryItemRef {
                        memory_id: "mem_platform_1".to_string(),
                        content_summary: "Platform memory summary".to_string(),
                        relevance_score: Some(0.95),
                    },
                    MemoryItemRef {
                        memory_id: "mem_platform_2".to_string(),
                        content_summary: "Another platform memory".to_string(),
                        relevance_score: None,
                    },
                ],
                kb: vec![KbEntry {
                    entry_id: "kb_1".to_string(),
                    title: "World Building Guide".to_string(),
                    content: "Keep worlds consistent.".to_string(),
                }],
                timeline: vec![TimelineEventRef {
                    event_id: "evt_1".to_string(),
                    event_type: "session_created".to_string(),
                    timestamp: "2026-04-14T10:00:00Z".to_string(),
                }],
                metadata: AssembleMetadata {
                    assembled_at: "2026-04-14T12:00:00Z".to_string(),
                    token_count_estimate: Some(500),
                },
            }),
            personality: "Creative and bold.".to_string(),
            experience: "10 years of writing.".to_string(),
            long_term_memories: vec![make_memory(
                "story_summary",
                "Local memory body.",
                "2026-04-14T00:00:00Z",
            )],
            fragment_keywords: vec!["character".to_string(), "plot".to_string()],
            user_prompt: "Write chapter 3.".to_string(),
            system_prefix: "You are a helpful assistant.".to_string(),
            max_tokens: None,
            runtime_mode: DomainRuntimeMode::new(RuntimeMode::LocalFirst),
        }
    }

    #[test]
    fn two_stage_assemble_with_stage1_data() {
        let asm = make_two_stage_with_stage1();
        let output = asm.assemble();

        // Verify §9.2 ordering: system → personality → memories → keywords → memory_items → kb/timeline → experience → prompt
        let sys_pos = output.find("You are a helpful assistant.").unwrap();
        let pers_pos = output.find(PERSONALITY_HEADING).unwrap();
        let mem_pos = output.find("### Memory:").unwrap();
        let kw_pos = output.find(FRAGMENT_KEYWORDS_HEADING).unwrap();
        let platform_mem_pos = output.find("mem_platform_1").unwrap();
        let kb_pos = output.find("World Building Guide").unwrap();
        let exp_pos = output.find(EXPERIENCE_HEADING).unwrap();
        let prompt_pos = output.find("Write chapter 3.").unwrap();

        assert!(sys_pos < pers_pos, "system should come before personality");
        assert!(
            pers_pos < mem_pos,
            "personality should come before memories"
        );
        assert!(mem_pos < kw_pos, "memories should come before keywords");
        assert!(
            kw_pos < platform_mem_pos,
            "keywords should come before platform memory_items"
        );
        assert!(
            platform_mem_pos < kb_pos,
            "platform memory_items should come before kb"
        );
        assert!(kb_pos < exp_pos, "kb should come before experience");
        assert!(exp_pos < prompt_pos, "experience should come before prompt");

        // Platform data should be present
        assert!(output.contains("Platform memory summary"));
        assert!(output.contains("Keep worlds consistent."));
        assert!(output.contains("session_created"));
    }

    #[test]
    fn two_stage_fallback_when_stage1_none() {
        use crate::runtime_mode::DomainRuntimeMode;
        use nexus_contracts::RuntimeMode;

        let asm = TwoStageAssembly {
            stage1_response: None,
            personality: "A writer.".to_string(),
            experience: "Some experience.".to_string(),
            long_term_memories: vec![make_memory(
                "character_note",
                "Alice profile.",
                "2026-04-14T00:00:00Z",
            )],
            fragment_keywords: vec!["plot".to_string()],
            user_prompt: "Task.".to_string(),
            system_prefix: String::new(),
            max_tokens: None,
            runtime_mode: DomainRuntimeMode::new(RuntimeMode::LocalFirst),
        };

        let output = asm.assemble_with_fallback();

        // Should contain local data (Stage0 ordering)
        assert!(output.contains(PERSONALITY_HEADING));
        assert!(output.contains("A writer."));
        assert!(output.contains("### Memory:"));
        assert!(output.contains("Alice profile."));
        assert!(output.contains("plot"));
        assert!(output.contains(EXPERIENCE_HEADING));
        assert!(output.contains("Some experience."));
        assert!(output.contains("Task."));

        // Should NOT contain any platform-only sections
        assert!(!output.contains("### Platform Memory Items"));
    }

    #[test]
    fn memory_dedup_local_over_platform() {
        use crate::runtime_mode::DomainRuntimeMode;
        use nexus_contracts::RuntimeMode;

        // Local memory with ID that also appears in platform response
        let mut local_mem = make_memory(
            "story_summary",
            "LOCAL content wins.",
            "2026-04-14T00:00:00Z",
        );
        local_mem.frontmatter.memory_id = "mem_overlap".to_string();

        let asm = TwoStageAssembly {
            stage1_response: Some(AssembleResponse {
                memory_items: vec![
                    MemoryItemRef {
                        memory_id: "mem_overlap".to_string(), // Same ID as local!
                        content_summary: "PLATFORM content should be deduped.".to_string(),
                        relevance_score: Some(0.9),
                    },
                    MemoryItemRef {
                        memory_id: "mem_platform_only".to_string(),
                        content_summary: "Platform-only memory.".to_string(),
                        relevance_score: Some(0.8),
                    },
                ],
                kb: Vec::new(),
                timeline: Vec::new(),
                metadata: AssembleMetadata {
                    assembled_at: "2026-04-14T12:00:00Z".to_string(),
                    token_count_estimate: None,
                },
            }),
            personality: String::new(),
            experience: String::new(),
            long_term_memories: vec![local_mem],
            fragment_keywords: Vec::new(),
            user_prompt: String::new(),
            system_prefix: String::new(),
            max_tokens: None,
            runtime_mode: DomainRuntimeMode::new(RuntimeMode::CloudEnhanced),
        };

        let output = asm.assemble();

        // Local content should appear (under ### Memory heading)
        assert!(output.contains("LOCAL content wins."));
        // Platform content for same ID should NOT appear
        assert!(
            !output.contains("PLATFORM content should be deduped."),
            "platform memory with same ID as local should be deduped"
        );
        // Platform-only memory should still appear
        assert!(output.contains("Platform-only memory."));
    }
}
