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

/// A KE-capable adapter whose selection authorizes the worlds these
/// fixtures own (v1.191 P1 T8 — the check/relate paths read knowledge).
fn scoped(pool: sqlx::SqlitePool) -> NexusAdapter<'static> {
    NexusAdapter::new(
        pool,
        nexus_knowledge::world_kb::KnowledgeReadScope::creator_management(
            vec![nexus_knowledge::world_kb::knowledge_entry::KnowledgeOwnerRef::world("wld_test")],
            Vec::new(),
        ),
    )
}

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

    let adapter = scoped(pool.clone());
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

    let adapter = scoped(pool.clone());
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

    // The cited target is a knowledge entry of the routed world, so the v1.191
    // P1 T8 target gate admits it (L2 F4 + T8 CI fix: the entry must live in the
    // routed world and be visible to the caller).
    let mut cited = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
        "wld_test",
        nexus_contracts::BlockType::Character,
        "Boxed Marble",
    );
    cited.entry_id = "kb_bo".to_string();
    cited.status = "confirmed".to_string();
    nexus_knowledge::world_kb::KbStore::insert_knowledge_entry(
        &nexus_local_db::kb_store::SqliteKbStore::new(pool.clone()),
        cited,
    )
    .await
    .unwrap();

    let adapter = scoped(pool.clone());
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

    let adapter = scoped(pool.clone());
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

    let adapter = scoped(pool.clone());
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

    let adapter = scoped(pool.clone());
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

    let adapter = scoped(pool.clone());
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

// ── v1.191 P1 T8: findings are written inside the bound selection ────────

/// `v1191_holder_ports`: a world-scoped finding must cite a world the caller's
/// selection authorizes, and its target entry must be one the caller can read.
/// A target the caller cannot see takes the same unknown-target branch as an
/// absent id, so findings cannot probe for hidden rows.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v1191_holder_ports_world_findings_stay_inside_the_bound_selection() {
    let (pool, _dir) = fresh_pool().await;
    seed_world(&pool, "wld_test").await;
    seed_world(&pool, "wld_foreign").await;

    let adapter = scoped(pool.clone());

    // (a) A world outside the bound selection is refused outright.
    let mut foreign = world_finding("fnd_foreign_world");
    if let Some(ext) = foreign.extensions.get_mut(
        &spoke_schemas::finding::FindingExtensionsKey::try_from("nexus")
            .expect("nexus namespace key"),
    ) {
        ext.insert(
            "world_id".to_string(),
            Value::String("wld_foreign".to_string()),
        );
    }
    match adapter.put_findings(vec![foreign]).await {
        SpokeResult::Reject(r) => {
            assert_eq!(r.code, SpokeRejectCode::InvalidInput, "got {r:?}");
            assert!(
                r.message.contains("outside the bound read selection"),
                "the refusal must name the selection boundary: {r:?}"
            );
        }
        SpokeResult::Ok(_) => panic!("a foreign world finding must reject"),
    }
    assert!(
        get_world_finding(&pool, "fnd_foreign_world")
            .await
            .unwrap()
            .is_none(),
        "a refused world finding must persist nothing"
    );

    // (b) A target entry the caller cannot read (here: absent) is refused with
    // the unknown-target shape, and nothing is written for that finding.
    let mut unknown_target = world_finding("fnd_unknown_target");
    unknown_target.target_entry_id = Some("kb_not_visible".to_string());
    match adapter.put_findings(vec![unknown_target]).await {
        SpokeResult::Reject(r) => {
            assert_eq!(r.code, SpokeRejectCode::InvalidInput, "got {r:?}");
            assert!(
                r.message.contains("unknown knowledge entry"),
                "the refusal must use the unknown-target shape: {r:?}"
            );
        }
        SpokeResult::Ok(_) => panic!("an unreadable target must reject"),
    }
    assert!(
        get_world_finding(&pool, "fnd_unknown_target")
            .await
            .unwrap()
            .is_none(),
        "a refused target finding must persist nothing"
    );
}

