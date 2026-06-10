//! Review algorithm for memory pipeline.
//!
//! Implements classification heuristics for pending review entries,
//! determining whether to drop, fragment, promote to long-term memory,
//! merge, or trigger SOUL experience aggregation.
//!
//! Also provides promotion functions for converting pending reviews
//! into long-term memory files with idempotency guarantees.
//!
//! See creator-memory-soul-lifecycle-v1.md §7.2, §7.3.

use std::collections::HashSet;
use std::future::Future;
use std::path::Path;
use std::str::FromStr;

use crate::errors::MemoryError;
use crate::long_term_memory::LongTermMemory;
use crate::review_quality::is_high_signal;

use serde::{Deserialize, Serialize};

/// Review action determined by classification algorithm.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAction {
    /// Not worth keeping — discard the pending entry.
    Drop,
    /// Keep as lightweight keyword fragment.
    FragmentOnly,
    /// Create or update long-term memory Markdown file.
    PromoteToLongTerm,
    /// Merge content into existing long-term memory.
    MergeIntoExisting,
    /// Only trigger SOUL experience aggregation (metadata update).
    TriggerSoulExperienceOnly,
}

/// Decision from review classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewDecision {
    /// The pending review entry ID.
    pub pending_id: String,
    /// The determined action.
    pub action: ReviewAction,
    /// Reason for the decision (for audit).
    pub reason: String,
}

/// Task kinds that can be detected from session metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskKind {
    Brainstorm,
    Outline,
    Chapter,
    Research,
    Unknown,
}

/// Error type for `TaskKind` parsing (always succeeds, Unknown is fallback).
#[derive(Debug, Clone)]
pub struct ParseTaskKindError(());

impl std::fmt::Display for ParseTaskKindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid task kind")
    }
}

impl std::error::Error for ParseTaskKindError {}

impl FromStr for TaskKind {
    type Err = ParseTaskKindError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "brainstorm" => Ok(Self::Brainstorm),
            "outline" => Ok(Self::Outline),
            "chapter" => Ok(Self::Chapter),
            "research" => Ok(Self::Research),
            "unknown" => Ok(Self::Unknown),
            other => {
                // S-002: Log when an unrecognized task_kind is encountered
                tracing::warn!(
                    task_kind = other,
                    "Unrecognized task_kind, falling back to Unknown"
                );
                Ok(Self::Unknown) // Fallback to Unknown for invalid values
            }
        }
    }
}

impl TaskKind {
    #[must_use]
    /// Convert to string representation.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Brainstorm => "brainstorm",
            Self::Outline => "outline",
            Self::Chapter => "chapter",
            Self::Research => "research",
            Self::Unknown => "unknown",
        }
    }

    /// Parse task kind from string (convenience wrapper that never fails).
    #[must_use]
    pub fn parse_fallible(s: &str) -> Self {
        s.parse().unwrap_or(Self::Unknown)
    }
}

/// Pending review data for classification (input from DB).
#[derive(Debug, Clone)]
pub struct PendingReviewInput {
    pub pending_id: String,
    pub session_id: String,
    pub creator_id: String,
    pub world_id: Option<String>,
    pub task_kind: String,
    pub raw_digest: String,
    pub created_at: String,
}

