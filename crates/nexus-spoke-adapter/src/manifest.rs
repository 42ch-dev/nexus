//! The HostCapabilityManifest **single builder SSOT** (DF-72 N-C0, product
//! draft `fl-r-connect-host-foundation.md` §4.1).
//!
//! `HostManifestPort::get_host_capability_manifest` and the Connect Host's
//! `ConnectConfig.local_manifest` (P3 T3) MUST both be built by
//! [`build_local_host_manifest`] — there is exactly one capability list and
//! one `extensions.nexus` block in the product. The N-C0 field lock:
//!
//! | Field | N-C0 value |
//! |-------|------------|
//! | `schema_version` | `1` (`NonZeroU64::MIN`) |
//! | `host_id` | caller-supplied installation device-id UUID |
//! | `roles` | `["data-store"]` |
//! | `capabilities` | `["spoke-baseline", "l2-computable", "l5-fork"]` |
//! | `namespaces` | `["nexus"]` |
//! | `authority` | `None` |
//! | `extensions.nexus` | `{ "connect_host_slice": "n-c0", "daemon_http_coexists": true }` |
//!
//! Honesty rules: `l5-fork` is included because `ForkTimelineQueryPort` is
//! production (V1.146); `"reasoning-complete"` MUST NOT appear anywhere
//! (reserved for N-C2 when `check`/`assemble` run over Connect).

use crate::{HostCapabilityManifest, SpokeReject, SpokeRejectCode, SpokeResult};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::num::NonZeroU64;

/// Wire schema version of this manifest (`NonZeroU64`, matches every spoke
/// 0.8.2 fixture and the previous static manifest).
pub const MANIFEST_SCHEMA_VERSION: NonZeroU64 = NonZeroU64::MIN;

/// Roles declared by the local host — `data-store` only until Connect op
/// dispatch exists for `checker` / `assembler` / `computable-engine`.
pub const LOCAL_ROLES: [&str; 1] = ["data-store"];

/// Capabilities advertised by the local host — each maps to a production
/// adapter port (see the honesty test below for the compile-time proof).
pub const LOCAL_CAPABILITIES: [&str; 3] = ["spoke-baseline", "l2-computable", "l5-fork"];

/// Namespaces owned by the local host.
pub const LOCAL_NAMESPACES: [&str; 1] = ["nexus"];

/// Build the N-C0 `HostCapabilityManifest` from the given `host_id`.
///
/// The `host_id` is the installation-scoped device-id UUID (`~/.nexus42/device-id`
/// via `nexus_home_layout::device_id::get_or_create_device_id`) — NOT the
/// libp2p `PeerId`, NOT a world id.
///
/// # Errors
/// Returns `InvalidInput` when `host_id` is empty (the spoke schema requires
/// `minLength: 1`); the namespace/extensions keys are locked constants and
/// cannot fail.
///
/// # Panics
/// Never in practice: the locked namespace (`"nexus"`) and extension key are
/// statically valid per the spoke schema regex; the defensive `expect` calls
/// only exist because the generated newtypes parse at runtime.
#[must_use]
pub fn build_local_host_manifest(host_id: &str) -> SpokeResult<HostCapabilityManifest> {
    let host_id = match host_id.parse() {
        Ok(id) => id,
        Err(e) => return SpokeResult::Reject(reject_invalid_host_id(e)),
    };
    let namespaces = LOCAL_NAMESPACES
        .iter()
        .map(|ns| ns.parse().expect("locked namespace is schema-valid"))
        .collect::<Vec<_>>();
    let mut extensions: HashMap<_, Map<String, Value>> = HashMap::new();
    extensions.insert(
        "nexus"
            .parse()
            .expect("locked extension key is schema-valid"),
        json!({
            "connect_host_slice": "n-c0",
            "daemon_http_coexists": true,
        })
        .as_object()
        .expect("locked extension block is an object")
        .clone(),
    );
    SpokeResult::Ok(HostCapabilityManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        host_id,
        roles: LOCAL_ROLES.iter().map(ToString::to_string).collect(),
        capabilities: LOCAL_CAPABILITIES.iter().map(ToString::to_string).collect(),
        namespaces,
        authority: None,
        extensions,
    })
}

