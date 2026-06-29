//! V1.73 P0 World KB patch-route integration tests.
//!
//! Exercises the four World KB Local API handlers directly against a
//! canonical daemon `WorkspaceState` with a seeded creator/world/KeyBlock:
//! - `patch_entity` happy path + per-row OCC 409 conflict + 422 validation.
//! - `promote_candidate` adopt + reject (entity-scope-model §5.5.2 state machine).
//! - `get_graph` + `get_candidates` read projections.
//!
//! Regression coverage: a stale `expected_version` must short-circuit as 409
//! BEFORE any write (per-row OCC catches stale writes from both canvas and
//! daemon-side writers).

use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::{
    WorldKbEntityPatch, WorldKbPatchEntityRequest, WorldKbPromoteCandidateRequest,
};
use nexus_daemon_runtime::api::handlers::world_kb::{
    get_candidates, get_graph, patch_entity, promote_candidate, CandidatesQuery,
};
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_local_db::kb_extract_job::insert_pending;

/// Seed a `kb_key_blocks` row directly (bypassing store validation) with a
/// controlled `status` and `revision`, returning its id.
// 8 params mirrors the kb_key_blocks column layout — same rationale as
//  nexus_local_db::kb_extract_job::insert_pending.
#[allow(clippy::too_many_arguments)]
async fn seed_key_block(
    pool: &sqlx::SqlitePool,
    key_block_id: &str,
    world_id: &str,
    block_type: &str,
    canonical_name: &str,
    status: &str,
    revision: Option<i64>,
    body_json: Option<&str>,
) {
    // SAFETY: test-only seed against the known kb_key_blocks schema.
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, world_id, block_type, canonical_name, status, revision, body_json, \
          created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(key_block_id)
    .bind(world_id)
    .bind(block_type)
    .bind(canonical_name)
    .bind(status)
    .bind(revision)
    .bind(body_json)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a `kb_extract_jobs` promotion-candidate row directly (bypassing the
/// `insert_pending` helper, which sets `work_entry_id = canonical_name_guess`
/// and so cannot produce two same-name rows). Lets the test model two distinct
/// extraction jobs that happen to guess the same canonical name (e.g. the same
/// character extracted from two different source works).
#[allow(clippy::too_many_arguments)]
async fn seed_pending_candidate(
    pool: &sqlx::SqlitePool,
    job_id: &str,
    work_entry_id: &str,
    world_id: &str,
    block_type_guess: &str,
    canonical_name_guess: &str,
) {
    // SAFETY: test-only seed against the known kb_extract_jobs schema.
    sqlx::query(
        "INSERT INTO kb_extract_jobs \
         (job_id, creator_id, workspace_id, work_entry_id, world_id, status, \
          promotion_status, proposed_payload, block_type_guess, canonical_name_guess, version) \
         VALUES (?, 'test_creator', 'ws', ?, ?, 'done', 'pending', ?, ?, ?, 0)",
    )
    .bind(job_id)
    .bind(work_entry_id)
    .bind(world_id)
    .bind(NOVEL_CHARACTER_BODY)
    .bind(block_type_guess)
    .bind(canonical_name_guess)
    .execute(pool)
    .await
    .unwrap();
}

async fn fresh_state() -> (
    nexus_daemon_runtime::test_utils::TestTempRoot,
    WorkspaceState,
) {
    let (tmp, nexus_home, db_path, workspace_dir) =
        nexus_daemon_runtime::test_utils::create_initialized_test_workspace().await;
    let state = WorkspaceState::new_for_testing(
        nexus_home,
        db_path,
        Some(workspace_dir.to_string_lossy().to_string()),
    )
    .await;
    nexus_daemon_runtime::test_utils::seed_test_creator_and_world(state.pool()).await;
    (tmp, state)
}

// ─── patch-entity ───────────────────────────────────────────────────────────

#[tokio::test]
async fn patch_entity_title_bumps_version() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool(),
        "kb_hero",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
        None, // NULL revision — normalized to 0
        None,
    )
    .await;

    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_hero".to_string(),
        expected_version: 0,
        patch: WorldKbEntityPatch {
            title: Some("Aria Stormwind".to_string()),
            body: None,
            aliases: None,
            block_type: None,
        },
        idempotency_key: None,
    };
    let Json(resp) = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("patch should succeed");

    assert_eq!(resp.version, 1, "NULL revision should bump to 1");
    assert_eq!(resp.entity.canonical_name, "Aria Stormwind");
    assert_eq!(resp.entity.status, "confirmed");
}

