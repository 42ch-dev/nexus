//! Two-stage context assembly for `local_first` / `cloud_enhanced` modes.
//!
//! Stage-1: Call platform `context/assemble` API.
//! Stage-2: Merge platform response with local SOUL + memories + fragments.
//!
//! Spec: `creator-memory-soul-lifecycle-v1.md` §9

use nexus_contracts::local::domain::RuntimeMode;
use nexus_creator_memory::LongTermMemory;
use std::collections::BTreeSet;
use std::collections::HashSet;

/// Section heading for personality (matches SOUL.md).
const PERSONALITY_HEADING: &str = "## Personality";

/// Section heading for experience (matches SOUL.md).
const EXPERIENCE_HEADING: &str = "## Experience";

/// Section heading for fragment keywords.
const FRAGMENT_KEYWORDS_HEADING: &str = "### Fragment keywords (deduped)";

/// Section heading for platform memory items.
const PLATFORM_MEMORY_HEADING: &str = "### Platform Memory Items";

/// Section heading for knowledge base entries.
const KB_HEADING: &str = "### Knowledge Base";

/// Section heading for timeline events.
const TIMELINE_HEADING: &str = "### Timeline Events";

/// Response from platform `context/assemble` API (Stage-1).
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

/// Domain wrapper for `RuntimeMode` used in two-stage assembly.
///
/// Reuses the contracts enum directly; provides downgrade/upgrade chain
/// for assembly merge rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AssemblyRuntimeMode(pub RuntimeMode);

impl AssemblyRuntimeMode {
    /// Create from generated enum.
    #[must_use]
    pub const fn new(mode: RuntimeMode) -> Self {
        Self(mode)
    }
}

impl std::fmt::Display for AssemblyRuntimeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Two-stage context assembly for `local_first` / `cloud_enhanced` modes.
///
/// Stage-1: Call platform `context/assemble` API.
/// Stage-2: Merge platform response with local SOUL + memories + fragments.
pub struct TwoStageAssembly {
    /// Stage-1 response from platform (optional — may fail or return empty).
    pub stage1_response: Option<AssembleResponse>,
    /// Local SOUL personality section.
    pub personality: String,
    /// Local SOUL experience section.
    pub experience: String,
    /// Local long-term memories.
    pub long_term_memories: Vec<LongTermMemory>,
    /// Local fragment keywords (union from `memory_fragments`).
    pub fragment_keywords: Vec<String>,
    /// User task prompt.
    pub user_prompt: String,
    /// System/policy prefix.
    pub system_prefix: String,
    /// Token budget.
    pub max_tokens: Option<usize>,
    /// Current runtime mode (for merge rules).
    pub runtime_mode: AssemblyRuntimeMode,
}

impl TwoStageAssembly {
    /// Assemble the final context (Stage-2 merge).
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
            let title = format!("### Memory: {}", mem.frontmatter.memory_id);
            parts.push(format!("{title}\n\n{}\n", mem.body));
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

    /// Assemble with fallback to Stage0-style output if Stage-1 failed.
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
            let title = format!("### Memory: {}", mem.frontmatter.memory_id);
            parts.push(format!("{title}\n\n{}\n", mem.body));
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

    fn sorted_memories(&self) -> Vec<&LongTermMemory> {
        let mut sorted: Vec<&LongTermMemory> = self.long_term_memories.iter().collect();
        sorted.sort_by(|a, b| {
            let kind_cmp = a.frontmatter.memory_kind.cmp(&b.frontmatter.memory_kind);
            if kind_cmp != std::cmp::Ordering::Equal {
                return kind_cmp;
            }
            b.frontmatter.updated_at.cmp(&a.frontmatter.updated_at)
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

    fn make_memory(kind: &str, body: &str, updated_at: &str) -> LongTermMemory {
        let mut mem = LongTermMemory::new(kind);
        mem.set_body(body);
        mem.frontmatter.updated_at = updated_at.to_string();
        mem
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
            runtime_mode: AssemblyRuntimeMode::new(RuntimeMode::LocalFirst),
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
            runtime_mode: AssemblyRuntimeMode::new(RuntimeMode::LocalFirst),
        };

        let output = asm.assemble_with_fallback();
        assert!(output.contains(PERSONALITY_HEADING));
        assert!(output.contains("A writer."));
        assert!(!output.contains("### Platform Memory Items"));
    }

    #[test]
    fn memory_dedup_local_over_platform() {
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
            runtime_mode: AssemblyRuntimeMode::new(RuntimeMode::CloudEnhanced),
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