/// Classify a pending review entry using rule-based heuristics.
///
/// Initial implementation is deterministic (no LLM). LLM-assisted classification
/// can be added later when needed.
///
/// # Classification Rules (§7.2)
///
/// - **Drop**: Very short digest (< 50 chars) or `task_kind` is diagnostic/noise.
/// - **`FragmentOnly`**: Medium-length digest, no clear long-term value.
///   Task kinds: "research", "exploration" (informational only).
/// - **`PromoteToLongTerm`**: Substantial digest with clear experience value.
///   Task kinds: "chapter", "outline", "brainstorm" with rich content.
/// - **`MergeIntoExisting`**: Content overlaps with existing memory (Phase 2 feature).
/// - **`TriggerSoulExperienceOnly`**: Metadata-only update, no new content.
///
/// # Example
///
/// ```rust
/// use nexus_creator_memory::review::{classify_pending_review, PendingReviewInput};
///
/// let input = PendingReviewInput {
///     pending_id: "pending_001".to_string(),
///     session_id: "sess_001".to_string(),
///     creator_id: "ctr_test".to_string(),
///     world_id: None,
///     task_kind: "brainstorm".to_string(),
///     raw_digest: "Discussed three key themes for the novel: narrative structure, character arcs, and emotional resonance. Explored how these interweave to create compelling storytelling.".to_string(),
///     created_at: "2026-04-14T10:00:00Z".to_string(),
/// };
///
/// let decision = classify_pending_review(&input);
/// match decision.action {
///     nexus_creator_memory::review::ReviewAction::PromoteToLongTerm => (),
///     _ => panic!("expected PromoteToLongTerm"),
/// }
/// ```
pub fn classify_pending_review(record: &PendingReviewInput) -> ReviewDecision {
    let digest_len = record.raw_digest.len();
    let task_kind: TaskKind = {
        let parsed: TaskKind = record.task_kind.parse().unwrap_or(TaskKind::Unknown);
        if parsed == TaskKind::Unknown && record.task_kind.to_lowercase() != "unknown" {
            // S-002: Already logged in FromStr, but ensure classification is aware
            tracing::warn!(
                task_kind = %record.task_kind,
                pending_id = %record.pending_id,
                "Classifying with Unknown task_kind due to unrecognized value"
            );
        }
        parsed
    };

    // Threshold constants
    const DROP_THRESHOLD: usize = 50; // Very short = no meaningful content
    const HIGH_SIGNAL_MIN_LENGTH: usize = 80; // High-signal creative content can promote shorter
    const DEFAULT_PROMOTE_THRESHOLD: usize = 200; // Unknown tasks promote at this length

    // Rule 1: Creative tasks use quality signal for promotion decisions
    if matches!(
        task_kind,
        TaskKind::Brainstorm | TaskKind::Outline | TaskKind::Chapter
    ) {
        // Creative tasks never get dropped based solely on length (experience value).
        // Promotion requires both minimum length AND high quality signal.
        if is_high_signal(&record.raw_digest) && digest_len >= HIGH_SIGNAL_MIN_LENGTH {
            return ReviewDecision {
                pending_id: record.pending_id.clone(),
                action: ReviewAction::PromoteToLongTerm,
                reason: format!(
                    "{} task with high-signal digest ({} chars) — long-term value",
                    task_kind.as_str(),
                    digest_len
                ),
            };
        }
        // Creative tasks with medium/low-signal digest go to fragment
        return ReviewDecision {
            pending_id: record.pending_id.clone(),
            action: ReviewAction::FragmentOnly,
            reason: format!(
                "{} task with medium or low-signal digest — fragment indexing",
                task_kind.as_str()
            ),
        };
    }

    // Rule 2: Drop if digest is very short (< 50 chars) for non-creative tasks
    if digest_len < DROP_THRESHOLD {
        return ReviewDecision {
            pending_id: record.pending_id.clone(),
            action: ReviewAction::Drop,
            reason: "Digest too short (< 50 chars) — no meaningful content".to_string(),
        };
    }

    // Rule 3: FragmentOnly for research/exploration tasks
    // These are informational but not typically worth long-term storage
    if task_kind == TaskKind::Research {
        return ReviewDecision {
            pending_id: record.pending_id.clone(),
            action: ReviewAction::FragmentOnly,
            reason: "Research task — informational, fragment indexing sufficient".to_string(),
        };
    }

    // Rule 4: Unknown tasks - medium digest -> FragmentOnly, substantial -> PromoteToLongTerm
    if digest_len < DEFAULT_PROMOTE_THRESHOLD {
        return ReviewDecision {
            pending_id: record.pending_id.clone(),
            action: ReviewAction::FragmentOnly,
            reason: "Medium-length digest — fragment indexing sufficient".to_string(),
        };
    }

    // Default: PromoteToLongTerm for substantial content with unknown task kind
    ReviewDecision {
        pending_id: record.pending_id.clone(),
        action: ReviewAction::PromoteToLongTerm,
        reason: format!(
            "Substantial digest ({} chars) — long-term value",
            digest_len
        ),
    }
}

/// Memory fragment data for creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFragment {
    /// Unique fragment ID.
    pub fragment_id: String,
    /// Session ID that generated this fragment.
    pub session_id: String,
    /// Creator ID for ownership.
    pub creator_id: String,
    /// Keywords extracted from digest.
    pub keywords: Vec<String>,
    /// Short summary.
    pub summary: String,
    /// Creation timestamp.
    pub created_at: String,
    /// Optional TTL.
    pub ttl: Option<String>,
}

