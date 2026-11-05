//! Bearer-parameterized memory pipeline (v1.190 P2-T2; E11 rerouted in NXM-8).
//!
//! Moved from the daemon `api::handlers::memory_pipeline` module: the shared
//! review/promotion orchestration dispatching to the Creator vs Character
//! bearer through the closed [`nexus_creator_memory::MemoryBearerRef`] (spec
//! §3). The review drain classifies with the unchanged deterministic classifier
//! and consumes each pending candidate through the provider
//! (`memory.candidates.review`), so it writes provider candidates/records only
//! and the legacy `*_pending_review` tables stay write-idle (compass D6). The
//! one coherence write that survives is the retained SOUL-narrative cache
//! invalidation on fragment promotion.
//!
//! The pipeline context is a **core-internal authorization capability**: its
//! fields are private, the only constructors are [`MemoryPipelineCtx::creator`]
//! (the trusted principal's own Creator arm), [`MemoryPipelineCtx::character_read`]
//! (retained reads; owned Character at any status) and
//! [`MemoryPipelineCtx::character_write`] (wraps an already-admitted core
//! [`ActorActivityLease`]). It is never `pub`, so a borrowed pipeline context
//! cannot cross napi — hosts call the typed [`crate::CoreService`] family
//! methods instead, and the context lives and dies inside one Rust call.
//! The writable Character arm owns the per-Character activity lease for the
//! context's whole lifetime, so every nested DB/file/provider effect stays
//! inside the fence (durable §11.3.1).

use crate::actor_fence::ActorActivityLease;
use crate::error::{CoreError, CoreResult};
use crate::memory::MemoryAdapter;
use crate::memory_provider::MemoryDisposition;
use nexus_creator_memory::bearer::MemoryBearerRef;
use nexus_creator_memory::errors::MemoryError;
use nexus_creator_memory::review::{ReviewAction, SessionDigestSummarizer};
use nexus_local_db::LocalDbError;
use serde_json::{json, Value};
use sqlx::SqlitePool;

/// Internal wire-code carrier: `"{CODE}: {message}"` core categories the
/// daemon adapter re-renders as the retained `Internal` envelopes.
pub fn internal_err(code: &str, message: impl std::fmt::Display) -> CoreError {
    CoreError::Internal {
        category: format!("{code}: {message}"),
    }
}

/// Map a local-db error to the retained bearer-memory envelope: a foreign or
/// missing actor row stays the `character_memory` 403; everything else is the
/// `DATABASE_ERROR` internal classification.
pub fn map_local_db_error(e: LocalDbError) -> CoreError {
    match e {
        LocalDbError::ActorNotFound { .. } => CoreError::ForbiddenReason {
            resource: "character_memory".to_string(),
            reason: e.to_string(),
        },
        other => internal_err("database_error", other),
    }
}

/// Map a raw driver error (the daemon mapped `NexusApiError::from(sqlx)`).
pub fn sqlx_internal(e: impl std::fmt::Display) -> CoreError {
    internal_err("database_error", e)
}

/// Whether a pipeline context may mutate. A read-only context (built by
/// [`MemoryPipelineCtx::character_read`]) cannot enter any mutation helper:
/// review processing, fragment/queue advance, forced reflection persistence,
/// or any file/DB write fail closed with an error. A writable context
/// (Creator arm or [`MemoryPipelineCtx::character_write`]) carries the
/// optional per-Character activity lease so it is held through all effects.
#[derive(Debug)]
enum BearerCapability {
    ReadOnly,
    /// The optional lease is held for the context's whole lifetime so every
    /// nested DB/file/provider effect stays inside the activity fence; it is
    /// dropped with the context. Never read explicitly.
    #[allow(dead_code)]
    Writable(Option<ActorActivityLease>),
}

