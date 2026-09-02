---
module: nexus-daemon-runtime, nexus-acp-host, apps/nexus42, spoke-connect
date: 2026-08-25
last_updated: 2026-09-02
problem_type: architecture_pattern
category: architecture-patterns
severity: high
plan_id: 2026-08-24-v1.174-p0-peer-tools-transport-registry
tags: 
  - peer-tools
  - mcp
  - dispatch-spine
  - peer-tool-table
  - admission
  - authz
  - session-lifecycle
  - df-85
applies_when:
  - "Extending the daemon tool registry with a new origin or dispatch arm"
  - "Adding an MCP transport (embedded service, streamable HTTP) onto the exposure lane"
  - "Adding admission filters, session-cap tuning, or eviction semantics to the peer lane"
  - "Routing or dispatching any tool id (peer, user-cap, builtin) through the daemon spine"
---

# Peer-tool two-lane capability architecture: WS registration + MCP exposure over one dispatch spine

## Context

DF-85 (consumer side) landed in V1.174 as a **two-lane product**: a
**registration lane** where integrator spokes dial nexus over real WebSocket
with a spoke `RemoteAdapter` and register arbitrary custom tools (advertised
via their hello manifest `tools[]`, admitted fail-closed into a
`PeerToolTable`), and an **exposure lane** where nexus serves a scoped
**tools-only MCP server** (`tools/list` + `tools/call`, rmcp 1.8.0, stdio)
over the **full registry** — builtin `nexus.*` rows, user capabilities, and
admitted peer tools — consumable by any MCP client (ACP `newSession.mcp_servers`,
native CLI `--mcp-config`, third-party).

The load-bearing reality that shaped the design: **two registries exist**.
The **orchestration** registry (`nexus-orchestration/src/capability/mod.rs`,
V1.172 graph capabilities + boot-scanned user WASM capabilities) and the
**daemon host-tool registry** (`nexus-daemon-runtime/src/capability_registry.rs`,
`LazyLock`-frozen static rows with plain `fn` pointer handlers). The MCP
catalog is neither of them — it is a read face over the **single dispatch
spine** that resolves across all three origins. The `origin` vocabulary is
four-way: `builtin` ≠ `user` ≠ `peer` ≠ `served` (the connect-host serving
face, V1.173 DF-84, is a separate process and stays frozen).

## Guidance

### 1. Two lanes, one spine — topology

```text
Integrator process                        nexus daemon process
──────────────────                        ─────────────────────
spoke RemoteAdapter (dial side)           TcpListener (behind `connect-client`)
  manifest tools[] + capabilities[]  WS   accept → WsTransport → connect_responder
  register_tool_handler(...)       ─────► handshake → admission → PeerToolTable
                                                                        │
  builtin nexus.* rows (LazyLock static) ──┐                           │
  user capabilities (orchestration reg.) ──┼─► CapabilityRegistry::lookup/dispatch
  peer tools (PeerToolTable) ─────────────┘   (single spine; HTTP face:
                                               POST …/tool-executions;
                                               catalog: GET /v1/daemon/tools)
MCP client (ACP agent / native CLI)             ▲ loopback HTTP
  spawns `nexus42 mcp serve` as stdio child ────┘
```

- `PeerToolTable` (`connect/table.rs`) is **not a second dispatch table**:
  it is consulted *inside* `CapabilityRegistry::lookup()`/`dispatch()`.
  `lookup()` resolves static rows → `PeerToolTable` → the user-capability
  registry; `dispatch()` gains matching branches. An unknown id yields
  `not_supported` exactly like an unknown builtin — same code path, same
  admission gate, no parallel dispatch function.
- The static `TOOL_ALLOWLIST ⇄ static rows` lockstep test keeps comparing
  **static** ids only; peer/user ids never enter that const.

### 2. Registration lane — admission pipeline (fail-closed exact-id)

Per authenticated manifest:

1. `validate_manifest_tools` on the **whole manifest** — failure ⇒ zero
   ingestion from that manifest; the session stays up with zero admitted
   tools (a malformed manifest never tears down an established session).
