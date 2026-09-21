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

use async_trait::async_trait;
use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::{KnowledgeEntryBody, KnowledgeEntryRecord};
use nexus_local_db::{open_pool, run_migrations};
// V1.145 P1b — adapter rehomed to nexus-spoke-adapter (spec §7.4).
use nexus_spoke_adapter::NexusAdapter;
use nexus_spoke_adapter::{
    orchestrate_promote, orchestrate_relate, orchestrate_upsert, FindingPort, HostManifestPort,
    KnowledgeEntryPort, PromoteRequest, PromoteResponse, RelateRequest, RelationPort,
    RuleQueryPort, ScopeQueryPort, SpokeRejectCode, SpokeResult, UpsertRequest, UpsertResponse,
};
use serde_json::json;
use sqlx::Row;
use std::sync::Mutex;

const WORLD_ID: &str = "wld_1";

// ── Pool + fixture helpers ───────────────────────────────────────────────

/// Fresh in-memory-ish pool: `tempfile::tempdir` + `open_pool` + `run_migrations`.
///
/// Mirrors the canonical nexus-local-db test harness pattern (see
/// `tests/kb_extract_jobs_upsert.rs::fresh_pool` and the production adapter's
/// own `fresh_pool` in `src/spoke_adapter/knowledge_entry_port.rs`). Returns
/// the pool AND the `TempDir` guard so the temp DB stays alive for the test
/// body.
/// A KE-capable adapter whose selection authorizes the worlds these
/// fixtures own (v1.191 P1 T8 — the check/relate paths read knowledge).
///
/// Both fixture worlds are admitted: the world-conflict regressions need the
/// moved-to world to stay *inside* the request's selection, so the CAS
/// classification (not the hidden-row rule) decides the outcome. The
/// hidden/absent parity regressions build a narrower selection with
/// [`scoped_worlds`].
fn scoped(pool: sqlx::SqlitePool) -> NexusAdapter<'static> {
    scoped_worlds(pool, &[WORLD_ID, "wld_2"])
}

/// A KE-capable adapter bound to an explicit world selection — the admission
/// boundary under test (a row outside it is hidden, never observable).
fn scoped_worlds(pool: sqlx::SqlitePool, worlds: &[&str]) -> NexusAdapter<'static> {
    NexusAdapter::new(
        pool,
        nexus_knowledge::world_kb::KnowledgeReadScope::creator_management(
            worlds
                .iter()
                .map(|world| {
                    nexus_knowledge::world_kb::knowledge_entry::KnowledgeOwnerRef::world(*world)
                })
                .collect(),
            Vec::new(),
        ),
    )
}

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

/// Build a spoke `KnowledgeEntry` via the production `KnowledgeEntryRecord → spoke`
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
    let mut world = KnowledgeEntryRecord::new(WORLD_ID, BlockType::Character, canonical_name);
    world.entry_id = entry_id.to_string();
    world.revision = revision;
    world.status = status.to_string();
    world.body = Some(KnowledgeEntryBody {
        summary: Some(format!("{canonical_name} summary")),
        ..Default::default()
    });
    nexus_spoke_adapter::conversion::knowledge_record_to_spoke(&world)
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
    let adapter = scoped(pool.clone());

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
    let adapter = scoped(pool.clone());

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
    let adapter = scoped(pool.clone());

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
    let adapter = scoped(pool.clone());

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
    let adapter = scoped(pool);

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
// fix is the orchestrator/storage CAS carrying the stored `world_id`:
// writer 1's world-verified preimage is invalidated by writer 2 moving the row
// to another world between that read and the conditional write. The CAS must
// deny with the adapter's world-conflict classification — never
// `REVISION_CONFLICT` / `STORED_REVISION_STALE`.
//
// v1.194 P2-T4 splits the two transitions that used to share these fixtures:
//
// 1. **Initially hidden** — the row was moved outside the request's selection
//    before it was read, so the request never admitted it. It must stay
//    indistinguishable from an absent row: same refusal, no world-conflict
//    classification, no foreign world id. See
//    [`initially_hidden_entry_is_indistinguishable_from_absent`] and
//    [`initially_hidden_relation_is_indistinguishable_from_absent`].
//
// 2. **Admitted, then moved by a second writer** — the request's read admitted
//    the preimage and the second writer moved the row inside the admitted
//    selection afterwards. Only this transition may classify `world_conflict`.
//    Three regressions cover it, driven by [`SecondWriterBarrier`], which fires
//    the second writer's move the instant the admission read returns — an
//    explicit barrier, never a sleep or a timing assumption.
//
// The old fixtures moved the row *before* the read and asserted an untouched
// revision. That is not reproducible: `bump_kb_key_blocks_revision` /
// `bump_kb_relationships_revision` advance the revision on any update that
// leaves it unchanged, so the move is visible to the pre-flight read as a
// stale revision and the CAS classification is never reached. Those fixtures
// measured `STORED_REVISION_STALE`, which is why the residual was misread as
// production folding.

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

