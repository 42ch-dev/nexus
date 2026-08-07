//! Production orchestrator integration test — proves `orchestrate_upsert` +
//! `orchestrate_promote` work end-to-end through `NexusAdapter` against
//! REAL `SQLite` storage (in-memory pool + migrations), not the V1.141 mock.
//!
//! Closes `R-V1141P1-001` — the deferred production port impl + orchestrator
//! end-to-end proof.
//!
//! # What this proves
//!
//! 1. The production `NexusAdapter` (six baseline port families backing
//!    real `SQLite` storage in `kb_key_blocks`) is genuinely consumable by
//!    spoke's Surface B orchestrators.
//! 2. The CAS revision lifecycle works through both orchestrators end-to-end
//!    against real storage (create → revision 1; update → CAS bump; stale
//!    reject; promote → confirmed + bump).
//! 3. The orchestrator return path is NOT trusted blindly — every scenario
//!    re-reads the row from `kb_key_blocks` via INDEPENDENT direct sqlx queries
//!    so storage mutation is proven, not implied.
//!
//! # Call-boundary invariant (spec §7, preserved)
//!
//! The test calls orchestrators through `&NexusAdapter` only — no spoke
//! invariant is reimplemented here. The adapter IS the boundary.
//!
//! # Harness pattern
//!
//! Mirrors the existing `crates/nexus-local-db/tests/` pattern
//! (`tempfile::tempdir` + `open_pool` + `run_migrations` + seed FK parents),
//! matching the production adapter's own `#[cfg(test)] mod tests` in
//! `nexus-spoke-adapter/src/adapter/knowledge_entry_port.rs` (V1.145 P1b
//! rehome).

#![allow(clippy::unwrap_used)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::{WorldKbBody, WorldKbEntry};
use nexus_local_db::{open_pool, run_migrations};
// V1.145 P1b — adapter rehomed to nexus-spoke-adapter (spec §7.4).
use nexus_spoke_adapter::NexusAdapter;
use nexus_spoke_adapter::{
    orchestrate_promote, orchestrate_relate, orchestrate_upsert, PromoteRequest, PromoteResponse,
    RelateRequest, SpokeRejectCode, SpokeResult, UpsertRequest, UpsertResponse,
};
use serde_json::json;
use sqlx::Row;

const WORLD_ID: &str = "wld_1";

// ── Pool + fixture helpers ───────────────────────────────────────────────

/// Fresh in-memory-ish pool: `tempfile::tempdir` + `open_pool` + `run_migrations`.
///
/// Mirrors the canonical nexus-local-db test harness pattern (see
/// `tests/kb_extract_jobs_upsert.rs::fresh_pool` and the production adapter's
/// own `fresh_pool` in `src/spoke_adapter/knowledge_entry_port.rs`). Returns
/// the pool AND the `TempDir` guard so the temp DB stays alive for the test
/// body.
async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = open_pool(&db_path).await.unwrap();
    run_migrations(&pool).await.unwrap();
    seed_world(&pool).await;
    (pool, dir)
}

/// Seed the FK parents (`creators`, `narrative_worlds`) the `kb_key_blocks`
/// FK requires. One row each — sufficient for any `kb_key_blocks` insert
/// driven through the orchestrator.
async fn seed_world(pool: &sqlx::SqlitePool) {
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
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) \
         VALUES ('wld_1', 'wrk_test', 'ctr_test', 'Test World', 'test-world', 'active', 'private', 'manual', '{}')",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Build a spoke `KnowledgeEntry` via the production `WorldKbEntry → spoke`
