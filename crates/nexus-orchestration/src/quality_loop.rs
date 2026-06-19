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
        LlmExtractOutcome::Fallback(reason) => {
            tracing::debug!(
                schedule_id,
                reason,
                "kb-extract: falling back to heuristic extraction"
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
        LlmExtractOutcome::Fallback(reason) => {
            tracing::debug!(
                schedule_id,
                reason,
                "kb-missing: falling back to heuristic extraction"
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

/// Outcome of an LLM extraction attempt.
///
/// `Candidates` carries the LLM-extracted candidates; `Fallback` signals that
/// the LLM pathway was unavailable (no registry, capability absent, or
/// `WorkerUnavailable`) and the caller should use the heuristic.
enum LlmExtractOutcome {
    Candidates(Vec<KbCandidate>),
    Fallback(&'static str),
}

/// Attempt LLM extraction via the `nexus.llm.extract` capability.
///
/// Returns [`LlmExtractOutcome::Fallback`] (with a reason for the debug log)
/// whenever the LLM pathway is unavailable, so the hook can fall back to the
/// heuristic. The capability itself returns an empty candidate list on
/// malformed LLM JSON (best-effort); that is surfaced as
/// `Candidates(vec![])`, not a fallback.
async fn extract_via_llm(
    registry: Option<&CapabilityRegistry>,
    ctx: &ChapterContext,
) -> LlmExtractOutcome {
    let Some(registry) = registry else {
        return LlmExtractOutcome::Fallback("no capability registry threaded");
    };
    let Some(cap) = registry.get(LLM_EXTRACT_CAPABILITY) else {
        return LlmExtractOutcome::Fallback("nexus.llm.extract not registered");
    };

    // High-level extraction instruction. The capability wraps this with the
    // JSON output-format framing + the verbatim chapter prose (llm-extract.md §1.3).
    let prompt = "Extract the fictional entities (characters, locations, organizations, items, events, abilities, conflicts, info points) that appear in the chapter prose below. For each entity, judge the most appropriate wire block_type, give a confidence in [0.0,1.0], and quote a verbatim excerpt from the chapter that justifies the extraction.";

    let input = serde_json::json!({
        "prompt": prompt,
        "chapter_prose": ctx.prose,
        "_creator_id": ctx.creator_id,
        // The review-time hook runs outside a preset session; pass an empty
        // session id. The capability only forwards it to the worker IPC for
        // routing — it is not a security identity (SEC-V131-01 covers creator_id).
        "_session_id": "",
    });

    let output = match cap.run(input).await {
        Ok(o) => o,
        Err(CapabilityError::WorkerUnavailable) => {
            return LlmExtractOutcome::Fallback("nexus.llm.extract worker unavailable");
        }
        Err(e) => {
            tracing::warn!(
                schedule_context = %ctx.work_id,
                error = %e,
                "kb-extract: nexus.llm.extract capability error; falling back to heuristic"
            );
            return LlmExtractOutcome::Fallback("nexus.llm.extract capability error");
        }
    };

    let candidates_json = output
        .get("candidates")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let candidates: Vec<KbCandidate> = candidates_json
        .iter()
        .filter_map(candidate_from_llm_json)
        .take(MAX_CANDIDATES_PER_PASS)
        .collect();
    LlmExtractOutcome::Candidates(candidates)
}

/// Build a [`KbCandidate`] from one LLM-returned candidate JSON object.
///
/// Returns `None` when `canonical_name` is missing/empty (no point persisting
/// a nameless candidate). Fills `proposed_payload` with a novel-profile
/// `KeyBlockBody` JSON whose `novel_category` is derived from the LLM-judged
/// `block_type` (entity-scope-model §5.1.1 mapping) so adopt-time
/// `ValidationMode::Novel` passes. The payload also carries the four LLM keys
/// so the adopt CLI can read them from either the dedicated columns or the
/// JSON (llm-extract.md §3.1).
///
/// `pub(crate)`: reused by `tasks::LlmExtractTask` so there is a single
/// LLM→KbCandidate mapping across the review-time hook and the task.
pub(crate) fn candidate_from_llm_json(c: &serde_json::Value) -> Option<KbCandidate> {
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
    let novel_category = block_type_to_novel_category(&block_type);

    let mut payload = serde_json::json!({
        "summary": summary.clone().unwrap_or_else(|| format!("LLM-extracted entity: {canonical_name}")),
        "attributes": {
            "novel_category": novel_category,
            "aliases": [canonical_name.as_str()],
        },
        "tags": ["novel", "llm-extracted"],
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
}
