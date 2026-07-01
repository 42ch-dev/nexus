//! Creator-SOUL narrative synthesis seam (V1.81).
//!
//! Defines the trait boundary between the memory domain (`nexus-creator-memory`)
//! and the daemon's ACP invocation layer (`nexus-daemon-runtime`). The trait
//! lives here (no daemon dependency); the real `AcpSoulNarrativeSynthesizer`
//! adapter lives in `nexus-daemon-runtime`.

use crate::errors::MemoryError;

/// Capped input signal for the LLM synthesis prompt.
///
/// Constructed by the handler from the creator's accumulated fragment data
/// before calling [`SoulNarrativeSynthesizer::synthesize`].
#[derive(Debug, Clone)]
pub struct SoulNarrativeSynthesisInput {
    /// Top N keyword → count pairs (max 30).
    pub top_keywords: Vec<(String, u64)>,
    /// Recent fragment summaries (max 24, each ≤280 chars).
    pub recent_summaries: Vec<String>,
    /// Temporal buckets (max 8), each with a label and top 5 keywords.
    pub temporal_buckets: Vec<TemporalBucket>,
    /// Total fragment count.
    pub total_fragment_count: u64,
    /// Total distinct keyword count.
    pub distinct_keyword_count: u64,
    /// Oldest fragment `created_at`.
    pub oldest_created_at: Option<String>,
    /// Newest fragment `created_at`.
    pub newest_created_at: Option<String>,
}

/// A time window with its top keywords.
#[derive(Debug, Clone)]
pub struct TemporalBucket {
    /// Human-readable label for the time window.
    pub label: String,
    /// Top 5 keywords in this bucket.
    pub top_keywords: Vec<String>,
    /// Fragment count in this bucket.
    pub fragment_count: u64,
}

/// The result of a narrative synthesis call.
#[derive(Debug, Clone)]
pub struct SoulNarrativeDraft {
    /// The generated narrative text.
    pub narrative: String,
}

/// Synthesis seam for Creator-SOUL narrative generation (V1.81).
///
/// One production impl: `AcpSoulNarrativeSynthesizer` in `nexus-daemon-runtime`
/// that dispatches through the orchestration `CapabilityRegistry` → `acp.prompt`.
/// Tests use a mock synthesizer to avoid real ACP/LLM calls.
#[allow(async_fn_in_trait)]
pub trait SoulNarrativeSynthesizer: Send + Sync {
    /// Synthesize a Creator-SOUL narrative from the capped input signal.
    ///
    /// # Errors
    ///
    /// Returns `MemoryError` on synthesis failure (e.g. ACP unavailable,
    /// malformed LLM output).
    async fn synthesize(
        &self,
        creator_id: &str,
        input: SoulNarrativeSynthesisInput,
    ) -> Result<SoulNarrativeDraft, MemoryError>;
}
