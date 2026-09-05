//! v1.184 P3 Task 2 — bearer-parameterized memory pipeline orchestration.
//!
//! This module owns the **shared** review/promotion and SOUL-narrative
//! reflect orchestration, dispatching to the Creator vs Character bearer
//! storage through the closed [`MemoryBearerRef`] (spec §3). The Creator arm
//! reproduces the pre-refactor bytes, paths, table names, and cache
//! semantics exactly; the Character arm routes to the dedicated
//! `character_*` repositories which enforce actor/owner provenance before
//! any persistence.
//!
//! Daemon API handlers (`memory.rs`) are the only other consumers: they
//! resolve the active Creator and pass the Creator arm. Task 3's generated
//! Character handlers will construct the Character arm through
//! [`BearerPipelineCtx::character`] (which rejects foreign/invalid actors
//! before any DB read, file write, or synthesis).

use crate::api::errors::NexusApiError;
use crate::character_tom::{CharacterTomListQuery, CharacterTomService};
use nexus_creator_memory::bearer::MemoryBearerRef;
use nexus_moment_context_assembly::CharacterMindInput;
use nexus_creator_memory::errors::MemoryError;
use nexus_creator_memory::review::{
    PendingReviewInput, ReviewAction, ReviewDecision, SessionDigestSummarizer,
};
use nexus_creator_memory::soul_narrative::SoulNarrativeSynthesizer;
use sqlx::SqlitePool;
use std::path::Path;

/// A bearer plus its scope provenance for one pipeline run.
///
/// This is an **authorization capability**, not a passive data bag: the
/// fields are private and the only ways to build one are [`Self::creator`]
/// (the trusted operator's own Creator arm, already authorized by the handler
/// auth gate) and [`Self::character`] (which verifies format, ownership, and
/// the ACTIVE lifecycle before the context is returned). Because the fields
/// are private, a caller cannot fabricate a Character context without passing
/// the async authorization check, so every Character pipeline entrypoint is
/// sealed behind owner/active-character validation.
///
/// `scope_id` is the Creator arm's world id or the Character arm's binding
/// id; `None` = whole Creator / shared Character.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BearerPipelineCtx<'a> {
    bearer: MemoryBearerRef<'a>,
    scope_id: Option<&'a str>,
}

impl<'a> BearerPipelineCtx<'a> {
    /// Build a Creator-arm context (trusted operator; handler already
    /// authorized the active Creator from config).
    pub(crate) const fn creator(creator_id: &'a str, scope_id: Option<&'a str>) -> Self {
        Self {
            bearer: MemoryBearerRef::Creator(creator_id),
            scope_id,
        }
    }

    /// Build a Character-arm context, validating format, ownership, and the
    /// active lifecycle **before** any DB read, file write, or synthesis.
    ///
    /// Rejects foreign, non-existent, and inactive/archived Characters
    /// (fail-closed; never falls back to the Creator's data). Because the
    /// context fields are private, this is the only way to obtain a
    /// Character context inside the crate, so mutating entrypoints cannot be
    /// invoked without a validated, authorized context.
    ///
    /// Consumed by the Task 3 generated Character handlers and the dual-bearer
    /// semantic suite; the public Creator handlers use the Creator arm.
    #[allow(dead_code)]
    pub(crate) async fn character(
        pool: &SqlitePool,
        owner_creator_id: &'a str,
        character_id: &'a str,
        scope_id: Option<&'a str>,
    ) -> Result<Self, NexusApiError> {
        let bearer = MemoryBearerRef::Character {
            owner_creator_id,
            character_id,
        };
        bearer.validate().map_err(|e| NexusApiError::InvalidInput {
            field: "character_id".into(),
            reason: e.to_string(),
        })?;
        let owned = nexus_local_db::get_character(pool, owner_creator_id, character_id)
            .await
            .map_err(map_local_db_error)?;
        match owned {
            None => Err(NexusApiError::Forbidden {
                resource: "character_memory".into(),
                reason: format!("character '{character_id}' is not owned by creator '{owner_creator_id}'"),
            }),
            Some(c) if c.status != "active" => Err(NexusApiError::Forbidden {
                resource: "character_memory".into(),
                reason: format!(
                    "character '{character_id}' is not active (status '{}'); only active Characters may enter the memory pipeline",
                    c.status
                ),
            }),
            Some(_) => Ok(Self { bearer, scope_id }),
        }
    }
}

/// Map a local-db error to the canonical daemon envelope.
fn map_local_db_error(e: nexus_local_db::LocalDbError) -> NexusApiError {
    match e {
        nexus_local_db::LocalDbError::ActorNotFound { .. } => NexusApiError::Forbidden {
            resource: "character_memory".into(),
            reason: e.to_string(),
        },
        other => NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: other.to_string(),
        },
    }
}

// ── Review pipeline ────────────────────────────────────────────────────────

/// Maximum pending rows inspected per review call (V1.80 REL-01).
pub(crate) const REVIEW_BATCH_LIMIT: i64 = 50;

