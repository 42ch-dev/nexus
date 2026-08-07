//! N-C1 Connect invoke dispatch (DF-72, V1.153 P1 → V1.154 P0 T2) — the
//! architect-locked home of the session-peer `InvokeHandlerV2` closure.
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
//! ## Caller identity (session peer — E2, V1.154 P0 T2)
//!
//! The host registers through the spoke-connect 0.9.2 **session-peer** hook
//! (`ConnectConfig::invoke_handler_v2` / [`InvokeHandlerV2`]): the node
//! calls the closure with the noise-authenticated **session peer id** as
//! its first argument — the peer that passed the allowlist, signed-hello,
//! and envelope-auth gates — never a payload-carried claim. The allowlist
//! handshake is the trust root; the session peer is the per-invoke caller
//! identity the `world_scope` / `op_scope` gates resolve against.
//!
//! The payload's `extensions.nexus.peer_id` is **informational only**
//! (spec §5.1 lock): when present it must equal the session peer — a
//! differing or unparseable claim is denied through the allowlist-denial
//! path (`op_unsupported` family) before any orchestrator call (hard deny,
//! fail-closed, zero side effects). Absent ⇒ the session peer is
//! authoritative. The legacy payload-carried identity path is removed from
//! this host (clean cutover, no dual registration). The target world id(s)
//! are read from the payload entries/relation
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
//!    the op. The relate **create** path additionally verifies its
//!    endpoints (plan QC, QC1 F-001 / QC2 W-2): `from_id` / `to_id` must
//!    exist and their stored worlds must equal the claimed world —
//!    `kb_relationships` FKs are single-column on `key_block_id`, so a
//!    world-A relation row could otherwise legally reference world-B
//!    entries (cross-world edge + id-existence oracle).
//!
//! ## Bounded async bridge (R2 closure, V1.154 P1)
//!
//! The orchestrators are native `async fn` (V1.153 P0 T2) but the handler
//! runs synchronously on the spoke-connect network event loop, so dispatch
//! moves the call to a per-process **`spawn_blocking` lane** capped by one
//! `tokio::sync::Semaphore` (spec §5.3 — the architect-locked shape; the
//! lane `Arc` lives beside the adapter singleton captured here). The
//! orchestrator call runs as a `Handle::block_on` inside the lane closure —
//! legal there because a blocking-pool thread is not inside an async
//! execution context, unlike the event-loop thread that calls the handler
//! (where `Handle::block_on` panics and the old worker-blocking bridge is
//! banned by the R2 contract). The handler thread waits with a bounded
//! synchronous acquire (polling the semaphore future with a parking waker)
//! and a bounded result wait over a std channel.
//!
//! Architect-locked limits (spec §5.4 — [`BridgeLimits`]): **8** concurrent
//! invokes per process, a **30,000 ms** per-invoke deadline, and **500**
//! logical collection entries or **2 MiB** serialized request bytes
//! (whichever is reached first). The deadline bounds the permit acquire
//! AND the result wait; the permit stays held until the lane closure
//! returns (the closure cannot be force-cancelled safely) — a late result
//! is discarded. Denials extend the N-C1 envelope table with the locked
//! bridge codes: `invoke_busy` (lane saturated), `invoke_deadline_exceeded`
//! (per-invoke budget exhausted), `payload_too_large` (over-cap payload).
//!
//! The adapter is a **per-process singleton** constructed once at host
//! boot and held for the process lifetime (P1 spec § Process model);
//! per-invoke construction is deliberately avoided.

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
use spoke_connect::InvokeHandlerV2;
use spoke_schemas::connect::connect_invoke_response::ErrorEnvelope;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

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

