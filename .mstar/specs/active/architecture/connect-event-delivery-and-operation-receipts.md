# v1.207 Spec — Connect Replay/Negotiation Wire + Operation Receipts

> **Status:** shipped (v1.207, PR #367) — the Connect event-delivery + durable
> operation-receipt surface. Frozen 2026-10-07 (Phase 1 Review & Edit pass 2/3,
> architect) with four in-flight amendments during delivery. Landed in tracked
> specs at v1.207 close; the gitignored iteration-package copy remains the
> working record of the amendment rounds.
> **Document class:** Master (Connect event surface + operation-receipt store:
> the durable contract for WS-lane event delivery, gap/reconcile, and the
> receipt-first recover handshake).
> **Coordinates with:** [spoke-adapter-architecture.md](spoke-adapter-architecture.md)
> (the spoke-connect transport + manifest seam), `schemas/core/`
> (subscribe/gap/receipt schemas), `knowledge/architecture-patterns/connect-event-delivery-and-operation-receipts.md`
> (the distilled reusable shape).
> Resolves compass Q1 and the P2/P3 architect markers; referenced by the
> v1.207 iteration plans (P2 §A/§C, P3 §B/§C). Field names are named **once**
> here (§C) — the plans point at this file instead of restating them.
>
> **Promoted to**: `.mstar/knowledge/architecture-patterns/connect-event-delivery-and-operation-receipts.md`
> (v1.207 §3.2 iteration-close, structured rewrite of the durable shape).
>
> Source survey basis: HEAD 2026-10-07, read-only. Anchor citations at the
> bottom; every placement claim below names the module it lands in.

> Amended 2026-10-07 (Phase 2, architect): §A.2a added — the WS
> event-delivery contract resolving the P2 Task 1 NEEDS_CONTEXT
> (the P2 Task 1 report);
> §C gained the §A.2a field rows. §A.1–§A.5 and §B are otherwise unchanged.

> Amended 2026-10-07 (Phase 2, second pass, architect): §A.2a(f) added —
> the composition-seam decision resolving the P2 Task 1 BLOCKED round
> (same report); the §A.2 subscribe bullet is re-pointed to the WS
> accept/responder lane; §A.2a(d)'s manifest-lockstep sentence is restated
> (subscribe enters no node-lane set); §C gained the
> `tools.nexus.subscribe` row. The §A.2a(a)–(e) wire contract, §A.3, §A.4,
> §A.5 and §B are otherwise unchanged.

> Amended 2026-10-07 (Phase 2, third pass, architect): §B.2 writer
> admission ruled cooperative — the Connect receipt-writer path,
> resolving the P3 Task 3 blocker
> (the P3 Task 3 report);
> §B.1 compute-Run `session` derivation confirmed run-scoped for the
> session-less daemon lane. §A and §C are unchanged (both rulings are
> P3-internal).

> Amended 2026-10-07 (Phase 2, fourth pass, architect): §A.2a(f)4
> ruled — the WS-lane host hello names `tools.nexus.deliver_events` as a
> **reverse-use** capability, resolving the P2 Task 2 BLOCKED round
> (the P2 Task 2 report):
> the pinned consumer-side reverse-invoke dispatch gate
> (`remote_adapter.rs:1051-1063`) reads the both-hello intersection
> (`remote_adapter.rs:1707-1710`), so consumer-side advertisement alone
> can never authorize the push. §A.2a(a)/(d)/(f)7 restated; the §C
> `tools.nexus.deliver_events` row updated; source anchors gained the
> gate/intersection citations. The id stays consumer-served, the
> node-lane prohibition stands, and §A beyond §A.2a(a)/(d)/(f) and §B
> are unchanged.

## §A Connect replay/gap + optional-capability negotiation wire (P2)

### A.1 Current-state finding (survey result)

The Connect path (`nexus-runtime` / `nexus42 connect start` — one boot via
`connect::build_host_config`, `apps/nexus42` feature `connect-host`) serves
an **invoke-only** spoke-connect surface: `invoke::SERVED_OPS`
(`apps/nexus42/src/commands/connect/invoke.rs:230`) = the six core ops +
`tools.nexus.*`, dispatched through the fail-closed served-op gate
(`invoke.rs:486`). **No event-push surface exists on the Connect path
today** — the runtime has no HTTP listener at all (readiness is
stdout-only, `docs/nexus-runtime.md:68`). The replayable run-event SSE
rings (`crates/nexus-core/src/execution/run_events.rs`) are the
current-host workflow surface and are **out of P2 scope** (plan boundary;
v1.188/v1.195). P2 therefore adds a new, Connect-scoped event surface —
it does not extend the workflow rings.

### A.2 Event surface placement

- **Ring**: new module `crates/nexus-core/src/connect/events.rs` — a
  bounded Connect event ring following the `RunEventRegistry` design
  (`crates/nexus-core/src/execution/run_events.rs:802` `subscribe_live`:
  atomic replay-then-live-tail; `:936-948` bounded-page + `resync_required`
  semantics). Connect-scoped (per peer session), not per workflow run.
- **Transport**: the existing spoke-connect WebSocket lane
  (`crates/nexus-core/src/connect/ws_transport.rs`; accept/responder lane
  `crates/nexus-core/src/connect/accept.rs:259` `spawn_accept_loop`).
  Event frames use the SSE dialect (`id:` / `event:` / `data:`,
  `SseFrame`, `run_events.rs:157`) carried on the established Connect
  session. `nexus-runtime` gains **no HTTP listener** (A.1 invariant).
- **Subscribe handshake**: a new served op riding the ordinary Connect
  invoke envelope (`{op, args}`,
  `schemas/core/core-connect-invoke-request.schema.json`) carrying the
  subscribe arguments `{stream, last_event_id?}`. Admission is fail-closed
  in the logical order session gate → capability advertisement (§A.5) →
  op admission. Executing surface (second amendment — §A.2a(f)): the op is
  served on the WS accept/responder lane under the `tools.` grammar as
  `tools.nexus.subscribe` (arguments ride the §4 `payload.arguments`
  convention there); it does NOT enter `SERVED_OPS`
  (`apps/nexus42/src/commands/connect/invoke.rs:230`) /
  `LOCAL_SERVED_OPS` (`crates/nexus-spoke-adapter/src/manifest.rs:279`),
  so the node-lane honesty machine
  (`apps/nexus42/src/commands/connect/interop.rs:534-601`) is untouched by
  it. The §A.2a(a) admission citations `invoke.rs:486` /
  `allowlist.rs:323-325` name the node-lane surface only; §A.2a(f)3 binds
  the logical gate order to the WS lane's concrete gates.

### A.2a Event-delivery contract on the WS lane (Phase 2 amendment)

§A.2 places the event surface on the existing spoke-connect WS lane; this
section defines how event pushes are multiplexed onto that lane next to
ordinary request/response envelopes. **Compatibility verdict:
compatible-inside-envelope** — every carrier used here already exists in
spoke-connect 0.14.1 and in the Nexus-owned invoke payload dialect; no
external protocol field, envelope kind, or behavior changes, and no
cross-boundary blocker exists.

**(a) Subscribe transition.** `subscribe` is an ordinary served op riding the
invoke envelope: request `{op: "subscribe", args: {stream, last_event_id?}}`
(envelope shape `schemas/core/core-connect-invoke-request.schema.json:8-11`;
`args` extensible `:19-24`). Admission follows the §A.2 fail-closed order —
served-op gate (`invoke.rs:486`) → op_scope allowlist
(`allowlist.rs:323-325`) → capability advertisement (§A.5) — plus one
reverse-leg check: the session's negotiated remote manifest (read at
admission, `accept.rs:410` `responder.remote_manifest()`) MUST advertise the
consumer-served delivery capability `tools.nexus.deliver_events`; otherwise
the invoke is refused `op_unsupported` with zero side effects (the §A.5
first-use rule applied to the reverse leg — no new negotiation round-trip).
The push's own authorization is a separate gate: the pinned consumer-side
reverse-invoke dispatch check evaluates the op against the both-hello
intersection (`remote_adapter.rs:1051-1063`, constructed `:1707-1710`),
so the delivery id must ALSO be named by the host WS-lane hello — the
reverse-use carriage ruled in (f)4 (fourth amendment).
On success the handler performs the ring's atomic subscribe (replay cut +
live-tail registration in one step, the `subscribe_live` precedent
`run_events.rs:802`) with the verbatim `last_event_id`, then responds. The
success response carries NO frames:
`result = {"stream": <echo>, "epoch": <ring epoch UUID>, "resumed_from": <the
verbatim cursor applied, or null>}` (the echo makes the cursor verbatim
round-trip testable; schemas `core-connect-subscribe-request.schema.json` /
`core-connect-subscribe-response.schema.json`). Frame delivery then runs per
(session, stream) as below.

