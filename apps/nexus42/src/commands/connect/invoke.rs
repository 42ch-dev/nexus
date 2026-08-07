//! N-C1 → N-C2 Connect invoke dispatch (DF-72, V1.153 P1 → V1.154 P1 T2) —
//! the architect-locked home of the session-peer `InvokeHandlerV2` closure.
//!
//! The handler is the product-owned spine of the Connect Host: it parses
//! the inbound invoke op/payload, resolves the calling peer, gates it
//! through the fail-closed `PeerScope` allowlist (T1), and routes exactly
//! `upsert` / `promote` / `relate` / `check` / `assemble` through the
//! production `NexusAdapter` orchestrators (re-exported via
//! `nexus_spoke_adapter`). Contract sources:
//! `.mstar/specs/spoke-adapter-architecture.md` §10.6 and the P1 spec
//! § OCC + error mapping / § World scoping.
//!
//! Every other op — `compute` (P2) / `project` / unknown — is refused with
//! `ErrorEnvelope.code = "op_unsupported"` and zero side effects (the N-C0
//! refusal contract extends).
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
//! ## World-scope gates (V1.153 P1 fix loop + V1.154 P1 N-C2 reads)
//!
//! Two fail-closed gates sit in front of the orchestrators, both with zero
//! side effects:
//!
//! 1. **Whole-payload world-id requirement (Important):** EVERY
//!    entry/relation must carry a parseable `extensions.nexus.world_id`
//!    (writes). Reads (`check` / `assemble`) carry the world selector on
//!    the schema's `scope.scope_id` object instead (spec §5.1 lock — no
//!    second ad-hoc world field), and the same strict rule applies: an
//!    absent scope / missing scope_id denies the WHOLE payload. If any
//!    entry lacks one the WHOLE payload is denied — the old
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
//! (whichever is reached first), plus a **2 MiB** serialized response byte
//! cap (P1 QC fix wave FW-2). The 8 permits bound the **blocking-pool
//! orchestrator work only** — the wire path is serialized on the node's
//! single event loop, where the handler parks until the lane returns, so
//! "8 concurrent" never means parallel wire processing. The deadline
//! bounds the permit acquire AND the result wait (one shared budget:
//! `invoke_busy` fires only after the full deadline, not instantly); the
//! permit stays held until the lane closure returns (the closure cannot be
//! force-cancelled safely) — a late result is discarded. Denials extend
//! the N-C1 envelope table with the locked bridge codes: `invoke_busy`
//! (lane saturated), `invoke_deadline_exceeded` (per-invoke budget
//! exhausted), `payload_too_large` (over-cap request),
//! `response_too_large` (over-cap response).
//!
//! The adapter is a **per-process singleton** constructed once at host
//! boot and held for the process lifetime (P1 spec § Process model);
//! per-invoke construction is deliberately avoided.

use super::allowlist::PeerScope;
use libp2p::PeerId;
use nexus_spoke_adapter::extensions::get_world_id;
use nexus_spoke_adapter::{
    orchestrate_assemble, orchestrate_check, orchestrate_promote, orchestrate_relate,
    orchestrate_upsert, AssembleRequest, AssembleResponse, CheckRequest, CheckResponse,
    KnowledgeEntryPort, NexusAdapter, PromoteRequest, PromoteResponse, RelateRequest,
    RelateResponse, Relation, RelationExtensionsKey, RelationPort, SpokeReject, SpokeRejectCode,
    SpokeResult, UpsertRequest, UpsertResponse,
};
use serde_json::{Map, Value};
use spoke_connect::InvokeHandlerV2;
use spoke_schemas::connect::connect_invoke_response::ErrorEnvelope;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// The ops this host serves (N-C1 writes → N-C2 read half).
///
/// This const is load-bearing, not declaration-only: [`dispatch`] gates on
/// it before routing, so the host can never serve an op it does not list.
/// The manifest-honesty test
/// (`n_c1_manifest_served_ops_match_dispatch_both_directions` in
/// `commands::connect::interop`) machine-checks this set ⇔ the manifest's
/// advertised `extensions.nexus.served_ops` (`nexus_spoke_adapter`'s
/// `LOCAL_SERVED_OPS`) in both directions — so the manifest ⇔ actual
/// dispatch routing lockstep holds by construction.
pub const SERVED_OPS: [&str; 5] = ["upsert", "promote", "relate", "check", "assemble"];

