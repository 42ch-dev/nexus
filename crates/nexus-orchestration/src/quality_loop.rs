//! Review-time KB candidate extraction (V1.50 T-B P1; V1.51 T-A P0 LLM swap).
//!
//! Implements the review-time hook referenced by
//! [`novel-writing/cron-staggering.md` §4.4](../../.mstar/knowledge/specs/novel-writing/cron-staggering.md)
//! and [`entity-scope-model.md` §5.5](../../.mstar/knowledge/specs/entity-scope-model.md).
//!
//! # Two pathways (V1.51)
//!
//! - **LLM pathway** (production, when the supervisor threads a
//!   `CapabilityRegistry` with a live worker): the hook invokes
//!   `LlmExtractTask` → `nexus.llm.extract` capability to obtain candidates
//!   carrying an LLM-judged `block_type`, `canonical_name`, `confidence`, and
//!   a verbatim `source_quote`. Closes `R-V150KBED-01`.
//! - **Heuristic fallback** (V1.50 behavior; used when no registry/worker is
//!   available — hermetic tests, daemon-without-worker): scans chapter prose
//!   for capitalized noun phrases, tags every candidate `character`. Kept so
//!   no-worker environments still produce character-name candidates.
//!
//! See [`extract_kb_candidates_for_review`] for the pathway selection rule and
//! `llm-extract.md` §5 for the full contract.
//!
//! # Hook wiring
//!
//! [`extract_kb_candidates_for_review`] is invoked by the schedule supervisor
//! when a `novel-review-master` schedule reaches a terminal state (see
//! `schedule::supervisor::ScheduleSupervisor::on_schedule_terminal`).
//!
//! V1.51 T-A P2 adds [`detect_missing_kb_on_finalize`], invoked by the same
//! supervisor when a `novel-writing` schedule reaches terminal `Completed`.
//! It scans the finalized chapter prose, diffs against confirmed World KB rows,
//! and writes an advisory log file under
//! `Works/<work_ref>/Logs/kb/missing/<date>-ch<chapter>.md`. Missing candidates
//! are **not** inserted into `kb_extract_jobs`.

use crate::auto_chain::AutoChainError;
use crate::capability::{CapabilityError, CapabilityRegistry};
use nexus_local_db::kb_extract_job::{insert_pending_with_llm, is_idempotent};
use regex::Regex;
use sqlx::SqlitePool;
use std::sync::OnceLock;

/// Capability name for the LLM extraction pathway (V1.51 T-A P0).
///
/// Registered in `CapabilityRegistry` as a sibling to `judge.llm`; both reuse
/// the V1.32 LLM worker pool. See `llm-extract.md` §1.
const LLM_EXTRACT_CAPABILITY: &str = "nexus.llm.extract";

/// Default `block_type` guess for heuristic candidates.
///
/// Capitalized noun phrases in fiction prose are most often character names,
/// so the heuristic tags every candidate as `character`. V1.51 `nexus.llm.extract`
/// replaces this guess with an LLM-judged value; the heuristic is retained only
/// as the no-worker fallback (entity-scope-model §5.5.6).
const DEFAULT_BLOCK_TYPE_GUESS: &str = "character";

/// Maximum candidates persisted per review pass (safety cap to avoid
/// flooding `kb_extract_jobs.pending` from a single chapter scan).
const MAX_CANDIDATES_PER_PASS: usize = 20;

/// A review-time KB candidate.
///
/// Carries both the V1.50 heuristic fields (`canonical_name_guess` +
/// `proposed_payload`) and the V1.51 LLM-extracted metadata (`block_type`,
/// `confidence`, `source_quote`). The heuristic pathway sets `confidence` +
/// `source_quote` to `None` and `block_type` to [`DEFAULT_BLOCK_TYPE_GUESS`];
/// the `nexus.llm.extract` pathway fills all five. [`persist_candidates`]
/// uses the presence of `confidence`/`source_quote` to decide whether to write
/// the dedicated LLM columns or leave them NULL.
#[derive(Debug, Clone, PartialEq)]
pub struct KbCandidate {
    /// Canonical entity name (heuristic: matched phrase; LLM: extracted name).
    pub canonical_name_guess: String,
    /// Proposed `KeyBlockBody` serialized as JSON.
    pub proposed_payload: String,
    /// `block_type` (`snake_case` wire value). Heuristic: always `character`;
    /// LLM: the model's judgement.
    pub block_type: String,
    /// LLM self-reported confidence in `[0.0, 1.0]`. `None` for heuristic
    /// candidates; `Some(x)` for `nexus.llm.extract` candidates. Stored as
    /// `f64` to match the `SQLite` `REAL` column + JSON number representation
    /// (avoids f32→f64 promotion precision loss in the persisted payload).
    pub confidence: Option<f64>,
    /// Verbatim chapter excerpt justifying the extraction. `None` for
    /// heuristic candidates; `Some(s)` for `nexus.llm.extract` candidates.
    pub source_quote: Option<String>,
}

// ── Heuristic ─────────────────────────────────────────────────────────

/// Compile-on-first-use regex for capitalized noun phrases (1–4 words).
///
/// Matches `Lin Xia`, `The Crimson Order`, `Mount Azure`, etc. Does NOT match
/// single-letter capitals or all-caps acronyms (those are noisy in prose).
fn capitalized_phrase_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // \b([A-Z][a-z]+(?:\s+[A-Z][a-z]+){0,3})\b
        // 1–4 capitalized words (Title Case), each word 2+ lowercase chars.
        Regex::new(r"\b([A-Z][a-z]+(?:\s+[A-Z][a-z]+){0,3})\b")
            .expect("static capitalized-phrase regex compiles")
    })
}

/// Common false positives that are structural prose rather than proper nouns.
///
/// Lowercased for case-insensitive filtering. This is intentionally a small,
/// conservative set — the author can reject any false positive via CLI, and
/// the idempotency guard prevents re-inserting it.
fn is_stopword_phrase(phrase: &str) -> bool {
    static STOPWORDS: &[&str] = &[
        "the",
        "a",
        "an",
        "this",
        "that",
        "these",
        "those",
        "it",
        "he",
        "she",
        "we",
        "they",
        "you",
        "i",
        "but",
        "and",
        "or",
        "for",
        "nor",
        "so",
        "yet",
        "chapter",
        "volume",
        "part",
        "section",
        "book",
        "story",
        "outline",
        "prologue",
        "epilogue",
        "note",
        "summary",
        "title",
        "first",
        "second",
        "third",
        "next",
        "previous",
        "when",
        "then",
        "now",
        "here",
        "there",
        "today",
        "tomorrow",
        "yesterday",
        "once",
        "upon",
        "time",
        "day",
        "night",
        "morning",
        "evening",
        "afternoon",
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
        "mr",
        "mrs",
        "ms",
        "dr",
    ];
    let lower = phrase.to_ascii_lowercase();
    // Single-word stopwords are filtered directly; multi-word phrases whose
    // *first* word is a stopword are also filtered (e.g. "The Crimson Order"
    // is kept only if neither word is a stopword — but "The" alone is filtered
    // by the single-word branch when the phrase is just "The").
    if lower.split_whitespace().count() == 1 {
        return STOPWORDS.contains(&lower.as_str());
    }
    // For multi-word phrases, filter if the first word is a common article/
    // structural word that signals sentence-initial casing rather than a name.
    let first = lower.split_whitespace().next().unwrap_or("");
    matches!(
        first,
        "the" | "a" | "an" | "this" | "that" | "these" | "those"
    )
}

/// Pure heuristic: extract KB candidates from chapter prose.
///
/// Returns deduplicated candidates ordered by first appearance. Each
/// candidate's `canonical_name_guess` is the matched phrase as-is; the
/// `proposed_payload` is a minimal novel-profile body with the name recorded
/// as an alias and `novel_category=character` so adopt-time validation
/// (`ValidationMode::Novel`) passes.
///
/// This function is pure (no I/O) so it can be unit-tested hermetically.
#[must_use]
pub fn extract_candidates_from_text(text: &str) -> Vec<KbCandidate> {
    let re = capitalized_phrase_regex();
    let mut seen: Vec<String> = Vec::new();
    for cap in re.captures_iter(text) {
        let phrase = match cap.get(1) {
            Some(m) => m.as_str().trim(),
            None => continue,
        };
        // Filter stopwords + require min length 2 (regex already enforces
        // first char uppercase + ≥1 lowercase, so "I" can't match, but a
        // single 2-char word like "Mr" is caught by the stopword list).
        if phrase.len() < 2 || is_stopword_phrase(phrase) {
            continue;
        }
        // Dedup case-sensitively (phrases are Title Case so this is stable).
        let key = phrase.to_string();
        if seen.iter().any(|s| s == &key) {
            continue;
        }
        seen.push(key);
        if seen.len() >= MAX_CANDIDATES_PER_PASS {
            break;
        }
    }
    seen.into_iter()
        .map(|canonical_name_guess| {
            let payload = serde_json::json!({
                "summary": format!("Candidate extracted from chapter prose: {canonical_name_guess}"),
                "attributes": {
                    "novel_category": "character",
                    "aliases": [canonical_name_guess.as_str()],
                },
                "tags": ["novel", "heuristic-extracted"],
            })
            .to_string();
            KbCandidate {
                canonical_name_guess,
                proposed_payload: payload,
                block_type: DEFAULT_BLOCK_TYPE_GUESS.to_string(),
                confidence: None,
                source_quote: None,
            }
        })
        .collect()
}

// ── Cross-chapter aggregation (V1.51 T-A P1) ──────────────────────────

/// An aggregate of the same canonical entity extracted across multiple
/// chapters of a Work (V1.51 T-A P1 cross-chapter rescan).
///
/// Produced by [`aggregate_candidates_by_canonical_name`]. The
/// `kb_extract_jobs` DB uniqueness `(creator, canonical_name, world)` (V1.50
/// P1 migration) collapses one [`AggregatedCandidate`] to a single pending
/// candidate row; `source_chapters` records every chapter that referenced the
/// entity so the merged row carries cross-chapter provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct AggregatedCandidate {
    /// Canonical entity name (case preserved from the first chapter that
    /// referenced the entity; grouping itself is case-insensitive).
    pub canonical_name: String,
    /// Chapters that referenced this entity, sorted ascending + deduped
    /// (e.g. `[3, 5, 7]`). Injected into `proposed_payload` as a
    /// `source_chapters` JSON array by the aggregator.
    pub source_chapters: Vec<i32>,
    /// Merged `KeyBlockBody` JSON. Based on the first contributing
    /// candidate's payload, with a `source_chapters` array injected so the
    /// upserted row records cross-chapter reuse (entity-scope-model §5.5.1).
    pub proposed_payload: String,
    /// `block_type` (`snake_case` wire value) from the contributing
    /// candidates. Heuristic aggregates: `character`; LLM aggregates: the
    /// model's judgement.
    pub block_type: String,
    /// LLM self-reported confidence, if any contributing candidate carried
    /// one. `None` for pure-heuristic aggregates.
    pub confidence: Option<f64>,
    /// Verbatim source quote, if any contributing candidate carried one.
    /// `None` for pure-heuristic aggregates.
    pub source_quote: Option<String>,
}

/// Pure aggregation: group per-chapter candidates by `canonical_name`
/// (case-insensitive), merging cross-chapter provenance.
///
/// Returns aggregates ordered by first-seen `canonical_name`. Each
/// aggregate's `source_chapters` is the sorted, deduped union of chapters
/// that produced the entity; `proposed_payload` is the first contributing
/// candidate's payload with a `source_chapters` array injected so the
/// upserted `kb_extract_jobs` row records cross-chapter reuse.
///
/// **Pathway-agnostic**: works for heuristic and LLM candidates. For
/// heuristic candidates (`confidence`/`source_quote` are `None`), the
/// aggregate's LLM fields stay `None`. When an LLM candidate and a heuristic
/// candidate share a canonical name, the LLM candidate's metadata wins (it is
/// strictly more informative); within the same pathway, the first-seen
/// candidate's payload is the merge base.
///
/// No I/O — unit-testable hermetically. Used by the work-scoped
/// `creator kb rescan --work <ref>` CLI path (V1.51 T-A P1).
#[must_use]
pub fn aggregate_candidates_by_canonical_name(
    per_chapter: &[(i32, Vec<KbCandidate>)],
) -> Vec<AggregatedCandidate> {
    use std::collections::HashMap;

    // The HashMap maps the lowercased canonical-name key → index into the
    // parallel output Vecs. Output order follows first-seen canonical name.
    let mut key_to_idx: HashMap<String, usize> = HashMap::new();
    // Accumulators keyed by output index.
    let mut canonical_names: Vec<String> = Vec::new();
    let mut chapter_sets: Vec<std::collections::BTreeSet<i32>> = Vec::new();
    let mut payloads: Vec<String> = Vec::new();
    let mut block_types: Vec<String> = Vec::new();
    let mut confidences: Vec<Option<f64>> = Vec::new();
    let mut source_quotes: Vec<Option<String>> = Vec::new();

    for (chapter, candidates) in per_chapter {
        for candidate in candidates {
            let key = candidate.canonical_name_guess.to_ascii_lowercase();
            let idx = if let Some(&i) = key_to_idx.get(&key) {
                i
            } else {
                let i = canonical_names.len();
                key_to_idx.insert(key.clone(), i);
                canonical_names.push(candidate.canonical_name_guess.clone());
                chapter_sets.push(std::collections::BTreeSet::new());
                payloads.push(candidate.proposed_payload.clone());
                block_types.push(candidate.block_type.clone());
                confidences.push(candidate.confidence);
                source_quotes.push(candidate.source_quote.clone());
                i
            };
            // Record this chapter's contribution.
            chapter_sets[idx].insert(*chapter);
            // Prefer LLM metadata when it appears (strictly more informative
            // than heuristic None). First-seen payload stays the merge base.
            if candidate.confidence.is_some() {
                confidences[idx] = candidate.confidence;
            }
            if candidate.source_quote.is_some() {
                source_quotes[idx].clone_from(&candidate.source_quote);
            }
            if candidate.block_type != DEFAULT_BLOCK_TYPE_GUESS
                && block_types[idx] == DEFAULT_BLOCK_TYPE_GUESS
            {
                // An LLM-judged block_type overrides the heuristic default.
                block_types[idx].clone_from(&candidate.block_type);
            }
        }
    }

    canonical_names
        .into_iter()
        .enumerate()
        .map(|(idx, name)| {
            let chapters: Vec<i32> = chapter_sets[idx].iter().copied().collect();
            let merged_payload = inject_source_chapters(&payloads[idx], &chapters);
            AggregatedCandidate {
                canonical_name: name,
                source_chapters: chapters,
                proposed_payload: merged_payload,
                block_type: block_types[idx].clone(),
                confidence: confidences[idx],
                source_quote: source_quotes[idx].clone(),
            }
        })
        .collect()
}