#[tokio::test]
async fn patch_entity_stale_version_returns_409() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool(),
        "kb_hero",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
        Some(3), // current version is 3
        None,
    )
    .await;

    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_hero".to_string(),
        expected_version: 2, // stale
        patch: WorldKbEntityPatch {
            title: Some("Aria v2".to_string()),
            body: None,
            aliases: None,
            block_type: None,
        },
        idempotency_key: None,
    };
    let err = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("stale version must 409");
    assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);
    assert_eq!(err.error_code(), "world_kb_conflict");
    let details = err.error_details().expect("conflict details");
    assert_eq!(details["current_version"], 3);
    assert_eq!(details["entity_id"], "kb_hero");
}

#[tokio::test]
async fn patch_entity_deleted_entity_rejected_422() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool(),
        "kb_dead",
        "wld_test_world",
        "character",
        "Ghost",
        "deleted",
        Some(0),
        None,
    )
    .await;

    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_dead".to_string(),
        expected_version: 0,
        patch: WorldKbEntityPatch {
            title: Some("Ghost Renamed".to_string()),
            body: None,
            aliases: None,
            block_type: None,
        },
        idempotency_key: None,
    };
    let err = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("deleted entity patch must 422");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(err.error_code(), "world_kb_validation_failed");
}

#[tokio::test]
async fn patch_entity_cross_author_forbidden() {
    let (_tmp, state) = fresh_state().await;
    // World owned by a different creator (seed creator + world for FK).
    // SAFETY: test-only seed of a foreign-owned world + its owner creator.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES ('wld_other', 'ws', 'other_creator', 'Other', 'other-world', 'active', 'private', \
          'manual', '{}', datetime('now'))",
    )
    .execute(state.pool())
    .await
    .unwrap();
    seed_key_block(
        state.pool(),
        "kb_other",
        "wld_other",
        "character",
        "Villain",
        "confirmed",
        Some(0),
        None,
    )
    .await;

    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_other".to_string(),
        expected_version: 0,
        patch: WorldKbEntityPatch {
            title: Some("Villain v2".to_string()),
            body: None,
            aliases: None,
            block_type: None,
        },
        idempotency_key: None,
    };
    let err = patch_entity(
        State(state.clone()),
        Path("wld_other".to_string()),
        Json(req),
    )
    .await
    .expect_err("cross-author must 403");
    assert_eq!(err.status_code(), axum::http::StatusCode::FORBIDDEN);
}

/// Regression for V1.73 greploop issue 3: `patch_entity` read the KeyBlock (and
/// ran the cross-world scope check) BEFORE `require_world_owner`. An
/// unauthenticated-but-locally-active creator could therefore distinguish
/// `NotFound` ("entity not in this world") from `Forbidden` ("not your world"),
/// leaking entity-existence signals across world boundaries.
///
/// Discriminating case: the active creator does NOT own the path world, and the
/// entity they quote exists in their OWN world (so `kb.world_id != path world`).
/// Under the buggy order this returned 404 NotFound; the fix runs
/// `require_world_owner` first (mirroring `promote_candidate` + the read
/// endpoints), so every cross-author request collapses to 403 regardless of
/// whether the entity exists in the path world.
#[tokio::test]
async fn patch_entity_cross_author_does_not_leak_existence() {
    let (_tmp, state) = fresh_state().await;

    // Foreign world owned by another creator.
    // SAFETY: test-only seed of a foreign-owned world + its owner creator.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES ('wld_other', 'ws', 'other_creator', 'Other', 'other-world', 'active', 'private', \
          'manual', '{}', datetime('now'))",
    )
    .execute(state.pool())
    .await
    .unwrap();

    // An entity that exists in the ACTIVE creator's OWN world (not the foreign
    // path world). This is the row whose existence must NOT be revealed.
    seed_key_block(
        state.pool(),
        "kb_mine",
        "wld_test_world",
        "character",
        "My Hero",
        "confirmed",
        Some(0),
        None,
    )
    .await;

    // Active creator (test_creator) does NOT own wld_other. Quoting an entity
    // that lives in their own world via the foreign world's path must collapse
    // to 403 Forbidden, NOT 404 NotFound.
    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_mine".to_string(),
        expected_version: 0,
        patch: WorldKbEntityPatch {
            title: Some("Whatever".to_string()),
            body: None,
            aliases: None,
            block_type: None,
        },
        idempotency_key: None,
    };
    let err = patch_entity(
        State(state.clone()),
        Path("wld_other".to_string()),
        Json(req),
    )
    .await
    .expect_err("cross-author must be forbidden before any entity read");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::FORBIDDEN,
        "cross-author patch-entity must return 403, not leak existence via 404"
    );
}