/// Architect-locked bounded-bridge limits (spec §5.4).
///
/// Defaults are the locked numbers — **8** concurrent invokes per process,
/// a **30,000 ms** per-invoke deadline, and **500** logical collection
/// entries or **2 MiB** serialized request bytes, whichever is reached
/// first. Tests construct explicit values (1 permit / short deadline) to
/// exercise saturation and deadline paths deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeLimits {
    /// Max concurrent in-flight invokes per process (semaphore permits).
    pub max_concurrent_invokes: usize,
    /// Per-invoke deadline — bounds the permit acquire AND the result wait
    /// (a busy-queue wait respects the same budget).
    pub invoke_deadline: std::time::Duration,
    /// Max logical collection entries per payload (the operation's batch
    /// arrays).
    pub max_collection_entries: usize,
    /// Max serialized request bytes per payload.
    pub max_payload_bytes: usize,
}

impl Default for BridgeLimits {
    fn default() -> Self {
        Self {
            max_concurrent_invokes: 8,
            invoke_deadline: std::time::Duration::from_secs(30),
            max_collection_entries: 500,
            max_payload_bytes: 2 * 1024 * 1024,
        }
    }
}

/// Build the N-C1 `InvokeHandlerV2` on the architect-locked bridge limits.
///
/// The dispatch pipeline (spec §5.4 — [`BridgeLimits::default`]) is a
/// fail-closed op gate + allowlist world/op scope gate in front of the
/// three `NexusAdapter` orchestrators, resolving caller identity from the
/// **session peer** (spoke-connect 0.9.2 session-peer hook, spec §3.2 /
/// §5.1).
///
/// The returned closure is `Send + Sync` (the node holds it in an
/// `Arc<InvokeHandlerV2>`); `scope`, `adapter`, and the bounded lane are
/// captured for the process lifetime — one adapter and one lane per
/// Connect process (P1 spec § Process model).
#[must_use]
pub fn build_handler(
    scope: PeerScope,
    adapter: Arc<NexusAdapter<'static>>,
) -> Arc<InvokeHandlerV2> {
    build_handler_with_limits(scope, adapter, BridgeLimits::default()).0
}