/// Inject a `source_chapters` array into a candidate payload JSON.
///
/// Parses `base_payload` as a JSON object, sets `source_chapters` to the
/// sorted chapter list, and re-serializes. If `base_payload` is not a JSON
/// object (defensive — the heuristic always produces an object), wraps it as
/// `{"source_chapters": [...]}` so the cross-chapter provenance is never lost.
fn inject_source_chapters(base_payload: &str, chapters: &[i32]) -> String {
    let mut value: serde_json::Value =
        serde_json::from_str(base_payload).unwrap_or_else(|_| serde_json::json!({}));
    if !value.is_object() {
        value = serde_json::json!({});
    }
    if let serde_json::Value::Object(map) = &mut value {
        let arr: Vec<serde_json::Value> = chapters.iter().map(|c| serde_json::json!(c)).collect();
        map.insert("source_chapters".to_string(), serde_json::Value::Array(arr));
    }
    serde_json::to_string(&value).unwrap_or_else(|_| base_payload.to_string())
}

// ── Hook entry point ──────────────────────────────────────────────────

/// Review-time KB extraction hook (V1.50 T-B P1; V1.51 T-A P0 LLM swap).
///
/// Mirrors the shape of
/// [`auto_chain::persist_review_findings_for_schedule`] and
/// [`auto_chain::promote_foreshadowing_for_schedule`]: load schedule → work →
/// chapter body, extract candidates, filter existing canonical names, insert
/// pending rows.
///
/// # Pathway selection (V1.51)
///
/// - `registry` is `Some` AND `nexus.llm.extract` is registered AND the worker
///   is available → **LLM pathway**: invokes the capability with the chapter
///   prose, parses `Vec<KbCandidate>` carrying LLM-judged `block_type` +
///   `confidence` + `source_quote`. Closes `R-V150KBED-01`.
/// - Otherwise (no registry, capability absent, or `WorkerUnavailable`) →
///   **heuristic fallback** ([`extract_candidates_from_text`]): V1.50 behavior,
///   tags every candidate `character`. Keeps no-worker environments
///   (hermetic tests, daemon-without-worker) functional.
///
/// # Behavior
///
/// 1. Loads the schedule row to read `preset_id`, `work_id`, `creator_id`.
/// 2. Returns `Ok(0)` early when the preset is not `novel-review-master`.
/// 3. Returns `Ok(0)` when `workspace_dir` is `None` (hermetic DB-only tests)
///    or the work/chapter body file is missing.
/// 4. Reads the current chapter body, runs the selected pathway, filters out
///    names already present in `kb_key_blocks.canonical_name` for the work's
///    world, applies the [`is_idempotent`] guard, and inserts `pending` rows.
///
/// Best-effort + non-blocking by contract: the caller logs any `Err` and does
/// NOT fail the terminal transition (mirrors the review-findings hook).
///
/// # Errors
///
/// Currently invoked only for `novel-review-master` schedules (preset-gated by
/// [`load_review_context`]); the extraction pathway (`extract_via_llm` →
/// [`run_llm_extract`]) is profile-aware via [`ChapterContext::work_profile`]
/// so game-bible review-time hooks (V1.56+) can reuse this function unchanged
/// once the preset gate is widened.
///
/// Returns `AutoChainError::Database` if the schedule/Work lookup or a
/// candidate INSERT fails.
pub async fn extract_kb_candidates_for_review(
    pool: &SqlitePool,
    schedule_id: &str,
    workspace_dir: Option<&std::path::Path>,
    registry: Option<&CapabilityRegistry>,
) -> Result<usize, AutoChainError> {
    let Some(ctx) = load_review_context(pool, schedule_id, workspace_dir).await? else {
        return Ok(0);
    };
    // Pathway selection: LLM when a registry + worker is available, else the
    // V1.50 heuristic fallback (llm-extract.md §5.1).
    let candidates = match extract_via_llm(registry, &ctx).await {
        LlmExtractOutcome::Candidates(c) => c,
        LlmExtractOutcome::WorkerUnavailable => {
            tracing::debug!(
                schedule_id,
                "kb-extract: LLM worker unavailable; falling back to heuristic"
            );
            extract_candidates_from_text(&ctx.prose)
        }
        LlmExtractOutcome::CapabilityError(ref reason) => {
            tracing::warn!(
                schedule_id,
                reason,
                "kb-extract: LLM extraction failed; falling back to heuristic"
            );
            extract_candidates_from_text(&ctx.prose)
        }
    };
    let existing_names = existing_canonical_names(pool, &ctx.world_id).await?;
    let inserted = persist_candidates(pool, schedule_id, &ctx, &existing_names, candidates).await?;
    if inserted > 0 {
        tracing::info!(
            schedule_id,
            work_id = %ctx.work_id,
            chapter = ctx.chapter,
            world_id = ctx.world_id,
            inserted,
            "kb-extract: inserted pending KB candidates"
        );
    }
    Ok(inserted)
}

/// Finalize-time missing-KB detection hook (V1.51 T-A P2).
///
/// When a `novel-writing` schedule completes, scan the finalized chapter prose
/// for entity references, diff against confirmed `KeyBlock` rows in the Work's
/// World, and write an advisory log file under
/// `Works/<work_ref>/Logs/kb/missing/<date>-ch<chapter>.md`. The log is
/// **advisory only**: missing candidates are not inserted into `kb_extract_jobs`.
///
/// # Pathway selection
///
/// Reuses the same LLM/heuristic pathway as [`extract_kb_candidates_for_review`]:
/// LLM when a registry + worker is available, heuristic fallback otherwise.
///
/// # Behavior
///
/// 1. Loads the schedule row; returns `Ok(0)` early unless `preset_id` is
///    `novel-writing` and `work_id` is set.
/// 2. Loads the Work and the finalized chapter body.
/// 3. Extracts candidates, filters out names already present in confirmed
///    `kb_key_blocks` for the World, and writes the missing log.
///
/// Best-effort + non-blocking: the caller logs any `Err` and does NOT fail the
/// terminal transition.
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the schedule/Work lookup fails, or
/// `AutoChainError::InvalidState` for decode failures.
pub async fn detect_missing_kb_on_finalize(
    pool: &SqlitePool,
    schedule_id: &str,
    workspace_dir: Option<&std::path::Path>,
    registry: Option<&CapabilityRegistry>,
) -> Result<usize, AutoChainError> {
    let Some(ctx) = load_finalize_context(pool, schedule_id, workspace_dir).await? else {
        return Ok(0);
    };

    let candidates = match extract_via_llm(registry, &ctx).await {
        LlmExtractOutcome::Candidates(c) => c,
        LlmExtractOutcome::WorkerUnavailable => {
            tracing::debug!(
                schedule_id,
                "kb-missing: LLM worker unavailable; falling back to heuristic"
            );
            extract_candidates_from_text(&ctx.prose)
        }
        LlmExtractOutcome::CapabilityError(ref reason) => {
            tracing::warn!(
                schedule_id,
                reason,
                "kb-missing: LLM extraction failed; falling back to heuristic"
            );
            extract_candidates_from_text(&ctx.prose)
        }
    };

    let existing_names = existing_canonical_names(pool, &ctx.world_id).await?;
    let missing: Vec<KbCandidate> = candidates
        .into_iter()
        .filter(|c| {
            !existing_names
                .iter()
                .any(|n| n.eq_ignore_ascii_case(&c.canonical_name_guess))
        })
        .collect();

    if missing.is_empty() {
        tracing::info!(
            schedule_id,
            work_id = %ctx.work_id,
            chapter = ctx.chapter,
            world_id = ctx.world_id,
            "kb-missing: no missing KB candidates detected"
        );
        return Ok(0);
    }

    let written = write_missing_kb_log(workspace_dir, &ctx, &missing).map_err(|e| {
        AutoChainError::InvalidState(format!(
            "failed to write missing-KB log for {schedule_id}: {e}"
        ))
    })?;

    tracing::info!(
        schedule_id,
        work_id = %ctx.work_id,
        chapter = ctx.chapter,
        world_id = ctx.world_id,
        missing = written,
        "kb-missing: wrote missing KB candidates log"
    );

    Ok(written)
}

/// Outcome of an LLM extraction attempt (V1.52 T-A P0).
///
/// - `Candidates`: LLM returned parsed candidates (may be empty if the LLM
///   produced no entities).
/// - `WorkerUnavailable`: no worker IPC was available. The caller should
///   fall back to the heuristic rather than treat this as "zero candidates".
/// - `CapabilityError`: the capability was missing or returned a non-worker
///   error. Best-effort callers may fall back to the heuristic and log the
///   reason (closes R-V151Q3-W002).
#[derive(Debug)]
pub(crate) enum LlmExtractOutcome {
    Candidates(Vec<KbCandidate>),
    WorkerUnavailable,
    CapabilityError(String),
}

/// Shared LLM extraction invocation used by both the review-time hook and
/// `LlmExtractTask::evaluate` (closes R-V151Q3-W001).
///
/// Resolves the capability from `registry`, builds the canonical input shape
/// `{ prompt, chapter_prose, _creator_id, _session_id }`, and parses the
/// `candidates` array through [`candidate_from_llm_json_for_profile`] scoped
/// to `work_profile`.
///
/// Returns [`LlmExtractOutcome::WorkerUnavailable`] when no worker IPC is
/// available, so callers can distinguish "no worker" from "zero candidates"
/// (closes R-V151Q3-W002).
///
/// V1.55 P2 fix-wave (F-001): added `work_profile` parameter so game-bible
/// callers produce `game_bible_category` payloads instead of `novel_category`.
///
/// R-V152TA-S001: Both `extract_kb_candidates_for_review` and
/// `LlmExtractTask::evaluate` route through this single helper, so the
/// LLM→KbCandidate mapping is consolidated in [`candidate_from_llm_json_for_profile`].
/// The only intentional divergence between the two call sites is the prompt
/// source (default review prompt vs. rendered preset template); parsing,
/// profile-aware payload shaping, and the `MAX_CANDIDATES_PER_PASS` cap are
/// shared.
pub(crate) async fn run_llm_extract(
    registry: Option<&CapabilityRegistry>,
    capability_name: &str,
    prompt: &str,
    chapter_prose: &str,
    creator_id: &str,
    session_id: &str,
    work_profile: &str,
) -> LlmExtractOutcome {
    let Some(registry) = registry else {
        return LlmExtractOutcome::WorkerUnavailable;
    };
    let Some(cap) = registry.get(capability_name) else {
        return LlmExtractOutcome::CapabilityError(format!(
            "capability '{capability_name}' not registered"
        ));
    };

    let input = serde_json::json!({
        "prompt": prompt,
        "chapter_prose": chapter_prose,
        "_creator_id": creator_id,
        // The review-time hook runs outside a preset session; pass an empty
        // session id. The capability only forwards it to the worker IPC for
        // routing — it is not a security identity (SEC-V131-01 covers creator_id).
        "_session_id": session_id,
    });

    let output = match cap.run(input).await {
        Ok(o) => o,
        Err(CapabilityError::WorkerUnavailable) => {
            return LlmExtractOutcome::WorkerUnavailable;
        }
        Err(e) => {
            return LlmExtractOutcome::CapabilityError(format!(
                "capability '{capability_name}' failed: {e}"
            ));
        }
    };

    let candidates_json = output.get("candidates").and_then(|v| v.as_array());
    // R-V152TA-S007: skip the `filter_map` + `collect` allocation when the LLM
    // produced no candidate array (or an empty one). Avoids the prior
    // `.cloned().unwrap_or_default()` which always allocated a `Vec<Value>`.
    let candidates: Vec<KbCandidate> = match candidates_json {
        Some(arr) if !arr.is_empty() => arr
            .iter()
            .filter_map(|c| candidate_from_llm_json_for_profile(c, work_profile))
            .take(MAX_CANDIDATES_PER_PASS)
            .collect(),
        _ => Vec::new(),
    };
    LlmExtractOutcome::Candidates(candidates)
}

