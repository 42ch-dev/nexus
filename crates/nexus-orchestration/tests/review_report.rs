//! V1.48 P0 T4 — Hermetic integration tests for the wired review-report
//! parser (Producer path).
//!
//! These tests exercise [`auto_chain::persist_review_findings_for_schedule`]
//! with an explicit `workspace_dir` to verify both branches of spec
//! `.mstar/knowledge/specs/novel-findings-maturity.md` §1:
//!
//! - **Parsed path (§1.2)**: when `Works/<work_ref>/Logs/review/review-report.md`
//!   exists and parses, persisted findings carry the parsed `kind` /
//!   `severity` / `body` / optional `rule_suggestion` — not the V1.47
//!   placeholder shape.
//! - **Fallback path (§1.3)**: when the file is missing, unreadable, or
//!   yields zero findings, exactly one placeholder finding is persisted
//!   (`kind=craft`, `severity=info`, `target_executor=none`) and the call
//!   returns `Ok(_)` (the supervisor terminal must not fail solely due to
//!   a parse problem).
//!
//! Companion file: `tests/review_findings.rs` covers the V1.47 baseline
//! (workspace_dir=None → placeholder only) and remains unchanged.

#![allow(clippy::unwrap_used)]

use nexus_local_db::findings::{self, FindingListFilters};
use nexus_local_db::works::{self, WorkRecord};
use nexus_orchestration::auto_chain;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::Registry;

// ── Fixtures ───────────────────────────────────────────────────────────────

const CREATOR: &str = "ctr_review_report_test";

fn novel_work(work_id: &str, work_ref: &str, chapter: i32, total: i32) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: CREATOR.to_string(),
        workspace_slug: "ws".to_string(),
        status: "active".to_string(),
        title: "Review Report Test Novel".to_string(),
        long_term_goal: "Finish a short novel".to_string(),
        initial_idea: "A detective story".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-16T10:00:00Z".to_string(),
        updated_at: "2026-06-16T10:00:00Z".to_string(),
        current_stage: "review".to_string(),
        stage_status: "active".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some(work_ref.to_string()),
        total_planned_chapters: if total > 0 { Some(total) } else { None },
        current_chapter: chapter,
        auto_chain_enabled: true,
        driver_schedule_id: None,
        auto_chain_interrupted: false,
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    }
}

async fn test_pool() -> SqlitePool {
    let db = tempfile::Builder::new()
        .prefix("review_report_test_")
        .suffix(".db")
        .tempfile()
        .unwrap();
    let db_path = db.path().to_path_buf();
    std::mem::forget(db);

    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    pool
}