/// Move a `kb_key_blocks` row to another world before the request reads it —
/// the **initially hidden** fixture (§7): the row lives outside the request's
/// selection from the first read on, so it must behave exactly like an absent
/// one. Not a race: nothing here is interleaved.
async fn move_key_block_to_world(pool: &sqlx::SqlitePool, entry_id: &str, world_id: &str) {
    // SAFETY: test-only static UPDATE against the post-migration schema.
    sqlx::query("UPDATE kb_key_blocks SET world_id = ? WHERE key_block_id = ?")
        .bind(world_id)
        .bind(entry_id)
        .execute(pool)
        .await
        .unwrap();
}

/// One `kb_relationships` row's observable columns, read directly from the
/// store — every mutable column a leaked CAS write could change (the immutable
/// `source` is excluded; `world_id`/`revision` are the second writer's).
#[derive(Debug, Clone, PartialEq, Eq)]
struct RelationRow {
    world_id: String,
    revision: i64,
    relation_type: String,
    custom_label: Option<String>,
    metadata: Option<String>,
    updated_at: String,
}

/// INDEPENDENT direct sqlx read of a `kb_relationships` row.
async fn read_relation_row(pool: &sqlx::SqlitePool, relation_id: &str) -> RelationRow {
    // SAFETY: test-only verification query against the post-migration
    // kb_relationships schema.
    let row = sqlx::query(
        "SELECT world_id, revision, relation_type, custom_label, metadata, updated_at \
         FROM kb_relationships WHERE relationship_id = ?",
    )
    .bind(relation_id)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("read back relation {relation_id}: {e}"));
    RelationRow {
        world_id: row.get(0),
        revision: row.get(1),
        relation_type: row.get(2),
        custom_label: row.get(3),
        metadata: row.get(4),
        updated_at: row.get(5),
    }
}

/// The `(world_id, revision)` a second writer left on a row — the preimage the
/// denied conditional write must not disturb.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WorldRevision {
    world_id: String,
    revision: i64,
}

/// The table a [`SecondWriterMove`] rewrites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecondWriterTable {
    KeyBlocks,
    Relations,
}

/// One world move by the interleaved "second writer" (a second Connect process
/// / the daemon on the same workspace DB). It carries no revision: the move is
/// a plain world write, and the store's revision-bump trigger advances the row
/// exactly as a foreign writer's own CAS would.
#[derive(Debug, Clone)]
struct SecondWriterMove {
    table: SecondWriterTable,
    ids: Vec<String>,
    world_id: String,
}

impl SecondWriterMove {
    fn key_block(entry_id: &str, world_id: &str) -> Self {
        Self {
            table: SecondWriterTable::KeyBlocks,
            ids: vec![entry_id.to_string()],
            world_id: world_id.to_string(),
        }
    }

    fn relation(relation_id: &str, world_id: &str) -> Self {
        Self {
            table: SecondWriterTable::Relations,
            ids: vec![relation_id.to_string()],
            world_id: world_id.to_string(),
        }
    }
}

/// Deterministic two-writer barrier at the orchestrator's admission read.
///
/// A pass-through wrapper over the production adapter that implements the six
/// baseline port families (delegating every method) and fires one armed
/// [`SecondWriterMove`] **the instant the admission read returns** — i.e.
/// between the request's world-verified preimage read and its conditional
/// write. There is no sleep and no thread-scheduling assumption: the second
/// writer runs on the same task, immediately after the read that admitted the
/// preimage, which is exactly the ordering the CAS has to survive.
///
/// The barrier also records the `(world_id, revision)` its move left behind, so
/// a regression can prove the denied write changed nothing.
struct SecondWriterBarrier<'a> {
    adapter: &'a NexusAdapter<'static>,
    pool: sqlx::SqlitePool,
    pending: Mutex<Option<SecondWriterMove>>,
    left: Mutex<Vec<(String, WorldRevision)>>,
}

