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
//! ## World-scope gates (V1.153 P1 fix loop)
//!
//! Two fail-closed gates sit in front of the orchestrators, both with zero
//! side effects:
//!
//! 1. **Whole-payload world-id requirement (Important):** EVERY
//!    entry/relation must carry a parseable `extensions.nexus.world_id`.
//!    If any entry lacks one the WHOLE payload is denied — the old
//!    filter-and-continue shape let a mixed payload pass the gate and fail
//!    later in the adapter as a partial-batch write surfaced as
//!    `internal_error`.
//! 2. **Stored-world check (Critical):** the orchestrators' stored lookups
//!    and CAS updates are world-agnostic (id + revision only), so a payload
//!    claiming world A could otherwise rewrite a row stored in world B by
//!    replaying the revision the OCC rejects disclose. Before the
//!    orchestrator CAS runs, every targeted existing row's stored `world_id`
//!    is verified against the payload-claimed `world_id`; a mismatch denies
//!    the op.
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
use nexus_spoke_adapter::extensions::get_world_id;
use nexus_spoke_adapter::{
    orchestrate_promote, orchestrate_relate, orchestrate_upsert, KnowledgeEntryPort, NexusAdapter,
    PromoteRequest, PromoteResponse, RelateRequest, RelateResponse, Relation,
    RelationExtensionsKey, RelationPort, SpokeReject, SpokeRejectCode, SpokeResult, UpsertRequest,
    UpsertResponse,
};
use serde_json::{Map, Value};
use spoke_connect::InvokeHandler;
use spoke_schemas::connect::connect_invoke_response::ErrorEnvelope;
use std::collections::HashMap;
use std::sync::Arc;

/// The write ops this host serves (N-C1).
///
/// This const is load-bearing, not declaration-only: [`dispatch`] gates on
/// it before routing, so the host can never serve an op it does not list.
/// The manifest-honesty test
/// (`n_c1_manifest_served_ops_match_dispatch_both_directions` in
/// `commands::connect::interop`) machine-checks this set ⇔ the manifest's
/// advertised `extensions.nexus.served_ops` (`nexus_spoke_adapter`'s
/// `LOCAL_SERVED_OPS`) in both directions — so the manifest ⇔ actual
/// dispatch routing lockstep holds by construction.
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
    // 1. Served-op gate (N-C1): `SERVED_OPS` is the load-bearing serving
    //    gate — the manifest-honesty machine check
    //    (`n_c1_manifest_served_ops_match_dispatch_both_directions`)
    //    verifies the manifest against this const, so dispatch MUST read it
    //    too, or the check would bind to a declaration-only table. Anything
    //    outside the const is refused unconditionally, regardless of payload
    //    shape (N-C0 refusal contract extends); a match arm for an op that
    //    is not in `SERVED_OPS` is unreachable by construction.
    if !SERVED_OPS.contains(&op) {
        return Err(unsupported(
            op,
            "this host serves only upsert / promote / relate",
        ));
    }

    // 2. Map the gate-passed op to its route. The arms cover exactly the
    //    `SERVED_OPS` set; the `_` tail below is unreachable for served ops
    //    (the gate refused everything else) and stays as a defensive
    //    fallthrough.
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

    // 3. Calling peer from the ops envelope (fail-closed — see module docs
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

    // 4. Op-scope gate (T1 PeerScope, fail-closed).
    if !scope.allows_op(&peer, op) {
        return Err(denied(&format!("op {op} is not in this peer's op_scope")));
    }

    // 5. World-scope gate (T1 PeerScope, fail-closed): every target world in
    //    the payload must be in the peer's `world_scope`. Strictness (fix
    //    loop, Important): EVERY entry/relation must carry a parseable
    //    `extensions.nexus.world_id` — a payload where any entry lacks one
    //    denies the WHOLE payload (no filter-and-continue; that shape let a
    //    mixed payload pass the gate and fail later as a partial write).
    let Some(worlds) = payload_world_ids(route, &payload) else {
        return Err(denied(
            "invoke payload requires extensions.nexus.world_id on every entry/relation; \
             cannot verify world scope",
        ));
    };
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

    // 6. Stored-world gate (fix loop, Critical): the orchestrators' stored
    //    lookups and CAS updates are world-agnostic (they match on id +
    //    revision only), so a payload claiming world A could rewrite a row
    //    stored in world B by replaying the revision the OCC rejects
    //    disclose. Before the orchestrator CAS runs, verify every targeted
    //    existing row's stored world_id equals the payload-claimed world_id;
    //    a mismatch denies with zero side effects.
    //
    // 7. Route through the orchestrator. The orchestrators are native async
    //    fn (V1.153 P0 T2) but this closure is sync on the node's event
    //    loop: bridge with block_in_place + Handle::block_on (multi-thread
    //    runtime; the CLI main and tokio::test default are multi-thread).
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            verify_stored_worlds(adapter, route, &payload).await?;
            route_orchestrator(route, adapter, payload).await
        })
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
///
/// Strictness (fix loop, Important): EVERY entry/relation must carry a
/// parseable world id. `None` means the payload cannot be scoped as a whole
/// — an entry/relation lacks the carrier (or the container is absent) — and
/// the whole payload is denied. Entries are never filtered out of the set.
fn payload_world_ids(route: Route, payload: &Value) -> Option<Vec<String>> {
    match route {
        Route::Upsert => {
            let entries = payload.get("knowledge_entries")?.as_array()?;
            let mut worlds = Vec::with_capacity(entries.len());
            for entry in entries {
                worlds.push(world_id_of(entry)?.to_string());
            }
            Some(worlds)
        }
        Route::Promote => {
            let candidate = payload.get("candidate")?;
            Some(vec![world_id_of(candidate)?.to_string()])
        }
        Route::Relate => {
            let relation = payload.get("relation")?;
            Some(vec![world_id_of(relation)?.to_string()])
        }
    }
}

