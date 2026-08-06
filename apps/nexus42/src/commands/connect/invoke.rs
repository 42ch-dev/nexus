//! N-C1 Connect invoke dispatch (DF-72, V1.153 P1) — the architect-locked
//! home of the `InvokeHandler` closure.
//!
//! The handler is the product-owned write spine of the Connect Host: it
//! parses the inbound invoke op/payload, resolves the calling peer, gates it
//! through the fail-closed `PeerScope` allowlist (T1), and routes exactly
//! `upsert` / `promote` / `relate` through the production `NexusAdapter`
//! orchestrators (re-exported via `nexus_spoke_adapter`). Contract sources:
//! `.mstar/specs/spoke-adapter-architecture.md` §10.6 and the P1 spec
//! § OCC + error mapping / § World scoping.
//!
//! Every other op — `check` / `assemble` / `project` / `compute` / unknown —
//! is refused with `ErrorEnvelope.code = "op_unsupported"` and zero side
//! effects (the N-C0 refusal contract extends).
//!
//! ## Caller identity (wire reality, documented)
//!
//! The locked spoke-connect 0.9.1 handler signature
//! (`dyn Fn(&str, serde_json::Value) -> Result<Value, ErrorEnvelope>`) does
//! NOT carry the authenticated session peer — the node calls the closure
//! with `(op, payload)` only. The allowlist handshake is the trust root
//! (only allowlisted peers ever reach the handler); the per-invoke **caller
//! peer id rides in the ops request envelope** under
//! `extensions.nexus.peer_id`, which the dispatch gate resolves and then
//! checks against the peer's `world_scope` / `op_scope`. Resolution is
//! fail-closed: a missing/unparseable `peer_id` denies the op. The target
//! world id(s) are read from the payload entries/relation
//! (`extensions.nexus.world_id` — the canonical carrier the conversion seam
//! uses); a payload without a world id is denied (cannot verify scope).
//!
//! ## Async bridge
//!
//! The orchestrators are native `async fn` (V1.153 P0 T2) but the handler
//! runs synchronously on the spoke-connect network event loop, so dispatch
//! bridges with `tokio::task::block_in_place` +
//! `Handle::current().block_on` (multi-thread runtime only — both the CLI
//! `main` and `#[tokio::test]` default are multi-thread). The adapter is a
//! **per-process singleton** constructed once at host boot and held for the
//! process lifetime (P1 spec § Process model); per-invoke construction is
//! deliberately avoided.

use super::allowlist::PeerScope;
use libp2p::PeerId;
use nexus_spoke_adapter::{
    orchestrate_promote, orchestrate_relate, orchestrate_upsert, NexusAdapter, PromoteRequest,
    PromoteResponse, RelateRequest, RelateResponse, SpokeReject, SpokeRejectCode, SpokeResult,
    UpsertRequest, UpsertResponse,
};
use serde_json::{Map, Value};
use spoke_connect::InvokeHandler;
use spoke_schemas::connect::connect_invoke_response::ErrorEnvelope;
use std::collections::HashMap;
use std::sync::Arc;

/// The write ops this host serves (N-C1). T3's manifest-honesty test
/// machine-checks this set ⇔ the advertised capabilities.
pub const SERVED_OPS: [&str; 3] = ["upsert", "promote", "relate"];

/// Build the N-C1 `InvokeHandler`: a fail-closed op gate + allowlist
/// world/op scope gate in front of the three `NexusAdapter` orchestrators.
///
/// The returned closure is `Send + Sync` (the node holds it in an
/// `Arc<InvokeHandler>`); `scope` and `adapter` are captured for the
/// process lifetime — one adapter per Connect process (P1 spec § Process
/// model).
#[must_use]
pub fn build_handler(scope: PeerScope, adapter: Arc<NexusAdapter<'static>>) -> Arc<InvokeHandler> {
    Arc::new(move |op: &str, payload: Value| dispatch(&scope, &adapter, op, payload))
}

/// One served write op.
#[derive(Debug, Clone, Copy)]
enum Route {
    Upsert,
    Promote,
    Relate,
}

