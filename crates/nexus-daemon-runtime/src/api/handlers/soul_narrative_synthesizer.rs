//! ACP-backed `SoulNarrativeSynthesizer` adapter (V1.81).
//!
//! Bridges the `SoulNarrativeSynthesizer` trait (in `nexus-creator-memory`)
//! to the daemon's orchestration `CapabilityRegistry` → `acp.prompt` capability.
//! The adapter looks up the `acp.prompt` capability from the registry, builds
//! a prompt from the capped input signal, dispatches it, and extracts `full_text`
//! from the result.
//!
//! Missing registry/capability or `WorkerUnavailable` maps to `MemoryError`.

use nexus_creator_memory::soul_narrative::{
    SoulNarrativeDraft, SoulNarrativeSynthesisInput, SoulNarrativeSynthesizer,
};
use nexus_creator_memory::MemoryError;
use nexus_orchestration::capability::{CapabilityError, CapabilityRegistry};
use serde_json::json;
use std::sync::Arc;

/// ACP-backed synthesizer that dispatches through the capability registry.
pub struct AcpSoulNarrativeSynthesizer {
    registry: Arc<CapabilityRegistry>,
}

impl AcpSoulNarrativeSynthesizer {
    /// Construct from a shared capability registry.
    #[must_use]
    pub const fn new(registry: Arc<CapabilityRegistry>) -> Self {
        Self { registry }
    }

    /// Build the synthesis prompt from the capped input signal.
    ///
    /// The prompt instructs the LLM to produce a reflective narrative with:
    /// 1. **Specificity** — references at least two distinct theme keywords.
    /// 2. **Temporality** — references at least one shift or development over time.
    /// 3. **Actionable tone** — ends with a forward-looking reflection or question.
    fn build_prompt(input: &SoulNarrativeSynthesisInput) -> String {
        use std::fmt::Write;

        let mut prompt = String::new();

        // Header
        prompt.push_str(
            "You are a reflective creative-writing mentor synthesizing a Creator-SOUL narrative.\n\n",
        );
        prompt.push_str("The creator has accumulated the following creative fragments. ");
        prompt.push_str(
            "Synthesize a coherent, reflective narrative of their creative identity — \
             who they are becoming as a writer. The narrative must:\n",
        );
        prompt.push_str("1. Reference at least two distinct theme keywords from their work.\n");
        prompt.push_str("2. Reference at least one shift or development over time.\n");
        prompt.push_str("3. End with a forward-looking reflection or question.\n\n");
        prompt.push_str(
            "Do NOT produce a generic summary. Be specific and grounded in the data below.\n\n",
        );

        // Stats
        let _ = write!(
            prompt,
            "Total fragments: {}\nDistinct keywords: {}\n",
            input.total_fragment_count, input.distinct_keyword_count
        );
        if let Some(ref oldest) = input.oldest_created_at {
            let _ = write!(prompt, "Fragment span: {oldest}");
        }
        if let Some(ref newest) = input.newest_created_at {
            let _ = write!(prompt, " → {newest}");
        }
        prompt.push_str("\n\n");

        // Top keywords
        if !input.top_keywords.is_empty() {
            prompt.push_str("Top keywords (by frequency):\n");
            for (kw, count) in &input.top_keywords {
                let _ = writeln!(prompt, "  - {kw} ({count})");
            }
            prompt.push('\n');
        }

        // Temporal buckets
        if !input.temporal_buckets.is_empty() {
            prompt.push_str("Temporal evolution:\n");
            for bucket in &input.temporal_buckets {
                let _ = writeln!(
                    prompt,
                    "  {} ({} fragments): {}",
                    bucket.label,
                    bucket.fragment_count,
                    bucket.top_keywords.join(", ")
                );
            }
            prompt.push('\n');
        }

        // Recent summaries
        if !input.recent_summaries.is_empty() {
            prompt.push_str("Recent fragment summaries:\n");
            for (i, summary) in input.recent_summaries.iter().enumerate() {
                let _ = writeln!(prompt, "  {}. {summary}", i + 1);
            }
            prompt.push('\n');
        }

        prompt.push_str(
            "Now, write a reflective Creator-SOUL narrative (2-4 paragraphs) \
             synthesizing who this creator is becoming.",
        );

        prompt
    }
}

impl SoulNarrativeSynthesizer for AcpSoulNarrativeSynthesizer {
    async fn synthesize(
        &self,
        creator_id: &str,
        input: SoulNarrativeSynthesisInput,
    ) -> Result<SoulNarrativeDraft, MemoryError> {
        let cap = self.registry.get("acp.prompt").ok_or_else(|| {
            MemoryError::ValidationError(
                "acp.prompt capability not available in registry".to_string(),
            )
        })?;

        let prompt = Self::build_prompt(&input);

        let result = cap
            .run(json!({
                "prompt": prompt,
                "tool_policy": "deny_all",
                "_creator_id": creator_id,
                "_session_id": "soul_narrative_reflect"
            }))
            .await
            .map_err(|e| match e {
                CapabilityError::WorkerUnavailable => MemoryError::ValidationError(
                    "ACP worker unavailable for narrative synthesis".to_string(),
                ),
                other => {
                    MemoryError::ValidationError(format!("narrative synthesis failed: {other}"))
                }
            })?;

        let full_text = result
            .get("full_text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                MemoryError::ValidationError(
                    "acp.prompt response missing 'full_text' field".to_string(),
                )
            })?;

        Ok(SoulNarrativeDraft {
            narrative: full_text.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_creator_memory::soul_narrative::TemporalBucket;

    fn sample_input() -> SoulNarrativeSynthesisInput {
        SoulNarrativeSynthesisInput {
            top_keywords: vec![
                ("historical fiction".to_string(), 12),
                ("moral ambiguity".to_string(), 8),
                ("character voice".to_string(), 6),
            ],
            recent_summaries: vec![
                "Explored the moral dilemma of a war-time medic.".to_string(),
                "Developed a secondary character's backstory.".to_string(),
            ],
            temporal_buckets: vec![TemporalBucket {
                label: "Early (Apr-May)".to_string(),
                top_keywords: vec!["historical fiction".to_string(), "dialogue".to_string()],
                fragment_count: 5,
            }],
            total_fragment_count: 15,
            distinct_keyword_count: 25,
            oldest_created_at: Some("2026-04-01T00:00:00Z".to_string()),
            newest_created_at: Some("2026-07-01T00:00:00Z".to_string()),
        }
    }

    #[test]
    fn build_prompt_includes_all_sections() {
        let prompt = AcpSoulNarrativeSynthesizer::build_prompt(&sample_input());
        assert!(prompt.contains("Total fragments: 15"));
        assert!(prompt.contains("Distinct keywords: 25"));
        assert!(prompt.contains("historical fiction (12)"));
        assert!(prompt.contains("moral ambiguity (8)"));
        assert!(prompt.contains("Early (Apr-May)"));
        assert!(prompt.contains("Explored the moral dilemma"));
        assert!(prompt.contains("Developed a secondary character"));
        // The prompt must include structure guidance.
        assert!(prompt.contains("theme keywords"));
        assert!(prompt.contains("shift or development"));
        assert!(prompt.contains("forward-looking"));
    }
}
