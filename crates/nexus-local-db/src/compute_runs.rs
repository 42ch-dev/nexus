//! Compute run persistence — direct Control Room lane (V1.147 P0 T2).
//!
//! Augments the existing `compute_sessions` table with direct-lane columns
//! (`run_id`, `world_id`, `module_id`, `module_version`, `status`,
//! `proposals_json`, `error_json`, `updated_at`, `accepted_at`,
//! `invocation_params_json`) while preserving spoke adapter backward
//! compatibility (spoke columns `session_id`, `entry_id`, `state_json`).
//!
//! Direct-lane rows have NULL spoke columns; adapter rows have NULL
//! direct-lane columns. Both shapes round-trip through [`ComputeRunRow`].

use crate::LocalDbError;
use sqlx::SqlitePool;

/// A row in the `compute_sessions` table covering the direct-lane columns
/// added by V1.147 P0.
#[derive(Debug, Clone)]
pub struct ComputeRunRow {
    pub run_id: String,
    pub world_id: String,
    pub module_id: String,
    pub module_version: Option<String>,
    pub status: String,
    pub proposals_json: Option<String>,
    pub error_json: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub accepted_at: Option<String>,
    pub invocation_params_json: Option<String>,
}

/// Valid status strings for the direct lane.
pub const RUN_STATUS_RUNNING: &str = "running";
pub const RUN_STATUS_SUCCEEDED: &str = "succeeded";
pub const RUN_STATUS_FAILED: &str = "failed";
pub const RUN_STATUS_APPLIED: &str = "applied";
pub const RUN_STATUS_DISCARDED: &str = "discarded";

/// Filters for [`list_runs`].
#[derive(Debug, Clone, Default)]
pub struct RunListFilters {
    pub world_id: Option<String>,
    pub module_id: Option<String>,
    pub status: Option<String>,
    pub creator_world_ids: Option<Vec<String>>,
}

/// Insert a new compute run row and return its generated `run_id`.
///
/// The `entry_id` column is set to `''` (empty string) because the
/// original schema defines it as `TEXT NOT NULL`, yet direct-lane rows
/// have no spoke entry.  Adapter reads key on `session_id`, never
/// `entry_id`, and `session_id` is NULL for direct-lane rows — SQL NULL
/// semantics guarantee the adapter's `WHERE session_id = ?` predicates
/// never match these rows.  `''` is also distinguishable from valid
/// KB entry ids (UUIDs), so it cannot leak into spoke-adapter reads.
///
/// # Errors
/// Returns [`LocalDbError`] on database failure.
pub async fn insert_run(
    pool: &SqlitePool,
    world_id: &str,
    module_id: &str,
    module_version: Option<&str>,
    invocation_params_json: Option<&str>,
) -> Result<String, LocalDbError> {
    let run_id = format!("run_{}", uuid::Uuid::new_v4());
    let created_at = chrono::Utc::now().to_rfc3339();
    sqlx::query!(
        "INSERT INTO compute_sessions \
         (run_id, world_id, module_id, module_version, status, \
          proposals_json, error_json, created_at, updated_at, accepted_at, \
          invocation_params_json, entry_id) \
         VALUES (?, ?, ?, ?, 'running', NULL, NULL, ?, NULL, NULL, ?, '')",
        run_id,
        world_id,
        module_id,
        module_version,
        created_at,
        invocation_params_json,
    )
    .execute(pool)
    .await?;
    Ok(run_id)
}

/// Look up a compute run by `run_id`.
///
/// Returns `Ok(None)` when no direct-lane row with the given `run_id` exists.
///
/// # Panics
/// Panics if the database returns a direct-lane row with a NULL
/// `run_id`/`world_id`/`module_id`/`status` column (violates the
/// direct-lane invariant).
///
/// # Errors
/// Returns [`LocalDbError`] on database failure.
pub async fn get_run(
    pool: &SqlitePool,
    run_id: &str,
) -> Result<Option<ComputeRunRow>, LocalDbError> {
    let row = sqlx::query!(
        "SELECT run_id, world_id, module_id, module_version, status, \
         proposals_json, error_json, created_at, updated_at, accepted_at, \
         invocation_params_json \
         FROM compute_sessions WHERE run_id = ?",
        run_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| ComputeRunRow {
        run_id: r.run_id.expect("run_id is not null for direct-lane rows"),
        world_id: r
            .world_id
            .expect("world_id is not null for direct-lane rows"),
        module_id: r
            .module_id
            .expect("module_id is not null for direct-lane rows"),
        module_version: r.module_version,
        status: r.status,
        proposals_json: r.proposals_json,
        error_json: r.error_json,
        created_at: r.created_at,
        updated_at: r.updated_at,
        accepted_at: r.accepted_at,
        invocation_params_json: r.invocation_params_json,
    }))
}