/// Verify the stored-world invariant before any orchestrator CAS (fix loop,
/// Critical): for every payload entry/relation that already exists in
/// storage, the stored row's `world_id` must equal the payload-claimed
/// `world_id`. The orchestrators' lookups and CAS updates match on id +
/// revision only (world-agnostic), so without this gate a peer scoped to
/// world A could rewrite a row stored in world B by replaying the revision
/// disclosed by OCC rejects. Denials happen before any write — zero side
/// effects. Rows that do not exist yet (create paths) carry no stored world
/// and need no check.
async fn verify_stored_worlds(
    adapter: &NexusAdapter<'static>,
    route: Route,
    payload: &Value,
) -> Result<(), ErrorEnvelope> {
    match route {
        Route::Upsert => {
            if let Some(entries) = payload.get("knowledge_entries").and_then(Value::as_array) {
                for entry in entries {
                    let Some(entry_id) = entry.get("entry_id").and_then(Value::as_str) else {
                        // No id ⇒ no stored row can be targeted; the typed
                        // parse rejects the payload later.
                        continue;
                    };
                    let Some(claimed) = world_id_of(entry) else {
                        // Unreachable: the world gate (step 4) already
                        // denied any payload with an entry lacking the id.
                        // Fail closed rather than skip the check.
                        return Err(denied(
                            "entry missing extensions.nexus.world_id; cannot verify stored world",
                        ));
                    };
                    assert_stored_entry_world_matches(adapter, entry_id, claimed).await?;
                }
            }
        }
        Route::Promote => {
            if let Some(candidate) = payload.get("candidate") {
                if let Some(entry_id) = candidate.get("entry_id").and_then(Value::as_str) {
                    let Some(claimed) = world_id_of(candidate) else {
                        return Err(denied(
                            "candidate missing extensions.nexus.world_id; cannot verify stored world",
                        ));
                    };
                    assert_stored_entry_world_matches(adapter, entry_id, claimed).await?;
                }
            }
        }
        Route::Relate => {
            if let Some(relation) = payload.get("relation") {
                if let Some(relation_id) = relation.get("relation_id").and_then(Value::as_str) {
                    let Some(claimed) = world_id_of(relation) else {
                        return Err(denied(
                            "relation missing extensions.nexus.world_id; cannot verify stored world",
                        ));
                    };
                    match adapter.get_relation(relation_id).await {
                        SpokeResult::Ok(stored) => {
                            if relation_world_id_of(&stored) != Some(claimed) {
                                return Err(cross_world_denied(
                                    "relation",
                                    relation_id,
                                    relation_world_id_of(&stored),
                                    claimed,
                                ));
                            }
                        }
                        SpokeResult::Reject(reject)
                            if reject.code == SpokeRejectCode::RelationNotFound => {}
                        SpokeResult::Reject(reject) => return Err(map_reject(&reject)),
                    }
                }
            }
        }
    }
    Ok(())
}

/// Assert an existing entry's stored `world_id` equals the payload-claimed
/// `world_id` (fix loop, Critical). A missing stored entry (create path) needs
/// no check; any other read reject fails closed through the locked reject
/// mapping (a storage fault must not read as a scope denial).
async fn assert_stored_entry_world_matches(
    adapter: &NexusAdapter<'static>,
    entry_id: &str,
    claimed: &str,
) -> Result<(), ErrorEnvelope> {
    match adapter.get_knowledge_entry(entry_id).await {
        SpokeResult::Ok(stored) => {
            let stored_world = get_world_id(&stored);
            if stored_world != Some(claimed) {
                return Err(cross_world_denied("entry", entry_id, stored_world, claimed));
            }
        }
        SpokeResult::Reject(reject) if reject.code == SpokeRejectCode::KnowledgeEntryNotFound => {}
        SpokeResult::Reject(reject) => return Err(map_reject(&reject)),
    }
    Ok(())
}

/// Read `extensions.nexus.world_id` from a stored spoke `Relation` (the
/// typed map lookup — `RelationExtensionsKey` does not implement
/// `Borrow<str>`, mirroring the adapter's own extension-key pattern).
fn relation_world_id_of(relation: &Relation) -> Option<&str> {
    let key = RelationExtensionsKey::try_from("nexus")
        .expect("\"nexus\" matches the extensions-key regex");
    relation
        .extensions
        .get(&key)
        .and_then(|namespace| namespace.get("world_id"))
        .and_then(Value::as_str)
}

/// Cross-world stored-mismatch denial: same `op_unsupported` refusal family
/// as the world-scope gate (the plan's locked code for wrong-world), with a
/// human-readable reason naming the row and both worlds. No information
/// about OTHER peers' scopes leaks; the stored world of a row the caller
/// already targets by id is disclosed (consistent with the OCC rejects,
/// which already disclose stored revisions).
fn cross_world_denied(
    kind: &str,
    id: &str,
    stored_world: Option<&str>,
    claimed: &str,
) -> ErrorEnvelope {
    denied(&format!(
        "stored {kind} {id} belongs to world {}; refusing cross-world write (claimed world {claimed})",
        stored_world.unwrap_or("<unknown>"),
    ))
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
