//! `core_changes` outbox reads.

use nexus_contracts::{
    core_changes_response::{
        CoreChangesResponseNextSequence, CoreChangesResponseSnapshotSequence, NexusCoreChangeRow,
        NexusCoreChangeRowSequence,
    },
    CoreChangesRequest, CoreChangesResponse,
};
use sqlx::SqlitePool;

use crate::error::{CoreError, CoreResult};

const DEFAULT_LIMIT: i64 = 64;
const MAX_LIMIT: i64 = 256;
const RETENTION_MIN_SEQUENCE_GAP: i64 = 4096;

/// One `core_changes` row as bound by `query_as`: `(sequence, world_id,
/// resource_kind, resource_id, resource_revision, change_kind, writer_id)`.
type ChangeRow = (i64, String, String, String, Option<String>, String, String);

pub async fn read_changes(
    pool: &SqlitePool,
    request: CoreChangesRequest,
) -> CoreResult<CoreChangesResponse> {
    let after = parse_decimal(request.after_sequence.as_str())?;
    let limit = i64::try_from(request.limit.get())
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT);

    let min_retained: Option<i64> = sqlx::query_scalar(
        "SELECT MIN(sequence) FROM (SELECT sequence FROM core_changes ORDER BY sequence DESC LIMIT ?)",
    )
    .bind(RETENTION_MIN_SEQUENCE_GAP)
    .fetch_optional(pool)
    .await
    .map_err(|e| db_err(&e))?;

    let resync_required = min_retained.is_some_and(|min_seq| after > 0 && after < min_seq - 1);

    let snapshot_sequence: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(sequence), 0) FROM core_changes")
            .fetch_one(pool)
            .await
            .map_err(|e| db_err(&e))?;

    let rows: Vec<ChangeRow> = sqlx::query_as(
        "SELECT sequence, world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id          FROM core_changes WHERE sequence > ? ORDER BY sequence ASC LIMIT ?",
    )
    .bind(after)
    .bind(limit + 1)
    .fetch_all(pool)
    .await
    .map_err(|e| db_err(&e))?;

    // `limit` is clamped to 1..=MAX_LIMIT above, so this conversion cannot fail.
    let take = usize::try_from(limit).unwrap_or_default();
    let has_extra = rows.len() > take;
    let page = rows.into_iter().take(take);
    let mut wire_rows = Vec::new();
    let mut next_sequence = after;
    for (
        sequence,
        world_id,
        resource_kind,
        resource_id,
        resource_revision,
        change_kind,
        writer_id,
    ) in page
    {
        next_sequence = sequence;
        wire_rows.push(NexusCoreChangeRow {
            sequence: decimal_string_seq(sequence)?,
            world_id,
            resource_kind,
            resource_id,
            resource_revision: resource_revision
                .map(|rev| rev.parse().map_err(|_| invalid_after_sequence()))
                .transpose()?,
            change_kind,
            writer_id,
        });
    }

    if has_extra {
        // next_sequence already points at the last row returned on this page.
    }

    Ok(CoreChangesResponse {
        rows: wire_rows,
        next_sequence: decimal_string(next_sequence)?,
        snapshot_sequence: snapshot_string(snapshot_sequence)?,
        resync_required,
    })
}

fn decimal_string_seq<T>(value: T) -> CoreResult<NexusCoreChangeRowSequence>
where
    T: std::fmt::Display,
{
    value
        .to_string()
        .parse()
        .map_err(|_| invalid_after_sequence())
}

fn decimal_string<T>(value: T) -> CoreResult<CoreChangesResponseNextSequence>
where
    T: std::fmt::Display,
{
    value
        .to_string()
        .parse()
        .map_err(|_| invalid_after_sequence())
}

fn snapshot_string<T>(value: T) -> CoreResult<CoreChangesResponseSnapshotSequence>
where
    T: std::fmt::Display,
{
    value
        .to_string()
        .parse()
        .map_err(|_| invalid_after_sequence())
}

fn parse_decimal(raw: &str) -> CoreResult<i64> {
    if raw.is_empty() || !raw.chars().all(|c| c.is_ascii_digit()) {
        return Err(CoreError::InvalidInput {
            field: "after_sequence".to_string(),
            reason: "must be a non-negative decimal string".to_string(),
        });
    }
    raw.parse().map_err(|_| invalid_after_sequence())
}

fn invalid_after_sequence() -> CoreError {
    CoreError::InvalidInput {
        field: "after_sequence".to_string(),
        reason: "must be a non-negative decimal string".to_string(),
    }
}

fn db_err(e: &sqlx::Error) -> CoreError {
    CoreError::Internal {
        category: format!("database_error: {e}"),
    }
}
