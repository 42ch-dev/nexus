//! `strategy_applies` pins every row to the approval it names, and the current apply of a strategy
//! is derivable as the one row with no successor.
//!
//! The pin cannot be a column CHECK — it compares two tables — so it lives in the
//! `pinned_approval_*` triggers: `(strategy_id, content_hash)` must be the named approval's own
//! pair. An apply pairing A's approval with B's strategy is unrepresentable, not merely unwritten.
//! "One current apply per strategy" is deliberately absent: a repeat apply of a superseded approval
//! is the rollback path, so only the applying statement can tell a repeat from a rollback.

use sqlx::SqlitePool;

const STRATEGY_A: &str = "strategy-a";
const STRATEGY_B: &str = "strategy-b";

async fn pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let guarded = nexus_local_db::init_engine_pool(&db_path).await.unwrap();
    (guarded.clone_pool(), dir)
}

/// A frozen approval snapshot that satisfies the `strategy_approvals` mirror CHECKs.
async fn insert_approval(pool: &SqlitePool, strategy: &str, approval_id: &str, content_hash: &str) {
    let document = format!(r#"{{"revision":0,"id":"{strategy}","state":"candidate"}}"#);
    let trajectory = format!(r#"{{"ir_content_hash":"{content_hash}"}}"#);
    sqlx::query(
        "INSERT INTO strategy_approvals \
         (approval_id, strategy_id, content_hash, candidate_revision, document_json, \
          trajectory_json, pins_json, approver, approved_at) \
         VALUES (?, ?, ?, 0, ?, ?, '[]', 'tester', '2026-11-14T00:00:00Z')",
    )
    .bind(approval_id)
    .bind(strategy)
    .bind(content_hash)
    .bind(document)
    .bind(trajectory)
    .execute(pool)
    .await
    .expect("a well-formed approval row is admitted");
}

/// One apply through the admitted writer, so only the table's own constraints can refuse it.
async fn insert_apply(
    pool: &SqlitePool,
    apply_id: &str,
    strategy: &str,
    approval_id: &str,
    content_hash: &str,
    supersedes: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO strategy_applies \
         (apply_id, strategy_id, approval_id, content_hash, applied_at, applied_by, \
          supersedes_apply_id) \
         VALUES (?, ?, ?, ?, '2026-11-14T00:00:00Z', 'tester', ?)",
    )
    .bind(apply_id)
    .bind(strategy)
    .bind(approval_id)
    .bind(content_hash)
    .bind(supersedes)
    .execute(pool)
    .await
    .map(|_| ())
}

/// The apply rows of one strategy that no other row supersedes.
async fn current_applies(pool: &SqlitePool, strategy: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT a.apply_id FROM strategy_applies a \
         WHERE a.strategy_id = ? \
           AND NOT EXISTS (SELECT 1 FROM strategy_applies s WHERE s.supersedes_apply_id = a.apply_id)",
    )
    .bind(strategy)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn an_apply_naming_another_strategys_approval_is_rejected() {
    let (pool, _dir) = pool().await;
    insert_approval(&pool, STRATEGY_A, "approval-a", "hash-a").await;
    insert_approval(&pool, STRATEGY_B, "approval-b", "hash-b").await;

    for (label, strategy, approval_id, content_hash) in [
        // B's approval under A's strategy.
        ("another strategy's approval", STRATEGY_A, "approval-b", "hash-b"),
        // A's own approval labelled with a hash it does not carry.
        ("a hash the approval does not have", STRATEGY_A, "approval-a", "hash-b"),
    ] {
        let error = insert_apply(&pool, "apply-1", strategy, approval_id, content_hash, None)
            .await
            .expect_err(&format!("`{label}` must fail the approval pin"));
        assert!(
            error.to_string().contains("APPLY_APPROVAL_MISMATCH"),
            "`{label}`: {error}"
        );
    }

    // The pin rejects only the mismatched pair: the approval's own pair is admitted.
    insert_apply(&pool, "apply-1", STRATEGY_A, "approval-a", "hash-a", None)
        .await
        .expect("the named approval's own strategy and hash are admitted");
}

/// The migration is additive: it neither backfills nor rewrites, so re-applying it to a database
/// that already holds rows creates only the new table and leaves the existing rows untouched.
#[tokio::test]
async fn the_migration_applies_to_a_database_that_already_holds_rows() {
    let (pool, _dir) = pool().await;
    insert_approval(&pool, STRATEGY_A, "approval-a", "hash-a").await;

    // Return the database to the shape it had before this migration: its triggers go with the
    // table, and sqlx only re-applies a migration whose record is gone.
    sqlx::query("DROP TABLE strategy_applies")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 20261114000001")
        .execute(&pool)
        .await
        .unwrap();

    nexus_local_db::run_migrations(&pool).await.unwrap();

    let approvals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM strategy_approvals")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(approvals, 1, "the existing approval row survives");
    insert_apply(&pool, "apply-1", STRATEGY_A, "approval-a", "hash-a", None)
        .await
        .expect("the re-applied migration admits an apply");
}

#[tokio::test]
async fn an_apply_cannot_supersede_itself() {
    let (pool, _dir) = pool().await;
    insert_approval(&pool, STRATEGY_A, "approval-a", "hash-a").await;

    let error = insert_apply(
        &pool,
        "apply-1",
        STRATEGY_A,
        "approval-a",
        "hash-a",
        Some("apply-1"),
    )
    .await
    .expect_err("a self-supersede must fail");
    assert!(
        error.to_string().contains("CHECK constraint failed"),
        "{error}"
    );
}

#[tokio::test]
async fn a_superseding_apply_leaves_exactly_one_current_row() {
    let (pool, _dir) = pool().await;
    insert_approval(&pool, STRATEGY_A, "approval-a", "hash-a").await;
    insert_apply(&pool, "apply-1", STRATEGY_A, "approval-a", "hash-a", None)
        .await
        .unwrap();

    assert_eq!(current_applies(&pool, STRATEGY_A).await, ["apply-1"]);

    insert_apply(
        &pool,
        "apply-2",
        STRATEGY_A,
        "approval-a",
        "hash-a",
        Some("apply-1"),
    )
    .await
    .unwrap();

    // Both rows stay readable; only the successor-less one is current.
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM strategy_applies")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 2);
    assert_eq!(current_applies(&pool, STRATEGY_A).await, ["apply-2"]);
}
