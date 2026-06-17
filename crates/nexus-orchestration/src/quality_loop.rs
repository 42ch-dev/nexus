//! Review-time KB candidate extraction (V1.50 T-B P1).
//!
//! Implements the review-time hook referenced by
//! [`novel-writing/cron-staggering.md` §4.4](../../.mstar/knowledge/specs/novel-writing/cron-staggering.md)
//! and [`entity-scope-model.md` §5.5](../../.mstar/knowledge/specs/entity-scope-model.md).
//!
//! # Heuristic-only (V1.50)
//!
//! Per compass §0.1 decision 6, V1.50 ships **heuristic-only** extraction —
//! no LLM call. The heuristic scans chapter prose for capitalized noun
//! phrases (the most common shape of a proper noun / character / place name
//! in English-language fiction), filters against existing
//! `kb_key_blocks.canonical_name` to avoid duplicates, and inserts
//! `kb_extract_jobs` rows with `promotion_status='pending'`.
//!
//! The author confirms or dismisses each candidate via
//! `creator world kb adopt|reject`.
//!
//! # Hook wiring
//!
//! [`extract_kb_candidates_for_review`] is invoked by the schedule supervisor
//! when a `novel-review-master` schedule reaches a terminal state (see
//! `schedule::supervisor::ScheduleSupervisor::on_schedule_terminal`).
//!
//! ```text
//! // COORDINATE-WITH-T-A-P2
//! ```
//! T-A P2 (`2026-06-18-v1.50-cron-review-staggering`) wires the per-Work
//! `review` cron role that enqueues `novel-review-master` schedules. Until
//! T-A P2 lands, the hook still fires for any `novel-review-master`
//! schedule that reaches the supervisor (e.g. the V1.39 stale-findings
//! `auto_review_master_on_timeout` path, or manual `creator run`), so the
//! extraction pipeline is independently testable.

use crate::auto_chain::AutoChainError;
use nexus_local_db::kb_extract_job::{insert_pending, is_idempotent};
use regex::Regex;
use sqlx::SqlitePool;
use std::sync::OnceLock;

/// Default `block_type` guess for heuristic candidates.
///
/// Capitalized noun phrases in fiction prose are most often character names,
/// so the heuristic tags every candidate as `character`. The author corrects
/// the type on adopt (or rejects) — see entity-scope-model §5.5.5: the
/// promotion state machine governs *how* a row enters the World, not *what*
/// it contains, and `ValidationMode::Novel` re-runs on adopt.
const DEFAULT_BLOCK_TYPE_GUESS: &str = "character";

/// Maximum candidates persisted per review pass (safety cap to avoid
/// flooding `kb_extract_jobs.pending` from a single chapter scan).
const MAX_CANDIDATES_PER_PASS: usize = 20;

/// A heuristic-extracted KB candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KbCandidate {
    /// Capitalized phrase as it appeared in the chapter prose.
    pub canonical_name_guess: String,
    /// Proposed `KeyBlockBody` serialized as JSON.
    pub proposed_payload: String,
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
            }
        })
        .collect()
}

// ── Hook entry point ──────────────────────────────────────────────────