/// `v1191_holder_ports` (L2 F4): a multi-world selection admits entries from
/// several containers, so the world route must additionally require the target
/// entry to live in the *routed* world — a cross-world target is refused with
/// the same unknown-target shape.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v1191_holder_ports_cross_world_target_is_refused() {
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::{KnowledgeEntryRecord, KnowledgeOwnerRef};
    use nexus_knowledge::world_kb::KbStore;
    use nexus_local_db::kb_store::SqliteKbStore;

    let (pool, _dir) = fresh_pool().await;
    seed_world(&pool, "wld_test").await;
    seed_world(&pool, "wld_foreign").await;

    // An entry that IS visible to the caller (both worlds authorized) but
    // lives in another world than the finding's route.
    let mut foreign = KnowledgeEntryRecord::new("wld_foreign", BlockType::Character, "ForeignNote");
    foreign.entry_id = "kb_foreign_note".to_string();
    SqliteKbStore::new(pool.clone())
        .insert_knowledge_entry(foreign)
        .await
        .unwrap();

    let both = nexus_spoke_adapter::NexusAdapter::new(
        pool.clone(),
        nexus_knowledge::world_kb::KnowledgeReadScope::creator_management(
            vec![
                KnowledgeOwnerRef::world("wld_test"),
                KnowledgeOwnerRef::world("wld_foreign"),
            ],
            Vec::new(),
        ),
    );

    let mut cross = world_finding("fnd_cross_world");
    cross.target_entry_id = Some("kb_foreign_note".to_string());
    match both.put_findings(vec![cross]).await {
        SpokeResult::Reject(r) => {
            assert_eq!(r.code, SpokeRejectCode::InvalidInput, "got {r:?}");
            assert!(
                r.message.contains("unknown knowledge entry"),
                "a cross-world target must use the unknown-target shape: {r:?}"
            );
        }
        SpokeResult::Ok(_) => panic!("a cross-world target must be refused"),
    }
    assert!(
        get_world_finding(&pool, "fnd_cross_world")
            .await
            .unwrap()
            .is_none(),
        "a refused cross-world finding must persist nothing"
    );
}

/// `v1191_holder_ports` (T8 CI product-scope fix, option a): a rule finding may
/// target one of the routed world's TIMELINE EVENTS (`rules_eval.rs`
/// `observer_cardinality`), while a knowledge-entry target still has to be an
/// admitted entry of that world.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v1191_holder_ports_same_world_timeline_target_accepted_cross_world_refused() {
    use nexus_local_db::narrative_gateway::seed;

    let (pool, _dir) = fresh_pool().await;
    seed_world(&pool, "wld_test").await;
    seed_world(&pool, "wld_foreign").await;
    seed::event(
        &pool,
        "evt_home",
        "wld_test",
        "fbk_root",
        "story_advance",
        0,
    )
    .await;
    seed::event(
        &pool,
        "evt_abroad",
        "wld_foreign",
        "fbk_root",
        "story_advance",
        0,
    )
    .await;

    let adapter = scoped(pool.clone());

    // (a) Same-world timeline target: accepted and visible on the stored row.
    let mut same_world = world_finding("fnd_evt_home");
    same_world.target_entry_id = Some("evt_home".to_string());
    match adapter.put_findings(vec![same_world]).await {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(r) => panic!("a same-world timeline target must be accepted: {r:?}"),
    }
    let stored = get_world_finding(&pool, "fnd_evt_home")
        .await
        .unwrap()
        .expect("the accepted finding is persisted");
    assert_eq!(
        stored.target_entry_id.as_deref(),
        Some("evt_home"),
        "the rule target round-trips onto the row"
    );

    // (b) Cross-world timeline target: refused, nothing persisted.
    let mut cross = world_finding("fnd_evt_abroad");
    cross.target_entry_id = Some("evt_abroad".to_string());
    match adapter.put_findings(vec![cross]).await {
        SpokeResult::Reject(r) => {
            assert_eq!(r.code, SpokeRejectCode::InvalidInput, "got {r:?}");
            assert!(
                r.message.contains("unknown knowledge entry"),
                "a cross-world timeline target must use the unknown-target shape: {r:?}"
            );
        }
        SpokeResult::Ok(_) => panic!("a cross-world timeline target must be refused"),
    }
    assert!(
        get_world_finding(&pool, "fnd_evt_abroad")
            .await
            .unwrap()
            .is_none(),
        "a refused finding must persist nothing"
    );
}
