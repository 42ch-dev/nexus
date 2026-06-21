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

## 0.5 Runtime registry pointer (V1.53)

This spec is the logical catalog for `nexus.*` capabilities. Each entry lists the capability id and a one-line description. **It is not the runtime source of truth for dispatch.** The runtime SSOT is [`capability-registry.md`](capability-registry.md) (Draft overlay, V1.53).

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

### 3.3 Capability sets

| Profile | Purpose | Notes |
| --- | --- | --- |
| `nexus.profile.minimal` | Smoke tests / doctor | read-only context + health |
| `nexus.profile.writer` | V1.0 default | world read + delta propose + manuscript bounded write + sync helpers |
| `nexus.profile.publisher` | Explicit publish flows | includes `publish.*` gated capabilities |

---

## 4. Minimal capability set

> **V1.34 implementation subset:** Daemon host tools for `nexus.context.whoami`, `nexus.workspace.info`, `nexus.work.get`, `nexus.work.patch`, `nexus.orchestration.schedule_status`, and `nexus.context.assemble` (local/read-only or `policy_blocked`) are specified in [agent-nexus-tool-bridge.md](agent-nexus-tool-bridge.md). Remaining IDs in this section remain **logical contract only** until DF-46.

### 4.1 Context

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.context.whoami` | yes | Resolve active Nexus profile / creator context |
| `nexus.workspace.info` | yes | Workspace root, linked world ref, environment flags |
| `nexus.workspace.paths` | yes | Enumerate allowed roots from the active preset: e.g. V1.36 `novel-writing` **`Works/<work_ref>/`** (正文 under **`Stories/`**), **`.nexus42/references/<run-id>/`**, workspace `.agents/skills/`, and other policy-defined subtrees — see [novel-writing/workflow-profile.md](./novel-writing/workflow-profile.md) |
| `nexus.context.assemble` | yes | Assemble a stable writing context from confirmed KB / canon timeline / memory slices |

### 4.2 World read

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.world.snapshot.get` | yes | Consistent read of structured world snapshot |
| `nexus.world.state.query` | yes | Query KB/timeline slices needed for reasoning |
| `nexus.timeline.recent.get` | yes | Fetch recent timeline tail for continuity |
| `nexus.kb_snapshot.read` | optional | Focused KB snapshot read; conceptual equivalent of `kb_snapshot_read` in notes/design docs |

### 4.3 World mutation

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.world.delta.propose` | yes | Produce structured proposed delta package |
| `nexus.world.delta.apply` | optional | Apply staged deltas locally under policy |
| `nexus.timeline.event.append` | yes | Append new events; must not silently rewrite canon history |
| `nexus.fork.create` | optional | Explicit branch creation when rewrite-past is intended |

### 4.3A World CLI write (V1.54 — DF-46)

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.kb_snapshot.write` | yes | Write/update key blocks for a world (kb edit/adopt) |
| `nexus.world.configure` | yes | Update world metadata (title, visibility, time policy) |

Permission note:

- Public / invited / private world access is determined by world policy, membership, and pairing state; capability presence alone does not grant private access.
- `nexus.kb_snapshot.write` and `nexus.world.configure` require world ownership (creator-level gate).

### 4.4 Sync

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.sync.prepare_push` | yes | Build idempotent push bundle metadata |
| `nexus.sync.push` | yes | Submit structured deltas via runtime-owned client |
| `nexus.sync.pull` | optional | Agent-triggered pull |
| `nexus.sync.status` | yes | Surface outbox / conflicts / cursors |

Platform credentials remain in Nexus runtime, not exported to the agent process in v1.

### 4.5 Manuscript

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.manuscript.list` | yes | List manuscript files under whitelist |
| `nexus.manuscript.read_range` | yes | Read a bounded range for prompting |
| `nexus.manuscript.write` | yes | Write only within whitelist paths and size quotas |
| `nexus.manuscript.phase.get` | yes | Read current manuscript phase |
| `nexus.manuscript.phase.set` | optional | Move between brainstorm / draft / review / finalize with runtime checks |
| `nexus.manuscript.chapter.update` | yes | Update chapter content and block metadata for a work (V1.54 DF-46) |

Runtime note:

- If `output_manuscript=false`, `nexus.manuscript.write` may be skipped by policy, but the capability contract remains available and unchanged.

### 4.6 Publish

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.publish.chapter` | optional | User-attested publish flow for a chapter artifact |
| `nexus.publish.story` | optional | User-attested publish flow for a story artifact |

### 4.6A Research

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.research.query` | optional | Query local-only `ReferenceSource` index / excerpts without syncing source material |

### 4.7B Game-Bible (V1.56 P-last)

| Capability ID | Required | Description |
| --- | --- | --- |
| `game_bible.section_status.update` | yes | Atomically update the `section_status` field in a game-bible `Design/*.md` YAML frontmatter; validates transition (draft → reviewed → accepted) and writes via temp+rename for durability |