/// Review-time KB extraction hook.
///
/// Mirrors the shape of
/// [`auto_chain::persist_review_findings_for_schedule`] and
/// [`auto_chain::promote_foreshadowing_for_schedule`]: load schedule → work →
/// chapter body, run the heuristic, filter existing canonical names, insert
/// pending rows.
///
/// # Behavior
///
/// 1. Loads the schedule row to read `preset_id`, `work_id`, `creator_id`.
/// 2. Returns `Ok(0)` early when the preset is not `novel-review-master`.
/// 3. Returns `Ok(0)` when `workspace_dir` is `None` (hermetic DB-only tests)
///    or the work/chapter body file is missing — the heuristic needs prose.
/// 4. Reads the current chapter body, runs [`extract_candidates_from_text`],
///    filters out names already present in `kb_key_blocks.canonical_name`
///    for the work's world, applies the [`is_idempotent`] guard, and inserts
///    `pending` rows.
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
) -> Result<usize, AutoChainError> {
    let Some(ctx) = load_review_context(pool, schedule_id, workspace_dir).await? else {
        return Ok(0);
    };
    let candidates = extract_candidates_from_text(&ctx.prose);
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

/// Loaded review context: schedule → work → chapter prose.
///
/// Returned by [`load_review_context`] when all preconditions are met
/// (preset is `novel-review-master`, work has a world, chapter body is
/// readable). `None` for any no-op early return (logged at `debug`/`warn`).
struct ReviewContext {
    creator_id: String,
    work_id: String,
    world_id: String,
    chapter: i32,
    workspace_id: String,
    prose: String,
}

/// Resolve the schedule → work → chapter body for the extraction hook.
///
/// Returns `Ok(None)` for every no-op early return (non-`review-master`
/// preset, NULL `work_id`, no `workspace_dir`, missing work/world/chapter/body).
/// All no-op branches emit a `tracing` event so operators can see the skip.
async fn load_review_context(
    pool: &SqlitePool,
    schedule_id: &str,
    workspace_dir: Option<&std::path::Path>,
) -> Result<Option<ReviewContext>, AutoChainError> {
    use crate::preset_ids::NOVEL_REVIEW_MASTER_PRESET_ID;

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
        tracing::debug!(schedule_id, "kb-extract: schedule row not found; skipping");
        return Ok(None);
    };

    let preset_id: String = sqlx::Row::try_get(&row, "preset_id")
        .map_err(|e| AutoChainError::InvalidState(format!("decode preset_id: {e}")))?;
    let work_id: Option<String> = sqlx::Row::try_get(&row, "work_id")
        .map_err(|e| AutoChainError::InvalidState(format!("decode work_id: {e}")))?;
    let creator_id: String = sqlx::Row::try_get(&row, "creator_id")
        .map_err(|e| AutoChainError::InvalidState(format!("decode creator_id: {e}")))?;

    if preset_id != NOVEL_REVIEW_MASTER_PRESET_ID {
        return Ok(None);
    }
    let Some(work_id) = work_id else {
        tracing::warn!(
            schedule_id,
            "kb-extract: schedule has NULL work_id; skipping"
        );
        return Ok(None);
    };
    let Some(ws_dir) = workspace_dir else {
        tracing::debug!(schedule_id, "kb-extract: no workspace_dir; skipping");
        return Ok(None);
    };

    let work = match nexus_local_db::works::get_work(pool, &creator_id, &work_id).await {
        Ok(Some(w)) => w,
        Ok(None) => {
            tracing::warn!(schedule_id, work_id = %work_id, "kb-extract: work not found; skipping");
            return Ok(None);
        }
        Err(e) => return Err(AutoChainError::from(e)),
    };
    let Some(world_id) = work.world_id.as_deref() else {
        tracing::debug!(schedule_id, work_id = %work_id, "kb-extract: work has no world_id; skipping");
        return Ok(None);
    };
    if work.current_chapter <= 0 {
        tracing::debug!(schedule_id, work_id = %work_id, "kb-extract: current_chapter <= 0; skipping");
        return Ok(None);
    }

    let workspace_id = resolve_workspace_id(pool, &creator_id).await;
    let Some(prose) =
        load_chapter_prose(pool, schedule_id, &work_id, work.current_chapter, ws_dir).await?
    else {
        return Ok(None);
    };

    Ok(Some(ReviewContext {
        creator_id,
        work_id,
        world_id: world_id.to_string(),
        chapter: work.current_chapter,
        workspace_id,
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
async fn persist_candidates(
    pool: &SqlitePool,
    schedule_id: &str,
    ctx: &ReviewContext,
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
        match insert_pending(
            pool,
            &ctx.creator_id,
            &ctx.workspace_id,
            &ctx.world_id,
            Some(&ctx.work_id),
            Some(i64::from(ctx.chapter)),
            DEFAULT_BLOCK_TYPE_GUESS,
            &candidate.canonical_name_guess,
            &candidate.proposed_payload,
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
}