/// Like [`build_handler`] with injectable bridge limits.
///
/// The test seam for the R2 bounded bridge: tests use 1 permit / short
/// deadlines to exercise saturation and deadline paths deterministically;
/// the default limits are the architect-locked numbers. Also returns the
/// process-wide lane so tests (and future multi-route wiring) can observe
/// the same semaphore the handler acquires — e.g. to hold permits and
/// force a saturation.
#[must_use]
pub fn build_handler_with_limits(
    scope: PeerScope,
    adapter: Arc<NexusAdapter<'static>>,
    limits: BridgeLimits,
) -> (Arc<InvokeHandlerV2>, Arc<Semaphore>) {
    let lane = Arc::new(Semaphore::new(limits.max_concurrent_invokes));
    let lane_for_handler = Arc::clone(&lane);
    let handler = Arc::new(move |peer: &PeerId, op: &str, payload: Value| {
        dispatch(
            &scope,
            Arc::clone(&adapter),
            &lane_for_handler,
            limits,
            peer,
            op,
            payload,
        )
    });
    (handler, lane)
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
    adapter: Arc<NexusAdapter<'static>>,
    lane: &Arc<Semaphore>,
    limits: BridgeLimits,
    peer: &PeerId,
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

    // 3. Caller identity = the noise-authenticated **session peer** (the
    //    `peer` argument; spoke-connect 0.9.2 `InvokeHandlerV2` — see
    //    module docs). Spec §5.1 lock: the payload's
    //    `extensions.nexus.peer_id` is informational only — present ⇒ it
    //    MUST equal the session peer (hard deny on mismatch, unparseable,
    //    or >128-char claim — the parse cap below mirrors the spoke
    //    session-core 128-char decode input cap; fail-closed, zero side
    //    effects); absent ⇒ fine. The legacy payload-carried identity path
    //    is removed (clean cutover, no dual registration).
    if let Some(claim) = payload.pointer("/extensions/nexus/peer_id") {
        // Parse cap: reject claims longer than 128 chars before any decode
        // work, mirroring the spoke session-core 128-char decode input cap
        // (libp2p-identity's `FromStr` bs58-decodes without a length bound).
        // Oversized claims share the identity-deny path below.
        let matches_session_peer = claim
            .as_str()
            .filter(|raw| raw.len() <= 128)
            .and_then(|raw| raw.parse::<PeerId>().ok())
            .is_some_and(|claimed| claimed == *peer);
        if !matches_session_peer {
            return Err(denied(
                "extensions.nexus.peer_id does not match the session peer; \
                 caller identity comes from the authenticated session",
            ));
        }
    }

    // 4. Op-scope gate (T1 PeerScope, fail-closed) — identity = session
    //    peer.
    if !scope.allows_op(peer, op) {
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
        .find(|world| !scope.allows_world(peer, world))
    {
        return Err(denied(&format!(
            "world {world} is not in this peer's world_scope"
        )));
    }

    // 6. Payload/batch cap (spec §5.4 — architect-locked): 500 logical
    //    collection entries OR 2 MiB serialized request bytes, whichever
    //    is reached first. Checked before the bridge so an over-cap
    //    request never consumes a lane permit or reaches the orchestrator.
    let entries = payload_collection_entries(route, &payload);
    if entries > limits.max_collection_entries {
        return Err(payload_too_large(&format!(
            "payload carries {entries} collection entries; the cap is {}",
            limits.max_collection_entries
        )));
    }
    let bytes = serde_json::to_vec(&payload)
        .expect("a serde_json::Value always serializes")
        .len();
    if bytes > limits.max_payload_bytes {
        return Err(payload_too_large(&format!(
            "payload serializes to {bytes} bytes; the cap is {}",
            limits.max_payload_bytes
        )));
    }

    // 7. Bounded async bridge (R2 closure — spec §5.3/§5.4): the
    //    orchestrators are native async fn but this closure is sync on the
    //    node's event loop, so the call moves to the per-process
    //    `spawn_blocking` lane capped by [`BridgeLimits`]. The handler runs
    //    inside an entered tokio context (the node's event-loop task),
    //    where `Handle::block_on` panics — the lane closure's own
    //    `block_on` is legal because blocking-pool threads are outside any
    //    async execution context.
    //
    // 8. Stored-world gate (fix loop, Critical), inside the lane closure
    //    before the orchestrator: the orchestrators' stored lookups and
    //    CAS updates are world-agnostic (they match on id + revision only),
    //    so a payload claiming world A could rewrite a row stored in world
    //    B by replaying the revision the OCC rejects disclose. Before the
    //    orchestrator CAS runs, verify every targeted existing row's stored
    //    world_id equals the payload-claimed world_id; a mismatch denies
    //    with zero side effects.
    let deadline = std::time::Instant::now() + limits.invoke_deadline;
    let permit = acquire_permit(lane, deadline)?;
    let (tx, rx) = std::sync::mpsc::channel();
    tokio::task::spawn_blocking(move || {
        // Permit semantics (spec §5.4): the lane closure cannot be
        // force-cancelled safely, so the permit moves in here and stays
        // held until the closure returns — the deadline only bounds the
        // caller's wait, and any late result is discarded below.
        let _permit = permit;
        let result = tokio::runtime::Handle::current().block_on(async {
            verify_stored_worlds(&adapter, route, &payload).await?;
            route_orchestrator(route, &adapter, payload).await
        });
        let _ = tx.send(result);
    });
    match rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(deadline_exceeded()),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(bridge_fault(
            "invoke lane worker terminated before returning a result",
        )),
    }
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

/// Logical collection-entry count for the payload cap (spec §5.4): the
/// operation's batch arrays. `upsert` counts `knowledge_entries`;
/// `promote` / `relate` carry a single candidate / relation (1 when
/// present). The orchestrator's typed parse rejects malformed payloads
/// after the cap gate.
fn payload_collection_entries(route: Route, payload: &Value) -> usize {
    match route {
        Route::Upsert => payload
            .get("knowledge_entries")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        Route::Promote => usize::from(payload.get("candidate").is_some()),
        Route::Relate => usize::from(payload.get("relation").is_some()),
    }
}

/// Verify the stored-world invariant before any orchestrator CAS (fix loop,
/// Critical): for every payload entry/relation that already exists in
/// storage, the stored row's `world_id` must equal the payload-claimed
/// `world_id`. The orchestrators' lookups and CAS updates match on id +
/// revision only (world-agnostic), so without this gate a peer scoped to
/// world A could rewrite a row stored in world B by replaying the revision
/// disclosed by OCC rejects. Denials happen before any write — zero side
/// effects.
///
/// Create paths carry no stored *target row*, but the relate create path
/// still has stored **endpoints**: `kb_relationships` FKs are single-column
/// on `key_block_id` (world-agnostic), so a new relation row claimed in
/// world A must not reference entries stored in world B (plan QC, QC1
/// F-001 / QC2 W-2) — `from_id` / `to_id` are resolved and their stored
/// worlds must equal the claimed world; mismatch or missing endpoint denies
/// the whole payload with zero insert.
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
                            if reject.code == SpokeRejectCode::RelationNotFound =>
                        {
                            // Create path (plan QC, QC1 F-001 / QC2 W-2):
                            // no relation row exists yet, so the stored-row
                            // check above is a no-op — but the INSERT's FKs
                            // are single-column on `key_block_id` (world-
                            // agnostic), so a new world-A relation could
                            // legally reference world-B entries. Endpoints
                            // are immutable on the update path (the update
                            // port carries no endpoint fields), so this
                            // create-path check is what closes the
                            // cross-world-edge gap: require each endpoint's
                            // stored world to equal the claimed world;
                            // mismatch or missing endpoint denies the whole
                            // payload with zero insert.
                            for endpoint in ["from_id", "to_id"] {
                                let Some(endpoint_id) =
                                    relation.get(endpoint).and_then(Value::as_str)
                                else {
                                    return Err(denied(&format!(
                                        "relation missing {endpoint}; cannot verify endpoint world"
                                    )));
                                };
                                assert_relate_endpoint_world_matches(adapter, endpoint_id, claimed)
                                    .await?;
                            }
                        }
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

/// Assert a relate create-path endpoint exists in storage AND its stored
/// `world_id` equals the relation's claimed world (plan QC, QC1 F-001 /
/// QC2 W-2). Unlike [`assert_stored_entry_world_matches`] (whose create
/// paths legitimately reference not-yet-stored rows), a relate endpoint
/// MUST exist: `kb_relationships` FKs are single-column on `key_block_id`,
/// so a missing endpoint would otherwise persist a dangling edge or
/// surface as an FK `internal_error` — an id-existence oracle via insert
/// success vs FK-failure differential. Denied like the stored-world gate:
/// `op_unsupported` family, zero insert.
async fn assert_relate_endpoint_world_matches(
    adapter: &NexusAdapter<'static>,
    endpoint_id: &str,
    claimed: &str,
) -> Result<(), ErrorEnvelope> {
    match adapter.get_knowledge_entry(endpoint_id).await {
        SpokeResult::Ok(stored) => {
            let stored_world = get_world_id(&stored);
            if stored_world != Some(claimed) {
                return Err(cross_world_denied(
                    "entry",
                    endpoint_id,
                    stored_world,
                    claimed,
                ));
            }
        }
        SpokeResult::Reject(reject) if reject.code == SpokeRejectCode::KnowledgeEntryNotFound => {
            return Err(denied(&format!(
                "relation endpoint {endpoint_id} does not exist; cannot verify its world"
            )));
        }
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

/// A [`std::task::Wake`] impl that unparks the owning thread — the waker
/// tokio's semaphore uses to wake the acquiring thread when a permit is
/// released.
struct ThreadWaker(Arc<std::thread::Thread>);

impl std::task::Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// Synchronously acquire a lane permit, bounded by `deadline`.
///
/// The handler runs inside an entered tokio context (the node's event-loop
/// task), where `Handle::block_on` panics and the R2 contract bans the old
/// worker-blocking bridge — so the acquire polls the tokio semaphore
/// future directly from this thread, parking between polls. `park_timeout`
/// keeps the wait bounded even when no wake arrives; the semaphore unparks
/// the thread via [`ThreadWaker`] as soon as a permit is released, so a
/// briefly saturated lane does not wait out the deadline.
fn acquire_permit(
    lane: &Arc<Semaphore>,
    deadline: std::time::Instant,
) -> Result<OwnedSemaphorePermit, ErrorEnvelope> {
    let mut acquire = std::pin::pin!(lane.clone().acquire_owned());
    let waker: std::task::Waker = Arc::new(ThreadWaker(Arc::new(std::thread::current()))).into();
    let mut cx = std::task::Context::from_waker(&waker);
    loop {
        match acquire.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(Ok(permit)) => return Ok(permit),
            std::task::Poll::Ready(Err(_)) => {
                return Err(bridge_fault("invoke lane is closed"));
            }
            std::task::Poll::Pending => {}
        }
        let now = std::time::Instant::now();
        if now >= deadline {
            return Err(busy_lane());
        }
        std::thread::park_timeout(deadline - now);
    }
}

/// Saturated-lane denial (spec §5.3): every permit is held by in-flight
/// invokes and none freed within the per-invoke deadline. Retry-safe — a
/// later invoke may be served.
fn busy_lane() -> ErrorEnvelope {
    ErrorEnvelope {
        code: "invoke_busy".to_string(),
        message: "invoke lane saturated: too many concurrent invokes in flight; retry later"
            .to_string(),
        details: Map::new(),
        extensions: HashMap::default(),
    }
}

/// Deadline denial (spec §5.4): the invoke did not complete within the
/// per-invoke budget. Retry-safe.
fn deadline_exceeded() -> ErrorEnvelope {
    ErrorEnvelope {
        code: "invoke_deadline_exceeded".to_string(),
        message: "invoke exceeded the per-invoke deadline".to_string(),
        details: Map::new(),
        extensions: HashMap::default(),
    }
}

/// Payload-cap denial (spec §5.4): the request is above the locked batch
/// caps. Retry-safe for peers that can shrink their payload.
fn payload_too_large(reason: &str) -> ErrorEnvelope {
    ErrorEnvelope {
        code: "payload_too_large".to_string(),
        message: format!("invoke payload rejected: {reason}"),
        details: Map::new(),
        extensions: HashMap::default(),
    }
}

/// Bridge fault: the lane could not run the invoke at all (closed
/// semaphore, lane worker terminated). Server fault — not retry-safe,
/// mapped through the locked `internal_error` default.
fn bridge_fault(reason: &str) -> ErrorEnvelope {
    ErrorEnvelope {
        code: "internal_error".to_string(),
        message: format!("invoke bridge failure: {reason}"),
        details: Map::new(),
        extensions: HashMap::default(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;
    use nexus_home_layout::connect_allowlist_path;
    use std::time::Duration;

    const WORLD_A: &str = "wld_bridge_a";

    /// A deterministic Ed25519 keypair (the interop golden-test shape).
    fn fixed_keypair(seed: u8) -> Keypair {
        Keypair::ed25519_from_bytes([seed; 32]).expect("fixed seed is a valid ed25519 secret")
    }

    /// A wire-shape upsert entry carrying the `extensions.nexus.world_id`
    /// carrier the dispatch gates read.
    fn entry_fixture(entry_id: &str, world_id: &str) -> Value {
        serde_json::json!({
            "schema_version": 1,
            "entry_id": entry_id,
            "entry_type": "character",
            "canonical_name": entry_id,
            "status": "confirmed",
            "revision": null,
            "body": { "summary": format!("{entry_id} summary") },
            "extensions": { "nexus": { "world_id": world_id } },
        })
    }

    /// A hermetic workspace DB + per-process adapter (the N-C1 golden-test
    /// shape): FK rows for `WORLD_A` so the production adapter's put paths
    /// can persist.
    async fn test_adapter() -> (tempfile::TempDir, Arc<NexusAdapter<'static>>) {
        let temp = tempfile::tempdir().expect("tempdir");
        let db_path = temp.path().join("workspace").join("state.db");
        let pool = crate::db::Schema::init(&db_path)
            .await
            .expect("workspace DB initializes");
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, 'test creator', 'active', 'now', '{}')",
        )
        .bind("ctr_bridge")
        .execute(&pool)
        .await
        .expect("creator seed");
        sqlx::query(
            "INSERT OR IGNORE INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) \
             VALUES (?, 'wrk_bridge', 'ctr_bridge', ?, ?, 'active', 'private', 'manual', '{}')",
        )
        .bind(WORLD_A)
        .bind(WORLD_A)
        .bind(WORLD_A)
        .execute(&pool)
        .await
        .expect("world seed");
        (temp, Arc::new(NexusAdapter::new(pool)))
    }

    /// A `PeerScope` allowlisting `peer` for `WORLD_A` with the upsert op,
    /// written through the on-disk allowlist shape (like the CLI boot).
    fn scoped_scope(peer: PeerId) -> PeerScope {
        let temp = tempfile::tempdir().expect("tempdir");
        let allow_path = connect_allowlist_path(temp.path());
        std::fs::create_dir_all(allow_path.parent().expect("parent dir")).expect("mkdir");
        std::fs::write(
            &allow_path,
            serde_json::json!({ "peer_ids": [{
                "peer_id": peer.to_string(),
                "world_scope": [WORLD_A],
                "op_scope": ["upsert"],
            }] })
            .to_string(),
        )
        .expect("write allowlist");
        crate::commands::connect::allowlist::load(temp.path(), &[]).expect("scoped allowlist loads")
    }

    /// A scoped peer + handler + lane over a hermetic adapter. `_temp`
    /// keeps the DB directory alive for the test's duration.
    async fn test_handler(
        limits: BridgeLimits,
    ) -> (
        Arc<InvokeHandlerV2>,
        Arc<Semaphore>,
        PeerId,
        tempfile::TempDir,
    ) {
        let peer = fixed_keypair(7).public().to_peer_id();
        let scope = scoped_scope(peer);
        let (_temp, adapter) = test_adapter().await;
        let (handler, lane) = build_handler_with_limits(scope, adapter, limits);
        (handler, lane, peer, _temp)
    }

    /// (a) Saturated lane: with the only permit held by a concurrent
    /// invoke, the next invoke fails fast with the locked `invoke_busy`
    /// envelope (spec §5.3) instead of queuing; once the permit frees, the
    /// lane serves again.
    #[tokio::test(flavor = "multi_thread")]
    async fn saturated_lane_returns_invoke_busy_then_recovers() {
        let (handler, lane, peer, _temp) = test_handler(BridgeLimits {
            max_concurrent_invokes: 1,
            invoke_deadline: Duration::from_millis(50),
            ..BridgeLimits::default()
        })
        .await;
        let held = lane
            .clone()
            .try_acquire_owned()
            .expect("test holds the only lane permit");
        let payload = serde_json::json!({
            "knowledge_entries": [entry_fixture("kb_b1", WORLD_A)],
        });
        match handler(&peer, "upsert", payload.clone()) {
            Err(envelope) => assert_eq!(envelope.code, "invoke_busy"),
            Ok(_) => panic!("saturated lane must reject with invoke_busy"),
        }
        drop(held);
        let served = handler(&peer, "upsert", payload).expect("lane recovers after permit frees");
        assert_eq!(served["knowledge_entries"][0]["entry_id"], "kb_b1");
    }

    /// (b) Slow invoke: a zero-budget deadline override simulates any op
    /// exceeding the budget — the acquire completes synchronously on a
    /// free permit, so the result wait deterministically exceeds the
    /// remaining budget and returns the locked `invoke_deadline_exceeded`
    /// envelope (spec §5.4).
    #[tokio::test(flavor = "multi_thread")]
    async fn slow_invoke_returns_invoke_deadline_exceeded() {
        let (handler, _lane, peer, _temp) = test_handler(BridgeLimits {
            max_concurrent_invokes: 1,
            invoke_deadline: Duration::ZERO,
            ..BridgeLimits::default()
        })
        .await;
        let payload = serde_json::json!({
            "knowledge_entries": [entry_fixture("kb_b2", WORLD_A)],
        });
        match handler(&peer, "upsert", payload) {
            Err(envelope) => assert_eq!(envelope.code, "invoke_deadline_exceeded"),
            Ok(_) => panic!("a zero-budget invoke must reject with invoke_deadline_exceeded"),
        }
    }

    /// A waiter whose permit frees mid-wait is served (the acquire wakes
    /// on release instead of waiting out the deadline): one permit, a long
    /// deadline, the test holds the permit and releases it after the
    /// invoke is queued.
    #[tokio::test(flavor = "multi_thread")]
    async fn lane_waiter_is_served_once_permit_frees() {
        let (handler, lane, peer, _temp) = test_handler(BridgeLimits {
            max_concurrent_invokes: 1,
            invoke_deadline: Duration::from_secs(5),
            ..BridgeLimits::default()
        })
        .await;
        let held = lane
            .clone()
            .try_acquire_owned()
            .expect("test holds the only lane permit");
        let handler_for_task = Arc::clone(&handler);
        let invoke = tokio::spawn(async move {
            handler_for_task(
                &peer,
                "upsert",
                serde_json::json!({
                    "knowledge_entries": [entry_fixture("kb_b3", WORLD_A)],
                }),
            )
        });
        // Give the invoke time to park in the acquire wait, then free the
        // permit; the waiter must be woken and served.
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(held);
        let served = invoke.await.expect("invoke task completes");
        let served = served.expect("waiter is served once the permit frees");
        assert_eq!(served["knowledge_entries"][0]["entry_id"], "kb_b3");
    }

    /// (c) Entry cap: 501 logical collection entries (> 500) are rejected
    /// with the locked `payload_too_large` envelope before the bridge
    /// (spec §5.4).
    #[tokio::test(flavor = "multi_thread")]
    async fn oversize_entry_count_returns_payload_too_large() {
        let (handler, _lane, peer, _temp) = test_handler(BridgeLimits::default()).await;
        let entries: Vec<Value> = (0..501)
            .map(|i| entry_fixture(&format!("kb_ov_{i}"), WORLD_A))
            .collect();
        let payload = serde_json::json!({ "knowledge_entries": entries });
        match handler(&peer, "upsert", payload) {
            Err(envelope) => assert_eq!(envelope.code, "payload_too_large"),
            Ok(_) => panic!("an over-cap payload must reject with payload_too_large"),
        }
    }

    /// (c) Byte cap: a payload serializing above 2 MiB is rejected with
    /// the locked `payload_too_large` envelope before the bridge (spec
    /// §5.4).
    #[tokio::test(flavor = "multi_thread")]
    async fn oversize_payload_bytes_returns_payload_too_large() {
        let (handler, _lane, peer, _temp) = test_handler(BridgeLimits::default()).await;
        let mut entry = entry_fixture("kb_ov_bytes", WORLD_A);
        entry["body"] = serde_json::json!({ "summary": "x".repeat(2 * 1024 * 1024 + 1024) });
        let payload = serde_json::json!({ "knowledge_entries": [entry] });
        match handler(&peer, "upsert", payload) {
            Err(envelope) => assert_eq!(envelope.code, "payload_too_large"),
            Ok(_) => panic!("an over-cap payload must reject with payload_too_large"),
        }
    }
}