/// Insert a `creator_schedules` row directly for the test scenario.
async fn insert_review_schedule(pool: &SqlitePool, schedule_id: &str, work_id: &str) {
    let now = chrono::Utc::now().timestamp();
    // SAFETY: test-only — DML helper for schedule row insertion.
    sqlx::query(
        r"INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version,
            label, created_at, updated_at, work_id)
           VALUES (?, ?, 'novel-chapter-review', 1, 'running', 'serial', 0, ?, ?, ?, ?)",
    )
    .bind(schedule_id)
    .bind(CREATOR)
    .bind(format!("review-report-{work_id}"))
    .bind(now)
    .bind(now)
    .bind(work_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn list_work_findings(
    pool: &SqlitePool,
    work_id: &str,
) -> Vec<nexus_local_db::findings::Finding> {
    let filters = FindingListFilters {
        work_id: Some(work_id.to_string()),
        ..Default::default()
    };
    findings::list_findings(pool, CREATOR, &filters)
        .await
        .unwrap()
}

/// Materialize `<temp>/Works/<work_ref>/Logs/review/review-report.md` with the
/// given content. Returns the temp workspace root so the caller can pass it
/// to `persist_review_findings_for_schedule`.
fn write_report_file(work_ref: &str, content: &str) -> PathBuf {
    let ws_root = tempfile::tempdir().unwrap().keep();
    let review_dir = nexus_home_layout::work_logs_subdir(&ws_root, work_ref, "review");
    std::fs::create_dir_all(&review_dir).unwrap();
    let report_path = review_dir.join("review-report.md");
    std::fs::write(&report_path, content).unwrap();
    ws_root
}

// ── W-3 (qc3): tracing field-name capture helper ──────────────────────────
//
// Minimal self-contained tracing capture for asserting that fallback
// `tracing::warn!` events emitted by `try_persist_parsed_findings` carry the
// `chapter` field per `.mstar/knowledge/specs/novel-findings-maturity.md`
// §1.3. Uses only workspace deps (`tracing` + `tracing-subscriber`); no new
// dev-dependency.

/// One captured event: its level + the structured field names attached.
#[derive(Debug, Clone)]
struct CapturedEvent {
    level: tracing::Level,
    fields: Vec<String>,
}

/// Records each captured event's level + structured field names.
#[derive(Default, Clone)]
struct FieldNamesCapture(Arc<Mutex<Vec<CapturedEvent>>>);

/// `tracing::field::Visit` impl that collects field names (values discarded).
struct FieldNameCollector(Vec<String>);

impl Visit for FieldNameCollector {
    // The default `record_*` impls in tracing 0.1.44 all delegate here, so
    // implementing only `record_debug` covers every field type (`?` Debug,
    // `%` Display, str, i64, …).
    fn record_debug(&mut self, field: &Field, _value: &dyn std::fmt::Debug) {
        self.0.push(field.name().to_string());
    }
}

impl Layer<Registry> for FieldNamesCapture {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, Registry>) {
        let mut collector = FieldNameCollector(Vec::new());
        event.record(&mut collector);
        self.0.lock().unwrap().push(CapturedEvent {
            level: *event.metadata().level(),
            fields: collector.0,
        });
    }
}

// ── Parsed path (spec §1.2) ────────────────────────────────────────────────

/// AC1 (parsed path): a well-formed `review-report.md` with three Issues
/// bullets yields three finding rows carrying the parsed kind/severity/body.
#[tokio::test]
async fn parsed_report_persists_findings_with_parsed_fields() {
    let pool = test_pool().await;
    let work = novel_work("wrk_parsed", "parsed-novel", 2, 5);
    works::create_work(&pool, &work).await.unwrap();
    insert_review_schedule(&pool, "sch_parsed_review", "wrk_parsed").await;

    let report = "\
# Review Report

## Overall Assessment
Solid chapter with a few craft issues.

## Issues
- POV drifts mid-chapter. kind: craft, severity: major, executor: write
- World timeline contradicts ch2. kind: world_inconsistency, severity: critical
- Plot thread introduced then dropped. kind: plot_hole, severity: minor
";
    let ws_root = write_report_file("parsed-novel", report);

    let count = auto_chain::persist_review_findings_for_schedule(
        &pool,
        "sch_parsed_review",
        Some(&ws_root),
    )
    .await
    .expect("parsed-path persist must succeed");
    assert_eq!(count, 3, "three parsed findings must be persisted");

    let rows = list_work_findings(&pool, "wrk_parsed").await;
    assert_eq!(rows.len(), 3);

    // Sanity: every persisted finding has the V1.47 §8.2 required fields.
    for f in &rows {
        assert_eq!(f.work_id, "wrk_parsed");
        assert_eq!(f.status, "open");
        assert!(!f.kind.is_empty());
        assert!(!f.severity.is_empty());
        assert!(!f.target_executor.is_empty());
        assert!(!f.description.is_empty());
    }

    // Spot-check the kind set + the critical→blocker severity mapping.
    let mut kinds: Vec<String> = rows.iter().map(|f| f.kind.clone()).collect();
    kinds.sort();
    assert_eq!(kinds, vec!["craft", "plot_hole", "world_inconsistency"]);

    let blocker = rows
        .iter()
        .find(|f| f.kind == "world_inconsistency")
        .expect("world_inconsistency finding must be present");
    assert_eq!(
        blocker.severity, "blocker",
        "critical report severity maps to blocker per spec §1.2"
    );

    // The parsed body should not carry the placeholder synthesis text.
    assert!(
        !blocker
            .description
            .contains("V1.47 P0: synthesized finding"),
        "parsed-path body must not contain the placeholder synthesis text"
    );
}