/// conversion seam (spec §7.1) so it satisfies the `kb_key_blocks` storage
/// shape (`world_id` under `extensions.nexus`, `canonical_name` format-valid,
/// `entry_type` derived from `BlockType`). Mirrors the production adapter's
/// own `spoke_entry` fixture helper.
fn spoke_entry(
    entry_id: &str,
    canonical_name: &str,
    revision: Option<u64>,
    status: &str,
) -> nexus_spoke_adapter::KnowledgeEntry {
    let mut world = WorldKbEntry::new(WORLD_ID, BlockType::Character, canonical_name);
    world.entry_id = entry_id.to_string();
    world.revision = revision;
    world.status = status.to_string();
    world.body = Some(WorldKbBody {
        summary: Some(format!("{canonical_name} summary")),
        ..Default::default()
    });
    nexus_spoke_adapter::conversion::world_kb_to_spoke(&world)
}

/// Build an `UpsertRequest` from a single spoke `KnowledgeEntry`. The entry is
/// serialized to wire JSON and re-deserialized as the orchestrator's request
/// shape — this mirrors the programmatic Surface B twin at
/// `nexus-spoke-adapter/tests/orchestration_adoption.rs::upsert_request`, just
/// sourcing the candidate from the production conversion seam instead of
/// hand-rolled JSON.
fn upsert_request(entry: &nexus_spoke_adapter::KnowledgeEntry) -> UpsertRequest {
    let wire = serde_json::to_value(entry).expect("KnowledgeEntry serializable");
    serde_json::from_value(json!({ "knowledge_entries": [wire] }))
        .expect("valid UpsertRequest fixture")
}

/// Build a `PromoteRequest` from a single spoke `KnowledgeEntry` candidate.
/// See [`upsert_request`] for the wire round-trip rationale.
fn promote_request(entry: &nexus_spoke_adapter::KnowledgeEntry) -> PromoteRequest {
    let wire = serde_json::to_value(entry).expect("KnowledgeEntry serializable");
    serde_json::from_value(json!({ "candidate": wire })).expect("valid PromoteRequest fixture")
}

/// INDEPENDENT direct sqlx read of a `kb_key_blocks` row.
///
/// This is the post-state verification seam: it does NOT go through the
/// orchestrator or the adapter, so it proves the storage was actually mutated
/// (rather than trusting the orchestrator's return value).
///
/// Returns `(canonical_name, status, revision, world_id, body_json)` for the
/// row. Panics if the row is absent — every test scenario asserts presence.
async fn read_kb_row(
    pool: &sqlx::SqlitePool,
    entry_id: &str,
) -> (String, String, i64, String, String) {
    // SAFETY: test-only verification query against the post-migration
    // kb_key_blocks schema (migration 20260525 + provenance + extensions_nexus
    // columns). COALESCE(revision, 0) NULL-normalizes per V1.73 CAS rule.
    let row = sqlx::query(
        "SELECT canonical_name, status, COALESCE(revision, 0), world_id, \
         COALESCE(body_json, '{}') \
         FROM kb_key_blocks WHERE key_block_id = ?",
    )
    .bind(entry_id)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("read back {entry_id}: {e}"));
    (
        row.get::<String, _>(0),
        row.get::<String, _>(1),
        row.get::<i64, _>(2),
        row.get::<String, _>(3),
        row.get::<String, _>(4),
    )
}

/// Assert a `SpokeResult` is a reject carrying the expected code; panic with
/// context otherwise.
fn expect_reject_with_code<T: std::fmt::Debug>(result: SpokeResult<T>, code: SpokeRejectCode) {
    match result {
        SpokeResult::Reject(reject) => {
            assert_eq!(
                reject.code, code,
                "reject code mismatch (message: {})",
                reject.message
            );
        }
        SpokeResult::Ok(_) => panic!("expected reject {code:?}, got Ok"),
    }
}

