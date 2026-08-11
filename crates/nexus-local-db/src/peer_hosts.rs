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

/// Maximum `last_peer_id` length (chars) accepted by [`record_peer_manifest`].
///
/// A libp2p `PeerId` renders as a ~52-char base58 string; the cap is a
/// fail-closed guard against unbounded blob storage, mirroring
/// [`MAX_HOST_ID_CHARS`].
pub const MAX_PEER_ID_CHARS: usize = 512;

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
    /// The dialed peer's libp2p `PeerId` at the observation (V1.158 P2 /
    /// R-V1155P0-001) — diagnostics only (the spoof/collision signal when a
    /// dialed peer claims a different `host_id`); never used for
    /// authorization. `None` for rows recorded before the column existed or
    /// observations without a peer id.
    pub last_peer_id: Option<String>,
}

/// Upsert an observed peer host.
///
/// Idempotent — never errors on a duplicate `host_id`; a second observation
/// refreshes `manifest_json` + `last_seen` + `last_peer_id`. Single atomic
/// upsert (QC fix wave F-002): `manifest_json` is the only manifest source
/// of truth — no denormalized columns, no two-step writes.
///
/// `last_peer_id` (V1.158 P2 / R-V1155P0-001) is the dialed peer's libp2p
/// `PeerId` at the observation — nexus-local diagnostics only (the
/// spoof/collision signal), never an authorization input. `None` records a
/// row without a peer id (pre-column observations backfill NULL).
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
    last_peer_id: Option<&str>,
) -> Result<(), LocalDbError> {
    validate_record_input(host_id, manifest_json, now, last_peer_id)?;
    sqlx::query!(
        "INSERT INTO peer_hosts (host_id, manifest_json, last_seen, last_peer_id) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT(host_id) DO UPDATE SET \
           manifest_json = excluded.manifest_json, \
           last_seen = excluded.last_seen, \
           last_peer_id = excluded.last_peer_id",
        host_id,
        manifest_json,
        now,
        last_peer_id,
    )
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
    // `host_id as "host_id!"` — SQLite describes a TEXT PRIMARY KEY as
    // nullable (sqlx 0.8.6), the non-null coercion matches the `String` field.
    let rows = sqlx::query_as!(
        PeerHostRow,
        "SELECT host_id as \"host_id!\", manifest_json, last_seen, last_peer_id \
         FROM peer_hosts \
         ORDER BY last_seen DESC, host_id ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Fail-closed input gate for [`record_peer_manifest`]: `host_id` non-empty
/// and bounded, `manifest_json` parseable JSON within the size cap, `now` an
/// RFC 3339 UTC timestamp, `last_peer_id` bounded when present. Rejects
/// malformed/oversized input with [`LocalDbError::ValidationError`] before
/// anything reaches the database.
fn validate_record_input(
    host_id: &str,
    manifest_json: &str,
    now: &str,
    last_peer_id: Option<&str>,
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
    if let Some(peer_id) = last_peer_id {
        if peer_id.chars().count() > MAX_PEER_ID_CHARS {
            return Err(LocalDbError::ValidationError(format!(
                "peer last_peer_id exceeds {MAX_PEER_ID_CHARS} chars"
            )));
        }
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
    // libp2p-style base58 `PeerId`s (R-V1155P0-001 diagnostics fixtures).
    const PEER_ID_A: &str = "12D3KooWPeerIdA000000000000000000000000000000000000";
    const PEER_ID_B: &str = "12D3KooWPeerIdB000000000000000000000000000000000000";
    const MANIFEST_V1: &str =
        r#"{"host_id":"peer_a","host_name":"alpha","capabilities":[{"kind":"kb","version":1}]}"#;
    const MANIFEST_V2: &str = r#"{"host_id":"peer_a","host_name":"alpha-renamed","capabilities":[{"kind":"kb","version":2}]}"#;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn insert_then_list_round_trips() {
        let (pool, _dir) = fresh_pool().await;
        record_peer_manifest(&pool, "peer_a", MANIFEST_V1, T1, Some(PEER_ID_A))
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
        assert_eq!(
            row.last_peer_id.as_deref(),
            Some(PEER_ID_A),
            "peer id recorded alongside host_id (R-V1155P0-001)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn duplicate_host_id_upserts_one_row_with_fresh_last_seen() {
        let (pool, _dir) = fresh_pool().await;
        record_peer_manifest(&pool, "peer_a", MANIFEST_V1, T1, Some(PEER_ID_A))
            .await
            .unwrap();
        record_peer_manifest(&pool, "peer_a", MANIFEST_V2, T2, Some(PEER_ID_B))
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
        assert_eq!(
            rows[0].last_peer_id.as_deref(),
            Some(PEER_ID_B),
            "last_peer_id refreshed on upsert (most recent observation wins)"
        );
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
        record_peer_manifest(&pool, "peer_older", MANIFEST_V1, T1, None)
            .await
            .unwrap();
        record_peer_manifest(&pool, "peer_newer", MANIFEST_V2, T2, None)
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
        let err = record_peer_manifest(&pool, "peer_a", "not json {", T1, None)
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
            let err = record_peer_manifest(&pool, "peer_a", MANIFEST_V1, bad, None)
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
        let err = record_peer_manifest(&pool, "peer_a", &huge, T1, None)
            .await
            .unwrap_err();
        assert!(matches!(err, LocalDbError::ValidationError(_)));
        assert!(list_peer_manifests(&pool).await.unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn empty_or_oversized_host_id_is_rejected_and_not_stored() {
        let (pool, _dir) = fresh_pool().await;
        for bad in ["", &"h".repeat(MAX_HOST_ID_CHARS + 1)] {
            let err = record_peer_manifest(&pool, bad, MANIFEST_V1, T1, None)
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

    /// The single atomic upsert contract (QC fix wave F-002): a second
    /// observation is ONE statement — `manifest_json` + `last_seen` refresh
    /// together; there is no denormalized column that could drift. Pinned
    /// here by asserting the row content after a conflict, and structurally
    /// by the absence of any second write path (the module no longer
    /// exports `set_peer_capabilities`).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn upsert_refreshes_manifest_and_last_seen_in_one_statement() {
        let (pool, _dir) = fresh_pool().await;
        record_peer_manifest(&pool, "peer_a", MANIFEST_V1, T1, Some(PEER_ID_A))
            .await
            .unwrap();
        record_peer_manifest(&pool, "peer_a", MANIFEST_V2, T2, Some(PEER_ID_B))
            .await
            .unwrap();

        let rows = list_peer_manifests(&pool).await.unwrap();
        assert_eq!(rows.len(), 1, "one row per host_id");
        assert_eq!(rows[0].manifest_json, MANIFEST_V2);
        assert_eq!(rows[0].last_seen, T2);
        assert_eq!(
            rows[0].last_peer_id.as_deref(),
            Some(PEER_ID_B),
            "last_peer_id refreshes in the same statement as manifest + last_seen"
        );
    }

    /// R-V1155P0-001: the spoof/collision signal — a second observation of
    /// the same `host_id` with a different `PeerId` refreshes
    /// `last_peer_id` (most recent observation wins); an observation
    /// without a peer id stores NULL (additive, pre-column rows backfill).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn peer_id_records_on_insert_and_refreshes_on_upsert() {
        let (pool, _dir) = fresh_pool().await;
        record_peer_manifest(&pool, "peer_a", MANIFEST_V1, T1, Some(PEER_ID_A))
            .await
            .unwrap();
        record_peer_manifest(&pool, "peer_a", MANIFEST_V2, T2, Some(PEER_ID_B))
            .await
            .unwrap();
        record_peer_manifest(&pool, "peer_b", MANIFEST_V1, T2, None)
            .await
            .unwrap();

        let rows = list_peer_manifests(&pool).await.unwrap();
        assert_eq!(rows.len(), 2, "one row per host_id");
        let by_id: std::collections::HashMap<&str, &PeerHostRow> =
            rows.iter().map(|r| (r.host_id.as_str(), r)).collect();
        assert_eq!(
            by_id["peer_a"].last_peer_id.as_deref(),
            Some(PEER_ID_B),
            "drifted peer id overwrites the prior observation (last wins)"
        );
        assert_eq!(
            by_id["peer_b"].last_peer_id, None,
            "observation without a peer id stores NULL"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn oversized_peer_id_is_rejected_and_not_stored() {
        let (pool, _dir) = fresh_pool().await;
        let huge = "p".repeat(MAX_PEER_ID_CHARS + 1);
        let err = record_peer_manifest(&pool, "peer_a", MANIFEST_V1, T1, Some(&huge))
            .await
            .unwrap_err();
        assert!(matches!(err, LocalDbError::ValidationError(_)));
        assert!(
            list_peer_manifests(&pool).await.unwrap().is_empty(),
            "fail-closed: oversized peer id must not be stored"
        );
    }
}