/// AC1 (parsed path, optional field): a finding with `rule_suggestion:` tag
/// round-trips the suggestion into the persisted row.
#[tokio::test]
async fn parsed_report_with_rule_suggestion_round_trips() {
    let pool = test_pool().await;
    let work = novel_work("wrk_rule", "rule-novel", 1, 3);
    works::create_work(&pool, &work).await.unwrap();
    insert_review_schedule(&pool, "sch_rule_review", "wrk_rule").await;

    let report = "\
## Issues
- Inconsistent dialect across chapters. kind: craft, severity: minor, \
rule_suggestion: Pin dialect per region in AGENTS.md
";
    let ws_root = write_report_file("rule-novel", report);

    auto_chain::persist_review_findings_for_schedule(&pool, "sch_rule_review", Some(&ws_root))
        .await
        .unwrap();

    let rows = list_work_findings(&pool, "wrk_rule").await;
    assert_eq!(rows.len(), 1);
    let f = &rows[0];
    assert_eq!(f.kind, "craft");
    assert_eq!(f.severity, "minor");
    assert_eq!(
        f.rule_suggestion.as_deref(),
        Some("Pin dialect per region in AGENTS.md"),
        "rule_suggestion prose must round-trip into the persisted row"
    );
}

/// AC1 (executor routing): when the report omits an executor tag, the
/// persisted row uses the spec §1.2 kind-based default (write for
/// craft/continuity, brainstorm otherwise).
#[tokio::test]
async fn parsed_report_applies_executor_default_when_omitted() {
    let pool = test_pool().await;
    let work = novel_work("wrk_exec", "exec-novel", 1, 3);
    works::create_work(&pool, &work).await.unwrap();
    insert_review_schedule(&pool, "sch_exec_review", "wrk_exec").await;

    let report = "\
## Issues
- Craft nit without executor tag. kind: craft, severity: minor
- Plot issue without executor tag. kind: plot_hole, severity: minor
";
    let ws_root = write_report_file("exec-novel", report);

    auto_chain::persist_review_findings_for_schedule(&pool, "sch_exec_review", Some(&ws_root))
        .await
        .unwrap();

    let rows = list_work_findings(&pool, "wrk_exec").await;
    assert_eq!(rows.len(), 2);

    let craft = rows
        .iter()
        .find(|f| f.kind == "craft")
        .expect("craft finding must be present");
    assert_eq!(
        craft.target_executor, "write",
        "craft kind with no executor → write per spec §1.2"
    );

    let plot = rows
        .iter()
        .find(|f| f.kind == "plot_hole")
        .expect("plot_hole finding must be present");
    assert_eq!(
        plot.target_executor, "brainstorm",
        "plot_hole kind with no executor → brainstorm per spec §1.2"
    );
}

// ── Fallback path (spec §1.3) ──────────────────────────────────────────────

