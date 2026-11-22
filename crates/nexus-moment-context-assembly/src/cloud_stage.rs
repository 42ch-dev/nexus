//! note: two-stage context assembly for `local_first` / `cloud_enhanced` modes.
//!
//! Stage-1 calls the platform `context/assemble` API; Stage-2 merges that
//! response with local SOUL + memories + fragments.
//!
//! Spec: `docs/specs/creator/creator-memory-soul-lifecycle.md` §9

use crate::recall::{memory_body, memory_display_id, memory_kind, memory_updated_at};
use nexus_contracts::local::domain::RuntimeMode;
use nexus_spoke_adapter::spoke_schemas::RetainedRecord;
use std::collections::BTreeSet;
use std::collections::HashSet;

/// note: heading text mirrors the `## Personality` section of the creator's SOUL.md.
const PERSONALITY_HEADING: &str = "## Personality";

/// note: heading text mirrors the `## Experience` section of the creator's SOUL.md.
const EXPERIENCE_HEADING: &str = "## Experience";

/// note: heading for the deduped, lowercased fragment-keyword list.
const FRAGMENT_KEYWORDS_HEADING: &str = "### Fragment keywords (deduped)";

/// note: heading for Stage-1 platform memory items after local-ID dedup.
const PLATFORM_MEMORY_HEADING: &str = "### Platform Memory Items";

/// note: heading for Stage-1 knowledge base entries.
const KB_HEADING: &str = "### Knowledge Base";

/// note: heading for Stage-1 timeline events.
const TIMELINE_HEADING: &str = "### Timeline Events";

/// note: Stage-1 body of the platform `context/assemble` API.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AssembleResponse {
    /// note: platform vector-search hits; empty when the platform matched none.
    pub memory_items: Vec<MemoryItemRef>,
    /// note: knowledge base entries the platform matched.
    pub kb: Vec<KbEntry>,
    /// note: timeline events the platform matched.
    pub timeline: Vec<TimelineEventRef>,
    /// note: platform assembly metadata; never rendered into the prompt.
    pub metadata: AssembleMetadata,
}

/// note: one platform vector-search hit; `relevance_score` renders as `[relevance: x.xx]`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryItemRef {
    pub memory_id: String,
    pub content_summary: String,
    pub relevance_score: Option<f32>,
}

/// note: knowledge base entry reference as the platform returns it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KbEntry {
    pub entry_id: String,
    pub title: String,
    pub content: String,
}

/// note: timeline event reference; rendered as `[timestamp] event_type (event_id)`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimelineEventRef {
    pub event_id: String,
    pub event_type: String,
    pub timestamp: String,
}

/// note: platform assembly metadata (assemble timestamp and token estimate).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AssembleMetadata {
    pub assembled_at: String,
    pub token_count_estimate: Option<u32>,
}

/// note: two-stage assembly for `local_first` / `cloud_enhanced` modes.
///
/// Stage-1 calls the platform `context/assemble` API; Stage-2 merges that
/// response with local SOUL + memories + fragments.
pub struct TwoStageAssembly {
    /// note: Stage-1 result; `None` routes [`Self::assemble_with_fallback`] to the Stage-0 path.
    pub stage1_response: Option<AssembleResponse>,
    /// note: rendered under [`PERSONALITY_HEADING`]; empty omits the section.
    pub personality: String,
    /// note: rendered under [`EXPERIENCE_HEADING`]; empty omits the section.
    pub experience: String,
    /// note: contract `nexus.<MemoryKind>` records, sorted by kind then newest-first.
    pub long_term_memories: Vec<RetainedRecord>,
    /// note: trimmed, lowercased and deduped before rendering; empty omits the section.
    pub fragment_keywords: Vec<String>,
    /// note: appended last; empty contributes nothing.
    pub user_prompt: String,
    /// note: emitted first; empty contributes nothing.
    pub system_prefix: String,
    /// note: token budget carried for callers; `assemble` does not truncate to it.
    pub max_tokens: Option<usize>,
    /// note: carried for merge rules; `assemble` currently renders the same output for every mode.
    pub runtime_mode: RuntimeMode,
}

