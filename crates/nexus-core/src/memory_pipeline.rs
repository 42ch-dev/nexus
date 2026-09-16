//! Bearer-parameterized memory pipeline (v1.190 P2-T2).
//!
//! Moved from the daemon `api::handlers::memory_pipeline` module: the shared
//! review/promotion orchestration dispatching to the Creator vs Character
//! bearer storage through the closed [`nexus_creator_memory::MemoryBearerRef`]
//! (spec §3). The Creator arm reproduces the pre-migration bytes, paths, table
//! names, and cache semantics exactly; the Character arm routes to the
//! dedicated `character_*` repositories which enforce actor/owner provenance
//! before any persistence.
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
use nexus_creator_memory::bearer::MemoryBearerRef;
use nexus_creator_memory::errors::MemoryError;
use nexus_creator_memory::review::{ReviewAction, ReviewDecision, SessionDigestSummarizer};
use nexus_local_db::LocalDbError;
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

    /// The bearer id, for read-only diagnostics.
    pub(crate) fn bearer_id(&self) -> String {
        self.bearer_ref().id().to_string()
    }

    /// Borrow the bearer for a read path. Readable contexts and writable
    /// contexts both allow reads.
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

/// Process a bounded slice of a bearer's review queue.
///
/// Bearer-agnostic: classifies each pending row and dispatches the
/// persistence (fragment insert, pending delete) to the Creator or Character
/// storage. The deadline semantics (stop on expiry, partial progress,
/// `any_row_remained_pending`) are identical to the pre-migration Creator
/// logic.
pub async fn process_bearer_review_batch(
    inputs: &[nexus_creator_memory::review::PendingReviewInput],
    nexus_home: &std::path::Path,
    ctx: &MemoryPipelineCtx,
    pool: &SqlitePool,
    deadline: tokio::time::Instant,
) -> CoreResult<ReviewBatchOutcome> {
    // A read-only context cannot advance a queue, promote, or write files.
    ctx.bearer_for_write()?;
    let mut outcome = ReviewBatchOutcome::new();

    for input in inputs {
        if tokio::time::Instant::now() >= deadline {
            break;
        }

        let decision = nexus_creator_memory::review::classify_pending_review(input);

        let row_result = tokio::time::timeout_at(
            deadline,
            process_single_review_row(&decision, input, nexus_home, ctx, pool),
        )
        .await;

        outcome.processed += 1;

        match row_result {
            Ok(action_counts) => {
                outcome.promoted += action_counts.promoted;
                outcome.fragmented += action_counts.fragmented;
                outcome.dropped += action_counts.dropped;
                if action_counts.promoted + action_counts.fragmented + action_counts.dropped == 0 {
                    outcome.any_row_remained_pending = true;
                }
            }
            Err(_elapsed) => {
                outcome.any_row_remained_pending = true;
                tracing::info!(
                    bearer_id = %ctx.bearer_id(),
                    pending_id = %input.pending_id,
                    processed = outcome.processed,
                    "Review deadline reached mid-batch; returning partial progress"
                );
                break;
            }
        }
    }

    Ok(outcome)
}