/// The full dispatch pipeline. Every gate is fail-closed and runs before
/// any orchestrator call, so denials have zero side effects.
fn dispatch(
    scope: &PeerScope,
    adapter: &NexusAdapter<'static>,
    op: &str,
    payload: Value,
) -> Result<Value, ErrorEnvelope> {
    // 1. Served op set (N-C1): anything else is refused unconditionally,
    //    regardless of payload shape (N-C0 refusal contract extends).
    let route = match op {
        "upsert" => Route::Upsert,
        "promote" => Route::Promote,
        "relate" => Route::Relate,
        _ => {
            return Err(unsupported(
                op,
                "this host serves only upsert / promote / relate",
            ));
        }
    };

    // 2. Calling peer from the ops envelope (fail-closed — see module docs
    //    for why the peer id must ride the payload).
    let Some(peer) = payload
        .pointer("/extensions/nexus/peer_id")
        .and_then(Value::as_str)
        .and_then(|raw| raw.parse::<PeerId>().ok())
    else {
        return Err(denied(
            "invoke payload must declare the calling peer id in extensions.nexus.peer_id",
        ));
    };

    // 3. Op-scope gate (T1 PeerScope, fail-closed).
    if !scope.allows_op(&peer, op) {
        return Err(denied(&format!("op {op} is not in this peer's op_scope")));
    }

    // 4. World-scope gate (T1 PeerScope, fail-closed): every target world in
    //    the payload must be in the peer's `world_scope`; a payload without
    //    a world id cannot be scoped and is denied.
    let worlds = payload_world_ids(route, &payload);
    if worlds.is_empty() {
        return Err(denied(
            "invoke payload carries no world id; cannot verify world scope",
        ));
    }
    if let Some(world) = worlds
        .iter()
        .find(|world| !scope.allows_world(&peer, world))
    {
        return Err(denied(&format!(
            "world {world} is not in this peer's world_scope"
        )));
    }

    // 5. Route through the orchestrator. The orchestrators are native async
    //    fn (V1.153 P0 T2) but this closure is sync on the node's event
    //    loop: bridge with block_in_place + Handle::block_on (multi-thread
    //    runtime; the CLI main and tokio::test default are multi-thread).
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(route_orchestrator(route, adapter, payload))
    })
}

/// Run one served op through its orchestrator and map the outcome to the
/// handler's return type. Rejects map through the locked
/// `SpokeRejectCode → ErrorEnvelope` table (P1 spec § OCC + error mapping).
async fn route_orchestrator(
    route: Route,
    adapter: &NexusAdapter<'static>,
    payload: Value,
) -> Result<Value, ErrorEnvelope> {
    match route {
        Route::Upsert => {
            let request: UpsertRequest = match serde_json::from_value(payload) {
                Ok(request) => request,
                Err(error) => return Err(map_reject(&invalid_payload("upsert", &error))),
            };
            match orchestrate_upsert(adapter, request).await {
                SpokeResult::Ok(response) => serialize_response::<UpsertResponse>(&response),
                SpokeResult::Reject(reject) => Err(map_reject(&reject)),
            }
        }
        Route::Promote => {
            let request: PromoteRequest = match serde_json::from_value(payload) {
                Ok(request) => request,
                Err(error) => return Err(map_reject(&invalid_payload("promote", &error))),
            };
            match orchestrate_promote(adapter, request).await {
                SpokeResult::Ok(response) => serialize_response::<PromoteResponse>(&response),
                SpokeResult::Reject(reject) => Err(map_reject(&reject)),
            }
        }
        Route::Relate => {
            let request: RelateRequest = match serde_json::from_value(payload) {
                Ok(request) => request,
                Err(error) => return Err(map_reject(&invalid_payload("relate", &error))),
            };
            match orchestrate_relate(adapter, request).await {
                SpokeResult::Ok(response) => serialize_response::<RelateResponse>(&response),
                SpokeResult::Reject(reject) => Err(map_reject(&reject)),
            }
        }
    }
}

/// Read the canonical world-id carrier from a wire object
/// (`extensions.nexus.world_id` — the field the conversion seam writes).
fn world_id_of(value: &Value) -> Option<&str> {
    value
        .pointer("/extensions/nexus/world_id")
        .and_then(Value::as_str)
}

