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
use nexus_creator_memory::MemoryBearerRef;
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
    /// `subject` is `"creator"` or `"character"`; for a Character bearer the
    /// prompt reflects that Character's own identity (never the owner
    /// Creator's), while the worker is still routed by the owner Creator's id.
    ///
    /// The prompt instructs the LLM to produce a reflective narrative with:
    /// 1. **Specificity** — references at least two distinct theme keywords.
    /// 2. **Temporality** — references at least one shift or development over time.
    /// 3. **Actionable tone** — ends with a forward-looking reflection or question.
    fn build_prompt(input: &SoulNarrativeSynthesisInput, subject: &str) -> String {
        use std::fmt::Write;

        let mut prompt = String::new();

        // Header. The subject-specific strings are selected by an exhaustive
        // branch so the Creator arm is byte-for-byte identical to the legacy
        // prompt (capitalized "Creator-SOUL", "as a writer", "this creator is
        // becoming"), and the Character arm never references the Creator.
        let (header, body_intro, final_instruction) = match subject {
            "character" => (
                "You are a reflective creative-writing mentor synthesizing a Character-SOUL narrative.\n\n",
                "The character has accumulated the following creative fragments. "
                    .to_string()
                    + "Synthesize a coherent, reflective narrative of their creative identity — who they are becoming as a character. The narrative must:\n",
                "Now, write a reflective Character-SOUL narrative (2-4 paragraphs) synthesizing who this character is becoming.",
            ),
            _ => (
                "You are a reflective creative-writing mentor synthesizing a Creator-SOUL narrative.\n\n",
                "The creator has accumulated the following creative fragments. "
                    .to_string()
                    + "Synthesize a coherent, reflective narrative of their creative identity — who they are becoming as a writer. The narrative must:\n",
                "Now, write a reflective Creator-SOUL narrative (2-4 paragraphs) synthesizing who this creator is becoming.",
            ),
        };
        prompt.push_str(header);
        prompt.push_str(&body_intro);
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

        prompt.push_str(final_instruction);

        prompt
    }

    /// Deterministic ACP worker session key for a reflection.
    ///
    /// `character_id` is `Some` for a Character bearer, `None` for the Creator
    /// arm. A Character reflection is namespaced as
    /// `soul_narrative_reflect:{character_id}` and, when reflecting a
    /// binding-local scope, additionally `:{binding}` so two World lives never
    /// share a conversation. The Creator arm returns the legacy
    /// `soul_narrative_reflect` key (the owner `_creator_id` is already part of
    /// the IPC worker key).
    fn session_id(character_id: Option<&str>, binding: Option<&str>) -> String {
        match (character_id, binding) {
            (Some(chr), Some(binding)) => format!("soul_narrative_reflect:{chr}:{binding}"),
            (Some(chr), None) => format!("soul_narrative_reflect:{chr}"),
            (None, _) => "soul_narrative_reflect".to_string(),
        }
    }
}

