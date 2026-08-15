//! `FindingPort.put_findings` AR-2 routing tests (V1.165 P1 T2 / DR-68) —
//! per-finding `extensions.nexus` discriminator.
//!
//! Covers both routings (`primary_spec` §AR-2 lock):
//! - `work_id` present (no `world_id`) → legacy work path — byte-identical
//!   mapping (spoke vocabulary → nexus `findings` vocabulary, unchanged).
//! - `world_id` present (no `work_id`) → world path — spoke `Finding` →
//!   `world_findings` row with AC-V165-3 fields (kind, spoke severity
//!   verbatim, `target_entry_id`, description) + `extensions_json` verbatim.
//! - both keys / neither key → `INVALID_INPUT` reject naming the `finding_id`.
//! - mixed batches commit atomically (W-1 transaction wraps both tables); a
//!   mid-batch failure rolls work- and world-scoped rows back together.

#![allow(clippy::unwrap_used)]

use nexus_local_db::world_findings::get_world_finding;
use nexus_local_db::{open_pool, run_migrations};
use nexus_spoke_adapter::{Finding, FindingPort, NexusAdapter, SpokeRejectCode, SpokeResult};
use serde_json::{json, Value};

async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = open_pool(&db_path).await.unwrap();
    run_migrations(&pool).await.unwrap();
    (pool, dir)
}

/// Seed creator + work (FK target for the legacy `findings` table).
async fn seed_work(pool: &sqlx::SqlitePool) {
    // SAFETY: test-only fixture scaffolding — inserts match the creators /
    // works DDL (same chain the in-crate finding_port tests seed).
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('ctr_test', 'Test', 'active', datetime('now'), '{}')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO works \
         (work_id, creator_id, workspace_slug, status, title, long_term_goal, \
          initial_idea, intake_status, created_at, updated_at) \
         VALUES ('wrk_test', 'ctr_test', 'wrk_test', 'active', 'Test', 'goal', 'idea', \
                 'complete', '2026-07-28T00:00:00Z', '2026-07-28T00:00:00Z')",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Seed creator + world (FK target for the `world_findings` table).
async fn seed_world(pool: &sqlx::SqlitePool, world_id: &str) {
    // SAFETY: test-only fixture scaffolding — inserts match the creators /
    // narrative_worlds DDL (mirrors nexus-local-db world_findings tests).
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('ctr_test', 'Test', 'active', datetime('now'), '{}')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, metadata_json) \
         VALUES (?, 'wrk_test', 'ctr_test', 'Routing Test World', \
                 'routing-test-world', 'active', 'private', 'manual', '{}')",
    )
    .bind(world_id)
    .execute(pool)
    .await
    .unwrap();
}

/// A spoke `Finding` fixture carrying exactly one routing key in
/// `extensions.nexus` (`work_id` xor `world_id`, or both, or neither).
fn spoke_finding(finding_id: &str, routing: &Value) -> Finding {
    serde_json::from_value(json!({
        "schema_version": 1,
        "finding_id": finding_id,
        "severity": "info",
        "status": "open",
        "title": format!("Finding {finding_id}"),
        "description": "test finding body",
        "extensions": { "nexus": routing },
    }))
    .expect("valid spoke Finding fixture")
}

/// A work-scoped finding: `extensions.nexus.work_id` only.
fn work_finding(finding_id: &str) -> Finding {
    spoke_finding(
        finding_id,
        &json!({ "work_id": "wrk_test", "creator_id": "ctr_test" }),
    )
}

/// A world-scoped finding: `extensions.nexus.world_id` only (creator id
/// rides along as provenance — optional on the world path).
fn world_finding(finding_id: &str) -> Finding {
    spoke_finding(
        finding_id,
        &json!({ "world_id": "wld_test", "creator_id": "ctr_test" }),
    )
}

/// Fetch the persisted legacy `findings` row (11-tuple projection; test-only).
#[allow(clippy::type_complexity)]
type FindingRow = (
    String,
    String,
    Option<i64>,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
);

async fn fetch_legacy_finding(pool: &sqlx::SqlitePool, finding_id: &str) -> FindingRow {
    sqlx::query_as(
        "SELECT finding_id, work_id, chapter, severity, status, title, description, \
         target_executor, creator_id, kind, rule_suggestion \
         FROM findings WHERE finding_id = ?",
    )
    .bind(finding_id)
    .fetch_one(pool)
    .await
    .expect("row persisted")
}

async fn assert_legacy_finding_absent(pool: &sqlx::SqlitePool, finding_id: &str) {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings WHERE finding_id = ?")
        .bind(finding_id)
        .fetch_one(pool)
        .await
        .expect("count query succeeds");
    assert_eq!(
        count, 0,
        "finding {finding_id} must not be in the legacy table"
    );
}

