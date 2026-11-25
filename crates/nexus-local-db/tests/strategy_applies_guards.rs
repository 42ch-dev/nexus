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
    let rows: Vec<&str> = plan.lines().collect();
    assert!(
        rows.iter()
            .any(|row| search_names_index(row, "idx_strategy_applies_strategy_current")),
        "the strategy filter and the total order are one SEARCH of their index: {plan}"
    );
    assert!(
        rows.iter()
            .any(|row| search_names_index(row, "idx_strategy_applies_supersedes")),
        "the anti-join's inner side is a SEARCH of the supersede-link index: {plan}"
    );
    assert!(
        !rows.iter().any(|row| scans_strategy_applies(row)),
        "neither side of the lookup may walk strategy_applies, by table or by index: {plan}"
    );
}

/// Whether a plan row is a `SEARCH` through exactly `index`: SQLite spells its leaf lookups
/// `SEARCH <alias> USING [COVERING] INDEX <name>`, so the prefix is the shape and the name after
/// `INDEX` is compared whole — a longer name that merely starts with the expected one is a
/// different index, not a match.
fn search_names_index(detail: &str, index: &str) -> bool {
    detail.starts_with("SEARCH") && searched_index(detail) == Some(index)
}

/// The index a plan row seeks through, or `None` when the row names no index. Real plans write the
/// name unquoted and echo a constraint list after it, so the candidate is the text after
/// `USING [COVERING] INDEX` with a trailing constraint list removed. A surrounding quote pair is
/// accepted and removed; an unterminated quote is refused, not compared.
fn searched_index(detail: &str) -> Option<&str> {
    let (_, after_using) = detail.split_once("USING ")?;
    let after_using = after_using.strip_prefix("COVERING ").unwrap_or(after_using);
    let candidate = after_using.strip_prefix("INDEX ")?;
    unquote_identifier(without_constraint_list(candidate))
}

/// Drops the plan's echoed constraint list, if and only if the text ends with one: the boundary is
/// the LAST ` (`, and only a suffix shaped `(...=...)` is a constraint. Anything else — including an
/// index name that itself contains ` (` — stays one candidate, which then cannot equal a bare
/// expected name by accident.
fn without_constraint_list(candidate: &str) -> &str {
    let text = candidate.trim();
    let Some(open) = text.rfind(" (") else {
        return text;
    };
    let suffix = &text[open + 1..];
    if suffix.starts_with('(') && suffix.ends_with(')') && suffix.contains('=') {
        text[..open].trim_end()
    } else {
        text
    }
}

/// Removes one pair of surrounding quotes, or refuses the candidate: an opening quote without its
/// close is not an identifier a comparison may use.
fn unquote_identifier(candidate: &str) -> Option<&str> {
    let opening = candidate.chars().next()?;
    let closing = match opening {
        '"' | '\'' | '`' => opening,
        '[' => ']',
        _ => return Some(candidate),
    };
    let inner = &candidate[opening.len_utf8()..];
    let end = inner.find(closing)?;
    inner[end + closing.len_utf8()..]
        .is_empty()
        .then_some(&inner[..end])
}

/// A plan row that walks `strategy_applies` in full — a `SCAN` of the table or of either alias,
/// whether or not an index is named. `SCAN cur USING INDEX ...` is a full index walk, as much a
/// degradation as `SCAN cur`.
fn scans_strategy_applies(detail: &str) -> bool {
    detail.starts_with("SCAN")
        && detail
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|token| matches!(token, "strategy_applies" | "cur" | "later"))
}

/// A full walk through an index is still a walk: only `SCAN` rows are refused, so the guard must
/// reject them however SQLite names the index, and must not mistake an unrelated scan for one.
#[test]
fn a_full_index_walk_is_a_scan() {
    assert!(scans_strategy_applies(
        "SCAN cur USING INDEX idx_strategy_applies_strategy_current"
    ));
    assert!(scans_strategy_applies("SCAN later"));
    assert!(scans_strategy_applies("SCAN TABLE strategy_applies"));
    assert!(!scans_strategy_applies(
        "SEARCH cur USING INDEX idx_strategy_applies_strategy_current"
    ));
    assert!(!scans_strategy_applies(
        "SEARCH later USING COVERING INDEX idx_strategy_applies_supersedes"
    ));
    assert!(!scans_strategy_applies(
        "SCAN constants USING INDEX idx_unrelated"
    ));
}