/// Attempt LLM extraction via the `nexus.llm.extract` capability for a chapter.
///
/// Thin wrapper around [`run_llm_extract`] that supplies the review-time hook's
/// default extraction prompt and identity fields from [`ChapterContext`].
/// The `work_profile` carried by [`ChapterContext`] controls whether candidates
/// carry `novel_category` or `game_bible_category` attributes (V1.55 P2 F-001).
async fn extract_via_llm(
    registry: Option<&CapabilityRegistry>,
    ctx: &ChapterContext,
) -> LlmExtractOutcome {
    // High-level extraction instruction. The capability wraps this with the
    // JSON output-format framing + the verbatim chapter prose (llm-extract.md §1.3).
    let prompt = "Extract the fictional entities (characters, locations, organizations, items, events, abilities, conflicts, info points) that appear in the chapter prose below. For each entity, judge the most appropriate wire block_type, give a confidence in [0.0,1.0], and quote a verbatim excerpt from the chapter that justifies the extraction.";

    run_llm_extract(
        registry,
        LLM_EXTRACT_CAPABILITY,
        prompt,
        &ctx.prose,
        &ctx.creator_id,
        "",
        &ctx.work_profile,
    )
    .await
}

/// Build a [`KbCandidate`] from one LLM-returned candidate JSON object,
/// scoped to the novel `work_profile`.
///
/// Convenience wrapper around [`candidate_from_llm_json_for_profile`] with
/// `work_profile = "novel"`. Kept for backward compatibility with existing
/// novel-only callers that don't carry a work-profile parameter, and for
/// `#[cfg(test)]` usage validating the novel path unchanged.
///
/// `pub(crate)`: reused by `tasks::LlmExtractTask` so there is a single
/// LLM→KbCandidate mapping across the review-time hook and the task.
// V1.55 P2 fix-wave (F-001): production callers now use
// candidate_from_llm_json_for_profile directly; this wrapper remains for
// tests and backward compatibility. lib dead_code warning suppressed.
#[allow(dead_code)]
pub(crate) fn candidate_from_llm_json(c: &serde_json::Value) -> Option<KbCandidate> {
    candidate_from_llm_json_for_profile(c, "novel")
}

/// Build a [`KbCandidate`] from one LLM-returned candidate JSON object,
/// producing a `proposed_payload` shaped for the given `work_profile`.
///
/// Returns `None` when `canonical_name` is missing/empty (no point persisting
/// a nameless candidate).
///
/// ## Profile-specific payload shape
///
/// | `work_profile` | category attribute | tags | `ValidationMode` |
/// |---|---|---|---|
/// | `"novel"` | `attributes.novel_category` via [`block_type_to_novel_category`] | `["novel", "llm-extracted"]` | `Novel` |
/// | `"game_bible"` | `attributes.game_bible_category` via [`block_type_to_game_bible_category`] | `["game-bible", "llm-extracted"]` | `GameBible` |
/// | `"script"` | `attributes.script_category` via [`block_type_to_script_category`] | `["script", "llm-extracted"]` | `Script` |
/// | other / unknown | `attributes.novel_category` (novel default) | `["novel", "llm-extracted"]` | `Novel` |
///
/// The payload also carries the four LLK keys (`block_type`, `canonical_name`,
/// `source_quote`, `confidence`) so the adopt CLI can read them from either the
/// dedicated columns or the JSON (llm-extract.md §3.1).
///
/// V1.55 P2 fix-wave (F-001): extracted from the body of the former
/// `candidate_from_llm_json` so game-bible extraction paths can call this
/// with `work_profile = "game_bible"`.
pub(crate) fn candidate_from_llm_json_for_profile(
    c: &serde_json::Value,
    work_profile: &str,
) -> Option<KbCandidate> {
    let canonical_name = c
        .get("canonical_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if canonical_name.is_empty() {
        return None;
    }
    let block_type = c
        .get("block_type")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_BLOCK_TYPE_GUESS)
        .to_string();
    let summary = c.get("summary").and_then(|v| v.as_str()).map(String::from);
    let confidence = c
        .get("confidence")
        .and_then(serde_json::Value::as_f64)
        .map(|x| x.clamp(0.0, 1.0));
    let source_quote = c
        .get("source_quote")
        .and_then(|v| v.as_str())
        .map(String::from);

    let (category_key, category_value, tags) = if work_profile == "game_bible" {
        (
            "game_bible_category",
            block_type_to_game_bible_category(&block_type),
            vec!["game-bible", "llm-extracted"],
        )
    } else if work_profile == "script" {
        (
            "script_category",
            block_type_to_script_category(&block_type),
            vec!["script", "llm-extracted"],
        )
    } else {
        (
            "novel_category",
            block_type_to_novel_category(&block_type),
            vec!["novel", "llm-extracted"],
        )
    };

    let mut payload = serde_json::json!({
        "summary": summary.clone().unwrap_or_else(|| format!("LLM-extracted entity: {canonical_name}")),
        "attributes": {
            category_key: category_value,
            "aliases": [canonical_name.as_str()],
        },
        "tags": tags,
        "block_type": block_type,
        "canonical_name": canonical_name,
        "source_quote": source_quote.clone().unwrap_or_default(),
        "confidence": confidence.unwrap_or(0.0),
    });
    // If the LLM gave no explicit summary, drop the placeholder so the
    // KeyBlockBody summary is None (cleaner adopt surface).
    if summary.is_none() {
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("summary");
        }
    }

    Some(KbCandidate {
        canonical_name_guess: canonical_name,
        proposed_payload: payload.to_string(),
        block_type,
        confidence,
        source_quote,
    })
}

/// Map a wire `block_type` (`snake_case`) to the novel-profile `novel_category`
/// body attribute (entity-scope-model §5.1.1 recommended default mapping).
///
/// Used when constructing the `proposed_payload` for LLM-extracted candidates
/// so adopt-time `ValidationMode::Novel` validates. Unknown `block_types`
/// (e.g. `ability`) default to `foundation` — the most generic category — and
/// emit no warning here (the V1.40 validator emits the advisory mismatch
/// warning on adopt).
fn block_type_to_novel_category(block_type: &str) -> &'static str {
    match block_type {
        "character" => "character",
        "scene" => "location",
        "organization" => "society",
        "item" => "economy",
        "conflict" => "rules",
        "event" => "background",
        // `info_point`, `ability`, and any unknown value → foundation (generic;
        // the V1.40 validator emits the advisory mismatch warning on adopt).
        _ => "foundation",
    }
}

/// Map a wire `block_type` (`snake_case`) to the game-bible `game_bible_category`
/// body attribute (game-bible-profile.md §7.2 mapping).
///
/// V1.55 P2: used when constructing the `proposed_payload` for game-bible
/// KB extraction so adopt-time `ValidationMode::GameBible` validates.
/// The seven valid categories are: `species`, `faction`, `magic_system`,
/// `technology`, `deity`, `level`, `economy_tier`. Existing cross-domain types
/// map to the closest game-bible category per §7.2 table.
///
/// Unknown `block_types` default to `species` — the most generic game-bible
/// category — and emit a `tracing::debug!` so operators can see unclassified
/// candidates.
// Direct-mapping arms and cross-domain fallback arms may produce the same
// string value, but the semantics differ (identity mapping vs. best-guess).
#[allow(clippy::match_same_arms)]
#[must_use]
pub fn block_type_to_game_bible_category(block_type: &str) -> &'static str {
    match block_type {
        // V1.54 new game-bible BlockTypes: direct mapping.
        "species" => "species",
        "faction" => "faction",
        "magic_system" => "magic_system",
        "technology" => "technology",
        "deity" => "deity",
        "level" => "level",
        "economy_tier" => "economy_tier",
        // Cross-domain reuse: existing BlockType → closest game_bible_category.
        "character" => "species", // Characters → species (biological/cultural)
        "ability" => "magic_system", // Abilities → magic/superpower system
        "scene" => "level",       // Scenes → levels
        "organization" | "conflict" => "faction", // Organizations/Conflicts → faction dynamics
        "item" => "technology",   // Items → technology/artifacts
        "info_point" | "event" => "deity", // Info points/Events → deity (lore)
        _ => {
            tracing::debug!(
                block_type,
                "block_type_to_game_bible_category: unknown block_type; defaulting to species"
            );
            "species"
        }
    }
}

/// Map a wire `block_type` (`snake_case`) to the script-profile `script_category`
/// body attribute (script-profile.md §7.2 mapping).
///
/// V1.60 P1: used when constructing the `proposed_payload` for script
/// KB extraction so adopt-time `ValidationMode::Script` validates.
/// The three valid categories are: `dialogue`, `beat`, `act`. Existing
/// cross-domain types map to the closest script category per §7.2 table.
///
/// Unknown `block_types` default to `dialogue` — the most generic script
/// category — and emit a `tracing::debug!` so operators can see unclassified
/// candidates.
// Direct-mapping arms and cross-domain fallback arms may produce the same
// string value, but the semantics differ (identity mapping vs. best-guess).
#[allow(clippy::match_same_arms)]
#[must_use]
pub fn block_type_to_script_category(block_type: &str) -> &'static str {
    match block_type {
        // V1.55 P3 new script BlockTypes: direct mapping.
        "dialogue" => "dialogue",
        "beat" => "beat",
        "act" => "act",
        // Cross-domain reuse: existing BlockType → closest script_category.
        "character" => "dialogue",  // Characters express through dialogue
        "scene" => "act",           // Scenes belong to acts
        "event" => "beat",          // Events are beats in narrative
        "organization" => "act",    // Organizations anchor acts
        "conflict" => "beat",       // Conflict is beat-level tension
        "info_point" => "dialogue", // Info conveyed through dialogue
        "ability" => "dialogue",    // Abilities expressed in dialogue
        "item" => "beat",           // Items are beat-level props
        _ => {
            tracing::debug!(
                block_type,
                "block_type_to_script_category: unknown block_type; defaulting to dialogue"
            );
            "dialogue"
        }
    }
}

/// Loaded chapter context: schedule → work → chapter prose.
///
/// Returned by [`load_review_context`] and [`load_finalize_context`] when all
/// preconditions are met (preset matches, work has a world, chapter body is
/// readable). `None` for any no-op early return (logged at `debug`/`warn`).
struct ChapterContext {
    creator_id: String,
    work_id: String,
    world_id: String,
    chapter: i32,
    workspace_id: String,
    /// Human-readable Work slug for on-disk paths (`work_ref`, falling back to
    /// `story_ref`). `None` in hermetic DB-only tests where no workspace exists.
    work_ref: Option<String>,
    /// Work profile (e.g. `"novel"`, `"game_bible"`) for profile-aware
    /// KB extraction payloads. Defaults to `"novel"` when the Work row carries
    /// `work_profile = NULL` (backward compatibility with pre-V1.36 Works).
    work_profile: String,
    prose: String,
}

/// Resolve the schedule → work → chapter body for the review-time extraction hook.
///
/// Returns `Ok(None)` for every no-op early return (non-`review-master`
/// preset, NULL `work_id`, no `workspace_dir`, missing work/world/chapter/body).
/// All no-op branches emit a `tracing` event so operators can see the skip.
async fn load_review_context(
    pool: &SqlitePool,
    schedule_id: &str,
    workspace_dir: Option<&std::path::Path>,
) -> Result<Option<ChapterContext>, AutoChainError> {
    load_context_for_preset(
        pool,
        schedule_id,
        workspace_dir,
        crate::preset_ids::NOVEL_REVIEW_MASTER_PRESET_ID,
        "kb-extract",
    )
    .await
}

/// Resolve the schedule → work → chapter body for the finalize-time missing-KB hook.
///
/// Returns `Ok(None)` for every no-op early return (non-`novel-writing`
/// preset, NULL `work_id`, no `workspace_dir`, missing work/world/chapter/body).
async fn load_finalize_context(
    pool: &SqlitePool,
    schedule_id: &str,
    workspace_dir: Option<&std::path::Path>,
) -> Result<Option<ChapterContext>, AutoChainError> {
    load_context_for_preset(
        pool,
        schedule_id,
        workspace_dir,
        crate::preset_ids::NOVEL_WRITING_PRESET_ID,
        "kb-missing",
    )
    .await
}

