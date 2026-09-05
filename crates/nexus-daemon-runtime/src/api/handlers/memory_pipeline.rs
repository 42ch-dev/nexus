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
use nexus_creator_memory::bearer::MemoryBearerRef;
use nexus_creator_memory::errors::MemoryError;
use nexus_creator_memory::review::{
    PendingReviewInput, ReviewAction, ReviewDecision, SessionDigestSummarizer,
};
use nexus_creator_memory::soul_narrative::SoulNarrativeSynthesizer;
use sqlx::SqlitePool;
use std::path::Path;

/// A bearer plus its scope provenance for one pipeline run.
///
/// `scope_id` is the Creator arm's world id or the Character arm's binding
/// id; `None` = whole Creator / shared Character.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BearerPipelineCtx<'a> {
    pub bearer: MemoryBearerRef<'a>,
    pub scope_id: Option<&'a str>,
}

impl<'a> BearerPipelineCtx<'a> {
    /// Build a Creator-arm context.
    pub(crate) const fn creator(creator_id: &'a str, scope_id: Option<&'a str>) -> Self {
        Self {
            bearer: MemoryBearerRef::Creator(creator_id),
            scope_id,
        }
    }

    /// Build a Character-arm context, validating format and actor ownership
    /// **before** any DB read, file write, or synthesis. Rejects foreign or
    /// non-existent Characters (fail-closed; never falls back to the
    /// Creator's data).
    ///
    /// Consumed by the Task 3 generated Character handlers and the in-crate
    /// dual-bearer semantic suite; the public Creator handlers use the
    /// Creator arm and never construct a Character context directly.
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
        if owned.is_none() {
            return Err(NexusApiError::Forbidden {
                resource: "character_memory".into(),
                reason: format!("character '{character_id}' is not owned by creator '{owner_creator_id}'"),
            });
        }
        Ok(Self { bearer, scope_id })
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
            let summarizer = PassthroughSummarizer::new(ctx.bearer);
            match nexus_creator_memory::review::promote_to_long_term(
                nexus_home,
                ctx.bearer,
                input,
                &summarizer,
            )
            .await
            {
                Ok(_) => {
                    counts.promoted = 1;
                    delete_pending_row(pool, ctx, &input.pending_id).await;
                }
                Err(e) => {
                    tracing::warn!(
                        pending_id = %input.pending_id,
                        error = %e,
                        "Failed to promote pending review; skipping"
                    );
                }
            }
        }
        ReviewAction::FragmentOnly => {
            let fragment = nexus_creator_memory::review::create_fragment_from_review(input);
            if let Err(e) = insert_bearer_fragment(pool, ctx, &fragment, input.scope_id.as_deref())
                .await
            {
                tracing::warn!(
                    pending_id = %input.pending_id,
                    error = %e,
                    "Failed to create fragment; skipping"
                );
            } else {
                counts.fragmented = 1;
                delete_pending_row(pool, ctx, &input.pending_id).await;
            }
        }
        ReviewAction::Drop => {
            delete_pending_row(pool, ctx, &input.pending_id).await;
            counts.dropped = 1;
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

/// Insert a fragment record into the bearer's fragment table.
async fn insert_bearer_fragment(
    pool: &SqlitePool,
    ctx: &BearerPipelineCtx<'_>,
    fragment: &nexus_creator_memory::review::MemoryFragment,
    scope_id: Option<&str>,
) -> Result<(), NexusApiError> {
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
            nexus_local_db::memory_fragment::create_fragment(pool, &record)
                .await
                .map_err(map_local_db_error)
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
            nexus_local_db::create_character_fragment(pool, owner_creator_id, &record)
                .await
                .map_err(map_local_db_error)
        }
    }
}

