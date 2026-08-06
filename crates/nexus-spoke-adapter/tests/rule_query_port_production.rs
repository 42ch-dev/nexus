//! Production `RuleQueryPort::list_rules` integration test (V1.148 P1) —
//! end-to-end through `NexusAdapter` against REAL `SQLite` storage.
//!
//! Closes `R-V1142P1-001` (production `RuleQueryPort` per spec §7.4). The
//! `#[cfg(test)]` unit tests in `src/adapter/rule_query_port.rs` prove the
//! row → wire projection; this file proves the *production semantics* the
//! plan's T3 section specifies against a migrated temp DB:
//!
//! 1. **Cross-world isolation** — `list_rules` resolves `rule_refs` by
//!    `rule_id` only (spoke `list_rules` has no world parameter); a rule that
//!    lives in another world's rows is returned **only when requested**.
//! 2. **Missing refs omitted, not an error** — `list_rules(&[known, missing,
//!    known])` returns exactly the known subset (spoke empty-subset semantics).
//! 3. **Empty ref-set → `Ok(vec![])`** — no DB dependency, no error.
//! 4. **No Work-side fabrication** — rows are sourced from `spoke_rules`
//!    only. Even when the Work-side rule stores (quality-loop
//!    `Works/<work_ref>/AGENTS.md` and `narrative_worlds.world_rules_json`)
//!    carry rule-shaped bait referencing a requested `rule_id`, `list_rules`
//!    returns nothing for it.
//! 5. **Duplicate refs dedup** — `SQLite` `IN (subquery)` membership semantics
//!    yield one row per distinct `rule_id` (documented contract of
//!    `get_spoke_rules_by_ids`).
//!
//! # Harness pattern
//!
//! Mirrors `spoke_orchestrator_integration.rs` / the production adapter's own
//! `#[cfg(test)] mod tests`: `tempfile::tempdir` + `open_pool` +
//! `run_migrations`, seed through the local-db `insert_spoke_rule_for_test`
//! helper (the only rule writer in P1 — no author-facing write API exists).
//!
//! # Call-boundary invariant (spec §7)
//!
//! The test drives `list_rules` through `&NexusAdapter` only — no spoke
//! invariant is reimplemented here; the adapter IS the boundary.

#![allow(clippy::unwrap_used)]

use nexus_local_db::spoke_rules::{insert_spoke_rule_for_test, SpokeRuleRow};
use nexus_local_db::{open_pool, run_migrations};
use nexus_spoke_adapter::{NexusAdapter, Rule, RuleQueryPort, SpokeResult};

// ── Pool + fixture helpers ───────────────────────────────────────────────

/// Fresh temp pool: `tempfile::tempdir` + `open_pool` + `run_migrations`.
///
/// Returns the pool AND the `TempDir` guard so the temp DB stays alive for
/// the test body (mirrors `spoke_orchestrator_integration.rs::fresh_pool`).
async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = open_pool(&db_path).await.unwrap();
    run_migrations(&pool).await.unwrap();
    (pool, dir)
}

/// Seed a `spoke_rules` row through the local-db test helper.
async fn seed_rule(pool: &sqlx::SqlitePool, rule_id: &str, world_id: &str) {
    let row = SpokeRuleRow {
        rule_id: rule_id.to_string(),
        world_id: world_id.to_string(),
        schema_version: 1,
        canonical_name: format!("Rule {rule_id}"),
        kind: "rule".to_string(),
        statement: Some(format!("Statement for {rule_id}")),
        description: None,
        target_entry_types_json: "[\"character\", \"event\"]".to_string(),
        severity_hint: Some("warning".to_string()),
        status: Some("active".to_string()),
        source_anchor_json: None,
        extensions_json: "{}".to_string(),
        created_at: Some(1_700_000_000),
        updated_at: None,
    };
    insert_spoke_rule_for_test(pool, &row).await.unwrap();
}

/// Seed a `narrative_worlds` row carrying Work-side rule bait
/// (`world_rules_json`) — the legacy World-side rule store `list_rules` MUST
/// NOT read. Needs the `creators` FK parent (`owner_creator_id`), same as
/// `spoke_orchestrator_integration.rs::seed_world`.
async fn seed_world_with_rules_bait(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    world_rules_json: &str,
) {
    // SAFETY: test-only static seed inserts against the post-migration schema.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('ctr_test', 'Test', 'active', datetime('now'), '{}')",
    )
    .execute(pool)
    .await
    .unwrap();
    // SAFETY: test-only static seed inserts against the post-migration schema.
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, world_rules_json, metadata_json) \
         VALUES (?, 'wrk_test', 'ctr_test', 'Test World', 'test-world', 'active', \
                 'private', 'manual', ?, '{}')",
    )
    .bind(world_id)
    .bind(world_rules_json)
    .execute(pool)
    .await
    .unwrap();
}

/// Test helper: unwrap a `SpokeResult::Ok(Vec<Rule>)` or panic with the
/// reject payload.
fn unwrap_ok(result: SpokeResult<Vec<Rule>>, label: &str) -> Vec<Rule> {
    match result {
        SpokeResult::Ok(v) => v,
        SpokeResult::Reject(r) => panic!("{label}: expected ok, got reject {r:?}"),
    }
}

/// Sort rule ids for order-independent comparison.
fn sorted_ids(rules: &[Rule]) -> Vec<&str> {
    let mut ids: Vec<&str> = rules.iter().map(|r| r.rule_id.as_str()).collect();
    ids.sort_unstable();
    ids
}

