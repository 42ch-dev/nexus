# Capability Registry — Master v1

**Status**: Normative / Shipped — Master
**Document class**: Master  
**Created**: 2026-06-20  
**Last updated**: 2026-09-29 — retained core dispatch and static roster
**Scope**: Runtime SSOT for the retained Nexus host-tool registry and dispatch spine — **30 static host tools (28 `nexus.*` + 2 `fs/*`)**, peer/user-capability resolution, admission and audit ownership.
**Coordinates with**: [acp-capability-set.md](acp-capability-set.md), [agent-nexus-tool-bridge.md](agent-nexus-tool-bridge.md), [acp-client-tech-spec.md](acp-client-tech-spec.md), [orchestration-engine.md](../orchestration/orchestration-engine.md) (separate orchestration registry), [local-runtime-boundary.md](../architecture/local-runtime-boundary.md) (current host boundaries)

---

## 0. Document position

This **Master** defines the **shipped runtime registry contract** for Nexus host-tool dispatch. It does **not** replace [acp-capability-set.md](acp-capability-set.md): that spec remains the logical catalog (capability id + one-line description). This registry spec owns handler binding, catalog shape, admission/failure behavior and test-vector authority.

Non-overlap rule: **catalog = ID + one-liner**; **registry = handler + wire + failure mode + test vector**.

### 0.1 Current implementation authority

[`nexus_core::execution::capabilities`](../../../../crates/nexus-core/src/execution/capabilities.rs) owns `host_tool_registry()` and its process-global `LazyLock<CapabilityRegistry>`. `execute_tool` runs admission, calls that registry's `dispatch`, then audits success or refusal. The allowlist uses `spine_resolves`: static builtin → admitted peer → admitted user capability. Unknown IDs fail with `not_supported`; callers must not maintain a second lookup/allowlist table.

The static registry contains **30 rows: 28 `nexus.*` and 2 `fs/*`**. `build_registry` is the row authority; [`retained_peer_contracts.rs`](../../../../crates/nexus-core/tests/retained_peer_contracts.rs), `host_tool_registry_roster_is_the_declared_nexus_surface`, pins the same set and excludes `nexus.profile.*` grouping metadata. Dynamic peer/user entries are additional spine resolutions, not part of this static count.

The [`nexus-orchestration` `CapabilityRegistry`](../../../../crates/nexus-orchestration/src/capability/mod.rs) is a **separate registry** of orchestration `Capability` implementations. Its live admitted user entries may be resolved by the core spine; it does not own the static host-tool dispatch table. ACP **agent discovery/selection** is yet another surface, governed by [registry-integration.md](registry-integration.md), not by this spec.

---

## 1. Scope / non-goals

### 1.1 Scope

- Registry fields needed to route `nexus.*` capabilities consistently.
- Authority chain between catalog, bridge, ACP tech spec, orchestration, and runtime handler code.
- Historical field-design and promotion records live under `specs/archived/agents/`; they are not a second current authority.

### 1.2 Non-goals

- Reopening the completed P-1/P0 field-design and Master-promotion decisions.
- New ACP wire protocol design outside existing ACP-client topology.
- Platform REST contracts, cloud publish, standalone MCP, or third-party registry.
- Skills-export CLI compatibility; DF-50 is Cancelled.

---

## 2. Authority chain

1. Repo root `AGENTS.md` defines scope and local-first boundaries.
2. `acp-capability-set.md` defines the logical capability catalog.
3. This shipped Master defines the runtime registry contract; §0.1 identifies current core ownership.
4. `agent-nexus-tool-bridge.md` defines retained core admission/execution invariants.
5. `acp-client-tech-spec.md` and `orchestration-engine.md` define current ACP provider composition and the separate orchestration capability surface.
6. Runtime implementation must not create a second dispatch table for the same `nexus.*` id.

---

## 3. Boundaries with existing specs