impl SoulNarrativeSynthesizer for AcpSoulNarrativeSynthesizer {
    async fn synthesize(
        &self,
        bearer: MemoryBearerRef<'_>,
        input: SoulNarrativeSynthesisInput,
        session_scope: Option<&str>,
    ) -> Result<SoulNarrativeDraft, MemoryError> {
        // Worker routing identity: the owner Creator whose ACP worker is
        // registered. For a Character bearer this is `owner_creator_id`, NOT
        // the `chr_…` storage id (which would resolve to no worker or the
        // wrong worker). The Character identity is preserved as trusted
        // context.
        let identity = bearer.identity();
        let (subject, character_id) = match identity.character_id {
            Some(chr) => ("character", Some(chr)),
            None => ("creator", None),
        };
        let cap =
            self.registry
                .get("acp.prompt")
                .ok_or_else(|| MemoryError::CapabilityMissing {
                    capability: "acp.prompt".to_string(),
                })?;

        let prompt = Self::build_prompt(&input, subject);

        // Namespace the ACP worker session by bearer identity (and binding
        // scope, where local) so a Character/binding reflection never resumes
        // another bearer's or scope's conversation. The Creator arm keeps the
        // legacy global `soul_narrative_reflect` key (its `_creator_id` is
        // already part of the IPC key) so Creator reflect history is unchanged.
        let session_id = Self::session_id(identity.character_id, session_scope);
        let mut payload = json!({
            "prompt": prompt,
            "tool_policy": "deny_all",
            "_creator_id": identity.creator_id,
            "_session_id": session_id,
        });
        if let Some(chr) = character_id {
            payload["_character_id"] = json!(chr);
        }

        let result = cap
            .run(payload)
            .await
            .map_err(|e| match e {
                CapabilityError::WorkerUnavailable => MemoryError::WorkerUnavailable,
                other => {
                    MemoryError::ValidationError(format!("narrative synthesis failed: {other}"))
                }
            })?;

        let full_text = result
            .get("full_text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| MemoryError::MalformedOutput {
                reason: "acp.prompt response missing 'full_text' field".to_string(),
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
        let prompt = AcpSoulNarrativeSynthesizer::build_prompt(&sample_input(), "creator");
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

    /// The Creator arm prompt is byte-for-byte identical to the legacy
    /// prompt (capitalized "Creator-SOUL", "as a writer", "this creator is
    /// becoming").
    #[test]
    fn creator_prompt_is_byte_identical_to_legacy() {
        let creator = AcpSoulNarrativeSynthesizer::build_prompt(&sample_input(), "creator");
        assert!(creator.contains(
            "You are a reflective creative-writing mentor synthesizing a Creator-SOUL narrative.\n\n"
        ));
        assert!(creator.contains(
            "The creator has accumulated the following creative fragments. "
        ));
        assert!(creator.contains(
            "Synthesize a coherent, reflective narrative of their creative identity — \
             who they are becoming as a writer. The narrative must:\n"
        ));
        assert!(creator.ends_with(
            "Now, write a reflective Creator-SOUL narrative (2-4 paragraphs) \
             synthesizing who this creator is becoming."
        ));
        // No Character wording leaks into the Creator arm.
        assert!(!creator.contains("Character-SOUL"));
        assert!(!creator.contains("as a character"));
    }

    /// The Character arm prompt is fully Character-subject-aware and carries
    /// NO Creator-SOUL or "this creator" wording anywhere.
    #[test]
    fn character_prompt_has_no_creator_wording() {
        let character = AcpSoulNarrativeSynthesizer::build_prompt(&sample_input(), "character");
        assert!(character.contains(
            "You are a reflective creative-writing mentor synthesizing a Character-SOUL narrative.\n\n"
        ));
        assert!(character.contains(
            "The character has accumulated the following creative fragments. "
        ));
        assert!(character.contains(
            "Synthesize a coherent, reflective narrative of their creative identity — \
             who they are becoming as a character. The narrative must:\n"
        ));
        assert!(character.ends_with(
            "Now, write a reflective Character-SOUL narrative (2-4 paragraphs) \
             synthesizing who this character is becoming."
        ));
        // The Character instruction contains no Creator-SOUL or "this creator"
        // wording (acceptance criterion).
        assert!(
            !character.contains("Creator-SOUL"),
            "Character prompt must not reference Creator-SOUL: {character}"
        );
        assert!(
            !character.contains("this creator"),
            "Character prompt must not reference 'this creator': {character}"
        );
        assert!(!character.contains("as a writer"));
    }

    /// A Character bearer's synthesis is routed by its owner Creator id
    /// (worker lookup) while the Character storage identity is preserved and
    /// never substituted into the worker routing slot.
    #[test]
    fn character_synthesis_identity_routes_by_owner_not_character() {
        let owner = "ctr_0123456789abcdef0123456789abcdef";
        let chr = "chr_0123456789abcdef0123456789abcdef";
        let bearer = MemoryBearerRef::Character {
            owner_creator_id: owner,
            character_id: chr,
        };
        let ident = bearer.identity();
        assert_eq!(ident.creator_id, owner, "ACP worker routed by owner Creator");
        assert_eq!(ident.character_id, Some(chr));

        // Creator arm routes by itself and has no Character identity.
        let bearer = MemoryBearerRef::Creator(owner);
        let ident = bearer.identity();
        assert_eq!(ident.creator_id, owner);
        assert_eq!(ident.character_id, None);
    }
}

#[cfg(test)]
mod session_id_tests {
    use super::AcpSoulNarrativeSynthesizer;

    #[test]
    fn session_id_is_namespaced_by_bearer_and_binding() {
        // Creator arm keeps the legacy global key.
        assert_eq!(AcpSoulNarrativeSynthesizer::session_id(None, None), "soul_narrative_reflect");
        assert_eq!(
            AcpSoulNarrativeSynthesizer::session_id(None, Some("binding")),
            "soul_narrative_reflect",
            "Creator scope is not a binding namespace"
        );

        // Character bearer is namespaced by character id.
        assert_eq!(
            AcpSoulNarrativeSynthesizer::session_id(Some("chrA"), None),
            "soul_narrative_reflect:chrA"
        );
        // Binding-local reflection adds the binding scope.
        assert_eq!(
            AcpSoulNarrativeSynthesizer::session_id(Some("chrA"), Some("bind1")),
            "soul_narrative_reflect:chrA:bind1"
        );

        // Distinctive both across bearers and across scopes.
        assert_ne!(
            AcpSoulNarrativeSynthesizer::session_id(Some("chrA"), None),
            AcpSoulNarrativeSynthesizer::session_id(Some("chrB"), None)
        );
        assert_ne!(
            AcpSoulNarrativeSynthesizer::session_id(Some("chrA"), Some("bind1")),
            AcpSoulNarrativeSynthesizer::session_id(Some("chrA"), Some("bind2"))
        );
    }
}
