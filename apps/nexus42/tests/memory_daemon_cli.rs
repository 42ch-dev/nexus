//! Creator memory review family against the direct-core fixture (v1.193
//! P0-T12).
//!
//! `creator memory review|fragments|pending-list|pending count|pending-show|
//! pending-dismiss` run the REAL `nexus42` binary against a hermetic
//! direct-core home — no daemon fixture, no HTTP client. The queue rows are
//! seeded on the released workspace DB and every read is the typed core's own
//! bearer operation. `pending-show` walks list pagination (PR #230 Greptile
//! P1) so an ID past the first 50-row page is still found.

#[path = "common/direct.rs"]
mod direct;

use direct::DirectFixture;
use nexus_home_layout::{nexus_root_from_home, workspace_state_db_path};
use nexus_local_db::pending_review::{create_pending_review, PendingReviewRecord};
use nexus_local_db::writer_protocol::release_retained_writer_guards;
use std::path::PathBuf;
use std::process::Output;

/// Workspace the fixture materializes and selects.
const WORKSPACE_SLUG: &str = "default";

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A hermetic direct-core home with one active creator/workspace.
struct MemoryEnv {
    fixture: DirectFixture,
    creator_id: String,
}

impl MemoryEnv {
    /// Run the real `nexus42` binary against this fixture's hermetic `HOME`.
    fn cli(&self, args: &[&str]) -> Output {
        self.fixture
            .command()
            .args(args)
            .output()
            .expect("spawn nexus42")
    }
}

/// The fixture home holds exactly one creator; its id is the directory name
/// under `~/.nexus42/creators/`.
fn fixture_creator_id(fixture: &DirectFixture) -> String {
    let creators_root = nexus_root_from_home(fixture.home.path()).join("creators");
    let mut entries: Vec<_> = std::fs::read_dir(&creators_root)
        .expect("read fixture creators root")
        .map(|entry| entry.expect("creator dir entry").file_name())
        .collect();
    assert_eq!(entries.len(), 1, "fixture registers exactly one creator");
    entries
        .pop()
        .expect("one creator")
        .to_string_lossy()
        .into_owned()
}

async fn fresh_env() -> MemoryEnv {
    let fixture = DirectFixture::new().await;
    let creator_id = fixture_creator_id(&fixture);
    MemoryEnv {
        fixture,
        creator_id,
    }
}

/// The released workspace state DB, opened for seeding only.
///
/// [`release`](WorkspaceSeed::release) hands the workspace back: the real CLI
/// child must never start while a seed writer is still admitted.
struct WorkspaceSeed {
    pool: sqlx::SqlitePool,
    db_path: PathBuf,
}

impl WorkspaceSeed {
    async fn open(env: &MemoryEnv) -> Self {
        let db_path =
            workspace_state_db_path(env.fixture.home.path(), &env.creator_id, WORKSPACE_SLUG);
        let pool = nexus_local_db::init_engine_pool(&db_path)
            .await
            .expect("workspace pool")
            .clone_pool();
        Self { pool, db_path }
    }

    async fn release(self) {
        self.pool.close().await;
        release_retained_writer_guards(&self.db_path);
    }
}