impl<'a> SecondWriterBarrier<'a> {
    fn new(
        adapter: &'a NexusAdapter<'static>,
        pool: sqlx::SqlitePool,
        r#move: SecondWriterMove,
    ) -> Self {
        Self {
            adapter,
            pool,
            pending: Mutex::new(Some(r#move)),
            left: Mutex::new(Vec::new()),
        }
    }

    /// The `(world_id, revision)` this barrier's move left on `id`.
    fn left(&self, id: &str) -> WorldRevision {
        self.left
            .lock()
            .expect("barrier state lock")
            .iter()
            .find(|(row_id, _)| row_id == id)
            .map(|(_, state)| state.clone())
            .unwrap_or_else(|| panic!("the second writer never moved {id}"))
    }

    /// Run the armed move (once). A later admission read is a no-op, so the
    /// second writer cannot fire twice.
    async fn fire(&self) {
        let Some(r#move) = self.pending.lock().expect("barrier state lock").take() else {
            return;
        };
        for id in &r#move.ids {
            let state = match r#move.table {
                SecondWriterTable::KeyBlocks => {
                    // SAFETY: test-only static UPDATE against the post-migration
                    // schema; `revision` is deliberately untouched (the bump
                    // trigger advances it, as any foreign writer's would).
                    sqlx::query("UPDATE kb_key_blocks SET world_id = ? WHERE key_block_id = ?")
                        .bind(&r#move.world_id)
                        .bind(id)
                        .execute(&self.pool)
                        .await
                        .expect("second writer moves the key block");
                    let (world_id, revision): (String, i64) = sqlx::query_as(
                        "SELECT world_id, COALESCE(revision, 0) FROM kb_key_blocks \
                         WHERE key_block_id = ?",
                    )
                    .bind(id)
                    .fetch_one(&self.pool)
                    .await
                    .expect("read the state the second writer left");
                    WorldRevision { world_id, revision }
                }
                SecondWriterTable::Relations => {
                    // SAFETY: test-only static UPDATE against the post-migration
                    // schema; see the key-block arm above.
                    sqlx::query(
                        "UPDATE kb_relationships SET world_id = ? WHERE relationship_id = ?",
                    )
                    .bind(&r#move.world_id)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .expect("second writer moves the relation");
                    let (world_id, revision): (String, i64) = sqlx::query_as(
                        "SELECT world_id, revision FROM kb_relationships WHERE relationship_id = ?",
                    )
                    .bind(id)
                    .fetch_one(&self.pool)
                    .await
                    .expect("read the state the second writer left");
                    WorldRevision { world_id, revision }
                }
            };
            self.left
                .lock()
                .expect("barrier state lock")
                .push((id.clone(), state));
        }
    }
}

#[async_trait]
impl KnowledgeEntryPort for SecondWriterBarrier<'_> {
    async fn get_knowledge_entry(
        &self,
        entry_id: &str,
    ) -> SpokeResult<nexus_spoke_adapter::KnowledgeEntry> {
        let read = self.adapter.get_knowledge_entry(entry_id).await;
        if matches!(read, SpokeResult::Ok(_)) {
            self.fire().await;
        }
        read
    }

    async fn put_knowledge_entry(
        &self,
        entry: nexus_spoke_adapter::KnowledgeEntry,
        expected_base_revision: Option<u64>,
    ) -> SpokeResult<nexus_spoke_adapter::KnowledgeEntry> {
        self.adapter
            .put_knowledge_entry(entry, expected_base_revision)
            .await
    }
}

#[async_trait]
impl RelationPort for SecondWriterBarrier<'_> {
    async fn get_relation(&self, relation_id: &str) -> SpokeResult<nexus_spoke_adapter::Relation> {
        let read = self.adapter.get_relation(relation_id).await;
        if matches!(read, SpokeResult::Ok(_)) {
            self.fire().await;
        }
        read
    }

    async fn put_relation(
        &self,
        relation: nexus_spoke_adapter::Relation,
        expected_base_revision: Option<u64>,
    ) -> SpokeResult<nexus_spoke_adapter::Relation> {
        self.adapter
            .put_relation(relation, expected_base_revision)
            .await
    }
}