/// A bearer plus its scope provenance for one pipeline run.
///
/// This is an **authorization capability**, not a passive data bag: the
/// fields are private and the only ways to build one are [`Self::creator`]
/// (the trusted operator's own Creator arm, already authorized by the
/// principal verification, writable), [`Self::character_read`] (formats,
/// owner-checks, any status; read-only — retained reads) and
/// [`Self::character_write`] (wraps an already-admitted core activity lease;
/// writable). Because the fields are private, a caller cannot fabricate a
/// Character context without passing the ownership check, so every Character
/// pipeline entrypoint is sealed behind owner validation.
///
/// `scope_id` is the Creator arm's world id or the Character arm's binding
/// id; `None` = whole Creator / shared Character. The strings are **owned**
/// so the context is never a borrow of host state (napi boundary rule).
#[derive(Debug)]
pub struct MemoryPipelineCtx {
    owner_creator_id: String,
    character_id: Option<String>,
    scope_id: Option<String>,
    capability: BearerCapability,
}

impl MemoryPipelineCtx {
    /// Build a Creator-arm context (trusted operator; the principal's own
    /// creator id). Creator behavior is unchanged: writable, no per-Character
    /// lease needed.
    pub(crate) fn creator(creator_id: &str, scope_id: Option<&str>) -> Self {
        Self {
            owner_creator_id: creator_id.to_string(),
            character_id: None,
            scope_id: scope_id.map(str::to_string),
            capability: BearerCapability::Writable(None),
        }
    }

    /// Build a read-only Character-arm context for retained-data reads.
    ///
    /// Validates the bearer format and current ownership (foreign or missing
    /// Character is `404 not_found`, indistinguishable) but imposes no
    /// lifecycle requirement: an archived Character's retained memory rows
    /// stay readable (§11.2). A read-only context cannot synthesize, write a
    /// file, bootstrap a cache, or advance a queue — any mutation helper
    pub(crate) async fn character_read(
        pool: &SqlitePool,
        owner_creator_id: &str,
        character_id: &str,
        scope_id: Option<&str>,
    ) -> CoreResult<Self> {
        Self::validated_bearer(owner_creator_id, character_id)?;
        let owned = nexus_local_db::get_character(pool, owner_creator_id, character_id)
            .await
            .map_err(map_local_db_error)?;
        match owned {
            None => Err(CoreError::NotFound {
                resource: format!("character {character_id}"),
            }),
            Some(_) => Ok(Self {
                owner_creator_id: owner_creator_id.to_string(),
                character_id: Some(character_id.to_string()),
                scope_id: scope_id.map(str::to_string),
                capability: BearerCapability::ReadOnly,
            }),
        }
    }

    /// Build a writable Character-arm context from an already-admitted
    /// activity lease. Verifies the lease identity matches the supplied ids;
    /// the lease is transferred into the context and held for its whole
    /// lifetime (all DB/file/provider effects).
    pub(crate) fn character_write(
        activity: ActorActivityLease,
        owner_creator_id: &str,
        character_id: &str,
        scope_id: Option<&str>,
    ) -> CoreResult<Self> {
        if activity.owner_creator_id() != owner_creator_id
            || activity.character_id() != character_id
        {
            return Err(internal_err(
                "pipeline_guard_mismatch",
                "activity guard identity does not match the requested Character",
            ));
        }
        Self::validated_bearer(owner_creator_id, character_id)?;
        Ok(Self {
            owner_creator_id: owner_creator_id.to_string(),
            character_id: Some(character_id.to_string()),
            scope_id: scope_id.map(str::to_string),
            capability: BearerCapability::Writable(Some(activity)),
        })
    }

    /// Closed bearer format validation (path-safe ids) before any capability
    /// is minted.
    fn validated_bearer(owner_creator_id: &str, character_id: &str) -> CoreResult<()> {
        MemoryBearerRef::Character {
            owner_creator_id,
            character_id,
        }
        .validate()
        .map_err(|e| CoreError::InvalidInput {
            field: "character_id".to_string(),
            reason: e.to_string(),
        })
    }

