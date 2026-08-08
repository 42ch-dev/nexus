---
module: nexus-spoke-adapter, apps/nexus42, nexus-local-db
date: 2026-08-08
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: 2026-08-08-v1.155-p0-n-c3-multi-host-production
tags: [multi-host, peer-bookkeeping, outbound-observation, local-first, host-id, peer-hosts, spoke-connect, n-c3]
applies_when: persisting observations of remote peers/hosts from a local-first node; deciding what identity to key peer state on; exposing a "what have I seen" list without fabricating data
---

# Outbound-Observation Peer Bookkeeping (N-C3 pattern)

## Context

The Connect Host's `HostManifestPort.list_peer_host_capability_manifests` was the
last adapter stub: it always returned `Ok(Vec::new())` (spec §7.3 stub matrix,
residual `R-V1142P1-002`). V1.155 P0 made it production. The design question was
not "how do we store manifests" but **"which peer observations are we allowed
to record, and what is an honest key for them?"** spoke-connect 0.9.2 exposes
the authenticated peer's manifest (`PeerSession::remote_manifest()`) **only on
the outbound path** — `SpokeConnectNode::connect()` return. The inbound invoke
boundary (`InvokeHandlerV2`) receives only `&PeerId`, no manifest, and there is
no inbound-session callback in the public API. A nexus host therefore cannot
honestly describe an inbound-only peer's capabilities in-scope.

## Guidance

### 1. Record ONLY manifest-backed observations — never fabricate

- Peers are recorded **only** from observed outbound Connect sessions: at
  `SpokeConnectNode::connect()` return, where `PeerSession::remote_manifest()`
  is available.
- The production dial trigger is the **`connect dial <multiaddr>` CLI**
  (`apps/nexus42/src/commands/connect/`). `connect start` / `nexus-runtime`
  boot are server-only (`SpokeConnectNode::start` + block, never dial) — they
  must never record. The dial is fail-closed: dial or record error aborts, no
  silent no-op.
- Empty store still returns `Ok(Vec::new())` — the stub contract (empty→empty)
  is preserved; the honesty contract is "never invent peers".
- Inbound-only peers (a peer dials us without being dialed) are **out of
  scope** this iteration: recording them needs a spoke-connect API change
  (e.g. `InvokeHandlerV3(&PeerId, &HostCapabilityManifest, …)` or an
  `on_inbound_session` callback). Do not approximate by writing a row with a
  missing manifest — `manifest_json NOT NULL` is a structural honesty guard.
  File the API gap as a spoke-connect follow-up; the two-node interop test is
  **bidirectional outbound** so both sides record cleanly.

### 2. Key peer state by the claimed `host_id`, not the libp2p `PeerId`

- The manifest's `host_id` (the peer's device id, mirroring the local
  manifest contract) is the `peer_hosts` table PK. A libp2p `PeerId` is **not
  stable per host installation**, so it is not an honest persistent key.
- Consequence (documented contract): a dialed peer may claim another host's
  `host_id` and the upsert overwrites that row's manifest + `last_seen` on the
  recorder. Impact is operator-visibility data only (no authorization path).
  A future iteration should record the session `peer_id` next to `host_id` so
  spoof/collision attempts are detectable via `last_seen` / `peer_id` drift.

### 3. `manifest_json` is the single source of truth — no denormalized columns

- `peer_hosts` table: `host_id` (PK), `manifest_json`, `last_seen` (RFC 3339
  UTC). Capabilities are read from the typed manifest, **never denormalized**
  into a `capabilities` column (QC fix wave F-002 removed the write-only
  column). The manifest is validated as `HostCapabilityManifest` before
  insert; malformed JSON is never stored (fail-closed).
- Recording upsert is idempotent: duplicate `host_id` → fresh `last_seen`,
  never an error.
- No index on `last_seen` needed at this scale: rows are bounded by distinct
  `host_id`s ever dialed (tens of rows); add
  `CREATE INDEX idx_peer_hosts_last_seen ON peer_hosts(last_seen DESC)` only
  when distinct host_ids reach thousands.

## Why This Matters

Local-first multi-host means a host can honestly answer "what peer hosts have
I seen, and what do they claim to be able to do?" — the answer is only as
trustworthy as the observation points that feed it. Recording non-manifest-
backed (inbound) observations would fabricate capability claims; keying by
libp2p `PeerId` would churn identity across reinstalls; denormalizing
capabilities would drift from the typed manifest. The stub contract
(empty→empty) also guarantees an operator with no peers sees an honest empty
list, not a guessed one.

## When to Apply

- Building "list peers I have seen" surfaces on a local-first node whose
  transport only exposes remote metadata on one side of the connection.
- Any persistence keyed by remote identity: prefer the remote's **self-claimed
  stable id** over transport-layer session ids when the transport id is not
  stable per installation.
- Persisting remote capability manifests: store the manifest blob as SSOT and
  read capabilities from it — never mirror them into derived columns.

## Examples

### Recording point (outbound only)

```rust
// apps/nexus42/src/commands/connect/ — production dial surface
let session = node.connect(addr, peer_id).await?;   // outbound
let manifest = session.remote_manifest().await?;     // manifest-backed
adapter.record_dialed_peer(manifest.host_id(), manifest).await?; // upsert, fail-closed
```

### What NOT to do

```rust
// ❌ Inbound invoke boundary: no manifest available in-scope (spoke-connect 0.9.2)
let handler = |peer_id: &PeerId, req: InvokeRequest| { ... };
// ❌ Do NOT record `peer_id` alone with a placeholder/unset manifest
// ❌ Do NOT add a `capabilities` column mirroring manifest_json
```
