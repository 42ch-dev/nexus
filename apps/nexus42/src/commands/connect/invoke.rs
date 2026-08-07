//! N-C1 → N-C2 Connect invoke dispatch (DF-72, V1.153 P1 → V1.154 P1 T2 →
//! P2 T1) — the architect-locked home of the session-peer `InvokeHandlerV2`
//! closure.
//!
//! The handler is the product-owned spine of the Connect Host: it parses
//! the inbound invoke op/payload, resolves the calling peer, gates it
//! through the fail-closed `PeerScope` allowlist (T1), and routes exactly
//! `upsert` / `promote` / `relate` / `check` / `assemble` / `compute`
//! through the production `NexusAdapter` orchestrators (re-exported via
//! `nexus_spoke_adapter`). Contract sources:
//! `.mstar/specs/spoke-adapter-architecture.md` §10.6 and the P1 spec
//! § OCC + error mapping / § World scoping, plus the P2 spec
//! §2 compute-over-Connect.
//!
//! Every other op — `project` / unknown — is refused with
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
//! identity the `world_scope` / `op_scope` / `module_scope` gates resolve
//! against.
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
//! The `compute` op is the exception: its world is resolved from the
//! **stored entry's** `extensions.nexus.world_id` inside the lane (spec
//! §2.2 — the `ComputeRequest` wire has no world carrier).
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
//!    absent scope / missing `scope_id` denies the WHOLE payload. If any
//!    entry lacks one the WHOLE payload is denied — the old
//!    filter-and-continue shape let a mixed payload pass the gate and fail
//!    later in the adapter as a partial-batch write surfaced as
//!    `internal_error`.
//! 2. **Stored-world check (Critical, defense-in-depth):** every targeted
//!    existing row's stored `world_id` is verified against the
//!    payload-claimed `world_id` before the orchestrator CAS runs; a
//!    mismatch denies the op with zero side effects. This gate is the
//!    **fast-fail layer** — since V1.154 P2 (R3 closure, spec §3) the
//!    orchestrator/storage CAS is itself world-aware (`world_id` joins the
//!    per-table CAS predicates), so even a check-then-act race between this
//!    gate and the CAS (two processes on the same workspace DB) is denied
//!    atomically at the storage layer with the `world_conflict` wire code.
//!    The relate **create** path additionally verifies its
//!    endpoints (plan QC, QC1 F-001 / QC2 W-2): `from_id` / `to_id` must
//!    exist and their stored worlds must equal the claimed world —
//!    `kb_relationships` FKs are single-column on `key_block_id`, so a
//!    world-A relation row could otherwise legally reference world-B
//!    entries (cross-world edge + id-existence oracle).
//!
//! ## Compute gates (V1.154 P2 — spec §2)
//!
//! The `compute` route is world-scoped like every other op, but the world
//! is the **stored entry's** `extensions.nexus.world_id` (spec §2.2 — no
//! wire carrier), so its gates run inside the bounded lane before any WASM
//! execution:
//!
//! 1. **Read-only lock (spec §5 / §6.5):** `settle: true` is rejected with
//!    the defined `settle_not_enabled` envelope — the N-C2 compute
//!    settlement helper is NOT enabled.
//! 2. **Stored-world gate:** the target entry's stored world must be in the
//!    peer's `world_scope` (`op_unsupported` family otherwise, like every
//!    other op).
//! 3. **Module identity (locked precedence, spec §2.2):** session state
//!    `module_id`, then entry `body.computable.module_id` — neither ⇒
//!    defined `module_not_found` (missing module name).
//! 4. **Module-scope gate (architect lock, spec §6.1):** the resolved
//!    module must be in the peer's `module_scope`; missing/empty scope
//!    denies ALL compute with the defined `module_not_scoped` envelope.
//! 5. **Host-local store gate (spec §2.1):** the module must be installed
//!    under `~/.nexus42/modules/` (never peer-supplied bytes) ⇒ defined
//!    `module_not_found` otherwise.
//!
//! Execution routes through `orchestrate_compute` → `ComputablePort::compute`
//! with the envelope locked to `spoke_schemas::ComputeRequest` /
//! `ComputeResponse` (spec §2.2 — the V1.147 `RunRequest` / `RunResponse`
//! HTTP pair is the reference mapping only, never a third wrapper).
//!
//! ## Bounded async bridge (R2 closure, V1.154 P1; compute lane P2)
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
//! Compute additionally takes a per-process **`compute_serializer`**
//! (`Semaphore(1)` — spec §2.4): the wasmtime epoch watchdog is
//! engine-global, so only one WASM invocation may run at a time inside the
//! shared lane (mirrors the daemon's `compute_runs.rs` W-2 permit). The
//! serializer is acquired inside the lane closure, so the per-invoke
//! deadline bounds the wait and no second thread pool exists.
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
//! `response_too_large` (over-cap response), plus the P2 compute codes
//! `module_not_found` / `module_not_scoped` / `settle_not_enabled`.
//!
//! The adapter is a **per-process singleton** constructed once at host
//! boot and held for the process lifetime (P1 spec § Process model);
//! per-invoke construction is deliberately avoided.