/// Outcome of a bounded review batch (V1.80 REL-01; moved from `memory.rs`).
pub(crate) struct ReviewBatchOutcome {
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
/// `any_row_remained_pending`) are identical to the pre-refactor Creator
/// logic.
pub(crate) async fn process_bearer_review_batch(
    inputs: &[PendingReviewInput],
    nexus_home: &Path,
    ctx: &BearerPipelineCtx<'_>,
    pool: &SqlitePool,
    deadline: tokio::time::Instant,
) -> ReviewBatchOutcome {
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
                    bearer_id = %ctx.bearer.id(),
                    pending_id = %input.pending_id,
                    processed = outcome.processed,
                    "Review deadline reached mid-batch; returning partial progress"
                );
                break;
            }
        }
    }

    outcome
}

/// Classify one pending row, perform the action (promote/fragment/drop), and
/// delete the pending row on success for the bearer's storage.
async fn process_single_review_row(
    decision: &ReviewDecision,
    input: &PendingReviewInput,
    nexus_home: &Path,
    ctx: &BearerPipelineCtx<'_>,
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
            let binding_local = matches!(ctx.bearer, MemoryBearerRef::Character { .. })
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
        ReviewAction::Drop => {
            match delete_pending_row(pool, ctx, &input.pending_id).await {
                Ok(()) => counts.dropped = 1,
                Err(e) => {
                    tracing::warn!(
                        pending_id = %input.pending_id,
                        error = %e,
                        "Failed to drop pending review; row stays pending"
                    );
                }
            }
        }
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
    ctx: &BearerPipelineCtx<'_>,
    pending_id: &str,
) -> Result<(), NexusApiError> {
    match ctx.bearer {
        MemoryBearerRef::Creator(_) => {
            let pid = pending_id.to_string();
            sqlx::query!(
                "DELETE FROM memory_pending_review WHERE pending_id = ?",
                pid
            )
            .execute(pool)
            .await
            .map_err(NexusApiError::from)?;
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
    nexus_home: &Path,
    pool: &SqlitePool,
    ctx: &BearerPipelineCtx<'_>,
    input: &PendingReviewInput,
) -> Result<(), NexusApiError> {
    let mut tx = pool.begin().await.map_err(NexusApiError::from)?;
    let deleted = match ctx.bearer {
        MemoryBearerRef::Creator(_) => {
            nexus_local_db::delete_pending_review_in_tx(&mut tx, &input.pending_id)
                .await
                .map_err(map_local_db_error)?
        }
        MemoryBearerRef::Character {
            character_id, ..
        } => {
            nexus_local_db::delete_character_pending_review_in_tx(
                &mut tx,
                character_id,
                &input.pending_id,
            )
            .await
            .map_err(map_local_db_error)?
        }
    };
    require_exactly_one_pending_delete(deleted, &input.pending_id)?;

    let summarizer = PassthroughSummarizer::new(ctx.bearer);
    match nexus_creator_memory::review::promote_to_long_term(
        nexus_home,
        ctx.bearer,
        input,
        &summarizer,
    )
    .await
    {
        Ok(_) => tx.commit().await.map_err(NexusApiError::from),
        Err(MemoryError::AlreadyPromoted { .. }) => {
            tracing::info!(
                pending_id = %input.pending_id,
                session_id = %input.session_id,
                "Session already promoted by an earlier attempt; committing claim without rewriting"
            );
            tx.commit().await.map_err(NexusApiError::from)
        }
        Err(e) => {
            let _ = tx.rollback().await;
            Err(NexusApiError::Internal {
                code: "PROMOTE_TO_LONG_TERM_FAILED".into(),
                message: e.to_string(),
            })
        }
    }
}

/// PR #240 review round 2: the fragment+delete transaction requires the
/// queue advance to consume exactly one pending row. A zero-row delete means
/// the row was already consumed by a concurrent/stale run; the fragment
/// insert must roll back with it so replays can never duplicate fragments.
fn require_exactly_one_pending_delete(
    deleted: bool,
    pending_id: &str,
) -> Result<(), NexusApiError> {
    if deleted {
        Ok(())
    } else {
        Err(NexusApiError::Internal {
            code: "PENDING_REVIEW_QUEUE_ADVANCE_STALE".into(),
            message: format!(
                "pending review {pending_id} deleted zero rows (already consumed); rolling back fragment insert"
            ),
        })
    }
}

/// Insert a review fragment and advance the queue in one transaction.
///
/// PR #240 finding 2: fragment creation and pending-row deletion commit
/// atomically, so a failed queue advance can no longer leave a duplicated
/// fragment behind while the row stays pending.
async fn insert_fragment_and_delete_pending(
    pool: &SqlitePool,
    ctx: &BearerPipelineCtx<'_>,
    fragment: &nexus_creator_memory::review::MemoryFragment,
    scope_id: Option<&str>,
    pending_id: &str,
) -> Result<(), NexusApiError> {
    let mut tx = pool.begin().await.map_err(NexusApiError::from)?;
    let result = async {
        match ctx.bearer {
            MemoryBearerRef::Creator(_) => {
                let record = nexus_local_db::memory_fragment::MemoryFragmentRecord {
                    fragment_id: fragment.fragment_id.clone(),
                    session_id: fragment.session_id.clone(),
                    creator_id: fragment.bearer_id.clone(),
                    keywords: serde_json::to_string(&fragment.keywords).unwrap_or_default(),
                    summary: fragment.summary.clone(),
                    created_at: fragment.created_at.clone(),
                    ttl: fragment.ttl.clone(),
                    world_id: scope_id.map(std::string::ToString::to_string),
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
                    actor_world_binding_id: scope_id.map(std::string::ToString::to_string),
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
                    character_id,
                    pending_id,
                )
                .await
                .map_err(map_local_db_error)?;
                require_exactly_one_pending_delete(deleted, pending_id)?;
            }
        }
        Ok::<(), NexusApiError>(())
    }
    .await;
    match result {
        Ok(()) => tx.commit().await.map_err(NexusApiError::from),
        Err(err) => {
            let _ = tx.rollback().await;
            Err(err)
        }
    }
}

/// Maximum allowed digest size in bytes (256 KiB). R-V133P4-06.
pub(crate) const MAX_DIGEST_BYTES: usize = 256 * 1024;

/// Passthrough summarizer that returns the raw digest with a provenance
/// header (V1.33 R-V133P4-03/06 behavior preserved for the Creator arm).
pub(crate) struct PassthroughSummarizer {
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

// ── SOUL narrative reflect pipeline ───────────────────────────────────────

/// Maximum Unicode scalar chars persisted for a synthesized narrative.
pub(crate) const SOUL_NARRATIVE_MAX_CHARS: usize = 16 * 1024;

/// Insufficient-data gate thresholds (V1.81 G1).
pub(crate) const MIN_SOUL_NARRATIVE_FRAGMENTS: i64 = 10;
pub(crate) const MIN_SOUL_NARRATIVE_DISTINCT_KEYWORDS: i64 = 20;

/// Forward-looking tokens checked by the narrative quality suffix heuristic.
const FORWARD_LOOKING_TOKENS: &[&str] = &[
    "will", "shall", "next", "upcoming", "future", "continue", "toward", "await", "explore",
    "discover",
];

/// Forward-looking bigrams checked by the narrative quality suffix heuristic.
const FORWARD_LOOKING_BIGRAMS: &[(&str, &str)] = &[
    ("looking", "ahead"),
    ("going", "forward"),
    ("what", "if"),
    ("how", "might"),
];

/// Internal reflect state (mapped to the wire `SoulNarrativeRequest` state in
/// the handler).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReflectState {
    InsufficientData,
    Ungenerated,
    Current,
    Stale,
}

/// Outcome of a bearer-parameterized reflect run (pre-wire mapping).
#[derive(Debug, Clone)]
pub(crate) struct ReflectOutcome {
    pub state: ReflectState,
    pub narrative: Option<String>,
    pub generated_at: Option<String>,
    pub stale: bool,
    pub fragment_count_at_generation: Option<u64>,
    pub max_fragment_created_at_at_generation: Option<String>,
    pub current_fragment_count: u64,
    pub current_distinct_keyword_count: u64,
}

/// Normalized fragment signal used to build the synthesis input (arm-agnostic).
struct FragmentSignal {
    keywords: String,
    summary: String,
    created_at: String,
}

/// Run the bearer-parameterized SOUL narrative reflect pipeline.
///
/// Behavior matches the pre-refactor Creator `reflect_soul` core:
/// 1. compute fragment stats + cache row (one DB round-trip);
/// 2. insufficient-data gate **before** any ACP call;
/// 3. stale detection (stats-only rows are ungenerated, not stale);
/// 4. read/poll path (force=false) returns current/stale/ungenerated without
///    calling the synthesizer;
/// 5. force=true synthesizes (on-demand only), validates/caps, persists.
pub(crate) async fn reflect_bearer_soul<S: SoulNarrativeSynthesizer + ?Sized>(
    pool: &SqlitePool,
    ctx: &BearerPipelineCtx<'_>,
    force: bool,
    synthesizer: Option<&S>,
) -> Result<ReflectOutcome, NexusApiError> {
    // 1. fragment stats + cache row in one DB round-trip.
    let (fragment_stats, cached) = bearer_fragment_stats(pool, ctx).await?;

    // 2. insufficient-data gate (before any ACP call).
    let min_distinct =
        usize::try_from(MIN_SOUL_NARRATIVE_DISTINCT_KEYWORDS).unwrap_or(usize::MAX);
    let insufficient = fragment_stats.fragment_count < MIN_SOUL_NARRATIVE_FRAGMENTS
        || fragment_stats.distinct_keyword_count < min_distinct;

    if insufficient {
        return Ok(ReflectOutcome {
            state: ReflectState::InsufficientData,
            narrative: None,
            generated_at: None,
            stale: false,
            fragment_count_at_generation: None,
            max_fragment_created_at_at_generation: None,
            current_fragment_count: u64::try_from(fragment_stats.fragment_count).unwrap_or(0),
            current_distinct_keyword_count: u64::try_from(fragment_stats.distinct_keyword_count)
                .unwrap_or(0),
        });
    }

    // 3. stale detection (stats-only rows = ungenerated, not stale).
    let has_narrative = cached.as_ref().and_then(|c| c.narrative.as_ref()).is_some();
    let stale = cached.as_ref().is_some_and(|c| {
        has_narrative
            && (c.fragment_count_at_generation != fragment_stats.fragment_count
                || c.max_fragment_created_at_at_generation.as_deref()
                    != fragment_stats.max_created_at.as_deref())
    });

    // 4. read/poll path (force=false): never calls the LLM.
    if !force {
        if let Some(ref c) = cached {
            if !has_narrative {
                return Ok(outcome_ungenerated(&fragment_stats));
            }
            if stale {
                return Ok(outcome_stale(c, &fragment_stats));
            }
            return Ok(outcome_current(c, &fragment_stats));
        }
        return Ok(outcome_ungenerated(&fragment_stats));
    }

    // 5. force=true → synthesize (explicit CTA, on-demand only).
    let synth = synthesizer.ok_or_else(|| NexusApiError::ServiceUnavailable {
        message: "capability registry not available".to_string(),
    })?;

    let signals = bearer_recent_fragment_signals(pool, ctx).await?;
    let input = build_soul_narrative_synthesis_input(&signals, &fragment_stats);

    let top_keywords = input.top_keywords.clone();

    let draft = synth
        .synthesize(ctx.bearer, input, ctx.scope_id)
        .await
        .map_err(map_soul_narrative_memory_error)?;

    let narrative = truncate_summary(&draft.narrative, SOUL_NARRATIVE_MAX_CHARS);
    validate_soul_narrative_draft(&narrative, &top_keywords)
        .map_err(map_soul_narrative_memory_error)?;

    bearer_persist_narrative(pool, ctx, &narrative, &fragment_stats).await?;

    Ok(ReflectOutcome {
        state: ReflectState::Current,
        narrative: Some(narrative),
        generated_at: Some(chrono::Utc::now().to_rfc3339()),
        stale: false,
        fragment_count_at_generation: Some(
            u64::try_from(fragment_stats.fragment_count).unwrap_or(0),
        ),
        max_fragment_created_at_at_generation: fragment_stats.max_created_at.clone(),
        current_fragment_count: u64::try_from(fragment_stats.fragment_count).unwrap_or(0),
        current_distinct_keyword_count: u64::try_from(fragment_stats.distinct_keyword_count)
            .unwrap_or(0),
    })
}

fn outcome_ungenerated(
    stats: &nexus_local_db::SoulNarrativeFragmentStats,
) -> ReflectOutcome {
    ReflectOutcome {
        state: ReflectState::Ungenerated,
        narrative: None,
        generated_at: None,
        stale: false,
        fragment_count_at_generation: None,
        max_fragment_created_at_at_generation: None,
        current_fragment_count: u64::try_from(stats.fragment_count).unwrap_or(0),
        current_distinct_keyword_count: u64::try_from(stats.distinct_keyword_count).unwrap_or(0),
    }
}

fn outcome_stale(
    c: &nexus_local_db::SoulNarrativeRecord,
    stats: &nexus_local_db::SoulNarrativeFragmentStats,
) -> ReflectOutcome {
    ReflectOutcome {
        state: ReflectState::Stale,
        narrative: c.narrative.clone(),
        generated_at: c.generated_at.clone(),
        stale: true,
        fragment_count_at_generation: Some(u64::try_from(c.fragment_count_at_generation).unwrap_or(0)),
        max_fragment_created_at_at_generation: c.max_fragment_created_at_at_generation.clone(),
        current_fragment_count: u64::try_from(stats.fragment_count).unwrap_or(0),
        current_distinct_keyword_count: u64::try_from(stats.distinct_keyword_count).unwrap_or(0),
    }
}

fn outcome_current(
    c: &nexus_local_db::SoulNarrativeRecord,
    stats: &nexus_local_db::SoulNarrativeFragmentStats,
) -> ReflectOutcome {
    ReflectOutcome {
        state: ReflectState::Current,
        narrative: c.narrative.clone(),
        generated_at: c.generated_at.clone(),
        stale: false,
        fragment_count_at_generation: Some(u64::try_from(c.fragment_count_at_generation).unwrap_or(0)),
        max_fragment_created_at_at_generation: c.max_fragment_created_at_at_generation.clone(),
        current_fragment_count: u64::try_from(stats.fragment_count).unwrap_or(0),
        current_distinct_keyword_count: u64::try_from(stats.distinct_keyword_count).unwrap_or(0),
    }
}

/// Dispatch fragment-stats + cache-row lookup to the bearer's scope.
async fn bearer_fragment_stats(
    pool: &SqlitePool,
    ctx: &BearerPipelineCtx<'_>,
) -> Result<
    (
        nexus_local_db::SoulNarrativeFragmentStats,
        Option<nexus_local_db::SoulNarrativeRecord>,
    ),
    NexusApiError,
> {
    match ctx.bearer {
        MemoryBearerRef::Creator(creator_id) => {
            let (stats, cached) = nexus_local_db::soul_narrative_fragment_stats(
                pool,
                creator_id,
                ctx.scope_id,
            )
            .await
            .map_err(map_local_db_error)?;
            Ok((stats, cached))
        }
        MemoryBearerRef::Character {
            owner_creator_id,
            character_id,
        } => {
            let (stats, cached) =
                nexus_local_db::character_soul_narrative_fragment_stats(
                    pool,
                    owner_creator_id,
                    character_id,
                    ctx.scope_id,
                )
                .await
                .map_err(map_local_db_error)?;
            // Same record shape — the creator cache uses `SoulNarrativeRecord`.
            let cached = cached.map(|c| nexus_local_db::SoulNarrativeRecord {
                creator_id: character_id.to_string(),
                world_id: c.actor_world_binding_id,
                narrative: c.narrative,
                generated_at: c.generated_at,
                fragment_count_at_generation: c.fragment_count_at_generation,
                max_fragment_created_at_at_generation: c.max_fragment_created_at_at_generation,
                distinct_keyword_count_cache: c.distinct_keyword_count_cache,
                stats_fingerprint: c.stats_fingerprint,
                created_at: c.created_at,
                updated_at: c.updated_at,
            });
            Ok((stats, cached))
        }
    }
}

/// Fetch a bounded page of recent fragmens for the bearer's scope.
async fn bearer_recent_fragment_signals(
    pool: &SqlitePool,
    ctx: &BearerPipelineCtx<'_>,
) -> Result<Vec<FragmentSignal>, NexusApiError> {
    const FETCH_LIMIT: i64 = 100;
    match ctx.bearer {
        MemoryBearerRef::Creator(creator_id) => {
            let rows =
                nexus_local_db::list_fragments_limited(pool, creator_id, ctx.scope_id, FETCH_LIMIT)
                    .await
                    .map_err(map_local_db_error)?;
            Ok(rows
                .into_iter()
                .map(|f| FragmentSignal {
                    keywords: f.keywords,
                    summary: f.summary,
                    created_at: f.created_at,
                })
                .collect())
        }
        MemoryBearerRef::Character {
            owner_creator_id,
            character_id,
        } => {
            let rows = nexus_local_db::list_character_fragments(
                pool,
                owner_creator_id,
                character_id,
                ctx.scope_id,
                FETCH_LIMIT,
                0,
            )
            .await
            .map_err(map_local_db_error)?;
            Ok(rows
                .into_iter()
                .map(|f| FragmentSignal {
                    keywords: f.keywords,
                    summary: f.summary,
                    created_at: f.created_at,
                })
                .collect())
        }
    }
}

/// Persist a synthesized narrative (with its stats cache) to the bearer's
/// narrative cache table.
async fn bearer_persist_narrative(
    pool: &SqlitePool,
    ctx: &BearerPipelineCtx<'_>,
    narrative: &str,
    stats: &nexus_local_db::SoulNarrativeFragmentStats,
) -> Result<(), NexusApiError> {
    let now = chrono::Utc::now().to_rfc3339();
    let stats_fingerprint = nexus_local_db::build_stats_fingerprint(
        stats.fragment_count,
        stats.max_created_at.as_deref(),
    );
    match ctx.bearer {
        MemoryBearerRef::Creator(creator_id) => {
            let record = nexus_local_db::SoulNarrativeRecord {
                creator_id: creator_id.to_string(),
                world_id: ctx.scope_id.map(std::string::ToString::to_string),
                narrative: Some(narrative.to_string()),
                generated_at: Some(now.clone()),
                fragment_count_at_generation: stats.fragment_count,
                max_fragment_created_at_at_generation: stats.max_created_at.clone(),
                distinct_keyword_count_cache: i64::try_from(stats.distinct_keyword_count)
                    .unwrap_or(0),
                stats_fingerprint: Some(stats_fingerprint),
                created_at: now.clone(),
                updated_at: now,
            };
            nexus_local_db::upsert_soul_narrative(pool, &record)
                .await
                .map_err(map_local_db_error)
        }
        MemoryBearerRef::Character {
            owner_creator_id,
            character_id,
        } => {
            let record = nexus_local_db::CharacterSoulNarrativeRecord {
                character_id: character_id.to_string(),
                actor_world_binding_id: ctx.scope_id.map(std::string::ToString::to_string),
                narrative: Some(narrative.to_string()),
                generated_at: Some(now.clone()),
                fragment_count_at_generation: stats.fragment_count,
                max_fragment_created_at_at_generation: stats.max_created_at.clone(),
                distinct_keyword_count_cache: i64::try_from(stats.distinct_keyword_count)
                    .unwrap_or(0),
                stats_fingerprint: Some(stats_fingerprint),
                created_at: now.clone(),
                updated_at: now,
            };
            nexus_local_db::upsert_character_soul_narrative(pool, owner_creator_id, &record)
                .await
                .map_err(map_local_db_error)
        }
    }
}

// ── Character mind projection (v1.184 P3) ────────────────────────────────

/// Max fragments fetched per scope before deterministic merge + cap.
const MIND_PROJECTION_FETCH_LIMIT: i64 = 100;

/// Max Character long-term-memory files projected before deterministic cap.
const MIND_PROJECTION_LTM_LIMIT: usize = 20;

/// Load the bounded, deterministic Character SOUL/Memory projection for an
/// admitted Character scope and fold it into a [`CharacterMindInput`].
///
/// Caller guarantees admission (owner/active Character + active binding).
///
/// **Honest-empty vs fail-closed:** only explicit *absent* optional data is an
/// honest empty — a missing SOUL.md yields `None` and a missing/empty
/// long-term-memory directory yields no lines. Any other read/DB error (e.g.
/// a permission error, malformed home path, or a failed fragment query) is
/// propagated so the caller aborts **before** host launch rather than
/// executing with an incomplete Character mind. Only the executing
/// Character's shared scope + the selected binding-local scope are included —
/// never another Character's or the Creator's data. The merged memory lines
/// (fragments + promoted long-term memory files) are bounded and
/// deterministically ordered by [`CharacterMindInput::new`], which caps and
/// truncates.
///
/// # Errors
///
/// Returns an `Internal`/`DATABASE_ERROR`/validation `NexusApiError` on any
/// projection read failure other than a recognised absent-data condition.
pub(crate) async fn load_character_mind_projection(
    pool: &SqlitePool,
    nexus_home: &Path,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: Option<&str>,
) -> Result<CharacterMindInput, NexusApiError> {
    let bearer = MemoryBearerRef::Character {
        owner_creator_id,
        character_id,
    };
    bearer.validate().map_err(|e| NexusApiError::InvalidInput {
        field: "character_id".into(),
        reason: e.to_string(),
    })?;

    // SOUL: a missing SOUL.md is honest-empty; any other read error fails closed.
    let soul = match std::fs::read_to_string(bearer.soul_path(nexus_home)) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return Err(NexusApiError::Internal {
                code: "CHARACTER_SOUL_READ_ERROR".into(),
                message: format!("failed to read Character SOUL.md: {e}"),
            });
        }
    };

    // Fragment rows: shared scope + the selected binding-local scope. A
    // binding read merges both; a shared read (None) fetches shared once. A
    // failed query is a fail-closed error, never an empty projection.
    let mut rows: Vec<(String, String, String)> = Vec::new(); // (created_at, fragment_id, summary)
    let mut keywords_by_fragment: Vec<(String, String)> = Vec::new(); // (fragment_id, keywords)
    let mut scopes = vec![None];
    if let Some(b) = binding_id {
        scopes.push(Some(b));
    }
    for scope in scopes {
        let fetched = nexus_local_db::list_character_fragments(
            pool,
            owner_creator_id,
            character_id,
            scope,
            MIND_PROJECTION_FETCH_LIMIT,
            0,
        )
        .await
        .map_err(map_local_db_error)?;
        for f in fetched {
            rows.push((f.created_at.clone(), f.fragment_id.clone(), f.summary));
            keywords_by_fragment.push((f.fragment_id, f.keywords));
        }
    }

    // Promoted long-term memory files (authoritative pipeline sink): the
    // capture→review→promote journey must be visible to `character run`.
    let mut ltm_lines: Vec<String> = Vec::new();
    let ltm_slugs = nexus_creator_memory::memory_io::list_memories(nexus_home, bearer)
        .map_err(|e| NexusApiError::Internal {
            code: "CHARACTER_MEMORY_LIST_ERROR".into(),
            message: e.to_string(),
        })?;
    for slug in ltm_slugs.into_iter().take(MIND_PROJECTION_LTM_LIMIT) {
        let content = nexus_creator_memory::memory_io::load_memory(nexus_home, bearer, &slug)
            .map_err(|e| NexusApiError::Internal {
                code: "CHARACTER_MEMORY_LOAD_ERROR".into(),
                message: e.to_string(),
            })?;
        // Render the frontmatter-body text as a deterministic memory line.
        let body = content
            .render()
            .map_err(|e| NexusApiError::Internal {
                code: "CHARACTER_MEMORY_RENDER_ERROR".into(),
                message: e.to_string(),
            })?;
        ltm_lines.push(format!("- {body}"));
    }

    // Deterministic merge: created_at DESC, fragment_id DESC (newest first).
    rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    let lines: Vec<String> = rows
        .into_iter()
        .map(|(_, fragment_id, summary)| {
            let keywords = keywords_by_fragment
                .iter()
                .find(|(id, _)| *id == fragment_id)
                .map(|(_, kw)| kw.clone())
                .unwrap_or_default();
            if keywords.is_empty() {
                format!("- {summary}")
            } else {
                format!("- {summary} — keywords: {keywords}")
            }
        })
        .collect();
    // Long-term memory files join the projection (bounded at the top of the
    // merged list); ordering is deterministic by the LTM slug sort then the
    // fragmentation merge order is preserved below the LTM block.
    // Reserve the Character-mind entry budget for the admitted fragment scopes
    // first (shared + selected binding), so a full global LTM never crowds out
    // binding-local fragments. LTM files fill only the remaining capacity.
    let mut all_lines = lines;
    all_lines.extend(ltm_lines);
    Ok(CharacterMindInput::new(soul, all_lines))
}

