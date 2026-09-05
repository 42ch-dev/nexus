//! v1.184 P3 Task 3 — Character SOUL/Memory Daemon API contract tests.
//!
//! Covers the generated Character bearer routes under
//! `/v1/daemon/characters/{character_id}/{memory,soul}`:
//! - capture/list/count/delete pending-review lifecycle with scope isolation
//! - bounded deterministic pagination
//! - review drain into Character-only storage (Creator tables untouched)
//! - revision-checked local→shared promotion (stable 409s, zero mutation)
//! - SOUL narrative reflect states (insufficient_data/ungenerated/current/stale)
//! - fail-closed authorization: foreign/missing/inactive Character and
//!   foreign/inactive binding reject before any row/file/synthesis side effect

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils::{self, TestTempRoot};
use nexus_daemon_runtime::workspace::WorkspaceState;
use serde_json::{json, Value};
fn j(resp: &axum_test::TestResponse) -> Value {
    resp.json()
}


const OWNER: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const WORLD_A: &str = "wld_worldA";
const WORLD_B: &str = "wld_worldB";
const WORLD_C: &str = "wld_worldC";

/// Digest ≥ 200 chars with unknown task kind → PromoteToLongTerm.
const PROMOTE_DIGEST: &str = "The character finally confronted the gatekeeper at the citadel and chose mercy over vengeance, a decision that rewrote the pact binding their lineage and set the third act in motion. The gatekeeper had waited a century for this reckoning, and the choice would echo through every covenant the family had sworn since the river first carried their name.";

/// ≥ 50 chars with research task kind → FragmentOnly.
const FRAGMENT_DIGEST: &str =
    "Researched the tidal customs of the southern ports for scene texture.";

/// < 50 chars with non-creative task kind → Drop.
const DROP_DIGEST: &str = "tiny note";

struct Ctx {
    _tmp: TestTempRoot,
    server: TestServer,
    pool: sqlx::SqlitePool,
    nexus_home: std::path::PathBuf,
}

async fn ctx() -> Ctx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{OWNER}\"\n\n[active_workspace_slug_by_creator]\n\"{OWNER}\" = \"default\"\n"
        ),
    )
    .unwrap();
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
    let pool = state.pool().unwrap().clone();
    seed_actor_fixture(&pool).await;
    let server = TestServer::new(api::create_router(state, DaemonApiConfig::keyless()))
        .expect("test server");
    Ctx {
        _tmp: tmp,
        server,
        pool,
        nexus_home,
    }
}

async fn seed_actor_fixture(pool: &sqlx::SqlitePool) {
    for (id, name) in [(OWNER, "Owner"), (OTHER, "Other")] {
        nexus_local_db::ensure_creator_row(pool, id, name)
            .await
            .unwrap();
    }
    for (world_id, owner) in [(WORLD_A, OWNER), (WORLD_B, OWNER), (WORLD_C, OTHER)] {
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(world_id)
        .bind(owner)
        .bind(world_id)
        .bind(world_id)
        .execute(pool)
        .await
        .unwrap();
    }
}

/// Create a Character through the public route; returns (character_id, binding_id).
async fn create_character(server: &TestServer, name: &str, world_id: &str) -> (String, String) {
    let resp = server
        .post("/v1/daemon/characters")
        .json(&json!({ "display_name": name, "world_id": world_id }))
        .await;
    assert_eq!(resp.status_code(), 201, "create {name}: {}", resp.text());
    let body: Value = resp.json();
    (
        body["character"]["character_id"].as_str().unwrap().to_string(),
        body["binding"]["binding_id"].as_str().unwrap().to_string(),
    )
}