**(b) Event-frame carrier.** One push = one host-originated **reverse tool
invoke** on the same lane: `ConnectResponder::invoke_tool` (spoke-connect
0.14.1 `remote/responder.rs:602` — the "reverse tool-invoke face (frozen
contract §6)", `:596`), op id `tools.nexus.deliver_events` (tools grammar
`tools.<ns>.<id>`, `accept.rs:194`), payload per the spoke §4 tool
convention `{ "arguments": <delivery> }` (`responder.rs:596-608`). Delivery
arguments (defined in this spec — the serving lane builds the object
directly; no standalone schema file exists):

```
{ "stream": "<stream-id>",
  "frames": [ { "id": "<epoch>:<seq>",   // data frames only
                "event": "<name>",
                "data": <json> }, ... ] }
```

`frames` is a bounded batch, contiguous in sequence, in ring order
(bounded-page precedent `run_events.rs:936-948`). The frame fields are the
§C SSE fields `id` / `event` / `data` carried as structured JSON — the §A.2
SSE dialect's field semantics without text serialization (consistent with
the A.1 no-HTTP invariant). §A.3 is preserved: data frames stamp
`id = <epoch>:<seq>`; control frames carry NO `id` (precedent
`retained_execution_contracts.rs:5842-5843`), so a verbatim consumer never
advances its cursor on a control frame. §A.4's gap-event carriage: the gap
event is delivered in-band as a control frame
`{"event": "gap", "data": <CoreConnectGapEvent>}` (no `id`); a stale-cursor
subscribe still succeeds and the first delivered frame on that subscription
is the gap event — never silent event loss — after which delivery continues
from the live tail (reconcile path per §A.4: clear cursor, resubscribe
without `last_event_id`).

**(c) Discrimination vs ordinary request/response consumers.** The lane is
bidirectional with request-id-correlated responses (pending reverse-invoke
waiters keyed by request id, `responder.rs:478`). A push arrives at the
consumer as an inbound invoke **request** (host-originated), which the
client runtime routes to its registered `tools.nexus.deliver_events` handler
(dialer-side registration `remote/remote_adapter.rs:554`; reverse-invoke
serving pipeline `:809`) — never to a pending request waiter; ordinary
invoke responses keep flowing as **responses** correlated by request id. No
frame-kind field is added to any envelope: direction + envelope kind is the
discriminator, and it is native to the external protocol. Consumers that
never subscribe receive no pushes (the host pushes only to sessions with an
active subscription) and need no handler; their request/response path is
unchanged. Lane ordering: host→consumer traffic is one FIFO WS message
stream (one envelope = one WS message, `ws_transport.rs:5-7`); the host MUST
emit the subscribe response before the first delivery push of that
subscription.

**(d) Flow control, failure, lifecycle.** Delivery is ack-gated: each push
resolves with the consumer's tool result (success payload `{ "result": {} }`
per the §4 convention) and the next batch is sent only after the previous
ack — total order and backpressure per subscription. A delivery failure
(consumer deny — mapped per `responder.rs:596-608`; timeout; transport
close) ends the subscription fail-closed; the ring retains frames within its
bound, and the consumer resubscribes with its verbatim last-received cursor
(§A.3) — this is the mechanism behind reconnect-mid-stream replay. One
active subscription per (session, stream): a new `subscribe` for the same
stream atomically replaces the old (new replay cut, old delivery
cancelled); session close ends all of its subscriptions (responder close
fails pending reverse invokes, `responder.rs:560-563`). Manifest lockstep
(restated by (f)7): `subscribe` is served exclusively on the WS
accept/responder lane as `tools.nexus.subscribe` and enters NO node-lane
set — not `SERVED_OPS`, not `LOCAL_SERVED_OPS`, not the node-lane hello —
so the interop honesty check (`interop.rs:534-601`) stays green with zero
node-lane diff. `tools.nexus.deliver_events` is
**consumer-served** and MUST NOT enter any host-SERVED set on either
lane; its presence in the host WS-lane hello is reverse-use-only
carriage per (f)4 — the pinned consumer-side dispatch gate reads the
both-hello intersection, so that carriage is what authorizes the push
(the no-`tools.*`-beyond-S lockstep, `manifest.rs:930-1011`, binds the
node-lane manifest; the honesty machine's host-advertised ⇔ host-served
invariant binds the NODE lane and is untouched).