| Existing spec | Boundary |
| --- | --- |
| `acp-capability-set.md` | Logical catalog only; no runtime dispatch authority. |
| `agent-nexus-tool-bridge.md` | Master spec. Retained core admission/execution boundary; this registry is the shared runtime dispatch authority. |
| `acp-client-tech-spec.md` | ACP client behavior and handshake; registry rows may reference wire details but do not redefine ACP. |
| `orchestration-engine.md` | Preset grammar and orchestration capabilities; its registry is separate from the core host-tool registry. |
| `cli-spec.md` | User-visible commands; capability registry is not a CLI command tree. |

---

## 4. `registry.refresh` capability body contract

### Body contract

The `registry.refresh` capability (`crates/nexus-orchestration/src/capability/builtins/registry.rs`)
has the following contract:

- **`force` param wired**: `RegistryRefreshInput.force` is
  parsed and honored. In synthetic mode it is a no-op (embedded snapshot is
  always fresh). In CDN mode it bypasses cache freshness. Logged via
  `tracing::info!(force, ...)`.
- **Tracing spans**: `run()` is wrapped in a
  `tracing::info_span!("registry_refresh", force, cdn_configured, generated_at)`
  covering admission → fetch → response phases.
- **Shared reqwest client**: a `LazyLock<reqwest::Client>`
  (`SHARED_CDN_CLIENT`) with `redirect(Policy::limited(0))` + connection
  pooling is reused across invocations. Per-request timeout applied via
  `.timeout()` on the request builder.
- **Help text**: `registry_refresh_help_text()` documents
  HTTPS-only + public-internet requirement + `force` semantics.
- **Body-size cap configurable**: `CdnConfig.max_body_bytes`
  (default 8 MiB via `DEFAULT_MAX_CDN_BODY_SIZE`); `CdnConfig::new`
  constructor.
- **Retry jitter**: attempt-aware
  full-jitter — the sleep IS the sample, uniform in
  `[0, min(8 000, 500·2^attempt))` ms (`RETRY_BASE_MS=500`,
  `RETRY_CAP_MS=8 000`; cap binds only from attempt ≥ 5), via
  `retry_jitter_ms(attempt)` (SystemTime-nanos entropy, non-cryptographic)
  with injectable seam `retry_jitter_ms_with(attempt, entropy)`.
- **Latency benchmark**:
  `crates/nexus-orchestration/benches/registry_refresh_latency.rs` (cold +
  warm).
- **`generated_at` determinism**: captured once per
  invocation (`now = Utc::now().to_rfc3339()`) before the retry loop.
- **Structured metrics**: AtomicU64 counters —
  `refresh_total`, `refresh_success`, `refresh_failure`,
  `refresh_cache_hit` — with pub readers.

### Failure-path test vectors

Failure-path test vectors for `registry.refresh`:
- `registry_refresh_rejects_invalid_input_type` — non-object input →
  `CapabilityError::InputInvalid`.
- `registry_refresh_rejects_non_boolean_force` — string `force` →
  `CapabilityError::InputInvalid`.
- `registry_refresh_rejects_unknown_field_strictly` — documents the
  serde-default contract (unknown fields ignored, not rejected).

---

## 5. Manuscript & misc capabilities (DF-47 parity set)

These 9 capabilities are `shipped` with a `host_tool` binding in `host_tool_registry()`. Each entry below
documents the runtime contract and per-ID test vectors (success + failure paths).

### `nexus.manuscript.list`

- **id**: `nexus.manuscript.list`
- **access**: `Read`
- **admission**: `ADMISSION_READ_WORKSPACE` (Allowlist, ActiveCreator, WorkspaceBounds, PermissionPolicy, AuditLog)
- **handler**: `execute_manuscript_list` → delegates to `works::list_works`.
- **ACP wire**: `{}` → `{"manuscripts": [{work_id, title, work_ref, work_profile, current_stage, stage_status, total_planned_chapters, current_chapter}], "count": int}`
- **failure mode**: `Forbidden` (missing active creator or workspace).
- **test vectors**:
  - success: `manuscript_list_returns_manuscripts` — returns ≥1 manuscript for active creator.
  - failure: `manuscript_list_rejects_without_active_creator` — `FORBIDDEN` when no active creator.