// ── 1. orchestrate_upsert — happy create ─────────────────────────────────
//
// Create path: orchestrator loads (NotFound), validates `validate_create_path`
// (candidate revision None OK), derives `expected_base_revision = None`, calls
// the adapter's `put_knowledge_entry(candidate, None)`. Production adapter
// inserts and normalizes revision to 1 (V1.73 NULL-normalization rule).

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orchestrate_upsert_happy_create() {
    let (pool, _dir) = fresh_pool().await;
    let adapter = NexusAdapter::new(pool.clone());

    let entry_id = "kb_create_happy";
    let candidate = spoke_entry(entry_id, "CreateHappy", None, "provisional");
    let request = upsert_request(&candidate);

    let result = orchestrate_upsert(&adapter, request).await;
    match result {
        SpokeResult::Ok(UpsertResponse::Variant0 {
            knowledge_entries, ..
        }) => {
            assert_eq!(knowledge_entries.len(), 1, "single entry upserted");
            assert_eq!(knowledge_entries[0].entry_id, entry_id);
            assert_eq!(
                knowledge_entries[0].revision,
                Some(1),
                "post-create revision must be 1 (V1.73 NULL-normalization)"
            );
        }
        _ => panic!("expected upsert success, got {result:?}"),
    }

    // INDEPENDENT post-state verification: read row directly from kb_key_blocks
    // — proves the storage was mutated, not just the orchestrator return value.
    let (name, status, rev, world_id, body) = read_kb_row(&pool, entry_id).await;
    assert_eq!(name, "CreateHappy");
    assert_eq!(status, "provisional");
    assert_eq!(rev, 1, "DB revision must be 1 after create");
    assert_eq!(world_id, WORLD_ID, "extensions.nexus.world_id persisted");
    assert!(
        body.contains("CreateHappy summary"),
        "body_json must carry the candidate summary: {body}"
    );
}

// ── 2. orchestrate_upsert — happy update (CAS bump) ──────────────────────
//
// Create → revision 1. Then upsert-update with candidate carrying revision =
// stored revision (1). Orchestrator's `validate_update_path` calls
// `assert_revision_match(1, 1)` OK; derives `expected_base_revision = Some(1)`;
// adapter CAS accepts and bumps to 2.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orchestrate_upsert_happy_update() {
    let (pool, _dir) = fresh_pool().await;
    let adapter = NexusAdapter::new(pool.clone());

    // Create → stored at revision 1.
    let entry_id = "kb_update_happy";
    let created = spoke_entry(entry_id, "UpdateHappy", None, "provisional");
    let create_result = orchestrate_upsert(&adapter, upsert_request(&created)).await;
    assert!(
        matches!(create_result, SpokeResult::Ok(_)),
        "create must succeed first"
    );

    // Build candidate carrying revision = Some(1) (matches stored), flip
    // status provisional → confirmed (valid transition per the cross-product
    // table) and tweak canonical_name to prove row mutation beyond just the
    // revision bump.
    let updated = spoke_entry(entry_id, "UpdateHappy Revised", Some(1), "confirmed");
    let result = orchestrate_upsert(&adapter, upsert_request(&updated)).await;
    match result {
        SpokeResult::Ok(UpsertResponse::Variant0 {
            knowledge_entries, ..
        }) => {
            assert_eq!(knowledge_entries.len(), 1);
            assert_eq!(knowledge_entries[0].entry_id, entry_id);
            assert_eq!(
                knowledge_entries[0].revision,
                Some(2),
                "CAS update must bump revision 1 → 2"
            );
            assert_eq!(knowledge_entries[0].status, "confirmed");
        }
        _ => panic!("expected upsert success, got {result:?}"),
    }

    // INDEPENDENT verification: row was mutated (name + status + revision).
    let (name, status, rev, world_id, _body) = read_kb_row(&pool, entry_id).await;
    assert_eq!(name, "UpdateHappy Revised", "canonical_name was replaced");
    assert_eq!(status, "confirmed", "status was flipped via CAS update");
    assert_eq!(rev, 2, "DB revision bumped to 2");
    assert_eq!(world_id, WORLD_ID, "world_id preserved across update");
}