// ── AR-2: work path (byte-identical regression) ────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_scoped_finding_routes_to_legacy_findings_table() {
    let (pool, _dir) = fresh_pool().await;
    seed_work(&pool).await;

    let adapter = NexusAdapter::new(pool.clone());
    let result = adapter.put_findings(vec![work_finding("fnd_wrk")]).await;
    let returned = match result {
        SpokeResult::Ok(v) => v,
        SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
    };
    assert_eq!(returned.len(), 1);
    assert_eq!(returned[0].finding_id, "fnd_wrk");

    // Legacy row: vocabulary MAPPED (info → info here), work FK present.
    let row = fetch_legacy_finding(&pool, "fnd_wrk").await;
    assert_eq!(row.1, "wrk_test", "work_id FK");
    assert_eq!(row.3, "info", "spoke `info` → nexus `info`");
    assert_eq!(row.8, "ctr_test", "creator_id");
    assert_eq!(row.9, "craft", "default kind");

    // No world row must exist for a work-scoped finding.
    assert!(get_world_finding(&pool, "fnd_wrk").await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_path_vocabulary_mapping_unchanged() {
    let (pool, _dir) = fresh_pool().await;
    seed_work(&pool).await;

    let adapter = NexusAdapter::new(pool.clone());
    // `warning` severity + `dismissed` status to assert the mapping still applies.
    let finding: Finding = serde_json::from_value(json!({
        "schema_version": 1,
        "finding_id": "fnd_voc_wrk",
        "severity": "warning",
        "status": "dismissed",
        "title": "vocab",
        "description": "x",
        "extensions": { "nexus": { "work_id": "wrk_test", "creator_id": "ctr_test" } },
    }))
    .expect("valid Finding");

    match adapter.put_findings(vec![finding]).await {
        SpokeResult::Ok(v) => assert_eq!(v.len(), 1),
        SpokeResult::Reject(r) => panic!("ok on valid vocabulary: {r:?}"),
    }

    let row = fetch_legacy_finding(&pool, "fnd_voc_wrk").await;
    assert_eq!(
        row.3, "minor",
        "spoke `warning` → nexus `minor` (unchanged)"
    );
    assert_eq!(
        row.4, "wont_fix",
        "spoke `dismissed` → nexus `wont_fix` (unchanged)"
    );
}

// ── AR-2: world path ──────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn world_scoped_finding_routes_to_world_findings_table() {
    let (pool, _dir) = fresh_pool().await;
    seed_world(&pool, "wld_test").await;

    let adapter = NexusAdapter::new(pool.clone());
    // Full AC-V165-3 shape: kind, spoke severity verbatim, target_entry_id,
    // description + text_position/source_anchor for verbatim JSON asserts.
    let finding: Finding = serde_json::from_value(json!({
        "schema_version": 1,
        "finding_id": "fnd_wld",
        "severity": "warning",
        "status": "open",
        "title": "The marble is in the box",
        "description": "kb_bo holds a stale belief: 'The marble is in the box' — informing event evt_transfer",
        "kind": "stale_belief_drift",
        "target_entry_id": "kb_bo",
        "source_anchor": {
            "schema_version": 1,
            "source_id": "evt_transfer",
            "label": "Marble transfer",
            "mime_type": "text/plain",
            "extensions": {}
        },
        "suggested_fix": "Update the belief",
        "text_position": { "paragraph": 3 },
        "created_at": "2026-08-14T10:00:00Z",
        "updated_at": "2026-08-14T10:00:00Z",
        "extensions": { "nexus": { "world_id": "wld_test", "creator_id": "ctr_test" } },
    }))
    .expect("valid spoke Finding fixture");

    let result = adapter.put_findings(vec![finding]).await;
    match result {
        SpokeResult::Ok(v) => assert_eq!(v.len(), 1),
        SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
    }

    let row = get_world_finding(&pool, "fnd_wld")
        .await
        .unwrap()
        .expect("world row persisted");
    assert_eq!(row.world_id, "wld_test");
    assert_eq!(row.schema_version, 1);
    // AC-V165-3: spoke vocabulary verbatim — no nexus mapping on the world path.
    assert_eq!(
        row.severity, "warning",
        "spoke severity verbatim (NOT `minor`)"
    );
    assert_eq!(row.status, "open", "spoke status verbatim");
    assert_eq!(row.kind.as_deref(), Some("stale_belief_drift"));
    assert_eq!(row.target_entry_id.as_deref(), Some("kb_bo"));
    assert_eq!(row.title, "The marble is in the box");
    assert!(
        row.description.contains("kb_bo") && row.description.contains("evt_transfer"),
        "description names actor + informing event: {}",
        row.description
    );
    // Verbatim JSON columns.
    let anchor: Value = serde_json::from_str(row.source_anchor_json.as_deref().unwrap_or(""))
        .expect("anchor parses");
    assert_eq!(anchor["source_id"], "evt_transfer");
    assert_eq!(anchor["label"], "Marble transfer");
    assert_eq!(anchor["schema_version"], 1);
    assert_eq!(row.text_position_json, r#"{"paragraph":3}"#);
    let ext: Value = serde_json::from_str(&row.extensions_json).expect("extensions_json parses");
    assert_eq!(
        ext["nexus"]["world_id"], "wld_test",
        "extensions_json carries the stamped world_id verbatim"
    );
    assert_eq!(
        ext["nexus"]["creator_id"], "ctr_test",
        "extensions_json carries creator_id provenance verbatim"
    );
    // Epoch conversion (RFC 3339 → Unix epoch, mirroring the work path).
    let expected_epoch = chrono::DateTime::parse_from_rfc3339("2026-08-14T10:00:00Z")
        .unwrap()
        .timestamp();
    assert_eq!(row.created_at, expected_epoch);
    assert_eq!(row.updated_at, expected_epoch);

    // No legacy row must exist for a world-scoped finding.
    assert_legacy_finding_absent(&pool, "fnd_wld").await;
}

// ── AR-2: discriminator rejects ───────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn both_routing_keys_reject_invalid_input() {
    let (pool, _dir) = fresh_pool().await;
    seed_work(&pool).await;
    seed_world(&pool, "wld_test").await;

    let adapter = NexusAdapter::new(pool.clone());
    let finding = spoke_finding(
        "fnd_both",
        &json!({ "work_id": "wrk_test", "world_id": "wld_test", "creator_id": "ctr_test" }),
    );

    match adapter.put_findings(vec![finding]).await {
        SpokeResult::Reject(r) => {
            assert_eq!(r.code, SpokeRejectCode::InvalidInput);
            assert_eq!(
                r.details.as_ref().and_then(|d| d.get("finding_id")),
                Some(&json!("fnd_both"))
            );
        }
        SpokeResult::Ok(_) => panic!("expected reject on both routing keys"),
    }
    // Nothing persisted to either table.
    assert_legacy_finding_absent(&pool, "fnd_both").await;
    assert!(get_world_finding(&pool, "fnd_both")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn neither_routing_key_rejects_invalid_input() {
    let (pool, _dir) = fresh_pool().await;
    seed_work(&pool).await;
    seed_world(&pool, "wld_test").await;

    let adapter = NexusAdapter::new(pool.clone());
    let finding = spoke_finding("fnd_neither", &json!({ "creator_id": "ctr_test" }));

    match adapter.put_findings(vec![finding]).await {
        SpokeResult::Reject(r) => {
            assert_eq!(r.code, SpokeRejectCode::InvalidInput);
            assert_eq!(
                r.details.as_ref().and_then(|d| d.get("finding_id")),
                Some(&json!("fnd_neither"))
            );
        }
        SpokeResult::Ok(_) => panic!("expected reject on missing routing keys"),
    }
    assert_legacy_finding_absent(&pool, "fnd_neither").await;
    assert!(get_world_finding(&pool, "fnd_neither")
        .await
        .unwrap()
        .is_none());
}

// ── AR-2: batch atomicity across both tables ──────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mixed_batch_commits_both_tables_atomically() {
    let (pool, _dir) = fresh_pool().await;
    seed_work(&pool).await;
    seed_world(&pool, "wld_test").await;

    let adapter = NexusAdapter::new(pool.clone());
    let result = adapter
        .put_findings(vec![
            world_finding("fnd_mix_wld"),
            work_finding("fnd_mix_wrk"),
        ])
        .await;
    match result {
        SpokeResult::Ok(v) => assert_eq!(v.len(), 2),
        SpokeResult::Reject(r) => panic!("expected ok on mixed batch: {r:?}"),
    }

    // Both rows landed in their respective tables.
    assert!(get_world_finding(&pool, "fnd_mix_wld")
        .await
        .unwrap()
        .is_some());
    assert_eq!(
        fetch_legacy_finding(&pool, "fnd_mix_wrk").await.1,
        "wrk_test"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mixed_batch_mid_failure_rolls_back_both_tables() {
    let (pool, _dir) = fresh_pool().await;
    seed_work(&pool).await;
    seed_world(&pool, "wld_test").await;

    let adapter = NexusAdapter::new(pool.clone());
    // Batch: [world row, work row, world row with the FIRST world finding's
    // id]. The third item collides on the `world_findings` PK mid-batch →
    // UNIQUE violation. The W-1 transaction must roll back the work row
    // (inserted second) AND the first world row together (AR-2 atomicity).
    let batch = vec![
        world_finding("fnd_rb_wld"),
        work_finding("fnd_rb_wrk"),
        world_finding("fnd_rb_wld"),
    ];

    match adapter.put_findings(batch).await {
        SpokeResult::Reject(r) => {
            assert_eq!(
                r.code,
                SpokeRejectCode::InternalError,
                "mid-batch collision must reject with INTERNAL_ERROR"
            );
        }
        SpokeResult::Ok(_) => panic!("expected reject on duplicate finding_id mid-batch"),
    }

    assert!(
        get_world_finding(&pool, "fnd_rb_wld")
            .await
            .unwrap()
            .is_none(),
        "first world row must roll back"
    );
    assert_legacy_finding_absent(&pool, "fnd_rb_wrk").await;
}