// ─── promote-candidate ──────────────────────────────────────────────────────

const NOVEL_CHARACTER_BODY: &str =
    r#"{"summary":"A brave hero","attributes":{"novel_category":"character"}}"#;

#[tokio::test]
async fn promote_adopt_confirms_candidate() {
    let (_tmp, state) = fresh_state().await;
    let candidate = insert_pending(
        state.pool(),
        "test_creator",
        "ws",
        "wld_test_world",
        None,
        None,
        "character",
        "Kael",
        NOVEL_CHARACTER_BODY,
    )
    .await
    .unwrap();

    let req = WorldKbPromoteCandidateRequest {
        job_id: candidate.job_id.clone(),
        candidate_id: "kb_cand".to_string(),
        action: "adopt".to_string(),
        expected_version: u64::try_from(candidate.version).unwrap_or(0),
        merge_target_id: None,
        patch: None,
        idempotency_key: None,
    };
    let Json(resp) = promote_candidate(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("adopt should succeed");

    let entity = resp.entity.expect("adopt returns a confirmed entity");
    assert_eq!(entity.status, "confirmed");
    assert_eq!(entity.canonical_name, "Kael");
    assert_eq!(resp.job.status, "confirmed");
}

#[tokio::test]
async fn promote_reject_dismisses_candidate() {
    let (_tmp, state) = fresh_state().await;
    let candidate = insert_pending(
        state.pool(),
        "test_creator",
        "ws",
        "wld_test_world",
        None,
        None,
        "character",
        "Rejecta",
        NOVEL_CHARACTER_BODY,
    )
    .await
    .unwrap();

    let req = WorldKbPromoteCandidateRequest {
        job_id: candidate.job_id.clone(),
        candidate_id: "kb_cand".to_string(),
        action: "reject".to_string(),
        expected_version: u64::try_from(candidate.version).unwrap_or(0),
        merge_target_id: None,
        patch: None,
        idempotency_key: None,
    };
    let Json(resp) = promote_candidate(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("reject should succeed");

    assert!(resp.entity.is_none(), "reject returns no entity");
    assert_eq!(resp.job.status, "rejected");
}

#[tokio::test]
async fn promote_stale_version_returns_409() {
    let (_tmp, state) = fresh_state().await;
    let candidate = insert_pending(
        state.pool(),
        "test_creator",
        "ws",
        "wld_test_world",
        None,
        None,
        "character",
        "Stalea",
        NOVEL_CHARACTER_BODY,
    )
    .await
    .unwrap();

    let req = WorldKbPromoteCandidateRequest {
        job_id: candidate.job_id.clone(),
        candidate_id: "kb_cand".to_string(),
        action: "adopt".to_string(),
        expected_version: u64::try_from(candidate.version).unwrap_or(0) + 100, // stale
        merge_target_id: None,
        patch: None,
        idempotency_key: None,
    };
    let err = promote_candidate(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("stale promote must 409");
    assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);
    assert_eq!(err.error_code(), "world_kb_conflict");
}

// ─── read endpoints ─────────────────────────────────────────────────────────

#[tokio::test]
async fn get_graph_returns_non_deleted_entities() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool(),
        "kb_one",
        "wld_test_world",
        "character",
        "Hero",
        "confirmed",
        Some(1),
        None,
    )
    .await;
    seed_key_block(
        state.pool(),
        "kb_two",
        "wld_test_world",
        "item",
        "Sword",
        "deleted",
        Some(0),
        None,
    )
    .await;

    let Json(resp) = get_graph(State(state.clone()), Path("wld_test_world".to_string()))
        .await
        .expect("graph should succeed");
    assert_eq!(resp.entities.len(), 1, "deleted entities are excluded");
    assert_eq!(resp.entities[0].key_block_id, "kb_one");
    assert!(
        resp.relationships.is_empty(),
        "relationships deferred to V1.74"
    );
}

