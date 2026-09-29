# Nexus ACP Capability Set

**Status**: Normative  
**Document class**: Master  

## 0. Document position

This document defines the **minimal ACP-facing capability contract** between **Nexus CLI runtime (ACP Client)** and **user-owned local agents (ACP Agent)**. It is a functional contract layer; it does not freeze ACP wire-level RPC method names or JSON shapes.

The same logical `nexus.*` IDs may appear in **platform REST** contracts for **naming alignment** with CLI capability surfaces; **Nexus Platform does not expose the ACP wire protocol** — ACP is **CLI / local-runtime only** (Nexus as ACP Client ↔ user agents as ACP Agent).

**Normative architecture baseline**:

- **Nexus runtime** participates on the ACP wire only as ACP Client.
- **User-side agent** is the ACP Agent.
- **Local hosting** is not an ACP Agent/Server: Electron owns the local TS service lifecycle; `nexus-agent-host` owns provider/session policy; the Rust ACP adapter or `packages/nexus-provider-acp` owns the selected agent connection. These hosts must not be advertised via ACP Registry as agents.

Related docs: nexus-platform `v1-spec/architecture.md`, [`cli-spec.md`](./cli-spec.md).

**Naming note (current):** CLI executable **`nexus42`**; local HTTP service **`apps/nexus-service`** under **`apps/desktop-electron`** lifecycle ownership; headless Connect host **`nexus-runtime`**. The former **daemon runtime** / `nexus-daemon-runtime` / `nexus42 daemon start` naming is historical: that integrated host was deleted in v1.193 P2. Product name **Nexus** (42ch / Creative Hub). **`nexus.*`** remains the stable logical capability ID prefix; IDs need not match executable names. Composition anchors: [`apps/nexus-service/src/index.ts`](../../apps/nexus-service/src/index.ts), [`crates/nexus-core-node/src/lifecycle.rs`](../../crates/nexus-core-node/src/lifecycle.rs); details in [acp-client-tech-spec.md](acp-client-tech-spec.md).

## 0.5 Runtime registry and bridge pointers (V1.53; V1.57)

This spec is the logical catalog for `nexus.*` capabilities. Each entry lists the capability id and a one-line description. **It is not the runtime source of truth for dispatch.** The shipped registry contract is [`capability-registry.md`](capability-registry.md) (Master); code ownership is identified in §4 below. [`agent-nexus-tool-bridge.md`](agent-nexus-tool-bridge.md) (Master) defines the retained core admission/execution boundary and labels the former mediated external-agent/daemon transport as historical.

---

## 1. Goals and non-goals

### 1.1 Goals

- Provide a **minimal, implementable** capability surface for V1.0 story/world advancement.
- Make **capability naming and versioning** stable for registry filtering and skills mapping.
- Document `initialize` handshake assumptions so clients and agents agree on negotiation, correlation, and safety gates.

### 1.2 Non-goals

- Replacing or duplicating the ACP specification.
- Defining platform HTTP APIs or sync delta schemas.
- Declaring Nexus daemon endpoints as ACP capabilities.

---

## 2. Topology

Current ACP clients are the CLI/Rust adapter and the service's admitted provider composition; registry discovery selects agent launch metadata, not a `nexus.*` tool handler. See [registry-integration.md](registry-integration.md) for the current owners.

**Historical topology (integrated-daemon era; retired v1.193 P2):**

```text
[User] -> [nexus42 / daemon runtime] --ACP Client--> [Local/Remote ACP Agent]
               |
               +-> [Nexus Daemon API / IPC]
               +-> [ACP Registry]
```

Hard rule: any “serve ACP to external consumers” pattern is out of scope for Nexus v1.

---

## 3. Capability naming and versioning

### 3.1 Naming convention

Capabilities are identified by dot-separated hierarchical names owned by Nexus:

- Prefix: `nexus.*`
- Grouping mirrors CLI Spec sections: context, world, timeline, sync, manuscript, publish, observability

Examples:

- `nexus.context.whoami`
- `nexus.world.snapshot.get`
- `nexus.world.delta.propose`
- `nexus.timeline.event.append`
- `nexus.fork.create`
- `nexus.sync.push`
- `nexus.manuscript.read_range`
- `nexus.publish.chapter`
- `nexus.observability.health`

### 3.2 Versioning

Each capability carries a semver-style contract version independent of Nexus CLI semver:

- **Major**: breaking input/output semantics or side effects
- **Minor**: backward-compatible additions
- **Patch**: documentation or clarification changes

Agents must declare supported `(capability_id, major)` pairs at minimum. Clients may request minor features if present.

### 3.3 Capability sets (profiles)

The three profile-set IDs (`nexus.profile.minimal`, `nexus.profile.writer`, `nexus.profile.publisher`) serve as **§3.3 metadata** (capability grouping) — they are not action IDs. Their status in the §4 roster is `scaffold-equivalent`. See the §4 roster for per-ID detail.

---

## 4. Capability roster (V1.60)

> **Roster governance:** This table is the logical catalog and V1.60 delivery record, not the current executable allowlist.
> Host-tool binding and dispatch are owned by `nexus_core::execution::capabilities::host_tool_registry()` in [`crates/nexus-core/src/execution/capabilities.rs`](../../crates/nexus-core/src/execution/capabilities.rs): `execute_tool` dispatches through it, and `admission_pipeline` uses the same `spine_resolves` authority.
> The [`nexus-orchestration` `CapabilityRegistry`](../../crates/nexus-orchestration/src/capability/mod.rs) is a **separate** registry of orchestration `Capability` implementations, not the owner of the core's static host-tool table. Its admitted user capabilities can participate in the core dispatch spine after builtin and peer lookup.
> The retained core registry has **30 static host tools: 28 `nexus.*` + 2 `fs/*`**; [`retained_peer_contracts.rs`](../../crates/nexus-core/tests/retained_peer_contracts.rs) pins that roster and excludes `nexus.profile.*` grouping metadata. The historical `nexus.reference.refresh` row below records its V1.58 P1 orchestration binding; a core host-tool binding also shipped in V1.58 P3 and is in today's registry. The retained ID `nexus.observability.daemon.health` does not restore the deleted daemon host.
> Cross-references: [`agent-nexus-tool-bridge.md`](agent-nexus-tool-bridge.md) (retained admission/execution boundary and historical agent transport), [`capability-registry.md`](capability-registry.md) (Master — current runtime dispatch contract).
>
> **Status tags (V1.60 delivery snapshot)**: `shipped` (runtime handler bound at that stage), `scaffold-equivalent` (§3.3 metadata, not an action ID), `OUT` (explicitly non-implemented), `catalog-only` (logical contract; runtime binding deferred or in orchestration engine), `deferred-to-V2.0+` (platform-gated). Current binding authority is the code-backed registry above.