/// Per-order MCA ToM fetch bound: L1 and L2 are fetched independently at the
/// `CharacterMindInput` slot cap so neither order can starve the other
/// (QC fix round 1, F-003).
const MIND_PROJECTION_TOM_ORDER_LIMIT: u32 = 20;

/// One deterministic human line for a projected ToM belief row (v1.184 P4).
fn format_tom_belief_line(row: &crate::character_tom::CharacterTomBeliefRow) -> String {
    let holder = row.belief.holder.as_deref().unwrap_or("?");
    let proposition = row.belief.proposition.as_deref().unwrap_or("");
    let order = row.belief.order.unwrap_or(0);
    let truth = row.belief.truth.as_deref().unwrap_or("Unknown");
    format!("- [{order}] holder={holder} truth={truth} {proposition}")
}

/// Load bounded SOUL/Memory plus L1-then-L2 ToM for an admitted Character run.
pub(crate) async fn load_character_mind_projection_with_tom(
    pool: &SqlitePool,
    nexus_home: &Path,
    owner_creator_id: &str,
    character_id: &str,
    world_id: &str,
    binding_id: &str,
) -> Result<CharacterMindInput, NexusApiError> {
    let mind = load_character_mind_projection(
        pool,
        nexus_home,
        owner_creator_id,
        character_id,
        Some(binding_id),
    )
    .await?;
    let service = CharacterTomService::new(pool.clone());
    // Independent bounded fill per order through the same query service: an
    // L1-heavy corpus can never crowd the L2 rows out of a mixed page.
    let mut tom_l1 = Vec::new();
    let mut tom_l2 = Vec::new();
    for (order, slot) in [(1_i64, &mut tom_l1), (2_i64, &mut tom_l2)] {
        let page = service
            .list(
                owner_creator_id,
                character_id,
                CharacterTomListQuery {
                    world_id: world_id.to_string(),
                    binding_id: binding_id.to_string(),
                    limit: MIND_PROJECTION_TOM_ORDER_LIMIT,
                    cursor: None,
                    order: Some(order),
                },
            )
            .await?;
        for row in page.items {
            slot.push(format_tom_belief_line(&row));
        }
    }
    Ok(mind.with_tom(tom_l1, tom_l2))
}