**(f) Composition seam — core-owned execution on the WS accept/responder
lane (Phase 2, second amendment).** Decided on the P2 Task 1 BLOCKED
evidence
(the P2 Task 1 report):
ONE owner for the subscription execution path. **Route (A), core-owned
execution** — the ring, the subscription table, the subscribe serving
path, and the ack-gated push loop are all owned by `nexus-core` on the WS
accept/responder lane; the nexus42 node lane (`SpokeConnectNode` +
`InvokeHandlerV2`) is not in the subscribe path at all.

Rejected alternatives (survey verdicts, HEAD 2026-10-07):

- **Route (B), handler-context extension — impossible inside this repo.**
  `InvokeHandlerV2` is a frozen external type,
  `dyn Fn(&PeerId, &str, Value) -> Result<Value, ErrorEnvelope>`
  (spoke-connect 0.14.1 `config.rs:51`), pinned `=0.14.1` (workspace
  `Cargo.toml:240`) — synchronous by definition and not editable here; and
  `SpokeConnectNode` exposes no responder/reverse-invoke handle (public
  API `start` / `local_peer_id` / `local_manifest` / `listen_addrs` /
  `connect` / `shutdown`, `node.rs:1646-1784`; `node.rs` never references
  `ConnectResponder` — the responder machinery lives behind the
  `remote-adapter` feature, which nexus42 does not enable
  (`apps/nexus42/Cargo.toml:103`) and nexus-core does
  (`crates/nexus-core/Cargo.toml:79`)). Any handler-context bridge
  requires an upstream spoke-connect release — rejected; the external
  spoke-connect protocol is unchanged.
- **Route (A) as "intercept beside the nexus42 handler" — impossible.**
  The node lane (libp2p/noise; `SpokeConnectNode::start`,
  `apps/nexus42/src/bin/nexus-runtime.rs:166`) and the WS accept lane
  (`spawn_accept_loop`, `accept.rs:259`) are disjoint listeners; an invoke
  admitted on the node lane is answered inside spoke-connect's session
  loop and cannot be intercepted by nexus-core. The feasible form of (A)
  therefore RE-HOMES `subscribe` onto the WS accept/responder lane, where
  the responder already lives.

The seam (all edit points named):

1. **Serving point.** `subscribe` is served as the host-served tool op
   **`tools.nexus.subscribe`** via `ConnectResponder::register_tool_handler`
   (spoke-connect 0.14.1 `remote/responder.rs:580`; `ToolHandler =
   Arc<dyn Fn(Value) -> BoxFuture<'static, SpokeResult<Value>> + Send +
   Sync>`, `remote/remote_adapter.rs:414-415` — async, so the ring's
   atomic subscribe can be awaited). Registration is per session in
   `monitor_session` Phase 2 (`accept.rs:405-442`), where the
   authenticated `peer_id` (`:383`), the `Arc<ConnectResponder>` (`:368`),
   and `responder.remote_manifest()` (`:410-411`) are all in hand; the
   closure captures them plus the ring handle. Serving is native to the
   responder dispatch (`responder.rs:1069-1073` → `dispatch_tool_invoke`
   `:1093`; no registered handler → `op_unsupported`, `:1103-1111`).
2. **Request/response carriage.** The invoke envelope family is unchanged
   (signed `{op, args}` envelope, envelope-auth v2). On this lane the §4
   tools convention carries the subscribe arguments `{stream,
   last_event_id?}` as `payload.arguments` (`responder.rs:1116-1121`) and
   the frozen response fields `{stream, epoch, resumed_from}` as the tool
   success result (`payload.result`; `send_tool_result`, `:1147`; the
   `:596-608` convention). The
   `core-connect-subscribe-request.schema.json` /
   `core-connect-subscribe-response.schema.json` schemas define exactly
   those argument/result payloads; every field name in (a) and §C is
   unchanged — only the op id carries the `tools.` grammar the responder
   lane requires.
3. **Admission binding.** (a)'s logical fail-closed order binds on this
   lane to: (i) the session handshake gate — fail-closed peer allowlist +
   pinned keys (`ConnectResponderOptions.allowlist` / `peer_keys`, wired
   `accept.rs:352-353`); (ii) the negotiated-capability gate — native
   `gate_allows` (`responder.rs:1084`) through the core rule that
   `tools.<ns>.<id>` requires its own id in the session's negotiated set
   (`core/dispatch.rs:40-42`; negotiated = hello intersection,
   `responder.rs:734-745`) — this IS the §A.5 first-use rule with zero
   new machinery: a session that never negotiated `tools.nexus.subscribe`
   is refused `op_unsupported` before the handler runs, zero side
   effects; (iii) registered-handler-or-deny (`responder.rs:1103-1111`);
   (iv) (a)'s reverse-leg check, executed by the handler before any side
   effect — `remote_manifest()` MUST advertise
   `tools.nexus.deliver_events`, else `op_unsupported` with zero side
   effects. The node-lane citations in (a) (`invoke.rs:486`,
   `allowlist.rs:323-325`) name the node-lane surface, which subscribe
   never reaches; the node-lane `PeerScope` op_scope grant does not apply
   on this lane and is not weakened — subscribe's authorization surface is
   the lane's operator-curated peer handshake plus negotiation (narrower:
   the lane is loopback-only, item 6).