    /// Borrow the bearer for a read path. Readable contexts and writable
    /// contexts both allow reads.
    #[allow(clippy::option_if_let_else)] // the match names both wire bearers directly
    pub(crate) fn bearer_ref(&self) -> MemoryBearerRef<'_> {
        match &self.character_id {
            None => MemoryBearerRef::Creator(&self.owner_creator_id),
            Some(character_id) => MemoryBearerRef::Character {
                owner_creator_id: &self.owner_creator_id,
                character_id,
            },
        }
    }

    /// Borrow the scope (Creator world id / Character binding id).
    pub(crate) fn scope(&self) -> Option<&str> {
        self.scope_id.as_deref()
    }

    /// Borrow the bearer only when writable; mutation helpers call this first
    /// so a read-only context cannot write. Returns `forbidden` otherwise.
    pub(crate) fn bearer_for_write(&self) -> Result<MemoryBearerRef<'_>, CoreError> {
        match &self.capability {
            BearerCapability::Writable(_) => Ok(self.bearer_ref()),
            BearerCapability::ReadOnly => Err(CoreError::ForbiddenReason {
                resource: "character_memory".to_string(),
                reason: "read-only pipeline context cannot mutate memory".to_string(),
            }),
        }
    }
}

// ── Review pipeline ────────────────────────────────────────────────────────

/// Maximum pending rows inspected per review call (V1.80 REL-01).
pub const REVIEW_BATCH_LIMIT: i64 = 50;

/// Maximum allowed digest size in bytes (256 KiB). R-V133P4-06.
pub const MAX_DIGEST_BYTES: usize = 256 * 1024;

/// Outcome of a bounded review batch (V1.80 REL-01).
#[derive(Debug)]
pub struct ReviewBatchOutcome {
    pub promoted: i64,
    pub fragmented: i64,
    pub dropped: i64,
    pub processed: usize,
    pub has_more: bool,
    pub any_row_remained_pending: bool,
    pub more_in_db: bool,
    pub processing_slice: usize,
}

impl ReviewBatchOutcome {
    pub(crate) const fn new() -> Self {
        Self {
            promoted: 0,
            fragmented: 0,
            dropped: 0,
            processed: 0,
            has_more: false,
            any_row_remained_pending: false,
            more_in_db: false,
            processing_slice: 0,
        }
    }
}

/// Counts produced by a single row's classify+action. Each field is 0 or 1.
struct RowActionCounts {
    promoted: i64,
    fragmented: i64,
    dropped: i64,
}

/// One pending candidate the review drain may classify.
///
/// The provider candidate is the review input: its classifier fields come from
/// the bridge's `nexus_legacy` block (E11's classifier is unchanged), and its
/// immutable capture scope decides whether a promotion is binding-local.
pub struct ReviewBatchRow {
    /// The provider candidate the decision consumes.
    pub candidate_id: String,
    /// The revision the candidate read served, used as the CAS precondition.
    pub expected_revision: u64,
    /// The retained classifier input.
    pub input: nexus_creator_memory::review::PendingReviewInput,
    /// A Character candidate captured under `context(binding_id)`: a promotion
    /// must stay binding-local (only the explicit fragment promotion shares it).
    pub binding_local: bool,
}

