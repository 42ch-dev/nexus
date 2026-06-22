# Nexus ACP Capability Set

**Status**: Normative  
**Document class**: Master  

## 0. Document position

This document defines the **minimal ACP-facing capability contract** between **Nexus CLI runtime (ACP Client)** and **user-owned local agents (ACP Agent)**. It is a functional contract layer; it does not freeze ACP wire-level RPC method names or JSON shapes.

The same logical `nexus.*` IDs may appear in **platform REST** contracts for **naming alignment** with CLI capability surfaces; **Nexus Platform does not expose the ACP wire protocol** — ACP is **CLI / local-runtime only** (Nexus as ACP Client ↔ user agents as ACP Agent).

**Normative architecture baseline**:

- **Nexus runtime** participates on the ACP wire only as ACP Client.
- **User-side agent** is the ACP Agent.
- **daemon runtime / daemon** is a local helper / supervisor. It is not an ACP Agent, not an ACP Server, and must not be advertised via ACP Registry as an agent.

Related docs: nexus-platform `v1-spec/architecture.md`, [`cli-spec.md`](./cli-spec.md).

**Naming note**: CLI executable **`nexus42`**; local supervisor is the **daemon runtime** (single-binary mode via `nexus42 daemon start`, crate `nexus-daemon-runtime`). Product name **Nexus** (42ch / Creative Hub). **`nexus.*`** is the stable logical capability ID prefix; capability IDs need not match executable names.

## 0.5 Runtime registry and bridge pointers (V1.53; V1.57)

This spec is the logical catalog for `nexus.*` capabilities. Each entry lists the capability id and a one-line description. **It is not the runtime source of truth for dispatch.** The runtime SSOT is [`capability-registry.md`](capability-registry.md) (Master, V1.54 P-last). The mediated external-agent tool invocation path is now Master-spec [`agent-nexus-tool-bridge.md`](agent-nexus-tool-bridge.md) (promoted from Feature line in V1.57 P-last).

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

```text
[User] -> [nexus42 / daemon runtime] --ACP Client--> [Local/Remote ACP Agent]
               |
               +-> [Nexus Local API / IPC]
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

## 4. Capability roster (V1.59)

> **Roster governance:** This table is the single SSOT for every `nexus.*` capability ID.
> Each row maps to a runtime binding via the `host_tool_registry()` (daemon host tools)
> or the `CapabilityRegistry` (orchestration engine capabilities). Cross-references:
> [`agent-nexus-tool-bridge.md`](agent-nexus-tool-bridge.md) (Master — mediated external-agent tool path),
> [`capability-registry.md`](capability-registry.md) (Master — runtime dispatch contract).
>
> **Status tags**: `shipped` (runtime handler bound), `scaffold-equivalent` (§3.3 metadata, not an action ID), `OUT` (explicitly non-implemented), `catalog-only` (logical contract; runtime binding deferred or in orchestration engine), `deferred-to-V2.0+` (platform-gated).

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
| `nexus.world.state.query` | Query KB/timeline slices needed for reasoning | catalog-only | — | orchestration |
| `nexus.timeline.recent.get` | Fetch recent timeline tail for continuity | shipped | V1.53 P1 | `host_tool` |
| `nexus.kb_snapshot.read` | Focused KB snapshot read | shipped | V1.53 P1 | `host_tool` |
| `nexus.world.delta.propose` | Produce structured proposed delta package | catalog-only | — | orchestration |
| `nexus.world.delta.apply` | Apply staged deltas locally under policy | catalog-only | — | orchestration |
| `nexus.timeline.event.append` | Append new events; must not silently rewrite canon history | catalog-only | — | orchestration |
| `nexus.fork.create` | Explicit branch creation when rewrite-past is intended | catalog-only | — | orchestration |
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

- Map each logical `nexus.*` capability to concrete ACP tool/resource schemas.
- Decide whether `world.delta.apply` is agent-side or runtime-side by default.
- Define maximum manuscript read/write quotas and default timeouts.