/// Shared loader for [`ChapterContext`].
///
/// `log_prefix` is used in `tracing` events so review-time and finalize-time
/// skips are distinguishable in logs.
async fn load_context_for_preset(
    pool: &SqlitePool,
    schedule_id: &str,
    workspace_dir: Option<&std::path::Path>,
    expected_preset_id: &str,
    log_prefix: &str,
) -> Result<Option<ChapterContext>, AutoChainError> {
    // SAFETY: dynamic SQL — single-row schedule lookup by PK (nullable work_id).
    let row = sqlx::query(
        "SELECT preset_id, work_id, creator_id
         FROM creator_schedules WHERE schedule_id = ?",
    )
    .bind(schedule_id)
    .fetch_optional(pool)
    .await
    .map_err(nexus_local_db::LocalDbError::from)?;

    let Some(row) = row else {
        tracing::debug!(
            schedule_id,
            "{log_prefix}: schedule row not found; skipping"
        );
        return Ok(None);
    };

    let preset_id: String = sqlx::Row::try_get(&row, "preset_id")
        .map_err(|e| AutoChainError::InvalidState(format!("decode preset_id: {e}")))?;
    let work_id: Option<String> = sqlx::Row::try_get(&row, "work_id")
        .map_err(|e| AutoChainError::InvalidState(format!("decode work_id: {e}")))?;
    let creator_id: String = sqlx::Row::try_get(&row, "creator_id")
        .map_err(|e| AutoChainError::InvalidState(format!("decode creator_id: {e}")))?;

    if preset_id != expected_preset_id {
        return Ok(None);
    }
    let Some(work_id) = work_id else {
        tracing::warn!(
            schedule_id,
            "{log_prefix}: schedule has NULL work_id; skipping"
        );
        return Ok(None);
    };
    let Some(ws_dir) = workspace_dir else {
        tracing::debug!(schedule_id, "{log_prefix}: no workspace_dir; skipping");
        return Ok(None);
    };

    let work = match nexus_local_db::works::get_work(pool, &creator_id, &work_id).await {
        Ok(Some(w)) => w,
        Ok(None) => {
            tracing::warn!(schedule_id, work_id = %work_id, "{log_prefix}: work not found; skipping");
            return Ok(None);
        }
        Err(e) => return Err(AutoChainError::from(e)),
    };
    let Some(world_id) = work.world_id.as_deref() else {
        tracing::debug!(schedule_id, work_id = %work_id, "{log_prefix}: work has no world_id; skipping");
        return Ok(None);
    };
    if work.current_chapter <= 0 {
        tracing::debug!(schedule_id, work_id = %work_id, "{log_prefix}: current_chapter <= 0; skipping");
        return Ok(None);
    }

    let workspace_id = resolve_workspace_id(pool, &creator_id).await;
    let work_ref = work.work_ref.clone().or_else(|| work.story_ref.clone());
    // V1.55 P2 fix-wave (F-001): read work_profile from the Work row so the
    // extraction hook produces profile-aware KB candidates. Defaults to "novel"
    // for Works created before V1.36 (work_profile = NULL).
    let work_profile = work
        .work_profile
        .clone()
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "novel".to_string());
    let Some(prose) =
        load_chapter_prose(pool, schedule_id, &work_id, work.current_chapter, ws_dir).await?
    else {
        return Ok(None);
    };

    Ok(Some(ChapterContext {
        creator_id,
        work_id,
        world_id: world_id.to_string(),
        chapter: work.current_chapter,
        workspace_id,
        work_ref,
        work_profile,
        prose,
    }))
}

/// Load the chapter body prose from the workspace filesystem.
///
/// Returns `Ok(None)` when the chapter row or body file is missing (logged at
/// `debug` — non-fatal skip).
async fn load_chapter_prose(
    pool: &SqlitePool,
    schedule_id: &str,
    work_id: &str,
    chapter: i32,
    ws_dir: &std::path::Path,
) -> Result<Option<String>, AutoChainError> {
    // Volume defaults to 1 for single-volume works (V1.42 backfill).
    let chapter_row =
        match nexus_local_db::work_chapters::get_chapter(pool, work_id, chapter, 1).await {
            Ok(Some(c)) => c,
            Ok(None) => {
                tracing::debug!(
                    schedule_id,
                    work_id,
                    chapter,
                    "kb-extract: chapter row missing; skipping"
                );
                return Ok(None);
            }
            Err(e) => return Err(AutoChainError::from(e)),
        };
    let Some(body_path_rel) = chapter_row.body_path.as_deref() else {
        tracing::debug!(
            schedule_id,
            work_id,
            chapter,
            "kb-extract: chapter has no body_path; skipping"
        );
        return Ok(None);
    };
    let body_path = ws_dir.join(body_path_rel);
    match std::fs::read_to_string(&body_path) {
        Ok(t) => Ok(Some(t)),
        Err(e) => {
            tracing::debug!(
                schedule_id, work_id, chapter, path = %body_path.display(), error = %e,
                "kb-extract: chapter body file unreadable; skipping"
            );
            Ok(None)
        }
    }
}

/// Filter candidates against existing names + idempotency, then insert pending.
///
/// Returns the count of newly-inserted rows. Per-candidate insert errors are
/// logged at `warn!` and do not abort the loop.
///
/// V1.51 T-A P0: writes `candidate.block_type` (LLM-judged or heuristic
/// `character`) and the LLM metadata (`llm_confidence` + `llm_source_quote`)
/// via [`insert_pending_with_llm`]. Heuristic candidates pass `None` for both
/// LLM fields so the dedicated columns stay NULL (entity-scope-model §5.5.6).
async fn persist_candidates(
    pool: &SqlitePool,
    schedule_id: &str,
    ctx: &ChapterContext,
    existing_names: &[String],
    candidates: Vec<KbCandidate>,
) -> Result<usize, AutoChainError> {
    let mut inserted = 0usize;
    for candidate in candidates {
        if existing_names
            .iter()
            .any(|n| n.eq_ignore_ascii_case(&candidate.canonical_name_guess))
        {
            continue;
        }
        if is_idempotent(pool, &ctx.work_id, &candidate.canonical_name_guess)
            .await
            .map_err(nexus_local_db::LocalDbError::from)?
        {
            continue;
        }
        // Convert for the REAL column; both are None for heuristic candidates.
        let llm_confidence = candidate.confidence;
        let llm_source_quote = candidate.source_quote.as_deref();
        match insert_pending_with_llm(
            pool,
            &ctx.creator_id,
            &ctx.workspace_id,
            &ctx.world_id,
            Some(&ctx.work_id),
            Some(i64::from(ctx.chapter)),
            &candidate.block_type,
            &candidate.canonical_name_guess,
            &candidate.proposed_payload,
            llm_confidence,
            llm_source_quote,
        )
        .await
        {
            Ok(_) => inserted += 1,
            Err(e) => {
                tracing::warn!(
                    schedule_id,
                    work_id = %ctx.work_id,
                    candidate = %candidate.canonical_name_guess,
                    error = %e,
                    "kb-extract: failed to insert pending candidate"
                );
            }
        }
    }
    Ok(inserted)
}

/// Fetch the set of active `canonical_name` values for a world.
///
/// Used to filter heuristic candidates that already exist as `KeyBlock`s
/// (avoids offering the author a duplicate they will reject).
///
/// R-V150-WLA-10 (V1.50 P-last WL-A / kb-auto-promotion qc3 S-001): errors
/// ARE propagated (the `?` below), but the caller's hook
/// (`extract_kb_candidates_for_review` → supervisor terminal) treats
/// extraction as best-effort and does **not** fail the terminal transition
/// on a returned `Err`. A flaky `kb_key_blocks` read therefore silently
/// produces zero candidates — the user sees no candidates and no error
/// surfaced at info! level. Log at `warn!` here before propagating so the
/// failure mode is visible to operators running with `RUST_LOG=warn`.
async fn existing_canonical_names(
    pool: &SqlitePool,
    world_id: &str,
) -> Result<Vec<String>, AutoChainError> {
    // SAFETY: static SELECT with bind param; reads the V1.26 kb_key_blocks.
    let rows: Result<Vec<(String,)>, sqlx::Error> = sqlx::query_as(
        "SELECT canonical_name FROM kb_key_blocks \
         WHERE world_id = ? AND status NOT IN ('deleted', 'merged', 'deprecated')",
    )
    .bind(world_id)
    .fetch_all(pool)
    .await;
    match rows {
        Ok(rows) => Ok(rows.into_iter().map(|(n,)| n).collect()),
        Err(e) => {
            tracing::warn!(
                world_id,
                error = %e,
                "kb-auto-promotion: existing_canonical_names read failed; \
                 review-time extraction will produce zero candidates for this pass \
                 (best-effort — terminal transition still completes)"
            );
            Err(nexus_local_db::LocalDbError::from(e).into())
        }
    }
}

/// Write the advisory missing-KB log for a finalized chapter.
///
/// Path: `<workspace_dir>/Works/<work_ref>/Logs/kb/missing/<YYYY-MM-DD>-ch<chapter>.md`.
/// The log uses YAML frontmatter with the candidate list and a human-readable
/// Markdown body. It overwrites any existing file for the same day/chapter so
/// repeated finalize transitions remain idempotent.
///
/// Returns the number of candidates written. Returns `Ok(0)` when there is no
/// `workspace_dir` or no resolvable `work_ref` (hermetic test path), and no log
/// is written.
fn write_missing_kb_log(
    workspace_dir: Option<&std::path::Path>,
    ctx: &ChapterContext,
    missing: &[KbCandidate],
) -> Result<usize, String> {
    if missing.is_empty() {
        return Ok(0);
    }

    let Some(ws_dir) = workspace_dir else {
        return Ok(0);
    };
    let Some(ref work_ref) = ctx.work_ref else {
        return Ok(0);
    };

    let date = chrono::Utc::now().format("%Y-%m-%d");
    let log_dir = ws_dir
        .join("Works")
        .join(work_ref)
        .join("Logs")
        .join("kb")
        .join("missing");
    std::fs::create_dir_all(&log_dir).map_err(|e| format!("create_dir_all: {e}"))?;

    let log_path = log_dir.join(format!("{date}-ch{}.md", ctx.chapter));

    let candidates: Vec<MissingLogCandidate> = missing
        .iter()
        .map(|c| MissingLogCandidate {
            canonical_name: c.canonical_name_guess.clone(),
            block_type: c.block_type.clone(),
            source_quote: c.source_quote.clone(),
            confidence: c.confidence,
        })
        .collect();

    let frontmatter = MissingLogFrontmatter {
        generated_at: chrono::Utc::now().to_rfc3339(),
        world_id: ctx.world_id.clone(),
        work_id: ctx.work_id.clone(),
        work_ref: work_ref.clone(),
        chapter: ctx.chapter,
        candidate_count: candidates.len(),
        candidates,
    };

    let yaml =
        serde_yaml::to_string(&frontmatter).map_err(|e| format!("serialize frontmatter: {e}"))?;
    let mut body = String::new();
    body.push_str("---\n");
    body.push_str(&yaml);
    body.push_str("---\n\n");
    body.push_str("# Missing KB candidates detected at finalize\n\n");
    let _ = std::fmt::Write::write_fmt(
        &mut body,
        format_args!(
            "Chapter **{}** of Work **{}** (world `{}`) was finalized at `{}`. \
             The following entities were referenced in the chapter prose but are not \
             yet present in the World KB. These are advisory signals only; they are \
             not pending candidates and cannot be adopted directly.\n\n",
            ctx.chapter, work_ref, ctx.world_id, frontmatter.generated_at
        ),
    );
    for c in missing {
        let _ = std::fmt::Write::write_fmt(
            &mut body,
            format_args!("- **{}** (`{}`)\n", c.canonical_name_guess, c.block_type),
        );
        if let Some(ref q) = c.source_quote {
            let _ = std::fmt::Write::write_fmt(&mut body, format_args!("  > Source: {q}\n"));
        }
    }

    std::fs::write(&log_path, body).map_err(|e| format!("write {}: {e}", log_path.display()))?;
    Ok(missing.len())
}

/// YAML-frontmatter candidate entry for the missing-KB log.
#[derive(Debug, serde::Serialize)]
struct MissingLogCandidate {
    canonical_name: String,
    block_type: String,
    source_quote: Option<String>,
    confidence: Option<f64>,
}

/// YAML-frontmatter header for the missing-KB log.
#[derive(Debug, serde::Serialize)]
struct MissingLogFrontmatter {
    generated_at: String,
    world_id: String,
    work_id: String,
    work_ref: String,
    chapter: i32,
    candidate_count: usize,
    candidates: Vec<MissingLogCandidate>,
}

// ═══════════════════════════════════════════════════════════════════════
// V1.52 T-A P0 — Outline 五问 quality gate
// ═══════════════════════════════════════════════════════════════════════

/// Per-dimension results of the outline 五问 heuristic check.
///
/// The five dimensions are intentionally different from the finalize 五問:
/// they evaluate the *outline* before any正文 is drafted.
// The five boolean dimensions are a natural, compact DTO for the gate result;
// a state machine would obscure the per-dimension breakdown.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FiveQDimensions {
    /// Outline has clear beat-level structure (headings, bullets, numbered beats).
    pub structure: bool,
    /// A character or situation arc is present (conflict, stakes, change).
    pub arc: bool,
    /// At least one future-setup / foreshadowing signal is present.
    pub foreshadow: bool,
    /// Outline length is within sane bounds (not empty, not a full draft).
    pub pacing: bool,
    /// Final line ends with tension, a question, or unresolved action.
    pub hook: bool,
}

/// Verdict returned by [`outline_five_q_check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiveQVerdict {
    /// `true` only when all five dimensions pass.
    pub go: bool,
    /// Per-dimension breakdown.
    pub dimensions: FiveQDimensions,
    /// Human-readable reason summarizing pass/fail dimensions.
    pub reason: String,
}