async fn add_binding(server: &TestServer, character_id: &str, world_id: &str) -> String {
    let resp = server
        .post(&format!("/v1/daemon/characters/{character_id}/bindings"))
        .json(&json!({ "world_id": world_id }))
        .await;
    assert_eq!(resp.status_code(), 201, "add binding: {}", resp.text());
    j(&resp)["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn memory_base(character_id: &str) -> String {
    format!("/v1/daemon/characters/{character_id}/memory")
}

async fn capture(
    server: &TestServer,
    character_id: &str,
    pending_id: &str,
    binding_id: Option<&str>,
    task_kind: Option<&str>,
    digest: &str,
    created_at: &str,
) -> axum_test::TestResponse {
    let mut body = json!({
        "pending_id": pending_id,
        "session_id": format!("sess_{pending_id}"),
        "raw_digest": digest,
        "created_at": created_at,
    });
    if let Some(b) = binding_id {
        body["binding_id"] = json!(b);
    }
    if let Some(k) = task_kind {
        body["task_kind"] = json!(k);
    }
    server
        .post(&format!("{}/pending-review", memory_base(character_id)))
        .json(&body)
        .await
}

async fn count_pending(server: &TestServer, character_id: &str, binding_id: Option<&str>) -> i64 {
    let path = match binding_id {
        Some(b) => format!(
            "{}/pending-review/count?binding_id={b}",
            memory_base(character_id)
        ),
        None => format!("{}/pending-review/count", memory_base(character_id)),
    };
    let resp = server.get(&path).await;
    assert_eq!(resp.status_code(), 200, "count: {}", resp.text());
    resp.json::<Value>()["count"].as_i64().unwrap()
}

async fn sql_count(pool: &sqlx::SqlitePool, table: &str) -> i64 {
    // SAFETY: test-only count over a fixed set of table names.
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_fragment(
    pool: &sqlx::SqlitePool,
    character_id: &str,
    binding_id: Option<&str>,
    fragment_id: &str,
    keyword: &str,
    created_at: &str,
) {
    sqlx::query(
        "INSERT INTO character_memory_fragments \
         (fragment_id, session_id, character_id, actor_world_binding_id, keywords, summary, created_at, ttl, revision) \
         VALUES (?, ?, ?, ?, ?, ?, ?, NULL, 0)",
    )
    .bind(fragment_id)
    .bind(format!("sess_{fragment_id}"))
    .bind(character_id)
    .bind(binding_id)
    .bind(format!(r#"["{keyword}"]"#))
    .bind(format!("summary of {fragment_id}"))
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
}

// ─── Lifecycle ────────────────────────────────────────────────────────────

#[tokio::test]
async fn capture_list_count_delete_lifecycle_and_scope_isolation() {
    let ctx = ctx().await;
    let (chr, bind1) = create_character(&ctx.server, "Ava", WORLD_A).await;
    let bind2 = add_binding(&ctx.server, &chr, WORLD_B).await;

    // Shared capture (no binding provenance).
    let resp = capture(
        &ctx.server,
        &chr,
        "pend_shared_1",
        None,
        Some("brainstorm"),
        FRAGMENT_DIGEST,
        "2026-01-01T00:00:01Z",
    )
    .await;
    assert_eq!(resp.status_code(), 200, "capture: {}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["success"], true);
    assert_eq!(body["pending_id"], "pend_shared_1");

    // Binding-local capture.
    let resp = capture(
        &ctx.server,
        &chr,
        "pend_local_1",
        Some(&bind1),
        Some("research"),
        FRAGMENT_DIGEST,
        "2026-01-01T00:00:02Z",
    )
    .await;
    assert_eq!(resp.status_code(), 200, "capture local: {}", resp.text());

    // Scoped reads: shared scope sees only the shared row; binding scope only its row.
    assert_eq!(count_pending(&ctx.server, &chr, None).await, 1);
    assert_eq!(count_pending(&ctx.server, &chr, Some(&bind1)).await, 1);
    assert_eq!(count_pending(&ctx.server, &chr, Some(&bind2)).await, 0);

    let resp = ctx
        .server
        .get(&format!("{}/pending-review", memory_base(&chr)))
        .await;
    assert_eq!(resp.status_code(), 200);
    let body: Value = resp.json();
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["pending_id"], "pend_shared_1");
    assert_eq!(items[0]["character_id"], chr);
    assert!(items[0].get("binding_id").is_none() || items[0]["binding_id"].is_null());
    assert_eq!(body["pagination"]["has_more"], false);

    let resp = ctx
        .server
        .get(&format!(
            "{}/pending-review?binding_id={bind1}",
            memory_base(&chr)
        ))
        .await;
    let body: Value = resp.json();
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["pending_id"], "pend_local_1");
    assert_eq!(items[0]["binding_id"], bind1);

    // Delete the shared row; the binding-local row survives.
    let resp = ctx
        .server
        .delete(&format!(
            "{}/pending-review/pend_shared_1",
            memory_base(&chr)
        ))
        .await;
    assert_eq!(resp.status_code(), 200, "delete: {}", resp.text());
    assert_eq!(resp.json::<Value>()["success"], true);
    assert_eq!(count_pending(&ctx.server, &chr, None).await, 0);
    assert_eq!(count_pending(&ctx.server, &chr, Some(&bind1)).await, 1);

    // Deleting a missing row is a stable 404.
    let resp = ctx
        .server
        .delete(&format!("{}/pending-review/pend_missing", memory_base(&chr)))
        .await;
    assert_eq!(resp.status_code(), 404);
}

#[tokio::test]
async fn pagination_is_bounded_and_deterministic() {
    let ctx = ctx().await;
    let (chr, _bind1) = create_character(&ctx.server, "Ava", WORLD_A).await;

    for i in 0..5 {
        let resp = capture(
            &ctx.server,
            &chr,
            &format!("pend_page_{i}"),
            None,
            Some("research"),
            FRAGMENT_DIGEST,
            &format!("2026-01-01T00:00:0{i}Z"),
        )
        .await;
        assert_eq!(resp.status_code(), 200);
    }

    // Page 1: limit=2 → newest first (created_at DESC, pending_id DESC).
    let resp = ctx
        .server
        .get(&format!("{}/pending-review?limit=2", memory_base(&chr)))
        .await;
    assert_eq!(resp.status_code(), 200);
    let page1: Value = resp.json();
    let ids1: Vec<&str> = page1["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["pending_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids1, vec!["pend_page_4", "pend_page_3"]);
    assert_eq!(page1["pagination"]["has_more"], true);
    let cursor = page1["pagination"]["next_cursor"].as_str().unwrap().to_string();

    // Page 2 via cursor.
    let resp = ctx
        .server
        .get(&format!(
            "{}/pending-review?limit=2&cursor={cursor}",
            memory_base(&chr)
        ))
        .await;
    let page2: Value = resp.json();
    let ids2: Vec<&str> = page2["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["pending_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids2, vec!["pend_page_2", "pend_page_1"]);
    let cursor2 = page2["pagination"]["next_cursor"]
        .as_str()
        .unwrap()
        .to_string();

    // Page 3: last row, no further cursor.
    let resp = ctx
        .server
        .get(&format!(
            "{}/pending-review?limit=2&cursor={cursor2}",
            memory_base(&chr)
        ))
        .await;
    let page3: Value = resp.json();
    let ids3: Vec<&str> = page3["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["pending_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids3, vec!["pend_page_0"]);
    assert_eq!(page3["pagination"]["has_more"], false);

    // Out-of-range limit is a stable 400.
    let resp = ctx
        .server
        .get(&format!("{}/pending-review?limit=0", memory_base(&chr)))
        .await;
    if resp.status_code() != 422 {
        panic!("limit=0 expected 422 got {} body: {}", resp.status_code(), resp.text());
    }
}

// ─── Review pipeline ──────────────────────────────────────────────────────

#[tokio::test]
async fn review_drains_queue_into_character_storage_only() {
    let ctx = ctx().await;
    let (chr, _bind1) = create_character(&ctx.server, "Ava", WORLD_A).await;

    capture(&ctx.server, &chr, "pend_promote", None, None, PROMOTE_DIGEST, "2026-01-01T00:00:01Z")
        .await;
    capture(
        &ctx.server,
        &chr,
        "pend_fragment",
        None,
        Some("research"),
        FRAGMENT_DIGEST,
        "2026-01-01T00:00:02Z",
    )
    .await;
    capture(
        &ctx.server,
        &chr,
        "pend_drop",
        None,
        Some("research"),
        DROP_DIGEST,
        "2026-01-01T00:00:03Z",
    )
    .await;

    let resp = ctx
        .server
        .post(&format!("{}/review", memory_base(&chr)))
        .json(&json!({}))
        .await;
    assert_eq!(resp.status_code(), 200, "review: {}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["promoted"], 1);
    assert_eq!(body["fragmented"], 1);
    assert_eq!(body["dropped"], 1);
    assert_eq!(body["has_more"], false);

    // Queue drained; fragment landed in Character shared scope.
    assert_eq!(count_pending(&ctx.server, &chr, None).await, 0);
    let resp = ctx
        .server
        .get(&format!("{}/fragments", memory_base(&chr)))
        .await;
    let body: Value = resp.json();
    let fragments = body["fragments"].as_array().unwrap();
    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0]["character_id"], chr);
    assert_eq!(fragments[0]["revision"], 0);

    // The promoted digest was written to the Character's long-term memory dir.
    // The daemon passes `state.nexus_home()` (= `<raw home>/.nexus42`) to the
    // bearer layout helpers, which join `.nexus42` internally — so the
    // promoted file nests under `<nexus_home>/.nexus42/creators/...`. Search
    // the entire tree to stay robust to that convention.
    let mut promoted_md: Vec<_> = Vec::new();
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().and_then(|e| e.to_str()) == Some("md") {
                    out.push(p);
                }
            }
        }
    }
    walk(&ctx.nexus_home, &mut promoted_md);
    assert_eq!(promoted_md.len(), 1, "exactly one promoted LTM file");

    // Creator storage is untouched by every Character operation above.
    assert_eq!(sql_count(&ctx.pool, "memory_pending_review").await, 0);
    assert_eq!(sql_count(&ctx.pool, "memory_fragments").await, 0);
    // The only .md written lives under the Character tree (walked above);
    // Creator SOUL/memory roots are never created.
    assert!(
        !ctx.nexus_home
            .join("creators")
            .join(OWNER)
            .join("memory")
            .exists()
            && !ctx.nexus_home
                .join(".nexus42")
                .join("creators")
                .join(OWNER)
                .join("memory")
                .exists(),
        "Creator memory dir must not be created by Character review"
    );
}