#[tokio::test]
async fn get_candidates_returns_pending() {
    let (_tmp, state) = fresh_state().await;
    insert_pending(
        state.pool(),
        "test_creator",
        "ws",
        "wld_test_world",
        None,
        None,
        "character",
        "Cand One",
        NOVEL_CHARACTER_BODY,
    )
    .await
    .unwrap();

    let Json(resp) = get_candidates(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(CandidatesQuery {
            limit: None,
            cursor: None,
        }),
    )
    .await
    .expect("candidates should succeed");
    assert_eq!(resp.items.len(), 1);
    assert_eq!(resp.items[0].canonical_name, "Cand One");
    assert_eq!(
        resp.items[0].block_type,
        nexus_contracts::BlockType::Character
    );
}

/// Regression for V1.73 qc3 W-01: cursor pagination must reach every pending
/// candidate, not just the first `limit + 1` window. Seeds 4 candidates,
/// walks the list with `limit = 2`, and asserts all 4 are returned exactly
/// once across the two pages (no loss, no duplication). The expected order is
/// derived from the seeded rows using the same `(created_at, job_id)`
/// comparator the storage query uses, so the assertion holds whether or not
/// the inserts land in the same `datetime('now')` second.
#[tokio::test]
async fn get_candidates_multi_page_cursor_reaches_all_rows() {
    let (_tmp, state) = fresh_state().await;

    // Seed 4 pending candidates; collect the returned rows so we can derive
    // the expected keyset order independently of the handler.
    let mut seeded: Vec<nexus_local_db::kb_extract_job::KbExtractPromotion> = Vec::new();
    for idx in 0..4u8 {
        let row = insert_pending(
            state.pool(),
            "test_creator",
            "ws",
            "wld_test_world",
            None,
            None,
            "character",
            &format!("Cand {idx}"),
            NOVEL_CHARACTER_BODY,
        )
        .await
        .expect("insert_pending should succeed");
        seeded.push(row);
    }
    // Expected keyset order: (created_at ASC, job_id ASC) — mirrors the SQL
    // `ORDER BY created_at ASC, job_id ASC` in `list_pending_for_world_after`.
    seeded.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.job_id.cmp(&b.job_id))
    });
    let expected_names: Vec<String> = seeded
        .iter()
        .map(|c| c.canonical_name_guess.clone().unwrap_or_default())
        .collect();
    let expected_ids: Vec<String> = seeded.iter().map(|c| c.job_id.clone()).collect();

    // Page 1: limit=2, no cursor.
    let Json(page1) = get_candidates(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(CandidatesQuery {
            limit: Some(2),
            cursor: None,
        }),
    )
    .await
    .expect("page 1 should succeed");
    assert_eq!(
        page1.items.len(),
        2,
        "page 1 should return exactly `limit` items"
    );
    assert_eq!(page1.items[0].canonical_name, expected_names[0]);
    assert_eq!(page1.items[1].canonical_name, expected_names[1]);
    assert_eq!(page1.items[0].job_id, expected_ids[0]);
    assert_eq!(page1.items[1].job_id, expected_ids[1]);
    assert!(
        page1.pagination.has_more,
        "page 1 must signal has_more when more rows remain"
    );
    let cursor1 = page1
        .pagination
        .next_cursor
        .clone()
        .expect("page 1 must return a next_cursor");

    // Page 2: limit=2, cursor from page 1 — must reach the REMAINING rows,
    // not re-skip inside the first truncated window.
    let Json(page2) = get_candidates(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(CandidatesQuery {
            limit: Some(2),
            cursor: Some(cursor1),
        }),
    )
    .await
    .expect("page 2 should succeed");
    assert_eq!(
        page2.items.len(),
        2,
        "page 2 should return the remaining 2 items (the W-01 bug returned 0)"
    );
    assert_eq!(page2.items[0].canonical_name, expected_names[2]);
    assert_eq!(page2.items[1].canonical_name, expected_names[3]);
    assert_eq!(page2.items[0].job_id, expected_ids[2]);
    assert_eq!(page2.items[1].job_id, expected_ids[3]);
    assert!(
        !page2.pagination.has_more,
        "page 2 is the last page; has_more must be false"
    );
    assert!(
        page2.pagination.next_cursor.is_none(),
        "page 2 is the last page; next_cursor must be absent"
    );

    // No loss, no duplication across the full walk.
    let mut seen: Vec<String> = page1
        .items
        .iter()
        .map(|c| c.job_id.clone())
        .chain(page2.items.iter().map(|c| c.job_id.clone()))
        .collect();
    seen.sort();
    assert_eq!(
        seen,
        {
            let mut all = expected_ids.clone();
            all.sort();
            all
        },
        "every seeded candidate must appear exactly once across pages 1+2"
    );

    // Page 3: cursor past the end — must be empty, not an error.
    let cursor2 = page2
        .pagination
        .next_cursor
        .clone()
        .or_else(|| {
            // Last page has no next_cursor by design; synthesize a cursor from
            // the final row so we can prove a follow-up request stays empty
            // rather than re-issuing page 2.
            seeded
                .last()
                .map(|r| format!("kbp:{}|{}", r.created_at, r.job_id))
        })
        .expect("a synthesized terminal cursor must be available");
    let Json(page3) = get_candidates(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(CandidatesQuery {
            limit: Some(2),
            cursor: Some(cursor2),
        }),
    )
    .await
    .expect("page 3 (past end) should succeed, not error");
    assert!(
        page3.items.is_empty(),
        "a cursor past the last row must yield an empty page, not a repeat"
    );
    assert!(!page3.pagination.has_more);
}