/// Update a run to `succeeded` status with proposals.
///
/// Clears `error_json` (set to NULL) and sets `updated_at` to now.
/// Only fires when the run is in `running` status — this prevents
/// a completed or discarded run from being overwritten.
///
/// Returns the number of affected rows (1 on success).
///
/// # Errors
/// Returns [`LocalDbError`] on database failure, or
/// [`LocalDbError::ConstraintViolation`] when the run does not exist
/// or is not in `running` status (0 rows affected).
pub async fn set_run_succeeded(
    pool: &SqlitePool,
    run_id: &str,
    proposals_json: &str,
) -> Result<u64, LocalDbError> {
    let updated_at = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query!(
        "UPDATE compute_sessions \
         SET status = 'succeeded', proposals_json = ?, error_json = NULL, updated_at = ? \
         WHERE run_id = ? AND status = 'running'",
        proposals_json,
        updated_at,
        run_id,
    )
    .execute(pool)
    .await?;
    let affected = result.rows_affected();
    if affected == 0 {
        return Err(LocalDbError::ConstraintViolation {
            table: "compute_sessions".to_string(),
            constraint: format!(
                "run {run_id} is not in 'running' status — cannot transition to 'succeeded'"
            ),
        });
    }
    Ok(affected)
}

/// Update a run to `failed` status with error details.
///
/// Sets `error_json`, clears `proposals_json` (set to NULL), and sets
/// `updated_at` to now.  Only fires when the run is in `running` status
/// — this prevents a completed or discarded run from being overwritten.
///
/// Returns the number of affected rows (1 on success).
///
/// # Errors
/// Returns [`LocalDbError`] on database failure, or
/// [`LocalDbError::ConstraintViolation`] when the run does not exist
/// or is not in `running` status (0 rows affected).
pub async fn set_run_failed(
    pool: &SqlitePool,
    run_id: &str,
    error_json: &str,
) -> Result<u64, LocalDbError> {
    let updated_at = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query!(
        "UPDATE compute_sessions \
         SET status = 'failed', error_json = ?, proposals_json = NULL, updated_at = ? \
         WHERE run_id = ? AND status = 'running'",
        error_json,
        updated_at,
        run_id,
    )
    .execute(pool)
    .await?;
    let affected = result.rows_affected();
    if affected == 0 {
        return Err(LocalDbError::ConstraintViolation {
            table: "compute_sessions".to_string(),
            constraint: format!(
                "run {run_id} is not in 'running' status — cannot transition to 'failed'"
            ),
        });
    }
    Ok(affected)
}

/// Update a run to `applied` status within an existing transaction.
///
/// Sets `accepted_at` and `updated_at`.  Only fires when the run is in
/// `succeeded` status — this is the atomic guard that prevents TOCTOU
/// races: two concurrent accepts both pass a pre-check but only one
/// UPDATE actually matches.
///
/// Caller owns the transaction lifecycle (`begin` / `commit` / `rollback`).
///
/// Returns the number of affected rows (1 on success).
///
/// # Errors
/// Returns [`LocalDbError`] on database failure, or
/// [`LocalDbError::ConstraintViolation`] when the run does not exist
/// or is not in `succeeded` status (0 rows affected — already accepted,
/// discarded, or still running).
pub async fn set_run_applied_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    run_id: &str,
    accepted_at: &str,
) -> Result<u64, LocalDbError> {
    let updated_at = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query!(
        "UPDATE compute_sessions \
         SET status = 'applied', accepted_at = ?, updated_at = ? \
         WHERE run_id = ? AND status = 'succeeded'",
        accepted_at,
        updated_at,
        run_id,
    )
    .execute(&mut **tx)
    .await?;
    let affected = result.rows_affected();
    if affected == 0 {
        return Err(LocalDbError::ConstraintViolation {
            table: "compute_sessions".to_string(),
            constraint: format!("run {run_id} is not in 'succeeded' status — cannot accept"),
        });
    }
    Ok(affected)
}

/// Update a run to `discarded` status.
///
/// Sets `updated_at` to now.  Only fires when the run is in `succeeded`
/// status — matches the route spec §2.4 prescribed SQL.  This prevents
/// double-discard TOCTOU races.
///
/// Returns the number of affected rows (1 on success).
///
/// # Errors
/// Returns [`LocalDbError`] on database failure, or
/// [`LocalDbError::ConstraintViolation`] when the run does not exist
/// or is not in `succeeded` status (0 rows affected — already discarded,
/// accepted, or still running).
pub async fn set_run_discarded(pool: &SqlitePool, run_id: &str) -> Result<u64, LocalDbError> {
    let updated_at = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query!(
        "UPDATE compute_sessions \
         SET status = 'discarded', updated_at = ? \
         WHERE run_id = ? AND status = 'succeeded'",
        updated_at,
        run_id,
    )
    .execute(pool)
    .await?;
    let affected = result.rows_affected();
    if affected == 0 {
        return Err(LocalDbError::ConstraintViolation {
            table: "compute_sessions".to_string(),
            constraint: format!("run {run_id} is not in 'succeeded' status — cannot discard"),
        });
    }
    Ok(affected)
}

