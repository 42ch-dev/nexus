---
module: nexus-core (connect), apps/nexus42, nexus-local-db, spoke-connect
date: 2026-10-07
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: 2026-10-07-v1.207-p2-connect-replay-negotiation
applies_when:
  - "extending the Connect host event surface (subscribe / delivery / gap / resume)"
  - "authorizing host→consumer reverse invokes under spoke-connect's negotiated-capability intersection"
  - "implementing durable operation receipts or stable operationId semantics on any consumer"
tags:
  - connect
  - events
  - sse
  - reverse-invoke
  - receipt
  - operation-id
  - negotiation
  - gap
related_components:
  - nexus-core
  - nexus-local-db
  - apps/nexus42
  - spoke-connect
last_updated: 2026-10-07
---

# Connect event delivery + durable operation receipts (v1.207, RN-OGA-4/5)

## Context

The Connect path (`nexus-runtime` / `nexus42 connect start`, the independent
Connect-only product) was **invoke-only** until v1.207: no event-push surface
existed and there is deliberately **no HTTP listener** in the runtime process.
A third-party consumer that disconnected could lose events with no resume, and
cancel/timeout ambiguity on writes had no receipt to answer a replay from.
v1.207 added two coupled subsystems and their wire contract:

- **Event delivery** — a bounded Connect event ring, a `subscribe` served op,
  ack-gated delivery, cursor resume, and a gap/reconcile contract.
- **Operation receipts** — a stable `operationId`, a receipt store, and the
  frozen recover-first handshake for both Connect writes and compute runs.

The frozen contract (four architect amendments during implementation) lives
in the tracked spec `.mstar/specs/architecture/connect-event-delivery-and-operation-receipts.md`;
this doc carries the durable, reusable shape.

## Guidance

### Event lane (WS accept/responder lane, not the node lane)

- **Owner**: `nexus-core` owns the ring, the `(session, stream)` subscription
  table, the `subscribe` serving path, and the ack-gated push loop. It is
  served as host tool op `tools.nexus.subscribe` via per-session
  `ConnectResponder::register_tool_handler` in `monitor_session` Phase 2.
  The nexus42 node lane is **not** in the subscribe path.
- **Frames**: data frames stamp `id = <UUID epoch>:<decimal sequence>`
  (identical cursor syntax to the workflow SSE surface); control frames
  (including gaps) carry **no** cursor, so a verbatim consumer never advances
  past a sequence that was never retained.
- **Ack gating**: a batch advances `sent_through`; the read cursor advances
  only on the consumer's ack. Batches and rings are bounded.
- **Ordering**: the subscribe response MUST be written before the first push.
  The implementation observes the real outbound write through the
  `ObservedTransport` seam and flips `PendingResponse → Active` there.
- **Cancellation** distinguishes unstarted from started invokes: a push
  cancelled **before its first poll** is dropped untouched; a **started**
  reverse invoke either completes on the wire under one deadline or the
  session is closed fail-closed — never abandon a sequence-owning future.
- **Gap contract**: a stale/unresolvable cursor emits a cursorless
  `CoreConnectGapEvent` **first** with `requires_transcript_reconciliation:
  true` (reason `stale_cursor`); a retention-trimmed same-epoch cursor emits
  `history_unavailable`; never a silent skip. Reconcile = clear cursor and
  re-pull from the live tail.

### Reverse-use capability advertisement (the non-obvious authorization rule)

spoke-connect authorizes a host→dialer reverse invoke against
`negotiated_capabilities` = **the intersection of both hellos'
`capabilities[]`**. A consumer-served op the host intends to *invoke*
therefore must be named by **the host's WS-lane hello** as a *reverse-use*
capability — the consumer advertising it is necessary but not sufficient.
Constraints:

- This is **WS-lane hello content only**. The node lane derives its hello from
  host-served `LOCAL_SERVED_OPS`; `SERVED_OPS` / allowlist / `LOCAL_SERVED_OPS`
  stay untouched (interop honesty machine stays green with zero node-lane
  diff).
- The advertised op stays **consumer-served**: the host registers no handler
  for it, so a consumer→host invoke of it still fails closed at the native
  registered-or-deny arm.

### Operation receipts

- **OperationId**: `op_<hex32>` = SHA-256-128 over the canonical
  `{actor, session, action, args}` document. Same logical call ⇒ same id.
  Caller-supplied ids are used verbatim after shape validation and are bound
  by a **complete-scope fingerprint** (actor + session + action + args) so a
  different owner scope is a typed `operation_id_conflict`, never a silent
  dedupe.
- **Recover handshake (frozen order)**: resolve id → ask the receipt store
  FIRST → terminal receipt answers the replay (no re-apply) → running with a
  **live** owner ⇒ typed Busy (`operation_in_progress`); ownership unknown
  (e.g. receipt from a previous process) ⇒ typed `uncertain`; no receipt ⇒
  write `running` FIRST, run the effect, settle terminal.
- **Terminal immutability**: terminal rows are immutable against every
  admitted path; the table is `WITHOUT ROWID` (see the paired
  `database-issues/sqlite-terminal-immutability-traps.md`). Every terminal
  settlement carries exactly one payload (`finished → result_json`;
  `failed/cancelled/interrupted → error_json`).
- **Effect observation**: committed-effect evidence is read from the
  monotonic AUTOINCREMENT high-water (`sqlite_sequence.seq` of
  `core_changes`), which survives outbox row retention/pruning. An
  unverifiable observation resolves **conservatively** to
  effect-may-have-committed (typed not-retryable) — never to "no effect".

## Why This Matters

- The resume/gap contract is what makes a reconnecting consumer honest:
  without it, retention loss silently skips frames and "reconnect" is a lie.
- The reverse-use advertisement rule is counter-intuitive and cost two
  BLOCKED rounds: a plausible "consumer advertises it" design is provably
  insufficient under the pinned protocol's intersection semantics.
- Receipts turn cancel/timeout ambiguity into a typed answer; the
  conservative observation direction is the difference between a safe
  not-retryable answer and a double-apply.

## When to Apply

- Changing anything on the Connect event surface (frames, cursor, subscribe,
  gap, delivery driver) or the WS-lane hello composition.
- Adding a new host→consumer reverse invoke (advertise reverse-use; keep the
  op out of host-served sets).
- Implementing receipts/op-ids for a new consumer (compute Run and Connect
  are the first two; the scope tuple is the extension point).

## Examples

### Gap-first on a stale cursor (consumer view)

```text
subscribe {stream, last_event_id: <epoch>:<seq>}

# stale/unresolvable cursor — the FIRST frame is the gap, no cursor:
{event: "gap", data: {reason: "stale_cursor",
                      requires_transcript_reconciliation: true,
                      resync_required: true, operation_id: null,
                      inspect_url: "..."}}
# reconcile: clear the cursor and resubscribe without last_event_id
```

### Reverse-use advertisement (host WS-lane hello)

```text
capabilities[] = [tools.nexus.subscribe,          # host-served (registered handler)
                  tools.nexus.deliver_events]     # reverse-use (host INVOKES it)
# node lane: SERVED_OPS / LOCAL_SERVED_OPS unchanged — honesty machine green
```

Source spec (frozen, four amendments):
`.mstar/specs/architecture/connect-event-delivery-and-operation-receipts.md`.