// ── 3. orchestrate_upsert — stale reject (STORED_REVISION_STALE) ──────────
//
// Create → revision 1. Bump it (revision → 2). Then attempt another upsert
// with a candidate carrying revision 1 (caller read a stale base). The
// orchestrator loads stored (revision 2), `validate_update_path` →
// `assert_revision_match(1, 2)` fires `STORED_REVISION_STALE` from the
// orchestrator's pre-flight check before reaching the adapter's CAS. Row is
// NOT mutated.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orchestrate_upsert_stale_reject() {
    let (pool, _dir) = fresh_pool().await;
    let adapter = NexusAdapter::new(pool.clone());

    // Create → revision 1.
    let entry_id = "kb_stale_reject";
    let created = spoke_entry(entry_id, "StaleReject", None, "provisional");
    let _ = orchestrate_upsert(&adapter, upsert_request(&created)).await;
    // Bump → revision 2 (happy update with matching revision).
    let bumped = spoke_entry(entry_id, "StaleReject", Some(1), "provisional");
    let bump_result = orchestrate_upsert(&adapter, upsert_request(&bumped)).await;
    assert!(
        matches!(bump_result, SpokeResult::Ok(_)),
        "first update must succeed to advance the stored revision"
    );

    // Stale candidate: revision 1 < stored 2.
    let stale_candidate = spoke_entry(entry_id, "StaleReject", Some(1), "provisional");
    let result = orchestrate_upsert(&adapter, upsert_request(&stale_candidate)).await;
    expect_reject_with_code(result, SpokeRejectCode::StoredRevisionStale);

    // INDEPENDENT verification: row is unchanged at revision 2 (the stale
    // candidate did NOT mutate storage).
    let (name, status, rev, _world_id, _body) = read_kb_row(&pool, entry_id).await;
    assert_eq!(name, "StaleReject");
    assert_eq!(status, "provisional");
    assert_eq!(
        rev, 2,
        "stored revision must remain 2 (stale reject did not mutate)"
    );
}

// ── 4. orchestrate_promote — happy path (provisional → confirmed) ────────
//
// Create a provisional entry → revision 1. Then `orchestrate_promote` with a
// candidate carrying revision = Some(1). Orchestrator's
// `assert_revision_match(1, 1)` passes; `validate_promote_request` accepts the
// provisional candidate; `apply_promote_acceptance` sets status = confirmed;
// orchestrator overrides the accepted revision to `stored + 1` (= 2); adapter
// CAS accepts put(confirmed@2, expected=Some(1)) and bumps to 2. Persisted row
// is `status = confirmed`, `revision = 2`.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orchestrate_promote_happy() {
    let (pool, _dir) = fresh_pool().await;
    let adapter = NexusAdapter::new(pool.clone());

    // Create a provisional entry → stored at revision 1.
    let entry_id = "kb_promote_happy";
    let created = spoke_entry(entry_id, "PromoteHappy", None, "provisional");
    let create_result = orchestrate_upsert(&adapter, upsert_request(&created)).await;
    assert!(
        matches!(create_result, SpokeResult::Ok(_)),
        "create must succeed before promote"
    );

    // Build promote candidate carrying revision = Some(1), status = provisional
    // (validate_promote_request enforces provisional).
    let candidate = spoke_entry(entry_id, "PromoteHappy", Some(1), "provisional");
    let result = orchestrate_promote(&adapter, promote_request(&candidate)).await;
    match result {
        SpokeResult::Ok(PromoteResponse::Variant0 {
            knowledge_entry, ..
        }) => {
            assert_eq!(knowledge_entry.entry_id, entry_id);
            assert_eq!(
                knowledge_entry.status, "confirmed",
                "promote acceptance must flip status to confirmed"
            );
            assert_eq!(
                knowledge_entry.revision,
                Some(2),
                "promote must bump revision 1 → 2"
            );
        }
        _ => panic!("expected promote success, got {result:?}"),
    }

    // INDEPENDENT verification: row is confirmed at revision 2.
    let (name, status, rev, _world_id, _body) = read_kb_row(&pool, entry_id).await;
    assert_eq!(name, "PromoteHappy", "canonical_name preserved by promote");
    assert_eq!(status, "confirmed", "DB status flipped to confirmed");
    assert_eq!(rev, 2, "DB revision bumped to 2 via promote CAS path");
}

