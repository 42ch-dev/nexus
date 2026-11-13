//! The `strategy_candidates` mirror CHECKs reject a document whose JSON member does not mirror the
//! row — including a `null` or missing one.
//!
//! SQLite passes a CHECK whose expression is NULL, so `json_extract(document_json, '$.revision')
//! = revision` accepts `revision: null` on its own. The type conjunction is what makes the
//! documented "a row whose document disagrees is unrepresentable" true for an admitted SQL writer;
//! the reader-side `decode_stored` check stays the fail-closed boundary.

use serde_json::{json, Value};
use sqlx::SqlitePool;

const STRATEGY: &str = "novel-writing";

async fn pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let guarded = nexus_local_db::init_engine_pool(&db_path).await.unwrap();
    (guarded.clone_pool(), dir)
}

/// One raw insert through the admitted writer, so the CHECK is the only thing that can refuse it.
async fn insert(pool: &SqlitePool, revision: i64, document: &Value) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO strategy_candidates \
         (strategy_id, document_json, revision, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(STRATEGY)
    .bind(serde_json::to_string(document).unwrap())
    .bind(revision)
    .bind("2026-11-13T00:00:00Z")
    .bind("2026-11-13T00:00:00Z")
    .execute(pool)
    .await
    .map(|_| ())
}

#[tokio::test]
async fn the_mirror_check_accepts_a_matching_document() {
    let (pool, _dir) = pool().await;
    insert(
        &pool,
        0,
        &json!({ "revision": 0, "id": STRATEGY, "state": "candidate" }),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn the_mirror_check_rejects_a_null_or_missing_member() {
    let (pool, _dir) = pool().await;
    for (label, document) in [
        (
            "revision: null",
            json!({ "revision": Value::Null, "id": STRATEGY, "state": "candidate" }),
        ),
        (
            "revision missing",
            json!({ "id": STRATEGY, "state": "candidate" }),
        ),
        (
            "revision disagrees",
            json!({ "revision": 1, "id": STRATEGY, "state": "candidate" }),
        ),
        (
            "id: null",
            json!({ "revision": 0, "id": Value::Null, "state": "candidate" }),
        ),
        ("id missing", json!({ "revision": 0, "state": "candidate" })),
        ("state missing", json!({ "revision": 0, "id": STRATEGY })),
    ] {
        let error = insert(&pool, 0, &document)
            .await
            .expect_err(&format!("`{label}` must fail the mirror CHECK"));
        assert!(
            error.to_string().contains("CHECK constraint failed"),
            "`{label}`: {error}"
        );
    }
}
