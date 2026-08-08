//! The HostCapabilityManifest **single builder SSOT** (DF-72 N-C0, product
//! draft `fl-r-connect-host-foundation.md` §4.1).
//!
//! `HostManifestPort::get_host_capability_manifest` and the Connect Host's
//! `ConnectConfig.local_manifest` (P3 T3) MUST both be built by
//! [`build_local_host_manifest`] — there is exactly one capability list and
//! The N-C0 field lock:
//!
//! | Field | N-C0 value | N-C1 (V1.153) | N-C2 read half (V1.154 P1) | N-C2 E2 (V1.154 P2) |
//! |-------|------------|---------------|----------------------------|----------------------|
//! | `schema_version` | `1` (`NonZeroU64::MIN`) | unchanged | unchanged | unchanged |
//! | `host_id` | caller-supplied installation device-id UUID | unchanged | unchanged | unchanged |
//! | `roles` | `["data-store"]` | unchanged | `["data-store", "checker", "assembler"]` | + `"computable-engine"` |
//! | `capabilities` | `["spoke-baseline", "l2-computable", "l5-fork"]` | unchanged | unchanged | unchanged |
//! | `namespaces` | `["nexus"]` | unchanged | unchanged | unchanged |
//! | `authority` | `None` | unchanged | unchanged | unchanged |
//! | `extensions.nexus` | `{ "connect_host_slice": "n-c0", "daemon_http_coexists": true }` | `{ "connect_host_slice": "n-c1", "served_ops": ["upsert", "promote", "relate"], "daemon_http_coexists": true }` | `connect_host_slice` → `"n-c2"`; `served_ops` → `["upsert", "promote", "relate", "check", "assemble"]` | `served_ops` → `+ "compute"` |
//!
//! Honesty rules: `l5-fork` is included because `ForkTimelineQueryPort` is
//! production (V1.146); `"reasoning-complete"` MUST NOT appear anywhere
//! (product lock — the semantic reasoning-complete milestone is expressed
//! by the `computable-engine` role + `l2-computable` capability, not a
//! literal string; P2 serves compute).
//! N-C1 → N-C2 E2: `extensions.nexus.served_ops` advertises **exactly** the
//! ops the Connect invoke dispatcher serves ([`LOCAL_SERVED_OPS`]) — the
//! connect-host dispatch gate owns the same set
//! (`apps/nexus42` `commands::connect::invoke::SERVED_OPS`), and the honesty
//! tests machine-check both directions (advertised ⇔ served) so the two
//! cannot drift unnoticed. Roles: `checker` / `assembler` /
//! `computable-engine` are the open-string roles (host-capability-manifest
//! schema: open string vocabulary) backing the served `check` / `assemble`
//! / `compute` ops.

use crate::{HostCapabilityManifest, SpokeReject, SpokeRejectCode, SpokeResult};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::num::NonZeroU64;

/// The `connect_hello` wire type used by `spoke_connect::ConnectConfig.local_manifest`.
///
/// Codegen inlines `$ref` types, so this is field-identical to the data-type
/// [`HostCapabilityManifest`] but a **distinct generated type** (per
/// spoke-connect crate docs) — the JSON round-trip in [`to_connect_hello`] is
/// the conversion. Re-exported here so Connect Host consumers spell the type
/// through the single spoke-adapter import boundary instead of adding a
/// direct `spoke-schemas` dependency.
pub use spoke_schemas::connect::connect_hello::HostCapabilityManifest as ConnectHelloManifest;

/// Wire schema version of this manifest (`NonZeroU64`, matches every spoke
/// 0.8.2 fixture and the previous static manifest).
pub const MANIFEST_SCHEMA_VERSION: NonZeroU64 = NonZeroU64::MIN;