use super::allowlist::PeerScope;
use libp2p::PeerId;
use nexus_spoke_adapter::extensions::get_world_id;
use nexus_spoke_adapter::{
    is_world_conflict_reject, orchestrate_assemble, orchestrate_check, orchestrate_compute,
    orchestrate_promote, orchestrate_relate, orchestrate_upsert, AssembleRequest, AssembleResponse,
    CheckRequest, CheckResponse, ComputeRequest, ComputeResponse, KnowledgeEntryPort, NexusAdapter,
    PromoteRequest, PromoteResponse, RelateRequest, RelateResponse, Relation,
    RelationExtensionsKey, RelationPort, SpokeReject, SpokeRejectCode, SpokeResult, UpsertRequest,
    UpsertResponse,
};
use serde_json::{Map, Value};
use spoke_connect::InvokeHandlerV2;
use spoke_schemas::connect::connect_invoke_response::ErrorEnvelope;
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// The ops this host serves (N-C1 writes → N-C2 read half → P2 compute).
///
/// This const is load-bearing, not declaration-only: [`dispatch`] gates on
/// it before routing, so the host can never serve an op it does not list.
/// The manifest-honesty test
/// (`n_c1_manifest_served_ops_match_dispatch_both_directions` in
/// `commands::connect::interop`) machine-checks this set ⇔ the manifest's
/// advertised `extensions.nexus.served_ops` (`nexus_spoke_adapter`'s
/// `LOCAL_SERVED_OPS`) in both directions — so the manifest ⇔ actual
/// dispatch routing lockstep holds by construction.
pub const SERVED_OPS: [&str; 6] = [
    "upsert", "promote", "relate", "check", "assemble", "compute",
];

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

/// Build the N-C2 E2 `InvokeHandlerV2` on the architect-locked bridge
/// limits.
///
/// The dispatch pipeline (spec §5.4 — [`BridgeLimits::default`]) is a
/// fail-closed op gate + allowlist world/op/module scope gates in front of
/// the six `NexusAdapter` orchestrator routes (`upsert` / `promote` /
/// `relate` / `check` / `assemble` / `compute`), resolving caller identity
/// from the **session peer** (spoke-connect 0.9.2 session-peer hook, spec
/// §3.2 / §5.1). Compute additionally serializes WASM execution through a
/// per-process `Semaphore(1)` inside the shared lane (spec §2.4 — the
/// engine-global epoch watchdog allows one invocation at a time).
///
/// The returned closure is `Send + Sync` (the node holds it in an
/// `Arc<InvokeHandlerV2>`); `scope`, `adapter`, the bounded lane, and the
/// compute serializer are captured for the process lifetime — one adapter,
/// one lane, one serializer per Connect process (P1 spec § Process model).
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
    // P2 compute serializer (spec §2.4): one WASM invocation at a time —
    // the wasmtime epoch watchdog is engine-global, so concurrent compute
    // calls would trap each other at the shortest budget (the daemon's
    // compute_runs.rs W-2 permit is the same shape).
    let compute_serializer = Arc::new(Semaphore::new(1));
    let handler = Arc::new(move |peer: &PeerId, op: &str, payload: Value| {
        dispatch(
            &scope,
            Arc::clone(&adapter),
            &lane_for_handler,
            &compute_serializer,
            limits,
            peer,
            op,
            payload,
        )
    });
    (handler, lane)
}