/// Create a memory fragment from a pending review entry.
///
/// This is a pure data transformation — no LLM needed.
/// Keywords are extracted by simple text analysis:
/// - Split on spaces and punctuation
/// - Filter stop words
/// - Take top N keywords by frequency
///
/// Summary is truncated `raw_digest` or first N chars.
///
/// # Example
///
/// ```rust
/// use nexus_creator_memory::review::{create_fragment_from_review, PendingReviewInput};
///
/// let input = PendingReviewInput {
///     pending_id: "pending_001".to_string(),
///     session_id: "sess_001".to_string(),
///     creator_id: "ctr_test".to_string(),
///     world_id: None,
///     task_kind: "research".to_string(),
///     raw_digest: "Explored three key concepts: narrative structure, character development, and pacing.".to_string(),
///     created_at: "2026-04-14T10:00:00Z".to_string(),
/// };
///
/// let fragment = create_fragment_from_review(&input);
/// assert!(!fragment.keywords.is_empty());
/// ```
#[must_use]
pub fn create_fragment_from_review(record: &PendingReviewInput) -> MemoryFragment {
    // Generate fragment ID (derived from pending_id with frag_ prefix)
    let fragment_id = format!("frag_{}", record.pending_id);

    // Extract keywords from raw_digest
    let keywords = extract_keywords(&record.raw_digest);

    // Generate summary (truncate to 200 chars max)
    let summary = if record.raw_digest.len() > 200 {
        format!("{}...", &record.raw_digest[..197])
    } else {
        record.raw_digest.clone()
    };

    // Default TTL based on task kind
    // V1.2 residual R9 (pipeline, nit): Fragment TTL has no cleanup mechanism
    // Fragment TTL cleanup deferred to V1.4; expired fragments consume disk until manual cleanup
    let ttl = match record
        .task_kind
        .parse::<TaskKind>()
        .unwrap_or(TaskKind::Unknown)
    {
        TaskKind::Research => Some("90d".to_string()), // Research expires faster
        TaskKind::Brainstorm | TaskKind::Outline => Some("180d".to_string()),
        TaskKind::Chapter => Some("365d".to_string()),
        TaskKind::Unknown => Some("30d".to_string()),
    };

    MemoryFragment {
        fragment_id,
        session_id: record.session_id.clone(),
        creator_id: record.creator_id.clone(),
        keywords,
        summary,
        created_at: record.created_at.clone(),
        ttl,
    }
}

/// Extract keywords from text using simple heuristics.
///
/// - Split on whitespace and punctuation
/// - Filter common stop words
/// - Lowercase and dedupe
/// - Limit to top 10 keywords
#[allow(clippy::too_many_lines)]
fn extract_keywords(text: &str) -> Vec<String> {
    // Common English stop words to filter out
    const STOP_WORDS: &[&str] = &[
        "the",
        "a",
        "an",
        "and",
        "or",
        "but",
        "in",
        "on",
        "at",
        "to",
        "for",
        "of",
        "with",
        "by",
        "from",
        "as",
        "is",
        "was",
        "are",
        "were",
        "been",
        "be",
        "have",
        "has",
        "had",
        "do",
        "does",
        "did",
        "will",
        "would",
        "could",
        "should",
        "may",
        "might",
        "must",
        "shall",
        "can",
        "need",
        "that",
        "this",
        "these",
        "those",
        "it",
        "its",
        "they",
        "them",
        "their",
        "we",
        "our",
        "you",
        "your",
        "he",
        "him",
        "his",
        "she",
        "her",
        "i",
        "me",
        "my",
        "not",
        "no",
        "yes",
        "so",
        "if",
        "then",
        "else",
        "when",
        "where",
        "how",
        "why",
        "what",
        "who",
        "which",
        "all",
        "each",
        "every",
        "both",
        "few",
        "more",
        "most",
        "other",
        "some",
        "such",
        "only",
        "own",
        "same",
        "than",
        "too",
        "very",
        "just",
        "also",
        "now",
        "here",
        "there",
        "then",
        // Session-specific noise
        "session",
        "task",
        "output",
        "file",
        "files",
        "path",
        "command",
        "result",
        "success",
        "error",
        "failed",
        "done",
        "completed",
        "started",
        "ended",
    ];

    // Split on whitespace and punctuation
    let words: Vec<String> = text
        .to_lowercase()
        .split(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '.' | ',' | ';' | ':' | '!' | '?' | '-' | '(' | ')' | '[' | ']'
                )
        })
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    // Filter stop words and dedupe using HashSet for O(1) lookups (R13).
    let mut seen: HashSet<String> = HashSet::new();
    let mut keywords: Vec<String> = Vec::new();
    for word in words {
        // Skip stop words
        if STOP_WORDS.contains(&word.as_str()) {
            continue;
        }
        // Skip very short words (likely noise)
        if word.len() < 3 {
            continue;
        }
        // Skip numbers
        if word.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        // Dedupe via HashSet (O(1) instead of Vec::contains O(N))
        if seen.insert(word.clone()) {
            keywords.push(word);
        }
    }

    // Limit to top 10 keywords
    keywords.truncate(10);
    keywords
}