/// Process a bounded slice of a bearer's review queue.
///
/// Bearer-agnostic: classifies each pending candidate and consumes it through
/// the provider (`memory.candidates.review`), so a drain writes provider
/// candidates/records only — the legacy `*_pending_review` tables stay
/// write-idle (compass D6). The deadline semantics (stop on expiry, partial
/// progress, `any_row_remained_pending`) are unchanged.
///
/// `section` is the bearer's memory-disposition section when the binding is
/// local, already held by the caller across the whole batch: every consumption
/// here runs under it. A remote binding has none (its candidate CAS supplies
/// the exclusion), so the batch runs without one.
pub async fn process_bearer_review_batch(
    rows: &[ReviewBatchRow],
    bearer: MemoryBearerRef<'_>,
    memory: &MemoryAdapter,
    section: Option<&MemoryDisposition>,
    deadline: tokio::time::Instant,
) -> CoreResult<ReviewBatchOutcome> {
    let mut outcome = ReviewBatchOutcome::new();

    for row in rows {
        if tokio::time::Instant::now() >= deadline {
            tracing::info!(
                bearer_id = %bearer.id(),
                pending_id = %row.input.pending_id,
                processed = outcome.processed,
                "Review deadline reached mid-batch; returning partial progress"
            );
            break;
        }

        let decision = nexus_creator_memory::review::classify_pending_review(&row.input);
        // A row that has started runs to completion: the provider journals each
        // decision, and cancelling one mid-transaction can leave its target
        // unsettled, which would refuse the bearer's next read. The call budget
        // is therefore checked between rows, never inside one.
        let action_counts =
            consume_review_decision(&decision.action, row, bearer, memory, section).await;

        outcome.processed += 1;
        outcome.promoted += action_counts.promoted;
        outcome.fragmented += action_counts.fragmented;
        outcome.dropped += action_counts.dropped;
        if action_counts.promoted + action_counts.fragmented + action_counts.dropped == 0 {
            outcome.any_row_remained_pending = true;
        }
    }

    Ok(outcome)
}

/// Classify one pending candidate and consume it through the provider.
///
/// A provider refusal leaves the candidate pending and the row counts stay
/// zero, so the caller reports `has_more` instead of a false completion.
async fn consume_review_decision(
    action: &ReviewAction,
    row: &ReviewBatchRow,
    bearer: MemoryBearerRef<'_>,
    memory: &MemoryAdapter,
    section: Option<&MemoryDisposition>,
) -> RowActionCounts {
    let mut counts = RowActionCounts {
        promoted: 0,
        fragmented: 0,
        dropped: 0,
    };

    let decision = match action {
        // Binding-local Character pending must stay binding-local: a
        // Promote decision on a World-life (binding) scope is coerced to
        // the fragment path so it retains binding provenance (blocking
        // binding removal via `binding_has_local_memory`) and only becomes
        // Character-shared after the explicit revision-checked fragment
        // promotion. Shared Character scope and the whole Creator arm promote.
        ReviewAction::PromoteToLongTerm if row.binding_local => fragment_decision(&row.input),
        ReviewAction::PromoteToLongTerm => match promote_decision(&row.input, bearer).await {
            Ok(decision) => decision,
            Err(error) => {
                tracing::warn!(
                    pending_id = %row.input.pending_id,
                    error = %error,
                    "Failed to build the promote decision; row stays pending"
                );
                return counts;
            }
        },
        ReviewAction::FragmentOnly => fragment_decision(&row.input),
        ReviewAction::Drop => json!({ "action": "drop" }),
        // MergeIntoExisting and TriggerSoulExperienceOnly are later features.
        _ => {
            tracing::debug!(
                pending_id = %row.input.pending_id,
                action = ?action,
                "Skipping unimplemented review action"
            );
            return counts;
        }
    };

    // D18: a binding-local candidate's dependency namespace is its binding; a
    // shared candidate (Creator arm or shared Character scope) carries none.
    let binding_id = if row.binding_local {
        row.input.scope_id.as_deref()
    } else {
        None
    };
    match memory
        .review(
            &row.candidate_id,
            row.expected_revision,
            decision,
            section,
            binding_id,
        )
        .await
    {
        Ok(()) => match action {
            ReviewAction::Drop => counts.dropped = 1,
            ReviewAction::PromoteToLongTerm if !row.binding_local => counts.promoted = 1,
            _ => counts.fragmented = 1,
        },
        Err(error) => {
            tracing::warn!(
                pending_id = %row.input.pending_id,
                error = %error,
                "Failed to persist the review decision; row stays pending"
            );
        }
    }

    counts
}