/// List compute runs with optional filters and key-based cursor pagination.
///
/// Returns `(items, next_cursor)` where `next_cursor` is `Some(last_run_id)`
/// when another page may exist (the caller should pass it back as `cursor`).
/// Rows are ordered by `run_id` ascending; the cursor is the last-seen
/// `run_id`.
///
/// Callers should pass `limit >= 1`.  When `limit == 0` the `limit+1`
/// trick still fetches one row to detect `has_more`, but `take(0)` yields
/// zero items and `next_cursor` will be `None` even though a row exists
/// — treat `limit = 0` as a caller bug.
///
/// # Filter: `creator_world_ids`
///
/// - `None` (or absent) — no world-id filter; all worlds included.
/// - `Some(vec![])` — an empty set: **no world can match**, so the
///   result is always empty.  This is explicit — the caller must pass
///   `None` to skip the filter.
/// - `Some(vec!["w1", "w2"])` — restricts results to those worlds.
///
/// # Panics
///
/// Panics if the database returns a direct-lane row with a NULL `run_id`,
/// `world_id`, or `module_id` column (violates the direct-lane invariant).
///
/// # Errors
/// Returns [`LocalDbError`] on database failure.
pub async fn list_runs(
    pool: &SqlitePool,
    filters: &RunListFilters,
    cursor: Option<&str>,
    limit: u32,
) -> Result<(Vec<ComputeRunRow>, Option<String>), LocalDbError> {
    // Build dynamic query with QueryBuilder because the WHERE clause shape
    // varies by filter presence.  Each push_bind parameterises the value so
    // the SQL is parameterised, not string-interpolated.
    // SAFETY: dynamic SQL — QueryBuilder constructs WHERE clauses from
    // parameterised filter values; every user-supplied value goes through
    // `push_bind` (parameterised); the SQL skeleton, ORDER BY, and LIMIT
    // clause are static.  No string interpolation of filter values.
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT run_id, world_id, module_id, module_version, status, \
         proposals_json, error_json, created_at, updated_at, accepted_at, \
         invocation_params_json \
         FROM compute_sessions WHERE run_id IS NOT NULL",
    );

    if let Some(ref cursor_val) = cursor {
        builder.push(" AND run_id > ");
        builder.push_bind(cursor_val);
    }

    if let Some(ref world_id) = filters.world_id {
        builder.push(" AND world_id = ");
        builder.push_bind(world_id);
    }

    if let Some(ref module_id) = filters.module_id {
        builder.push(" AND module_id = ");
        builder.push_bind(module_id);
    }

    if let Some(ref status) = filters.status {
        builder.push(" AND status = ");
        builder.push_bind(status);
    }

    if let Some(ref world_ids) = filters.creator_world_ids {
        if world_ids.is_empty() {
            // Empty set explicitly matches nothing (definitive no-op).
            // Without this the IN clause is skipped and every world matches,
            // which is surprising for an explicit `Some(vec![])`.
            builder.push(" AND 1 = 0");
        } else {
            builder.push(" AND world_id IN (");
            let mut separated = builder.separated(", ");
            for wid in world_ids {
                separated.push_bind(wid);
            }
            separated.push_unseparated(")");
        }
    }

    builder.push(" ORDER BY run_id LIMIT ");
    // Fetch limit + 1 to detect `has_more` without a second query.
    // When limit == 0 this fetches 1 row: items will be empty but the
    // row exists — callers should pass limit >= 1 (see doc above).
    builder.push_bind(i64::from(limit) + 1);

    let rows = builder
        .build_query_as::<RawRunRow>()
        .fetch_all(pool)
        .await?;

    let has_more = rows.len() > limit as usize;
    let items: Vec<ComputeRunRow> = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| ComputeRunRow {
            run_id: r.run_id.expect("run_id is not null for direct-lane rows"),
            world_id: r
                .world_id
                .expect("world_id is not null for direct-lane rows"),
            module_id: r
                .module_id
                .expect("module_id is not null for direct-lane rows"),
            module_version: r.module_version,
            status: r.status,
            proposals_json: r.proposals_json,
            error_json: r.error_json,
            created_at: r.created_at,
            updated_at: r.updated_at,
            accepted_at: r.accepted_at,
            invocation_params_json: r.invocation_params_json,
        })
        .collect();

    let next_cursor = if has_more {
        items.last().map(|r| r.run_id.clone())
    } else {
        None
    };

    Ok((items, next_cursor))
}

/// Raw row from `sqlx::query_as` for the dynamic list query.
#[derive(Debug, sqlx::FromRow)]
struct RawRunRow {
    run_id: Option<String>,
    world_id: Option<String>,
    module_id: Option<String>,
    module_version: Option<String>,
    status: String,
    proposals_json: Option<String>,
    error_json: Option<String>,
    created_at: String,
    updated_at: Option<String>,
    accepted_at: Option<String>,
    invocation_params_json: Option<String>,
}
