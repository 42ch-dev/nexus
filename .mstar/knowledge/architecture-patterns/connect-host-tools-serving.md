---
module: nexus-spoke-adapter, apps/nexus42
date: 2026-08-22
problem_type: knowledge
category: architecture-patterns
severity: medium
plan_id: 2026-08-22-v1.173-p0-connect-tools-serving
tags: [spoke-connect, connect-host, tools, capability, dispatch, authorization, df-84, host-capability-manifest]
applies_when: serving named schema-described tools over the Connect host; extending the served op surface; adopting a spoke `tools.*` capability
last_updated: 2026-08-22
---

# Connect Host `tools.*` serving (DF-84)

## Context

The Connect Host (`nexus42 connect start`, a separate OS process) advertises a
`HostCapabilityManifest` and serves core ops through `InvokeHandlerV2`. Until
V1.173 the manifest honestly declared `tools: []` (V1.169 AR-1/AR-4) and the
dispatch gate refused every `tools.*` op — even though spoke-connect 0.11.1
already implements the protocol surface: `HostCapabilityManifest.tools:
Vec<ToolDescriptor>` (capability_id / op / description / input / output JSON
Schemas) and a core dispatch rule `required_capability("tools.<ns>.<id>") ==
the op string itself` (exact-match against `negotiated_capabilities`, which is
the **intersection** of both hello `capabilities[]` arrays; capability tokens
are AND, never a substitute).

V1.173 (DF-84) served the first user-locked tool set: `S = {
tools.nexus.list_observed_peers, tools.nexus.list_modules }`.

## Guidance

### 1. Tool id grammar + descriptor discipline

- Ids match `^tools\.[a-z][a-z0-9_-]*\.[a-z0-9][a-z0-9_-]*$`; the second
  segment is the **namespace** (nexus uses `nexus`, already in
  `LOCAL_NAMESPACES`). `capability_id == op` (spoke `validate_manifest_tools`
  MUST — enforced in honesty tests).
- Descriptors are built with `serde_json::from_value` from **locked JSON**
  (spec §2.1) — never hand-built newtypes. `schema_version: 1`; `idempotent`
  omitted unless a tool is intentionally non-idempotent.
- **Advertised input schema is a contract**: both S tools advertise
  `{ "type": "object", "additionalProperties": false }` and the handler MUST
  enforce it structurally (reject non-empty objects with `invalid_input`
  before any adapter I/O) — otherwise the served behavior is weaker than the
  advertised schema (QC S-2).

### 2. Single-source lockstep (manifest ⇔ dispatch ⇔ capabilities)

- `LOCAL_TOOL_OPS` is the single source. `LOCAL_SERVED_OPS` and
  `LOCAL_CAPABILITIES` MUST be **const-block compositions** from
  `CORE_OPS`/`BASELINE_CAPABILITIES` ++ `LOCAL_TOOL_OPS` (a `let mut ops =
  [""; N]; fill` block) — never hand-written literals that duplicate the tool
  ids (QC W-1: plan Interface contract vs implementation drift).
- `apps/nexus42` `SERVED_OPS` imports the same source (no second literal).
- Honesty tests machine-check **both directions**: manifest
  served_ops/capabilities/wire-tools vs dispatch SERVED_OPS routing — a
  served op with no route arm panics the routing sweep; an advertised tool id
  missing from `capabilities[]` fails `validate_manifest_tools`.
- The dispatch match arms re-list the tool ids (a third site); the routing
  sweep is the loud drift guard — a `bridge_fault` tail here means S drifted
  from the arms.

### 3. Authorization is three exact-string layers

1. **spoke core gate**: `required_capability(op) == op` for `tools.*`,
   evaluated against `negotiated_capabilities` (the intersection). A calling
   peer MUST advertise each tool id in **its own** hello `capabilities[]`;
   host advertisement alone is not enough.
2. **capability token AND** with (1) when a grant is in effect — a token
   never substitutes for a missing negotiated id.
3. **`PeerScope.op_scope`** must contain the exact `tools.nexus.*` op
   (allowlist), same exact-membership gate as core ops.

Refusal (`op_unsupported` / `denied`) happens **before** lane permit,
adapter I/O, and any side effect.

### 4. Host-level tools skip the world-scope gate

Today every non-`compute` core op requires a world carrier. Host-level tools
(`list_observed_peers`, `list_modules`) are process-local reads — they MUST
NOT require `world_id` (AR-49). World-scoped future tools (e.g. `list_worlds`)
stay behind the world gate.

### 5. Process-boundary reachability

The Connect host is a separate OS process: handlers can reach `NexusAdapter`
ports (observed peers, workspace SQLite via WAL, host-local module dir) —
**never** daemon HTTP, the daemon-side V1.172 capability registry, or
`ToolInvokePort` (that is DF-85, client-side, evaluated not implemented).
- `list_observed_peers` → `list_observed_peer_hosts`, omit `last_peer_id`,
  empty → `{ "peers": [] }`, never fabricate.
- `list_modules` → scan `~/.nexus42/modules/` via `is_safe_module_id` +
  `<id>/<id>.wasm`+`manifest.json` pair; skip `_`/`.` dirs; return ids only
  (never bytes/paths); missing dir → `{ "modules": [] }`. Does not list the
  daemon capability store.

### 6. Payload shape + integrity

- Request `payload = { "arguments": <object> }`; success `Value =
  { "result": <object> }` — the generic spoke convention so any spoke client
  works without a Nexus-specific envelope.
- Missing/non-object/non-empty `arguments` → `invalid_input` **before** any
  adapter read (pin with a unit test — QC F-001).
- Tools reuse the existing `BridgeLimits` lane and response cap; no second
  pool. Refusal has zero side effects.

## Why this matters

The protocol supports `tools.*` today; the gap was nexus-host serving.
Advertised names + schemas are **integrator contracts** — a mis-served tool or
a drift between manifest and dispatch erodes the honesty posture the whole
Connect surface is built on (N-C0 → N-C3). The single-source composition +
machine-checked lockstep makes drift loud, not silent.

## When to apply

- Extending the Connect served surface (more `tools.nexus.*` tools).
- Landing DF-85 (client-side peer-tool invocation) — the remote layer
  (`remote_adapter` / `multi_peer_router` / `responder`) exists; adoption
  requires `ToolInvokePort`, product-triggered.
- Adding a new spoke `tools.*` namespace (not `nexus`): extend
  `LOCAL_NAMESPACES` + validate_manifest_tools + the honesty family.

## Examples

- `crates/nexus-spoke-adapter/src/manifest.rs` — `LOCAL_TOOL_OPS`,
  `local_tool_descriptors()`, composed `LOCAL_SERVED_OPS`/`LOCAL_CAPABILITIES`
- `apps/nexus42/src/commands/connect/invoke.rs` — `SERVED_OPS` composition,
  `Route::Tool` + `route_tool`, `tool_invalid_input` gates
- `apps/nexus42/src/commands/connect/interop.rs` —
  `ac_v173_1_two_node_interop_for_each_served_tool` + AR-50 authz tests
- Lock spec (iteration snapshot): `.mstar/iterations/v1.173/specs/v1.173-connect-tools-lock.md`
- Companion: `architecture-patterns/connect-host-opt-in-feature-gate.md` (N-C0)