/// The §7.15 fragment decision: one `fact` record carrying the retained
/// summary, keywords, TTL, session and pending provenance. The record inherits
/// the candidate's immutable capture scope.
fn fragment_decision(input: &nexus_creator_memory::review::PendingReviewInput) -> Value {
    let fragment = nexus_creator_memory::review::create_fragment_from_review(input);
    json!({
        "action": "fragment",
        "fragments": [{
            "kind": "fact",
            "content": {
                "text": fragment.summary,
                "structured": { "nexus_legacy": {
                    "pending_id": input.pending_id,
                    "session_id": fragment.session_id,
                    "keywords": fragment.keywords,
                    "ttl": fragment.ttl,
                    "created_at": fragment.created_at,
                }},
            },
        }],
    })
}

/// The §7.15 promote decision for shared Character and whole-Creator scope: the
/// retained long-term body (passthrough summarizer, UNTRUSTED header included)
/// under the task-kind-derived `nexus.<MemoryKind>` record kind.
async fn promote_decision(
    input: &nexus_creator_memory::review::PendingReviewInput,
    bearer: MemoryBearerRef<'_>,
) -> CoreResult<Value> {
    let summarizer = PassthroughSummarizer::new(bearer);
    let body = summarizer
        .summarize(
            &input.session_id,
            &input.task_kind,
            &input.raw_digest,
            input.scope_id.as_deref(),
        )
        .await
        .map_err(|error| internal_err("promote_to_long_term_failed", error))?;
    // The authored identity the CLI addresses a long-term memory by: the
    // promotion derives it from the pending row, because the retained file
    // promotion's slug came from its minted `memory_id` and a promoted record
    // with no slug cannot be listed, shown, edited or deleted
    // (fix round 1, Greptile `memory_pipeline.rs:441`).
    let memory_kind = nexus_creator_memory::review::task_kind_to_memory_kind(&input.task_kind);
    let memory_id = format!("mem_{}", input.pending_id);
    // The prefix above is local, so a slug-less id is a broken invariant rather
    // than a pending row the pipeline can serve (fix round 3, F3).
    let slug = nexus_creator_memory::long_term_memory::slug_from_memory_id(&memory_id);
    let slug = slug.ok_or_else(|| {
        internal_err(
            "promote_to_long_term_failed",
            format!("memory id '{memory_id}' does not carry the mem_ prefix"),
        )
    })?;
    Ok(json!({
        "action": "promote",
        "kind": crate::memory_provider::policy::namespaced_record_kind(memory_kind),
        "content_override": {
            "text": body,
            "structured": { "nexus_legacy": {
                "slug": slug,
                "memory_id": memory_id,
                "memory_kind": memory_kind,
                "source_session_ids": [input.session_id],
            }},
        },
    }))
}

/// Passthrough summarizer that returns the raw digest with a provenance
/// header (V1.33 R-V133P4-03/06 behavior preserved for the Creator arm).
pub struct PassthroughSummarizer {
    /// Header key (`creator_id` or `character_id`) for the bearer.
    id_key: &'static str,
    /// Header value.
    id_value: String,
    /// Header key (`world_id` or `binding_id`) for the scope.
    scope_key: &'static str,
    /// Header scope value.
    scope_value: String,
}

impl PassthroughSummarizer {
    pub(crate) fn new(bearer: MemoryBearerRef<'_>) -> Self {
        match bearer {
            MemoryBearerRef::Creator(id) => Self {
                id_key: "creator_id",
                id_value: id.to_string(),
                scope_key: "world_id",
                scope_value: "(none)".to_string(),
            },
            MemoryBearerRef::Character { character_id, .. } => Self {
                id_key: "character_id",
                id_value: character_id.to_string(),
                scope_key: "binding_id",
                scope_value: "(none)".to_string(),
            },
        }
    }
}