#[async_trait]
impl ScopeQueryPort for SecondWriterBarrier<'_> {
    async fn list_knowledge_entries(
        &self,
        scope: &nexus_spoke_adapter::Scope,
    ) -> SpokeResult<Vec<nexus_spoke_adapter::KnowledgeEntry>> {
        self.adapter.list_knowledge_entries(scope).await
    }

    async fn list_timeline_events(
        &self,
        scope: &nexus_spoke_adapter::Scope,
    ) -> SpokeResult<Vec<nexus_spoke_adapter::TimelineEvent>> {
        self.adapter.list_timeline_events(scope).await
    }
}

#[async_trait]
impl FindingPort for SecondWriterBarrier<'_> {
    async fn put_findings(
        &self,
        findings: Vec<nexus_spoke_adapter::Finding>,
    ) -> SpokeResult<Vec<nexus_spoke_adapter::Finding>> {
        self.adapter.put_findings(findings).await
    }
}

#[async_trait]
impl RuleQueryPort for SecondWriterBarrier<'_> {
    async fn list_rules(
        &self,
        rule_refs: &[String],
    ) -> SpokeResult<Vec<nexus_spoke_adapter::Rule>> {
        self.adapter.list_rules(rule_refs).await
    }
}

#[async_trait]
impl HostManifestPort for SecondWriterBarrier<'_> {
    async fn get_host_capability_manifest(
        &self,
    ) -> SpokeResult<nexus_spoke_adapter::HostCapabilityManifest> {
        self.adapter.get_host_capability_manifest().await
    }

    async fn list_peer_host_capability_manifests(
        &self,
    ) -> SpokeResult<Vec<nexus_spoke_adapter::HostCapabilityManifest>> {
        self.adapter.list_peer_host_capability_manifests().await
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orchestrate_upsert_denies_row_moved_to_another_world_between_verification_and_cas() {
    let (pool, _dir) = fresh_pool().await;
    seed_second_world(&pool).await;
    let adapter = scoped(pool.clone());

    // Writer 1 creates the entry in WORLD_ID → revision 1.
    let entry_id = "kb_wc_upsert";
    let created = spoke_entry(entry_id, "WorldCasUpsert", None, "provisional");
    let create_result = orchestrate_upsert(&adapter, upsert_request(&created)).await;
    assert!(
        matches!(create_result, SpokeResult::Ok(_)),
        "create must succeed first"
    );
    assert_eq!(
        read_kb_row(&pool, entry_id).await.3,
        WORLD_ID,
        "precondition: the created row is in the claimed world"
    );

    // Writer 2 is armed on the admission read: the orchestrator reads the
    // world-A preimage (admitted by this request), writer 2 moves the row to
    // wld_2 — still inside the admitted selection — and only then does writer 1
    // attempt its conditional write.
    let barrier = SecondWriterBarrier::new(
        &adapter,
        pool.clone(),
        SecondWriterMove::key_block(entry_id, "wld_2"),
    );

    // The candidate carries a distinct payload, so any leaked write is visible
    // in the post-state.
    let candidate = spoke_entry(entry_id, "WorldCasUpsert Rewritten", Some(1), "provisional");
    let result = orchestrate_upsert(&barrier, upsert_request(&candidate)).await;
    expect_world_conflict_reject(result, WORLD_ID, "wld_2");

    // INDEPENDENT verification: the denied CAS left the row exactly as the
    // second writer left it — its world and revision, plus the pre-move
    // canonical_name/body. A cross-world rewrite would show all four.
    let left = barrier.left(entry_id);
    assert_eq!(
        left.world_id, "wld_2",
        "precondition: the second writer moved the row"
    );
    let (name, _status, revision, world, body) = read_kb_row(&pool, entry_id).await;
    assert_eq!(
        world, left.world_id,
        "row stays in the world the interleaved writer set"
    );
    assert_eq!(
        revision, left.revision,
        "denied CAS must not bump the revision the second writer left"
    );
    assert_eq!(
        name, "WorldCasUpsert",
        "denied CAS must not rewrite canonical_name"
    );
    assert!(
        body.contains("WorldCasUpsert summary"),
        "denied CAS must not rewrite the body: {body}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orchestrate_promote_denies_row_moved_to_another_world_between_verification_and_cas() {
    let (pool, _dir) = fresh_pool().await;
    seed_second_world(&pool).await;
    let adapter = scoped(pool.clone());

    // Writer 1 creates a provisional entry in WORLD_ID → revision 1.
    let entry_id = "kb_wc_promote";
    let created = spoke_entry(entry_id, "WorldCasPromote", None, "provisional");
    let create_result = orchestrate_upsert(&adapter, upsert_request(&created)).await;
    assert!(
        matches!(create_result, SpokeResult::Ok(_)),
        "create must succeed first"
    );

    // Armed on the admission read, exactly as in the upsert regression.
    let barrier = SecondWriterBarrier::new(
        &adapter,
        pool.clone(),
        SecondWriterMove::key_block(entry_id, "wld_2"),
    );

    // Writer 1 promotes its world-A preimage (revision 1); the orchestrator
    // bases the accepted revision on that preimage (= stored + 1). The
    // candidate carries a distinct body, so a leaked write is visible in the
    // post-state (the store's create path already wrote the summary).
    let mut candidate = spoke_entry(entry_id, "WorldCasPromote", Some(1), "provisional");
    candidate.body.summary = Some("WorldCasPromote raced body".to_string());
    let result = orchestrate_promote(&barrier, promote_request(&candidate)).await;
    expect_world_conflict_reject(result, WORLD_ID, "wld_2");

    // INDEPENDENT verification: neither the world/revision nor the promotion
    // landed — the row is still provisional with its pre-race body in the
    // second writer's world.
    let left = barrier.left(entry_id);
    assert_eq!(left.world_id, "wld_2");
    let (name, status, revision, world, body) = read_kb_row(&pool, entry_id).await;
    assert_eq!(world, left.world_id);
    assert_eq!(revision, left.revision);
    assert_eq!(
        name, "WorldCasPromote",
        "denied promote must not rewrite the row"
    );
    assert_eq!(
        status, "provisional",
        "denied promote must not flip status to confirmed"
    );
    assert!(
        body.contains("WorldCasPromote summary"),
        "denied promote must not rewrite the body: {body}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orchestrate_relate_denies_row_moved_to_another_world_between_verification_and_cas() {
    let (pool, _dir) = fresh_pool().await;
    seed_second_world(&pool).await;
    let adapter = scoped(pool.clone());

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
    let created_relation = relate_relation(relation_id, "kb_wc_src", "kb_wc_dst");
    let create_result = orchestrate_relate(&adapter, relate_request(&created_relation)).await;
    assert!(
        matches!(create_result, SpokeResult::Ok(_)),
        "relate create must succeed first"
    );
    // Capture the row the create left, so "untouched" is checked against the
    // pre-race values of every mutable column rather than a table of literals.
    let before = read_relation_row(&pool, relation_id).await;
    assert_eq!(
        before.world_id, WORLD_ID,
        "precondition: the relation row is in the claimed world"
    );

    // Armed on the relation admission read.
    let barrier = SecondWriterBarrier::new(
        &adapter,
        pool.clone(),
        SecondWriterMove::relation(relation_id, "wld_2"),
    );

    // Writer 1 replays its world-A preimage with a distinct label AND a
    // distinct metadata bag, so a leaked write is visible in the post-state.
    let mut candidate = relate_relation(relation_id, "kb_wc_src", "kb_wc_dst");
    candidate.revision = Some(1);
    candidate.label = Some("raced write".to_string());
    candidate
        .metadata
        .insert("raced".to_string(), serde_json::Value::Bool(true));
    let result = orchestrate_relate(&barrier, relate_request(&candidate)).await;
    expect_world_conflict_reject(result, WORLD_ID, "wld_2");

    // INDEPENDENT verification: apart from the second writer's move, the
    // relation row is exactly what the create left — the candidate's label and
    // metadata bag, and the update timestamp, are all unchanged.
    let left = barrier.left(relation_id);
    assert_eq!(left.world_id, "wld_2");
    let after = read_relation_row(&pool, relation_id).await;
    assert_eq!(
        after.world_id, left.world_id,
        "relation stays in the world the interleaved writer set"
    );
    assert_eq!(
        after.revision, left.revision,
        "denied CAS must not bump the revision the second writer left"
    );
    assert_eq!(
        after.relation_type, before.relation_type,
        "denied CAS must not rewrite relation_type"
    );
    assert_eq!(
        after.custom_label, before.custom_label,
        "denied CAS must not write the candidate label: {:?}",
        after.custom_label
    );
    assert_eq!(
        after.metadata, before.metadata,
        "denied CAS must not write the candidate metadata bag: {:?}",
        after.metadata
    );
    assert_eq!(
        after.updated_at, before.updated_at,
        "denied CAS must not touch the update timestamp"
    );
}

// ── 7. Initially hidden rows stay indistinguishable from absent ─────────
//
// The other half of the classification: a row the request never admitted (it
// was moved outside the selection before the read, or it is owned elsewhere)
// must not be observable at all — the same refusal an absent id produces, no
// world-conflict classification, and no world id in the outcome. These are
// **not** races: the row is hidden from the very first read.
//
// Each parity regression runs the same request against two fresh stores that
// differ only in whether the hidden row exists, and compares the full
// observable outcome. Anything else is an existence oracle.

/// The observable shape of a result — code, message and details; everything a
/// caller can see.
fn outcome<T: std::fmt::Debug>(result: &SpokeResult<T>) -> String {
    match result {
        SpokeResult::Ok(value) => format!("Ok({value:?})"),
        SpokeResult::Reject(reject) => format!(
            "Reject(code={:?}, message={}, details={:?})",
            reject.code, reject.message, reject.details
        ),
    }
}

/// Whether a result carries the adapter's world-conflict classification (the
/// marker Connect's `map_reject` remaps to the `world_conflict` wire code).
fn carries_world_conflict<T: std::fmt::Debug>(result: &SpokeResult<T>) -> bool {
    match result {
        SpokeResult::Reject(reject) => nexus_spoke_adapter::is_world_conflict_reject(reject),
        SpokeResult::Ok(_) => false,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initially_hidden_entry_is_indistinguishable_from_absent() {
    let entry_id = "kb_hidden_parity";
    let candidate = spoke_entry(entry_id, "HiddenParity", Some(1), "provisional");

    // (A) the row exists, but a second writer moved it to wld_2 before the
    // request read it; the probing selection admits only WORLD_ID.
    let (hidden_pool, _hidden_dir) = fresh_pool().await;
    seed_second_world(&hidden_pool).await;
    let full = scoped(hidden_pool.clone());
    let created = spoke_entry(entry_id, "HiddenParity", None, "provisional");
    let create_result = orchestrate_upsert(&full, upsert_request(&created)).await;
    assert!(
        matches!(create_result, SpokeResult::Ok(_)),
        "create must succeed first"
    );
    move_key_block_to_world(&hidden_pool, entry_id, "wld_2").await;
    let hidden = scoped_worlds(hidden_pool.clone(), &[WORLD_ID]);

    // (B) the same request against a store where the id never existed.
    let (absent_pool, _absent_dir) = fresh_pool().await;
    let absent = scoped_worlds(absent_pool.clone(), &[WORLD_ID]);

    // Port level — the classification owner. Hidden and absent must produce the
    // identical reject, and never the world-conflict marker.
    let hidden_port = hidden.put_knowledge_entry(candidate.clone(), Some(1)).await;
    let absent_port = absent.put_knowledge_entry(candidate.clone(), Some(1)).await;
    assert_eq!(
        outcome(&hidden_port),
        outcome(&absent_port),
        "a hidden row must be indistinguishable from an absent one at the port"
    );
    assert!(
        !carries_world_conflict(&hidden_port),
        "a row the request never admitted must not classify as a world conflict: {}",
        outcome(&hidden_port)
    );
    assert!(
        !outcome(&hidden_port).contains("wld_2"),
        "no foreign world id may leave the adapter: {}",
        outcome(&hidden_port)
    );

    // Orchestrator level — the request sees the same absent-shaped outcome.
    let hidden_orch = orchestrate_upsert(&hidden, upsert_request(&candidate)).await;
    let absent_orch = orchestrate_upsert(&absent, upsert_request(&candidate)).await;
    assert_eq!(
        outcome(&hidden_orch),
        outcome(&absent_orch),
        "the orchestrator must not observe the hidden row either"
    );
    assert!(
        !carries_world_conflict(&hidden_orch),
        "the orchestrator must not surface a world conflict for a hidden row"
    );
    assert!(
        !outcome(&hidden_orch).contains("wld_2"),
        "no foreign world id may leave the orchestrator: {}",
        outcome(&hidden_orch)
    );

    // The probes never touched the hidden row.
    let (_, _, _, world, _) = read_kb_row(&hidden_pool, entry_id).await;
    assert_eq!(world, "wld_2", "the hidden row is left where it was moved");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initially_hidden_relation_is_indistinguishable_from_absent() {
    let relation_id = "rel_hidden_parity";
    let endpoints = ["kb_hp_src", "kb_hp_dst"];

    // (A) the relation exists, but its endpoints were moved to wld_2 before the
    // request read it — relation visibility is endpoint-scoped (durable §4.2),
    // so the probing selection (WORLD_ID only) cannot see either the endpoints
    // or the relation.
    let (hidden_pool, _hidden_dir) = fresh_pool().await;
    seed_second_world(&hidden_pool).await;
    let full = scoped(hidden_pool.clone());
    for endpoint in endpoints {
        let ep = spoke_entry(endpoint, endpoint, None, "confirmed");
        let r = orchestrate_upsert(&full, upsert_request(&ep)).await;
        assert!(
            matches!(r, SpokeResult::Ok(_)),
            "endpoint {endpoint} create must succeed"
        );
    }
    let created_relation = relate_relation(relation_id, endpoints[0], endpoints[1]);
    let create_result = orchestrate_relate(&full, relate_request(&created_relation)).await;
    assert!(
        matches!(create_result, SpokeResult::Ok(_)),
        "relate create must succeed first"
    );
    for endpoint in endpoints {
        move_key_block_to_world(&hidden_pool, endpoint, "wld_2").await;
    }
    let hidden = scoped_worlds(hidden_pool.clone(), &[WORLD_ID]);

    // (B) the same request against a store where the relation never existed.
    let (absent_pool, _absent_dir) = fresh_pool().await;
    let absent = scoped_worlds(absent_pool.clone(), &[WORLD_ID]);

    // Port level: the hidden relation is served the single not-found shape.
    let hidden_get = hidden.get_relation(relation_id).await;
    let absent_get = absent.get_relation(relation_id).await;
    assert_eq!(
        outcome(&hidden_get),
        outcome(&absent_get),
        "a hidden relation must read as absent"
    );
    assert!(
        !carries_world_conflict(&hidden_get),
        "the hidden-relation read must not carry the world-conflict marker: {}",
        outcome(&hidden_get)
    );
    assert!(
        !outcome(&hidden_get).contains("wld_2"),
        "the not-found shape must not disclose the endpoint world: {}",
        outcome(&hidden_get)
    );

    let mut candidate = relate_relation(relation_id, endpoints[0], endpoints[1]);
    candidate.revision = Some(1);
    let hidden_put = hidden.put_relation(candidate.clone(), Some(1)).await;
    let absent_put = absent.put_relation(candidate.clone(), Some(1)).await;
    assert_eq!(
        outcome(&hidden_put),
        outcome(&absent_put),
        "a hidden relation must be indistinguishable from an absent one on put"
    );
    assert!(
        !carries_world_conflict(&hidden_put),
        "a row the request never admitted must not classify as a world conflict: {}",
        outcome(&hidden_put)
    );
    assert!(
        !outcome(&hidden_put).contains("wld_2"),
        "no foreign world id may leave the adapter: {}",
        outcome(&hidden_put)
    );

    // Orchestrator level — same absent-shaped outcome for both stores.
    let hidden_orch = orchestrate_relate(&hidden, relate_request(&candidate)).await;
    let absent_orch = orchestrate_relate(&absent, relate_request(&candidate)).await;
    assert_eq!(
        outcome(&hidden_orch),
        outcome(&absent_orch),
        "the orchestrator must not observe the hidden relation either"
    );
    assert!(
        !carries_world_conflict(&hidden_orch),
        "the orchestrator must not surface a world conflict for a hidden relation"
    );
    assert!(
        !outcome(&hidden_orch).contains("wld_2"),
        "no foreign world id may leave the orchestrator: {}",
        outcome(&hidden_orch)
    );

    // Both fixture rows are untouched by the probes.
    for endpoint in endpoints {
        let (_, _, _, world, _) = read_kb_row(&hidden_pool, endpoint).await;
        assert_eq!(world, "wld_2");
    }
    assert_eq!(
        read_relation_row(&hidden_pool, relation_id).await.world_id,
        WORLD_ID
    );
}