### `nexus.manuscript.read_range`

- **id**: `nexus.manuscript.read_range`
- **access**: `Read`
- **admission**: `ADMISSION_READ_WORKSPACE`
- **handler**: `execute_manuscript_read_range` → reads chapter body file, applies `[start_line, end_line]` range (1-indexed inclusive).
- **ACP wire**: `{work_id, chapter, volume?, start_line?, end_line?}` → `{work_id, chapter, volume, content, range: {start_line, end_line}, total_lines, truncated}`
- **failure mode**: `InvalidInput` (missing field, bad type); `Forbidden` (cross-creator); `NotFound` (missing chapter or body).
- **test vectors**:
  - success: `manuscript_read_range_returns_bounded_content` — returns lines 2-4 of a 5-line body.
  - failure: `manuscript_read_range_rejects_missing_chapter` — `INVALID_INPUT` when `chapter` absent.

### `nexus.manuscript.write`

- **id**: `nexus.manuscript.write`
- **access**: `Write`
- **admission**: `ADMISSION_WRITE_WORKSPACE`
- **handler**: `execute_manuscript_write` → writes content to chapter body via temp+atomic-rename, updates `actual_word_count`. Enforces `MANUSCRIPT_WRITE_MAX_BYTES` (1 MiB) size quota.
- **ACP wire**: `{work_id, chapter, volume?, content}` → `{written, work_id, chapter, volume, word_count, bytes_written}`
- **failure mode**: `InvalidInput` (missing field, oversized content); `Forbidden` (cross-creator); `NotFound` (missing chapter).
- **test vectors**:
  - success: `manuscript_write_writes_content` — writes 12-word body, returns `written=true`.
  - failure: `manuscript_write_rejects_oversized_content` — `INVALID_INPUT` when content > 1 MiB.

### `nexus.manuscript.phase.get`

- **id**: `nexus.manuscript.phase.get`
- **access**: `Read`
- **admission**: `ADMISSION_READ_WORKSPACE`
- **handler**: `execute_manuscript_phase_get` → delegates to `works::get_work_stage`.
- **ACP wire**: `{work_id}` → `{work_id, phase, stage_status}`
- **failure mode**: `Forbidden` (cross-creator or missing work).
- **test vectors**:
  - success: `manuscript_phase_get_returns_current_phase` — returns `phase="brainstorm"` for seeded work.
  - failure: `manuscript_phase_get_rejects_cross_creator` — `FORBIDDEN` for unknown work_id.

### `nexus.manuscript.phase.set`

- **id**: `nexus.manuscript.phase.set`
- **access**: `Write`
- **admission**: `ADMISSION_WRITE_WORKSPACE`
- **handler**: `execute_manuscript_phase_set` → validates phase against canonical set `[brainstorm, draft, review, finalize]`; enforces forward-transition rule (backward transitions require `force=true`); delegates to `works::update_work_stage`.
- **ACP wire**: `{work_id, phase, force?}` → `{work_id, previous_phase, current_phase, stage_status, transitioned}`
- **failure mode**: `InvalidInput` (invalid phase, illegal backward transition without force); `Forbidden` (cross-creator).
- **test vectors**:
  - success: `manuscript_phase_set_advances_phase` — moves `brainstorm` → `draft`, returns `transitioned=true`.
  - failure: `manuscript_phase_set_rejects_invalid_phase` — `INVALID_INPUT` for non-canonical phase value.

### `nexus.workspace.paths`

- **id**: `nexus.workspace.paths`
- **access**: `Read`
- **admission**: `ADMISSION_READ_CONTEXT`
- **handler**: `execute_workspace_paths` → returns workspace root + allowed roots (`Works/`, `Worlds/`, `References/`, `.nexus42/`).
- **ACP wire**: `{}` → `{workspace_root, allowed_roots: [string], preset_id}`
- **failure mode**: `InvalidInput` (workspace not initialized).
- **test vectors**:
  - success: `workspace_paths_returns_allowed_roots` — returns ≥1 allowed root after `init_workspace`.
  - failure: `workspace_paths_rejects_without_workspace` — `INVALID_INPUT` when `workspace_path()` is `None`.