**Invocation contract**:

- **Input**: `{ work_ref: string, section_path: string, new_status: "draft" | "reviewed" | "accepted", reason?: string, works_root?: string }`
- **Output**: `{ updated: bool, new_section_status: string, section_path: string }`
- **Transition rules**:
  - `draft → reviewed` — auto-transition after design-writing review pass (GO)
  - `reviewed → accepted` — explicit author accept
  - No skipping (`draft → accepted` rejected with `InputInvalid`)
  - No backwards (`accepted → draft` / `accepted → reviewed` rejected with `InputInvalid`)
  - No self-transition (same-status rejected)
- **Atomicity**: writes to `{tmp}` then renames over the target; no half-written file survives a crash
- **Field preservation**: updates `section_status` and `last_updated` (if present); all other frontmatter fields (`section_weight`, etc.) are preserved unchanged
- **Errors**:
  - `InputInvalid` — invalid transition, unknown status, section not found, missing frontmatter
  - `Internal` — I/O errors, write failures

**Preset integration**: The `design-writing` preset (V1.56 P-last) invokes this capability automatically after a review pass (GO) to transition `draft → reviewed`. The `requires_capabilities` gate ensures the capability is registered before the preset can be loaded.

### 4.7 Observability

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.trace.correlation` | yes | Propagate correlation IDs across tool calls |
| `nexus.runtime.health` | yes | Agent-visible health, registry reachability, sync state |

### 4.7A Registry

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.registry.refresh` | optional | Refresh agent capability registry from embedded snapshot or optional CDN; returns synthetic output by default (sandbox/air-gap safe) with snapshot version, capability count, and source metadata. When `--cdn-url` is configured at daemon start, fetches from CDN with configurable timeout (default 10s) and retry (default 3); falls back to synthetic on network failure. |

#### 4.7A.1 Security contract (V1.56 P1 fix-wave)

`nexus.registry.refresh` enforces the following security invariants regardless of configuration:

- **HTTPS-only**: `--cdn-url` MUST use `https://` scheme. `http://` is rejected at CLI parse time and at runtime `fetch_from_cdn` with `CdnError::InsecureScheme`.
- **No open redirects**: `reqwest::redirect::Policy::limited(0)` — zero redirect hops allowed. Exceeded redirects return `CdnError::TooManyRedirects`.
- **Private-IP / metadata block**: rejected hosts include `127.0.0.0/8`, `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `169.254.0.0/16` (including AWS metadata endpoint `169.254.169.254`), `fc00::/7`, `::1`, and IPv4-mapped IPv6 in private ranges. Enforced at CLI parse (DNS resolution) and at runtime `fetch_from_cdn` with `CdnError::BlockedHost`.
- **Body size cap**: 8 MiB max response body. Exceeded returns `CdnError::BodyTooLarge` (streaming read with byte counter).
- **Typed errors**: failures carry `CdnError` enum variants (`InsecureScheme`, `BlockedHost`, `TooManyRedirects`, `BodyTooLarge`, `Timeout`, `ServerStatus(u16)`, `Parse`, `Io`, `EmptyUrl`, `UrlParse`, `Other`) — not raw strings. `RegistryRefreshOutput.fallback_reason` is a stringified `CdnError` variant, never a raw reqwest error message.
- **Sandbox/air-gap guarantee**: when `--cdn-url` is absent at daemon start, the capability makes zero network calls; `source` field in output is `synthetic`. Network mode is opt-in via the boot-time flag only; runtime-mutable CDN URL is not supported.

#### 4.7A.2 Negative test coverage (V1.56 P1 fix-wave)

Eleven negative tests cover the rejection classes:

- `c_fetch_from_cdn_rejects_http_scheme`
- `c_fetch_from_cdn_rejects_https_with_private_ip` (RFC 5737 docs IP)
- `c_fetch_from_cdn_rejects_https_with_localhost`
- `c_fetch_from_cdn_rejects_https_with_metadata_ip_169_254_169_254`
- `c_fetch_from_cdn_rejects_too_many_redirects`
- `c_fetch_from_cdn_rejects_body_too_large`
- `c_set_cdn_config_rejects_empty_url`
- `c_set_cdn_config_rejects_whitespace_url`
- `c_set_cdn_config_rejects_http_scheme`
- `c_set_cdn_config_rejects_private_ip_at_parse`
- `c_fallback_reason_carries_typed_error`

### 4.8 Work & orchestration write (V1.54 — DF-46)

| Capability ID | Required | Description |
| --- | --- | --- |
| `nexus.work.schedule.set` | optional | Link/unlink schedules to a work (schedule DAO write) |
| `nexus.finding.resolve` | optional | Resolve/close a finding entry (findings DAO write) |
| `nexus.pool.entry.manage` | optional | Add/remove entries from the selection pool (pool DAO write) |

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