/// The index is identified whole: a plan that seeks a superset-named index is a different index,
/// so it must not satisfy the check for the expected one.
#[test]
fn a_search_must_name_the_expected_index_exactly() {
    assert!(search_names_index(
        "SEARCH cur USING INDEX idx_strategy_applies_strategy_current (strategy_id=?)",
        "idx_strategy_applies_strategy_current"
    ));
    assert!(search_names_index(
        "SEARCH later USING COVERING INDEX idx_strategy_applies_supersedes (supersedes_apply_id=?)",
        "idx_strategy_applies_supersedes"
    ));
    assert!(!search_names_index(
        "SEARCH cur USING INDEX idx_strategy_applies_strategy_current_v2 (strategy_id=?)",
        "idx_strategy_applies_strategy_current"
    ));
    assert!(!search_names_index(
        "SEARCH cur USING INDEX idx_strategy_applies_strategy_current",
        "idx_strategy_applies_strategy_current_v2"
    ));
    assert!(!search_names_index(
        "SEARCH cur USING INTEGER PRIMARY KEY (rowid=?)",
        "idx_strategy_applies_strategy_current"
    ));
    assert!(!search_names_index(
        "SCAN cur USING INDEX idx_strategy_applies_strategy_current",
        "idx_strategy_applies_strategy_current"
    ));
}

/// The candidate is read as the plan writes it: unquoted, ending at the parameter list, and
/// compared whole — a name with a space is one name, never truncated at the space. A quote pair
/// around it is removed; an unterminated quote is refused instead of compared.
#[test]
fn plan_index_names_are_read_whole() {
    assert_eq!(
        searched_index(
            "SEARCH cur USING INDEX idx_strategy_applies_strategy_current (strategy_id=?)"
        ),
        Some("idx_strategy_applies_strategy_current")
    );
    assert_eq!(
        searched_index(
            "SEARCH later USING COVERING INDEX idx_strategy_applies_supersedes (supersedes_apply_id=?)"
        ),
        Some("idx_strategy_applies_supersedes")
    );
    assert_eq!(
        searched_index("SEARCH cur USING INDEX \"idx strategy current\" (strategy_id=?)"),
        Some("idx strategy current"),
        "the whole spaced name is the candidate, not its first word"
    );
    assert_eq!(
        searched_index("SEARCH cur USING INDEX \"idx strategy current\""),
        Some("idx strategy current"),
        "the end of the line is the other boundary"
    );
    assert_eq!(
        searched_index("SEARCH cur USING INDEX idx_strategy_applies_strategy_current (extra)"),
        Some("idx_strategy_applies_strategy_current (extra)"),
        "a parenthesised tail without `=` is part of the name, not a constraint list"
    );
    assert!(
        !search_names_index(
            "SEARCH cur USING INDEX idx_strategy_applies_strategy_current (extra)",
            "idx_strategy_applies_strategy_current"
        ),
        "a name followed by a non-constraint tail is a different name"
    );
    assert!(
        search_names_index(
            "SEARCH cur USING INDEX idx_strategy_applies_strategy_current (strategy_id=?)",
            "idx_strategy_applies_strategy_current"
        ),
        "an echoed constraint list is a boundary, not part of the name"
    );
    assert_eq!(
        searched_index("SEARCH cur USING INDEX \"idx_strategy_applies_strategy_current"),
        None,
        "an unterminated quote is refused"
    );
    assert_eq!(
        searched_index("SEARCH cur USING INTEGER PRIMARY KEY (rowid=?)"),
        None,
        "a row that names no index has no candidate"
    );
}