### `nexus.research.query`

- **id**: `nexus.research.query`
- **access**: `Read`
- **admission**: `ADMISSION_READ_WORKSPACE`
- **handler**: `execute_research_query` → queries `reference_sources` table; supports `reference_source_id` direct lookup or paginated list with optional tag filter.
- **ACP wire**: `{reference_source_id?, tags?, limit?}` → `{results: [{reference_source_id, title, uri, source_type, tags, scan_status}], count}`
- **failure mode**: `InvalidInput`; `NotFound` (unknown `reference_source_id`).
- **test vectors**:
  - success: `research_query_returns_reference_sources` — returns `results` array (empty or populated).
  - failure: `research_query_rejects_unknown_reference_id` — `NOT_FOUND` for unknown `reference_source_id`.

### `nexus.runtime.health`

- **id**: `nexus.runtime.health`
- **access**: `Read`
- **admission**: `ADMISSION_READ_CONTEXT`
- **handler**: `execute_runtime_health` → returns agent-visible health (distinct from `nexus.observability.daemon.health` which exposes uptime/lifecycle). Returns `runtime_mode`, `registry_reachable`, `registry_size`, `sync_state`, `cloud_enabled`, `pool_healthy`.
- **ACP wire**: `{}` → `{runtime_mode, registry_reachable, registry_size, sync_state, cloud_enabled, pool_healthy}`
- **failure mode**: `Forbidden` (missing active creator).
- **test vectors**:
  - success: `runtime_health_returns_agent_visible_status` — returns `registry_size=30`, `cloud_enabled=false`, `sync_state="disabled"` in local-only mode.
  - failure: `runtime_health_rejects_without_active_creator` — `FORBIDDEN` when no active creator.

### `nexus.trace.correlation`

- **id**: `nexus.trace.correlation`
- **access**: `Read`
- **admission**: `ADMISSION_READ_CONTEXT`
- **handler**: `execute_trace_correlation` → echoes incoming `correlation_id` (or generates one if absent) plus `session_id`, `parent_request_id`, `trace_timestamp`. Enables agents to thread trace context through multi-step tool chains.
- **ACP wire**: `{correlation_id?, session_id?}` → `{correlation_id, session_id?, parent_request_id?, trace_timestamp, propagated}`
- **failure mode**: `Forbidden` (missing active creator).
- **test vectors**:
  - success: `trace_correlation_propagates_correlation_id` — echoes `correlation_id`, `session_id`, `parent_request_id`.
  - failure: `trace_correlation_rejects_without_active_creator` — `FORBIDDEN` when no active creator.

## 6. Local orchestration capabilities (DF-46 parity set)

These 5 capabilities are `shipped` with an
orchestration `CapabilityRegistry` handler (Registry row ref = `orchestration`,
**not** `host_tool`). They are registered in
`CapabilityRegistry::{with_builtins, with_builtins_and_pool, with_runtime_deps}`
in `crates/nexus-orchestration/src/capability/mod.rs`, NOT in
`host_tool_registry()`. Each is admission-gated by creator world-ownership
(`ensure_world_owned` — `owner_creator_id` match). Per-ID test vectors (success +
failure + admission gate) live inline in each handler module.

Delta-package semantics and the agent-vs-runtime split are normatively defined
in [`world-delta-propose-apply.md`](../contracts/world-delta-propose-apply.md) (Draft). No new DB migrations — all five reuse existing `narrative_worlds`,
`narrative_timeline_events`, and `kb_key_blocks` tables.

### `nexus.world.state.query`