/// Roles declared by the local host (N-C2 E2, V1.154 P2).
///
/// The open-string roles backing the served Connect op set: `data-store`
/// (storage), `checker` (served `check`), `assembler` (served `assemble`),
/// and `computable-engine` (served `compute` — the semantic
/// reasoning-complete milestone wire form; the literal
/// `"reasoning-complete"` string stays absent per the product lock, spec
/// §4/§6.6 — the role vocabulary is open-string per the
/// host-capability-manifest schema, verified not invented).
pub const LOCAL_ROLES: [&str; 4] = ["data-store", "checker", "assembler", "computable-engine"];

/// Capabilities advertised by the local host — each maps to a production
/// adapter port (see the honesty test below for the compile-time proof).
pub const LOCAL_CAPABILITIES: [&str; 3] = ["spoke-baseline", "l2-computable", "l5-fork"];

/// Namespaces owned by the local host.
pub const LOCAL_NAMESPACES: [&str; 1] = ["nexus"];

/// Ops the Nexus host serves over Connect (N-C1 writes → N-C2 read half →
/// P2 compute: `check` / `assemble` / `compute`) — the manifest's
/// advertised op set (`extensions.nexus.served_ops`).
///
/// The connect-host dispatch gate owns the same set as its served-op table
/// (`apps/nexus42` `commands::connect::invoke::SERVED_OPS`); the honesty
/// tests machine-check both directions (advertised ⇔ served, see
/// [`build_local_host_manifest`] docs) so the two cannot drift unnoticed.
pub const LOCAL_SERVED_OPS: [&str; 6] = [
    "upsert", "promote", "relate", "check", "assemble", "compute",
];

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
            // Slice marker (P1 QC fix wave FW-6): "n-c2" tracks the
            // delivered N-C2 read-half surface — the served set is
            // authoritative (`served_ops`); the marker is the ladder
            // position, not a capability list.
            "connect_host_slice": "n-c2",
            "served_ops": LOCAL_SERVED_OPS,
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

/// Convert the data-type manifest into the field-identical `connect_hello`
/// wire type via a JSON round-trip (the connect variant has no `PartialEq`,
/// so equality is asserted at the JSON level).
///
/// # Errors
/// Returns `InternalError` when the round-trip fails. The two types are
/// schema-identical today, so this only fires if a future schema edit
/// desynchronizes them — the locked wire family must stay in lockstep.
#[must_use]
pub fn to_connect_hello(manifest: &HostCapabilityManifest) -> SpokeResult<ConnectHelloManifest> {
    let value = match serde_json::to_value(manifest) {
        Ok(value) => value,
        Err(e) => {
            return SpokeResult::Reject(SpokeReject {
                code: SpokeRejectCode::InternalError,
                message: format!("data manifest serialization failed: {e}"),
                details: None,
            });
        }
    };
    match serde_json::from_value(value) {
        Ok(manifest) => SpokeResult::Ok(manifest),
        Err(e) => SpokeResult::Reject(SpokeReject {
            code: SpokeRejectCode::InternalError,
            message: format!("connect_hello manifest round-trip failed: {e}"),
            details: None,
        }),
    }
}

/// Convert the `connect_hello` wire manifest back into the data-type
/// [`HostCapabilityManifest`] via a JSON round-trip — the companion of
/// [`to_connect_hello`].
///
/// The wire variant has no `PartialEq`, so equality is asserted at the JSON
/// level. V1.155 P0 N-C3: the outbound `connect()` return exposes the
/// peer's manifest as the wire type (`spoke_connect::PeerSession::remote_manifest`
/// returns `&ConnectHelloManifest`); the adapter's peer-recording boundary
/// (`NexusAdapter::record_peer_manifest`) consumes the data type, so the
/// recording wiring converts here.
///
/// # Errors
/// Returns `InternalError` when the round-trip fails. The two types are
/// schema-identical today, so this only fires if a future schema edit
/// desynchronizes them — the locked wire family must stay in lockstep.
#[must_use]
pub fn from_connect_hello(wire: &ConnectHelloManifest) -> SpokeResult<HostCapabilityManifest> {
    let value = match serde_json::to_value(wire) {
        Ok(value) => value,
        Err(e) => {
            return SpokeResult::Reject(SpokeReject {
                code: SpokeRejectCode::InternalError,
                message: format!("connect_hello manifest serialization failed: {e}"),
                details: None,
            });
        }
    };
    match serde_json::from_value(value) {
        Ok(manifest) => SpokeResult::Ok(manifest),
        Err(e) => SpokeResult::Reject(SpokeReject {
            code: SpokeRejectCode::InternalError,
            message: format!("data manifest round-trip failed: {e}"),
            details: None,
        }),
    }
}