/// One served op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    Upsert,
    Promote,
    Relate,
    Check,
    Assemble,
    Compute,
}

/// The full dispatch pipeline. Every gate is fail-closed and runs before
/// any orchestrator call, so denials have zero side effects.
///
/// The argument list is the architect-locked pipeline context (scope,
/// adapter, lane, compute serializer, limits, caller, op, payload);
/// bundling it would obscure the explicit fail-closed ordering.
#[allow(clippy::too_many_arguments)]
fn dispatch(
    scope: &PeerScope,
    adapter: Arc<NexusAdapter<'static>>,
    lane: &Arc<Semaphore>,
    compute_serializer: &Arc<Semaphore>,
    limits: BridgeLimits,
    peer: &PeerId,
    op: &str,
    payload: Value,
) -> Result<Value, ErrorEnvelope> {
    // 1. Served-op gate (N-C1 → N-C2 E2): `SERVED_OPS` is the load-bearing
    //    serving gate — the manifest-honesty machine check
    //    (`n_c1_manifest_served_ops_match_dispatch_both_directions`)
    //    verifies the manifest against this const, so dispatch MUST read it
    //    too, or the check would bind to a declaration-only table. Anything
    //    outside the const (incl. `project`) is refused unconditionally,
    //    regardless of payload shape (N-C0 refusal contract extends); a
    //    match arm for an op that is not in `SERVED_OPS` is unreachable by
    //    construction.
    if !SERVED_OPS.contains(&op) {
        return Err(unsupported(
            op,
            "this host serves only upsert / promote / relate / check / assemble / compute",
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
        "compute" => Route::Compute,
        _ => {
            return Err(unsupported(
                op,
                "this host serves only upsert / promote / relate / check / assemble / compute",
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
    //
    //    `compute` is the exception (P2, spec §2.2): the ComputeRequest
    //    wire has no world carrier — the world is the **stored entry's**
    //    `extensions.nexus.world_id`, resolved inside the lane by
    //    [`verify_compute_gates`] with the same fail-closed rule.
    if route != Route::Compute {
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
        if let Some(world) = worlds.iter().find(|world| !scope.allows_world(peer, world)) {
            return Err(denied(&format!(
                "world {world} is not in this peer's world_scope"
            )));
        }
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
    // 8. Stored-world gate (fix loop, Critical — defense-in-depth), inside
    //    the lane closure before the orchestrator: every targeted existing
    //    row's stored world_id is verified against the payload-claimed
    //    world_id; a mismatch denies with zero side effects. The gate is the
    //    fast-fail layer — since V1.154 P2 (R3 closure, spec §3) the
    //    orchestrator/storage CAS is itself world-aware (`world_id` joins
    //    the per-table CAS predicates), so a check-then-act race between
    //    this gate and the CAS (a second process moved the row to another
    //    world) is denied atomically at the storage layer with the
    //    `world_conflict` wire code. Compute runs its own gate set instead
    //    ([`verify_compute_gates`] — stored world + module identity +
    //    module_scope + host-local store + the read-only settle lock, spec
    //    §2.1–§2.3), all inside the lane, before any WASM execution.
    let deadline = std::time::Instant::now() + limits.invoke_deadline;
    let permit = acquire_permit(lane, deadline)?;
    let (tx, rx) = std::sync::mpsc::channel();
    let scope_for_lane = scope.clone();
    let adapter_for_lane = Arc::clone(&adapter);
    let serializer_for_lane = Arc::clone(compute_serializer);
    let peer_id = *peer;
    tokio::task::spawn_blocking(move || {
        // Permit semantics (spec §5.4): the lane closure cannot be
        // force-cancelled safely, so the permit moves in here and stays
        // held until the closure returns — the deadline only bounds the
        // caller's wait, and any late result is discarded below.
        let _permit = permit;
        let result = tokio::runtime::Handle::current().block_on(async {
            if route == Route::Compute {
                verify_compute_gates(&scope_for_lane, &adapter_for_lane, &peer_id, &payload)
                    .await?;
            } else {
                verify_stored_worlds(&adapter_for_lane, route, &payload).await?;
            }
            route_orchestrator(route, &adapter_for_lane, &serializer_for_lane, payload).await
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
///
/// The compute route additionally holds the per-process compute serializer
/// (spec §2.4 — one WASM invocation at a time; the engine-global epoch
/// watchdog would otherwise trap concurrent calls at the shortest budget).
/// The acquire runs inside the lane closure, so the per-invoke deadline
/// bounds it like every other lane wait.
async fn route_orchestrator(
    route: Route,
    adapter: &NexusAdapter<'static>,
    compute_serializer: &Arc<Semaphore>,
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
        // N-C2 E2 (P2, spec §2): the envelope is locked to the spoke
        // `ComputeRequest` / `ComputeResponse` (no third wrapper; the
        // V1.147 generated `RunRequest` / `RunResponse` HTTP pair is the
        // reference mapping only). The read-only lock, world gate, module
        // identity, module_scope, and host-local store gates all ran in
        // [`verify_compute_gates`] before this point — zero WASM execution
        // on denial. The serializer permit is held across the whole
        // orchestration (one compute invocation at a time, spec §2.4).
        Route::Compute => {
            let request: ComputeRequest = match serde_json::from_value(payload) {
                Ok(request) => request,
                Err(error) => return Err(map_reject(&invalid_payload("compute", &error))),
            };
            let serializer = Arc::clone(compute_serializer);
            let _permit = serializer.acquire_owned().await.map_err(|_| {
                bridge_fault("compute serializer closed before the WASM invocation")
            })?;
            match orchestrate_compute(adapter, request).await {
                SpokeResult::Ok(response) => serialize_response::<ComputeResponse>(&response),
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
        // Compute carries no world on the wire (P2, spec §2.2): the world is
        // the stored entry's `extensions.nexus.world_id`, resolved inside
        // the lane by verify_compute_gates. Dispatch skips the raw world
        // gate for this route; this arm is unreachable and exists only for
        // match exhaustiveness.
        Route::Compute => None,
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
        // Compute (P2): the dynamic `computable` map is the request's batch
        // surface (the session state is staged out-of-band); its key count
        // bounds the payload's logical entry amplification. The response
        // cap additionally bounds the merged-state output.
        Route::Compute => payload
            .get("computable")
            .and_then(Value::as_object)
            .map_or(0, Map::len),
    }
}

/// Verify the stored-world invariant before any orchestrator CAS (fix loop,
/// Critical — defense-in-depth): for every payload entry/relation that
/// already exists in storage, the stored row's `world_id` must equal the
/// payload-claimed `world_id`. Since V1.154 P2 (R3 closure, spec §3) the
/// orchestrator/storage CAS is itself world-aware (`world_id` joins the
/// per-table CAS predicates), so this gate is the fast-fail layer: a
/// mismatch is denied here with zero side effects, and even a
/// check-then-act race between this gate and the CAS (a second process
/// moved the row to another world) is denied atomically at the storage
/// layer with the `world_conflict` wire code instead of a cross-world
/// rewrite.
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
        // Reads (N-C2) and compute (P2): no stored row is targeted by id —
        // `check` / `assemble` reach storage only through the
        // orchestrators' own world-scoped `ScopeQueryPort` reads (world
        // gate verified `scope.scope_id` at step 5), and `compute` runs its
        // own gate set ([`verify_compute_gates`] — stored-world + module
        // gates, spec §2).
        Route::Check | Route::Assemble | Route::Compute => {}
    }
    Ok(())
}

/// The P2 compute gate set (spec §2.1–§2.3, architect locks). Every gate
/// runs inside the bounded lane BEFORE any WASM execution, fail-closed, zero
/// side effects:
///
/// 1. **Read-only lock** (spec §5 / §6.5): `settle: true` ⇒ defined
///    `settle_not_enabled` — the N-C2 compute settlement helper is NOT
///    enabled.
/// 2. **Stored-world gate**: the target entry's stored
///    `extensions.nexus.world_id` must be in the peer's `world_scope`
///    (`op_unsupported` family — same fail-closed rule as every other op).
/// 3. **Module identity** (locked precedence, spec §2.2): session state
///    `module_id`, then entry `body.computable.module_id`; neither ⇒
///    defined `module_not_found` (missing module name).
/// 4. **Module-scope gate** (architect lock, spec §6.1): the resolved
///    module must be in the peer's `module_scope`; missing/empty scope
///    denies ALL compute ⇒ defined `module_not_scoped`.
/// 5. **Host-local store gate** (spec §2.1): the module must be installed
///    under the configured `~/.nexus42/modules/` (never peer-supplied
///    bytes) ⇒ defined `module_not_found`.
/// 6. **Module-id pin** (L2 review C-1): `ComputablePort::compute` merges
///    `request.computable` over the session state BEFORE re-resolving the
///    module id — without this gate a request-carried
///    `computable.module_id` naming a DIFFERENT installed module would
///    execute an unscoped module. The request may only repeat the gated
///    id; a differing override ⇒ defined `module_not_scoped`.
///
/// Returns `Ok(())` once every gate passes (the orchestrator route
/// re-parses the payload for execution); the first denial returns the
/// mapped envelope.
async fn verify_compute_gates(
    scope: &PeerScope,
    adapter: &NexusAdapter<'static>,
    peer: &PeerId,
    payload: &Value,
) -> Result<(), ErrorEnvelope> {
    let request: ComputeRequest = match serde_json::from_value(payload.clone()) {
        Ok(request) => request,
        Err(error) => return Err(map_reject(&invalid_payload("compute", &error))),
    };

    // 1. Read-only compute lock — the cheapest gate, checked first.
    if request.settle == Some(true) {
        return Err(settle_not_enabled());
    }

    // 2. Stored-world gate: the compute world is the stored entry's
    //    `extensions.nexus.world_id` (spec §2.2 — no wire carrier). A
    //    missing stored world cannot be verified ⇒ denied like a payload
    //    without a world carrier.
    let entry = match adapter.get_knowledge_entry(&request.entry_id).await {
        SpokeResult::Ok(entry) => entry,
        SpokeResult::Reject(reject) => return Err(map_reject(&reject)),
    };
    let Some(world) = get_world_id(&entry) else {
        return Err(denied(&format!(
            "entry {} has no stored world id; cannot verify world scope",
            request.entry_id
        )));
    };
    if !scope.allows_world(peer, world) {
        return Err(denied(&format!(
            "world {world} is not in this peer's world_scope"
        )));
    }

    // 3. Module identity — the locked resolution precedence (session state
    //    → entry body.computable). A resolution reject with the locked
    //    "module identity required" message is the missing-module-name
    //    denial (defined `module_not_found`); any other reject (missing
    //    session / entry / storage fault) maps through the locked table.
    let module_id = match adapter
        .resolve_compute_module_id(&request.session_id, &request.entry_id)
        .await
    {
        SpokeResult::Ok(id) => id,
        SpokeResult::Reject(reject)
            if reject.code == SpokeRejectCode::InvalidInput
                && reject.message.contains("module identity required") =>
        {
            return Err(module_not_found(None, &reject.message));
        }
        SpokeResult::Reject(reject) => return Err(map_reject(&reject)),
    };

    // 4. Module-scope gate (architect lock, spec §6.1): missing/empty
    //    `module_scope` denies ALL compute; a resolved module outside the
    //    list is denied before any WASM execution.
    if !scope.allows_module(peer, &module_id) {
        return Err(module_not_scoped(&module_id));
    }

    // 5. Host-local store gate (spec §2.1): the module must be installed
    //    under the configured module store; bytes are never peer-supplied.
    //    A host without a configured store serves no compute module.
    let Some(modules_dir) = adapter.user_modules_dir() else {
        return Err(module_not_found(
            Some(&module_id),
            "this host has no module store configured; no compute module can be served",
        ));
    };
    if !module_installed(modules_dir, &module_id) {
        return Err(module_not_found(
            Some(&module_id),
            &format!(
                "module {module_id:?} is not installed under {}",
                modules_dir.display()
            ),
        ));
    }

    // 6. Module-id pin (L2 review C-1): the adapter's `ComputablePort`
    //    merges request.computable over the session state before
    //    re-resolving the module id, so a request-carried
    //    `computable.module_id` that differs from the gated id would
    //    execute an unscoped module. The dynamic computable map may carry
    //    any invocation params EXCEPT a conflicting module identity; a
    //    differing override is denied before any WASM execution.
    if let Some(override_id) = request.computable.get("module_id").and_then(Value::as_str) {
        if override_id != module_id {
            return Err(module_not_scoped(override_id));
        }
    }

    Ok(())
}

/// Fail-closed host-local module store check (spec §2.1): the peer can name
/// only a module already installed under `~/.nexus42/modules/` as
/// `<id>/<id>.wasm` + `<id>/manifest.json`. The id must be a single path
/// component (no separators, no `.` / `..`) so the join can never escape
/// the store directory.
fn module_installed(modules_dir: &Path, module_id: &str) -> bool {
    let safe = !module_id.is_empty()
        && !module_id.contains('/')
        && !module_id.contains('\\')
        && module_id != "."
        && module_id != "..";
    safe && modules_dir
        .join(module_id)
        .join(format!("{module_id}.wasm"))
        .is_file()
        && modules_dir.join(module_id).join("manifest.json").is_file()
}

/// Compute denial — the N-C2 compute surface is read-only (spec §5 / §6.5):
/// `settle: true` is rejected because the compute settlement helper is NOT
/// enabled over Connect. Retry-safe for peers that re-issue with
/// `settle: false`.
fn settle_not_enabled() -> ErrorEnvelope {
    ErrorEnvelope {
        code: "settle_not_enabled".to_string(),
        message: "compute over Connect is read-only: settle:true is not enabled \
                  (the N-C2 settlement helper is not served)"
            .to_string(),
        details: Map::new(),
        extensions: HashMap::new(),
    }
}

/// Compute denial — the resolved module is not in the peer's `module_scope`
/// (architect lock, spec §6.1): a missing/empty scope denies ALL compute;
/// a resolved module outside the list is denied before any WASM execution.
/// Retry-safe for the operator (the allowlist is edited out-of-band).
fn module_not_scoped(module_id: &str) -> ErrorEnvelope {
    let mut details = Map::new();
    details.insert(
        "module_id".to_string(),
        Value::String(module_id.to_string()),
    );
    ErrorEnvelope {
        code: "module_not_scoped".to_string(),
        message: format!(
            "module {module_id:?} is not in this peer's module_scope; \
             compute is denied (fail-closed — missing/empty module_scope denies all modules)"
        ),
        details,
        extensions: HashMap::new(),
    }
}

/// Compute denial — no module identity is available (missing module name,
/// spec §2.2 resolution precedence) or the resolved module is not installed
/// in the host-local store (spec §2.1 — module bytes are never
/// peer-supplied). Retry-safe for peers that fix their session/entry
/// declaration or for operators who install the module.
fn module_not_found(module_id: Option<&str>, reason: &str) -> ErrorEnvelope {
    let mut details = Map::new();
    if let Some(id) = module_id {
        details.insert("module_id".to_string(), Value::String(id.to_string()));
    }
    ErrorEnvelope {
        code: "module_not_found".to_string(),
        message: format!("compute module unavailable: {reason}"),
        details,
        extensions: HashMap::new(),
    }
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
/// | world-conflict reject (V1.154 P2, spec §3.2 — adapter `world_conflict` details marker) | `world_conflict` | yes (client re-requests in the stored world) |
///
/// `InvalidInput` / `InvalidPacketInput` are the check/assemble path's
/// client-input rejects (scope wire conversion, packet extensions
/// namespace, malformed payloads) — the daemon's 400 class, so they must
/// not read as server faults. `reject.message` flows into
/// `ErrorEnvelope.message`; `reject.details` (when present) flows into
/// `ErrorEnvelope.details`.
fn map_reject(reject: &SpokeReject) -> ErrorEnvelope {
    // V1.154 P2 (R3 closure, spec §3.2 LOCKED): a zero-row CAS caused by a
    // world mismatch must surface as `world_conflict` — never collapsed into
    // `revision_conflict` / `stored_revision_stale`. The adapter carries the
    // classification on the `InternalError` carrier with a `world_conflict:
    // true` details marker; remap before the code table so the fixed wire
    // spelling wins.
    if is_world_conflict_reject(reject) {
        return ErrorEnvelope {
            code: "world_conflict".to_string(),
            message: reject.message.clone(),
            details: reject.details.clone().unwrap_or_default(),
            extensions: HashMap::default(),
        };
    }
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

    /// R3 wire spelling (spec §3.2 LOCKED): a zero-row CAS caused by a world
    /// mismatch must surface as the `world_conflict` ErrorEnvelope code —
    /// never collapsed into `revision_conflict` / `stored_revision_stale` —
    /// and a same-world stale revision keeps its existing code.
    #[test]
    fn map_reject_world_conflict_surfaces_world_conflict_wire_code() {
        let mut details = Map::new();
        details.insert("world_conflict".to_string(), Value::Bool(true));
        details.insert(
            "table".to_string(),
            Value::String("kb_key_blocks".to_string()),
        );
        details.insert("id".to_string(), Value::String("kb_race".to_string()));
        details.insert(
            "expectedWorld".to_string(),
            Value::String("wld_a".to_string()),
        );
        details.insert(
            "actualWorld".to_string(),
            Value::String("wld_b".to_string()),
        );
        let reject = SpokeReject {
            code: SpokeRejectCode::InternalError,
            message: "row moved to another world between verification and CAS".to_string(),
            details: Some(details),
        };

        let envelope = map_reject(&reject);
        assert_eq!(
            envelope.code, "world_conflict",
            "a world-mismatch CAS miss must surface as world_conflict"
        );
        assert_eq!(
            envelope
                .details
                .get("expectedWorld")
                .and_then(Value::as_str),
            Some("wld_a"),
            "expectedWorld rides through to the envelope details"
        );
        assert_eq!(
            envelope.details.get("actualWorld").and_then(Value::as_str),
            Some("wld_b"),
            "actualWorld rides through to the envelope details"
        );

        // Same-world stale/conflict rejects keep the existing codes — the
        // world-conflict branch must not widen their mapping.
        let rev_reject = SpokeReject {
            code: SpokeRejectCode::RevisionConflict,
            message: "expected base ahead of store".to_string(),
            details: None,
        };
        assert_eq!(map_reject(&rev_reject).code, "revision_conflict");
        let stale_reject = SpokeReject {
            code: SpokeRejectCode::StoredRevisionStale,
            message: "store ahead of expected base".to_string(),
            details: None,
        };
        assert_eq!(map_reject(&stale_reject).code, "stored_revision_stale");
    }
}