// ── Synthesis input building (arm-agnostic; V1.81 G2 caps preserved) ──────

fn build_soul_narrative_synthesis_input(
    signals: &[FragmentSignal],
    stats: &nexus_local_db::SoulNarrativeFragmentStats,
) -> nexus_creator_memory::soul_narrative::SoulNarrativeSynthesisInput {
    use nexus_creator_memory::soul_narrative::SoulNarrativeSynthesisInput;

    let mut keyword_counts: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
    let mut summaries: Vec<String> = Vec::new();

    for frag in signals {
        if let Ok(keywords) = serde_json::from_str::<Vec<String>>(&frag.keywords) {
            for kw in keywords {
                *keyword_counts.entry(kw).or_default() += 1;
            }
        }
        if summaries.len() < 24 {
            let summary = truncate_summary(&frag.summary, 280);
            summaries.push(summary);
        }
    }

    let mut top_keywords: Vec<(String, u64)> = keyword_counts.into_iter().collect();
    top_keywords.sort_by_key(|(_k, count)| std::cmp::Reverse(*count));
    top_keywords.truncate(30);

    let temporal_buckets = build_temporal_buckets(signals);

    SoulNarrativeSynthesisInput {
        top_keywords,
        recent_summaries: summaries,
        temporal_buckets,
        total_fragment_count: u64::try_from(stats.fragment_count).unwrap_or(0),
        distinct_keyword_count: u64::try_from(stats.distinct_keyword_count).unwrap_or(0),
        oldest_created_at: signals.last().map(|f| f.created_at.clone()),
        newest_created_at: signals.first().map(|f| f.created_at.clone()),
    }
}

