//! SOUL narrative reflection and the bounded Character mind projection
//! (v1.190 P2-T2).
//!
//! Moved from the daemon `api::handlers::memory_pipeline` module: the
//! bearer-parameterized SOUL narrative reflect pipeline (insufficient-data
//! gate, stale detection, cached projection, on-demand synthesis through the
//! existing [`SoulNarrativeSynthesizer`] seam) and the bounded
//! SOUL/Memory/`ToM` mind projection consumed by the Host and (in P2-T3) the
//! moment-context assembly.
//!
//! Synthesis stays explicit and on-demand: the deterministic local classifier,
//! cached projection and quality gate are domain-only, while an explicitly
//! requested (`force_regenerate`) synthesis consumes the optional provider
//! effect the execution host supplies. A missing provider yields the retained
//! truthful `ServiceUnavailable` error — never background synthesis and never
//! an empty success.

use std::path::{Path, PathBuf};

use crate::error::{CoreError, CoreResult};
use crate::memory::CharacterTomService;
use crate::memory_pipeline::{MemoryPipelineCtx, internal_err, map_local_db_error, sqlx_internal};
use crate::principal::Principal;
use crate::service::CoreService;
use nexus_contracts::generated::daemon_api::characters::soul::character_soul_narrative_request::CharacterSoulNarrativeRequest;
use nexus_contracts::generated::daemon_api::characters::soul::character_soul_narrative_response::CharacterSoulNarrativeResponse;
use nexus_contracts::generated::daemon_api::memory::soul_narrative_request::SoulNarrativeRequest;
use nexus_contracts::generated::daemon_api::memory::soul_narrative_response::SoulNarrativeResponse;
use nexus_creator_memory::bearer::MemoryBearerRef;
use nexus_creator_memory::errors::MemoryError;
use nexus_creator_memory::soul_narrative::{SoulNarrativeSynthesisInput, SoulNarrativeSynthesizer};
use nexus_local_db::SoulNarrativeFragmentStats;
use nexus_moment_context_assembly::CharacterMindInput;
use sqlx::SqlitePool;

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
/// the wire mapping).
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

/// Map narrative-synthesis `MemoryError` to the retained core error shapes.
fn map_soul_narrative_memory_error(err: MemoryError) -> CoreError {
    match err {
        MemoryError::WorkerUnavailable => {
            CoreError::ServiceUnavailable("ACP worker unavailable for narrative synthesis".into())
        }
        MemoryError::CapabilityMissing { capability } => CoreError::ServiceUnavailable(format!(
            "{capability} capability not available in registry"
        )),
        MemoryError::MalformedOutput { reason }
        | MemoryError::QualityThresholdMissed { reason } => CoreError::NarrativeRejected(reason),
        other => internal_err("narrative_synthesis_error", other),
    }
}