4. **Advertisement / negotiation.** `tools.nexus.subscribe` is a
   host-served capability constant joined at the WS-lane hello
   composition — `daemon_manifest` (`accept.rs:187`, called `:320`) —
   never derived from the peer tool allowlist (that allowlist is the
   peer-tool set, AR-69). The consumer opts the session in by advertising
   `tools.nexus.subscribe` in its own hello (negotiation = intersection).
   The host WS-lane hello ALSO names `tools.nexus.deliver_events` — as a
   **reverse-use** capability, not a serve claim (fourth amendment; the
   P2 Task 2 BLOCKED ruling): the host announces that it will INVOKE
   this consumer-served op on the session and accepts the consumer's
   serving of it. This host-side carriage is REQUIRED by the pinned
   reverse-invoke authorization: the push is gated on the CONSUMER side
   by `dispatch_allowed(op, &negotiated)`
   (`remote_adapter.rs:1051-1063`), the negotiated set is the both-hello
   `capabilities[]` intersection (dialer construction
   `remote_adapter.rs:1707-1710`; responder mirror
   `responder.rs:734-745`), and `dispatch_allowed` requires the exact
   `tools.*` op string in that set (`core/dispatch.rs:40-43`, `:57-62`)
   — an intersection contains the id only when BOTH hellos name it, so
   the consumer's hello alone can never authorize the push. The id
   stays consumer-SERVED: the host registers no handler for it, so a
   consumer→host invoke of the id still fails closed at the responder's
   registered-or-deny arm (`responder.rs:1103-1111`) after the
   capability gate; it enters no host-SERVED set on either lane (item
   7). The consumer's native reverse gate (sequence + envelope-auth,
   `remote_adapter.rs:927` `run_reverse_gate`) still runs first — the
   pre-amendment text erred in naming it the only consumer-side check;
   the dispatch gate is the arm behind it. Join mechanics mirror the
   subscribe constant: a fixed reverse-use constant at the
   `daemon_manifest` composition (never allowlist-derived), joining
   `capabilities[]` — the intersection input — while the `tools[]`
   descriptor list stays empty.
5. **Ordering mechanism for (c)'s MUST.** A new subscription registers as
   `PendingResponse`; the per-session delivery driver never sends for a
   `PendingResponse` subscription. The flip to `Active` happens exactly
   when the subscribe response's outbound write is observed at the
   nexus-owned session transport decorator — the `ObservedTransport` seam
   (`accept.rs:71-109`, installed per connection at `:343`) — extended to
   correlate the response's echoed `request_id` with the subscribe request
   it observed inbound. Observation is read-only (never
   re-serialization), so `WsTransport`'s opacity invariant
   (`ws_transport.rs:8-9`) is preserved: the decode-observe lives in the
   decorator, not the transport. One driver task per session (spawned in
   Phase 2) keeps host→consumer sends single-threaded per session — (c)'s
   lane ordering and (d)'s total order / ack gating hold by construction;
   (d)'s replace/cancel/close lifecycle is the subscription table's
   replace rule plus the driver's close observation (responder close fails
   pending reverse invokes, `responder.rs:560-563`; Phase 3 eviction,
   `accept.rs:451-473`).
6. **Boot wiring + lane constraint.** The WS lane is built and
   contract-tested but has no production caller on HEAD
   (`start_peer_tools_lane`, `accept.rs:519`; only
   `crates/nexus-core/tests/retained_peer_contracts.rs` wires it).
   `connect start` (`apps/nexus42/src/commands/connect/mod.rs:295-301`)
   and `nexus-runtime` (`apps/nexus42/src/bin/nexus-runtime.rs:163-167`)
   start the lane after `build_host_config` — NOT inside it
   (`connect dial` shares `build_host_config`, `mod.rs:378`; a short-lived
   dialer must not bind the lane port). Lane startup failure follows the
   lane's documented caller policy (`accept.rs:515-518`): the boot warns
   and continues WITHOUT the event surface — honest by construction,
   because `tools.nexus.subscribe` is advertised only in the lane's
   per-connection hello, which never exists without the lane. The lane is
   plaintext and loopback-only by fail-closed refusal
   (`accept.rs:550-560`): the Connect event surface is loopback-only in
   P2; remote event consumption is a future TLS decision, explicitly out
   of scope. **Lane address disclosure (v1.210 P3, architect-locked in the
   Phase 1 Review & Edit chain, 2026-10-10; ships with v1.210).** Both boot
   surfaces print the event-lane address they actually bound (from the
   lane handle's bound address, never the config echo) as one stable,
   greppable readiness line — `event_lane: ws://<host>:<port>/connect` —
   alongside the node-lane `listen:` lines: `connect start` on its stderr
   startup block, `nexus-runtime` on its stdout readiness block (that
   block is its only liveness surface). Lane startup failure keeps the
   warn-and-continue policy above, but the warning names the configured
   `host:port`, the failure class (bind conflict / non-loopback refusal /
   malformed `daemon.json`), and the `daemon.json` port remedy. Neither
   surface grows a `--json` envelope (none exists today) or a lane-address
   override flag — host/port stay `daemon.json` operator config,
   boot-scoped per DF-92.
7. **Manifest lockstep (restates the sentence in (d)).**
   `tools.nexus.subscribe` enters NO node-lane set — not `SERVED_OPS`
   (`invoke.rs:230`), not `LOCAL_SERVED_OPS` (`manifest.rs:279`), not the
   node-lane hello — so the interop honesty machine
   (`interop.rs:534-601`) is untouched and stays green with zero
   node-lane diff; its advertisement is the WS-lane per-connection hello
   (item 4). `tools.nexus.deliver_events` enters no host-SERVED set on
   EITHER lane — its WS-lane hello presence (item 4) is reverse-use
   carriage, not serving (the no-`tools.*`-beyond-S lockstep,
   `manifest.rs:930-1011`, binds the node-lane manifest; the WS-lane
   reserved-set admission, `accept.rs:425`, governs peer-advertised
   tools and is unaffected — the host never invokes a peer's
   `tools.nexus.subscribe`). **WS-lane-vs-node-lane distinction (fourth
   amendment):** the node-lane hello's `capabilities[]` derives
   exclusively from host-SERVED ops (`build_local_host_manifest`,
   `manifest.rs:310`, fed by `LOCAL_SERVED_OPS`, `manifest.rs:279`), so
   on the node lane "advertised ⇒ served" is the invariant the interop
   honesty machine (`interop.rs:534-601`) enforces — it is untouched
   and stays green with zero node-lane diff. The WS-lane hello is a
   separate composition (`daemon_manifest`, `accept.rs:187`) whose
   `capabilities[]` carries two honesty classes: advertised-SERVED
   (`tools.nexus.subscribe` ⇔ a per-session handler is registered,
   `responder.rs:580`) and advertised-REVERSE-USE
   (`tools.nexus.deliver_events` ⇔ the host actually issues that
   reverse invoke via `ConnectResponder::invoke_tool`,
   `responder.rs:602`, from the per-session delivery driver). Both
   classes are honest by construction; the pinned negotiation semantics
   (hello intersection) make no served/used distinction, and the §A.2a
   compatibility verdict stands — hello `capabilities[]` content is
   product-defined session scope, so this amendment changes no external
   protocol field, envelope kind, or behavior.

### A.3 Resume cursor

- **Syntax**: `<UUID epoch>:<decimal sequence>` — identical to the
  workflow subscription cursor (generated
  `crates/nexus-contracts/src/generated/core/core_workflow_subscribe_request.rs:5`;
  parse/epoch-mismatch handling `run_events.rs:848-851`). One cursor
  syntax across nexus SSE surfaces.
- **Location**: the subscribe request **payload** field `last_event_id`
  (field name per `schemas/core/core-workflow-subscribe-request.schema.json`
  / generated TS `core-workflow-subscribe-request.ts:18`). The Connect
  path has no HTTP layer, so there is no `Last-Event-ID` **header** here —
  the header forwarding in `crates/nexus-core-node/src/execution.rs:521-525`
  belongs to the out-of-scope workflow surface. Consumers persist the last
  received frame's `id` verbatim and pass it back untouched (the transport
  never parses or renumbers it).