/// AC2 (fallback, missing file): when `review-report.md` does not exist,
/// exactly one placeholder finding is persisted and the call succeeds.
#[tokio::test]
async fn missing_report_falls_back_to_placeholder_finding() {
    let pool = test_pool().await;
    let work = novel_work("wrk_missing", "missing-novel", 1, 3);
    works::create_work(&pool, &work).await.unwrap();
    insert_review_schedule(&pool, "sch_missing_review", "wrk_missing").await;

    // Empty temp dir — no review-report.md anywhere.
    let ws_root = tempfile::tempdir().unwrap().keep();

    let count = auto_chain::persist_review_findings_for_schedule(
        &pool,
        "sch_missing_review",
        Some(&ws_root),
    )
    .await
    .expect("missing-report must NOT fail the persist call");

    // Spec §8.2: ≥1 finding per review pass.
    assert!(
        count >= 1,
        "missing-report fallback must persist ≥1 placeholder finding; got {count}"
    );

    let rows = list_work_findings(&pool, "wrk_missing").await;
    assert_eq!(rows.len(), 1, "exactly one placeholder finding");
    let f = &rows[0];
    // V1.47 placeholder shape.
    assert_eq!(f.kind, "craft");
    assert_eq!(f.severity, "info");
    assert_eq!(f.target_executor, "none");
    // The body must carry the V1.47 marker so reviewers can tell this row
    // came from the fallback rather than the parsed path.
    assert!(
        f.description.contains("V1.47 P0: synthesized finding"),
        "placeholder body must carry the V1.47 synthesis marker"
    );
    assert!(f.rule_suggestion.is_none());
}

/// AC2 (fallback, parse error / zero findings): when the file exists but
/// contains no parseable `## Issues` bullets, exactly one placeholder
/// finding is persisted.
#[tokio::test]
async fn empty_issues_section_falls_back_to_placeholder_finding() {
    let pool = test_pool().await;
    let work = novel_work("wrk_empty", "empty-novel", 1, 3);
    works::create_work(&pool, &work).await.unwrap();
    insert_review_schedule(&pool, "sch_empty_review", "wrk_empty").await;

    // Report exists but has no Issues section — parser yields zero findings.
    let report = "\
# Review Report

## Overall Assessment
Adequate; no actionable issues this pass.

## Recommendations
- Ship as-is.
";
    let ws_root = write_report_file("empty-novel", report);

    let count =
        auto_chain::persist_review_findings_for_schedule(&pool, "sch_empty_review", Some(&ws_root))
            .await
            .expect("zero-findings report must NOT fail the persist call");
    assert!(
        count >= 1,
        "zero-findings fallback must persist ≥1 placeholder; got {count}"
    );

    let rows = list_work_findings(&pool, "wrk_empty").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, "craft");
    assert_eq!(rows[0].severity, "info");
}

/// AC (W-1, qc3): a `review-report.md` larger than `MAX_REVIEW_REPORT_BYTES`
/// (256 KiB) must NOT be buffered into memory on the producer hot path. The
/// producer falls back to the V1.47 placeholder synthesis with a
/// `tracing::warn!` carrying `size_bytes` / `cap_bytes` / `chapter`.
///
/// The fixture uses a valid `## Issues` body padded past the cap so that ONLY
/// the size guard (not the parser) is what rejects the read — proving the cap
/// fires before `read_to_string` is ever called.
#[tokio::test]
async fn large_report_falls_back_to_placeholder() {
    let pool = test_pool().await;
    let work = novel_work("wrk_large", "large-novel", 3, 5);
    works::create_work(&pool, &work).await.unwrap();
    insert_review_schedule(&pool, "sch_large_review", "wrk_large").await;

    // 300 KiB of padding > 256 KiB cap. The literal mirrors the
    // `MAX_REVIEW_REPORT_BYTES` constant in `auto_chain.rs`; if that constant
    // changes, bump this fixture to match (the assert! below guards it).
    const CAP_BYTES: usize = 256 * 1024;
    let pad = "x".repeat(300 * 1024);
    let report = format!(
        "# Review Report\n\n## Issues\n\
         - Filler to exceed the bounded-read cap. kind: craft, severity: minor\n\n\
         {pad}\n"
    );
    assert!(
        report.len() > CAP_BYTES,
        "fixture must exceed the {}-byte cap; got {} bytes",
        CAP_BYTES,
        report.len()
    );
    let ws_root = write_report_file("large-novel", &report);

    let count =
        auto_chain::persist_review_findings_for_schedule(&pool, "sch_large_review", Some(&ws_root))
            .await
            .expect("oversized report must NOT fail the persist call");

    assert!(
        count >= 1,
        "oversized-report fallback must persist ≥1 placeholder; got {count}"
    );

    let rows = list_work_findings(&pool, "wrk_large").await;
    assert_eq!(rows.len(), 1, "exactly one placeholder finding");
    let f = &rows[0];
    // V1.47 placeholder shape (not parsed fields).
    assert_eq!(f.kind, "craft");
    assert_eq!(f.severity, "info");
    assert_eq!(f.target_executor, "none");
    assert!(
        f.description.contains("V1.47 P0: synthesized finding"),
        "oversized-report fallback must use the placeholder body"
    );
    assert!(f.rule_suggestion.is_none());
}