/// Delete a consumed pending row from the bearer's table (best-effort).
async fn delete_pending_row(pool: &SqlitePool, ctx: &BearerPipelineCtx<'_>, pending_id: &str) {
    match ctx.bearer {
        MemoryBearerRef::Creator(_) => {
            let pid = pending_id.to_string();
            if let Err(e) = sqlx::query!(
                "DELETE FROM memory_pending_review WHERE pending_id = ?",
                pid
            )
            .execute(pool)
            .await
            {
                tracing::warn!(pending_id = %pending_id, error = %e, "Failed to delete pending review after processing");
            }
        }
        MemoryBearerRef::Character {
            owner_creator_id,
            character_id,
        } => {
            if let Err(e) = nexus_local_db::delete_character_pending_review(
                pool,
                owner_creator_id,
                character_id,
                pending_id,
            )
            .await
            {
                tracing::warn!(pending_id = %pending_id, error = %e, "Failed to delete character pending review after processing");
            }
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
        .synthesize(ctx.bearer, input)
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
    use nexus_creator_memory::soul_narrative::SoulNarrativeSynthesizer as _;

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


#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod semantic_tests {
    use super::*;
    use nexus_creator_memory::bearer::MemoryBearerRef;
    use nexus_creator_memory::long_term_memory::LongTermMemory;
    use nexus_creator_memory::review::PendingReviewInput;
    use nexus_creator_memory::soul_narrative::{
        SoulNarrativeDraft, SoulNarrativeSynthesisInput, SoulNarrativeSynthesizer,
    };
    use nexus_local_db::{
        create_character_with_initial_binding, ensure_creator_row, CreateCharacterParams,
    };
    use std::path::PathBuf;

    const OWNER_A: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OWNER_B: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const WORLD_A: &str = "wld_worldA";

    const PROMOTE_DIGEST: &str =
        "The chapter pivots from betrayal to alliance, with causal consequences for three factions.";
    const FRAGMENT_DIGEST: &str =
        "Research summary long enough to classify as a fragment rather than being dropped for shortness.";
    const DROP_DIGEST: &str = "Too short.";

    struct Sync {
        tmp: crate::test_utils::TestTempRoot,
        nexus_home: PathBuf,
        pool: sqlx::SqlitePool,
        chr_a: String,
    }

    async fn setup() -> Sync {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
        ensure_creator_row(&pool, OWNER_A, "Owner A").await.unwrap();
        ensure_creator_row(&pool, OWNER_B, "Owner B").await.unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(WORLD_A)
        .bind(OWNER_A)
        .bind(WORLD_A)
        .bind(WORLD_A)
        .execute(&pool)
        .await
        .unwrap();
        let created = create_character_with_initial_binding(
            &pool,
            CreateCharacterParams {
                owner_creator_id: OWNER_A,
                display_name: "Ava",
                image_uri: None,
                persona_json: "{}",
                world_id: WORLD_A,
                world_sheet_entry_id: None,
            },
        )
        .await
        .unwrap();
        Sync {
            tmp,
            nexus_home,
            pool,
            chr_a: created.character.character_id.clone(),
        }
    }

    fn ctxc() -> BearerPipelineCtx<'static> {
        BearerPipelineCtx {
            bearer: MemoryBearerRef::Creator(OWNER_A),
            scope_id: None,
        }
    }

    fn ctxh(chr: &str) -> BearerPipelineCtx<'_> {
        BearerPipelineCtx {
            bearer: MemoryBearerRef::Character {
                owner_creator_id: OWNER_A,
                character_id: chr,
            },
            scope_id: None,
        }
    }

    fn pcr(id: &str, sess: &str, digest: &str, kind: &str) -> PendingReviewInput {
        PendingReviewInput {
            pending_id: id.to_string(),
            session_id: sess.to_string(),
            bearer_id: OWNER_A.to_string(),
            scope_id: None,
            task_kind: kind.to_string(),
            raw_digest: digest.to_string(),
            created_at: "2026-01-01T00:00:01Z".to_string(),
        }
    }

    fn pch(id: &str, sess: &str, digest: &str, kind: &str, chr: &str) -> PendingReviewInput {
        PendingReviewInput {
            pending_id: id.to_string(),
            session_id: sess.to_string(),
            bearer_id: chr.to_string(),
            scope_id: None,
            task_kind: kind.to_string(),
            raw_digest: digest.to_string(),
            created_at: "2026-01-01T00:00:01Z".to_string(),
        }
    }

    async fn count(pool: &sqlx::SqlitePool, sql: &str, bind: &str) -> i64 {
        let row: (i64,) = sqlx::query_as(sql)
            .bind(bind)
            .fetch_one(pool)
            .await
            .unwrap();
        row.0
    }

    async fn count_all(pool: &sqlx::SqlitePool, sql: &str) -> i64 {
        let row: (i64,) = sqlx::query_as(sql).fetch_one(pool).await.unwrap();
        row.0
    }

    struct NoSynth;
    impl SoulNarrativeSynthesizer for NoSynth {
        async fn synthesize(
            &self,
            _: MemoryBearerRef<'_>,
            _: SoulNarrativeSynthesisInput,
        ) -> Result<SoulNarrativeDraft, MemoryError> {
            Err(MemoryError::WorkerUnavailable)
        }
    }

    #[tokio::test]
    async fn review_both_arms_share_classification_and_isolate_storage() {
        let s = setup().await;
        let home = s.nexus_home.clone();
        let pool = s.pool.clone();
        let chr = s.chr_a.clone();

        let horizon = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        let creator_out = process_bearer_review_batch(
            &[pcr("c_p", "s_p", PROMOTE_DIGEST, "brainstorm"),
              pcr("c_f", "s_f", FRAGMENT_DIGEST, "research"),
              pcr("c_d", "s_d", DROP_DIGEST, "unknown")],
            &home,
            &ctxc(),
            &pool,
            horizon,
        )
        .await;
        assert_eq!(creator_out.promoted, 1);
        assert_eq!(creator_out.fragmented, 1);
        assert_eq!(creator_out.dropped, 1);

        let char_out = process_bearer_review_batch(
            &[pch("c_p", "s_p", PROMOTE_DIGEST, "brainstorm", &chr),
              pch("c_f", "s_f", FRAGMENT_DIGEST, "research", &chr),
              pch("c_d", "s_d", DROP_DIGEST, "unknown", &chr)],
            &home,
            &ctxh(&chr),
            &pool,
            horizon,
        )
        .await;
        assert_eq!(char_out.promoted, 1);
        assert_eq!(char_out.fragmented, 1);
        assert_eq!(char_out.dropped, 1);

        // Both pending queues drained.
        assert_eq!(count_all(&pool, "SELECT COUNT(*) FROM memory_pending_review").await, 0);
        assert_eq!(
            count_all(&pool, "SELECT COUNT(*) FROM character_memory_pending_review").await,
            0
        );
        // Fragments landed in bearer-specific tables only.
        assert_eq!(
            count(&pool, "SELECT COUNT(*) FROM memory_fragments WHERE creator_id = ?", OWNER_A).await,
            1
        );
        assert_eq!(
            count(&pool, "SELECT COUNT(*) FROM character_memory_fragments WHERE character_id = ?", &chr).await,
            1
        );
        assert_eq!(
            count_all(&pool, "SELECT COUNT(*) FROM memory_fragments WHERE creator_id = 'ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'").await,
            0
        );

        // File isolation: Creator and Character memory dirs contain exactly
        // their own promoted files and are distinct roots.
        let cdir = MemoryBearerRef::Creator(OWNER_A).long_term_memory_dir(&home);
        let hdir = MemoryBearerRef::Character {
            owner_creator_id: OWNER_A,
            character_id: &chr,
        }
        .long_term_memory_dir(&home);
        assert_ne!(cdir, hdir);
        assert_eq!(std::fs::read_dir(&cdir).unwrap().count(), 1, "creator memory dir");
        assert_eq!(std::fs::read_dir(&hdir).unwrap().count(), 1, "character memory dir");

        drop(s.tmp);
    }

    #[tokio::test]
    async fn promotion_is_idempotent_for_both_arms() {
        let s = setup().await;
        let home = s.nexus_home.clone();
        let chr = s.chr_a.clone();

        use nexus_creator_memory::review::SessionDigestSummarizer;
        struct Fix;
        impl SessionDigestSummarizer for Fix {
            async fn summarize(
                &self,
                _: &str,
                _: &str,
                _: &str,
                _: Option<&str>,
            ) -> Result<String, MemoryError> {
                Ok("fixed body.".to_string())
            }
        }
        let fix = Fix;

        let ci = pcr("p1", "sess_x", PROMOTE_DIGEST, "brainstorm");
        let hi = pch("p2", "sess_x", PROMOTE_DIGEST, "brainstorm", &chr);
        let cb = MemoryBearerRef::Creator(OWNER_A);
        let hb = MemoryBearerRef::Character {
            owner_creator_id: OWNER_A,
            character_id: &chr,
        };

        nexus_creator_memory::review::promote_to_long_term(&home, cb, &ci, &fix)
            .await
            .unwrap();
        let dup = nexus_creator_memory::review::promote_to_long_term(&home, cb, &ci, &fix).await;
        assert!(dup.is_err());
        assert!(dup.unwrap_err().to_string().contains("already promoted"));

        nexus_creator_memory::review::promote_to_long_term(&home, hb, &hi, &fix)
            .await
            .unwrap();
        let dup = nexus_creator_memory::review::promote_to_long_term(&home, hb, &hi, &fix).await;
        assert!(dup.is_err());
        assert!(dup.unwrap_err().to_string().contains("already promoted"));

        drop(s.tmp);
    }

    #[tokio::test]
    async fn aggregation_updates_soul_in_the_right_root() {
        let s = setup().await;
        let home = s.nexus_home.clone();
        let chr = s.chr_a.clone();

        let cb = MemoryBearerRef::Creator(OWNER_A);
        let hb = MemoryBearerRef::Character {
            owner_creator_id: OWNER_A,
            character_id: &chr,
        };
        nexus_creator_memory::soul_io::create(&home, cb).unwrap();
        let mut cmem = LongTermMemory::new("story_summary");
        cmem.set_body("A grand adventure story.");
        nexus_creator_memory::memory_io::save_memory(&home, cb, "adventure", &cmem).unwrap();

        nexus_creator_memory::soul_io::create(&home, hb).unwrap();
        let mut chmem = LongTermMemory::new("story_summary");
        chmem.set_body("A grand adventure story.");
        nexus_creator_memory::memory_io::save_memory(&home, hb, "adventure", &chmem).unwrap();

        let cres = nexus_creator_memory::experience_aggregation::aggregate_experience(
            &home, cb, None,
        )
        .await
        .unwrap();
        let hres = nexus_creator_memory::experience_aggregation::aggregate_experience(
            &home, hb, None,
        )
        .await
        .unwrap();
        assert_eq!(cres.experience_markdown, hres.experience_markdown);
        assert_eq!(cres.memories_processed, 1);
        assert_eq!(hres.memories_processed, 1);

        let c_soul = std::fs::read_to_string(cb.soul_path(&home)).unwrap();
        let h_soul = std::fs::read_to_string(hb.soul_path(&home)).unwrap();
        assert!(c_soul.contains("### Story Summary"));
        assert!(h_soul.contains("### Story Summary"));

        drop(s.tmp);
    }

    #[tokio::test]
    async fn reflect_both_arms_report_insufficient_data_and_ungenerated() {
        let s = setup().await;
        let pool = s.pool.clone();
        let chr = s.chr_a.clone();
        let home = s.nexus_home.clone();

        let c_ctx = ctxc();
        let h_ctx = ctxh(&chr);
        let no_synth: Option<&NoSynth> = None;

        assert_eq!(
            reflect_bearer_soul(&pool, &c_ctx, false, no_synth).await.unwrap().state,
            ReflectState::InsufficientData
        );
        assert_eq!(
            reflect_bearer_soul(&pool, &h_ctx, false, no_synth).await.unwrap().state,
            ReflectState::InsufficientData
        );

        for i in 0..25 {
            let kw = format!("uniq_{i}");
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO memory_fragments \
                 (fragment_id, session_id, creator_id, keywords, summary, created_at, ttl, world_id) \
                 VALUES (?, ?, ?, ?, ?, ?, NULL, NULL)",
            )
            .bind(format!("cf_{i:04}"))
            .bind(format!("scf_{i:04}"))
            .bind(OWNER_A)
            .bind(format!(r#"["{kw}"]"#))
            .bind(format!("summary {i}"))
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();

            sqlx::query(
                "INSERT INTO character_memory_fragments \
                 (fragment_id, session_id, character_id, actor_world_binding_id, keywords, summary, created_at, ttl, revision) \
                 VALUES (?, ?, ?, NULL, ?, ?, ?, NULL, 0)",
            )
            .bind(format!("chf_{i:04}"))
            .bind(format!("schf_{i:04}"))
            .bind(&chr)
            .bind(format!(r#"["{kw}"]"#))
            .bind(format!("summary {i}"))
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();
        }

        let o = reflect_bearer_soul(&pool, &c_ctx, false, no_synth).await.unwrap();
        assert_eq!(o.state, ReflectState::Ungenerated);
        assert_eq!(o.current_fragment_count, 25);
        let o = reflect_bearer_soul(&pool, &h_ctx, false, no_synth).await.unwrap();
        assert_eq!(o.state, ReflectState::Ungenerated);
        assert_eq!(o.current_fragment_count, 25);

        struct Mock;
        impl SoulNarrativeSynthesizer for Mock {
            async fn synthesize(
                &self,
                _: MemoryBearerRef<'_>,
                input: SoulNarrativeSynthesisInput,
            ) -> Result<SoulNarrativeDraft, MemoryError> {
                let kw = input
                    .top_keywords
                    .first()
                    .map(|(k, _)| k.clone())
                    .unwrap_or_default();
                Ok(SoulNarrativeDraft {
                    narrative: format!("A reflective narrative about {kw} and magic, looking ahead."),
                })
            }
        }
        let mock = Mock;

        let o = reflect_bearer_soul(&pool, &c_ctx, true, Some(&mock)).await.unwrap();
        assert_eq!(o.state, ReflectState::Current);
        assert_eq!(count_all(&pool, "SELECT COUNT(*) FROM memory_soul_narratives").await, 1);

        let o = reflect_bearer_soul(&pool, &h_ctx, true, Some(&mock)).await.unwrap();
        assert_eq!(o.state, ReflectState::Current);
        assert_eq!(
            count_all(&pool, "SELECT COUNT(*) FROM character_soul_narratives").await,
            1
        );

        // The character's synthesized narrative landed only in the character
        // cache table (Creator cache unchanged).
        assert_eq!(count_all(&pool, "SELECT COUNT(*) FROM memory_soul_narratives").await, 1);

        // Created SOUL/context files only for the creator (reflect does not
        // write files), but home dir exists; no cross writes.
        let _ = home;
        drop(s.tmp);
    }

    #[tokio::test]
    async fn character_provenance_rejects_foreign_owner_before_side_effects() {
        let s = setup().await;
        let charted = s.chr_a.clone();

        let res = BearerPipelineCtx::character(&s.pool, OWNER_B, &charted, None).await;
        assert!(res.is_err());
        assert!(matches!(res, Err(NexusApiError::Forbidden { .. })));

        let res = BearerPipelineCtx::character(
            &s.pool,
            OWNER_A,
            "chr_ffffffffffffffffffffffffffffffff",
            None,
        )
        .await;
        assert!(res.is_err());

        drop(s.tmp);
    }
}