/// Build up to 8 temporal buckets from fragments ordered by `created_at` DESC.
fn build_temporal_buckets(signals: &[FragmentSignal]) -> Vec<nexus_creator_memory::soul_narrative::TemporalBucket> {
    use nexus_creator_memory::soul_narrative::TemporalBucket;

    if signals.is_empty() {
        return Vec::new();
    }

    let max_buckets = 8;
    let n = signals.len();
    let bucket_size = n.div_ceil(max_buckets).max(1);

    let mut buckets: Vec<TemporalBucket> = Vec::new();

    for (bi, chunk) in signals.chunks(bucket_size).enumerate() {
        if buckets.len() >= max_buckets {
            break;
        }
        let mut kw_counts: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        for frag in chunk {
            if let Ok(keywords) = serde_json::from_str::<Vec<String>>(&frag.keywords) {
                for kw in keywords {
                    *kw_counts.entry(kw).or_default() += 1;
                }
            }
        }
        let mut top: Vec<(String, u64)> = kw_counts.into_iter().collect();
        top.sort_by_key(|(_k, count)| std::cmp::Reverse(*count));
        top.truncate(5);
        let top_keywords: Vec<String> = top.into_iter().map(|(k, _)| k).collect();

        let label = chunk.first().map_or_else(
            || format!("bucket_{bi}"),
            |f| {
                if f.created_at.len() >= 10 {
                    f.created_at[..10].to_string()
                } else {
                    f.created_at.clone()
                }
            },
        );

        buckets.push(TemporalBucket {
            label,
            top_keywords,
            fragment_count: u64::try_from(chunk.len()).unwrap_or(0),
        });
    }

    buckets.reverse();
    buckets
}