/// Architect-locked bounded-bridge limits (spec §5.4).
///
/// Defaults are the locked numbers — **8** concurrent invokes per process,
/// a **30,000 ms** per-invoke deadline, **500** logical collection entries
/// or **2 MiB** serialized request bytes, and a **2 MiB** serialized
/// response byte cap (P1 QC fix wave FW-2), whichever cap is reached
/// first. The 8 permits bound blocking-pool orchestrator work only; the
/// wire path stays serialized on the single event loop. Tests construct
/// explicit values (1 permit / short deadline / tiny caps) to exercise
/// saturation, deadline, and cap paths deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeLimits {
    /// Max concurrent in-flight invokes per process (semaphore permits) —
    /// bounds BLOCKING-pool orchestrator work, not wire processing (the
    /// event loop parks per invoke).
    pub max_concurrent_invokes: usize,
    /// Per-invoke deadline — ONE shared budget for the permit acquire AND
    /// the result wait (a saturated-lane wait consumes the same budget, so
    /// `invoke_busy` fires only after the full deadline).
    pub invoke_deadline: std::time::Duration,
    /// Max logical collection entries per payload (the operation's batch
    /// arrays).
    pub max_collection_entries: usize,
    /// Max serialized request bytes per payload.
    pub max_payload_bytes: usize,
    /// Max serialized response bytes per invoke — measured after the
    /// orchestrator returns, before the invoke result is returned (P1 QC
    /// fix wave FW-2; mirrors the request-side 2 MiB).
    pub max_response_bytes: usize,
}

impl Default for BridgeLimits {
    fn default() -> Self {
        Self {
            max_concurrent_invokes: 8,
            invoke_deadline: std::time::Duration::from_secs(30),
            max_collection_entries: 500,
            max_payload_bytes: 2 * 1024 * 1024,
            max_response_bytes: 2 * 1024 * 1024,
        }
    }
}

