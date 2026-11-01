//! Nexus-owned deterministic provider policy (spoke-memory §4.1, §7.1).
//!
//! The control half declares the surface the provider serves and the bounds it
//! enforces; the content half holds the deterministic algorithm policy the
//! content families need: the D-7 kind mapping, the §6.7 advisory classifier,
//! and the §9 recall ranking and budget.
//!
//! Nothing here calls out to a model or the network: the quota/retention values
//! are operator policy, `engine_policy_version` is the opaque stamp clients pin
//! recall/reflection bases to, and the §8.2 freeze allowlist is the protocol's
//! own predicate (re-used from `spoke_operations`, never re-implemented here).
//!
//! Staleness (MEM-R013) is defined only over a *pinned reflection basis*; N1
//! serves no reflection op and stores no reflection, so there is nothing here
//! to compute it over and no dead function stands in for it.

use std::num::NonZeroU64;

use serde_json::{json, Value};

/// §3: the memory protocol revision this provider speaks.
pub(super) const MEMORY_PROTOCOL_VERSION: u64 = 1;

/// §4.1: the family flag served. The sub-capabilities
/// (`spoke-memory-reflection`, `spoke-memory-transfer`) are not served, so they
/// are never advertised and their ops refuse `op_unsupported`.
pub(super) const CAPABILITIES: [&str; 1] = ["spoke-memory"];

/// §4.2: the authored content kinds this provider requires before generation.
pub(super) const REQUIRED_PROFILE_KINDS: [&str; 1] = ["personality"];

/// Opaque provider policy version (§6.1). Bumped whenever a policy function's
/// output would change for unchanged input.
pub(super) const ENGINE_POLICY_VERSION: &str = "nexus-local-memory-v1";

/// §7.1 `limits`. Every value is a positive integer bound the provider enforces
/// or guarantees; `dedup_retention_seconds` covers
/// `idempotency_retention_seconds` as MEM-R021 requires.
const MAX_CONTENT_BYTES: u64 = 65_536;
const MAX_CANDIDATES_PER_CAPTURE: u64 = 32;
const MAX_PAGE_SIZE: u64 = 100;
const EXPORT_CHUNK_BYTES: u64 = 262_144;
const IDEMPOTENCY_RETENTION_SECONDS: u64 = 2_592_000;
/// §6.10/MEM-R021: how long a receipt *body* is served; afterwards only its
/// content-free dedup tombstone answers (§7.28 `RECEIPT_EXPIRED`).
pub(super) const RECEIPT_RETENTION_SECONDS: u64 = 2_592_000;
/// §5.2/MEM-R014: how long an applied journal shell is retained. Its file
/// content is scrubbed at settlement (`targets_json` becomes `'[]'`), so the
/// window only bounds the diagnostic trail and the outbox history; an unsettled
/// intent is never reclaimed, it is the recovery authority.
pub(super) const JOURNAL_RETENTION_SECONDS: u64 = 2_592_000;
/// How long the tombstone (namespace, digest, receipt id, terminal status)
/// outlives the body; past it the key is new again.
pub(super) const DEDUP_RETENTION_SECONDS: u64 = 5_184_000;
const STAGED_IMPORT_TTL_SECONDS: u64 = 86_400;
const EXPORT_RETENTION_SECONDS: u64 = 86_400;

/// §7.1 `limits` payload.
pub(super) fn limits() -> Value {
    json!({
        "max_content_bytes": MAX_CONTENT_BYTES,
        "max_candidates_per_capture": MAX_CANDIDATES_PER_CAPTURE,
        "max_page_size": MAX_PAGE_SIZE,
        "export_chunk_bytes": EXPORT_CHUNK_BYTES,
        "idempotency_retention_seconds": IDEMPOTENCY_RETENTION_SECONDS,
        "receipt_retention_seconds": RECEIPT_RETENTION_SECONDS,
        "dedup_retention_seconds": DEDUP_RETENTION_SECONDS,
        "staged_import_ttl_seconds": STAGED_IMPORT_TTL_SECONDS,
        "export_retention_seconds": EXPORT_RETENTION_SECONDS,
    })
}

/// §7.1 `max_content_bytes` as the content validators take it.
pub(super) fn max_content_bytes() -> usize {
    usize::try_from(MAX_CONTENT_BYTES).unwrap_or(usize::MAX)
}

/// §7.0.7 page bound: the caller's `limit` defaults to and may never exceed the
/// advertised `max_page_size`.
pub(super) const MAX_PAGE_SIZE_BOUND: NonZeroU64 = match NonZeroU64::new(MAX_PAGE_SIZE) {
    Some(bound) => bound,
    None => panic!("max_page_size must be positive"),
};

// ── Kind mapping, classification, recall ranking + budget ───────────────