// ── 5. (stretch) orchestrate_assemble — scope-filtered happy path ────────
//
// Create two entries in world wld_1. Assemble with a scope filtering to one
// entry id. Verify the packet contains exactly that entry — proving the
// ScopeQueryPort production impl returns rows from real SQLite storage and
// spoke's scope helpers filter them through the orchestrator.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orchestrate_assemble_scope_filtered() {
    use nexus_spoke_adapter::{orchestrate_assemble, AssembleResponse};

    let (pool, _dir) = fresh_pool().await;
    let adapter = NexusAdapter::new(pool);

    // Create two entries in wld_1.
    let a = spoke_entry("kb_assemble_a", "AssembleA", None, "provisional");
    let b = spoke_entry("kb_assemble_b", "AssembleB", None, "provisional");
    let _ = orchestrate_upsert(&adapter, upsert_request(&a)).await;
    let _ = orchestrate_upsert(&adapter, upsert_request(&b)).await;

    // Scope filters to entry A only.
    let request = serde_json::from_value(json!({
        "scope": { "scope_id": WORLD_ID, "entry_ids": ["kb_assemble_a"] },
        "max_entries": 10
    }))
    .expect("valid AssembleRequest fixture");

    let result = orchestrate_assemble(&adapter, request).await;
    match result {
        SpokeResult::Ok(AssembleResponse::Variant0 { packet, .. }) => {
            assert_eq!(
                packet.packet_id,
                format!("assemble:{WORLD_ID}"),
                "packet_id derived from scope_id"
            );
            assert_eq!(packet.entries.len(), 1, "scope filter selected one entry");
            assert_eq!(packet.entries[0].entry_id, "kb_assemble_a");
        }
        _ => panic!("expected assemble success, got {result:?}"),
    }
}

// ── 6. R3 closure: world-aware CAS (spec §3) ────────────────────────────
//
// The N-C1 invoke gate's stored-world check is check-then-act. The durable
// fix is the orchestrator/storage CAS carrying the stored `world_id`. These
// tests reproduce the interleaved two-writer race at the orchestrator layer
// (the atomic source of truth): writer 1's world-verified preimage is
// invalidated by writer 2 moving the row to another world between the
// gate-check and the CAS. The CAS must deny with the adapter's world-conflict
// classification — never `REVISION_CONFLICT` / `STORED_REVISION_STALE`.

/// Seed a second world row so a test can FK-move rows across worlds.
async fn seed_second_world(pool: &sqlx::SqlitePool) {
    // SAFETY: test-only static seed insert against the post-migration schema.
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) \
         VALUES ('wld_2', 'wrk_test', 'ctr_test', 'Second World', 'second-world', 'active', 'private', 'manual', '{}')",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Build a spoke `Relation` fixture carrying `extensions.nexus.world_id`
/// (required by the production adapter's persist path).
fn relate_relation(relation_id: &str, from_id: &str, to_id: &str) -> nexus_spoke_adapter::Relation {
    serde_json::from_value(json!({
        "schema_version": 1,
        "relation_id": relation_id,
        "from_id": from_id,
        "to_id": to_id,
        "relation_type": "allied_with",
        "label": "test edge",
        "metadata": {},
        "extensions": { "nexus": { "world_id": WORLD_ID } },
    }))
    .expect("valid Relation fixture")
}

/// Build a `RelateRequest` from a spoke `Relation` (wire round-trip, mirrors
/// [`upsert_request`]).
fn relate_request(relation: &nexus_spoke_adapter::Relation) -> RelateRequest {
    let wire = serde_json::to_value(relation).expect("Relation serializable");
    serde_json::from_value(json!({ "relation": wire })).expect("valid RelateRequest fixture")
}