/// Target world ids inside an ops payload, in wire order — raw JSON reads so
/// the scope gate runs before any typed parse (fail-closed, zero side
/// effects). `extensions.nexus.world_id` is the canonical carrier the
/// conversion seam writes on entries/relations.
fn payload_world_ids(route: Route, payload: &Value) -> Vec<String> {
    match route {
        Route::Upsert => payload
            .get("knowledge_entries")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| world_id_of(entry).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        Route::Promote => payload
            .get("candidate")
            .and_then(world_id_of)
            .map(str::to_string)
            .into_iter()
            .collect(),
        Route::Relate => payload
            .get("relation")
            .and_then(world_id_of)
            .map(str::to_string)
            .into_iter()
            .collect(),
    }
}

/// Locked `SpokeRejectCode → ErrorEnvelope` mapping (P1 spec § OCC + error
/// mapping — verbatim):
///
/// | `SpokeRejectCode` | `ErrorEnvelope.code` | Retry-safe? |
/// |-------------------|----------------------|-------------|
/// | `KnowledgeEntryAlreadyExists` | `knowledge_entry_already_exists` | yes |
/// | `StoredRevisionStale` | `stored_revision_stale` | yes |
/// | `RevisionConflict` | `revision_conflict` | yes |
/// | `InternalError` | `internal_error` | no |
/// | any other reject | `internal_error` (carries `reject.code`/`message` in `details`) | no |
///
/// `reject.message` flows into `ErrorEnvelope.message`; `reject.details`
/// (when present) flows into `ErrorEnvelope.details`.
fn map_reject(reject: &SpokeReject) -> ErrorEnvelope {
    let code = match reject.code {
        SpokeRejectCode::KnowledgeEntryAlreadyExists => "knowledge_entry_already_exists",
        SpokeRejectCode::StoredRevisionStale => "stored_revision_stale",
        SpokeRejectCode::RevisionConflict => "revision_conflict",
        // Locked safe default for every other reject (incl. InternalError).
        _ => "internal_error",
    };
    let mut details = Map::new();
    if let Some(extra) = &reject.details {
        details.extend(extra.clone());
    }
    // The safe-default branch preserves the original reject identity so a
    // client can distinguish a validation reject from a server fault.
    if code == "internal_error" && reject.code != SpokeRejectCode::InternalError {
        details.insert(
            "reject_code".to_string(),
            Value::String(reject.code.as_str().to_string()),
        );
        details.insert(
            "reject_message".to_string(),
            Value::String(reject.message.clone()),
        );
    }
    ErrorEnvelope {
        code: code.to_string(),
        message: reject.message.clone(),
        details,
        extensions: HashMap::default(),
    }
}

/// A synthetic `InvalidInput` reject for a payload the handler cannot
/// deserialize into the typed ops envelope. Mapped through the same locked
/// table as orchestrator rejects (a malformed envelope would have produced
/// the identical `InvalidInput` from the orchestrator's own `wire_convert`).
fn invalid_payload(op: &str, error: &serde_json::Error) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::InvalidInput,
        message: format!("invalid {op} payload: {error}"),
        details: None,
    }
}

/// Serialize an orchestrator success response into the opaque ops response
/// success body the wire carries. Serialization cannot fail for a typed
/// spoke response; a failure would be a wire-shape drift, mapped to
/// `internal_error` (server fault).
fn serialize_response<T: serde::Serialize>(response: &T) -> Result<Value, ErrorEnvelope> {
    serde_json::to_value(response).map_err(|error| ErrorEnvelope {
        code: "internal_error".to_string(),
        message: format!("orchestrator response serialization failed: {error}"),
        details: Map::new(),
        extensions: HashMap::default(),
    })
}

/// Non-served op refusal (N-C0 contract): `op_unsupported`, zero side
/// effects.
fn unsupported(op: &str, reason: &str) -> ErrorEnvelope {
    ErrorEnvelope {
        code: "op_unsupported".to_string(),
        message: format!("op {op} is not supported: {reason}"),
        details: Map::new(),
        extensions: HashMap::default(),
    }
}

/// Scope-gate denial: same `op_unsupported` refusal family as N-C0 (P1 spec
/// § World scoping — "the spoke `op_unsupported` family"), with a
/// human-readable reason. No information about other peers' scopes leaks.
fn denied(reason: &str) -> ErrorEnvelope {
    ErrorEnvelope {
        code: "op_unsupported".to_string(),
        message: format!("op denied: {reason}"),
        details: Map::new(),
        extensions: HashMap::default(),
    }
}