/// Classify one pending row, perform the action (promote/fragment/drop), and
/// delete the pending row on success for the bearer's storage.
async fn process_single_review_row(
    decision: &ReviewDecision,
    input: &nexus_creator_memory::review::PendingReviewInput,
    nexus_home: &std::path::Path,
    ctx: &MemoryPipelineCtx,
    pool: &SqlitePool,
) -> RowActionCounts {
    let mut counts = RowActionCounts {
        promoted: 0,
        fragmented: 0,
        dropped: 0,
    };

    match decision.action {
        ReviewAction::PromoteToLongTerm => {
            // Binding-local Character pending must stay binding-local: a
            // Promote decision on a World-life (binding) scope is coerced to
            // the fragment path so it retains binding provenance (blocking
            // binding removal via `binding_has_local_memory`) and only becomes
            // Character-shared after the explicit revision-checked fragment
            // promotion. Shared Character scope and the whole Creator arm are
            // unchanged.
            let binding_local = matches!(ctx.bearer_ref(), MemoryBearerRef::Character { .. })
                && input.scope_id.is_some();
            if binding_local {
                let fragment = nexus_creator_memory::review::create_fragment_from_review(input);
                match insert_fragment_and_delete_pending(
                    pool,
                    ctx,
                    &fragment,
                    input.scope_id.as_deref(),
                    &input.pending_id,
                )
                .await
                {
                    Ok(()) => counts.fragmented = 1,
                    Err(e) => {
                        tracing::warn!(
                            pending_id = %input.pending_id,
                            error = %e,
                            "Failed to create binding-local fragment from Promote decision atomically; row stays pending"
                        );
                    }
                }
            } else {
                match claim_pending_and_promote(nexus_home, pool, ctx, input).await {
                    Ok(()) => counts.promoted = 1,
                    Err(e) => {
                        tracing::warn!(
                            pending_id = %input.pending_id,
                            error = %e,
                            "Failed to promote pending review; row stays pending"
                        );
                    }
                }
            }
        }
        ReviewAction::FragmentOnly => {
            let fragment = nexus_creator_memory::review::create_fragment_from_review(input);
            match insert_fragment_and_delete_pending(
                pool,
                ctx,
                &fragment,
                input.scope_id.as_deref(),
                &input.pending_id,
            )
            .await
            {
                Ok(()) => counts.fragmented = 1,
                Err(e) => {
                    tracing::warn!(
                        pending_id = %input.pending_id,
                        error = %e,
                        "Failed to create fragment and advance queue atomically; row stays pending"
                    );
                }
            }
        }
        ReviewAction::Drop => match delete_pending_row(pool, ctx, &input.pending_id).await {
            Ok(()) => counts.dropped = 1,
            Err(e) => {
                tracing::warn!(
                    pending_id = %input.pending_id,
                    error = %e,
                    "Failed to drop pending review; row stays pending"
                );
            }
        },
        // MergeIntoExisting and TriggerSoulExperienceOnly are later features.
        _ => {
            tracing::debug!(
                pending_id = %input.pending_id,
                action = ?decision.action,
                "Skipping unimplemented review action"
            );
        }
    }

    counts
}

/// Delete a consumed pending row from the bearer's table.
///
/// PR #240 finding 2: queue advancement is part of the reported result — a
/// failed delete returns an error so the caller leaves the success counter
/// untouched and the row visible as still pending.
///
/// A delete that affects zero rows is treated as success: the queue has
/// already advanced (another consumer removed the row first).
async fn delete_pending_row(
    pool: &SqlitePool,
    ctx: &MemoryPipelineCtx,
    pending_id: &str,
) -> CoreResult<()> {
    match ctx.bearer_ref() {
        MemoryBearerRef::Creator(_) => {
            let pid = pending_id.to_string();
            sqlx::query!(
                "DELETE FROM memory_pending_review WHERE pending_id = ?",
                pid
            )
            .execute(pool)
            .await
            .map_err(sqlx_internal)?;
            Ok(())
        }
        MemoryBearerRef::Character {
            owner_creator_id,
            character_id,
        } => nexus_local_db::delete_character_pending_review(
            pool,
            owner_creator_id,
            character_id,
            pending_id,
        )
        .await
        .map_err(map_local_db_error)
        .map(|_| ()),
    }
}

/// Claim exactly one pending row in a transaction, then promote to
/// long-term memory while the claim is uncommitted (PR #240 review round 3).
///
/// Recovery semantics (filesystem cannot join the SQLite transaction):
/// - Stale/ghost input (zero-row claim): the transaction rolls back and NO
///   file is written — fresh stale input is safe.
/// - Filesystem/promote failure: the transaction rolls back, restoring the
///   pending row for a later retry.
/// - Commit failure after a successful file write: the row stays pending and
///   the durable file (keyed by `session_id` via
///   `check_session_already_promoted`) makes the next attempt hit
///   `AlreadyPromoted`, which commits the claim without rewriting.
async fn claim_pending_and_promote(
    nexus_home: &std::path::Path,
    pool: &SqlitePool,
    ctx: &MemoryPipelineCtx,
    input: &nexus_creator_memory::review::PendingReviewInput,
) -> CoreResult<()> {
    let mut tx = pool.begin().await.map_err(sqlx_internal)?;
    let deleted = match ctx.bearer_ref() {
        MemoryBearerRef::Creator(_) => {
            nexus_local_db::delete_pending_review_in_tx(&mut tx, &input.pending_id)
                .await
                .map_err(map_local_db_error)?
        }
        MemoryBearerRef::Character {
            character_id,
            owner_creator_id,
            ..
        } => nexus_local_db::delete_character_pending_review_in_tx(
            &mut tx,
            owner_creator_id,
            character_id,
            &input.pending_id,
        )
        .await
        .map_err(map_local_db_error)?,
    };
    require_exactly_one_pending_delete(deleted, &input.pending_id)?;

    let summarizer = PassthroughSummarizer::new(ctx.bearer_ref());
    match nexus_creator_memory::review::promote_to_long_term(
        nexus_home,
        ctx.bearer_ref(),
        input,
        &summarizer,
    )
    .await
    {
        Ok(_) => tx.commit().await.map_err(sqlx_internal),
        Err(MemoryError::AlreadyPromoted { .. }) => {
            tracing::info!(
                pending_id = %input.pending_id,
                session_id = %input.session_id,
                "Session already promoted by an earlier attempt; committing claim without rewriting"
            );
            tx.commit().await.map_err(sqlx_internal)
        }
        Err(e) => {
            let _ = tx.rollback().await;
            Err(internal_err("promote_to_long_term_failed", e))
        }
    }
}