/// Assert a reject is the adapter's world-conflict classification (details
/// marker), and specifically NOT a collapse into `RevisionConflict` /
/// `StoredRevisionStale` (spec §3.2).
fn expect_world_conflict_reject<T: std::fmt::Debug>(
    result: SpokeResult<T>,
    expected_world: &str,
    actual_world: &str,
) {
    match result {
        SpokeResult::Reject(reject) => {
            assert_ne!(
                reject.code,
                SpokeRejectCode::RevisionConflict,
                "world conflict must never collapse into REVISION_CONFLICT"
            );
            assert_ne!(
                reject.code,
                SpokeRejectCode::StoredRevisionStale,
                "world conflict must never collapse into STORED_REVISION_STALE"
            );
            // The pinned spoke-operations `SpokeRejectCode` has no
            // conflict-class code; the adapter carries the classification on
            // the `InternalError` carrier with a `world_conflict` details
            // marker, and the host mappings (Connect / daemon) remap it to
            // the fixed `world_conflict` wire code.
            assert_eq!(
                reject.code,
                SpokeRejectCode::InternalError,
                "world-conflict rides the InternalError carrier (message: {})",
                reject.message
            );
            let details = reject
                .details
                .as_ref()
                .expect("world-conflict reject carries details");
            assert_eq!(
                details
                    .get("world_conflict")
                    .and_then(serde_json::Value::as_bool),
                Some(true),
                "world_conflict marker present"
            );
            assert_eq!(
                details
                    .get("expectedWorld")
                    .and_then(serde_json::Value::as_str),
                Some(expected_world)
            );
            assert_eq!(
                details
                    .get("actualWorld")
                    .and_then(serde_json::Value::as_str),
                Some(actual_world)
            );
        }
        SpokeResult::Ok(_) => panic!("expected world-conflict reject, got Ok"),
    }
}

