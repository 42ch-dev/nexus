//! Review algorithm for memory pipeline.
//!
//! Implements classification heuristics for pending review entries,
//! determining whether to drop, fragment, promote to long-term memory,
//! merge, or trigger SOUL experience aggregation.
//!
//! See creator-memory-soul-lifecycle-v1.md §7.2.

use std::str::FromStr;

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

/// Error type for TaskKind parsing (always succeeds, Unknown is fallback).
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
            _ => Ok(Self::Unknown), // Fallback to Unknown for invalid values
        }
    }
}

impl TaskKind {
    /// Convert to string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Brainstorm => "brainstorm",
            Self::Outline => "outline",
            Self::Chapter => "chapter",
            Self::Research => "research",
            Self::Unknown => "unknown",
        }
    }

    /// Parse task kind from string (convenience wrapper that never fails).
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
/// - **Drop**: Very short digest (< 50 chars) or task_kind is diagnostic/noise.
/// - **FragmentOnly**: Medium-length digest, no clear long-term value.
///   Task kinds: "research", "exploration" (informational only).
/// - **PromoteToLongTerm**: Substantial digest with clear experience value.
///   Task kinds: "chapter", "outline", "brainstorm" with rich content.
/// - **MergeIntoExisting**: Content overlaps with existing memory (Phase 2 feature).
/// - **TriggerSoulExperienceOnly**: Metadata-only update, no new content.
///
/// # Example
///
/// ```rust
/// use nexus_domain::review::{classify_pending_review, PendingReviewInput};
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
///     nexus_domain::review::ReviewAction::PromoteToLongTerm => (),
///     _ => panic!("expected PromoteToLongTerm"),
/// }
/// ```
pub fn classify_pending_review(record: &PendingReviewInput) -> ReviewDecision {
    let digest_len = record.raw_digest.len();
    let task_kind: TaskKind = record.task_kind.parse().unwrap_or(TaskKind::Unknown);

    // Threshold constants
    const DROP_THRESHOLD: usize = 50; // Very short = no meaningful content
    const CREATIVE_PROMOTE_THRESHOLD: usize = 100; // Creative tasks promote at this length
    const DEFAULT_PROMOTE_THRESHOLD: usize = 200; // Unknown tasks promote at this length

    // Rule 1: Creative tasks have lower thresholds
    if matches!(
        task_kind,
        TaskKind::Brainstorm | TaskKind::Outline | TaskKind::Chapter
    ) {
        // Creative tasks never get dropped based solely on length (experience value)
        if digest_len >= CREATIVE_PROMOTE_THRESHOLD {
            return ReviewDecision {
                pending_id: record.pending_id.clone(),
                action: ReviewAction::PromoteToLongTerm,
                reason: format!(
                    "{} task with substantial digest ({} chars) — long-term value",
                    task_kind.as_str(),
                    digest_len
                ),
            };
        }
        // Creative tasks with medium digest go to fragment
        return ReviewDecision {
            pending_id: record.pending_id.clone(),
            action: ReviewAction::FragmentOnly,
            reason: format!(
                "{} task with medium digest — fragment indexing",
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
/// Summary is truncated raw_digest or first N chars.
///
/// # Example
///
/// ```rust
/// use nexus_domain::review::{create_fragment_from_review, PendingReviewInput};
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

    // Filter stop words and dedupe
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
        // Dedupe
        if !keywords.contains(&word) {
            keywords.push(word);
        }
    }

    // Limit to top 10 keywords
    keywords.truncate(10);
    keywords
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