/// Truncate `summary` to at most `max_chars` Unicode scalar characters,
/// appending `…` when truncating (UTF-8 safe; avoids mid-char byte panic).
pub(crate) fn truncate_summary(summary: &str, max_chars: usize) -> String {
    if summary.chars().count() <= max_chars {
        summary.to_string()
    } else {
        let t: String = summary.chars().take(max_chars - 1).collect();
        format!("{t}…")
    }
}

/// Lightweight deterministic quality gate for a synthesized narrative draft.
fn validate_soul_narrative_draft(
    narrative: &str,
    top_keywords: &[(String, u64)],
) -> Result<(), MemoryError> {
    let lower = narrative.to_lowercase();
    let keyword_hits = top_keywords
        .iter()
        .filter(|(kw, _)| lower.contains(&kw.to_lowercase()))
        .count();

    if keyword_hits >= 2 || has_forward_looking_suffix(narrative) {
        return Ok(());
    }

    Err(MemoryError::QualityThresholdMissed {
        reason: format!(
            "narrative quality floor missed: {keyword_hits} keyword hits and no forward-looking suffix"
        ),
    })
}

/// Heuristic: does the narrative end with a forward-looking reflection?
fn has_forward_looking_suffix(narrative: &str) -> bool {
    let trimmed = narrative.trim_end();
    if trimmed.ends_with('?') {
        return true;
    }

    let last_sentence = trimmed
        .rsplit(['.', '!', '?'])
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or(trimmed);

    let words: Vec<String> = last_sentence
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect();

    if words
        .iter()
        .any(|w| FORWARD_LOOKING_TOKENS.contains(&w.as_str()))
    {
        return true;
    }

    words.windows(2).any(|pair| {
        FORWARD_LOOKING_BIGRAMS
            .iter()
            .any(|(a, b)| pair[0] == *a && pair[1] == *b)
    })
}