/// Regression for V1.73 greploop issue 2: `candidate_id` was projected from the
/// non-unique `canonical_name_guess`. Two pending candidates that share the
/// same guessed name (the same character extracted from two different source
/// works — distinct `work_entry_id`) collided on `candidate_id`, so their React
/// Flow node IDs clashed and `candidateItems.find(c => c.candidate_id === ...)`
/// resolved to the FIRST match, promoting the wrong `job_id`. The fix projects
/// `candidate_id` from the unique row PK `job_id`.
#[tokio::test]
async fn get_candidates_distinct_candidate_id_for_same_canonical_name() {
    let (_tmp, state) = fresh_state().await;

    // Two pending candidates with the SAME canonical_name_guess but distinct
    // work_entry_id (the idempotency index is on (creator, work_entry_id,
    // world), so distinct work_entry_id lets both rows coexist).
    seed_pending_candidate(
        state.pool(),
        "xj_aaaaaa0000000000000000000001",
        "we_source_work_one",
        "wld_test_world",
        "character",
        "Duplicate Name",
    )
    .await;
    seed_pending_candidate(
        state.pool(),
        "xj_aaaaaa0000000000000000000002",
        "we_source_work_two",
        "wld_test_world",
        "character",
        "Duplicate Name",
    )
    .await;

    let Json(resp) = get_candidates(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(CandidatesQuery {
            limit: None,
            cursor: None,
        }),
    )
    .await
    .expect("candidates should succeed");

    assert_eq!(
        resp.items.len(),
        2,
        "both same-name candidates must be listed"
    );
    let ids: Vec<String> = resp.items.iter().map(|c| c.candidate_id.clone()).collect();
    assert_ne!(
        ids[0], ids[1],
        "candidate_id must be unique per row even when canonical_name_guess collides"
    );
    // The fix: candidate_id == job_id (the row PK), not canonical_name_guess.
    assert!(
        resp.items.iter().all(|c| c.candidate_id == c.job_id),
        "candidate_id must equal job_id; got {ids:?}"
    );
    // Display name is still the shared guess.
    assert!(
        resp.items
            .iter()
            .all(|c| c.canonical_name == "Duplicate Name"),
        "canonical_name stays the guessed display name"
    );
}

