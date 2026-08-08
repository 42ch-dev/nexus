//! Production `HostManifestPort` impl — see spec §7.4 production-vs-stub
//! matrix.
//!
//! The self-manifest declares the full capability set proven by the
//! adapter's port implementations (built by the single shared builder
//! [`crate::manifest::build_local_host_manifest`], DF-72 N-C0 lock):
//! - `spoke-baseline` — all six baseline ports (T1).
//! - `l2-computable` — production `ComputablePort` (T2, 10 tests incl.
//!   orchestrate round-trip).
//! - `l5-fork` — production `ForkTimelineQueryPort` (T3, 9 tests;
//!   reviewer adjudicated declaration justified).
//!
//! V1.148 P3 N-C0 honesty lock: `host_id` is the installation device-id UUID
//! (`~/.nexus42/device-id` via `nexus_home_layout::device_id`), replacing the
//! former static `"nexus-local"` constant — an installation-scoped stable id,
//! not a libp2p `PeerId`, not a world id. An injected [`NexusAdapter::with_host_id`]
//! id wins; otherwise the device id is resolved from the standard nexus home
//! on demand.
//!
//! V1.155 P0 N-C3 (multi-host production, DF-72): the last stub is gone.
//! [`HostManifestPort::list_peer_host_capability_manifests`] is a production
//! query over the `peer_hosts` table (`nexus-local-db`, same workspace DB the
//! adapter already runs against); peers are recorded ONLY from observed
//! Connect sessions — the outbound `connect()` return observation point
//! (iteration spec `fl-r-w3-n-c3-multi-host.md` §Design lock #1):
//! [`NexusAdapter::record_peer_manifest`] upserts the dialed peer's
//! `HostCapabilityManifest` (`host_id` PK, fail-closed validation at this
//! adapter boundary — `nexus-local-db` stays spoke-schema-free per spec §8
//! dep reversal). Inbound-only peers (a peer dials us) are NOT recorded this
//! iteration: the invoke boundary carries only `&PeerId`, and the
//! inbound-manifest API change is a spoke-connect change, out of nexus
//! scope. Honesty: the port never fabricates peers — only observed
//! manifests are ever stored.

use super::NexusAdapter;
use crate::{HostCapabilityManifest, HostManifestPort, SpokeRejectCode, SpokeResult};
use async_trait::async_trait;
use nexus_home_layout::device_id::get_or_create_device_id;

#[async_trait]
impl HostManifestPort for NexusAdapter<'_> {
    async fn get_host_capability_manifest(&self) -> SpokeResult<HostCapabilityManifest> {
        // Single builder SSOT (product draft §4.1): the Connect Host's
        // `local_manifest` (P3 T3) calls the same function with the same
        // device-id host_id — one capability list, one extensions.nexus block.
        let host_id = match &self.host_id {
            Some(id) => id.clone(),
            None => {
                return match resolve_device_id_from_standard_home() {
                    SpokeResult::Ok(id) => crate::manifest::build_local_host_manifest(&id),
                    SpokeResult::Reject(r) => SpokeResult::Reject(r),
                };
            }
        };
        crate::manifest::build_local_host_manifest(&host_id)
    }

    /// Production query (V1.155 P0 / N-C3 multi-host production) — the last
    /// adapter stub is gone (spec §7.3 stub matrix).
    ///
    /// Returns the persisted `HostCapabilityManifest`s of observed peer
    /// hosts (recorded at the outbound `connect()` return —
    /// [`NexusAdapter::record_peer_manifest`]), most recently seen first.
    /// Empty table → `Ok(vec![])` (the former stub contract is preserved).
    /// Honesty: never fabricates — only manifest-backed observations are
    /// ever stored; there is no injection path in this impl.
    ///
    /// # Errors
    /// `InternalError` when the store query fails or a stored row fails to
    /// re-parse as a `HostCapabilityManifest` (rows are validated before
    /// insert, so a parse failure is storage corruption — surfaced, never
    /// skipped, never replaced).
    async fn list_peer_host_capability_manifests(
        &self,
    ) -> SpokeResult<Vec<HostCapabilityManifest>> {
        let rows = match nexus_local_db::list_peer_manifests(&self.pool).await {
            Ok(rows) => rows,
            Err(e) => {
                return SpokeResult::Reject(reject_internal(format!(
                    "peer_hosts query failed: {e}"
                )));
            }
        };
        let mut manifests = Vec::with_capacity(rows.len());
        for row in rows {
            let manifest = match serde_json::from_str::<HostCapabilityManifest>(&row.manifest_json)
            {
                Ok(manifest) => manifest,
                Err(e) => {
                    return SpokeResult::Reject(reject_internal(format!(
                        "stored peer manifest for host_id {} is corrupt: {e}",
                        row.host_id
                    )));
                }
            };
            manifests.push(manifest);
        }
        SpokeResult::Ok(manifests)
    }
}