/// AC (W-2, qc3): the parsed-report path persists all findings inside a
/// single SQLite transaction. This regression guard verifies the two
/// observable invariants of that change:
///
/// 1. **Commit semantics**: a parsed report with N findings yields exactly
///    N persisted rows (the transaction commits, not rolls back).
/// 2. **Idempotency inside the transaction**: re-running the same schedule
///    on the same report is a no-op — `ON CONFLICT DO NOTHING` still works
///    when the inserts share one transaction boundary.
///
/// The transaction itself is documented in code via a `tracing::debug!`
/// marker emitted at the `BEGIN` boundary in `persist_parsed_findings`; this
/// test exercises the behavior that marker promises.
#[tokio::test]
async fn parsed_findings_transaction_commits_and_is_idempotent_on_retry() {
    let pool = test_pool().await;
    let work = novel_work("wrk_tx", "tx-novel", 2, 4);
    works::create_work(&pool, &work).await.unwrap();
    insert_review_schedule(&pool, "sch_tx_review", "wrk_tx").await;

    let report = "\
# Review Report

## Issues
- Craft nit one. kind: craft, severity: minor
- Craft nit two. kind: craft, severity: major, executor: write
- World timeline gap. kind: world_inconsistency, severity: critical
";
    let ws_root = write_report_file("tx-novel", report);

    // First pass: transaction commits all three parsed findings.
    let count1 =
        auto_chain::persist_review_findings_for_schedule(&pool, "sch_tx_review", Some(&ws_root))
            .await
            .expect("parsed-path transaction must commit");
    assert_eq!(count1, 3, "first pass persists all 3 findings");

    let rows1 = list_work_findings(&pool, "wrk_tx").await;
    assert_eq!(rows1.len(), 3, "transaction committed all 3 rows");

    // Second pass: same schedule + same report → idempotent (ON CONFLICT
    // DO NOTHING fires inside the transaction). The row count must not grow.
    let count2 =
        auto_chain::persist_review_findings_for_schedule(&pool, "sch_tx_review", Some(&ws_root))
            .await
            .expect("idempotent retry must not fail");
    assert_eq!(
        count2, 3,
        "retry returns the parsed count but the DB rows are unchanged (idempotent)"
    );

    let rows2 = list_work_findings(&pool, "wrk_tx").await;
    assert_eq!(
        rows2.len(),
        3,
        "idempotent retry inside the transaction must not duplicate rows"
    );
}

/// AC3 (hermetic / supervisor contract): when `workspace_dir` is `None`
/// (the V1.47 hermetic DB-only mode), the producer always uses the
/// placeholder synthesis — never tries to touch the filesystem.
#[tokio::test]
async fn workspace_none_uses_placeholder_path_without_filesystem() {
    let pool = test_pool().await;
    let work = novel_work("wrk_no_ws", "no-ws-novel", 1, 3);
    works::create_work(&pool, &work).await.unwrap();
    insert_review_schedule(&pool, "sch_no_ws_review", "wrk_no_ws").await;

    let count = auto_chain::persist_review_findings_for_schedule(
        &pool,
        "sch_no_ws_review",
        None, // hermetic DB-only mode
    )
    .await
    .expect("None workspace must not fail");

    assert_eq!(count, 1, "placeholder path persists exactly 1 finding");
    let rows = list_work_findings(&pool, "wrk_no_ws").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, "craft");
    assert_eq!(rows[0].severity, "info");
    assert_eq!(rows[0].target_executor, "none");
}