/// Regression for the V1.73 greploop iter-2 greptile P1: when a concurrent
/// write bumps `kb_extract_jobs.version` between the outer version check and
/// the promote-reject CAS UPDATE, the 409 conflict MUST report the re-read
/// current version — NOT the stale `req.expected_version`. Otherwise the
/// canvas client (`promotion-inspector.tsx`) resubmits with the same stale
/// version and hits a second, avoidable conflict.
///
/// We force the CAS-miss path deterministically by firing two concurrent
/// `reject` actions on the same pending candidate. On the current-thread
/// `tokio::test` runtime, `tokio::join!` interleaves both futures at their
/// await points, so BOTH outer reads see version V before EITHER CAS UPDATE
/// runs; `SQLite` then serializes the writes — the first CAS wins (V → V+1,
/// `pending` → `rejected`) and the second CAS affects 0 rows (the bug path).
/// The losing 409 must therefore carry `current_version = V+1`, not the stale
/// `expected_version = V`.
#[tokio::test]
async fn promote_reject_cas_miss_conflict_carries_bumped_version() {
    let (_tmp, state) = fresh_state().await;
    let candidate = insert_pending(
        state.pool(),
        "test_creator",
        "ws",
        "wld_test_world",
        None,
        None,
        "character",
        "Racea",
        NOVEL_CHARACTER_BODY,
    )
    .await
    .unwrap();
    let stale_expected = u64::try_from(candidate.version).unwrap_or(0);

    let mk_req = || WorldKbPromoteCandidateRequest {
        job_id: candidate.job_id.clone(),
        candidate_id: "kb_cand".to_string(),
        action: "reject".to_string(),
        expected_version: stale_expected,
        merge_target_id: None,
        patch: None,
        idempotency_key: None,
    };

    // Two concurrent rejects against the same pending candidate. `join!`
    // interleaves at await points on the current-thread runtime, so both
    // outer reads observe version V before either CAS commits.
    let (res_a, res_b) = tokio::join!(
        promote_candidate(
            State(state.clone()),
            Path("wld_test_world".to_string()),
            Json(mk_req()),
        ),
        promote_candidate(
            State(state.clone()),
            Path("wld_test_world".to_string()),
            Json(mk_req()),
        ),
    );

    // Exactly one CAS wins; the other hits the CAS-miss branch under test.
    let (winner, loser) = match (res_a, res_b) {
        (Ok(_), Err(e)) => (true, e),
        (Err(e), Ok(_)) => (false, e),
        (Ok(_), Ok(_)) => panic!("both rejects won the CAS — race did not serialize"),
        (Err(a), Err(b)) => panic!(
            "both rejects failed; neither reached the CAS-miss path. \
             errors: a={a:?} b={b:?}"
        ),
    };
    let _ = winner; // either order is fine; only the loser's 409 is asserted.

    assert_eq!(loser.status_code(), axum::http::StatusCode::CONFLICT);
    assert_eq!(loser.error_code(), "world_kb_conflict");
    let details = loser.error_details().expect("conflict must carry details");
    // The fix: `current_version` is re-read after the CAS miss, reflecting the
    // winner's bump (V+1). Pre-fix this was the stale `expected_version` (V).
    assert_eq!(
        details["current_version"],
        serde_json::json!(stale_expected + 1),
        "CAS-miss 409 must report the re-read bumped version, not the stale expected_version"
    );
    assert_ne!(
        details["current_version"],
        serde_json::json!(stale_expected),
        "regression: CAS-miss 409 echoed the stale expected_version"
    );
    assert_eq!(details["entity_id"], candidate.job_id);
}