#[tokio::test]
async fn review_scoped_to_binding_processes_only_that_scope() {
    let ctx = ctx().await;
    let (chr, bind1) = create_character(&ctx.server, "Ava", WORLD_A).await;

    capture(&ctx.server, &chr, "pend_shared", None, Some("research"), FRAGMENT_DIGEST, "2026-01-01T00:00:01Z")
        .await;
    capture(
        &ctx.server,
        &chr,
        "pend_local",
        Some(&bind1),
        Some("research"),
        FRAGMENT_DIGEST,
        "2026-01-01T00:00:02Z",
    )
    .await;

    let resp = ctx
        .server
        .post(&format!("{}/review", memory_base(&chr)))
        .json(&json!({ "binding_id": bind1 }))
        .await;
    assert_eq!(resp.status_code(), 200, "review: {}", resp.text());
    assert_eq!(resp.json::<Value>()["fragmented"], 1);

    // The shared row is untouched; the binding-local row was drained.
    assert_eq!(count_pending(&ctx.server, &chr, None).await, 1);
    assert_eq!(count_pending(&ctx.server, &chr, Some(&bind1)).await, 0);

    // The fragment carries the binding-local provenance.
    let resp = ctx
        .server
        .get(&format!("{}/fragments?binding_id={bind1}", memory_base(&chr)))
        .await;
    let fragments = resp.json::<Value>()["fragments"].as_array().unwrap().clone();
    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0]["binding_id"], bind1);
    let resp = ctx.server.get(&format!("{}/fragments", memory_base(&chr))).await;
    assert_eq!(j(&resp)["fragments"].as_array().unwrap().len(), 0);
}