/// Resolve the installation device id from the standard nexus home
/// (`$HOME/.nexus42` — matches `apps/nexus42` `config::nexus_home()`), so an
/// adapter without an injected host id still advertises the honest identity.
fn resolve_device_id_from_standard_home() -> SpokeResult<String> {
    let Some(home) = dirs::home_dir() else {
        return SpokeResult::Reject(reject_internal(
            "cannot resolve home directory for device-id".into(),
        ));
    };
    // `get_or_create_device_id` takes the RAW home and joins `.nexus42`
    // itself (canonical `~/.nexus42/device-id`; device_id_path contract).
    match get_or_create_device_id(&home) {
        Ok(id) => SpokeResult::Ok(id),
        Err(e) => SpokeResult::Reject(reject_internal(format!("device-id unavailable: {e}"))),
    }
}

impl NexusAdapter<'_> {
    /// Record an observed peer host (V1.155 P0 / N-C3 multi-host production).
    ///
    /// The outbound `connect()` return observation point (iteration spec
    /// §Design lock #1): the dialed peer's `HostCapabilityManifest` from
    /// `PeerSession::remote_manifest()`. The connect host calls this at
    /// `SpokeConnectNode::connect()` return (`connect::record_dialed_peer`);
    /// inbound-only peers (peer dials us) are not recorded this iteration —
    /// the invoke boundary carries only `&PeerId`, and the inbound-manifest
    /// API change is a spoke-connect change, out of nexus scope.
    ///
    /// The adapter is the semantic boundary (spec §8 dep reversal —
    /// `nexus-local-db` stays spoke-schema-free): the manifest's `host_id`
    /// becomes the `peer_hosts` PK, the manifest is re-serialized for
    /// storage (never stored by reference), and the denormalized
    /// `capabilities` column is written through
    /// [`nexus_local_db::set_peer_capabilities`]. Fail-closed: a manifest
    /// with an empty or oversized `host_id` is rejected before insert;
    /// storage failures surface as rejects (never swallowed).
    ///
    /// # Errors
    /// `InvalidInput` when the manifest `host_id` is empty or exceeds
    /// [`nexus_local_db::MAX_HOST_ID_CHARS`]; `InternalError` on
    /// serialization or storage failure.
    pub async fn record_peer_manifest(&self, manifest: &HostCapabilityManifest) -> SpokeResult<()> {
        let host_id = manifest.host_id.as_str();
        if host_id.is_empty() {
            return SpokeResult::Reject(reject_invalid_input(
                "peer manifest host_id must not be empty".into(),
            ));
        }
        if host_id.chars().count() > nexus_local_db::MAX_HOST_ID_CHARS {
            return SpokeResult::Reject(reject_invalid_input(format!(
                "peer manifest host_id exceeds {} chars",
                nexus_local_db::MAX_HOST_ID_CHARS
            )));
        }
        let manifest_json = match serde_json::to_string(manifest) {
            Ok(json) => json,
            Err(e) => {
                return SpokeResult::Reject(reject_internal(format!(
                    "peer manifest serialization failed: {e}"
                )));
            }
        };
        let capabilities_json = match serde_json::to_string(&manifest.capabilities) {
            Ok(json) => json,
            Err(e) => {
                return SpokeResult::Reject(reject_internal(format!(
                    "peer capabilities serialization failed: {e}"
                )));
            }
        };
        let now = chrono::Utc::now().to_rfc3339();
        match nexus_local_db::record_peer_manifest(&self.pool, host_id, &manifest_json, &now).await
        {
            Ok(()) => {}
            Err(e) => {
                return SpokeResult::Reject(reject_internal(format!(
                    "peer manifest recording failed: {e}"
                )));
            }
        }
        match nexus_local_db::set_peer_capabilities(&self.pool, host_id, &capabilities_json).await {
            Ok(()) => {}
            Err(e) => {
                return SpokeResult::Reject(reject_internal(format!(
                    "peer capabilities recording failed: {e}"
                )));
            }
        }
        SpokeResult::Ok(())
    }
}