/// Seed `n` pending-review rows for the fixture creator.
async fn seed_pending(env: &MemoryEnv, n: usize) {
    let seed = WorkspaceSeed::open(env).await;
    for i in 0..n {
        let record = PendingReviewRecord {
            pending_id: format!("pending_test_{i}"),
            session_id: format!("sess_test_{i}"),
            creator_id: env.creator_id.clone(),
            world_id: Some("wld_test_world".to_string()),
            task_kind: "research".to_string(),
            raw_digest: format!(
                "Research digest for pending review {i}: a sufficiently long body of \
                 informational content that classifies as FragmentOnly for research tasks."
            ),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        create_pending_review(&seed.pool, &record)
            .await
            .expect("seed pending review");
    }
    seed.release().await;
}

/// Seed `n` pending-review rows with strictly-decreasing `created_at` so the
/// core's `created_at DESC` page order is deterministic (row `0` newest).
async fn seed_pending_desc(env: &MemoryEnv, n: usize) {
    let seed = WorkspaceSeed::open(env).await;
    let base = chrono::Utc::now();
    for i in 0..n {
        // `i` is bounded by the test's seed count (60), far below `i64::MAX`.
        let minutes_back = i64::try_from(i).expect("seed count fits in i64");
        let record = PendingReviewRecord {
            pending_id: format!("pending_test_{i}"),
            session_id: format!("sess_test_{i}"),
            creator_id: env.creator_id.clone(),
            world_id: Some("wld_test_world".to_string()),
            task_kind: "research".to_string(),
            raw_digest: format!(
                "Research digest for pending review {i}: a sufficiently long body of \
                 informational content that counts as FragmentOnly for research tasks."
            ),
            created_at: (base - chrono::Duration::minutes(minutes_back)).to_rfc3339(),
        };
        create_pending_review(&seed.pool, &record)
            .await
            .expect("seed pending review");
    }
    seed.release().await;
}

/// Seed one stored memory fragment for the fixture creator.
async fn seed_fragment(env: &MemoryEnv) {
    let seed = WorkspaceSeed::open(env).await;
    sqlx::query(
        "INSERT INTO memory_fragments \
         (fragment_id, session_id, creator_id, keywords, summary, created_at, ttl) \
         VALUES ('frag_test_1', 'sess_test_1', ?, '[]', \
                 'A test fragment summary', datetime('now'), NULL)",
    )
    .bind(&env.creator_id)
    .execute(&seed.pool)
    .await
    .expect("seed memory fragment");
    seed.release().await;
}

// ── pending count ─────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_count_reports_seeded_depth() {
    let env = fresh_env().await;
    seed_pending(&env, 3).await;

    let out = env.cli(&["creator", "memory", "pending", "count"]);
    assert!(out.status.success(), "count failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("3 pending review(s)"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_count_json_emits_dto_verbatim() {
    let env = fresh_env().await;
    seed_pending(&env, 2).await;

    let out = env.cli(&["creator", "memory", "pending", "count", "--json"]);
    assert!(out.status.success(), "count failed: {}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(parsed["count"], 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_count_zero_when_empty() {
    let env = fresh_env().await;
    let out = env.cli(&["creator", "memory", "pending", "count"]);
    assert!(out.status.success(), "count failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("0 pending review(s)"), "{text}");
}

// ── review drain ──────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn review_drains_small_queue() {
    let env = fresh_env().await;
    seed_pending(&env, 2).await;

    let out = env.cli(&["creator", "memory", "review"]);
    assert!(out.status.success(), "review failed: {}", stderr(&out));
    let text = stdout(&out);
    // Research digests classify as FragmentOnly → fragmented count ≥ 1.
    assert!(text.contains("fragmented="), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn review_json_emits_cumulative_report() {
    let env = fresh_env().await;
    seed_pending(&env, 1).await;

    let out = env.cli(&["creator", "memory", "review", "--json"]);
    assert!(out.status.success(), "review failed: {}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert!(parsed["fragmented"].as_i64().unwrap_or(0) >= 1);
    assert_eq!(parsed["has_more"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn review_empty_queue_prints_no_pending() {
    let env = fresh_env().await;
    let out = env.cli(&["creator", "memory", "review"]);
    assert!(out.status.success(), "review failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("No pending memories"), "{text}");
}

// ── pending-list --json ────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_list_json_emits_dto_verbatim() {
    let env = fresh_env().await;
    seed_pending(&env, 1).await;

    let out = env.cli(&["creator", "memory", "pending-list", "--json"]);
    assert!(
        out.status.success(),
        "pending-list failed: {}",
        stderr(&out)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    let items = parsed["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["pending_id"], "pending_test_0");
}

// ── fragments --json (AR-83 #3: wire DTO verbatim) ───────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fragments_json_emits_wrapper_dto() {
    let env = fresh_env().await;
    seed_fragment(&env).await;

    let out = env.cli(&["creator", "memory", "fragments", "--json"]);
    assert!(out.status.success(), "fragments failed: {}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    // The wire shape is `{ "fragments": [ … ] }` — the wrapper, not a bare array.
    let fragments = parsed["fragments"]
        .as_array()
        .expect("fragments wrapper array");
    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0]["fragment_id"], "frag_test_1");
    assert_eq!(fragments[0]["summary"], "A test fragment summary");
}

// ── pending-show pagination walk (PR #230 Greptile P1) ────────────────────

/// `pending-show` must find an ID past the first page (50-row default list
/// limit): the leaf walks `pagination.next_cursor` until the ID is found or
/// the pages are exhausted instead of reporting not-found from page 1.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_show_finds_id_beyond_first_page() {
    let env = fresh_env().await;
    // Row `pending_test_0` is newest → it lands on page 1; row 55 lands on
    // page 2 (newest-first DESC order, 50 rows/page).
    seed_pending_desc(&env, 60).await;

    let out = env.cli(&["creator", "memory", "pending-show", "pending_test_55"]);
    assert!(
        out.status.success(),
        "pending-show failed: {}",
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("pending_id: pending_test_55"), "{text}");
    assert!(text.contains("sess_test_55"), "{text}");
}

/// The pagination walk must terminate with the not-found error when the ID
/// does not exist anywhere (bounded loop — no infinite page following).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_show_missing_id_past_first_page_fails_closed() {
    let env = fresh_env().await;
    seed_pending_desc(&env, 60).await;

    let out = env.cli(&[
        "creator",
        "memory",
        "pending-show",
        "pending_does_not_exist",
    ]);
    assert!(!out.status.success(), "missing pending-show must fail");
    let err = stderr(&out);
    assert!(
        err.contains("not found"),
        "stderr should surface not-found: {err}"
    );
}