/// Move a `kb_key_blocks` row to another world WITHOUT bumping its revision —
/// the interleaved "other writer" that the pre-fix (id + revision) CAS could
/// not distinguish from a legitimate same-world update.
async fn move_key_block_to_world(pool: &sqlx::SqlitePool, entry_id: &str, world_id: &str) {
    // SAFETY: test-only static UPDATE against the post-migration schema.
    sqlx::query("UPDATE kb_key_blocks SET world_id = ? WHERE key_block_id = ?")
        .bind(world_id)
        .bind(entry_id)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orchestrate_upsert_denies_row_moved_to_another_world_between_verification_and_cas() {
    let (pool, _dir) = fresh_pool().await;
    seed_second_world(&pool).await;
    let adapter = NexusAdapter::new(pool.clone());

    // Writer 1 creates the entry in WORLD_ID → revision 1.
    let entry_id = "kb_wc_upsert";
    let created = spoke_entry(entry_id, "WorldCasUpsert", None, "provisional");
    let create_result = orchestrate_upsert(&adapter, upsert_request(&created)).await;
    assert!(
        matches!(create_result, SpokeResult::Ok(_)),
        "create must succeed first"
    );

    // Gate check passes: the stored row is in the claimed world.
    let (_, _, _, stored_world, _) = read_kb_row(&pool, entry_id).await;
    assert_eq!(
        stored_world, WORLD_ID,
        "gate-check precondition: row is in the claimed world"
    );

    // Writer 2 (a second Connect process / the daemon) moves the row to
    // wld_2 between the gate-check and writer 1's CAS, revision untouched.
    move_key_block_to_world(&pool, entry_id, "wld_2").await;

    // Writer 1 replays its world-A preimage (revision 1 — still the stored
    // revision, so the pre-fix id+revision CAS would have succeeded).
    let candidate = spoke_entry(entry_id, "WorldCasUpsert", Some(1), "provisional");
    let result = orchestrate_upsert(&adapter, upsert_request(&candidate)).await;
    expect_world_conflict_reject(result, WORLD_ID, "wld_2");

    // INDEPENDENT verification: the row was not rewritten — still in wld_2
    // at revision 1 (the denied CAS must leave the row untouched).
    let (_, _, rev, world, _) = read_kb_row(&pool, entry_id).await;
    assert_eq!(
        world, "wld_2",
        "row stays in the world the interleaved writer set"
    );
    assert_eq!(rev, 1, "denied CAS must not mutate the row");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orchestrate_promote_denies_row_moved_to_another_world_between_verification_and_cas() {
    let (pool, _dir) = fresh_pool().await;
    seed_second_world(&pool).await;
    let adapter = NexusAdapter::new(pool.clone());

    // Writer 1 creates a provisional entry in WORLD_ID → revision 1.
    let entry_id = "kb_wc_promote";
    let created = spoke_entry(entry_id, "WorldCasPromote", None, "provisional");
    let create_result = orchestrate_upsert(&adapter, upsert_request(&created)).await;
    assert!(
        matches!(create_result, SpokeResult::Ok(_)),
        "create must succeed first"
    );

    // Gate check passes for WORLD_ID; writer 2 then moves the row to wld_2.
    let (_, _, _, stored_world, _) = read_kb_row(&pool, entry_id).await;
    assert_eq!(stored_world, WORLD_ID);
    move_key_block_to_world(&pool, entry_id, "wld_2").await;

    // Writer 1 promotes its world-A preimage (revision 1).
    let candidate = spoke_entry(entry_id, "WorldCasPromote", Some(1), "provisional");
    let result = orchestrate_promote(&adapter, promote_request(&candidate)).await;
    expect_world_conflict_reject(result, WORLD_ID, "wld_2");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orchestrate_relate_denies_row_moved_to_another_world_between_verification_and_cas() {
    let (pool, _dir) = fresh_pool().await;
    seed_second_world(&pool).await;
    let adapter = NexusAdapter::new(pool.clone());

    // Endpoints must exist in WORLD_ID (kb_relationships FKs on key_block_id).
    for endpoint in ["kb_wc_src", "kb_wc_dst"] {
        let ep = spoke_entry(endpoint, endpoint, None, "confirmed");
        let r = orchestrate_upsert(&adapter, upsert_request(&ep)).await;
        assert!(
            matches!(r, SpokeResult::Ok(_)),
            "endpoint {endpoint} create must succeed"
        );
    }

    // Writer 1 creates the relation in WORLD_ID → revision 1.
    let relation_id = "rel_wc";
    let relation = relate_relation(relation_id, "kb_wc_src", "kb_wc_dst");
    let create_result = orchestrate_relate(&adapter, relate_request(&relation)).await;
    assert!(
        matches!(create_result, SpokeResult::Ok(_)),
        "relate create must succeed first"
    );

    // Gate check passes: the stored row is in the claimed world.
    let stored_world: String =
        sqlx::query_scalar("SELECT world_id FROM kb_relationships WHERE relationship_id = ?")
            .bind(relation_id)
            .fetch_one(&pool)
            .await
            .expect("read persisted relation world");
    assert_eq!(stored_world, WORLD_ID);

    // Writer 2 moves the relation row to wld_2, revision untouched.
    // SAFETY: test-only static UPDATE against the post-migration schema.
    sqlx::query("UPDATE kb_relationships SET world_id = ? WHERE relationship_id = ?")
        .bind("wld_2")
        .bind(relation_id)
        .execute(&pool)
        .await
        .unwrap();

    // Writer 1 replays its world-A preimage (revision 1).
    let mut updated = relation;
    updated.revision = Some(1);
    let result = orchestrate_relate(&adapter, relate_request(&updated)).await;
    expect_world_conflict_reject(result, WORLD_ID, "wld_2");
}