/// Regression for the V1.73 greploop iter-5 greptile P1: when a concurrent
/// write bumps the merge TARGET's `kb_key_blocks.revision` between the merge's
/// read and its in-tx CAS UPDATE, the 409 conflict MUST distinctly mark the
/// TARGET as the conflicting entity (`conflicting_path = "merge_target"`,
/// `entity_id = <target_id>`) — NOT the candidate's `"version"` path.
///
/// Without the marker, the client (`promotion-inspector.tsx`) cannot tell a
/// target conflict from a candidate conflict: it treats the 409's
/// `current_version` (the target's revision) as the candidate's version and
/// retries the promote with `expected_version = <target revision>`, which fails
/// the candidate CAS again — a two-round-trip conflict loop with misleading
/// modal text.
///
/// We force the target-CAS-miss deterministically by firing two concurrent
/// `merge` actions on the same confirmed target (two distinct pending
/// candidates). On the current-thread `tokio::test` runtime, `tokio::join!`
/// interleaves both futures at their await points, so BOTH target reads see
/// revision V before EITHER target CAS UPDATE runs; SQLite then serializes the
/// writes — the first target CAS wins (V → V+1) and the second affects 0 rows
/// (the bug path). The losing 409 must carry `conflicting_path = "merge_target"`
/// and `entity_id = <target_id>`, not the candidate's `"version"`.
#[tokio::test]
async fn promote_merge_target_cas_miss_marks_target_conflict() {
    let (_tmp, state) = fresh_state().await;
    // Confirmed merge target at revision 0.
    seed_key_block(
        state.pool(),
        "kb_target",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
        Some(0),
        None,
    )
    .await;
    // Two distinct pending candidates targeting the same entity. Both start at
    // version 0 so each merge's outer OCC precondition passes independently.
    seed_pending_candidate(
        state.pool(),
        "xj_merge_c1",
        "Racea1",
        "wld_test_world",
        "character",
        "Racea",
    )
    .await;
    seed_pending_candidate(
        state.pool(),
        "xj_merge_c2",
        "Racea2",
        "wld_test_world",
        "character",
        "Racea",
    )
    .await;

    let mk_req = |job_id: &str| WorldKbPromoteCandidateRequest {
        job_id: job_id.to_string(),
        candidate_id: "kb_cand".to_string(),
        action: "merge".to_string(),
        expected_version: 0,
        merge_target_id: Some("kb_target".to_string()),
        patch: None,
        idempotency_key: None,
    };

    // Two concurrent merges against the same target. `join!` interleaves at
    // await points on the current-thread runtime, so both target reads observe
    // revision 0 before either target CAS commits.
    let (res_a, res_b) = tokio::join!(
        promote_candidate(
            State(state.clone()),
            Path("wld_test_world".to_string()),
            Json(mk_req("xj_merge_c1")),
        ),
        promote_candidate(
            State(state.clone()),
            Path("wld_test_world".to_string()),
            Json(mk_req("xj_merge_c2")),
        ),
    );

    // Exactly one target CAS wins; the other hits the target-CAS-miss branch.
    let loser = match (res_a, res_b) {
        (Ok(_), Err(e)) | (Err(e), Ok(_)) => e,
        (Ok(_), Ok(_)) => panic!(
            "both merges won the target CAS — race did not serialize; \
             expected one winner + one target-conflict 409"
        ),
        (Err(a), Err(b)) => panic!(
            "both merges failed; neither reached the target-CAS-miss path. \
             errors: a={a:?} b={b:?}"
        ),
    };

    assert_eq!(loser.status_code(), axum::http::StatusCode::CONFLICT);
    assert_eq!(loser.error_code(), "world_kb_conflict");
    let details = loser.error_details().expect("conflict must carry details");
    // The fix: the target CAS miss is tagged `conflicting_path = "merge_target"`
    // (distinct from the candidate's `"version"`) so the client can distinguish
    // a target conflict from a candidate conflict. Pre-fix this was
    // `conflicting_path = "version"` — indistinguishable from a candidate CAS
    // miss, causing the client to retry the candidate with the target's
    // revision as `expected_version`.
    assert_eq!(
        details["conflicting_path"], "merge_target",
        "target CAS-miss 409 must tag conflicting_path = merge_target so the \
         client distinguishes a target conflict from a candidate conflict"
    );
    assert_eq!(
        details["entity_id"], "kb_target",
        "target CAS-miss 409 must carry the target's entity_id"
    );
    assert_eq!(
        details["current_version"], 1,
        "target CAS-miss 409 must report the bumped target revision (V+1)"
    );
}