// `summarize` performs no async I/O (passthrough) — trait contract requires
// `async`; clippy 1.98 `unused_async_trait_impl` is toolchain-drift debt.
#[allow(clippy::unused_async_trait_impl)]
impl SessionDigestSummarizer for PassthroughSummarizer {
    async fn summarize(
        &self,
        session_id: &str,
        task_kind: &str,
        raw_digest: &str,
        scope_id: Option<&str>,
    ) -> Result<String, MemoryError> {
        let digest = if raw_digest.len() > MAX_DIGEST_BYTES {
            tracing::warn!(
                original_len = raw_digest.len(),
                max_bytes = MAX_DIGEST_BYTES,
                "PassthroughSummarizer: raw_digest exceeds 256 KiB cap, truncating"
            );
            &raw_digest[..MAX_DIGEST_BYTES]
        } else {
            raw_digest
        };
        let captured_at = chrono::Utc::now().to_rfc3339();
        let header = format!(
            "# UNTRUSTED: sourced from session_capture digest\n# {}: {}\n# session_id: {session_id}\n# task_kind: {task_kind}\n# {}: {}\n# captured_at: {captured_at}\n\n",
            self.id_key,
            self.id_value,
            self.scope_key,
            scope_id.unwrap_or(&self.scope_value)
        );
        Ok(format!("{header}{digest}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_creator_memory::bearer::MemoryBearerRef;

    #[tokio::test]
    async fn passthrough_summarizer_includes_untrusted_header() {
        let summarizer = PassthroughSummarizer::new(MemoryBearerRef::Creator("ctr_test_creator"));
        let result = summarizer
            .summarize(
                "sess_123",
                "brainstorm",
                "My brainstorm content",
                Some("world_1"),
            )
            .await
            .unwrap();

        assert!(
            result.starts_with("# UNTRUSTED:"),
            "LTM body should start with UNTRUSTED header, got: {}",
            &result[..result.len().min(50)]
        );
        assert!(
            result.contains("# creator_id: ctr_test_creator"),
            "Header should include creator_id (active creator)"
        );
        assert!(
            result.contains("# session_id: sess_123"),
            "Header should include session_id"
        );
        assert!(
            result.contains("# task_kind: brainstorm"),
            "Header should include task_kind"
        );
        assert!(
            result.contains("# world_id: world_1"),
            "Header should include world_id"
        );
        assert!(
            result.contains("# captured_at: "),
            "Header should include captured_at (RFC 3339)"
        );
        assert!(
            result.contains("My brainstorm content"),
            "Body should contain the raw digest after the header"
        );
    }

    #[tokio::test]
    async fn passthrough_summarizer_character_header_labelled() {
        let summarizer = PassthroughSummarizer::new(MemoryBearerRef::Character {
            owner_creator_id: "ctr_ownerx",
            character_id: "chr_0123456789abcdef0123456789abcdef",
        });
        let result = summarizer
            .summarize("sess_1", "brainstorm", "Body", Some("bnd_x"))
            .await
            .unwrap();
        assert!(result.contains("# character_id: chr_0123456789abcdef0123456789abcdef"));
        assert!(result.contains("# binding_id: bnd_x"), "got: {result}");
        assert!(!result.contains("# creator_id:"), "no creator_id key");
    }

    #[tokio::test]
    async fn passthrough_summarizer_truncates_large_digest() {
        let summarizer = PassthroughSummarizer::new(MemoryBearerRef::Creator("ctr_big"));
        let large_digest = "x".repeat(MAX_DIGEST_BYTES + 1000);
        let result = summarizer
            .summarize("sess_big", "test", &large_digest, None)
            .await
            .unwrap();

        let body_after_header = result.split_once("\n\n").map_or("", |(_, body)| body);
        assert_eq!(
            body_after_header.len(),
            MAX_DIGEST_BYTES,
            "Digest should be truncated to MAX_DIGEST_BYTES"
        );
    }

    #[tokio::test]
    async fn passthrough_summarizer_small_digest_unchanged() {
        let summarizer = PassthroughSummarizer::new(MemoryBearerRef::Creator("ctr_small"));
        let small = "Hello world";
        let result = summarizer
            .summarize("sess_small", "test", small, None)
            .await
            .unwrap();

        assert!(
            result.contains(small),
            "Small digest should be included verbatim"
        );
    }
}