impl TwoStageAssembly {
    /// note: Stage-2 merge order — system prefix, personality, memories, fragment
    /// keywords, Stage-1 platform items/kb/timeline, experience, prompt.
    #[must_use]
    pub fn assemble(&self) -> String {
        let mut parts = Vec::new();

        if !self.system_prefix.is_empty() {
            parts.push(self.system_prefix.clone());
        }

        if !self.personality.is_empty() {
            parts.push(format!("{PERSONALITY_HEADING}\n\n{}\n", self.personality));
        }

        let sorted = self.sorted_memories();
        for mem in &sorted {
            let title = format!("### Memory: {}", memory_display_id(mem));
            parts.push(format!("{title}\n\n{}\n", memory_body(mem)));
        }

        let keywords = self.deduped_keywords();
        if !keywords.is_empty() {
            parts.push(format!(
                "{FRAGMENT_KEYWORDS_HEADING}\n\n{}\n",
                keywords.join(", ")
            ));
        }

        if let Some(ref response) = self.stage1_response {
            let deduped_items = self.deduped_platform_memories(&response.memory_items);
            if !deduped_items.is_empty() {
                let mem_lines: Vec<String> = deduped_items
                    .iter()
                    .map(|item| {
                        let score = item
                            .relevance_score
                            .map_or_default(|s| format!(" [relevance: {s:.2}]"));
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

        if !self.experience.is_empty() {
            parts.push(format!("{EXPERIENCE_HEADING}\n\n{}\n", self.experience));
        }

        if !self.user_prompt.is_empty() {
            parts.push(self.user_prompt.clone());
        }

        parts.join("\n")
    }

    /// note: Stage-1 absent → Stage-0 output with no platform sections; otherwise [`Self::assemble`].
    #[must_use]
    pub fn assemble_with_fallback(&self) -> String {
        if self.stage1_response.is_none() {
            return self.assemble_stage0_fallback();
        }
        self.assemble()
    }

    fn assemble_stage0_fallback(&self) -> String {
        let mut parts = Vec::new();

        if !self.system_prefix.is_empty() {
            parts.push(self.system_prefix.clone());
        }
        if !self.personality.is_empty() {
            parts.push(format!("{PERSONALITY_HEADING}\n\n{}\n", self.personality));
        }

        let sorted = self.sorted_memories();
        for mem in &sorted {
            let title = format!("### Memory: {}", memory_display_id(mem));
            parts.push(format!("{title}\n\n{}\n", memory_body(mem)));
        }

        let keywords = self.deduped_keywords();
        if !keywords.is_empty() {
            parts.push(format!(
                "{FRAGMENT_KEYWORDS_HEADING}\n\n{}\n",
                keywords.join(", ")
            ));
        }

        if !self.experience.is_empty() {
            parts.push(format!("{EXPERIENCE_HEADING}\n\n{}\n", self.experience));
        }

        if !self.user_prompt.is_empty() {
            parts.push(self.user_prompt.clone());
        }

        parts.join("\n")
    }

    fn deduped_platform_memories<'a>(&self, items: &'a [MemoryItemRef]) -> Vec<&'a MemoryItemRef> {
        let local_ids: HashSet<String> = self
            .long_term_memories
            .iter()
            .map(memory_display_id)
            .collect();
        items
            .iter()
            .filter(|item| !local_ids.contains(&item.memory_id))
            .collect()
    }

    fn sorted_memories(&self) -> Vec<&RetainedRecord> {
        let mut sorted: Vec<&RetainedRecord> = self.long_term_memories.iter().collect();
        sorted.sort_by(|a, b| {
            let kind_cmp = memory_kind(a).cmp(memory_kind(b));
            if kind_cmp != std::cmp::Ordering::Equal {
                return kind_cmp;
            }
            memory_updated_at(b).cmp(memory_updated_at(a))
        });
        sorted
    }

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

    fn make_memory(kind: &str, body: &str, updated_at: &str) -> RetainedRecord {
        crate::test_support::memory(&format!("mem_{kind}_{updated_at}"), kind, body, updated_at)
    }

    #[test]
    fn two_stage_assemble_with_stage1_data() {
        let asm = TwoStageAssembly {
            stage1_response: Some(AssembleResponse {
                memory_items: vec![MemoryItemRef {
                    memory_id: "mem_platform_1".to_string(),
                    content_summary: "Platform memory summary".to_string(),
                    relevance_score: Some(0.95),
                }],
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
            runtime_mode: RuntimeMode::LocalFirst,
        };

        let output = asm.assemble();
        assert!(output.contains("Platform memory summary"));
        assert!(output.contains("Keep worlds consistent."));
        assert!(output.contains("session_created"));
    }

    #[test]
    fn two_stage_fallback_when_stage1_none() {
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
            runtime_mode: RuntimeMode::LocalFirst,
        };

        let output = asm.assemble_with_fallback();
        assert!(output.contains(PERSONALITY_HEADING));
        assert!(output.contains("A writer."));
        assert!(!output.contains("### Platform Memory Items"));
    }

    #[test]
    fn memory_dedup_local_over_platform() {
        let local_mem = crate::test_support::memory(
            "mem_overlap",
            "story_summary",
            "LOCAL content wins.",
            "2026-04-14T00:00:00Z",
        );

        let asm = TwoStageAssembly {
            stage1_response: Some(AssembleResponse {
                memory_items: vec![
                    MemoryItemRef {
                        memory_id: "mem_overlap".to_string(),
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
            runtime_mode: RuntimeMode::CloudEnhanced,
        };

        let output = asm.assemble();
        assert!(output.contains("LOCAL content wins."));
        assert!(
            !output.contains("PLATFORM content should be deduped."),
            "platform memory with same ID as local should be deduped"
        );
        assert!(output.contains("Platform-only memory."));
    }
}