2. Candidate set = manifest `tools[]`.
3. Named exact-id filters, each refusing at its named layer:
   - spoke grammar `^tools\.[a-z][a-z0-9_-]*\.[a-z0-9][a-z0-9_-]*$`;
   - reserved namespaces: `nexus` refused; an id equal to an existing
     builtin/user capability name refused;
   - negotiated membership (intersection of both hellos' `capabilities[]`);
   - operator allowlist (missing/empty = **default deny**, zero admitted).
4. Admitted entries carry the manifest's input/output JSON Schemas and
   description **verbatim**.

Collision policy: two peers, same id ⇒ later peer refused (first stays
bound; skip + warn). Same-peer reconnect ⇒ evict-then-admit. No
round-robin — `MultiPeerRouter` is a roadmap row (DF-91).

### 3. Outbound authorization — four fail-closed layers, no umbrella

| # | Layer | Where | Rule |
|---|-------|-------|------|
| 0 | Dialer identity | responder handshake | dialer `peer_id` ∈ configured allowlist **and** preconfigured Ed25519 `peer_keys`; miss ⇒ immediate close |
| 1 | Negotiation (spoke, load-bearing) | both hellos | nexus hello `capabilities[]` = baseline ∪ operator-allowlisted tool ids (exact strings); negotiated = intersection; the integrator's adapter denies non-negotiated reverse ops (`op_unsupported`) |
| 2 | Admission (nexus) | `PeerToolTable` ingestion | filter chain in §2 (manifest ∧ negotiated ∧ allowlist ∧ grammar ∧ reserved-ns) |
| 3 | Dispatch (nexus) | Gate 1 + peer/user branches | an id is dispatchable iff it resolves in the spine — and the spine only contains ids that passed §2 / §6 admission |

- **Derivation lock:** nexus hello tool `capabilities[]` derive **only** from
  the operator allowlist config, validated at load (grammar + reserved-ns +
  umbrella rejection; an invalid entry fails config load with a named error).
- **Restart-scoped:** allowlist edits take effect on daemon restart (config
  snapshot read once at subsystem boot; runtime reload = DF-92).
- **MCP is not a second authorization domain:** the MCP child holds no
  allowlist and no policy; every `tools/call` is authorized daemon-side by
  the same layers. Token grants are not used on this lane.

### 4. Session lifecycle — reserve at accept, last-wins replace, evict same-tick

- **In-flight reservation at accept.** The session cap counts *registered*
  sessions only; concurrent accepts can exceed `max_sessions` and a dial
  flood spawns unbounded per-connection handshake tasks. Keep an
  **in-flight counter reserved at accept**, converted to registered on
  handshake success, released on failure/close; pin with a burst test
  (`dial_flood_of_incomplete_handshakes_cannot_exceed_session_cap`).
- **Last-wins replace.** Reconnect = replace: old session closed + its
  entries evicted, fresh admission. Deterministic; never two live sessions
  per peer id.
- **Close observation.** The nexus-owned `WsTransport` wrapper sets a flag +
  `Notify` on first error/close (`responder.state()` poll as fallback) — no
  spoke API changes.
- **Eviction same-tick.** Session end ⇒ every entry of that peer id removed
  from table + listings in the same tick as close observation. In-flight
  invokes resolve as honest refusals — fail-fast, never a hang, never stale
  dispatch.
- **Bounds:** per-invoke bounded by `invoke_timeout_ms` (default 5000,
  config-gated); max concurrent sessions default 8 (config-gated), excess
  closed at accept with logged refusal; max inbound WS message 2 MiB
  default; the accept loop never awaits session work.

### 5. Exposure lane — one catalog, one dispatch face

- **`GET /v1/daemon/tools`** is the spine's read face: rows `{ id,
  description, input_schema, output_schema?, origin: builtin|user|peer }`
  for static `nexus.*` rows ∪ admitted user caps ∪ `PeerToolTable` entries
  (peer merge compiled under `connect-client`; without the feature the route
  lists builtin + user). It is a **new route with its own wire contract** —
  NOT a merge into `GET /v1/daemon/orchestration/capabilities` (different
  registry, different vocabulary).
- **`tools/call` reuses the existing spine HTTP face**
  (`POST /v1/daemon/agent-host/internal/tool-executions`) — no new dispatch
  route.
- **User-capability dispatch arm (standalone path).** User capabilities had
  *no* standalone dispatch path before V1.174: graph→host-tool existed,
  host-tool→orchestration-capability did not. Full-registry exposure forces
  the spine to resolve `origin() == User` capabilities and call
  `Capability::run(arguments)` — a listed-but-uncallable catalog entry would
  violate the honesty lockstep. Orchestration **builtin** caps
  (`judge.llm`, `sync.pull`, …) are graph-internal machinery and are NOT
  exposed on any lane. User-cap catalog admission: name must not start with
  `nexus.`, must not match the peer grammar `^tools\.…`, and
  `input_schema()` must parse as a JSON object (else named refusal).
- **Structural argument gate** (both arms, before any adapter I/O): args
  must be a JSON object with declared top-level `required` keys present;
  else `invalid_input`. Full draft-2020-12 validation is not a runtime
  validator (V1.172 AR-37 posture).

### 6. Typed refusal discriminator (unroutable vs deny)

The refusal vocabulary must stay machine-distinguishable by a **typed
field**, never by message text:

| Class | Spine code | `details.wire_code` |
|-------|-----------|---------------------|
| Unroutable (never admitted / evicted / allowlist-missing) | `not_supported` | **absent** |
| Peer deny (adapter refused) | `not_supported` | **present** (lowercase spoke code, e.g. `op_unsupported`) |
| Structural argument failure | `invalid_input` | — |
| Invoke timeout | `internal` (timeout-named) | — |
| Transport closed mid-invoke | `internal` (disconnect-named) | — |
| User-cap `run()` error | honest failure code (e.g. `service_unavailable` when no executor wired) | — |

On the worker spine path the `WorkerToolResult` wire is code+message only,
so the wire code rides the message (`(wire_code: op_unsupported)`); the HTTP
lane keeps the typed `details.wire_code` DTO.

## Why This Matters

Every tool id — builtin, user, peer — resolves through one `lookup`/`dispatch`
and refuses identically when unknown. That single-table invariant is what
makes the honesty lockstep family machine-checkable in both directions:
admitted ⇄ derivation, catalog ⇄ spine, `tools/list` ⇄ catalog, listing ⇄
table, hello ⇄ allowlist, default-deny, named negatives, duplicate collision,
eviction, reconnect lockstep, static-table pins, and the MCP refusal mapping
(`crates/nexus-daemon-runtime/tests/honesty_lockstep.rs` +
`apps/nexus42/tests/e2e_peer_mcp.rs`). A second dispatch table or an MCP-only
allowlist would silently re-open the authorization surface; the four-layer
chain keeps all policy daemon-side.

## When to Apply

- Adding a new origin or dispatch arm to the spine (keep the single-table
  invariant; no parallel dispatch function).
- Any new MCP transport (embedded Model B, streamable HTTP — DF-87/88) — it
  must ride the same spine + catalog, not a second face.
- Tuning admission (new named filters), session bounds, or eviction
  semantics — keep reserve-at-accept and same-tick eviction.
- Routing/refusal work on any tool id: consult the typed discriminator table
  before writing message-text matches.
- **Visibility ≠ authorization split (V1.180 RN-OGA-2)**: `tools/list`
  visibility (`VisibilityPolicy`, per-consumer subset in `daemon.json`
  `mcp_visibility`, fail-closed parse, slash ids allowed for `fs/*`)
  is an additive layer evaluated at the two MCP construction sites
  (Model A `load_visibility_policy()` in `apps/nexus42/src/commands/mcp/mod.rs`;
  Model B `EmbeddedMcpServer.policy` injected at `establish()`). Execution
  authz stays spine-owned (L0–L3 unchanged). Two refusal classes: a
  **hidden-tool** call is seam-minted `ToolCallOutcome::NotAuthorized` →
  MCP `METHOD_NOT_FOUND` + `tool_not_authorized: {id}` discriminator before
  the backend; a **visible-but-denied** call keeps the spine-shaped
  `ExecutedError`/`DaemonRefused` mapping (wire codes unchanged). Absent
  policy ⇒ byte-identical behavior. `mcp_visibility` is boot-scoped —
  edits surface in `restart_required` (`connect/watch.rs`).

## Examples

- Spine + admission: `crates/nexus-daemon-runtime/src/capability_registry.rs`
  (`dispatch_peer_tool` / `dispatch_user_cap` / `user_cap_catalog_admission`),
  `src/api/handlers/host_tool_executor.rs` (5-gate admission,
  `dispatch_from_worker`), `src/api/handlers/tools.rs` (catalog route).
- Peer lane: `crates/nexus-daemon-runtime/src/connect/{accept.rs, session.rs,
  table.rs, config.rs, identity.rs}`; `connect/table.rs`
  (`McpCatalogRefusal::InputSchemaNotRootObject`).
- Proof suites (all behind `connect-client`): `tests/peer_session.rs` (7),
  `tests/peer_tool.rs` (11), `tests/authz_hello.rs` (7),
  `tests/honesty_lockstep.rs` (4), `tests/worker_spine_peer.rs` (8),
  `apps/nexus42/tests/e2e_peer_mcp.rs` (8).
- Lock spec (iteration snapshot): `.mstar/iterations/v1.174/specs/v1.174-peer-tools-lock.md` (AR-66..77).
- Companions: `architecture-patterns/connect-host-tools-serving.md` (serving
  side, separate process — frozen), `architecture-patterns/wasm-module-as-capability-executor.md`
  (user-cap trio + admission), `architecture-patterns/stateless-mcp-bridge-child.md`
  (the exposure-lane child process).