/// D-7: every Nexus `MemoryKind` keeps its own name inside a namespaced record
/// kind, so the nine-value authoring taxonomy survives the protocol's open
/// `RetainedRecord.kind` vocabulary instead of being collapsed into the core
/// `event`/`fact`/`note` buckets. The mapping is a lossless bijection at the
/// adapter boundary: `nexus.story_summary` ⇄ `story_summary`.
pub(super) const NEXUS_KIND_PREFIX: &str = "nexus.";

/// The nine `nexus_creator_memory::memory_item::MemoryKind` values.
pub const NEXUS_MEMORY_KINDS: [&str; 9] = [
    "story_summary",
    "research_material",
    "review_note",
    "character_note",
    "world_building",
    "plot_outline",
    "theme_analysis",
    "personality_core",
    "custom",
];

#[must_use]
pub fn namespaced_record_kind(memory_kind: &str) -> String {
    format!("{NEXUS_KIND_PREFIX}{memory_kind}")
}

/// The inverse mapping, for the adapter boundary that renders an authored kind.
///
/// `None` for a kind outside the Nexus taxonomy, which the protocol allows
/// (product kinds are open vocabulary).
#[must_use]
pub fn memory_kind_of(record_kind: &str) -> Option<&str> {
    record_kind
        .strip_prefix(NEXUS_KIND_PREFIX)
        .filter(|kind| NEXUS_MEMORY_KINDS.contains(kind))
}

/// §6.7 advisory classification: what the provider *suggests* for a capture.
/// The reviewer decision stays authoritative; this is deterministic and derived
/// from the captured text alone (no model, no network, no stored state).
pub(super) struct Classification {
    pub disposition: &'static str,
    pub rationale_codes: Vec<&'static str>,
}

/// Shorter than this (in words) a capture is treated as a fragment claim rather
/// than a promotable record.
const PROMOTABLE_MIN_WORDS: usize = 8;

pub(super) fn classify(text: &str, kind_hint: Option<&str>) -> Classification {
    let words = text.split_whitespace().count();
    if words == 0 {
        return Classification {
            disposition: "drop",
            rationale_codes: vec!["empty_content"],
        };
    }
    // An explicit hint means the caller already knows what the capture is;
    // classification then only confirms it.
    if kind_hint.is_some_and(|hint| !hint.is_empty()) {
        return Classification {
            disposition: "promote",
            rationale_codes: vec!["kind_hint"],
        };
    }
    if words < PROMOTABLE_MIN_WORDS {
        return Classification {
            disposition: "fragment",
            rationale_codes: vec!["short_capture"],
        };
    }
    Classification {
        disposition: "promote",
        rationale_codes: vec!["multi_word_capture"],
    }
}

/// §9.2 ranking hint terms: lowercase alphanumeric words, de-duplicated, order
/// preserved so the score is computable from a stored record alone.
pub(super) fn query_terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for term in query.split(|c: char| !c.is_alphanumeric()) {
        if term.is_empty() {
            continue;
        }
        let term = term.to_lowercase();
        if !terms.contains(&term) {
            terms.push(term);
        }
    }
    terms
}

/// §9.2: how many distinct query terms occur in the record text. Ranking is
/// provider policy; it only has to be stable for identical state + policy.
pub(super) fn recall_score(text: &str, terms: &[String]) -> usize {
    if terms.is_empty() {
        return 0;
    }
    let text = text.to_lowercase();
    terms
        .iter()
        .filter(|term| text.contains(term.as_str()))
        .count()
}

/// §9.3 budget outcome over one ranked page candidate list.
pub(super) struct BudgetOutcome {
    /// Indices into the ranked list that the page carries, in rank order.
    pub kept: Vec<usize>,
    /// §9.3 progress rule: the single oversized first item, surfaced whole.
    pub overflow_record_id: Option<String>,
}

/// §9.3: `max_items` is a hard cap and `max_bytes` a hard UTF-8 budget. When the
/// next ranked item would exceed the byte budget and the page is still empty,
/// that item is returned whole and named as the overflow; otherwise it is
/// deferred. Whether the page is truncated is the caller's comparison between
/// what it kept and the authorized set.
pub(super) fn apply_budget(
    sizes: &[(String, usize)],
    max_items: u64,
    max_bytes: Option<u64>,
) -> BudgetOutcome {
    let max_items = usize::try_from(max_items).unwrap_or(usize::MAX);
    let max_bytes = max_bytes.map(|budget| usize::try_from(budget).unwrap_or(usize::MAX));
    let mut kept = Vec::new();
    let mut used = 0usize;
    let mut overflow_record_id = None;
    for (index, (record_id, bytes)) in sizes.iter().enumerate() {
        if kept.len() == max_items {
            break;
        }
        if max_bytes.is_some_and(|max_bytes| used + bytes > max_bytes) {
            if kept.is_empty() {
                kept.push(index);
                overflow_record_id = Some(record_id.clone());
            }
            break;
        }
        kept.push(index);
        used += bytes;
    }
    BudgetOutcome {
        kept,
        overflow_record_id,
    }
}