| Capability ID | Description | Status | Shipped in | Registry row ref |
| --- | --- | --- | --- | --- |
| `nexus.profile.minimal` | Smoke tests / doctor; read-only context + health | scaffold-equivalent | — | §3.3 metadata |
| `nexus.profile.writer` | V1.0 default profile; world read + delta propose + manuscript bounded write + sync helpers | scaffold-equivalent | — | §3.3 metadata |
| `nexus.profile.publisher` | Explicit publish flows; includes `publish.*` gated capabilities | scaffold-equivalent | — | §3.3 metadata |
| `nexus.context.whoami` | Resolve active Nexus profile / creator context | shipped | V1.34 | `host_tool` |
| `nexus.workspace.info` | Workspace root, linked world ref, environment flags | shipped | V1.34 | `host_tool` |
| `nexus.workspace.paths` | Enumerate allowed roots from the active preset | shipped | V1.59 P0 | `host_tool` |
| `nexus.context.assemble` | Assemble stable writing context from confirmed KB / canon timeline / memory slices | shipped | V1.34 | `host_tool` |
| `nexus.work.get` | Read Work row + stage fields for active creator | shipped | V1.34 | `host_tool` |
| `nexus.work.patch` | Append inspiration; update policy-approved stage_metadata keys | shipped | V1.34 | `host_tool` |
| `nexus.orchestration.schedule_status` | Schedules linked to a work_id; debug / agent planning | shipped | V1.34 | `host_tool` |
| `nexus.world.snapshot.get` | Consistent read of structured world snapshot | shipped | V1.53 P1 | `host_tool` |
| `nexus.world.state.query` | Query KB/timeline slices needed for reasoning | shipped | V1.60 P0 | orchestration |
| `nexus.timeline.recent.get` | Fetch recent timeline tail for continuity | shipped | V1.53 P1 | `host_tool` |
| `nexus.kb_snapshot.read` | Focused KB snapshot read | shipped | V1.53 P1 | `host_tool` |
| `nexus.world.delta.propose` | Produce structured proposed delta package | shipped | V1.60 P0 | orchestration |
| `nexus.world.delta.apply` | Apply staged deltas locally under policy | shipped | V1.60 P0 | orchestration |
| `nexus.timeline.event.append` | Append new events; must not silently rewrite canon history | shipped | V1.60 P0 | orchestration |
| `nexus.fork.create` | Explicit branch creation when rewrite-past is intended | shipped | V1.60 P0 | orchestration |
| `nexus.kb_snapshot.write` | Write/update key blocks for a world (kb edit/adopt) | shipped | V1.54 P0 | `host_tool` |
| `nexus.world.configure` | Update world metadata (title, visibility, time policy) | shipped | V1.54 P0 | `host_tool` |
| `nexus.sync.prepare_push` | Build idempotent push bundle metadata | catalog-only | — | orchestration |
| `nexus.sync.push` | Submit structured deltas via runtime-owned client | catalog-only | — | orchestration |
| `nexus.sync.pull` | Agent-triggered pull | catalog-only | — | orchestration |
| `nexus.sync.status` | Surface outbox / conflicts / cursors | catalog-only | — | orchestration |
| `nexus.manuscript.list` | List manuscript files under whitelist | shipped | V1.59 P0 | `host_tool` |
| `nexus.manuscript.read_range` | Read a bounded range for prompting | shipped | V1.59 P0 | `host_tool` |
| `nexus.manuscript.write` | Write only within whitelist paths and size quotas | shipped | V1.59 P0 | `host_tool` |
| `nexus.manuscript.phase.get` | Read current manuscript phase | shipped | V1.59 P0 | `host_tool` |
| `nexus.manuscript.phase.set` | Move between brainstorm / draft / review / finalize with runtime checks | shipped | V1.59 P0 | `host_tool` |
| `nexus.manuscript.chapter.get` | Read chapter content and block metadata for a work | shipped | V1.53 P1 | `host_tool` |
| `nexus.manuscript.chapter.update` | Update chapter content and block metadata for a work | shipped | V1.54 P0 | `host_tool` |
| `nexus.publish.chapter` | User-attested publish flow for a chapter artifact | OUT | — | DF-59 Backlog |
| `nexus.publish.story` | User-attested publish flow for a story artifact | OUT | — | DF-59 Backlog |
| `nexus.research.query` | Query local-only `ReferenceSource` index / excerpts | shipped | V1.59 P0 | `host_tool` |
| `nexus.trace.correlation` | Propagate correlation IDs across tool calls | shipped | V1.59 P0 | `host_tool` |
| `nexus.runtime.health` | Agent-visible health, registry reachability, sync state | shipped | V1.59 P0 | `host_tool` |
| `nexus.observability.daemon.health` | Daemon runtime status (uptime, lifecycle, registry) | shipped | V1.53 P1 | `host_tool` |
| `nexus.registry.refresh` | Refresh agent capability registry from embedded snapshot or optional CDN | shipped | V1.56 P1 | `host_tool` |
| `nexus.work.schedule.set` | Link/unlink schedules to a work (schedule DAO write) | shipped | V1.54 P0 | `host_tool` |
| `nexus.finding.resolve` | Resolve/close a finding entry (findings DAO write) | shipped | V1.54 P0 | `host_tool` |
| `nexus.pool.entry.manage` | Add/remove entries from the selection pool (pool DAO write) | shipped | V1.54 P0 | `host_tool` |
| `nexus.reference.refresh` | Refresh a reference source body by fetching its URL and comparing content hash; honors refresh_policy (on_change / scheduled / offline) | shipped | V1.58 P1 | orchestration |