- **Server side**: replayed/live frames stamp `id = <epoch>:<seq>`;
  control frames (including the gap event, §A.4) carry **no** cursor, so a
  verbatim consumer never advances its cursor to a sequence that was never
  retained (precedent: `crates/nexus-core/tests/retained_execution_contracts.rs:5842-5843`).

### A.4 Gap-event wire shape

New generated type **`CoreConnectGapEvent`** (new schema
`schemas/core/core-connect-gap-event.schema.json`, generating into both
`crates/nexus-contracts` and `packages/nexus-contracts`), modeled on the
existing `CoreStreamGap` precedent
(`schemas/core/provider-event-batch.schema.json`; TS
`packages/nexus-contracts/src/generated/core/provider-event-batch.ts:105-110`;
producer `crates/nexus-agent-host/src/providers/port.rs:595-598`):

| field | type | meaning |
|-------|------|---------|
| `reason` | enum: `lagging` \| `oversized` \| `history_unavailable` \| `interrupted` \| `stale_cursor` | why the stream cannot be served contiguously |
| `requires_transcript_reconciliation` | literal `true` | the RN-OGA-4 reconcile signal (done-definition's `requiresTranscriptReconciliation`; wire field is snake_case per repo codegen convention, cf. `resync_required`) |
| `operation_id` | `string` \| `null` | receipt reference — **P3-owned** (§C); present when the gap interrupts an operation-bearing stream |
| `resync_required` | literal `true` | parity with `CoreStreamGap` consumers |
| `inspect_url` | `string` | operator inspection target |

Reason discriminator: `stale_cursor` = the cursor itself is unresolvable
(epoch mismatch, unknown stream, cursor ahead of retained — the case that
maps to `HistoryUnavailableWire` on the workflow ring,
`run_events.rs:851`); `history_unavailable` = cursor valid but frames
retention-trimmed. A stale cursor MUST produce this gap event with
`requires_transcript_reconciliation: true`, never silent event loss. The
consumer's reconcile path: full transcript re-pull from the live tail
(clear cursor, resubscribe without `last_event_id`).

### A.5 Optional-capability negotiation handshake location

**At connect session setup — the hello exchange — not a new RPC, and not
a `HostCapabilityManifest`-side channel.** The manifest is the *carrier
document* inside `connect_hello`; the session's negotiated capabilities
are the intersection of both hello `capabilities[]` lists
(`docs/nexus-runtime.md:130-135`). Anchors:

- Builder SSOT: `crates/nexus-spoke-adapter/src/manifest.rs:310`
  `build_local_host_manifest`; wire conversion `:382` `to_connect_hello` /
  `:419` `from_connect_hello`; wire type re-export `:62`
  `ConnectHelloManifest` (`spoke_schemas::connect::connect_hello::HostCapabilityManifest`).
- Host wiring: `ConnectConfig.local_manifest` (boot
  `connect::build_host_config`, `docs/nexus-runtime.md:53-61`); accept-lane
  manifest `crates/nexus-core/src/connect/accept.rs:187` `daemon_manifest`.
- **First-use-without-negotiation fails closed with the existing typed
  refusal**: an op whose capability was not negotiated is refused
  `op_unsupported` at the spoke dispatch gate before any host handler runs
  (`docs/nexus-runtime.md:93-94`, `:141-143`; dispatch gate
  `invoke.rs:486`). P2 pins this with a test (first-use without
  negotiation ⇒ typed refusal, zero side effects) and adds any missing
  gate arm for newly introduced optional capabilities — it does not add a
  negotiation round-trip.

## §B Durable operation receipts + stable operationId (P3)

### B.1 operationId syntax and scope derivation

- **Wire syntax**: `op_<hex32>` — 32 lowercase hex characters
  (`^op_[0-9a-f]{32}$`); 128 bits, SHA-256 truncated. One syntax on every
  surface; the wire field name is `operation_id` (snake_case, per
  `schemas/core/core-provider-operation.schema.json:17-20` and
  `provider-call.schema.json`).
- **Derivation (default)**: `operationId = "op_" + hex128(SHA-256(
  canonical_json({actor, session, action, args}) ))` where:
  - `actor` = `creator_id` (the principal);
  - `session` = the consumer's session scope — Connect: the authenticated
    peer session id; compute Run: the run's session correlation when the
    carrying lane provides one (`ToolExecuteRequest.session_id`,
    `capabilities.rs:91`). The direct daemon lane's `RunRequest` carries
    no session field, so there the run's own id is the session component
    (`session = run_id`, `compute.rs:1287-1303`; confirmed 2026-10-07,
    third amendment) — a run-scoped shadow, unique per run, stable for
    the run, reachable from the row by `subject_id`;
  - `action` = the op/tool id string;
  - `args` = canonical JSON of the operation arguments.
  Same logical call retried/replayed ⇒ identical tuple ⇒ identical id ⇒
  the receipt dedupes. Any scope difference (actor, session, action)
  changes the hash ⇒ collision-free across scopes.
  Dedupe scope (third amendment): with `session = run_id`, compute
  receipts dedupe ONLY the same run's replay/recovery — two independent
  identical runs derive distinct ids and each executes (behavior
  preservation; a request-scoped derivation would dedupe every repeated
  identical run repo-wide, a behavior change the P3 acceptance's
  behavior-preserving clause does not authorize). Cross-retry replay
  dedupe by stable logical-call id is the Connect consumer's semantics
  (the peer-session-scoped ids above).
- **Caller-supplied ids**: a caller MAY supply `args.operation_id`; it is
  used verbatim after shape validation. The receipt primary key is
  first-writer-wins: a conflicting insert carrying a **different**
  `request_fingerprint` (§B.2) is a typed `operation_id_conflict` refusal,
  never a silent dedupe (ownership precedent:
  `crates/nexus-agent-host/src/providers/multiplex.rs:346-349`
  "operation_id already owned").
- **Explicit non-model**: `HostOperationId::new()`
  (`crates/nexus-agent-host/src/ids.rs:117-121`) is random-unique per
  attempt — stable uniqueness, not replay stability. Receipts exist
  precisely to cover what that scheme cannot.

### B.2 Receipt storage location per consumer

**One table, `operation_receipts`, in `nexus-local-db`** (the active
workspace SQLite DB — shared by the compute lane and the Connect host,
`docs/nexus-runtime.md` coexistence §), created by a new migration sibling
of the `compute_sessions` migrations
(`crates/nexus-local-db/migrations/20260730_000001_compute_sessions.sql`,
`20260731_000002_compute_sessions_direct_lane.sql`). Columns:

| column | notes |
|--------|-------|
| `operation_id` | PRIMARY KEY (`op_<hex32>` or validated caller id) |
| `consumer` | `CHECK (consumer IN ('compute_run','connect_invoke'))` — the two first consumers |
| `subject_id` | compute: `run_id`; Connect: `<peer_session_id>/<op>` |
| `status` | `CHECK (status IN ('running','finished','failed','cancelled','interrupted'))` — mirrors `core-provider-operation.schema.json:31-37`; **terminal is never downgraded** (that schema's doc contract) |
| `request_fingerprint` | SHA-256 of the canonical request — replay-vs-conflict check (§B.1) |
| `result_json` / `error_json` | terminal payload, exactly one set on terminal settlement |
| `created_at` / `updated_at` / `terminal_at` | RFC 3339 UTC |
| `sequence` | monotonic, assigned at write time (journal precedent `CoreProviderOperation.sequence`) |

- Writer admission (**ruled 2026-10-07, third amendment** — resolves the
  P3 Task 3 Connect-writer blocker): the table is a **cooperative core
  table** under the core writer protocol, not engine-owned — because it
  is shared by the compute lane (engine writer) and the Connect host
  (DIRECT writer). The `guard_operation_receipts_*` family admits the
  migration writer, the epoch-matched single engine owner, AND the
  cooperative DIRECT admission: `r.mode IN ('direct', 'migration')` OR
  epoch-matched `'engine'` — byte-for-byte the
  `guard_knowledge_entries_*` shape
  (`20260912000001_core_writer_protocol.sql:1820-1843`). The guard
  distinguishes admitted writers by their `core_writer_registration` row
  (mode + epoch pins); the Connect host keeps its existing DIRECT
  pool — it never takes the daemon's engine ownership
  (`computable_port.rs:934-937`, `invoke.rs:2190-2193`). Receipt
  integrity survives on every admitted path because it is enforced
  below the admission layer and is writer-agnostic: terminal
  immutability (`immutable_terminal_operation_receipts_*`,
  `20260919000001_operation_receipts.sql:150-164`) fires for EVERY
  writer; first-writer-wins is the `operation_id` PRIMARY KEY (`:38`)
  plus the store's fingerprint-conflict check, arbitrated by SQLite
  single-writer serialization, not by the guard; the payload CHECK
  (`:49-56`) binds all writers; raw/unregistered pools stay fenced (no
  registration row ⇒ `WRITER_FENCED`); a DIRECT registration pins
  `migration_epoch`, so a writer spanning a migration is fenced
  (cooperative-quiescence protocol, `writer_protocol.rs:921-963`); and
  the outbox target `core_changes` already admits `'direct'`
  (`20260912000001_core_writer_protocol.sql:125-140`), so the
  `outbox_operation_receipts_*` family (modeled on
  `outbox_compute_sessions_*`, `:4237-4260`) mirrors Connect-host
  receipt mutations exactly like engine writes. Effect on the
  Task-2-approved invariants: ONLY this table's admission class changes
  (engine-owned → cooperative core table) — `compute_sessions` stays
  engine-owned (`guard_compute_sessions_*`, `:919-974`; its session
  state machine wants the single-owner CAS discipline),
  `core_writer_registration` / `core_workspace_gate` are untouched, and
  no engine-epoch semantics change. Rejected in the same ruling: (a) an
  engine-admitted receipt writer inside the Connect process — every
  in-repo form either takes the exclusive engine lock
  (`writer_protocol.rs:807-811`), the verbatim ownership the Connect
  host must never take, or proxies receipt writes through the daemon
  (new cross-process machinery that couples every served Connect write
  to daemon liveness — itself a behavior change); (c) dropping the
  Connect re-drive consumer — unnecessary, since (b) is feasible
  in-repo.
- Compute Run receipts **shadow** the existing `compute_sessions`
  transitions (`crates/nexus-local-db/src/compute_runs.rs:166-300`:
  running→succeeded/failed, accept/discard stay behavior-preserving); no
  `compute_sessions` schema change.
- Read path: core handle `get_operation_receipt(operation_id)`; wire DTO
  `CoreOperationReceipt` from a new
  `schemas/core/core-operation-receipt.schema.json` (columns above,
  status enum per B.1/B.2).
- Design precedent (semantics only, no schema change — compass non-goal):
  mstar `execution_lease` claim/renewal/terminal-settlement + revision
  CAS; the in-repo durable journal precedent is
  `CoreProviderOperation` / `CoreProviderJournalWrite`.

### B.3 Recover handshake order (frozen)

1. **Resolve the id** — derive (B.1) or take the caller-supplied
   `operation_id`.
2. **Ask the receipt store first** — `get_operation_receipt(operation_id)`
   BEFORE any re-apply. Recovery entry points: compute rows stuck
   `running` at boot; Connect re-drive after cancel/timeout.
3. **Decide by receipt**:
   - terminal receipt (`finished`/`failed`/`cancelled`/`interrupted`) ⇒
     answer the replay **from the receipt**; no re-apply (no
     double-apply);
   - `running` receipt with a live owner ⇒ typed Busy / in-progress
   - no receipt ⇒ safe to apply exactly once: write the receipt row
     (`status = running`) FIRST, run the effect, then settle terminal.
     On the Connect surface the FIRST write runs on the host's existing
     cooperative DIRECT pool through the same `begin_operation` /
     `settle_operation` store functions (§B.2 admission ruling) — no
     engine ownership is taken and no second pool is opened.
4. **Ambiguity fails typed, never blind**:
   - receipt write failed after the effect committed ⇒ typed
     effect-committed / not-retryable answer (precedent:
     `crates/nexus-core-node/src/admitting_provider_port.rs:41-62`
     `effect_committed_failure`);
   - non-idempotent write without a terminal receipt ⇒ typed
     `uncertain`/`blocked` answer to the caller — never a blind retry.

## §C Frozen cross-plan receipt-wire field ownership (P2 transport ↔ P3 semantics)

One owner per field; named once here. A field's owner is the only plan
that may change its wire name or meaning; the other plan references this
section.

| wire field | owner | defined in | carried by |
|------------|-------|------------|------------|
| `operation_id` | **P3** | §B.1 (syntax + derivation) | P2 envelopes/gap events carry it **verbatim**, never rename or interpret |
| `consumer`, `status`, `subject_id`, `request_fingerprint`, `result_json`, `error_json`, `created_at`, `updated_at`, `terminal_at`, `sequence` | **P3** | §B.2 (`CoreOperationReceipt` / `operation_receipts`) | receipt store + receipt read DTO only |
| `id`, `event`, `data` (SSE frame) | **P2** | §A.2 (`SseFrame` dialect); wire carriage §A.2a(b) | Connect event stream — delivery batches (`frames[]`) |
| `last_event_id` | **P2** | §A.3 | subscribe request payload |
| `stream` | **P2** | §A.2a | subscribe request `args`, subscribe response, delivery arguments |
| `epoch`, `resumed_from` | **P2** | §A.2a | subscribe response `result` |
| `frames` (bounded frame batch) | **P2** | §A.2a(b) | delivery arguments of `tools.nexus.deliver_events` |
| `tools.nexus.deliver_events` (delivery op / capability id) | **P2** | §A.2a, §A.2a(f)4 | host→consumer reverse-invoke op; consumer hello `capabilities[]` (serve declaration) AND host WS-lane hello `capabilities[]` (reverse-use session-scope carriage — the intersection both sides' dispatch gates read); consumer-served: enters no host-served set on either lane |
| `tools.nexus.subscribe` (subscribe op / capability id) | **P2** | §A.2a(f) | consumer→host subscribe invoke op on the WS accept/responder lane; advertised in the WS-lane hello `capabilities[]` (host) and the consumer hello (opt-in); enters no node-lane set |
| `reason`, `requires_transcript_reconciliation`, `resync_required`, `inspect_url` | **P2** | §A.4 (`CoreConnectGapEvent`) | gap event |
| `operation_id_conflict` / `uncertain` / `operation_in_progress` refusal codes | **P3** | §B.1, §B.3 | typed refusals on any consumer surface (`operation_in_progress` = §B.3's typed Busy answer; ratified at v1.207 plan QC revalidation 2026-10-07) |

P2 never defines receipt semantics or a competing receipt shape; P3 never
defines SSE frame fields. Neither plan blocks the other's implementation
beyond this table.

## Source anchors (HEAD 2026-10-07)

- `docs/nexus-runtime.md:53-61` — boot via `connect::build_host_config`;
  `:68` no HTTP in the runtime process; `:85-103` served ops + gates;
  `:130-135` hello capabilities intersection; `:141-143` fail-closed
  `op_unsupported` before host handlers.
- `apps/nexus42/src/commands/connect/invoke.rs:230` — `SERVED_OPS` const;
  `:486` — served-op dispatch gate.
- `apps/nexus42/src/commands/connect/interop.rs:534-601` — manifest ⇔
  served-ops honesty machine check.
- `apps/nexus42/src/commands/connect/allowlist.rs:323-325` — op_scope
  exact-membership gate.
- `crates/nexus-core/src/connect/accept.rs:187` — `daemon_manifest`;
  `:259` — `spawn_accept_loop`; `:410` — `responder.remote_manifest()`
  admission read (§A.2a reverse-leg negotiation check);
  `crates/nexus-core/src/connect/ws_transport.rs` — the WS lane (`:5-7` one
  envelope = one WS message, opaque bytes — frame-kind discrimination lives
  above the transport seam).
- spoke-connect 0.14.1 (registry crate; frozen contract §6):
  `remote/responder.rs:478` — request-id-keyed pending reverse-invoke
  waiters; `:567` — tool serving + reverse invoke section; `:596-608` —
  reverse tool-invoke face (`{arguments}` payload convention, deny →
  `CAPABILITY_PORT_MISSING` mapping); `:602` — `ConnectResponder::invoke_tool`;
  `remote/remote_adapter.rs:554` — dialer-side `register_tool_handler`;
  `:809` — reverse-invoke serving pipeline.
- Workspace pin/feature evidence for the seam decision: `Cargo.toml:240`
  — `spoke-connect = "=0.14.1"` (external, not editable in-repo);
  `apps/nexus42/Cargo.toml:103` — nexus42 builds spoke-connect WITHOUT
  `remote-adapter`; `crates/nexus-core/Cargo.toml:79` — nexus-core enables
  it.
- spoke-connect 0.14.1 (composition-seam evidence): `config.rs:51` —
  frozen synchronous `InvokeHandlerV2`; `node.rs:1646-1784` —
  `SpokeConnectNode` public API (no responder/reverse-invoke handle);
  `remote/responder.rs:580` — `register_tool_handler`; `:1069-1073` /
  `:1093` / `:1103-1111` — `tools.*` serving arm + `dispatch_tool_invoke` +
  registered-or-deny; `:1084` — `gate_allows`; `:734-745` — negotiated =
  hello intersection; `:1116-1121` — `payload.arguments` extraction;
  `:1147` — `send_tool_result`; `core/dispatch.rs:40-42` —
  `tools.<ns>.<id>` requires its own id negotiated;
  `remote/remote_adapter.rs:414-415` — async `ToolHandler`; `:927` —
  consumer-side reverse gate (sequence + envelope-auth).
- spoke-connect 0.14.1 (reverse-invoke authorization, fourth amendment):
  `remote/remote_adapter.rs:1707-1710` — dialer-side negotiated set =
  both-hello `capabilities[]` intersection (`:388-392` field doc: the
  dispatch gate for inbound invokes; responder-side mirror
  `remote/responder.rs:734-745`); `remote/remote_adapter.rs:1036-1063` —
  the reverse-invoke dispatch gate (`dispatch_allowed(op, &negotiated)`
  → `op_unsupported`, "not authorized by this session");
  `core/dispatch.rs:40-43` / `:57-62` — a `tools.*` op requires its own
  op string in the negotiated set, fail-closed otherwise.
- `crates/nexus-core/src/connect/accept.rs:71-109` — `ObservedTransport`
  decorator seam (subscribe-response ordering observation point); `:343` —
  per-connection install; `:383` — established peer id; `:405-442` —
  Phase 2 admission window (handler registration + driver spawn point);
  `:451-473` — Phase 3 close observation; `:519` — `start_peer_tools_lane`
  (no production caller on HEAD); `:550-560` — loopback fail-closed bind;
  `crates/nexus-core/src/connect/session.rs:163` / `:197` —
  `PeerSessionManager.get` / `register` (responder ownership registry).
- `crates/nexus-spoke-adapter/src/manifest.rs:930-1011` —
  no-`tools.*`-beyond-S served-ops lockstep (why the consumer-served
  delivery op never enters `LOCAL_SERVED_OPS`).
- `crates/nexus-spoke-adapter/src/manifest.rs:62` — `ConnectHelloManifest`;
  `:279` `LOCAL_SERVED_OPS`; `:310` `build_local_host_manifest`; `:382` /
  `:419` hello conversions.
- `crates/nexus-core/src/execution/run_events.rs:157` — `SseFrame`;
  `:802-851` — `subscribe_live` replay + cursor parse; `:936-948` —
  bounded page + `resync_required`.
- `crates/nexus-contracts/src/generated/core/core_workflow_subscribe_request.rs:5`
  — cursor syntax `<UUID epoch>:<decimal sequence>`;
  `packages/nexus-contracts/src/generated/core/core-workflow-subscribe-request.ts:18`
  — `last_event_id` field.
- `crates/nexus-core/tests/retained_execution_contracts.rs:5842-5843` —
  control frames carry no cursor.
- `packages/nexus-contracts/src/generated/core/provider-event-batch.ts:105-110`
  — `CoreStreamGap` shape; `crates/nexus-agent-host/src/providers/port.rs:595-598`
  — gap producer (`resync_required: true`).
- `crates/nexus-core-node/src/execution.rs:521-525` — `Last-Event-ID`
  header forwarding (workflow surface, out of scope here).
- `schemas/core/core-provider-operation.schema.json:17-43` — journal
  record shape + terminal-never-downgraded contract;
  `crates/nexus-agent-host/src/providers/multiplex.rs:346-349` —
  operation-id ownership refusal; `crates/nexus-agent-host/src/ids.rs:117-121`
  — random-unique id non-model.
- `crates/nexus-local-db/src/compute_runs.rs:166-300` — compute run
  transitions; `crates/nexus-local-db/migrations/20260912000001_core_writer_protocol.sql:919-974`
  + `:4237-4260` — writer-protocol guard/outbox trigger families.
- `crates/nexus-core-node/src/admitting_provider_port.rs:41-62` —
  `effect_committed_failure` (journal failure after commit = not
  retryable).

- v1.207 P3 branch anchors for the §B.1/§B.2 rulings (Task-3 report
  HEAD `412989b23`, branch `feat/v1.207-p3-op-receipts`):
  `crates/nexus-local-db/migrations/20260919000001_operation_receipts.sql:37-57`
  — table + payload CHECK; `:67-140` — `guard_operation_receipts_*`
  (admission ruled cooperative, §B.2); `:150-164` — terminal
  immutability; `:168-190` — outbox family.
  `crates/nexus-local-db/src/operation_receipts.rs:262` / `:334` /
  `:385` / `:410` — `begin_operation` / `settle_operation` /
  `classify_recovery` / `recover` (admission-agnostic store).
  `crates/nexus-local-db/src/writer_protocol.rs:65-69` — `WriterMode`;
  `:807-811` — Direct holds the shared migration lock, never the
  exclusive engine lock; `:921-963` — migration quiescence of
  cooperative pools.
  `crates/nexus-spoke-adapter/src/adapter/computable_port.rs:934-937`
  and `apps/nexus42/src/commands/connect/invoke.rs:2190-2193` — the
  Connect host's no-engine-ownership lock (untouched by the ruling);
  `invoke.rs:432-435` / `:451` / `:898-975` — the Connect write routes
  (the `LiveWrite` dispatch the receipt lifecycle wires into).
  `crates/nexus-core/src/execution/compute.rs:254-263` — receipt begun
  before the effect; `:1287-1303` — `compute_run_operation_id`
  (`session = run_id`); `:1357-1397` — `recover_stuck_compute_runs`
  asking by `subject_id`; `capabilities.rs:91` —
  `ToolExecuteRequest.session_id`.
