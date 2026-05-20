//! Stage-0 context assembly for `local_only` mode.
//!
//! Collects all local context sources and assembles them in the
//! spec-defined order (§9.2):
//!
//! 1. System/policy prefix (runtime injection)
//! 2. `## Personality` (verbatim from SOUL.md)
//! 3. Long-term memory Markdown (sorted by `memory_kind`, then recency)
//! 4. `### Fragment keywords (deduped)` (omitted if empty)
//! 5. `## Experience` (aggregated result from SOUL.md)
//! 6. User task prompt
//!
//! Token budget / truncation (spec §9.3):
//! - Uses chars/4 heuristic for token estimation
//! - Personality section is NEVER truncated
//! - Truncatable sections are dropped from the end when budget exceeded

use nexus_creator_memory::LongTermMemory;
use std::collections::BTreeSet;

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
    /// Fragment keywords (deduped union from `memory_fragments`).
    pub fragment_keywords: Vec<String>,
    /// System/policy prefix (runtime injection).
    pub system_prefix: String,
    /// Current user task prompt.
    pub user_prompt: String,
    /// Optional token budget for the assembled context.
    pub max_tokens: Option<usize>,
}

impl Stage0Assembly {
    #[must_use]
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
    #[must_use]
    pub fn assemble_with_truncation(&self) -> String {
        let Some(budget) = self.max_tokens else {
            return self.assemble();
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

    /// Deduplicate fragment keywords using a `BTreeSet` for deterministic ordering.
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
/// NOTE (S-004): Token estimation uses `chars/4` as a rough approximation.
/// Actual tokenization depends on the tokenizer used by the LLM.
#[must_use]
pub const fn estimate_tokens(text: &str) -> usize {
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
/// Truncatable sections are included greedily from the start.
#[must_use]
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
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    match truncated.rfind(' ') {
        Some(pos) if pos > 0 => format!("{}…", &truncated[..pos]),
        _ => format!("{truncated}…"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_creator_memory::LongTermMemory;

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
        assert!(!output.contains(PERSONALITY_HEADING));
        assert!(!output.contains(FRAGMENT_KEYWORDS_HEADING));
        assert!(output.contains(EXPERIENCE_HEADING));
        assert!(output.contains("Task."));
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
                "plot".to_string(),
                "CHARACTER".to_string(),
                "  setting  ".to_string(),
            ],
            user_prompt: String::new(),
            max_tokens: None,
        };

        let output = asm.assemble();
        assert!(output.contains("plot"));
        assert!(output.contains("character"));
        assert!(output.contains("setting"));
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
            long_term_memories: vec![mem4, mem1, mem3, mem2],
            fragment_keywords: Vec::new(),
            user_prompt: String::new(),
            max_tokens: None,
        };

        let output = asm.assemble();

        let bob_pos = output.find("Bob is brave").unwrap();
        let alice_pos = output.find("Alice is kind").unwrap();
        let summary_pos = output.find("Summary content").unwrap();
        let world_pos = output.find("World A content").unwrap();

        assert!(bob_pos < alice_pos, "Bob (newer) before Alice (older)");
        assert!(bob_pos < summary_pos, "character_note before story_summary");
        assert!(
            summary_pos < world_pos,
            "story_summary before world_building"
        );
    }

    #[test]
    fn assemble_with_truncation_keeps_personality() {
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
            max_tokens: Some(20),
        };

        let output = asm.assemble_with_truncation();
        assert!(
            output.contains("Important personality."),
            "Personality should never be truncated: {output}"
        );
        assert!(output.contains(PERSONALITY_HEADING));
    }

    #[test]
    fn estimate_tokens_basic() {
        assert_eq!(estimate_tokens("Hello"), 2);
    }

    #[test]
    fn estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

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
    fn truncate_never_drops_non_truncatable() {
        let sections = vec![
            ("System: ".to_string(), "policy".to_string(), true),
            ("## Mem\n".to_string(), "x".repeat(1000), false),
            ("Prompt: ".to_string(), "do task".to_string(), true),
        ];
        let result = truncate_with_budget(&sections, 10);
        let headings: Vec<&str> = result.iter().map(|(h, _)| h.as_str()).collect();
        assert!(headings.iter().any(|h| h.contains("System")));
        assert!(headings.iter().any(|h| h.contains("Prompt")));
    }

    #[test]
    fn truncate_to_char_count_basic() {
        assert_eq!(truncate_to_char_count("hello world", 5), "hello…");
    }

    #[test]
    fn truncate_to_char_count_zero() {
        assert_eq!(truncate_to_char_count("text", 0), "");
    }
}