/// Build the [`FiveQVerdict`] from already-computed dimensions.
///
/// R-V152TA-S006: centralizes the NOGO info-log so every caller of
/// [`outline_five_q_check`] gets consistent observability of the per-dimension
/// scores without duplicating the logging logic.
fn build_five_q_verdict(dimensions: FiveQDimensions) -> FiveQVerdict {
    let FiveQDimensions {
        structure,
        arc,
        foreshadow,
        pacing,
        hook,
    } = dimensions;
    let go = structure && arc && foreshadow && pacing && hook;

    let failed: Vec<&str> = [
        (!structure, "structure"),
        (!arc, "arc"),
        (!foreshadow, "foreshadow"),
        (!pacing, "pacing"),
        (!hook, "hook"),
    ]
    .into_iter()
    .filter(|(miss, _)| *miss)
    .map(|(_, name)| name)
    .collect();

    let reason = if go {
        "outline 五问: all dimensions pass (structure, arc, foreshadow, pacing, hook)".to_string()
    } else {
        // R-V152TA-S006: info-log NOGO with per-dimension scores so operators
        // can see why the heuristic gate blocked draft generation.
        tracing::info!(
            structure = structure,
            arc = arc,
            foreshadow = foreshadow,
            pacing = pacing,
            hook = hook,
            failed = ?failed,
            "outline 五问: NOGO"
        );
        format!("outline 五问: failed on {}", failed.join(", "))
    };

    FiveQVerdict {
        go,
        dimensions,
        reason,
    }
}

/// Pure heuristic evaluation of a chapter outline against the outline 五问 gate.
///
/// This is the deterministic / no-worker complement to the `llm_judge` outline
/// gate in the `novel-writing` preset. It returns GO only when all five
/// dimensions pass, plus a per-dimension breakdown and a short reason.
///
/// The heuristic is intentionally conservative: a sparse or generic outline
/// will fail one or more dimensions, blocking draft generation until the
/// author revises or overrides.
#[must_use]
pub fn outline_five_q_check(outline: &str) -> FiveQVerdict {
    let trimmed = outline.trim();
    let non_empty_lines: Vec<&str> = trimmed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let lower = trimmed.to_ascii_lowercase();

    // 1. structure: at least 3 non-empty lines and visible beats/sections.
    let has_headings = trimmed.contains("##") || trimmed.contains("# ");
    let has_bullets = trimmed.contains("- ") || trimmed.contains("* ") || trimmed.contains("1. ");
    let structure = non_empty_lines.len() >= 3 && (has_headings || has_bullets);

    // 2. arc: conflict / stakes / change language.
    let arc_signals = [
        "conflict",
        "stakes",
        "revelation",
        "discovers",
        "realizes",
        "must",
        "change",
        "turn",
        "arc",
        "confronts",
        "decides",
        "because",
        "wants",
        "needs",
        "fear",
        "risk",
    ];
    let arc = arc_signals.iter().any(|s| lower.contains(s));

    // 3. foreshadow: explicit F### or future-setup language.
    let fore_signals = [
        "f###",
        "foreshadow",
        "setup",
        "plant",
        "later",
        "will return",
        "comes back",
        "looming",
        "seed",
        "promise",
    ];
    let foreshadow = fore_signals.iter().any(|s| lower.contains(s));

    // 4. pacing: not empty and not a full draft dumped into the outline field.
    let char_count = trimmed.chars().count();
    let pacing = (80..=2000).contains(&char_count);

    // 5. hook: final non-empty line ends with punctuation or word signals that
    //    leave the reader hanging.
    let hook_signals = [
        "cliffhanger",
        "hook",
        "tension",
        "unresolved",
        "mystery",
        "what happens",
        "to be continued",
        "hangs",
        "unknown",
    ];
    let last_line = non_empty_lines.last().copied().unwrap_or("").trim();
    let hook_punctuation = last_line.ends_with('?')
        || last_line.ends_with('!')
        || last_line.ends_with("...")
        || last_line.ends_with("—");
    let hook_words = hook_signals.iter().any(|s| lower.contains(s));
    let hook = hook_punctuation || hook_words;

    build_five_q_verdict(FiveQDimensions {
        structure,
        arc,
        foreshadow,
        pacing,
        hook,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// V1.55 P2 — Game-bible design 五问 quality rubric
// ═══════════════════════════════════════════════════════════════════════

/// Per-dimension results of the game-bible design 五问 rubric.
///
/// Unlike the novel outline/finalize gates, this rubric evaluates
/// **design documents** — not prose chapters. The five dimensions are:
///
/// 1. **Pillars**: every design claim traces back to a stated pillar or constraint.
/// 2. **Mechanics**: gameplay mechanics are concrete, specific, and testable.
/// 3. **Continuity**: the section is internally consistent with other Design sections.
/// 4. **Playability**: a reader can visualize how a player would experience it.
/// 5. **Clarity**: the section avoids placeholder language and stubs.
// The five boolean dimensions are a natural DTO for the gate result.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DesignFiveQDimensions {
    pub pillars: bool,
    pub mechanics: bool,
    pub continuity: bool,
    pub playability: bool,
    pub clarity: bool,
}

/// Verdict returned by [`design_five_q_check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignFiveQVerdict {
    pub go: bool,
    pub dimensions: DesignFiveQDimensions,
    pub reason: String,
}

/// Pure heuristic evaluation of a game-bible design section against the
/// design 五问 rubric (V1.55 P2).
///
/// Deterministic / no-worker complement to the `llm_judge` gate for the
/// `design-writing` preset. The rubric is intentionally stricter than the
/// novel outline 五問 because design documents serve as **reference artifacts**
/// for a whole game.
// Five-dimension check naturally exceeds default line-count ceiling; the
// function is a single concept (quality gate), not a god function.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn design_five_q_check(section_body: &str) -> DesignFiveQVerdict {
    let trimmed = section_body.trim();
    let lower = trimmed.to_ascii_lowercase();
    let non_empty_lines: Vec<&str> = trimmed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();

    // 1. pillars: section references a design pillar or constraint.
    let pillar_signals = [
        "pillar",
        "constraint",
        "principle",
        "non-goal",
        "because",
        "therefore",
        "rule",
        "must",
        "must not",
        "should",
        "should not",
    ];
    let pillars = pillar_signals.iter().any(|s| lower.contains(s)) || non_empty_lines.len() >= 5;

    // 2. mechanics: concrete gameplay mechanics.
    let mechanics_signals = [
        "damage", "health", "mana", "currency", "resource", "costs", "requires", "grants", "loop",
        "feedback", "cycle", "phase", "turn", "per", "level", "unlock", "skill", "craft", "trade",
        "build", "shoot", "jump",
    ];
    let mechanics =
        mechanics_signals.iter().any(|s| lower.contains(s)) || non_empty_lines.len() >= 8;

    // 3. continuity: cross-references and relational language.
    let continuity_signals = [
        "see also",
        "above",
        "below",
        "relates to",
        "depends on",
        "conflicts with",
        "consistent with",
        "aligns with",
        "because",
        "therefore",
        "however",
    ];
    let continuity = continuity_signals.iter().any(|s| lower.contains(s))
        || char_count_range(trimmed, 200, 10_000);

    // 4. playability: player experience signals.
    let playability_signals = [
        "player",
        "players",
        "experience",
        "feel",
        "feels",
        "imagine",
        "encounter",
        "discover",
        "explore",
        "moment",
        "puzzle",
        "challenge",
        "reward",
        "fun",
        "engaging",
        "immersive",
    ];
    let playability =
        playability_signals.iter().any(|s| lower.contains(s)) || non_empty_lines.len() >= 5;

    // 5. clarity: no TBD/placeholder, not a stub.
    let placeholder_signals = [
        "tbd",
        "todo",
        "to be determined",
        "placeholder",
        "to be decided",
        "tk",
        "???",
    ];
    let has_placeholders = placeholder_signals.iter().any(|s| lower.contains(s));
    let is_stub = char_count_range(trimmed, 0, 80);
    let clarity = !has_placeholders && !is_stub;

    let dimensions = DesignFiveQDimensions {
        pillars,
        mechanics,
        continuity,
        playability,
        clarity,
    };
    let go = pillars && mechanics && continuity && playability && clarity;
    let reason = if go {
        "design 五问: all dimensions pass (pillars, mechanics, continuity, playability, clarity)"
            .to_string()
    } else {
        let mut failed = Vec::new();
        if !pillars {
            failed.push("pillars");
        }
        if !mechanics {
            failed.push("mechanics");
        }
        if !continuity {
            failed.push("continuity");
        }
        if !playability {
            failed.push("playability");
        }
        if !clarity {
            failed.push("clarity");
        }
        format!("design 五问: failed on {}", failed.join(", "))
    };
    DesignFiveQVerdict {
        go,
        dimensions,
        reason,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// V1.63 P2 — Essay 4-dimension quality rubric
// ═══════════════════════════════════════════════════════════════════════

/// Per-dimension results of the essay 4-dimension quality rubric.
///
/// The four dimensions evaluate the essay as a complete artifact:
///
/// 1. **Thesis clarity**: the central argument is specific, arguable, and
///    prominently stated early in the essay.
/// 2. **Evidence support**: every major claim is backed by specific, credible
///    evidence — named sources, data, or concrete examples.
/// 3. **Coherence**: the essay flows logically from introduction to conclusion
///    with clear topic sentences, smooth transitions, and integrated counterargument.
/// 4. **Ending takeaway**: the conclusion delivers a clear, memorable insight
///    beyond mere summary — a "so what?" that lingers.
// The four boolean dimensions are a natural DTO for the gate result.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EssayFourDimDimensions {
    pub thesis_clarity: bool,
    pub evidence_support: bool,
    pub coherence: bool,
    pub ending_takeaway: bool,
}

/// Verdict returned by [`essay_four_dim_check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EssayFourDimVerdict {
    pub go: bool,
    pub dimensions: EssayFourDimDimensions,
    pub reason: String,
}

/// Pure heuristic evaluation of an essay against the 4-dimension quality
/// rubric (V1.63 P2).
///
/// Deterministic / no-worker complement to the `llm_judge` gate for the
/// `essay-writing` preset. All four dimensions must pass for GO. The
/// rubric is intentionally stricter than a simple length check because
/// essays serve as standalone persuasive artifacts.
// Four-dimension check is a single concept (quality gate), not a god function.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn essay_four_dim_check(essay_body: &str) -> EssayFourDimVerdict {
    let trimmed = essay_body.trim();
    let lower = trimmed.to_ascii_lowercase();
    let non_empty_lines: Vec<&str> = trimmed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();

    // 1. thesis_clarity: identifiable thesis statement with argumentative language.
    let thesis_signals = [
        "argue",
        "thesis",
        "claim",
        "contend",
        "position",
        "this essay",
        "i argue",
        "i contend",
        "my argument",
        "central claim",
        "main argument",
    ];
    let has_thesis_language = thesis_signals.iter().any(|s| lower.contains(s));
    // Also check: essay has enough substance (not a stub) and contains declarative
    // language suggesting a specific argument.
    let has_argument_structure = lower.contains("because")
        || lower.contains("therefore")
        || lower.contains("thus")
        || lower.contains("consequently")
        || lower.contains("however")
        || lower.contains("first")
        || lower.contains("second");
    let thesis_clarity =
        has_thesis_language && has_argument_structure && non_empty_lines.len() >= 5;

    // 2. evidence_support: specific evidence markers vs vague authority.
    let evidence_signals = [
        "according to",
        "study by",
        "research from",
        "data from",
        "survey",
        "percent",
        "%",
        "for example",
        "for instance",
        "specifically",
        "in particular",
        "cites",
        "reports that",
        "found that",
        "demonstrates",
        "shows that",
        "published in",
        "case study",
        "experiment",
        "statistic",
    ];
    let has_specific_evidence = evidence_signals.iter().any(|s| lower.contains(s));
    // Vague authority patterns that should lower confidence.
    let vague_signals = [
        "studies show",
        "research shows",
        "experts say",
        "many people",
        "it is known",
        "everyone knows",
        "obviously",
    ];
    let vague_count = vague_signals.iter().filter(|s| lower.contains(*s)).count();
    // Evidence passes if specific markers present AND not relying solely on vague authority.
    let evidence_support =
        has_specific_evidence && (vague_count < 3 || char_count_range(trimmed, 500, 50_000));

    // 3. coherence: logical flow markers and paragraph structure.
    let coherence_signals = [
        "first",
        "second",
        "third",
        "finally",
        "moreover",
        "furthermore",
        "in addition",
        "on the other hand",
        "conversely",
        "in contrast",
        "similarly",
        "likewise",
        "for this reason",
        "as a result",
        "consequently",
        "therefore",
        "thus",
        "however",
    ];
    let coherence_signal_count = coherence_signals
        .iter()
        .filter(|s| lower.contains(*s))
        .count();
    // Also check: the essay has paragraphs (multi-line structure).
    let has_paragraphs = non_empty_lines.len() >= 6;
    // Counterargument language: essay engages with opposing views.
    let counterarg_signals = [
        "objection",
        "counterargument",
        "opposing",
        "critics",
        "some may argue",
        "one might",
        "admittedly",
        "granted",
        "to be fair",
        "while it is true",
        "on the other hand",
        "conversely",
        "some argue",
    ];
    let has_counterarg = counterarg_signals.iter().any(|s| lower.contains(s));
    let coherence = coherence_signal_count >= 3 && has_paragraphs && has_counterarg;

    // 4. ending_takeaway: conclusion delivers insight beyond summary.
    let takeaway_signals = [
        "conclusion",
        "in summary",
        "to conclude",
        "ultimately",
        "in the end",
        "the takeaway",
        "what this means",
        "the implication",
        "this suggests",
        "looking ahead",
        "moving forward",
        "call to action",
        "we must",
        "we should",
        "it is time",
        "so what",
        "why this matters",
        "the point is",
    ];
    let has_takeaway_language = takeaway_signals.iter().any(|s| lower.contains(s));
    // Also check: the last non-empty lines contain forward-looking or
    // insight-oriented language, not just mechanical restatement.
    let last_few_lines: String = non_empty_lines
        .iter()
        .rev()
        .take(5)
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    let last_lines_lower = last_few_lines.to_ascii_lowercase();
    let ending_has_insight = last_lines_lower.contains("therefore")
        || last_lines_lower.contains("thus")
        || last_lines_lower.contains("means")
        || last_lines_lower.contains("suggests")
        || last_lines_lower.contains("must")
        || last_lines_lower.contains("should")
        || last_lines_lower.contains("matters");
    let ending_takeaway = has_takeaway_language && ending_has_insight;

    let dimensions = EssayFourDimDimensions {
        thesis_clarity,
        evidence_support,
        coherence,
        ending_takeaway,
    };
    let go = thesis_clarity && evidence_support && coherence && ending_takeaway;
    let reason = if go {
        "essay 4-dim: all dimensions pass (thesis clarity, evidence support, coherence, ending takeaway)".to_string()
    } else {
        let mut failed = Vec::new();
        if !thesis_clarity {
            failed.push("thesis clarity");
        }
        if !evidence_support {
            failed.push("evidence support");
        }
        if !coherence {
            failed.push("coherence");
        }
        if !ending_takeaway {
            failed.push("ending takeaway");
        }
        format!("essay 4-dim: failed on {}", failed.join(", "))
    };
    EssayFourDimVerdict {
        go,
        dimensions,
        reason,
    }
}