> **Note:** `sync.*` runtime binding deferred — tracked DF-46 (§2.3) / PD-05 (§2.1).

### 4.1 Host tool permissions note

Public / invited / private world access is determined by world policy, membership, and pairing state; capability presence alone does not grant private access. `nexus.kb_snapshot.write` and `nexus.world.configure` require world ownership (creator-level gate).

### 4.2 `nexus.registry.refresh` security contract (V1.56 P1 fix-wave)

`nexus.registry.refresh` enforces the following security invariants regardless of configuration:

- **HTTPS-only**: `--cdn-url` MUST use `https://` scheme. `http://` is rejected at CLI parse time and at runtime `fetch_from_cdn` with `CdnError::InsecureScheme`.
- **No open redirects**: `reqwest::redirect::Policy::limited(0)` — zero redirect hops allowed. Exceeded redirects return `CdnError::TooManyRedirects`.
- **Private-IP / metadata block**: rejected hosts include `127.0.0.0/8`, `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `169.254.0.0/16` (including AWS metadata endpoint `169.254.169.254`), `fc00::/7`, `::1`, and IPv4-mapped IPv6 in private ranges. Enforced at CLI parse (DNS resolution) and at runtime `fetch_from_cdn` with `CdnError::BlockedHost`.
- **Body size cap**: 8 MiB max response body. Exceeded returns `CdnError::BodyTooLarge` (streaming read with byte counter).
- **Typed errors**: failures carry `CdnError` enum variants — not raw strings.
- **Sandbox/air-gap guarantee**: when `--cdn-url` is absent at daemon start, the capability makes zero network calls; `source` field in output is `synthetic`.

### 4.3 `game_bible.section_status.update` (V1.56 P-last)

**Invocation contract**: `game_bible.section_status.update` atomically updates the `section_status` field in a game-bible `Design/*.md` YAML frontmatter; validates transition (draft → reviewed → accepted) and writes via temp+rename for durability. Input/output shape and transition rules are documented in the orchestration engine spec. This capability is registered in the orchestration `CapabilityRegistry`, not in `host_tool_registry()`.

---

## 5. `initialize` handshake assumptions

### 5.1 Preconditions

- Transport is established, with JSON-RPC over stdio as V1.0 default.
- Client has selected an agent identity from Registry or local override.

### 5.2 Required negotiation topics

During `initialize` or equivalent bootstrap:

1. ACP protocol version compatibility.
2. `nexus.acp_contract_version` announced by client.
3. Capability intersection returned by agent.
4. Profile selection.
5. Workspace binding and whitelist digest.
6. Safety mode confirmation.

### 5.3 Session metadata

- Every session has a `session_correlation_id`.
- Registry-provided signatures or checksums should be stored if available.

### 5.4 Degraded handshake

If handshake succeeds but capability set is incomplete:

- Client enters **Degraded** mode with explicit user-visible reason.
- Operations depending on missing capabilities are blocked.

---

## 6. Invariants and forbidden behaviors

- No canon history silent rewrite.
- No promotion of provisional facts to shared canon without explicit confirmation path.
- No arbitrary filesystem access.
- No secret exfiltration by default.
- No ACP role inversion.

---

## 7. Change management

- Bump `nexus.acp_contract_version` on breaking capability semantics.
- Registry entries should pin compatible contract version ranges.
- V1.53 retires the skills-export CLI/spec line (DF-50 Cancelled); runtime capability dispatch consistency is governed by [`capability-registry.md`](capability-registry.md).

---

## 8. Open items

> **Durable roadmap:** DR-25 (capability→ACP tool/resource schema map + manuscript read/write quotas + default timeouts).

- ~~Decide whether `world.delta.apply` is agent-side or runtime-side by default.~~ **Resolved V1.60 P0** — runtime-side; see [`world-delta-propose-apply.md`](world-delta-propose-apply.md) §3 (agent proposes, runtime applies under transaction + lost-update guard).