const fn reject_invalid_input(message: String) -> crate::SpokeReject {
    crate::SpokeReject {
        code: SpokeRejectCode::InvalidInput,
        message,
        details: None,
    }
}

const fn reject_internal(message: String) -> crate::SpokeReject {
    crate::SpokeReject {
        code: SpokeRejectCode::InternalError,
        message,
        details: None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HostManifestPort;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn self_manifest_reports_injected_host_id_and_n_c1_contract() {
        // HostManifestPort is storage-free; the pool is only needed to
        // satisfy the adapter struct shape. Use an in-memory pool with
        // migrations so the adapter construction path mirrors the other
        // ports. The host id is injected so the test stays hermetic — no
        // reads/writes against the real `~/.nexus42`.
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();

        let adapter = NexusAdapter::new(pool).with_host_id("test-host-uuid-0000");
        let manifest = match adapter.get_host_capability_manifest().await {
            SpokeResult::Ok(m) => m,
            SpokeResult::Reject(r) => panic!("self manifest is Ok: {r:?}"),
        };

        assert_eq!(manifest.host_id.as_str(), "test-host-uuid-0000");
        assert_eq!(
            manifest.roles,
            vec![
                "data-store".to_string(),
                "checker".to_string(),
                "assembler".to_string(),
                "computable-engine".to_string()
            ]
        );
        assert_eq!(
            manifest.capabilities,
            vec![
                "spoke-baseline".to_string(),
                "l2-computable".to_string(),
                "l5-fork".to_string()
            ]
        );
        assert_eq!(
            manifest
                .namespaces
                .iter()
                .map(|n| n.as_str().to_string())
                .collect::<Vec<_>>(),
            vec!["nexus".to_string()]
        );
        // Same builder as the Connect Host → the extensions.nexus N-C2
        // read-half block is present on the port surface too (product draft
        // §4.3 item 9).
        let nexus_ext = manifest
            .extensions
            .get(&"nexus".parse().expect("locked key parses"))
            .expect("extensions.nexus block present");
        assert_eq!(
            nexus_ext
                .get("connect_host_slice")
                .and_then(serde_json::Value::as_str),
            Some("n-c2")
        );
        let expected_ops_value = serde_json::json!(crate::manifest::LOCAL_SERVED_OPS);
        let expected_ops = expected_ops_value
            .as_array()
            .expect("locked op list serializes as an array");
        assert_eq!(
            nexus_ext
                .get("served_ops")
                .and_then(serde_json::Value::as_array),
            Some(expected_ops)
        );
        assert_eq!(
            nexus_ext
                .get("daemon_http_coexists")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert!(!serde_json::to_string(&manifest)
            .expect("serializes")
            .contains("reasoning-complete"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn peer_manifests_is_empty_for_local_first_nexus() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();

        let adapter = NexusAdapter::new(pool);
        let peers = match adapter.list_peer_host_capability_manifests().await {
            SpokeResult::Ok(p) => p,
            SpokeResult::Reject(r) => panic!("peer list is Ok: {r:?}"),
        };
        assert!(peers.is_empty(), "empty store has no peers");
    }

    /// The N-C3 recording contract (V1.155 P0): record a peer manifest at
    /// the outbound observation point → the port returns exactly it.
    /// Also pins the production path: the manifest is re-serialized through
    /// the typed wire (a recorded `HostCapabilityManifest` round-trips field
    /// for field, including the denormalized capabilities write).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn recorded_peer_manifest_round_trips_through_port() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();

        let adapter = NexusAdapter::new(pool);
        let manifest = match crate::manifest::build_local_host_manifest("peer-host-uuid-0001") {
            SpokeResult::Ok(m) => m,
            SpokeResult::Reject(r) => panic!("manifest build is Ok: {r:?}"),
        };
        match adapter.record_peer_manifest(&manifest).await {
            SpokeResult::Ok(()) => {}
            SpokeResult::Reject(r) => panic!("recording is Ok: {r:?}"),
        }

        let peers = match adapter.list_peer_host_capability_manifests().await {
            SpokeResult::Ok(p) => p,
            SpokeResult::Reject(r) => panic!("peer list is Ok: {r:?}"),
        };
        assert_eq!(peers.len(), 1, "recorded peer is listed");
        assert_eq!(
            peers[0].host_id, manifest.host_id,
            "host_id round-trips as the peer_hosts PK"
        );
        assert_eq!(
            peers[0].capabilities, manifest.capabilities,
            "capabilities round-trip through the typed wire"
        );
        assert_eq!(peers[0].roles, manifest.roles);
        assert_eq!(peers[0].namespaces, manifest.namespaces);
        assert_eq!(peers[0].schema_version, manifest.schema_version);
    }

    /// Duplicate host_id → one row: a second observation of the same peer
    /// upserts (fresh manifest, fresh capabilities) instead of duplicating.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn duplicate_host_id_recording_upserts_single_peer_row() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();

        let adapter = NexusAdapter::new(pool);
        let first = match crate::manifest::build_local_host_manifest("peer-host-uuid-0001") {
            SpokeResult::Ok(m) => m,
            SpokeResult::Reject(r) => panic!("manifest build is Ok: {r:?}"),
        };
        let mut second = first.clone();
        second.capabilities.push("extra-capability".to_string());
        match adapter.record_peer_manifest(&first).await {
            SpokeResult::Ok(()) => {}
            SpokeResult::Reject(r) => panic!("first record is Ok: {r:?}"),
        }
        match adapter.record_peer_manifest(&second).await {
            SpokeResult::Ok(()) => {}
            SpokeResult::Reject(r) => panic!("second record is Ok: {r:?}"),
        }

        let peers = match adapter.list_peer_host_capability_manifests().await {
            SpokeResult::Ok(p) => p,
            SpokeResult::Reject(r) => panic!("peer list is Ok: {r:?}"),
        };
        assert_eq!(peers.len(), 1, "upsert keeps exactly one row per host_id");
        assert_eq!(
            peers[0].capabilities, second.capabilities,
            "second observation refreshes the manifest + capabilities"
        );
    }

    /// Fail-closed: a peer manifest whose `host_id` exceeds the storage cap
    /// is rejected before insert (never stored, never listed).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn oversized_peer_host_id_is_rejected_before_insert() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();

        let adapter = NexusAdapter::new(pool);
        // `HostCapabilityManifestHostId` only enforces minLength 1 at parse —
        // an oversized id is constructible, so the adapter's own gate is the
        // fail-closed boundary (mirrors the storage cap).
        let huge = "h".repeat(nexus_local_db::MAX_HOST_ID_CHARS + 1);
        let manifest = match crate::manifest::build_local_host_manifest(&huge) {
            SpokeResult::Ok(m) => m,
            SpokeResult::Reject(r) => panic!("manifest build is Ok: {r:?}"),
        };
        match adapter.record_peer_manifest(&manifest).await {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InvalidInput);
            }
            SpokeResult::Ok(()) => panic!("oversized host_id must be rejected"),
        }
        let peers = match adapter.list_peer_host_capability_manifests().await {
            SpokeResult::Ok(p) => p,
            SpokeResult::Reject(r) => panic!("peer list is Ok: {r:?}"),
        };
        assert!(
            peers.is_empty(),
            "fail-closed: rejected manifest is never stored"
        );
    }
}
