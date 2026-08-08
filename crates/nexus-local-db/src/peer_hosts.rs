//! Peer-host bookkeeping storage (V1.155 P0 / N-C3 multi-host production) —
//! the `peer_hosts` table.
//!
//! Pure storage (spec §8 dep-graph reversal: `nexus-local-db` has no spoke
//! dependency): `manifest_json` is carried as an opaque JSON string and
//! validated as JSON before insert (fail-closed — malformed input is
//! rejected, never stored). Semantic validation of the manifest as a spoke
//! `HostCapabilityManifest` happens at the `nexus-spoke-adapter` boundary
//! before this primitive is called.
//!
//! Recording contract (iteration spec §Design): a row exists only for
//! manifest-backed observations — the outbound `connect()` path. Upserts are
//! idempotent and never error on duplicate `host_id`.

use crate::LocalDbError;
use sqlx::SqlitePool;

/// Maximum serialized manifest size accepted by [`record_peer_manifest`].
///
/// Fail-closed guard against unbounded blob storage: a spoke
/// `HostCapabilityManifest` is a small document — capability kind/version
/// entries — so 64 KiB is far beyond any realistic payload.
pub const MAX_MANIFEST_JSON_BYTES: usize = 64 * 1024;

/// Maximum `host_id` length (chars) accepted by [`record_peer_manifest`].
pub const MAX_HOST_ID_CHARS: usize = 512;

/// Row type matching the `peer_hosts` DDL (`20260808120000_peer_hosts.sql`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PeerHostRow {
    /// Peer manifest `host_id` (the peer's device id) — PK, stable per host
    /// installation (iteration spec §Design lock #3).
    pub host_id: String,
    /// Serialized spoke `HostCapabilityManifest` observed at the outbound
    /// `connect()` return (opaque to the storage layer).
    pub manifest_json: String,
    /// RFC 3339 UTC timestamp of the observation.
    pub last_seen: String,
    /// Denormalized JSON array of the peer's capabilities (DEFAULT '[]';
    /// populated by the adapter layer).
    pub capabilities: String,
}