/// Run the bearer-parameterized SOUL narrative reflect pipeline.
///
/// Behavior matches the pre-migration Creator `reflect_soul` core:
/// 1. compute fragment stats + cache row (one DB round-trip);
/// 2. insufficient-data gate **before** any ACP call;
/// 3. stale detection (stats-only rows are ungenerated, not stale);
/// 4. read/poll path (force=false) returns current/stale/ungenerated without
///    calling the synthesizer;
/// 5. force=true synthesizes (on-demand only), validates/caps, persists.
pub(crate) async fn reflect_bearer_soul<S: SoulNarrativeSynthesizer>(
    pool: &SqlitePool,
    ctx: &MemoryPipelineCtx,
    force: bool,
    synthesizer: impl FnOnce() -> Option<S>,
) -> CoreResult<ReflectOutcome> {
    // A forced reflect is a mutation request (persist a synthesized
    // narrative), so a read-only context is rejected up-front — before the
    // insufficient-data early return, which would otherwise let a read ctx
    // "succeed" on a forced request.
    if force {
        ctx.bearer_for_write()?;
    }
    // 1. fragment stats + cache row in one DB round-trip.
    let (fragment_stats, cached) = bearer_fragment_stats(pool, ctx).await?;

    // 2. insufficient-data gate (before any ACP call).
    let min_distinct = usize::try_from(MIN_SOUL_NARRATIVE_DISTINCT_KEYWORDS).unwrap_or(usize::MAX);
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
        if let Some(c) = &cached {
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

    // 5. force=true → synthesize (explicit CTA, on-demand only). The write
    //    gate was taken up-front (a read-only context never reaches here) and
    //    the provider factory is invoked only NOW — after the ownership /
    //    activity admission and the insufficient-data gate — so an
    //    unauthorized or under-gate request never touches provider state.
    //    A missing provider is the retained truthful 503 — no background
    //    synthesis and no empty success.
    let signals = bearer_recent_fragment_signals(pool, ctx).await?;
    let input = build_soul_narrative_synthesis_input(&signals, &fragment_stats);

    let top_keywords = input.top_keywords.clone();

    let synth = synthesizer()
        .as_ref()
        .ok_or_else(|| {
            CoreError::ServiceUnavailable("capability registry not available".to_string())
        })?;
    let draft = synth
        .synthesize(ctx.bearer_ref(), input, ctx.scope())
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

fn outcome_ungenerated(stats: &SoulNarrativeFragmentStats) -> ReflectOutcome {
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
    stats: &SoulNarrativeFragmentStats,
) -> ReflectOutcome {
    ReflectOutcome {
        state: ReflectState::Stale,
        narrative: c.narrative.clone(),
        generated_at: c.generated_at.clone(),
        stale: true,
        fragment_count_at_generation: Some(
            u64::try_from(c.fragment_count_at_generation).unwrap_or(0),
        ),
        max_fragment_created_at_at_generation: c.max_fragment_created_at_at_generation.clone(),
        current_fragment_count: u64::try_from(stats.fragment_count).unwrap_or(0),
        current_distinct_keyword_count: u64::try_from(stats.distinct_keyword_count).unwrap_or(0),
    }
}

fn outcome_current(
    c: &nexus_local_db::SoulNarrativeRecord,
    stats: &SoulNarrativeFragmentStats,
) -> ReflectOutcome {
    ReflectOutcome {
        state: ReflectState::Current,
        narrative: c.narrative.clone(),
        generated_at: c.generated_at.clone(),
        stale: false,
        fragment_count_at_generation: Some(
            u64::try_from(c.fragment_count_at_generation).unwrap_or(0),
        ),
        max_fragment_created_at_at_generation: c.max_fragment_created_at_at_generation.clone(),
        current_fragment_count: u64::try_from(stats.fragment_count).unwrap_or(0),
        current_distinct_keyword_count: u64::try_from(stats.distinct_keyword_count).unwrap_or(0),
    }
}

/// Dispatch fragment-stats + cache-row lookup to the bearer's scope.
///
/// Read-only by contract: the local-db `*_readonly` variants never persist
/// stats-only cache rows, so a non-forced reflect performs zero DB writes
/// (rows, cache rows and files) even on fingerprint mismatch or for an
/// archived Character's retained read. The cache is only ever persisted by
/// the explicit write path (`bearer_persist_narrative` after a successful
/// synthesis).
async fn bearer_fragment_stats(
    pool: &SqlitePool,
    ctx: &MemoryPipelineCtx,
) -> CoreResult<(
    SoulNarrativeFragmentStats,
    Option<nexus_local_db::SoulNarrativeRecord>,
)> {
    match ctx.bearer_ref() {
        MemoryBearerRef::Creator(creator_id) => {
            let (stats, cached) =
                nexus_local_db::soul_narrative_fragment_stats_readonly(pool, creator_id, ctx.scope())
                    .await
                    .map_err(map_local_db_error)?;
            Ok((stats, cached))
        }
        MemoryBearerRef::Character {
            owner_creator_id,
            character_id,
        } => {
            let (stats, cached) = nexus_local_db::character_soul_narrative_fragment_stats_readonly(
                pool,
                owner_creator_id,
                character_id,
                ctx.scope(),
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

/// Fetch a bounded page of recent fragments for the bearer's scope.
async fn bearer_recent_fragment_signals(
    pool: &SqlitePool,
    ctx: &MemoryPipelineCtx,
) -> CoreResult<Vec<FragmentSignal>> {
    const FETCH_LIMIT: i64 = 100;
    match ctx.bearer_ref() {
        MemoryBearerRef::Creator(creator_id) => {
            let rows =
                nexus_local_db::list_fragments_limited(pool, creator_id, ctx.scope(), FETCH_LIMIT)
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
                ctx.scope(),
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
    ctx: &MemoryPipelineCtx,
    narrative: &str,
    stats: &SoulNarrativeFragmentStats,
) -> CoreResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let stats_fingerprint = nexus_local_db::build_stats_fingerprint(
        stats.fragment_count,
        stats.max_created_at.as_deref(),
    );
    match ctx.bearer_ref() {
        MemoryBearerRef::Creator(creator_id) => {
            let record = nexus_local_db::SoulNarrativeRecord {
                creator_id: creator_id.to_string(),
                world_id: ctx.scope().map(str::to_string),
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
                actor_world_binding_id: ctx.scope().map(str::to_string),
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
    stats: &SoulNarrativeFragmentStats,
) -> SoulNarrativeSynthesisInput {
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
fn build_temporal_buckets(
    signals: &[FragmentSignal],
) -> Vec<nexus_creator_memory::soul_narrative::TemporalBucket> {
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

// ── Character mind projection (v1.184 P3; core-owned since v1.190 P2-T2) ──

/// Max fragments fetched per scope before deterministic merge + cap.
const MIND_PROJECTION_FETCH_LIMIT: i64 = 100;

/// Max Character long-term-memory files projected before deterministic cap.
const MIND_PROJECTION_LTM_LIMIT: usize = 20;

/// Per-order MCA `ToM` fetch bound: L1 and L2 are fetched independently at the
/// `CharacterMindInput` slot cap so neither order can starve the other
/// (QC fix round 1, F-003).
const MIND_PROJECTION_TOM_ORDER_LIMIT: u32 = 20;

/// One deterministic human line for a projected `ToM` belief row (v1.184 P4).
fn format_tom_belief_line(row: &crate::memory::CharacterTomBeliefRow) -> String {
    let holder = row.belief.holder.as_deref().unwrap_or("?");
    let proposition = row.belief.proposition.as_deref().unwrap_or("");
    let order = row.belief.order.unwrap_or(0);
    let truth = row.belief.truth.as_deref().unwrap_or("Unknown");
    format!("- [{order}] holder={holder} truth={truth} {proposition}")
}

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
/// Returns an internal/`DATABASE_ERROR`/validation [`CoreError`] on any
/// projection read failure other than a recognised absent-data condition.
pub(crate) async fn load_character_mind_projection(
    pool: &SqlitePool,
    nexus_home: &Path,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: Option<&str>,
) -> CoreResult<CharacterMindInput> {
    let bearer = MemoryBearerRef::Character {
        owner_creator_id,
        character_id,
    };
    bearer.validate().map_err(|e| CoreError::InvalidInput {
        field: "character_id".to_string(),
        reason: e.to_string(),
    })?;

    // SOUL: a missing SOUL.md is honest-empty; any other read error fails closed.
    let soul = match std::fs::read_to_string(bearer.soul_path(nexus_home)) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return Err(internal_err(
                "character_soul_read_error",
                format!("failed to read Character SOUL.md: {e}"),
            ));
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
        .map_err(|e| internal_err("character_memory_list_error", e))?;
    for slug in ltm_slugs.into_iter().take(MIND_PROJECTION_LTM_LIMIT) {
        let content = nexus_creator_memory::memory_io::load_memory(nexus_home, bearer, &slug)
            .map_err(|e| internal_err("character_memory_load_error", e))?;
        // Render the frontmatter-body text as a deterministic memory line.
        let body = content
            .render()
            .map_err(|e| internal_err("character_memory_render_error", e))?;
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

/// Load bounded SOUL/Memory plus L1-then-L2 `ToM` for an admitted Character run.
pub(crate) async fn load_character_mind_projection_with_tom(
    pool: &SqlitePool,
    nexus_home: &Path,
    owner_creator_id: &str,
    character_id: &str,
    world_id: &str,
    binding_id: &str,
) -> CoreResult<CharacterMindInput> {
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
                crate::memory::CharacterTomListQuery {
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

/// Pool-bound bounded Character mind projection (SOUL + Memory + `ToM`).
///
/// The daemon `agent_host` composition (P4-T2's file) and the P2-T3 context
/// assembly consume this reader; [`crate::CoreService`] hosts the
/// principal-verified families around it. No admission is performed here —
/// callers guarantee stored ownership exactly as before the migration.
#[derive(Debug, Clone)]
pub struct CoreCharacterMind {
    pool: SqlitePool,
    nexus_home: PathBuf,
}

impl CoreCharacterMind {
    /// Bind the projection reader to a workspace pool and nexus home.
    #[must_use]
    pub fn new(pool: SqlitePool, nexus_home: PathBuf) -> Self {
        Self { pool, nexus_home }
    }

    /// Bounded SOUL/Memory projection for one Character scope.
    ///
    /// # Errors
    /// Any fail-closed projection read failure (see
    /// [`load_character_mind_projection`]).
    pub async fn projection(
        &self,
        owner_creator_id: &str,
        character_id: &str,
        binding_id: Option<&str>,
    ) -> CoreResult<CharacterMindInput> {
        load_character_mind_projection(
            &self.pool,
            &self.nexus_home,
            owner_creator_id,
            character_id,
            binding_id,
        )
        .await
    }

    /// Bounded SOUL/Memory plus L1-then-L2 `ToM` for one admitted run scope.
    ///
    /// # Errors
    /// Any fail-closed projection or `ToM` read failure.
    pub async fn projection_with_tom(
        &self,
        owner_creator_id: &str,
        character_id: &str,
        world_id: &str,
        binding_id: &str,
    ) -> CoreResult<CharacterMindInput> {
        load_character_mind_projection_with_tom(
            &self.pool,
            &self.nexus_home,
            owner_creator_id,
            character_id,
            world_id,
            binding_id,
        )
        .await
    }
}

// ── CoreService SOUL commands ────────────────────────────────────────────

/// Wire-mapping failure carrier (same retained code the daemon emitted).
fn wire_err(err: impl std::fmt::Display) -> CoreError {
    internal_err("character_tom_wire_invalid", err)
}

fn map_wire<T: serde::de::DeserializeOwned>(value: impl serde::Serialize) -> CoreResult<T> {
    let json = serde_json::to_value(value).map_err(wire_err)?;
    serde_json::from_value(json).map_err(wire_err)
}

/// Map a bearer-agnostic reflect outcome to the Character wire response.
pub(crate) fn character_reflect_wire(
    character_id: &str,
    o: &ReflectOutcome,
) -> CoreResult<CharacterSoulNarrativeResponse> {
    let state_str = match o.state {
        ReflectState::InsufficientData => "insufficient_data",
        ReflectState::Ungenerated => "ungenerated",
        ReflectState::Current => "current",
        ReflectState::Stale => "stale",
    };
    map_wire(serde_json::json!({
        "character_id": character_id,
        "state": state_str,
        "narrative": o.narrative,
        "generated_at": o.generated_at,
        "stale": o.stale,
        "fragment_count_at_generation": o.fragment_count_at_generation,
        "max_fragment_created_at_at_generation": o.max_fragment_created_at_at_generation,
        "current_fragment_count": o.current_fragment_count,
        "current_distinct_keyword_count": o.current_distinct_keyword_count,
        "min_fragment_count": MIN_SOUL_NARRATIVE_FRAGMENTS,
        "min_distinct_keyword_count": MIN_SOUL_NARRATIVE_DISTINCT_KEYWORDS,
    }))
}

/// Map a bearer-agnostic reflect outcome to the Creator wire response.
pub(crate) fn creator_reflect_wire(
    creator_id: String,
    o: ReflectOutcome,
) -> CoreResult<SoulNarrativeResponse> {
    let state_str = match o.state {
        ReflectState::InsufficientData => "insufficient_data",
        ReflectState::Ungenerated => "ungenerated",
        ReflectState::Current => "current",
        ReflectState::Stale => "stale",
    };
    Ok(SoulNarrativeResponse {
        creator_id,
        state: state_str.parse().expect("valid state constant"),
        narrative: o.narrative,
        generated_at: o.generated_at,
        stale: o.stale,
        fragment_count_at_generation: o.fragment_count_at_generation,
        max_fragment_created_at_at_generation: o.max_fragment_created_at_at_generation,
        current_fragment_count: o.current_fragment_count,
        current_distinct_keyword_count: o.current_distinct_keyword_count,
        min_fragment_count: MIN_SOUL_NARRATIVE_FRAGMENTS,
        min_distinct_keyword_count: MIN_SOUL_NARRATIVE_DISTINCT_KEYWORDS,
    })
}

impl CoreService {
    /// Read or regenerate the cached SOUL narrative for one owned Character
    /// scope (v1.190 P2-T2 contract API).
    ///
    /// A non-regenerating reflect is an observational retained read and never
    /// creates a cache/file or calls the synthesizer (§11.2); a forced reflect
    /// persists a synthesized narrative and holds the per-Character activity
    /// lease across every DB/file/provider effect. An explicitly requested
    /// synthesis consumes the optional provider effect supplied by the
    /// execution host through a lazily evaluated factory — invoked only after
    /// principal verification, stored-ownership admission and the
    /// insufficient-data gate, so an unauthorized request never constructs or
    /// probes a provider. A missing provider yields the retained truthful
    /// `ServiceUnavailable` error — never background synthesis.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::NotFound`] for a foreign/missing Character,
    /// [`CoreError::ActorConflict`] `character_busy`/`character_inactive` from
    /// the activity fence on a forced reflect, the retained 503 when a forced
    /// reflect has no provider, and the mapped synthesis/storage errors
    /// otherwise.
    pub async fn reflect_character_soul<S: SoulNarrativeSynthesizer>(
        &self,
        principal: &Principal,
        character_id: String,
        request: CharacterSoulNarrativeRequest,
        synthesizer: impl FnOnce() -> Option<S>,
    ) -> CoreResult<CharacterSoulNarrativeResponse> {
        self.verify_principal(principal)?;
        let binding_id = request.binding_id.as_ref().map(|id| id.as_str());
        let pool = &self.inner.pool;
        // A non-regenerating reflect is an observational retained read and
        // must never create a cache/file or call the synthesizer (§11.2). A
        // forced reflect persists a synthesized narrative: it requires
        // activity, held across every effect.
        let ctx = if request.force_regenerate {
            let lease = self
                .acquire_actor_activity(
                    principal,
                    &crate::actors::AdmittedActor::Character {
                        character_id: character_id.clone(),
                    },
                )
                .await?;
            MemoryPipelineCtx::character_write(
                lease,
                principal.creator_id(),
                &character_id,
                binding_id,
            )?
        } else {
            MemoryPipelineCtx::character_read(
                pool,
                principal.creator_id(),
                &character_id,
                binding_id,
            )
            .await?
        };
        let outcome =
            reflect_bearer_soul(pool, &ctx, request.force_regenerate, synthesizer).await?;
        drop(ctx);
        character_reflect_wire(&character_id, &outcome)
    }

    /// Read or regenerate the cached whole-Creator (or per-World) SOUL
    /// narrative for the principal (v1.190 P2-T2).
    ///
    /// A requested world scope must be owned; the world-ownership gate keeps
    /// the retained 403 (`creator does not own world`). Synthesis is on-demand
    /// only and consumes the host-supplied optional provider effect.
    ///
    /// # Errors
    /// As [`Self::reflect_character_soul`], plus [`CoreError::ForbiddenReason`]
    /// for a foreign world scope.
    pub async fn reflect_creator_soul<S: SoulNarrativeSynthesizer>(
        &self,
        principal: &Principal,
        request: SoulNarrativeRequest,
        synthesizer: impl FnOnce() -> Option<S>,
    ) -> CoreResult<SoulNarrativeResponse> {
        self.verify_principal(principal)?;
        let world_id = request.world_id.as_deref();

        if let Some(w) = world_id {
            let owned = nexus_local_db::narrative_write::is_world_owned(
                &self.inner.pool,
                principal.creator_id(),
                w,
            )
            .await
            .map_err(sqlx_internal)?;
            if !owned {
                return Err(CoreError::ForbiddenReason {
                    resource: "soul_narrative".to_string(),
                    reason: format!("creator does not own world '{w}'"),
                });
            }
        }

        let ctx = MemoryPipelineCtx::creator(principal.creator_id(), world_id);
        let outcome = reflect_bearer_soul(
            &self.inner.pool,
            &ctx,
            request.force_regenerate,
            synthesizer,
        )
        .await?;
        drop(ctx);
        creator_reflect_wire(principal.creator_id().to_string(), outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
