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
//! Local-first nexus has no peers: the production daemon runs against a
//! single local host that owns the `nexus` extension namespace and declares
//! the `data-store` role (it is the SQLite-backed authority for
//! `KnowledgeEntry`, `Relation`, and `Finding` storage). Multi-host / peer
//! discovery is not implemented.
//!
//! # Roadmap trigger — `list_peer_host_capability_manifests` (spec §7.4 stub matrix)
//!
//! **Trigger:** when nexus ships multi-host collaboration (peer daemon
//! discovery + cross-host orchestration). Nexus is local-first today;
//! peer manifests have no backing table and no discovery protocol.
//!
//! **Upgrade path:** add a `peer_hosts` table (`host_id`, manifest JSON,
//! `last_seen`, capabilities); implement a peer-discovery protocol (mDNS
//! or spoke's host-hello handshake); wire `list_peer_host_capability_manifests`
//! to query the table. Until a trigger fires, this stub returns the
//! documented empty peer list.
//!
//! **Residual:** tracked as `R-V1143P0-STRETCH` (closed V1.146 P5 — deferred;
//! peer discovery is multi-host infra, not spoke fork-port scope).

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

    /// Stub — returns the documented empty peer list (spec §7.4).
    ///
    /// Local-first nexus has no peers; peer discovery is a roadmap item
    /// triggered when nexus supports multi-host collaboration. See the
    /// module-level docs.
    async fn list_peer_host_capability_manifests(
        &self,
    ) -> SpokeResult<Vec<HostCapabilityManifest>> {
        SpokeResult::Ok(Vec::new())
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
                "assembler".to_string()
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
        // Same builder as the Connect Host → the extensions.nexus N-C1 block
        // is present on the port surface too (product draft §4.3 item 9).
        let nexus_ext = manifest
            .extensions
            .get(&"nexus".parse().expect("locked key parses"))
            .expect("extensions.nexus block present");
        assert_eq!(
            nexus_ext
                .get("connect_host_slice")
                .and_then(serde_json::Value::as_str),
            Some("n-c1")
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
        assert!(peers.is_empty(), "local-first nexus has no peers");
    }
}
