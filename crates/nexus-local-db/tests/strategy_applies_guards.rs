//! `strategy_applies` pins every row to the approval it names, and the current apply of a strategy
//! is derivable as the one row with no successor.
//!
//! The pin cannot be a column CHECK — it compares two tables — so it lives in the
//! `pinned_approval_*` triggers: `(strategy_id, content_hash)` must be the named approval's own
//! pair. An apply pairing A's approval with B's strategy is unrepresentable, not merely unwritten.
//! "One current apply per strategy" is deliberately absent: a repeat apply of a superseded approval
//! is the rollback path, so only the applying statement can tell a repeat from a rollback.
//!
//! I-000444 adds the rest of the documented claim: a committed row refuses `UPDATE`, `DELETE` and
//! `INSERT OR REPLACE`, and a supersede link may only name an apply of the same strategy.

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
        (
            "another strategy's approval",
            STRATEGY_A,
            "approval-b",
            "hash-b",
        ),
        (
            "a hash the approval does not have",
            STRATEGY_A,
            "approval-a",
            "hash-b",
        ),
    ] {
        let error = insert_apply(&pool, "apply-1", strategy, approval_id, content_hash, None)
            .await
            .expect_err(&format!("`{label}` must fail the approval pin"));
        assert!(
            error.to_string().contains("APPLY_APPROVAL_MISMATCH"),
            "`{label}`: {error}"
        );
    }

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

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM strategy_applies")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 2);
    assert_eq!(current_applies(&pool, STRATEGY_A).await, ["apply-2"]);
}

/// I-000444: a committed apply is immutable, not merely approval-pinned. Every mutation an
/// admitted writer can spell is refused — `UPDATE`, `DELETE`, and the `REPLACE` spelling whose
/// conflict resolution deletes the victim without firing its DELETE trigger — and a supersede
/// link may not name another strategy's row.
#[tokio::test]
async fn a_committed_apply_is_immutable_and_may_only_supersede_its_own_strategy() {
    let (pool, _dir) = pool().await;
    insert_approval(&pool, STRATEGY_A, "approval-a", "hash-a").await;
    insert_apply(&pool, "apply-a", STRATEGY_A, "approval-a", "hash-a", None)
        .await
        .unwrap();
    insert_approval(&pool, STRATEGY_B, "approval-b", "hash-b").await;
    insert_apply(&pool, "apply-b", STRATEGY_B, "approval-b", "hash-b", None)
        .await
        .unwrap();

    let update = sqlx::query(
        "UPDATE strategy_applies SET applied_by = 'someone-else' WHERE apply_id = 'apply-a'",
    )
    .execute(&pool)
    .await
    .expect_err("a committed apply must refuse UPDATE");
    assert!(
        update.to_string().contains("APPLY_ROW_IMMUTABLE"),
        "{update}"
    );

    let delete = sqlx::query("DELETE FROM strategy_applies WHERE apply_id = 'apply-a'")
        .execute(&pool)
        .await
        .expect_err("a committed apply must refuse DELETE");
    assert!(
        delete.to_string().contains("APPLY_ROW_IMMUTABLE"),
        "{delete}"
    );

    let replace = sqlx::query(
        "INSERT OR REPLACE INTO strategy_applies \
         (apply_id, strategy_id, approval_id, content_hash, applied_at, applied_by, \
          supersedes_apply_id) \
         VALUES ('apply-a', ?, 'approval-a', 'hash-a', '2026-11-16T00:00:00Z', 'tester', NULL)",
    )
    .bind(STRATEGY_A)
    .execute(&pool)
    .await
    .expect_err("REPLACE must not delete-and-reinsert a committed apply");
    assert!(
        replace.to_string().contains("APPLY_ROW_IMMUTABLE"),
        "{replace}"
    );

    let cross = insert_apply(
        &pool,
        "apply-a-2",
        STRATEGY_A,
        "approval-a",
        "hash-a",
        Some("apply-b"),
    )
    .await
    .expect_err("a supersede may not name another strategy's apply");
    assert!(
        cross
            .to_string()
            .contains("APPLY_SUPERSEDE_STRATEGY_MISMATCH"),
        "{cross}"
    );

    let applied_by: String =
        sqlx::query_scalar("SELECT applied_by FROM strategy_applies WHERE apply_id = 'apply-a'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(applied_by, "tester");
    assert_eq!(current_applies(&pool, STRATEGY_A).await, ["apply-a"]);
    assert_eq!(current_applies(&pool, STRATEGY_B).await, ["apply-b"]);
}

/// `EXPLAIN QUERY PLAN` of the query `apply.rs` runs for the current apply (the same text with
/// the plan prefix): the strategy's successor-less rows in the total order the reads pick a winner
/// by. `sqlx` requires the statement to be a literal, hence one constant rather than a format
/// argument.
// SAFETY: test-only EXPLAIN QUERY PLAN inspection; static SQL mirror.
const CURRENT_ROW_LOOKUP_PLAN: &str = "EXPLAIN QUERY PLAN \
     SELECT cur.apply_id, cur.approval_id, cur.content_hash, cur.applied_at, \
            cur.supersedes_apply_id \
     FROM strategy_applies cur \
     WHERE cur.strategy_id = 'strategy-a' \
       AND NOT EXISTS (SELECT 1 FROM strategy_applies later \
                        WHERE later.supersedes_apply_id = cur.apply_id) \
     ORDER BY cur.applied_at DESC, cur.apply_id DESC \
     LIMIT 1";

async fn query_plan(pool: &SqlitePool, sql: &'static str) -> String {
    sqlx::query_as::<_, (i64, i64, i64, String)>(sql)
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .map(|(_, _, _, detail)| detail)
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn the_current_apply_lookup_is_two_index_searches_not_two_scans() {
    let (pool, _dir) = pool().await;
    insert_approval(&pool, STRATEGY_A, "approval-a", "hash-a").await;
    insert_apply(&pool, "apply-1", STRATEGY_A, "approval-a", "hash-a", None)
        .await
        .unwrap();
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

    let plan = query_plan(&pool, CURRENT_ROW_LOOKUP_PLAN).await;
    assert!(
        plan.contains("idx_strategy_applies_strategy_current"),
        "the strategy filter and the total order are one index range: {plan}"
    );
    assert!(
        plan.contains("idx_strategy_applies_supersedes"),
        "the anti-join's inner side is a supersede-link index lookup: {plan}"
    );
    assert!(
        !plan.lines().any(full_scan_of_strategy_applies),
        "neither side of the lookup may fall back to a full table scan: {plan}"
    );
}

/// A plan row that walks `strategy_applies` in full: `SCAN` of one alias (or the table) with no
/// index named. SQLite's wording is free to change, so only the scan shape is matched.
fn full_scan_of_strategy_applies(detail: &str) -> bool {
    detail.starts_with("SCAN")
        && !detail.contains("USING")
        && ["strategy_applies", " cur", " later"]
            .iter()
            .any(|name| detail.contains(name))
}