/// Check whether the character count of `text` falls within `[min, max]`.
fn char_count_range(text: &str, min: usize, max: usize) -> bool {
    let count = text.chars().count();
    count >= min && count <= max
}

/// Best-effort `workspace_id` resolution for the `kb_extract_jobs` row.
///
/// Falls back to the `creator_id` when no workspace is registered (the column
/// is informational; the extraction logic keys off `world_id` + `work_id`).
async fn resolve_workspace_id(pool: &SqlitePool, creator_id: &str) -> String {
    // SAFETY: static scalar lookup against narrative_gateway (workspace table).
    let ws: Option<String> = sqlx::query_scalar(
        "SELECT workspace_id FROM narrative_gateway WHERE creator_id = ? LIMIT 1",
    )
    .bind(creator_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    ws.unwrap_or_else(|| creator_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_simple_character_name() {
        let text = "Lin Xia walked into the tavern.";
        let candidates = extract_candidates_from_text(text);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].canonical_name_guess, "Lin Xia");
    }

    #[test]
    fn extracts_multi_word_names() {
        let text = "The Crimson Order met at Mount Azure.";
        // "The Crimson Order": first word "The" is filtered for multi-word.
        // "Mount Azure": kept.
        let names: Vec<_> = extract_candidates_from_text(text)
            .into_iter()
            .map(|c| c.canonical_name_guess)
            .collect();
        assert!(names.contains(&"Mount Azure".to_string()));
        assert!(
            !names.contains(&"The Crimson Order".to_string()),
            "'The Crimson Order' should be filtered (article-first): {names:?}"
        );
    }

    #[test]
    fn deduplicates_repeated_names() {
        let text = "Lin Xia spoke. Lin Xia nodded. Lin Xia left.";
        let candidates = extract_candidates_from_text(text);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].canonical_name_guess, "Lin Xia");
    }

    #[test]
    fn filters_stopwords() {
        let text = "The cat sat. When morning came, Chapter 5 began.";
        let candidates = extract_candidates_from_text(text);
        // "The", "When", "Chapter" should all be filtered.
        for c in &candidates {
            assert!(
                !matches!(c.canonical_name_guess.as_str(), "The" | "When" | "Chapter"),
                "stopword leaked: {}",
                c.canonical_name_guess
            );
        }
    }

    #[test]
    fn caps_at_max_candidates() {
        // Generate 30 distinct capitalized names.
        let names: String = (0..30)
            .map(|i| format!("Name{i} Person"))
            .collect::<Vec<_>>()
            .join(". ");
        let candidates = extract_candidates_from_text(&names);
        assert!(
            candidates.len() <= MAX_CANDIDATES_PER_PASS,
            "expected <= {} candidates, got {}",
            MAX_CANDIDATES_PER_PASS,
            candidates.len()
        );
    }

    #[test]
    fn proposed_payload_is_valid_json_with_novel_category() {
        let text = "Aria Stormblade appeared.";
        let candidates = extract_candidates_from_text(text);
        assert_eq!(candidates.len(), 1);
        let payload: serde_json::Value =
            serde_json::from_str(&candidates[0].proposed_payload).unwrap();
        assert_eq!(payload["attributes"]["novel_category"], "character");
        assert_eq!(payload["attributes"]["aliases"][0], "Aria Stormblade");
    }

    // ── V1.51 T-A P0: heuristic candidates carry the new fields as defaults ──

    #[test]
    fn heuristic_candidates_default_block_type_and_null_llm_fields() {
        let candidates = extract_candidates_from_text("Lin Xia walked.");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].block_type, "character");
        assert_eq!(candidates[0].confidence, None);
        assert_eq!(candidates[0].source_quote, None);
    }

    // ── V1.51 T-A P0: block_type -> novel_category mapping ───────────────────

    #[test]
    fn block_type_mapping_covers_known_types() {
        assert_eq!(block_type_to_novel_category("character"), "character");
        assert_eq!(block_type_to_novel_category("scene"), "location");
        assert_eq!(block_type_to_novel_category("organization"), "society");
        assert_eq!(block_type_to_novel_category("item"), "economy");
        assert_eq!(block_type_to_novel_category("conflict"), "rules");
        assert_eq!(block_type_to_novel_category("info_point"), "foundation");
        assert_eq!(block_type_to_novel_category("event"), "background");
        // Unknown / ability -> foundation (generic).
        assert_eq!(block_type_to_novel_category("ability"), "foundation");
        assert_eq!(block_type_to_novel_category("nonsense"), "foundation");
    }

    // ── V1.51 T-A P0: candidate_from_llm_json builder ─────────────────────────

    #[test]
    fn candidate_from_llm_json_builds_full_payload() {
        let c = serde_json::json!({
            "canonical_name": "Azure Gate",
            "block_type": "scene",
            "summary": "The eastern gate",
            "confidence": 0.92,
            "source_quote": "...the eastern gate groaned open...",
        });
        let built = candidate_from_llm_json(&c).expect("canonical_name present");
        assert_eq!(built.canonical_name_guess, "Azure Gate");
        assert_eq!(built.block_type, "scene");
        assert_eq!(built.confidence, Some(0.92));
        assert_eq!(
            built.source_quote.as_deref(),
            Some("...the eastern gate groaned open...")
        );

        // proposed_payload JSON carries the 4 LLM keys + derived novel_category.
        let payload: serde_json::Value = serde_json::from_str(&built.proposed_payload).unwrap();
        assert_eq!(payload["block_type"], "scene");
        assert_eq!(payload["canonical_name"], "Azure Gate");
        assert_eq!(payload["confidence"], 0.92);
        assert_eq!(
            payload["source_quote"],
            "...the eastern gate groaned open..."
        );
        assert_eq!(payload["attributes"]["novel_category"], "location");
        assert_eq!(payload["tags"][1], "llm-extracted");
    }

    #[test]
    fn candidate_from_llm_json_rejects_empty_name() {
        let c = serde_json::json!({
            "canonical_name": "   ",
            "block_type": "character",
            "confidence": 0.5,
            "source_quote": "q",
        });
        assert!(candidate_from_llm_json(&c).is_none());
    }

    #[test]
    fn candidate_from_llm_json_clamps_confidence() {
        let c = serde_json::json!({
            "canonical_name": "X",
            "block_type": "character",
            "confidence": 1.7,
            "source_quote": "q",
        });
        let built = candidate_from_llm_json(&c).unwrap();
        assert_eq!(built.confidence, Some(1.0));
    }

    #[test]
    fn candidate_from_llm_json_defaults_missing_optional_fields() {
        // Only canonical_name is required; block_type defaults to character,
        // confidence/source_quote to None.
        let c = serde_json::json!({"canonical_name": "Y"});
        let built = candidate_from_llm_json(&c).unwrap();
        assert_eq!(built.block_type, "character");
        assert_eq!(built.confidence, None);
        assert_eq!(built.source_quote, None);
    }

    // ── V1.55 P2 fix-wave (F-001): profile-aware candidate materialization ──

    #[test]
    fn candidate_from_llm_json_for_profile_game_bible_produces_game_bible_category() {
        let c = serde_json::json!({
            "canonical_name": "Eldritch Species",
            "block_type": "species",
            "summary": "An ancient species",
            "confidence": 0.88,
            "source_quote": "the Eldritch emerged from the void",
        });
        let built =
            candidate_from_llm_json_for_profile(&c, "game_bible").expect("canonical_name present");
        let payload: serde_json::Value = serde_json::from_str(&built.proposed_payload).unwrap();

        // game-bible payload MUST have game_bible_category, NOT novel_category.
        assert_eq!(
            payload["attributes"]["game_bible_category"], "species",
            "game-bible profile: species block_type → game_bible_category species"
        );
        assert!(
            payload["attributes"].get("novel_category").is_none(),
            "game-bible profile: must NOT emit novel_category"
        );
        // Tags must be game-bible scoped.
        assert_eq!(payload["tags"][0], "game-bible");
        assert_eq!(payload["tags"][1], "llm-extracted");
        // LLM keys still present.
        assert_eq!(payload["block_type"], "species");
        assert_eq!(payload["canonical_name"], "Eldritch Species");
        assert_eq!(payload["confidence"], 0.88);
        assert_eq!(
            payload["source_quote"],
            "the Eldritch emerged from the void"
        );
    }

    #[test]
    fn candidate_from_llm_json_for_profile_game_bible_cross_domain_maps_character_to_species() {
        // Cross-domain: character BlockType → species game_bible_category.
        let c = serde_json::json!({
            "canonical_name": "Hero",
            "block_type": "character",
            "confidence": 0.9,
        });
        let built =
            candidate_from_llm_json_for_profile(&c, "game_bible").expect("canonical_name present");
        let payload: serde_json::Value = serde_json::from_str(&built.proposed_payload).unwrap();
        assert_eq!(payload["attributes"]["game_bible_category"], "species");
        assert!(
            payload["attributes"].get("novel_category").is_none(),
            "game-bible profile: must NOT emit novel_category for cross-domain block_type"
        );
        assert_eq!(payload["tags"][0], "game-bible");
    }

    #[test]
    fn candidate_from_llm_json_for_profile_game_bible_cross_domain_item_to_technology() {
        let c = serde_json::json!({
            "canonical_name": "Plasma Rifle",
            "block_type": "item",
            "confidence": 0.85,
        });
        let built =
            candidate_from_llm_json_for_profile(&c, "game_bible").expect("canonical_name present");
        let payload: serde_json::Value = serde_json::from_str(&built.proposed_payload).unwrap();
        assert_eq!(payload["attributes"]["game_bible_category"], "technology");
        assert_eq!(payload["tags"][0], "game-bible");
    }

    #[test]
    fn candidate_from_llm_json_for_profile_game_bible_unknown_defaults_species() {
        // Unknown block_type defaults to species per the mapping.
        let c = serde_json::json!({
            "canonical_name": "???",
            "block_type": "goblin_king",
            "confidence": 0.5,
        });
        let built =
            candidate_from_llm_json_for_profile(&c, "game_bible").expect("canonical_name present");
        let payload: serde_json::Value = serde_json::from_str(&built.proposed_payload).unwrap();
        assert_eq!(payload["attributes"]["game_bible_category"], "species");
    }

    #[test]
    fn candidate_from_llm_json_novel_profile_still_works() {
        // Regression: the novel wrapper must produce novel_category as before.
        let c = serde_json::json!({
            "canonical_name": "Azure Gate",
            "block_type": "scene",
            "confidence": 0.92,
        });
        let built = candidate_from_llm_json(&c).expect("canonical_name present");
        let payload: serde_json::Value = serde_json::from_str(&built.proposed_payload).unwrap();
        assert_eq!(payload["attributes"]["novel_category"], "location");
        assert!(
            payload["attributes"].get("game_bible_category").is_none(),
            "novel profile: must NOT emit game_bible_category"
        );
        assert_eq!(payload["tags"][0], "novel");
        assert_eq!(payload["tags"][1], "llm-extracted");
    }

    // ── V1.60 P1: profile-aware candidate materialization for script ──

    #[test]
    fn candidate_from_llm_json_for_profile_script_produces_script_category() {
        let c = serde_json::json!({
            "canonical_name": "Alice's Confession",
            "block_type": "dialogue",
            "summary": "A pivotal character reveal",
            "confidence": 0.92,
            "source_quote": "I never wanted you to find out this way.",
        });
        let built =
            candidate_from_llm_json_for_profile(&c, "script").expect("canonical_name present");
        let payload: serde_json::Value = serde_json::from_str(&built.proposed_payload).unwrap();

        // script payload MUST have script_category, NOT novel_category or game_bible_category.
        assert_eq!(
            payload["attributes"]["script_category"], "dialogue",
            "script profile: dialogue block_type → script_category dialogue"
        );
        assert!(
            payload["attributes"].get("novel_category").is_none(),
            "script profile: must NOT emit novel_category"
        );
        assert!(
            payload["attributes"].get("game_bible_category").is_none(),
            "script profile: must NOT emit game_bible_category"
        );
        // Tags must be script scoped.
        assert_eq!(payload["tags"][0], "script");
        assert_eq!(payload["tags"][1], "llm-extracted");
        // LLM keys still present.
        assert_eq!(payload["block_type"], "dialogue");
        assert_eq!(payload["canonical_name"], "Alice's Confession");
        assert_eq!(payload["confidence"], 0.92);
        assert_eq!(
            payload["source_quote"],
            "I never wanted you to find out this way."
        );
    }

    #[test]
    fn candidate_from_llm_json_for_profile_script_cross_domain_maps_character_to_dialogue() {
        // Cross-domain: character BlockType → dialogue script_category.
        let c = serde_json::json!({
            "canonical_name": "Bob",
            "block_type": "character",
            "confidence": 0.9,
        });
        let built =
            candidate_from_llm_json_for_profile(&c, "script").expect("canonical_name present");
        let payload: serde_json::Value = serde_json::from_str(&built.proposed_payload).unwrap();
        assert_eq!(payload["attributes"]["script_category"], "dialogue");
        assert!(
            payload["attributes"].get("novel_category").is_none(),
            "script profile: must NOT emit novel_category for cross-domain block_type"
        );
        assert_eq!(payload["tags"][0], "script");
    }

    #[test]
    fn candidate_from_llm_json_for_profile_script_cross_domain_event_to_beat() {
        // Cross-domain: event BlockType → beat script_category.
        let c = serde_json::json!({
            "canonical_name": "The Explosion",
            "block_type": "event",
            "confidence": 0.95,
        });
        let built =
            candidate_from_llm_json_for_profile(&c, "script").expect("canonical_name present");
        let payload: serde_json::Value = serde_json::from_str(&built.proposed_payload).unwrap();
        assert_eq!(payload["attributes"]["script_category"], "beat");
        assert_eq!(payload["tags"][0], "script");
    }

    #[test]
    fn candidate_from_llm_json_for_profile_script_unknown_defaults_dialogue() {
        // Unknown block_type defaults to dialogue per the mapping.
        let c = serde_json::json!({
            "canonical_name": "???",
            "block_type": "unknown_thing",
            "confidence": 0.5,
        });
        let built =
            candidate_from_llm_json_for_profile(&c, "script").expect("canonical_name present");
        let payload: serde_json::Value = serde_json::from_str(&built.proposed_payload).unwrap();
        assert_eq!(payload["attributes"]["script_category"], "dialogue");
    }

    #[test]
    fn block_type_to_script_category_direct_mappings() {
        assert_eq!(block_type_to_script_category("dialogue"), "dialogue");
        assert_eq!(block_type_to_script_category("beat"), "beat");
        assert_eq!(block_type_to_script_category("act"), "act");
    }

    #[test]
    fn block_type_to_script_category_cross_domain_mappings() {
        assert_eq!(
            block_type_to_script_category("character"),
            "dialogue",
            "character → dialogue"
        );
        assert_eq!(block_type_to_script_category("scene"), "act", "scene → act");
        assert_eq!(
            block_type_to_script_category("event"),
            "beat",
            "event → beat"
        );
        assert_eq!(
            block_type_to_script_category("organization"),
            "act",
            "organization → act"
        );
        assert_eq!(
            block_type_to_script_category("conflict"),
            "beat",
            "conflict → beat"
        );
        assert_eq!(
            block_type_to_script_category("info_point"),
            "dialogue",
            "info_point → dialogue"
        );
    }

    #[test]
    fn block_type_to_script_category_unknown_defaults_dialogue() {
        assert_eq!(
            block_type_to_script_category("goblin_king"),
            "dialogue",
            "unknown → dialogue (default)"
        );
    }

    // ── V1.51 T-A P1: cross-chapter aggregation ───────────────────────────

    /// Helper: a heuristic candidate for `name` (the shape
    /// `extract_candidates_from_text` produces).
    fn heuristic_candidate(name: &str) -> KbCandidate {
        KbCandidate {
            canonical_name_guess: name.to_string(),
            proposed_payload: serde_json::json!({
                "summary": format!("Candidate extracted from chapter prose: {name}"),
                "attributes": {"novel_category": "character", "aliases": [name]},
                "tags": ["novel", "heuristic-extracted"],
            })
            .to_string(),
            block_type: DEFAULT_BLOCK_TYPE_GUESS.to_string(),
            confidence: None,
            source_quote: None,
        }
    }

    #[test]
    fn aggregate_collapses_same_name_across_chapters() {
        // Chapters 3, 5, 7 each reference "Aelin".
        let per_chapter: Vec<(i32, Vec<KbCandidate>)> = vec![
            (3, vec![heuristic_candidate("Aelin")]),
            (5, vec![heuristic_candidate("Aelin")]),
            (7, vec![heuristic_candidate("Aelin")]),
        ];
        let aggregates = aggregate_candidates_by_canonical_name(&per_chapter);
        assert_eq!(
            aggregates.len(),
            1,
            "same canonical_name across chapters must collapse to one aggregate"
        );
        assert_eq!(aggregates[0].canonical_name, "Aelin");
        assert_eq!(aggregates[0].source_chapters, vec![3, 5, 7]);
    }

    #[test]
    fn aggregate_keeps_distinct_names_separate() {
        let per_chapter: Vec<(i32, Vec<KbCandidate>)> = vec![
            (1, vec![heuristic_candidate("Aelin")]),
            (2, vec![heuristic_candidate("Bran")]),
        ];
        let aggregates = aggregate_candidates_by_canonical_name(&per_chapter);
        assert_eq!(aggregates.len(), 2);
        let names: Vec<&str> = aggregates
            .iter()
            .map(|a| a.canonical_name.as_str())
            .collect();
        assert!(names.contains(&"Aelin"));
        assert!(names.contains(&"Bran"));
    }

    #[test]
    fn aggregate_is_case_insensitive_and_preserves_first_seen_case() {
        // "Aelin" in ch1, "aelin" in ch2 (different case, same entity).
        let per_chapter: Vec<(i32, Vec<KbCandidate>)> = vec![
            (1, vec![heuristic_candidate("Aelin")]),
            (2, vec![heuristic_candidate("aelin")]),
        ];
        let aggregates = aggregate_candidates_by_canonical_name(&per_chapter);
        assert_eq!(aggregates.len(), 1, "case-insensitive match must collapse");
        assert_eq!(
            aggregates[0].canonical_name, "Aelin",
            "first-seen case is preserved"
        );
        assert_eq!(aggregates[0].source_chapters, vec![1, 2]);
    }

    #[test]
    fn aggregate_dedupes_chapter_list() {
        // Chapter 1 mentions "Aelin" twice (defensive — the heuristic dedupes
        // already, but aggregation must be robust to duplicates) + chapter 2
        // mentions it once.
        let per_chapter: Vec<(i32, Vec<KbCandidate>)> = vec![
            (
                1,
                vec![heuristic_candidate("Aelin"), heuristic_candidate("Aelin")],
            ),
            (2, vec![heuristic_candidate("Aelin")]),
        ];
        let aggregates = aggregate_candidates_by_canonical_name(&per_chapter);
        assert_eq!(aggregates.len(), 1);
        assert_eq!(aggregates[0].source_chapters, vec![1, 2]);
    }

    #[test]
    fn aggregate_records_source_chapters_in_payload() {
        let per_chapter: Vec<(i32, Vec<KbCandidate>)> = vec![
            (3, vec![heuristic_candidate("Aelin")]),
            (5, vec![heuristic_candidate("Aelin")]),
            (7, vec![heuristic_candidate("Aelin")]),
        ];
        let aggregates = aggregate_candidates_by_canonical_name(&per_chapter);
        let payload: serde_json::Value =
            serde_json::from_str(&aggregates[0].proposed_payload).unwrap();
        assert_eq!(
            payload["source_chapters"],
            serde_json::json!([3, 5, 7]),
            "merged payload must carry the cross-chapter provenance array"
        );
    }

    #[test]
    fn aggregate_empty_input_returns_empty() {
        let per_chapter: Vec<(i32, Vec<KbCandidate>)> = vec![];
        let aggregates = aggregate_candidates_by_canonical_name(&per_chapter);
        assert!(aggregates.is_empty());
    }

    #[test]
    fn aggregate_skips_chapters_with_no_candidates() {
        let per_chapter: Vec<(i32, Vec<KbCandidate>)> = vec![
            (1, vec![heuristic_candidate("Aelin")]),
            (2, vec![]), // chapter 2 has no candidates
            (3, vec![heuristic_candidate("Aelin")]),
        ];
        let aggregates = aggregate_candidates_by_canonical_name(&per_chapter);
        assert_eq!(aggregates.len(), 1);
        assert_eq!(aggregates[0].source_chapters, vec![1, 3]);
    }

    #[test]
    fn aggregate_preserves_llm_metadata_when_present() {
        // An LLM-pathway candidate (confidence + source_quote) aggregating with
        // a heuristic candidate: the LLM metadata is carried forward.
        let llm = KbCandidate {
            canonical_name_guess: "Azure Gate".to_string(),
            proposed_payload: serde_json::json!({
                "summary": "x", "attributes": {"novel_category": "scene"},
                "tags": ["novel", "llm-extracted"],
            })
            .to_string(),
            block_type: "scene".to_string(),
            confidence: Some(0.92),
            source_quote: Some("...the eastern gate groaned open...".to_string()),
        };
        let per_chapter: Vec<(i32, Vec<KbCandidate>)> =
            vec![(1, vec![llm]), (2, vec![heuristic_candidate("Azure Gate")])];
        let aggregates = aggregate_candidates_by_canonical_name(&per_chapter);
        assert_eq!(aggregates.len(), 1);
        assert_eq!(aggregates[0].confidence, Some(0.92));
        assert_eq!(
            aggregates[0].source_quote.as_deref(),
            Some("...the eastern gate groaned open...")
        );
        assert_eq!(aggregates[0].block_type, "scene");
    }

    // ── V1.52 T-A P0: outline 五问 heuristic gate ───────────────────────

    #[test]
    fn outline_five_q_passes_on_complete_outline() {
        let outline = "## Opening\n- Lin Xia enters the tavern and confronts the stranger.\n- She discovers the stranger carries her brother's blade.\n## Middle\n- Stakes rise: the stranger knows where the blade was found.\n- F001: he plants a seed about the eastern gate.\n## End\n- Lin Xia must decide: trust him or draw her sword?\n";
        let verdict = outline_five_q_check(outline);
        assert!(verdict.go, "expected GO, got: {verdict:?}");
        assert!(verdict.dimensions.structure);
        assert!(verdict.dimensions.arc);
        assert!(verdict.dimensions.foreshadow);
        assert!(verdict.dimensions.pacing);
        assert!(verdict.dimensions.hook);
        assert!(verdict.reason.contains("all dimensions pass"));
    }

    #[test]
    fn outline_five_q_fails_on_empty_outline() {
        let verdict = outline_five_q_check("");
        assert!(!verdict.go);
        assert!(!verdict.dimensions.structure);
        assert!(!verdict.dimensions.pacing);
        assert!(verdict.reason.contains("structure"));
    }

    #[test]
    fn outline_five_q_fails_without_arc_or_hook() {
        // Structured and paced, but no conflict/change and no hook.
        let outline = "## Scene 1\n- The party walks through the forest.\n- They see many trees.\n## Scene 2\n- They stop for lunch.\n";
        let verdict = outline_five_q_check(outline);
        assert!(!verdict.go);
        assert!(verdict.dimensions.structure);
        assert!(!verdict.dimensions.arc);
        assert!(!verdict.dimensions.foreshadow);
        assert!(verdict.dimensions.pacing);
        assert!(!verdict.dimensions.hook);
        assert!(verdict.reason.contains("arc"));
        assert!(verdict.reason.contains("hook"));
    }

    #[test]
    fn outline_five_q_detects_hook_via_question() {
        // Minimal structured outline with arc and foreshadow words; hook via '?'.
        let outline = "## Beat 1\n- Kael must steal the map before dawn.\n- He plants a promise that the guard will return.\n## Beat 2\n- Will he make it out alive?";
        let verdict = outline_five_q_check(outline);
        assert!(verdict.go, "expected GO, got: {verdict:?}");
    }

    // R-V152TA-S001: regression test confirming the single shared LLM→KbCandidate
    // parser (`candidate_from_llm_json_for_profile`) is used by both extraction
    // pathways. If the two call sites ever diverge, this test still guards the
    // canonical mapping shape.
    #[test]
    fn llm_candidate_parser_shape_is_shared_across_pathways() {
        let llm_json = serde_json::json!({
            "canonical_name": "Azure Gate",
            "block_type": "scene",
            "confidence": 0.92,
            "source_quote": "...the eastern gate groaned open...",
            "summary": "The eastern gate of the citadel."
        });

        let candidate = candidate_from_llm_json_for_profile(&llm_json, "novel")
            .expect("valid LLM candidate parses");

        assert_eq!(candidate.canonical_name_guess, "Azure Gate");
        assert_eq!(candidate.block_type, "scene");
        assert_eq!(candidate.confidence, Some(0.92));
        assert_eq!(
            candidate.source_quote.as_deref(),
            Some("...the eastern gate groaned open...")
        );

        // Profile-aware payload shaping: novel uses novel_category.
        let payload: serde_json::Value =
            serde_json::from_str(&candidate.proposed_payload).expect("valid JSON payload");
        assert_eq!(payload["attributes"]["novel_category"], "location");

        // The same LLM JSON parsed for game_bible/script profiles keeps the
        // core fields identical; only the profile-specific category attribute
        // changes. This proves the parser is the shared source of truth.
        let game_bible_candidate =
            candidate_from_llm_json_for_profile(&llm_json, "game_bible").unwrap();
        assert_eq!(game_bible_candidate.canonical_name_guess, "Azure Gate");
        assert_eq!(game_bible_candidate.block_type, "scene");
        let gb_payload: serde_json::Value =
            serde_json::from_str(&game_bible_candidate.proposed_payload).unwrap();
        assert_eq!(gb_payload["attributes"]["game_bible_category"], "level");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // V1.55 P2 — Game-bible design 五问 rubric + category mapping tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn design_five_q_passes_on_good_design_section() {
        let section = "# Combat System\n\n\
            ## Core Pillars\n\
            Combat must serve the Momentum Pillar.\n\n\
            ## Mechanics\n\
            - Initiative uses deck-building: 5-card hand, draw 2 per turn.\n\
            - Damage is flat (3 base + weapon modifier). No dice.\n\
            - Stagger at 5 stacks loses next action.\n\n\
            ## Player Experience\n\
            The player feels tactical pressure: each card spent is a resource.\n\
            This creates a push-your-luck feeling.\n\n\
            ## Continuity\n\
            Aligns with technology level in Design/technology.md.\n";
        let verdict = design_five_q_check(section);
        assert!(verdict.go, "expected GO, got: {verdict:?}");
        assert!(verdict.dimensions.pillars);
        assert!(verdict.dimensions.mechanics);
        assert!(verdict.dimensions.continuity);
        assert!(verdict.dimensions.playability);
        assert!(verdict.dimensions.clarity);
    }

    #[test]
    fn design_five_q_fails_on_empty_stub() {
        let verdict = design_five_q_check("");
        assert!(!verdict.go);
        assert!(!verdict.dimensions.pillars);
        assert!(!verdict.dimensions.mechanics);
        assert!(!verdict.dimensions.clarity);
    }

    #[test]
    fn design_five_q_fails_on_tbd() {
        let section = "# Magic System\n\nTBD - will be decided later. TODO: add more.";
        let verdict = design_five_q_check(section);
        assert!(!verdict.go);
        assert!(!verdict.dimensions.clarity);
        assert!(verdict.reason.contains("clarity"));
    }

    #[test]
    fn design_five_q_is_deterministic() {
        let section = "# Economy\n\n\
            ## Currency\n- Gold pieces (GP) as primary currency.\n\
            - Players craft items from raw materials at a 20% discount.\n";
        let v1 = design_five_q_check(section);
        let v2 = design_five_q_check(section);
        assert_eq!(v1.go, v2.go);
        assert_eq!(v1.reason, v2.reason);
    }

    #[test]
    fn block_type_to_game_bible_category_direct() {
        assert_eq!(block_type_to_game_bible_category("species"), "species");
        assert_eq!(block_type_to_game_bible_category("faction"), "faction");
        assert_eq!(
            block_type_to_game_bible_category("magic_system"),
            "magic_system"
        );
        assert_eq!(
            block_type_to_game_bible_category("technology"),
            "technology"
        );
        assert_eq!(block_type_to_game_bible_category("deity"), "deity");
        assert_eq!(block_type_to_game_bible_category("level"), "level");
        assert_eq!(
            block_type_to_game_bible_category("economy_tier"),
            "economy_tier"
        );
    }

    #[test]
    fn block_type_to_game_bible_category_cross_domain() {
        assert_eq!(block_type_to_game_bible_category("character"), "species");
        assert_eq!(block_type_to_game_bible_category("organization"), "faction");
        assert_eq!(block_type_to_game_bible_category("item"), "technology");
        assert_eq!(block_type_to_game_bible_category("ability"), "magic_system");
        assert_eq!(block_type_to_game_bible_category("conflict"), "faction");
    }

    #[test]
    fn block_type_to_game_bible_category_unknown_defaults_species() {
        assert_eq!(block_type_to_game_bible_category("nonsense"), "species");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // V1.63 P2 — Essay 4-dimension rubric tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn essay_four_dim_passes_on_good_essay() {
        let essay = "# The Case for Remote Work\n\n\
            This essay argues that remote work improves productivity and \
            well-being for knowledge workers. I contend that companies should \
            adopt remote-first policies because the evidence overwhelmingly \
            supports this model.\n\n\
            First, according to a 2023 Stanford study by Bloom et al., \
            remote workers are 13% more productive than office-based \
            counterparts. For example, a two-year study of 16,000 workers \
            found that attrition dropped by 50%. Specifically, call center \
            employees at Ctrip handled 13.5% more calls from home.\n\n\
            Second, remote work reduces environmental impact. Research from \
            Global Workplace Analytics found that if everyone who could work \
            remotely did so half the time, greenhouse gas emissions would \
            drop by 54 million tons annually. For instance, commuting accounts \
            for 28% of transportation emissions in the US.\n\n\
            However, some argue that remote work reduces collaboration and \
            innovation. Admittedly, spontaneous interactions decrease in \
            remote settings. But research from Microsoft's 2022 study of \
            61,000 employees suggests that scheduled collaboration sessions \
            are more effective than serendipitous encounters. Furthermore, \
            tools like Slack and Zoom enable asynchronous deep work.\n\n\
            Therefore, remote work is a net positive for knowledge workers. \
            Ultimately, companies must embrace remote-first policies to stay \
            competitive. What this means for the future of work is that the \
            office-centric model is obsolete — and we should act accordingly.\n";
        let verdict = essay_four_dim_check(essay);
        assert!(
            verdict.go,
            "expected GO for good essay, got: {} — {:?}",
            verdict.reason, verdict.dimensions
        );
        assert!(verdict.dimensions.thesis_clarity);
        assert!(verdict.dimensions.evidence_support);
        assert!(verdict.dimensions.coherence);
        assert!(verdict.dimensions.ending_takeaway);
    }

    #[test]
    fn essay_four_dim_fails_on_empty_stub() {
        let verdict = essay_four_dim_check("");
        assert!(!verdict.go);
        assert!(!verdict.dimensions.thesis_clarity);
        assert!(!verdict.dimensions.evidence_support);
        assert!(!verdict.dimensions.coherence);
        assert!(!verdict.dimensions.ending_takeaway);
        assert!(verdict.reason.contains("thesis clarity"));
    }

    #[test]
    fn essay_four_dim_fails_on_weak_thesis() {
        let essay = "# Some Thoughts\n\n\
            This is a topic that matters. Many people think about it. \
            It is important to consider. Some say one thing, others say another. \
            In conclusion, this is a complex topic that deserves more thought.\n";
        let verdict = essay_four_dim_check(essay);
        // Should fail on multiple dimensions: no specific thesis language,
        // no evidence, poor coherence.
        assert!(!verdict.go);
        assert!(
            !verdict.dimensions.thesis_clarity,
            "should fail thesis clarity"
        );
        assert!(!verdict.dimensions.evidence_support, "should fail evidence");
    }

    #[test]
    fn essay_four_dim_fails_on_no_evidence() {
        let essay = "# Why Dogs Are Better Than Cats\n\n\
            This essay argues that dogs make superior pets. Dogs are loyal. \
            They are also friendly. Furthermore, they are playful. In addition, \
            dogs protect their owners. Moreover, dogs are trainable. \
            However, some people prefer cats. Admittedly, cats are independent. \
            But dogs offer more companionship. Therefore, dogs are the best pets. \
            In the end, choosing a dog is the right decision.\n";
        let verdict = essay_four_dim_check(essay);
        // Has thesis language and coherence, but no specific evidence
        // (no named studies, data, percentages, specific examples).
        assert!(
            !verdict.dimensions.evidence_support,
            "should fail evidence: no studies/data/citations"
        );
    }

    #[test]
    fn essay_four_dim_fails_on_weak_takeaway() {
        let essay = "# The Impact of Social Media\n\n\
            This essay argues that social media has changed communication \
            patterns. I contend that the effects are significant and wide-ranging.\n\n\
            First, according to a 2022 Pew Research study, 72% of adults \
            use social media daily. For example, platforms like Twitter enable \
            real-time news sharing. Specifically, 53% of users get news from \
            social media according to the same report.\n\n\
            Second, social media enables rapid information dissemination. \
            Research published in Nature in 2021 found that false news spreads \
            six times faster than true news on Twitter. For instance, during \
            breaking events, social media often outpaces traditional outlets.\n\n\
            However, some argue that social media causes polarization and \
            echo chambers. Admittedly, algorithms can amplify extreme content. \
            But a 2023 study in Science found that exposure to diverse \
            viewpoints is actually higher on social media than in traditional \
            news consumption.\n\n\
            Social media has both positive and negative aspects. \
            In conclusion, social media is a complex phenomenon that has \
            altered communication habits. The topic is multifaceted and \
            continues to be studied by researchers worldwide.\n";
        let verdict = essay_four_dim_check(essay);
        // Has thesis, evidence, and coherence, but ending lacks insight -
        // merely descriptively restates without "so what?" or forward-looking language.
        assert!(
            !verdict.dimensions.ending_takeaway,
            "should fail ending takeaway: ending is purely descriptive"
        );
    }

    #[test]
    fn essay_four_dim_is_deterministic() {
        let essay = "# The Value of Education\n\n\
            I argue that liberal arts education is undervalued. \
            According to a 2021 AAC&U report, 93% of employers value critical \
            thinking over specific majors. For example, history majors score \
            higher on analytical reasoning tests. First, liberal arts teach \
            adaptable thinking. Second, they foster communication skills. \
            However, some argue STEM degrees are more practical. \
            Admittedly, STEM salaries are higher initially. But liberal arts \
            graduates catch up within 10 years according to Burning Glass data. \
            Therefore, liberal arts education provides lasting value. \
            Ultimately, we must stop treating education as job training.\n";
        let v1 = essay_four_dim_check(essay);
        let v2 = essay_four_dim_check(essay);
        assert_eq!(v1.go, v2.go);
        assert_eq!(v1.reason, v2.reason);
    }

    #[test]
    fn essay_four_dim_passes_on_persuasive_essay() {
        let essay = "# Cities Must Ban Single-Use Plastics\n\n\
            This essay argues that cities should ban single-use plastics \
            because the environmental benefits decisively outweigh short-term \
            economic costs. I contend that policy action is urgent and \
            evidence-based.\n\n\
            First, according to the UN Environment Programme, 300 million \
            tons of plastic waste are produced annually. For example, Rwanda \
            banned plastic bags in 2008 and saw a 15% reduction in litter \
            within one year according to government data. Specifically, a \
            2020 study in Science found that countries with bans had 40% \
            less plastic in waterways within three years.\n\n\
            Second, plastic pollution has severe ecological consequences. \
            Research published in Nature Communications in 2023 found that \
            microplastics were detected in 80% of human blood samples tested. \
            For instance, the Great Pacific Garbage Patch now covers 1.6 \
            million square kilometers — three times the size of France.\n\n\
            However, some argue that plastic bans hurt small businesses. \
            Admittedly, alternative packaging costs 10-15% more initially. \
            But research from the Ellen MacArthur Foundation suggests that \
            reusable systems are cheaper over a 5-year horizon, saving \
            businesses an average of 20% on packaging costs. Furthermore, \
            consumer demand for sustainable products is growing at 8% annually.\n\n\
            Therefore, the environmental benefits of plastic bans outweigh \
            temporary economic disruptions. What this means is that cities \
            must act now — delaying only worsens the ecological debt we leave \
            to future generations. Ultimately, we should treat plastic pollution \
            with the same urgency as climate change.\n";
        let verdict = essay_four_dim_check(essay);
        assert!(
            verdict.go,
            "persuasive essay should pass all dimensions, got: {}",
            verdict.reason
        );
    }
}