/// PR #240 review round 2: the fragment+delete transaction requires the
/// queue advance to consume exactly one pending row. A zero-row delete means
/// the row was already consumed by a concurrent/stale run; the fragment
/// insert must roll back with it so replays can never duplicate fragments.
fn require_exactly_one_pending_delete(deleted: bool, pending_id: &str) -> CoreResult<()> {
    if deleted {
        Ok(())
    } else {
        Err(internal_err(
            "pending_review_queue_advance_stale",
            format!(
                "pending review {pending_id} deleted zero rows (already consumed); rolling back fragment insert"
            ),
        ))
    }
}

/// Insert a review fragment and advance the queue in one transaction.
///
/// PR #240 finding 2: fragment creation and pending-row deletion commit
/// atomically, so a failed queue advance can no longer leave a duplicated
/// fragment behind while the row stays pending.
async fn insert_fragment_and_delete_pending(
    pool: &SqlitePool,
    ctx: &MemoryPipelineCtx,
    fragment: &nexus_creator_memory::review::MemoryFragment,
    scope_id: Option<&str>,
    pending_id: &str,
) -> CoreResult<()> {
    let mut tx = pool.begin().await.map_err(sqlx_internal)?;
    let result = async {
        match ctx.bearer_ref() {
            MemoryBearerRef::Creator(_) => {
                let record = nexus_local_db::memory_fragment::MemoryFragmentRecord {
                    fragment_id: fragment.fragment_id.clone(),
                    session_id: fragment.session_id.clone(),
                    creator_id: fragment.bearer_id.clone(),
                    keywords: serde_json::to_string(&fragment.keywords).unwrap_or_default(),
                    summary: fragment.summary.clone(),
                    created_at: fragment.created_at.clone(),
                    ttl: fragment.ttl.clone(),
                    world_id: scope_id.map(str::to_string),
                };
                nexus_local_db::memory_fragment::create_fragment_in_tx(&mut tx, &record)
                    .await
                    .map_err(map_local_db_error)?;
                let deleted = nexus_local_db::delete_pending_review_in_tx(&mut tx, pending_id)
                    .await
                    .map_err(map_local_db_error)?;
                require_exactly_one_pending_delete(deleted, pending_id)?;
            }
            MemoryBearerRef::Character {
                owner_creator_id,
                character_id,
            } => {
                let record = nexus_local_db::NewCharacterMemoryFragment {
                    fragment_id: fragment.fragment_id.clone(),
                    session_id: fragment.session_id.clone(),
                    character_id: (*character_id).to_string(),
                    actor_world_binding_id: scope_id.map(str::to_string),
                    keywords: serde_json::to_string(&fragment.keywords).unwrap_or_default(),
                    summary: fragment.summary.clone(),
                    created_at: fragment.created_at.clone(),
                    ttl: fragment.ttl.clone(),
                };
                nexus_local_db::create_character_fragment_in_tx(&mut tx, owner_creator_id, &record)
                    .await
                    .map_err(map_local_db_error)?;
                let deleted = nexus_local_db::delete_character_pending_review_in_tx(
                    &mut tx,
                    owner_creator_id,
                    character_id,
                    pending_id,
                )
                .await
                .map_err(map_local_db_error)?;
                require_exactly_one_pending_delete(deleted, pending_id)?;
            }
        }
        Ok::<(), CoreError>(())
    }
    .await;
    match result {
        Ok(()) => tx.commit().await.map_err(sqlx_internal),
        Err(err) => {
            let _ = tx.rollback().await;
            Err(err)
        }
    }
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