/// AC4 (non-review preset no-op): a `novel-writing` schedule must never
/// enter the parsing path, even with a workspace_dir provided.
#[tokio::test]
async fn non_review_preset_is_noop_with_workspace_dir() {
    let pool = test_pool().await;
    let work = novel_work("wrk_nonrev", "nonrev-novel", 1, 3);
    works::create_work(&pool, &work).await.unwrap();

    // Insert a non-review schedule directly.
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        r"INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version,
            label, created_at, updated_at, work_id)
           VALUES (?, ?, 'novel-writing', 7, 'running', 'serial', 0, ?, ?, ?, ?)",
    )
    .bind("sch_nonrev")
    .bind(CREATOR)
    .bind("nonrev")
    .bind(now)
    .bind(now)
    .bind("wrk_nonrev")
    .execute(&pool)
    .await
    .unwrap();

    let ws_root = tempfile::tempdir().unwrap().keep();

    let count =
        auto_chain::persist_review_findings_for_schedule(&pool, "sch_nonrev", Some(&ws_root))
            .await
            .expect("non-review call must succeed (early no-op)");

    assert_eq!(count, 0, "non-review preset must be a no-op");
    let rows = list_work_findings(&pool, "wrk_nonrev").await;
    assert!(
        rows.is_empty(),
        "non-review preset must not persist any findings"
    );
}

// ── W-3 (qc3): chapter field in fallback tracing spans ───────────────────

/// AC (W-3, qc3): every fallback `tracing::warn!` in the producer path
/// includes the `chapter` field per spec §1.3 (operator-debugging field for
/// chapter-scoped review passes).
///
/// Exercises the missing-report fallback with `chapter = Some(2)` and asserts
/// the captured warn event carries `chapter` in its structured fields. The
/// `set_default` guard scopes the capturing subscriber to this thread; the
/// current-thread `#[tokio::test]` runtime resumes `.await` on the same
/// thread, so the guard stays active across the await.
#[tokio::test]
async fn fallback_warn_includes_chapter_field() {
    let pool = test_pool().await;
    let work = novel_work("wrk_chapter_field", "chapter-field-novel", 2, 4);
    works::create_work(&pool, &work).await.unwrap();
    insert_review_schedule(&pool, "sch_chapter_field", "wrk_chapter_field").await;

    // Empty workspace dir → guarantees the missing-report fallback fires.
    let ws_root = tempfile::tempdir().unwrap().keep();

    let capture = FieldNamesCapture::default();
    let subscriber = Registry::default().with(capture.clone());
    let guard = tracing::subscriber::set_default(subscriber);

    let _ = auto_chain::persist_review_findings_for_schedule(
        &pool,
        "sch_chapter_field",
        Some(&ws_root),
    )
    .await
    .expect("missing-report fallback must not fail");

    drop(guard);

    let events = capture.0.lock().unwrap().clone();
    assert!(
        !events.is_empty(),
        "expected ≥1 tracing event from the fallback path; got none"
    );

    // Filter to WARN-level events (the fallback `warn!` calls). The success
    // `info!` from `persist_placeholder_finding` also carries `schedule_id`
    // but is not a fallback warn, so filter on level to isolate the warns.
    let warn_events: Vec<&CapturedEvent> = events
        .iter()
        .filter(|e| e.level == tracing::Level::WARN)
        .collect();
    assert!(
        !warn_events.is_empty(),
        "expected ≥1 WARN-level fallback event; got events: {events:?}"
    );
    for (i, ev) in warn_events.iter().enumerate() {
        assert!(
            ev.fields.iter().any(|f| f == "chapter"),
            "fallback warn event #{i} is missing the `chapter` field; \
             got fields: {:?}",
            ev.fields
        );
    }
}