/// Build the N-C2 read-half `InvokeHandlerV2` on the architect-locked
/// bridge limits.
///
/// The dispatch pipeline (spec §5.4 — [`BridgeLimits::default`]) is a
/// fail-closed op gate + allowlist world/op scope gate in front of the
/// five `NexusAdapter` orchestrator routes (`upsert` / `promote` / `relate`
/// / `check` / `assemble`), resolving caller identity from the **session
/// peer** (spoke-connect 0.9.2 session-peer hook, spec §3.2 / §5.1).
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
/// **Test seam only** (P1 QC fix wave FW-10): `pub(crate)` because every
/// caller lives in this crate — production wiring goes through
/// [`build_handler`], whose `BridgeLimits::default()` are the
/// architect-locked numbers; this entrypoint exists so tests can inject
/// 1-permit lanes, short deadlines, and tiny caps deterministically. Also
/// returns the process-wide lane so tests can observe the same semaphore
/// the handler acquires — e.g. to hold permits and force a saturation.
/// Do not ship non-default caps through this function from production
/// callers without an architect lock.
#[must_use]
pub(crate) fn build_handler_with_limits(
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

/// One served op.
#[derive(Debug, Clone, Copy)]
enum Route {
    Upsert,
    Promote,
    Relate,
    Check,
    Assemble,
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
    // 1. Served-op gate (N-C1 → N-C2): `SERVED_OPS` is the load-bearing
    //    serving gate — the manifest-honesty machine check
    //    (`n_c1_manifest_served_ops_match_dispatch_both_directions`)
    //    verifies the manifest against this const, so dispatch MUST read it
    //    too, or the check would bind to a declaration-only table. Anything
    //    outside the const (incl. `compute` — P2, and `project`) is refused
    //    unconditionally, regardless of payload shape (N-C0 refusal
    //    contract extends); a match arm for an op that is not in
    //    `SERVED_OPS` is unreachable by construction.
    if !SERVED_OPS.contains(&op) {
        return Err(unsupported(
            op,
            "this host serves only upsert / promote / relate / check / assemble",
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
        "check" => Route::Check,
        "assemble" => Route::Assemble,
        _ => {
            return Err(unsupported(
                op,
                "this host serves only upsert / promote / relate / check / assemble",
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

    // 5. World-scope gate (T1 PeerScope, fail-closed; spec §5.5 — reads
    //    scoped exactly like writes): every target world in the payload
    //    must be in the peer's `world_scope`. Strictness (fix loop,
    //    Important): EVERY entry/relation must carry a parseable
    //    `extensions.nexus.world_id` (writes) / `scope.scope_id` (reads) —
    //    a payload where any carrier is missing denies the WHOLE payload
    //    (no filter-and-continue; that shape let a mixed payload pass the
    //    gate and fail later as a partial write).
    let Some(worlds) = payload_world_ids(route, &payload) else {
        return Err(denied(
            "invoke payload carries no verifiable world scope \
             (writes need extensions.nexus.world_id on every entry/relation; \
             check/assemble need scope.scope_id); cannot verify world scope",
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
        // Response-cap gate (spec §5.4 — P1 QC fix wave FW-2): the
        // orchestrator result is serialized and measured AFTER the lane
        // returns and BEFORE the invoke result is handed back to the
        // peer. An over-cap response maps to the locked
        // `response_too_large` envelope instead of surfacing as a hard
        // codec failure on the peer (the reference peer's inbound-response
        // codec cap is 10 MiB — this cap fails gracefully well under it).
        Ok(result) => enforce_response_cap(result, limits.max_response_bytes),
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
        // N-C2 read half (spec §5.1 lock): the Connect payload deserializes
        // DIRECTLY into the spoke wire types; the request scope is the
        // schema's `scope` object (world-scoped by the step-5 gate).
        // `run_checker` is the production baseline no-op evaluator — the
        // V1.148 daemon-route cutover shape (zero findings; rules still
        // resolve via `RuleQueryPort` inside the orchestrator).
        Route::Check => {
            let request: CheckRequest = match serde_json::from_value(payload) {
                Ok(request) => request,
                Err(error) => return Err(map_reject(&invalid_payload("check", &error))),
            };
            match orchestrate_check(adapter, request, |_input| SpokeResult::Ok(vec![])).await {
                SpokeResult::Ok(response) => serialize_response::<CheckResponse>(&response),
                SpokeResult::Reject(reject) => Err(map_reject(&reject)),
            }
        }
        Route::Assemble => {
            let request: AssembleRequest = match serde_json::from_value(payload) {
                Ok(request) => request,
                Err(error) => return Err(map_reject(&invalid_payload("assemble", &error))),
            };
            match orchestrate_assemble(adapter, request).await {
                SpokeResult::Ok(response) => serialize_response::<AssembleResponse>(&response),
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
/// effects). Writes carry `extensions.nexus.world_id` (the canonical carrier
/// the conversion seam writes on entries/relations); reads (`check` /
/// `assemble`) carry the world selector on the schema's `scope.scope_id`
/// (spec §5.1 lock — the scope object, not a second ad-hoc world field).
///
/// Strictness (fix loop, Important): EVERY entry/relation must carry a
/// parseable world id (writes) / `scope.scope_id` must be present (reads).
/// `None` means the payload cannot be scoped as a whole — a carrier is
/// missing (or the container is absent) — and the whole payload is denied.
/// Entries are never filtered out of the set.
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
        Route::Check | Route::Assemble => {
            let scope_id = payload.pointer("/scope/scope_id")?.as_str()?;
            Some(vec![scope_id.to_string()])
        }
    }
}

/// Logical collection-entry count for the payload cap (spec §5.4): the
/// operation's collection fields. `upsert` counts `knowledge_entries`;
/// `promote` / `relate` carry a single candidate / relation (1 when
/// present); `check` counts its batch arrays (`rule_refs` + `rules` +
/// `checker_kinds`); `assemble` counts `max_entries` — the entries the
/// peer asks the packet to carry, so an oversized hint is rejected before
/// the orchestrator (prevents assembled context amplification). The
/// orchestrator's typed parse rejects malformed payloads after the cap
/// gate.
///
/// The scope-object batch arrays (`scope.entry_ids` /
/// `scope.entry_types` / `scope.timeline_event_ids`) count for BOTH
/// `check` and `assemble` (P1 QC fix wave FW-1): the spoke Scope schema
/// places no `maxItems` on them and the adapter consumes them as IN-list
/// filters (`ScopeQueryPort::list_knowledge_entries` /
/// `list_timeline_events`), so a payload could otherwise carry far more
/// than 500 logical collection entries under the byte cap.
fn payload_collection_entries(route: Route, payload: &Value) -> usize {
    // Length of one scope-object batch array — raw JSON reads so the cap
    // gate runs before any typed parse.
    let scope_array_len = |field: &str| {
        payload
            .pointer(&format!("/scope/{field}"))
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    };
    match route {
        Route::Upsert => payload
            .get("knowledge_entries")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        Route::Promote => usize::from(payload.get("candidate").is_some()),
        Route::Relate => usize::from(payload.get("relation").is_some()),
        Route::Check => {
            let array_len = |field: &str| {
                payload
                    .get(field)
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len)
            };
            array_len("rule_refs")
                + array_len("rules")
                + array_len("checker_kinds")
                + scope_array_len("entry_ids")
                + scope_array_len("entry_types")
                + scope_array_len("timeline_event_ids")
        }
        Route::Assemble => {
            payload
                .get("max_entries")
                .and_then(Value::as_u64)
                .map_or(0, |max| usize::try_from(max).unwrap_or(usize::MAX))
                + scope_array_len("entry_ids")
                + scope_array_len("entry_types")
                + scope_array_len("timeline_event_ids")
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
        // Reads (N-C2): no stored row is targeted by id — `check` /
        // `assemble` reach storage only through the orchestrators' own
        // world-scoped `ScopeQueryPort` reads, and the world gate (step 5)
        // already verified `scope.scope_id` against the peer's
        // `world_scope`, so no stored-world verification applies (spec
        // §5.5 — fail-closed world scoping happens before the
        // orchestrator, zero side effects on denial).
        Route::Check | Route::Assemble => {}
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

/// Response-cap denial (spec §5.4 — P1 QC fix wave FW-2): the orchestrator
/// produced a result above the locked response byte cap. Retry-safe for
/// peers that can narrow their request (e.g. fewer `max_entries`).
fn response_too_large(reason: &str) -> ErrorEnvelope {
    ErrorEnvelope {
        code: "response_too_large".to_string(),
        message: format!("invoke response rejected: {reason}"),
        details: Map::new(),
        extensions: HashMap::default(),
    }
}

/// Response-cap gate (spec §5.4 — P1 QC fix wave FW-2): serialize the
/// orchestrator success value and measure it; over the locked cap, return
/// the `response_too_large` envelope instead of the value. Applied at the
/// bridge boundary — after the orchestrator returns, before the invoke
/// returns the `Value` — so an amplified response fails as a graceful
/// envelope, never as a hard peer codec failure. Rejects pass through
/// untouched.
fn enforce_response_cap(
    result: Result<Value, ErrorEnvelope>,
    max_response_bytes: usize,
) -> Result<Value, ErrorEnvelope> {
    let value = result?;
    let bytes = serde_json::to_vec(&value)
        .expect("a serde_json::Value always serializes")
        .len();
    if bytes > max_response_bytes {
        return Err(response_too_large(&format!(
            "orchestrator response serializes to {bytes} bytes; the cap is {max_response_bytes}"
        )));
    }
    Ok(value)
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
/// mapping — verbatim; extended for the N-C2 read half per the V1.148
/// daemon-route precedent, where the check handler maps spoke client-input
/// rejects to the 400 class):
///
/// | `SpokeRejectCode` | `ErrorEnvelope.code` | Retry-safe? |
/// |-------------------|----------------------|-------------|
/// | `KnowledgeEntryAlreadyExists` | `knowledge_entry_already_exists` | yes |
/// | `StoredRevisionStale` | `stored_revision_stale` | yes |
/// | `RevisionConflict` | `revision_conflict` | yes |
/// | `InvalidInput` | `invalid_input` | yes (client fixes payload) |
/// | `InvalidPacketInput` | `invalid_input` | yes (client fixes payload) |
/// | `InternalError` | `internal_error` | no |
/// | any other reject | `internal_error` (carries `reject.code`/`message` in `details`) | no |
///
/// `InvalidInput` / `InvalidPacketInput` are the check/assemble path's
/// client-input rejects (scope wire conversion, packet extensions
/// namespace, malformed payloads) — the daemon's 400 class, so they must
/// not read as server faults. `reject.message` flows into
/// `ErrorEnvelope.message`; `reject.details` (when present) flows into
/// `ErrorEnvelope.details`.
fn map_reject(reject: &SpokeReject) -> ErrorEnvelope {
    let code = match reject.code {
        SpokeRejectCode::KnowledgeEntryAlreadyExists => "knowledge_entry_already_exists",
        SpokeRejectCode::StoredRevisionStale => "stored_revision_stale",
        SpokeRejectCode::RevisionConflict => "revision_conflict",
        // Client-input family (V1.148 daemon check mapping precedent —
        // spoke InvalidInput → 400 class; the Connect envelope equivalent).
        SpokeRejectCode::InvalidInput | SpokeRejectCode::InvalidPacketInput => "invalid_input",
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

    /// A `PeerScope` allowlisting `peer` for `WORLD_A` with the given ops,
    /// written through the on-disk allowlist shape (like the CLI boot).
    fn scoped_scope_for(peer: PeerId, ops: &[&str]) -> PeerScope {
        let temp = tempfile::tempdir().expect("tempdir");
        let allow_path = connect_allowlist_path(temp.path());
        std::fs::create_dir_all(allow_path.parent().expect("parent dir")).expect("mkdir");
        std::fs::write(
            &allow_path,
            serde_json::json!({ "peer_ids": [{
                "peer_id": peer.to_string(),
                "world_scope": [WORLD_A],
                "op_scope": ops,
            }] })
            .to_string(),
        )
        .expect("write allowlist");
        crate::commands::connect::allowlist::load(temp.path(), &[]).expect("scoped allowlist loads")
    }

    /// A `PeerScope` allowlisting `peer` for `WORLD_A` with the upsert op.
    fn scoped_scope(peer: PeerId) -> PeerScope {
        scoped_scope_for(peer, &["upsert"])
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

    /// N-C2 (T1 Minor follow-up, spec §5.4): the entry cap extends to the
    /// check op's collection fields — `rule_refs` + `rules` +
    /// `checker_kinds` all count as logical collection entries. 501
    /// rule_refs (> 500) are rejected with `payload_too_large` before the
    /// bridge.
    #[tokio::test(flavor = "multi_thread")]
    async fn check_payload_entry_cap_counts_collection_fields() {
        let peer = fixed_keypair(8).public().to_peer_id();
        let scope = scoped_scope_for(peer, &["check"]);
        let (_temp, adapter) = test_adapter().await;
        let (handler, _lane) = build_handler_with_limits(scope, adapter, BridgeLimits::default());
        let payload = serde_json::json!({
            "scope": { "scope_id": WORLD_A },
            "rule_refs": (0..501).map(|i| format!("rule_{i}")).collect::<Vec<_>>(),
        });
        match handler(&peer, "check", payload) {
            Err(envelope) => assert_eq!(
                envelope.code, "payload_too_large",
                "501 rule_refs must trip the check entry cap"
            ),
            Ok(_) => panic!("an over-cap check payload must reject with payload_too_large"),
        }
    }

    /// N-C2 (T1 Minor follow-up, spec §5.4): the entry cap extends to the
    /// assemble op's collection field — `max_entries` counts as the
    /// logical entries the peer asks the packet to carry. 501 (> 500) is
    /// rejected with `payload_too_large` before the bridge (prevents
    /// assembled context amplification).
    #[tokio::test(flavor = "multi_thread")]
    async fn assemble_payload_entry_cap_counts_max_entries() {
        let peer = fixed_keypair(9).public().to_peer_id();
        let scope = scoped_scope_for(peer, &["assemble"]);
        let (_temp, adapter) = test_adapter().await;
        let (handler, _lane) = build_handler_with_limits(scope, adapter, BridgeLimits::default());
        let payload = serde_json::json!({
            "scope": { "scope_id": WORLD_A },
            "max_entries": 501,
        });
        match handler(&peer, "assemble", payload) {
            Err(envelope) => assert_eq!(
                envelope.code, "payload_too_large",
                "max_entries=501 must trip the assemble entry cap"
            ),
            Ok(_) => panic!("an over-cap assemble payload must reject with payload_too_large"),
        }
    }

    /// P1 QC fix wave (FW-1, spec §5.4): the check entry cap extends to the
    /// scope-object batch arrays — `scope.entry_ids` / `scope.entry_types`
    /// / `scope.timeline_event_ids` all count as logical collection
    /// entries (the spoke Scope schema places no `maxItems` on them and
    /// the adapter consumes them as IN-list filters). 501 entry_ids
    /// (> 500) are rejected with `payload_too_large` before the bridge —
    /// zero side effects, no orchestrator call.
    #[tokio::test(flavor = "multi_thread")]
    async fn check_payload_entry_cap_counts_scope_batch_arrays() {
        let peer = fixed_keypair(11).public().to_peer_id();
        let scope = scoped_scope_for(peer, &["check"]);
        let (_temp, adapter) = test_adapter().await;
        let (handler, _lane) = build_handler_with_limits(scope, adapter, BridgeLimits::default());
        let payload = serde_json::json!({
            "scope": {
                "scope_id": WORLD_A,
                "entry_ids": (0..501).map(|i| format!("kb_scope_{i}")).collect::<Vec<_>>(),
            },
        });
        match handler(&peer, "check", payload) {
            Err(envelope) => assert_eq!(
                envelope.code, "payload_too_large",
                "501 scope.entry_ids must trip the check entry cap"
            ),
            Ok(_) => panic!("an over-cap check payload must reject with payload_too_large"),
        }
    }

    /// P1 QC fix wave (FW-1, spec §5.4): the assemble entry cap counts the
    /// scope-object batch arrays the same way — 501 `scope.timeline_event_ids`
    /// (> 500) are rejected with `payload_too_large` before the bridge.
    #[tokio::test(flavor = "multi_thread")]
    async fn assemble_payload_entry_cap_counts_scope_batch_arrays() {
        let peer = fixed_keypair(12).public().to_peer_id();
        let scope = scoped_scope_for(peer, &["assemble"]);
        let (_temp, adapter) = test_adapter().await;
        let (handler, _lane) = build_handler_with_limits(scope, adapter, BridgeLimits::default());
        let payload = serde_json::json!({
            "scope": {
                "scope_id": WORLD_A,
                "timeline_event_ids": (0..501).map(|i| format!("ev_scope_{i}")).collect::<Vec<_>>(),
            },
        });
        match handler(&peer, "assemble", payload) {
            Err(envelope) => assert_eq!(
                envelope.code, "payload_too_large",
                "501 scope.timeline_event_ids must trip the assemble entry cap"
            ),
            Ok(_) => panic!("an over-cap assemble payload must reject with payload_too_large"),
        }
    }

    /// P1 QC fix wave (FW-2, spec §5.4): the orchestrator result is
    /// serialized and measured at the bridge boundary — a response above
    /// the locked byte cap maps to the `response_too_large` envelope, not
    /// a hard peer codec failure. The tiny-cap override stands in for a
    /// fake large check response deterministically (any real check
    /// response serializes well over 1 byte).
    #[tokio::test(flavor = "multi_thread")]
    async fn oversize_response_returns_response_too_large() {
        let peer = fixed_keypair(13).public().to_peer_id();
        let scope = scoped_scope_for(peer, &["check"]);
        let (_temp, adapter) = test_adapter().await;
        let (handler, _lane) = build_handler_with_limits(
            scope,
            adapter,
            BridgeLimits {
                max_response_bytes: 1,
                ..BridgeLimits::default()
            },
        );
        let payload = serde_json::json!({ "scope": { "scope_id": WORLD_A } });
        match handler(&peer, "check", payload) {
            Err(envelope) => assert_eq!(
                envelope.code, "response_too_large",
                "an over-cap response must reject with response_too_large"
            ),
            Ok(_) => panic!("an over-cap response must reject with response_too_large"),
        }
    }

    /// N-C2 mapping extension (V1.148 daemon precedent — spoke client-input
    /// rejects are the 400 class, not server faults): a payload that passes
    /// the world gate but fails the typed orchestrator parse maps through
    /// the synthetic `InvalidInput` reject to the `invalid_input` envelope
    /// (retry-safe), not `internal_error`.
    #[tokio::test(flavor = "multi_thread")]
    async fn malformed_payload_maps_to_invalid_input() {
        let peer = fixed_keypair(10).public().to_peer_id();
        let scope = scoped_scope_for(peer, &["upsert"]);
        let (_temp, adapter) = test_adapter().await;
        let (handler, _lane) = build_handler_with_limits(scope, adapter, BridgeLimits::default());
        // The entry carries the world_id carrier (world gate passes) but
        // misses the typed wire's required fields (parse rejects).
        let payload = serde_json::json!({
            "knowledge_entries": [{
                "extensions": { "nexus": { "world_id": WORLD_A } },
            }],
        });
        match handler(&peer, "upsert", payload) {
            Err(envelope) => assert_eq!(
                envelope.code, "invalid_input",
                "a malformed payload must map to the invalid_input envelope"
            ),
            Ok(_) => panic!("a malformed payload must reject with invalid_input"),
        }
    }
}