/// Map narrative-synthesis `MemoryError` to canonical daemon error shapes.
fn map_soul_narrative_memory_error(err: MemoryError) -> NexusApiError {
    match err {
        MemoryError::WorkerUnavailable => NexusApiError::ServiceUnavailable {
            message: "ACP worker unavailable for narrative synthesis".into(),
        },
        MemoryError::CapabilityMissing { capability } => {
            NexusApiError::ServiceUnavailable {
                message: format!("{capability} capability not available in registry"),
            }
        }
        MemoryError::MalformedOutput { reason }
        | MemoryError::QualityThresholdMissed { reason } => NexusApiError::BadRequest {
            code: "narrative_generation_failed".into(),
            message: reason,
        },
        other => NexusApiError::Internal {
            code: "NARRATIVE_SYNTHESIS_ERROR".into(),
            message: other.to_string(),
        },
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use nexus_creator_memory::bearer::MemoryBearerRef;
    use nexus_creator_memory::errors::MemoryError;

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
        assert!(result.contains("# creator_id:") == false, "no creator_id key");
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

    #[test]
    fn truncate_summary_short_enough_returns_unchanged() {
        let short = "Hello world";
        assert_eq!(truncate_summary(short, 280), short);
    }

    #[test]
    fn truncate_summary_exactly_at_limit_returns_unchanged() {
        let exact = "a".repeat(280);
        assert_eq!(truncate_summary(&exact, 280), exact);
    }

    #[test]
    fn truncate_summary_over_limit_ascii_truncates_with_ellipsis() {
        let long = "a".repeat(300);
        let result = truncate_summary(&long, 280);
        assert_eq!(result.chars().count(), 280);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn truncate_summary_cjk_multibyte_no_panic() {
        let cjk = "字".repeat(300);
        let result = truncate_summary(&cjk, 280);
        assert_eq!(result.chars().count(), 280);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn truncate_summary_emoji_multibyte_no_panic() {
        let emoji = "🎉".repeat(300);
        let result = truncate_summary(&emoji, 280);
        assert_eq!(result.chars().count(), 280);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn truncate_summary_short_below_limit_unchanged() {
        assert_eq!(truncate_summary("abc", 280), "abc");
        assert_eq!(truncate_summary("", 280), "");
    }

    #[test]
    fn validate_draft_passes_with_two_keyword_hits() {
        let keywords = vec![
            ("magic".to_string(), 5),
            ("science".to_string(), 3),
            ("love".to_string(), 1),
        ];
        let narrative = "A story about magic and science intertwined.";
        assert!(validate_soul_narrative_draft(narrative, &keywords).is_ok());
    }

    #[test]
    fn validate_draft_passes_with_forward_looking_suffix() {
        let keywords = vec![("magic".to_string(), 5)];
        let narrative = "The hero stood alone. What will happen next?";
        assert!(validate_soul_narrative_draft(narrative, &keywords).is_ok());
    }

    #[test]
    fn validate_draft_forward_looking_suffix_with_period_terminator() {
        let keywords = vec![("magic".to_string(), 5)];
        let narrative = "The hero stood alone. Their journey will continue.";
        assert!(validate_soul_narrative_draft(narrative, &keywords).is_ok());
    }

    #[test]
    fn validate_draft_fails_when_quality_floor_missed() {
        let keywords = vec![("magic".to_string(), 5), ("science".to_string(), 3)];
        let narrative = "The hero stood alone in a room.";
        let err = validate_soul_narrative_draft(narrative, &keywords)
            .expect_err("should fail quality floor");
        match err {
            MemoryError::QualityThresholdMissed { .. } => {}
            other => panic!("expected QualityThresholdMissed, got {other:?}"),
        }
    }

    #[test]
    fn narrative_longer_than_max_chars_is_truncated_cleanly() {
        let long = "x".repeat(SOUL_NARRATIVE_MAX_CHARS + 100);
        let truncated = truncate_summary(&long, SOUL_NARRATIVE_MAX_CHARS);
        assert_eq!(truncated.chars().count(), SOUL_NARRATIVE_MAX_CHARS);
        assert!(truncated.ends_with('…'));
    }
}