// ── 1. Cross-world + missing refs (plan T3 core scenario) ────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_rules_returns_exactly_requested_rules_across_worlds() {
    let (pool, _dir) = fresh_pool().await;
    // World A: two rules with distinct ids.
    seed_rule(&pool, "rule_a1", "wld_a").await;
    seed_rule(&pool, "rule_a2", "wld_a").await;
    // World B: a third rule that must NOT leak into the result unasked.
    seed_rule(&pool, "rule_b1", "wld_b").await;

    let adapter = NexusAdapter::new(pool);
    let rules = unwrap_ok(
        adapter
            .list_rules(&[
                "rule_a1".to_string(),
                "rule_missing".to_string(),
                "rule_a2".to_string(),
            ])
            .await,
        "list_rules",
    );

    // Missing ref omitted (empty subset, NOT an error — spoke semantics).
    assert_eq!(rules.len(), 2, "missing ref must be omitted");
    assert_eq!(
        sorted_ids(&rules),
        vec!["rule_a1", "rule_a2"],
        "order-independent exact subset"
    );
    assert!(
        !rules.iter().any(|r| r.rule_id == "rule_b1"),
        "world B rule must not be returned when not requested"
    );

    // Spot-check the production row → spoke `Rule` projection on one row
    // (full field map is covered by the unit tests).
    let rule_a1 = rules.iter().find(|r| r.rule_id == "rule_a1").unwrap();
    assert_eq!(rule_a1.canonical_name.to_string(), "Rule rule_a1");
    assert_eq!(rule_a1.schema_version.get(), 1);
}

// ── 2. Empty ref-set ─────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_rules_empty_refs_returns_empty_vec() {
    let (pool, _dir) = fresh_pool().await;
    seed_rule(&pool, "rule_a1", "wld_a").await;

    let adapter = NexusAdapter::new(pool);
    let rules = unwrap_ok(adapter.list_rules(&[]).await, "list_rules empty");
    assert!(
        rules.is_empty(),
        "empty refs must return Ok(vec![]) without error"
    );
}

// ── 3. No Work-side fabrication ──────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_rules_does_not_fabricate_from_work_side_rule_sources() {
    let (pool, dir) = fresh_pool().await;
    seed_rule(&pool, "rule_a1", "wld_a").await;

    // Bait 1 — legacy World-side rule store: a `narrative_worlds` row whose
    // `world_rules_json` contains a rule-shaped object carrying the SAME
    // requested id (`rule_phantom`). If `list_rules` consulted this store,
    // the phantom would come back.
    seed_world_with_rules_bait(
        &pool,
        "wld_a",
        r#"[{"rule_id": "rule_phantom", "canonical_name": "Phantom Rule", "kind": "rule"}]"#,
    )
    .await;

    // Bait 2 — Work quality-loop store: `Works/<work_ref>/AGENTS.md` (V1.48
    // P2 Layer 2 location) carrying an accepted rule suggestion for the same
    // phantom id. `NexusAdapter` holds only a `SqlitePool` — it has no
    // filesystem path — so this documents that the boundary cannot observe
    // Work-side rule findings at all.
    let work_agents_md = dir
        .path()
        .join("Works")
        .join("rule-test-work")
        .join("AGENTS.md");
    std::fs::create_dir_all(work_agents_md.parent().unwrap()).unwrap();
    std::fs::write(
        &work_agents_md,
        "# AGENTS.md — rule-test-work\n\n## Accepted rule suggestions\n\n- (rule_phantom) Never fabricate rows.\n",
    )
    .unwrap();

    let adapter = NexusAdapter::new(pool);

    // Phantom id requested alongside a real id: only the `spoke_rules` row
    // comes back.
    let rules = unwrap_ok(
        adapter
            .list_rules(&["rule_a1".to_string(), "rule_phantom".to_string()])
            .await,
        "list_rules phantom + real",
    );
    assert_eq!(
        sorted_ids(&rules),
        vec!["rule_a1"],
        "world_rules_json / AGENTS.md bait must not produce phantom rows"
    );

    // Phantom id requested alone: resolves to the empty subset.
    let rules = unwrap_ok(
        adapter.list_rules(&["rule_phantom".to_string()]).await,
        "list_rules phantom only",
    );
    assert!(
        rules.is_empty(),
        "a rule id that exists only in Work-side stores must resolve to nothing"
    );
}

// ── 4. Duplicate refs (documented contract) ──────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_rules_duplicate_refs_are_deduplicated() {
    let (pool, _dir) = fresh_pool().await;
    seed_rule(&pool, "rule_a1", "wld_a").await;
    seed_rule(&pool, "rule_a2", "wld_a").await;

    let adapter = NexusAdapter::new(pool);
    // SQLite `IN (subquery)` membership semantics dedup: the
    // `get_spoke_rules_by_ids` doc contract ("duplicate ids in the input are
    // deduplicated; one row per distinct `rule_id`") holds end-to-end through
    // the production port.
    let rules = unwrap_ok(
        adapter
            .list_rules(&[
                "rule_a1".to_string(),
                "rule_a1".to_string(),
                "rule_a2".to_string(),
            ])
            .await,
        "list_rules duplicates",
    );
    assert_eq!(
        sorted_ids(&rules),
        vec!["rule_a1", "rule_a2"],
        "duplicate refs must not duplicate rows"
    );
}