// ─── Promotion ────────────────────────────────────────────────────────────

#[tokio::test]
async fn promotion_is_revision_checked_atomic_and_cache_scoped() {
    let ctx = ctx().await;
    let (chr, bind1) = create_character(&ctx.server, "Ava", WORLD_A).await;

    capture(
        &ctx.server,
        &chr,
        "pend_local",
        Some(&bind1),
        Some("research"),
        FRAGMENT_DIGEST,
        "2026-01-01T00:00:01Z",
    )
    .await;
    let resp = ctx
        .server
        .post(&format!("{}/review", memory_base(&chr)))
        .json(&json!({ "binding_id": bind1 }))
        .await;
    assert_eq!(resp.status_code(), 200);

    let resp = ctx
        .server
        .get(&format!("{}/fragments?binding_id={bind1}", memory_base(&chr)))
        .await;
    let fragments = resp.json::<Value>()["fragments"].as_array().unwrap().clone();
    assert_eq!(fragments.len(), 1);
    let fragment_id = fragments[0]["fragment_id"].as_str().unwrap().to_string();
    assert_eq!(fragments[0]["revision"], 0);

    let promote_path = format!("{}/fragments/{fragment_id}:promote", memory_base(&chr));

    // Stale revision → stable 409 version_mismatch, zero mutation.
    let resp = ctx
        .server
        .post(&promote_path)
        .json(&json!({ "expected_revision": 7 }))
        .await;
    if resp.status_code() != 409 {
        panic!("stale expected 409 got {}: {}", resp.status_code(), resp.text());
    }
    assert_eq!(j(&resp)["error"]["code"], "version_mismatch");
    let resp = ctx
        .server
        .get(&format!("{}/fragments?binding_id={bind1}", memory_base(&chr)))
        .await;
    let after = j(&resp)["fragments"].as_array().unwrap().clone();
    assert_eq!(after.len(), 1, "stale promotion must not mutate");
    assert_eq!(after[0]["revision"], 0);

    // Correct revision → 200; same id, provenance cleared, revision bumped.
    let resp = ctx
        .server
        .post(&promote_path)
        .json(&json!({ "expected_revision": 0 }))
        .await;
    assert_eq!(resp.status_code(), 200, "promote: {}", resp.text());
    let promoted = j(&resp)["fragment"].clone();
    assert_eq!(promoted["fragment_id"], fragment_id);
    assert_eq!(promoted["revision"], 1);
    assert!(
        promoted.get("binding_id").is_none() || promoted["binding_id"].is_null(),
        "promotion clears binding provenance"
    );

    // The fragment now lives in the shared scope only.
    let resp = ctx.server.get(&format!("{}/fragments", memory_base(&chr))).await;
    assert_eq!(j(&resp)["fragments"].as_array().unwrap().len(), 1);
    let resp = ctx
        .server
        .get(&format!("{}/fragments?binding_id={bind1}", memory_base(&chr)))
        .await;
    assert_eq!(j(&resp)["fragments"].as_array().unwrap().len(), 0);

    // Re-promotion of an already-shared fragment → stable 409.
    let resp = ctx
        .server
        .post(&promote_path)
        .json(&json!({ "expected_revision": 1 }))
        .await;
    assert_eq!(resp.status_code(), 409, "re-promote body: {}", resp.text());
    assert_eq!(j(&resp)["error"]["code"], "character_fragment_already_shared");
}