- **id**: `nexus.world.state.query`
- **access**: `Read`
- **admission**: creator world-ownership (`ensure_world_owned`).
- **handler**: `WorldStateQuery::run()` in `crates/nexus-orchestration/src/capability/builtins/world.rs`. Joined read via `SqliteNarrativeGateway` (world state + timeline) and `SqliteKbStore::list_by_world` (KB blocks).
- **ACP wire**: `{world_id, creator_id, slice?: "kb"|"timeline"|"all", branch_id?, limit?}` → `{world_id, world, kb_blocks: [...], timeline: [...], generated_at}`
- **failure mode**: `Forbidden` (cross-creator); `InputInvalid` (bad input); `WorkerUnavailable` (no pool).
- **test vectors**: `world_state_query_success`, `world_state_query_rejects_cross_creator` (admission gate), `world_state_query_rejects_invalid_input`.

### `nexus.world.delta.propose`

- **id**: `nexus.world.delta.propose`
- **access**: `Read` (no writes — produces a package only)
- **admission**: creator world-ownership.
- **handler**: `WorldDeltaPropose::run()` — reads current state to populate `old_value` per change; no writes.
- **ACP wire**: `{world_id, creator_id, changeset: [{entity, entity_id?, field, new_value, rationale}]}` → `{schema_version, policy_context, proposed_changes: [...], atomic}`
- **failure mode**: `Forbidden` (cross-creator); `InputInvalid` (unsupported entity / bad input).
- **test vectors**: `world_delta_propose_success_populates_old_value`, `world_delta_propose_rejects_cross_creator`, `world_delta_propose_rejects_invalid_input`.

### `nexus.world.delta.apply`

- **id**: `nexus.world.delta.apply`
- **access**: `Write`
- **admission**: creator world-ownership (re-checked inside the transaction — TOCTOU guard).
- **handler**: `WorldDeltaApply::run()` — applies a delta package atomically in one sqlx transaction with a lost-update guard (`old_value` must match the live row). Closes acp §8 line 223 Open Item: **runtime-side**.
- **ACP wire**: `{policy_context, proposed_changes: [...], atomic?}` → `{applied: [{entity, entity_id, field, status: "applied"|"conflict", live_value?, rationale}], atomic_applied, source_work_id}`
- **failure mode**: `Forbidden` (cross-creator); `InputInvalid` (unsupported field/entity); `TransientExternal` (commit failure).
- **test vectors**: `world_delta_apply_title_update_success`, `world_delta_apply_rejects_cross_creator`, `world_delta_apply_lost_update_guard_reports_conflict` (lost-update → `conflict` + rollback).

### `nexus.timeline.event.append`

- **id**: `nexus.timeline.event.append`
- **access**: `Write`
- **admission**: creator world-ownership.
- **handler**: `TimelineEventAppend::run()` in `crates/nexus-orchestration/src/capability/builtins/timeline.rs` — immutable append via `narrative_write::append_event`. New events are always `provisional`; `event_id` collisions are rejected (canon immutability, acp §6).
- **ACP wire**: `{world_id, creator_id, branch_id, event_type, title?, summary?, event_id?}` → `{event_id, sequence_no, status: "provisional", created_at}`
- **failure mode**: `Forbidden` (cross-creator); `InputInvalid` (event_id collision, bad input, sequence conflict).
- **test vectors**: `timeline_event_append_success`, `timeline_event_append_rejects_cross_creator`, `timeline_event_append_rejects_collision`.

### `nexus.fork.create`

- **id**: `nexus.fork.create`
- **access**: `Write`
- **admission**: creator world-ownership.
- **handler**: `ForkCreate::run()` in `crates/nexus-orchestration/src/capability/builtins/fork.rs` — allocates a new `fbk_*` branch id and materializes it with a `fork_created` marker event. **PD-01 boundary**: local timeline branching only; community/social fork is platform-only.
- **ACP wire**: `{world_id, creator_id, parent_branch_id, forked_from_event_id, label?}` → `{branch_id, parent_branch_id, forked_from_event_id, created_at}`
- **failure mode**: `Forbidden` (cross-creator); `InputInvalid` (fork point event not found on parent branch, bad input).
- **test vectors**: `fork_create_success`, `fork_create_rejects_cross_creator`, `fork_create_rejects_bad_fork_point`.
