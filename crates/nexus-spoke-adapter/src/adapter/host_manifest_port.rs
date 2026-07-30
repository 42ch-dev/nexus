//! Static-stub `HostManifestPort` impl — see spec §7.4 production-vs-stub
//! matrix.
//!
//! Local-first nexus has no peers: the production daemon runs against a
//! single `nexus-local` host that owns the `nexus` extension namespace and
//! declares the `data-store` role (it is the SQLite-backed authority for
//! `KnowledgeEntry`, `Relation`, and `Finding` storage). Multi-host /
//! peer discovery is not implemented.
//!
//! # Roadmap trigger (peers)
//!
//! Spec §7.4 stub matrix — `list_peer_host_capability_manifests` returns
//! `Ok(Vec::new())` until peer discovery lands (roadmap item triggered
//! when nexus supports multi-host collaboration). The static self
//! manifest IS authoritative — the daemon is the data-store for its
//! local storage.

use super::NexusBaselineAdapter;
use crate::{HostCapabilityManifest, HostManifestPort, SpokeResult};
use serde_json::json;
/// The local-first host id (spec §7.4 — `nexus-local` is the documented
/// default host identity for the production adapter).
const HOST_ID: &str = "nexus-local";

impl HostManifestPort for NexusBaselineAdapter<'_> {
    fn get_host_capability_manifest(&self) -> SpokeResult<HostCapabilityManifest> {
        // Mirror V1.141 mock's `make_manifest` pattern: construct the
        // canonical shape via `serde_json::from_value`, which exercises the
        // same typify-generated newtype validation a real wire manifest
        // would impose (host_id min-length, namespace regex, etc.).
        let manifest = serde_json::from_value(json!({
            "schema_version": 1,
            "host_id": HOST_ID,
            "roles": ["data-store"],
            "capabilities": ["spoke-baseline"],
            "namespaces": ["nexus"],
            "extensions": {}
        }))
        .expect("static nexus-local manifest is schema-valid");
        SpokeResult::Ok(manifest)
    }

    /// Stub — returns the documented empty peer list (spec §7.4).
    ///
    /// Local-first nexus has no peers; peer discovery is a roadmap item
    /// triggered when nexus supports multi-host collaboration. See the
    /// module-level docs.
    fn list_peer_host_capability_manifests(&self) -> SpokeResult<Vec<HostCapabilityManifest>> {
        SpokeResult::Ok(Vec::new())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HostManifestPort;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn self_manifest_reports_nexus_local_data_store() {
        // HostManifestPort is storage-free; the pool is only needed to
        // satisfy the adapter struct shape. Use an in-memory pool with
        // migrations so the adapter construction path mirrors the other
        // ports.
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();

        let adapter = NexusBaselineAdapter::new(pool);
        let manifest = match adapter.get_host_capability_manifest() {
            SpokeResult::Ok(m) => m,
            SpokeResult::Reject(r) => panic!("self manifest is Ok: {r:?}"),
        };

        assert_eq!(manifest.host_id.as_str(), HOST_ID);
        assert_eq!(manifest.roles, vec!["data-store".to_string()]);
        assert_eq!(manifest.capabilities, vec!["spoke-baseline".to_string()]);
        assert_eq!(
            manifest
                .namespaces
                .iter()
                .map(|n| n.as_str().to_string())
                .collect::<Vec<_>>(),
            vec!["nexus".to_string()]
        );
        assert!(manifest.extensions.is_empty(), "no product extensions");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn peer_manifests_is_empty_for_local_first_nexus() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();

        let adapter = NexusBaselineAdapter::new(pool);
        let peers = match adapter.list_peer_host_capability_manifests() {
            SpokeResult::Ok(p) => p,
            SpokeResult::Reject(r) => panic!("peer list is Ok: {r:?}"),
        };
        assert!(peers.is_empty(), "local-first nexus has no peers");
    }
}