/// One-call N-C0 builder for the Connect Host's `ConnectConfig.local_manifest`.
///
/// Same single-builder SSOT as [`build_local_host_manifest`]; the connect
/// wire type is the JSON round-trip of the data type.
///
/// # Errors
/// `InvalidInput` when `host_id` is empty; `InternalError` on a wire-type
/// round-trip failure (schema lockstep drift).
#[must_use]
pub fn build_connect_hello_manifest(host_id: &str) -> SpokeResult<ConnectHelloManifest> {
    match build_local_host_manifest(host_id) {
        SpokeResult::Ok(manifest) => to_connect_hello(&manifest),
        SpokeResult::Reject(reject) => SpokeResult::Reject(reject),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NexusAdapter;

    /// Compile-time production-orchestrator proof for one advertised write op.
    ///
    /// The `orchestrate_*` entrypoints are re-exported from `spoke_operations`
    /// (Surface B, spec §7.3) and generic over the ports bound. Each closure
    /// is **typecheck-only — never invoked**: its body must compile, proving
    /// the op maps to a production orchestrator that accepts the production
    /// adapter's ports bound. An advertised op whose orchestrator regressed
    /// (or whose port implementation disappeared) stops compiling here.
    fn assert_op_maps_to_production_orchestrator(adapter: &NexusAdapter<'_>, op: &str) {
        match op {
            "upsert" => {
                let _ = || {
                    let request: crate::UpsertRequest =
                        serde_json::from_value(serde_json::json!({})).expect("typecheck-only");
                    let _ = crate::orchestrate_upsert(adapter, request);
                };
            }
            "promote" => {
                let _ = || {
                    let request: crate::PromoteRequest =
                        serde_json::from_value(serde_json::json!({})).expect("typecheck-only");
                    let _ = crate::orchestrate_promote(adapter, request);
                };
            }
            "relate" => {
                let _ = || {
                    let request: crate::RelateRequest =
                        serde_json::from_value(serde_json::json!({})).expect("typecheck-only");
                    let _ = crate::orchestrate_relate(adapter, request);
                };
            }
            "check" => {
                let _ = || {
                    let request: crate::CheckRequest =
                        serde_json::from_value(serde_json::json!({})).expect("typecheck-only");
                    // Typecheck-only: the baseline no-op checker is the
                    // production run_checker shape (V1.148 daemon cutover).
                    let _ = crate::orchestrate_check(adapter, request, |_input| {
                        SpokeResult::Ok(vec![])
                    });
                };
            }
            "assemble" => {
                let _ = || {
                    let request: crate::AssembleRequest =
                        serde_json::from_value(serde_json::json!({})).expect("typecheck-only");
                    let _ = crate::orchestrate_assemble(adapter, request);
                };
            }
            "compute" => {
                let _ = || {
                    let request: crate::ComputeRequest =
                        serde_json::from_value(serde_json::json!({})).expect("typecheck-only");
                    let _ = crate::orchestrate_compute(adapter, request);
                };
            }
            other => panic!("advertised op {other:?} is not backed by a production orchestrator"),
        }
    }

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

    /// AC-I4.3 / product draft §4.3 + P1 spec § Manifest honesty — the
    /// manifest honesty test (machine-checkable). Renamed for the delivered
    /// slice: it now machine-checks the N-C1 → N-C2 read-half manifest
    /// (roles + served ops) while keeping the full N-C0 baseline
    /// (capabilities / namespaces / no `"reasoning-complete"`).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn n_c1_manifest_is_honest() {
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

        // 3. roles == the locked list exactly — data-store (storage) +
        //    checker + assembler + computable-engine (the N-C2 E2
        //    open-string roles, spec §4/§6.6; computable-engine is the
        //    semantic reasoning-complete wire form — no literal string).
        //    Role ⇔ served-op pairing: checker backs check, assembler backs
        //    assemble, computable-engine backs compute — both directions.
        assert_eq!(
            manifest.roles,
            LOCAL_ROLES.map(ToString::to_string).to_vec()
        );
        let has_role = |role: &str| manifest.roles.iter().any(|r| r == role);
        assert_eq!(
            has_role("checker"),
            LOCAL_SERVED_OPS.contains(&"check"),
            "role checker ⇔ served op check must hold in both directions"
        );
        assert_eq!(
            has_role("assembler"),
            LOCAL_SERVED_OPS.contains(&"assemble"),
            "role assembler ⇔ served op assemble must hold in both directions"
        );
        assert_eq!(
            has_role("computable-engine"),
            LOCAL_SERVED_OPS.contains(&"compute"),
            "role computable-engine ⇔ served op compute must hold in both directions"
        );

        // 4. namespaces == ["nexus"].
        assert_eq!(manifest.namespaces.len(), 1);
        assert_eq!(manifest.namespaces[0].as_str(), "nexus");

        // 5. authority is None (absent).
        assert!(
            manifest.authority.is_none(),
            "N-C0 must not declare authority"
        );

        // 6. No "reasoning-complete" anywhere in the manifest JSON (the
        //    semantic milestone is expressed as `computable-engine` role +
        //    `l2-computable` capability, never a literal string — spec
        //    §4/§6.6).
        let manifest_json = serde_json::to_value(&manifest).expect("manifest serializes");
        assert!(
            !manifest_json.to_string().contains("reasoning-complete"),
            "MUST NOT advertise the literal reasoning-complete string (semantic milestone is \
             computable-engine + l2-computable)"
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

        // 9. extensions["nexus"] carries the N-C2 read-half marker block.
        let nexus_key = "nexus".parse().expect("locked extension key parses");
        let nexus_ext = manifest
            .extensions
            .get(&nexus_key)
            .expect("extensions.nexus block present");
        assert_eq!(
            nexus_ext.get("connect_host_slice").and_then(Value::as_str),
            Some("n-c2")
        );
        assert_eq!(
            nexus_ext
                .get("daemon_http_coexists")
                .and_then(Value::as_bool),
            Some(true)
        );

        // 10. N-C1 served-op advertisement (P1 spec § Manifest honesty,
        //     machine-checkable): extensions.nexus.served_ops is EXACTLY
        //     LOCAL_SERVED_OPS — no more, no fewer — and every advertised op
        //     maps to a production orchestrator (compile-time proof).
        let served_ops = nexus_ext
            .get("served_ops")
            .and_then(Value::as_array)
            .expect("extensions.nexus.served_ops array present");
        assert_eq!(
            served_ops
                .iter()
                .map(|op| op.as_str().expect("served op is a string"))
                .collect::<Vec<_>>(),
            LOCAL_SERVED_OPS,
            "advertised served_ops must be exactly the N-C1 → N-C2 served-op set"
        );
        for op in served_ops
            .iter()
            .map(|op| op.as_str().expect("served op is a string"))
        {
            assert_op_maps_to_production_orchestrator(&adapter, op);
        }
        // Direction (b) — every dispatch-served op is advertised — is
        // machine-checked against the real dispatch table in the
        // connect-host interop suite (apps/nexus42
        // `n_c1_manifest_served_ops_match_dispatch_both_directions`), where
        // both the manifest builder and the dispatch gate are visible.
    }

    #[test]
    fn builder_rejects_empty_host_id() {
        let result = build_local_host_manifest("");
        assert!(
            matches!(result, SpokeResult::Reject(r) if r.code == SpokeRejectCode::InvalidInput),
            "empty host_id must be rejected (schema minLength 1)"
        );
    }

    #[test]
    fn connect_hello_manifest_round_trips_through_the_wire_type() {
        // The Connect Host's `ConnectConfig.local_manifest` must carry the
        // `connect_hello::HostCapabilityManifest` wire type. The conversion
        // is a JSON round-trip of the data type (both generated from the
        // same schema; equality is asserted at the JSON level).
        let data = match build_local_host_manifest("test-device-uuid-0000") {
            SpokeResult::Ok(m) => m,
            SpokeResult::Reject(r) => panic!("builder rejected: {r:?}"),
        };
        let hello = match build_connect_hello_manifest("test-device-uuid-0000") {
            SpokeResult::Ok(m) => m,
            SpokeResult::Reject(r) => panic!("connect hello builder rejected: {r:?}"),
        };
        let data_json = serde_json::to_value(&data).expect("data manifest serializes");
        let hello_json = serde_json::to_value(&hello).expect("hello manifest serializes");
        assert_eq!(
            hello_json, data_json,
            "connect_hello manifest must be field-identical to the data manifest"
        );
        assert_eq!(
            hello_json["host_id"],
            serde_json::json!("test-device-uuid-0000")
        );
        assert_eq!(hello_json["schema_version"], serde_json::json!(1));
        assert_eq!(
            hello_json["extensions"]["nexus"]["connect_host_slice"],
            serde_json::json!("n-c2")
        );
        assert_eq!(
            hello_json["extensions"]["nexus"]["served_ops"],
            serde_json::json!(LOCAL_SERVED_OPS)
        );

        // The round-tripped value deserializes back into the data type
        // without loss (both directions are lossless).
        let back: HostCapabilityManifest =
            serde_json::from_value(hello_json).expect("wire type deserializes as data type");
        assert_eq!(
            serde_json::to_value(back).expect("back serializes"),
            data_json
        );

        // Rejected host_id propagates through the connect builder unchanged.
        assert!(
            matches!(
                build_connect_hello_manifest(""),
                SpokeResult::Reject(r) if r.code == SpokeRejectCode::InvalidInput
            ),
            "connect builder must reject empty host_id like the data builder"
        );
    }

    #[test]
    fn from_connect_hello_round_trips_the_wire_type_back_to_data() {
        // N-C3 (V1.155 P0): the outbound `connect()` return exposes the
        // peer's manifest as the `connect_hello` wire type
        // (`PeerSession::remote_manifest()`); the adapter's recording
        // boundary consumes the data type. The reverse conversion must be
        // lossless — a wire manifest round-trips to the identical data
        // manifest (JSON-level equality: neither type has `PartialEq`).
        let data = match build_local_host_manifest("test-device-uuid-0000") {
            SpokeResult::Ok(m) => m,
            SpokeResult::Reject(r) => panic!("builder rejected: {r:?}"),
        };
        let hello = match to_connect_hello(&data) {
            SpokeResult::Ok(m) => m,
            SpokeResult::Reject(r) => panic!("to_connect_hello rejected: {r:?}"),
        };
        let back = match from_connect_hello(&hello) {
            SpokeResult::Ok(m) => m,
            SpokeResult::Reject(r) => panic!("from_connect_hello rejected: {r:?}"),
        };
        assert_eq!(
            serde_json::to_value(&back).expect("back serializes"),
            serde_json::to_value(&data).expect("data serializes"),
            "wire → data round-trip must be lossless"
        );
        assert_eq!(back.host_id, data.host_id);
        assert_eq!(back.capabilities, data.capabilities);
        assert_eq!(back.roles, data.roles);
    }
}