// ──────────────────────────────────────────────────────────────────────────
// Promotion functions (T5.9, T5.10)
// ──────────────────────────────────────────────────────────────────────────

/// Trait for summarizing session digests into long-term memory content.
///
/// This abstracts the ACP-based summarization so the domain layer
/// doesn't need to depend on the CLI's ACP client directly.
/// Implementations can use local ACP agents or other summarizers.
///
/// The trait is designed for async operations since ACP calls are async.
#[allow(async_fn_in_trait)]
pub trait SessionDigestSummarizer: Send + Sync {
    /// Summarize a session digest into a long-term memory markdown body.
    ///
    /// The implementation should:
    /// - Generate a well-structured memory entry
    /// - Include key facts, decisions, and context from the session
    /// - Produce output suitable for markdown storage
    ///
    /// Returns the markdown body (without frontmatter, which is added by `LongTermMemory`).
    fn summarize(
        &self,
        session_id: &str,
        task_kind: &str,
        raw_digest: &str,
        world_id: Option<&str>,
    ) -> impl Future<Output = Result<String, MemoryError>> + Send;
}

/// Check if a session has already been promoted to long-term memory.
///
/// Idempotency check (T5.10): The same `session_id` must not produce
/// duplicate long-term memories. This function scans all memories
/// for the creator and checks their `source_session_ids` frontmatter field.
///
// V1.2 residual R7 (pipeline, nit): O(N) idempotency check on promotion
// O(N) file scan acceptable at current scale; optimize if review count exceeds ~1000
///
/// # Errors
/// Returns `Err(MemoryError::...)` if validation fails.
///
/// Returns `true` if the session is already present in any memory's
/// `source_session_ids` list.
///
/// # Example
///
/// ```rust
/// use std::path::PathBuf;
/// use nexus_creator_memory::review::check_session_already_promoted;
///
/// let home = PathBuf::from("/tmp/test_home");
/// let already = check_session_already_promoted(&home, "ctr_test", "sess_123").unwrap();
/// if already {
///     println!("Session already promoted — skipping");
/// }
/// ```
pub fn check_session_already_promoted(
    home: &Path,
    creator_id: &str,
    session_id: &str,
) -> Result<bool, MemoryError> {
    // List all long-term memories for this creator
    let slugs = crate::memory_io::list_memories(home, creator_id)?;

    // Check each memory's source_session_ids
    for slug in slugs {
        if let Ok(memory) = crate::memory_io::load_memory(home, creator_id, &slug) {
            if memory
                .frontmatter
                .source_session_ids
                .contains(&session_id.to_string())
            {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Promote a pending review to a long-term memory file.
///
/// This function (T5.9):
/// 1. Checks idempotency (skips if session already promoted)
/// 2. Calls the summarizer to generate memory content
/// 3. Creates a `LongTermMemory` with the summarized content
/// 4. Saves the memory via `memory_io::save_memory`
/// 5. Adds the `session_id` to `source_session_ids`
///
/// The `memory_kind` is determined from `task_kind`:
/// - "brainstorm" → "`story_summary`"
/// - "outline" → "`plot_outline`"
/// - "chapter" → "`story_summary`"
/// - "research" → "`research_material`"
/// - default → "custom"
///
/// Returns the created `LongTermMemory` with its `memory_id`.
///
/// # Errors
///
/// Returns `MemoryError::ValidationError` if:
/// - Session already promoted (idempotency check)
/// - Summarizer fails (graceful degradation)
/// - Memory save fails
///
/// # Example
///
/// ```rust
/// use std::path::PathBuf;
/// use nexus_creator_memory::review::{promote_to_long_term, PendingReviewInput, SessionDigestSummarizer};
/// use nexus_creator_memory::errors::MemoryError;
///
/// struct MockSummarizer;
/// impl SessionDigestSummarizer for MockSummarizer {
///     async fn summarize(&self, _: &str, _: &str, _: &str, _: Option<&str>) -> Result<String, MemoryError> {
///         Ok("This is a summarized memory entry.".to_string())
///     }
/// }
///
/// // Example usage (async context):
/// // let home = PathBuf::from("/tmp/test_home");
/// // let input = PendingReviewInput { ... };
/// // let summarizer = MockSummarizer;
/// // let memory = promote_to_long_term(&home, "ctr_test", &input, &summarizer).await.unwrap();
/// ```
pub async fn promote_to_long_term<S: SessionDigestSummarizer>(
    home: &Path,
    creator_id: &str,
    record: &PendingReviewInput,
    summarizer: &S,
) -> Result<LongTermMemory, MemoryError> {
    // 1. Check idempotency
    if check_session_already_promoted(home, creator_id, &record.session_id)? {
        return Err(MemoryError::ValidationError(format!(
            "Session '{}' already promoted to long-term memory",
            record.session_id
        )));
    }

    // R-V133P4-06: Size guard — cap raw_digest before summarization to prevent
    // unbounded LTM file growth. 256 KiB is generous for a session digest.
    const MAX_DIGEST_BYTES: usize = 256 * 1024;
    let raw_digest = if record.raw_digest.len() > MAX_DIGEST_BYTES {
        tracing::warn!(
            session_id = %record.session_id,
            digest_len = record.raw_digest.len(),
            max = MAX_DIGEST_BYTES,
            "raw_digest exceeds max_digest_bytes; truncating before summarization"
        );
        &record.raw_digest[..MAX_DIGEST_BYTES]
    } else {
        &record.raw_digest
    };

    // 2. Call summarizer to generate content
    let body = summarizer
        .summarize(
            &record.session_id,
            &record.task_kind,
            raw_digest,
            record.world_id.as_deref(),
        )
        .await?;

    // 3. Determine memory_kind from task_kind
    let memory_kind = task_kind_to_memory_kind(&record.task_kind);

    // 4. Create LongTermMemory
    let mut memory = LongTermMemory::new(memory_kind);
    memory.set_body(&body);
    memory.add_source_session(&record.session_id);

    // 5. Validate before saving
    memory.validate()?;

    // 6. Generate slug from memory_id (strip mem_ prefix)
    // V1.2 residual R10 (pipeline, nit): Slug collision risk (truncation)
    // Slug collision probability low at expected scale; add collision detection if needed
    let slug = memory.frontmatter.memory_id.replace("mem_", "memory-");

    // 7. Save via memory_io
    crate::memory_io::save_memory(home, creator_id, &slug, &memory)?;

    tracing::info!(
        memory_id = %memory.frontmatter.memory_id,
        session_id = %record.session_id,
        memory_kind = %memory_kind,
        "Promoted session to long-term memory"
    );

    Ok(memory)
}

/// Map `task_kind` to `memory_kind` for promotion.
///
/// This determines the appropriate `memory_kind` field in the
/// long-term memory frontmatter based on the session's task type.
fn task_kind_to_memory_kind(task_kind: &str) -> &'static str {
    match task_kind.to_lowercase().as_str() {
        "brainstorm" => "story_summary",
        "outline" => "plot_outline",
        "chapter" => "story_summary",
        "research" => "research_material",
        _ => "custom",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_input(task_kind: &str, raw_digest: &str) -> PendingReviewInput {
        PendingReviewInput {
            pending_id: "pending_test".to_string(),
            session_id: "sess_test".to_string(),
            creator_id: "ctr_test".to_string(),
            world_id: None,
            task_kind: task_kind.to_string(),
            raw_digest: raw_digest.to_string(),
            created_at: "2026-04-14T10:00:00Z".to_string(),
        }
    }

    #[test]
    fn classify_drop_short_digest() {
        let input = sample_input("unknown", "Short text");
        let decision = classify_pending_review(&input);
        assert_eq!(decision.action, ReviewAction::Drop);
        assert!(decision.reason.contains("too short"));
    }

    #[test]
    fn classify_fragment_only_research() {
        let input = sample_input(
            "research",
            "This is a research summary with enough content to pass the length check.",
        );
        let decision = classify_pending_review(&input);
        assert_eq!(decision.action, ReviewAction::FragmentOnly);
        assert!(decision.reason.contains("Research task"));
    }

    #[test]
    fn classify_promote_long_term_brainstorm() {
        let input = sample_input(
            "brainstorm",
            "Discussed three key themes for the novel: narrative structure, character arcs, and emotional resonance. Explored how these interweave to create compelling storytelling.",
        );
        let decision = classify_pending_review(&input);
        assert_eq!(decision.action, ReviewAction::PromoteToLongTerm);
        assert!(decision.reason.contains("brainstorm"));
    }

    #[test]
    fn classify_fragment_only_medium_digest() {
        let input = sample_input("unknown", "This is a medium-length digest that has some content but not enough to be substantial.");
        let decision = classify_pending_review(&input);
        assert_eq!(decision.action, ReviewAction::FragmentOnly);
        assert!(decision.reason.contains("Medium-length"));
    }

    #[test]
    fn classify_drop_unknown_short() {
        let input = sample_input("unknown", "Very short text with unknown task kind.");
        let decision = classify_pending_review(&input);
        assert_eq!(decision.action, ReviewAction::Drop);
        assert!(decision.reason.contains("too short"));
    }

    #[test]
    fn classify_promote_chapter() {
        let input = sample_input(
            "chapter",
            "Completed chapter five revisions: tightened the opening dialogue, improved pacing in the middle section, and strengthened the emotional climax at the end.",
        );
        let decision = classify_pending_review(&input);
        assert_eq!(decision.action, ReviewAction::PromoteToLongTerm);
        assert!(decision.reason.contains("chapter"));
    }

    #[test]
    fn create_fragment_extracts_keywords() {
        let input = sample_input(
            "research",
            "Explored narrative structure, character development, and pacing techniques.",
        );
        let fragment = create_fragment_from_review(&input);
        assert!(!fragment.keywords.is_empty());
        // Stop words should be filtered
        assert!(!fragment.keywords.contains(&"and".to_string()));
        // Should have substantive keywords
        assert!(fragment
            .keywords
            .iter()
            .any(|k| k.contains("narrative") || k.contains("character")));
    }

    #[test]
    fn create_fragment_truncates_summary() {
        let long_digest = "This is a very long digest that needs to be truncated because it exceeds the maximum allowed summary length of 200 characters. We want to make sure that the fragment summary is properly truncated with an ellipsis at the end so readers know there's more content.";
        let input = sample_input("research", long_digest);
        let fragment = create_fragment_from_review(&input);
        assert!(fragment.summary.len() <= 200);
        assert!(fragment.summary.ends_with("..."));
    }

    #[test]
    fn create_fragment_sets_ttl_by_task_kind() {
        let research = sample_input("research", "Research content summary here.");
        let research_fragment = create_fragment_from_review(&research);
        assert_eq!(research_fragment.ttl, Some("90d".to_string()));

        let chapter = sample_input(
            "chapter",
            "Chapter revision notes with sufficient content for testing.",
        );
        let chapter_fragment = create_fragment_from_review(&chapter);
        assert_eq!(chapter_fragment.ttl, Some("365d".to_string()));
    }

    #[test]
    fn extract_keywords_filters_stop_words() {
        let keywords = extract_keywords("The narrative structure and character development");
        assert!(!keywords.contains(&"the".to_string()));
        assert!(!keywords.contains(&"and".to_string()));
        assert!(keywords.contains(&"narrative".to_string()));
        assert!(keywords.contains(&"structure".to_string()));
        assert!(keywords.contains(&"character".to_string()));
        assert!(keywords.contains(&"development".to_string()));
    }

    #[test]
    fn extract_keywords_limits_to_ten() {
        let text = "one two three four five six seven eight nine ten eleven twelve thirteen";
        let keywords = extract_keywords(text);
        assert_eq!(keywords.len(), 10);
    }

    #[test]
    fn extract_keywords_filters_short_words() {
        let keywords = extract_keywords("This is a big important concept");
        assert!(!keywords.contains(&"is".to_string()));
        assert!(!keywords.contains(&"a".to_string()));
        assert!(keywords.contains(&"big".to_string()));
        assert!(keywords.contains(&"important".to_string()));
        assert!(keywords.contains(&"concept".to_string()));
    }

    #[test]
    fn task_kind_parsing() {
        assert_eq!(
            "brainstorm".parse::<TaskKind>().unwrap(),
            TaskKind::Brainstorm
        );
        assert_eq!(
            "BRAINSTORM".parse::<TaskKind>().unwrap(),
            TaskKind::Brainstorm
        );
        assert_eq!("outline".parse::<TaskKind>().unwrap(), TaskKind::Outline);
        assert_eq!("chapter".parse::<TaskKind>().unwrap(), TaskKind::Chapter);
        assert_eq!("research".parse::<TaskKind>().unwrap(), TaskKind::Research);
        assert_eq!("unknown".parse::<TaskKind>().unwrap(), TaskKind::Unknown);
        // Invalid values fall back to Unknown
        assert_eq!("invalid".parse::<TaskKind>().unwrap(), TaskKind::Unknown);
    }

    #[test]
    fn review_action_serialization() {
        let action = ReviewAction::PromoteToLongTerm;
        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(json, "\"promote_to_long_term\"");

        let parsed: ReviewAction = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ReviewAction::PromoteToLongTerm);
    }

    #[test]
    fn classify_promote_default_substantial() {
        let input = sample_input(
            "unknown",
            "This is a substantial piece of content that exceeds two hundred characters and should be promoted to long-term memory by default when the task kind is unknown. This additional sentence ensures the total length passes the threshold.",
        );
        let decision = classify_pending_review(&input);
        assert_eq!(decision.action, ReviewAction::PromoteToLongTerm);
        assert!(decision.reason.contains("Substantial digest"));
    }

    #[test]
    fn classify_outline_medium() {
        let input = sample_input("outline", "Brief outline discussion notes.");
        let decision = classify_pending_review(&input);
        assert_eq!(decision.action, ReviewAction::FragmentOnly);
        assert!(decision.reason.contains("outline"));
    }

    // ── Promotion tests ────────────────────────────────────────────────────

    #[test]
    fn task_kind_to_memory_kind_mapping() {
        assert_eq!(task_kind_to_memory_kind("brainstorm"), "story_summary");
        assert_eq!(task_kind_to_memory_kind("BRAINSTORM"), "story_summary");
        assert_eq!(task_kind_to_memory_kind("outline"), "plot_outline");
        assert_eq!(task_kind_to_memory_kind("chapter"), "story_summary");
        assert_eq!(task_kind_to_memory_kind("research"), "research_material");
        assert_eq!(task_kind_to_memory_kind("unknown"), "custom");
        assert_eq!(task_kind_to_memory_kind("invalid"), "custom");
    }

    #[test]
    fn check_session_already_promoted_returns_false_when_no_memories() {
        let home = std::path::PathBuf::from("/tmp/test_promotion_empty");
        let _ = std::fs::remove_dir_all(&home);

        let result = check_session_already_promoted(&home, "ctr_test", "sess_123");
        assert!(result.is_ok());
        assert!(!result.unwrap());

        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn promote_to_long_term_creates_valid_memory() {
        use std::path::PathBuf;

        // Mock summarizer
        struct MockSummarizer;
        impl SessionDigestSummarizer for MockSummarizer {
            async fn summarize(
                &self,
                _session_id: &str,
                _task_kind: &str,
                _raw_digest: &str,
                _world_id: Option<&str>,
            ) -> Result<String, MemoryError> {
                Ok("This is a summarized memory entry from the session.".to_string())
            }
        }

        let home = PathBuf::from("/tmp/test_promotion_create");
        let _ = std::fs::remove_dir_all(&home);

        let input = sample_input("brainstorm", "Session digest for testing promotion.");
        let summarizer = MockSummarizer;

        let memory = promote_to_long_term(&home, "ctr_test", &input, &summarizer)
            .await
            .unwrap();

        // Check memory properties
        assert!(memory.frontmatter.memory_id.starts_with("mem_"));
        assert_eq!(memory.frontmatter.memory_kind, "story_summary");
        assert!(memory.body.contains("summarized memory entry"));
        assert!(memory
            .frontmatter
            .source_session_ids
            .contains(&"sess_test".to_string()));
        assert!(memory.validate().is_ok());

        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn promote_to_long_term_rejects_duplicate_session() {
        use std::path::PathBuf;

        struct MockSummarizer;
        impl SessionDigestSummarizer for MockSummarizer {
            async fn summarize(
                &self,
                _: &str,
                _: &str,
                _: &str,
                _: Option<&str>,
            ) -> Result<String, MemoryError> {
                Ok("Summarized content.".to_string())
            }
        }

        let home = PathBuf::from("/tmp/test_promotion_idempotent");
        let _ = std::fs::remove_dir_all(&home);

        let input = sample_input("chapter", "First promotion.");
        let summarizer = MockSummarizer;

        // First promotion succeeds
        let _ = promote_to_long_term(&home, "ctr_test", &input, &summarizer)
            .await
            .unwrap();

        // Second promotion with same session_id fails
        let result = promote_to_long_term(&home, "ctr_test", &input, &summarizer).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("already promoted"));

        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn promote_to_long_term_gracefully_handles_summarizer_failure() {
        use std::path::PathBuf;

        struct FailingSummarizer;
        impl SessionDigestSummarizer for FailingSummarizer {
            async fn summarize(
                &self,
                _: &str,
                _: &str,
                _: &str,
                _: Option<&str>,
            ) -> Result<String, MemoryError> {
                Err(MemoryError::ValidationError(
                    "Summarizer unavailable".to_string(),
                ))
            }
        }

        let home = PathBuf::from("/tmp/test_promotion_failure");
        let _ = std::fs::remove_dir_all(&home);

        let input = sample_input("brainstorm", "Session to promote.");
        let summarizer = FailingSummarizer;

        let result = promote_to_long_term(&home, "ctr_test", &input, &summarizer).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Summarizer unavailable"));

        let _ = std::fs::remove_dir_all(&home);
    }

    // ── Quality signal tests (T1) ──────────────────────────────────────────

    #[test]
    fn classify_rejects_long_low_signal_noise() {
        let input = PendingReviewInput {
            pending_id: "p1".into(),
            session_id: "s1".into(),
            creator_id: "ctr_test".into(),
            world_id: None,
            task_kind: "brainstorm".into(),
            raw_digest: "aaa aaa aaa aaa aaa aaa aaa aaa aaa aaa ".repeat(40),
            created_at: "2026-04-15T00:00:00Z".into(),
        };
        let decision = classify_pending_review(&input);
        assert_ne!(decision.action, ReviewAction::PromoteToLongTerm);
    }

    #[test]
    fn classify_promotes_medium_high_signal_digest() {
        let input = PendingReviewInput {
            pending_id: "p2".into(),
            session_id: "s2".into(),
            creator_id: "ctr_test".into(),
            world_id: None,
            task_kind: "brainstorm".into(),
            raw_digest: "The chapter pivots from betrayal to alliance, with causal consequences for three factions."
                .into(),
            created_at: "2026-04-15T00:00:00Z".into(),
        };
        let decision = classify_pending_review(&input);
        assert_eq!(decision.action, ReviewAction::PromoteToLongTerm);
    }

    /// R-V133P4-06: Size guard truncates oversized raw_digest before summarization.
    #[tokio::test]
    async fn promote_truncates_oversized_raw_digest() {
        let home = PathBuf::from("/tmp/test_promotion_size_guard");
        let _ = std::fs::remove_dir_all(&home);

        struct Passthrough;
        #[allow(async_fn_in_trait)]
        impl SessionDigestSummarizer for Passthrough {
            async fn summarize(
                &self,
                _: &str,
                _: &str,
                raw_digest: &str,
                _: Option<&str>,
            ) -> Result<String, MemoryError> {
                // Return raw_digest as body — simulates PassthroughSummarizer
                Ok(raw_digest.to_string())
            }
        }

        // Create a digest larger than 256 KiB
        let big_digest = "x".repeat(300 * 1024);
        let input = PendingReviewInput {
            pending_id: "p_trunc".into(),
            session_id: "s_trunc".into(),
            creator_id: "ctr_test".into(),
            world_id: None,
            task_kind: "brainstorm".into(),
            raw_digest: big_digest,
            created_at: "2026-04-15T00:00:00Z".into(),
        };

        let memory = promote_to_long_term(&home, "ctr_test", &input, &Passthrough)
            .await
            .expect("promotion should succeed");
        // The body should be truncated to 256 KiB, not the full 300 KiB input
        assert!(
            memory.body.len() <= 256 * 1024,
            "body should be truncated to max_digest_bytes, got {} bytes",
            memory.body.len()
        );

        let _ = std::fs::remove_dir_all(&home);
    }
}