// ─── SOUL narrative reflect ───────────────────────────────────────────────

#[tokio::test]
async fn reflect_reports_insufficient_ungenerated_current_and_stale() {
    let ctx = ctx().await;
    let (chr, _bind1) = create_character(&ctx.server, "Ava", WORLD_A).await;
    let reflect_path = format!("/v1/daemon/characters/{chr}/soul/reflect");

    // Empty scope → insufficient_data, no synthesis attempted.
    let resp = ctx
        .server
        .post(&reflect_path)
        .json(&json!({ "force_regenerate": false }))
        .await;
    assert_eq!(resp.status_code(), 200, "reflect: {}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["character_id"], chr);
    assert_eq!(body["state"], "insufficient_data");
    assert_eq!(body["current_fragment_count"], 0);
    assert_eq!(body["min_fragment_count"], 10);
    assert_eq!(body["min_distinct_keyword_count"], 20);

    // Above the data gate but never synthesized → ungenerated.
    for i in 0..10 {
        seed_fragment(
            &ctx.pool,
            &chr,
            None,
            &format!("frag_{i:02}"),
            &format!("kw_a_{i}"),
            &format!("2026-01-02T00:00:{i:02}Z"),
        )
        .await;
        seed_fragment(
            &ctx.pool,
            &chr,
            None,
            &format!("frag_b_{i:02}"),
            &format!("kw_b_{i}"),
            &format!("2026-01-02T01:00:{i:02}Z"),
        )
        .await;
    }
    let resp = ctx
        .server
        .post(&reflect_path)
        .json(&json!({ "force_regenerate": false }))
        .await;
    let body: Value = resp.json();
    assert_eq!(body["state"], "ungenerated");
    assert_eq!(body["current_fragment_count"], 20);
    assert_eq!(body["current_distinct_keyword_count"], 20);
    assert!(body.get("narrative").is_none() || body["narrative"].is_null());

    // force=true above the gate needs a capability registry; the test daemon
    // has none, so synthesis fails closed with 503 before any LLM call.
    let resp = ctx
        .server
        .post(&reflect_path)
        .json(&json!({ "force_regenerate": true }))
        .await;
    assert_eq!(resp.status_code(), 503, "force: {}", resp.text());

    // Simulate a completed synthesis by caching a narrative at the current
    // stats fingerprint → current, not stale.
    let (stats, _cached) = nexus_local_db::character_soul_narrative_fragment_stats(
        &ctx.pool, OWNER, &chr, None,
    )
    .await
    .unwrap();
    nexus_local_db::upsert_character_soul_narrative(
        &ctx.pool,
        OWNER,
        &nexus_local_db::CharacterSoulNarrativeRecord {
            character_id: chr.clone(),
            actor_world_binding_id: None,
            narrative: Some("Ava keeps choosing mercy, and it keeps costing her.".to_string()),
            generated_at: Some("2026-01-02T02:00:00Z".to_string()),
            fragment_count_at_generation: stats.fragment_count,
            max_fragment_created_at_at_generation: stats.max_created_at.clone(),
            distinct_keyword_count_cache: i64::try_from(stats.distinct_keyword_count).unwrap(),
            stats_fingerprint: Some(nexus_local_db::build_stats_fingerprint(
                stats.fragment_count,
                stats.max_created_at.as_deref(),
            )),
            created_at: "2026-01-02T02:00:00Z".to_string(),
            updated_at: "2026-01-02T02:00:00Z".to_string(),
        },
    )
    .await
    .unwrap();
    let resp = ctx
        .server
        .post(&reflect_path)
        .json(&json!({ "force_regenerate": false }))
        .await;
    let body: Value = resp.json();
    assert_eq!(body["state"], "current");
    assert_eq!(body["stale"], false);
    assert_eq!(
        body["narrative"],
        "Ava keeps choosing mercy, and it keeps costing her."
    );

    // A new fragment diverges the stats → stale.
    seed_fragment(&ctx.pool, &chr, None, "frag_new", "kw_new", "2026-01-03T00:00:00Z").await;
    let resp = ctx
        .server
        .post(&reflect_path)
        .json(&json!({ "force_regenerate": false }))
        .await;
    let body: Value = resp.json();
    assert_eq!(body["state"], "stale");
    assert_eq!(body["stale"], true);
}

// ─── Fail-closed authorization ────────────────────────────────────────────

#[tokio::test]
async fn foreign_missing_inactive_character_and_binding_fail_before_side_effects() {
    let ctx = ctx().await;
    let (chr, bind1) = create_character(&ctx.server, "Ava", WORLD_A).await;

    // A Character owned by a different Creator (seeded directly).
    let foreign = nexus_local_db::create_character_with_initial_binding(
        &ctx.pool,
        nexus_local_db::CreateCharacterParams {
            owner_creator_id: OTHER,
            display_name: "Mordred",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_C,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let foreign_chr = foreign.character.character_id.clone();

    // An inactive Character owned by the active Creator.
    let (archived_chr, archived_bind) = create_character(&ctx.server, "Ghost", WORLD_B).await;
    sqlx::query("UPDATE characters SET status = 'archived' WHERE character_id = ?")
        .bind(&archived_chr)
        .execute(&ctx.pool)
        .await
        .unwrap();

    let missing = "chr_ffffffffffffffffffffffffffffffff";
    for target in [missing, foreign_chr.as_str(), archived_chr.as_str()] {
        let resp = capture(
            &ctx.server,
            target,
            "pend_denied",
            None,
            Some("research"),
            FRAGMENT_DIGEST,
            "2026-01-01T00:00:01Z",
        )
        .await;
        assert!(
            resp.status_code() == 403 || resp.status_code() == 404,
            "capture against {target} must deny, got {}: {}",
            resp.status_code(),
            resp.text()
        );
        let resp = ctx
            .server
            .post(&format!("{}/review", memory_base(target)))
            .json(&json!({}))
            .await;
        assert!(
            resp.status_code() == 403 || resp.status_code() == 404,
            "review against {target} must deny"
        );
        let resp = ctx
            .server
            .post(&format!("/v1/daemon/characters/{target}/soul/reflect"))
            .json(&json!({ "force_regenerate": false }))
            .await;
        assert!(
            resp.status_code() == 403 || resp.status_code() == 404,
            "reflect against {target} must deny"
        );
    }
    // Foreign/missing → 404 (existence hidden from non-owners); inactive → 403.
    let resp = ctx
        .server
        .get(&format!("{}/pending-review", memory_base(missing)))
        .await;
    assert_eq!(resp.status_code(), 404);
    let resp = ctx
        .server
        .get(&format!("{}/pending-review", memory_base(&foreign_chr)))
        .await;
    assert_eq!(resp.status_code(), 404);
    let resp = ctx
        .server
        .get(&format!("{}/pending-review", memory_base(&archived_chr)))
        .await;
    assert_eq!(resp.status_code(), 403);

    // A binding of a different Character is not a valid scope for this one.
    let (chr_b, bind_b) = create_character(&ctx.server, "Boris", WORLD_A).await;
    let resp = capture(
        &ctx.server,
        &chr,
        "pend_denied_binding",
        Some(&bind_b),
        Some("research"),
        FRAGMENT_DIGEST,
        "2026-01-01T00:00:01Z",
    )
    .await;
    assert!(
        resp.status_code() == 403 || resp.status_code() == 404 || resp.status_code() == 400,
        "cross-character binding must deny, got {}",
        resp.status_code()
    );

    // An unknown binding id is rejected the same way.
    let resp = capture(
        &ctx.server,
        &chr,
        "pend_denied_binding2",
        Some("awb_ffffffffffffffffffffffffffffffff"),
        Some("research"),
        FRAGMENT_DIGEST,
        "2026-01-01T00:00:01Z",
    )
    .await;
    assert!(resp.status_code() != 200);

    // Zero side effects: no pending rows or fragments anywhere.
    assert_eq!(
        sql_count(&ctx.pool, "character_memory_pending_review").await,
        0
    );
    assert_eq!(sql_count(&ctx.pool, "character_memory_fragments").await, 0);

    // The archived character's binding scope is also denied.
    let resp = ctx
        .server
        .get(&format!(
            "{}/pending-review?binding_id={archived_bind}",
            memory_base(&archived_chr)
        ))
        .await;
    assert_eq!(resp.status_code(), 403);

    // Valid bindings still work after the deny matrix (no state drift).
    assert_eq!(count_pending(&ctx.server, &chr, Some(&bind1)).await, 0);
    assert_eq!(count_pending(&ctx.server, &chr_b, Some(&bind_b)).await, 0);
}