fn reject_invalid_host_id(e: impl std::fmt::Display) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::InvalidInput,
        message: format!("invalid host_id for HostCapabilityManifest: {e}"),
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NexusAdapter;

    /// Compile-time production-port proof for one advertised capability.
    ///
    /// The trait upcasts only compile while the corresponding `impl
    /// <PortFamily> for NexusAdapter` exists (spec §7.4 production matrix);
    /// a capability whose port regressed to a stub stops compiling here.
    fn assert_capability_maps_to_production_port(adapter: &NexusAdapter<'_>, capability: &str) {
        match capability {
            // spoke-baseline — the six baseline ports (T1/T2-era) are production.
            "spoke-baseline" => {
                let _: &dyn crate::BaselinePorts = adapter;
                let _: &dyn crate::KnowledgeEntryPort = adapter;
                let _: &dyn crate::RelationPort = adapter;
                let _: &dyn crate::FindingPort = adapter;
                let _: &dyn crate::ScopeQueryPort = adapter;
                let _: &dyn crate::RuleQueryPort = adapter;
            }
            // l2-computable — production ComputablePort (V1.146).
            "l2-computable" => {
                let _: &dyn crate::ComputablePorts = adapter;
            }
            // l5-fork — production ForkTimelineQueryPort (V1.146).
            "l5-fork" => {
                let _: &dyn crate::ForkPorts = adapter;
                let _: &dyn crate::ForkTimelineQueryPort = adapter;
            }
            other => panic!("advertised capability {other:?} is not backed by a production port"),
        }
    }

    /// AC-I4.3 / product draft §4.3 — the manifest honesty test (machine-checkable).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn n_c0_manifest_is_honest() {
        // The builder is host_id-injectable so the test is hermetic (no
        // writes to the real `~/.nexus42`).
        let manifest = match build_local_host_manifest("test-device-uuid-0000") {
            SpokeResult::Ok(m) => m,
            SpokeResult::Reject(r) => panic!("builder rejected: {r:?}"),
        };

        // 1. Every capability is in the locked allowlist AND maps to a
        //    production adapter port (compile-time proof, §4.3 items 2).
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();
        let adapter = NexusAdapter::new(pool);
        for capability in &manifest.capabilities {
            assert_capability_maps_to_production_port(&adapter, capability);
        }
        assert_eq!(
            manifest.capabilities,
            LOCAL_CAPABILITIES.map(ToString::to_string).to_vec(),
            "capabilities must be exactly the locked N-C0 list"
        );

        // 3. roles == ["data-store"] only.
        assert_eq!(manifest.roles, vec!["data-store".to_string()]);

        // 4. namespaces == ["nexus"].
        assert_eq!(manifest.namespaces.len(), 1);
        assert_eq!(manifest.namespaces[0].as_str(), "nexus");

        // 5. authority is None (absent).
        assert!(
            manifest.authority.is_none(),
            "N-C0 must not declare authority"
        );

        // 6. No "reasoning-complete" anywhere in the manifest JSON.
        let manifest_json = serde_json::to_value(&manifest).expect("manifest serializes");
        assert!(
            !manifest_json.to_string().contains("reasoning-complete"),
            "N-C0 MUST NOT advertise reasoning-complete (reserved for N-C2)"
        );

        // 7. Round-trips through the spoke generated serde type without
        //    unknown fields (`deny_unknown_fields` on the generated struct;
        //    the generated type has no PartialEq, so equality is JSON-level).
        let round_tripped: HostCapabilityManifest =
            serde_json::from_value(manifest_json.clone()).expect("round-trip deserializes");
        assert_eq!(
            serde_json::to_value(round_tripped).expect("round-trip serializes"),
            manifest_json,
            "round-trip must be lossless"
        );

        // 8. schema_version == 1.
        assert_eq!(manifest.schema_version.get(), 1);

        // 9. extensions["nexus"] carries the N-C0 marker block.
        let nexus_key = "nexus".parse().expect("locked extension key parses");
        let nexus_ext = manifest
            .extensions
            .get(&nexus_key)
            .expect("extensions.nexus block present");
        assert_eq!(
            nexus_ext.get("connect_host_slice").and_then(Value::as_str),
            Some("n-c0")
        );
        assert_eq!(
            nexus_ext
                .get("daemon_http_coexists")
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn builder_rejects_empty_host_id() {
        let result = build_local_host_manifest("");
        assert!(
            matches!(result, SpokeResult::Reject(r) if r.code == SpokeRejectCode::InvalidInput),
            "empty host_id must be rejected (schema minLength 1)"
        );
    }
}