/// Upsert an observed peer host.
///
/// Idempotent — never errors on a duplicate `host_id`; a second observation
/// refreshes `manifest_json` + `last_seen` and preserves the previously
/// stored `capabilities` (a fresh row starts with the `'[]'` DEFAULT).
///
/// Fail-closed: `host_id` must be non-empty (≤ [`MAX_HOST_ID_CHARS`]),
/// `manifest_json` must parse as JSON (≤ [`MAX_MANIFEST_JSON_BYTES`]), and
/// `now` must be an RFC 3339 UTC timestamp. Malformed input is rejected with
/// [`LocalDbError::ValidationError`] and nothing is stored.
///
/// # Errors
///
/// Returns [`LocalDbError::ValidationError`] for malformed/oversized input
/// and [`LocalDbError::Sqlx`] on database failure.
pub async fn record_peer_manifest(
    pool: &SqlitePool,
    host_id: &str,
    manifest_json: &str,
    now: &str,
) -> Result<(), LocalDbError> {
    validate_record_input(host_id, manifest_json, now)?;
    // SAFETY: static INSERT ... ON CONFLICT upsert with bind params only (no
    // user-controlled SQL fragments). `capabilities` is left to its DEFAULT
    // '[]' on insert and preserved on conflict (denormalized field populated
    // by the adapter layer).
    sqlx::query(
        "INSERT INTO peer_hosts (host_id, manifest_json, last_seen) \
         VALUES (?, ?, ?) \
         ON CONFLICT(host_id) DO UPDATE SET \
           manifest_json = excluded.manifest_json, \
           last_seen = excluded.last_seen",
    )
    .bind(host_id)
    .bind(manifest_json)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// List recorded peer hosts, most recently seen first.
///
/// Ordering: `last_seen` DESC, `host_id` ASC tiebreak. Empty table →
/// `Ok(vec![])` (stub contract preserved for the port impl).
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn list_peer_manifests(pool: &SqlitePool) -> Result<Vec<PeerHostRow>, LocalDbError> {
    // SAFETY: static SELECT against peer_hosts with a fixed ORDER BY; no
    // user-controlled SQL fragments.
    let rows = sqlx::query_as::<_, PeerHostRow>(
        "SELECT host_id, manifest_json, last_seen, capabilities \
         FROM peer_hosts \
         ORDER BY last_seen DESC, host_id ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Fail-closed input gate for [`record_peer_manifest`]: `host_id` non-empty
/// and bounded, `manifest_json` parseable JSON within the size cap, `now` an
/// RFC 3339 UTC timestamp. Rejects malformed/oversized input with
/// [`LocalDbError::ValidationError`] before anything reaches the database.
fn validate_record_input(
    host_id: &str,
    manifest_json: &str,
    now: &str,
) -> Result<(), LocalDbError> {
    if host_id.is_empty() {
        return Err(LocalDbError::ValidationError(
            "peer host_id must not be empty".to_string(),
        ));
    }
    if host_id.chars().count() > MAX_HOST_ID_CHARS {
        return Err(LocalDbError::ValidationError(format!(
            "peer host_id exceeds {MAX_HOST_ID_CHARS} chars"
        )));
    }
    if manifest_json.len() > MAX_MANIFEST_JSON_BYTES {
        return Err(LocalDbError::ValidationError(format!(
            "peer manifest_json exceeds {MAX_MANIFEST_JSON_BYTES} bytes"
        )));
    }
    if serde_json::from_str::<serde_json::Value>(manifest_json).is_err() {
        return Err(LocalDbError::ValidationError(
            "peer manifest_json is not valid JSON".to_string(),
        ));
    }
    match chrono::DateTime::parse_from_rfc3339(now) {
        Ok(dt) if dt.offset().local_minus_utc() == 0 => Ok(()),
        _ => Err(LocalDbError::ValidationError(
            "peer last_seen must be an RFC 3339 UTC timestamp".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{open_pool, run_migrations};

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    const T1: &str = "2026-08-08T10:00:00+00:00";
    const T2: &str = "2026-08-08T11:30:00+00:00";
    const MANIFEST_V1: &str =
        r#"{"host_id":"peer_a","host_name":"alpha","capabilities":[{"kind":"kb","version":1}]}"#;
    const MANIFEST_V2: &str = r#"{"host_id":"peer_a","host_name":"alpha-renamed","capabilities":[{"kind":"kb","version":2}]}"#;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn insert_then_list_round_trips() {
        let (pool, _dir) = fresh_pool().await;
        record_peer_manifest(&pool, "peer_a", MANIFEST_V1, T1)
            .await
            .unwrap();

        let rows = list_peer_manifests(&pool).await.unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.host_id, "peer_a");
        assert_eq!(
            row.manifest_json, MANIFEST_V1,
            "manifest_json stored verbatim"
        );
        assert_eq!(row.last_seen, T1);
        assert_eq!(row.capabilities, "[]", "capabilities defaults to '[]'");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn duplicate_host_id_upserts_one_row_with_fresh_last_seen() {
        let (pool, _dir) = fresh_pool().await;
        record_peer_manifest(&pool, "peer_a", MANIFEST_V1, T1)
            .await
            .unwrap();
        record_peer_manifest(&pool, "peer_a", MANIFEST_V2, T2)
            .await
            .unwrap();

        let rows = list_peer_manifests(&pool).await.unwrap();
        assert_eq!(
            rows.len(),
            1,
            "upsert must keep exactly one row per host_id"
        );
        assert_eq!(rows[0].host_id, "peer_a");
        assert_eq!(
            rows[0].manifest_json, MANIFEST_V2,
            "manifest refreshed on upsert"
        );
        assert_eq!(rows[0].last_seen, T2, "last_seen refreshed on upsert");
        assert_eq!(rows[0].capabilities, "[]");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn empty_table_returns_empty_list() {
        let (pool, _dir) = fresh_pool().await;
        let rows = list_peer_manifests(&pool).await.unwrap();
        assert!(rows.is_empty(), "empty table must list no peers");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn multiple_peers_list_most_recent_first() {
        let (pool, _dir) = fresh_pool().await;
        record_peer_manifest(&pool, "peer_older", MANIFEST_V1, T1)
            .await
            .unwrap();
        record_peer_manifest(&pool, "peer_newer", MANIFEST_V2, T2)
            .await
            .unwrap();

        let rows = list_peer_manifests(&pool).await.unwrap();
        let ids: Vec<&str> = rows.iter().map(|r| r.host_id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["peer_newer", "peer_older"],
            "ordered by last_seen DESC"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn malformed_manifest_json_is_rejected_and_not_stored() {
        let (pool, _dir) = fresh_pool().await;
        let err = record_peer_manifest(&pool, "peer_a", "not json {", T1)
            .await
            .unwrap_err();
        assert!(matches!(err, LocalDbError::ValidationError(_)));
        assert!(
            list_peer_manifests(&pool).await.unwrap().is_empty(),
            "fail-closed: rejected input must not be stored"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn malformed_or_non_utc_last_seen_is_rejected_and_not_stored() {
        let (pool, _dir) = fresh_pool().await;
        for bad in ["2026-08-08T10:00:00+05:00", "not-a-timestamp"] {
            let err = record_peer_manifest(&pool, "peer_a", MANIFEST_V1, bad)
                .await
                .unwrap_err();
            assert!(matches!(err, LocalDbError::ValidationError(_)), "bad={bad}");
        }
        assert!(list_peer_manifests(&pool).await.unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn oversized_manifest_is_rejected_and_not_stored() {
        let (pool, _dir) = fresh_pool().await;
        let padding = "x".repeat(MAX_MANIFEST_JSON_BYTES + 1);
        let huge = format!("{{\"padding\":\"{padding}\"}}");
        let err = record_peer_manifest(&pool, "peer_a", &huge, T1)
            .await
            .unwrap_err();
        assert!(matches!(err, LocalDbError::ValidationError(_)));
        assert!(list_peer_manifests(&pool).await.unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn empty_or_oversized_host_id_is_rejected_and_not_stored() {
        let (pool, _dir) = fresh_pool().await;
        for bad in ["", &"h".repeat(MAX_HOST_ID_CHARS + 1)] {
            let err = record_peer_manifest(&pool, bad, MANIFEST_V1, T1)
                .await
                .unwrap_err();
            assert!(
                matches!(err, LocalDbError::ValidationError(_)),
                "bad host_id len={}",
                bad.chars().count()
            );
        }
        assert!(list_peer_manifests(&pool).await.unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn upsert_preserves_existing_capabilities() {
        let (pool, _dir) = fresh_pool().await;
        record_peer_manifest(&pool, "peer_a", MANIFEST_V1, T1)
            .await
            .unwrap();
        // SAFETY: test-only direct UPDATE — simulates a capabilities value
        // populated by the adapter layer; verifies the upsert preserves it.
        sqlx::query("UPDATE peer_hosts SET capabilities = ? WHERE host_id = ?")
            .bind("[{\"kind\":\"kb\",\"version\":1}]")
            .bind("peer_a")
            .execute(&pool)
            .await
            .unwrap();

        record_peer_manifest(&pool, "peer_a", MANIFEST_V2, T2)
            .await
            .unwrap();

        let rows = list_peer_manifests(&pool).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].capabilities, "[{\"kind\":\"kb\",\"version\":1}]");
    }
}
