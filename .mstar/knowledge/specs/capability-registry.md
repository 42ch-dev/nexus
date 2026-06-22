# Capability Registry — Master v1

**Status**: Master (V1.57 P-last promote — bridge Master promotion + P0/P1/P3 spec changes folded in)
**Document class**: Master  
**Created**: 2026-06-20 (V1.53 P-1 Draft)  
**Last updated**: 2026-06-22 (V1.57 P-last — folded in P0 test vectors + P1 3-caller dispatch + P3 dynamic allowlist mechanism)  
**Scope**: Runtime SSOT for Nexus `nexus.*` capability dispatch — 18 host tools (per V1.57 P0 acp §4 roster, reconciled from 35 plan estimate) + dynamic worker allowlist (per V1.57 P3) + 3-caller entry point shape (per V1.57 P1)  
**Coordinates with**: [acp-capability-set.md](acp-capability-set.md), [agent-nexus-tool-bridge.md](agent-nexus-tool-bridge.md) (now Master), [acp-client-tech-spec.md](acp-client-tech-spec.md), [orchestration-engine.md](orchestration-engine.md) (§6.4 worker IPC), [daemon-runtime.md](daemon-runtime.md) (3-caller topology), [local-runtime-boundary.md](local-runtime-boundary.md) (3-caller adapter pattern)  
**Iteration compass**: [v1.57-df46-df47-full-parity-and-adapter-unification-delivery-compass-v1.md](../../iterations/v1.57-df46-df47-full-parity-and-adapter-unification-delivery-compass-v1.md)

---

## 0. Document position

This Draft overlay defines the target runtime registry shape for Nexus `nexus.*` capability dispatch. It does **not** replace [acp-capability-set.md](acp-capability-set.md): the capability-set spec remains the logical catalog (capability id + one-line description). This registry spec is the runtime SSOT for handler binding, ACP wire shape, failure mode, and test-vector coverage.

Non-overlap rule: **catalog = ID + one-liner**; **registry = handler + wire + failure mode + test vector**.

---

## 1. Scope / non-goals

### 1.1 Scope

- Registry fields needed to route `nexus.*` capabilities consistently.
- Authority chain between catalog, bridge, ACP tech spec, orchestration, and runtime handler code.
- Promote-decision checklist for P-last.

### 1.2 Non-goals

- Full field semantics in P-1; P0 owns details.
- New ACP wire protocol design outside existing ACP-client topology.
- Platform REST contracts, cloud publish, standalone MCP, or third-party registry.
- Skills-export CLI compatibility; DF-50 is Cancelled.

---

## 2. Registry field skeleton

| Field | One-line meaning | P0 detail status |
| --- | --- | --- |
| `id` | Stable `nexus.*` capability id. | **Filled (P0).** |
| `access` | Read/write/policy classification used by admission and audit. | **Filled (P0).** |
| `admission` | Ordered fail-closed gates before handler dispatch. | **Filled (P0).** |
| `handler` | Runtime handler binding or adapter entrypoint. | **Filled (P0).** |
| `ACP wire` | Request/response/failure envelope exposed to ACP-facing callers. | **Filled (P0).** |
| `failure mode` | Stable error code/reason contract for denied or failed execution. | **Filled (P0).** |
| `handler test vector` | Required success/failure/admission test vector proving the registry row. | **Filled (P0).** |

### 2.1 `id`

Stable dot-separated capability identifier. Must match one row in the
`acp-capability-set.md` logical catalog if the capability is ACP-facing.
Internal-only capabilities (e.g. `fs/read_text_file`) use the `fs/*`
prefix convention inherited from V1.33.

**Concrete Rust type**: `&'static str`.

**Naming rules**:
- `nexus.*` prefix for Nexus domain capabilities.
- `fs/*` prefix for filesystem proxy tools (V1.33 baseline).
- Flat `nexus.<domain>.<action>` or `nexus.<compound-domain>.<action>`
  (e.g. `nexus.workspace.info`, `nexus.orchestration.schedule_status`).
- For KB reads: `nexus.kb_snapshot.read` (compound domain; resolved in
  P0 KB naming sub-grill — see V1.53 P0 plan §7).

**Cross-validation**: Every registry `id` must have a corresponding
row in `acp-capability-set.md` for `nexus.*` capabilities. Every
`acp-capability-set.md` entry that is implemented as a host tool
must have a registry row. Tests enforce this invariant.

### 2.2 `access`

Classifies the capability's risk profile for admission gating
and audit trail.

**Concrete Rust type**: `enum Access { Read, Write, PolicyGated }`.

| Variant | Meaning | Example |
| --- | --- | --- |
| `Read` | No side effects; read-only data access. | `nexus.context.whoami` |
| `Write` | Mutation-capable; may write to DB, filesystem, or state. | `nexus.work.patch` |
| `PolicyGated` | Access depends on runtime policy (e.g. `permissions.toml` or DA-005 `ContextPermissionGrant`). | `nexus.context.assemble` (platform-gated) |

**Test requirement**: Each row's `access` classification must be
consistent with its handler behavior. A `Read` row must not perform
writes; a `Write` row must include `PermissionPolicy` in its
admission gates.

### 2.3 `admission`

Ordered fail-closed gates executed before the handler is invoked.
If any gate fails, the request is rejected and the handler is
never called.

**Concrete Rust type**: `&'static [AdmissionGate]` where
`enum AdmissionGate { Allowlist, ActiveCreator, WorkspaceBounds, PermissionPolicy, RequireWorldOwnership, AuditLog }`.

**V1.54 P0 T5 optimization**: admission gates are now `&'static [AdmissionGate]` (zero-allocation) using 7 reusable static slices (`ADMISSION_READ_CONTEXT`, `ADMISSION_READ_WORKSPACE`, `ADMISSION_READ_WORLD`, `ADMISSION_WRITE_WORKSPACE`, `ADMISSION_WRITE_WORLD`, `ADMISSION_FS_READ`, `ADMISSION_FS_WRITE`, `ADMISSION_POOL_WRITE`).

**Gate order** (canonical for all V1.34 host tools):
1. `Allowlist` — tool ID must be in the runtime allowlist.
2. `ActiveCreator` — active creator must exist (for `nexus.*` tools).
3. `WorkspaceBounds` — operation must be within workspace boundaries.
4. `PermissionPolicy` — `permissions.toml` must grant the capability.
5. `AuditLog` — audit entry written on all paths (always last; applied by `HostToolExecutor::execute()`, not the registry).

**Test requirement**: Each row must have a test that verifies at
least one admission gate rejection (e.g. unknown tool →
`Allowlist` reject, cross-creator access → `ActiveCreator` reject,
etc.).

### 2.4 `handler`

Runtime handler binding that executes the capability logic.

**Concrete Rust type**: `type RegistryHandlerFn = for<'a> fn(&'a ToolExecuteRequest, &'a WorkspaceState, &'a str) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>>`.

The handler receives the tool request, workspace state, and creator
id (empty string for `fs/*` tools). It returns a boxed future
resolving to either a JSON result or a `NexusApiError`.

**Pattern for sync handlers**: Wrap in `Box::pin(async move { Ok(result) })`.
**Pattern for async handlers**: `Box::pin(original_async_fn(req, state, creator_id))`.

**Test requirement**: Each row must have at least one success test
and one failure test that exercises the handler through the registry's
`dispatch()` method (not by calling the handler directly).

### 2.5 `ACP wire`

Stable contract for the request, response, and error shapes
exposed to ACP-facing callers. Does not redefine ACP protocol —
only documents the JSON shapes.

**Concrete Rust type**:
```rust
struct AcpWire {
    request_schema_ref: &'static str,   // JSON Schema ref or inline shape
    response_schema_ref: &'static str,  // JSON Schema ref or inline shape
    error_schema_ref: &'static str,     // JSON Schema ref or inline shape
}
```

For V1.53, all entries use human-readable inline shape descriptions
(e.g. `r#"{"work_id":"string"}"#`). Full JSON Schema drafts are
deferred to a future plan that introduces schema-aware codegen.

**Cross-reference**: `acp-capability-set.md` is the logical catalog
(one-liner per ID). This field provides the wire contract detail.

### 2.6 `failure mode`

Stable error code/reason contract that a caller can expect when
the capability is denied or fails.

**Concrete Rust type**: `enum FailureMode { NotSupported, PolicyBlocked, Forbidden, InvalidInput, Internal }`.

| Variant | When used | NexusApiError mapping |
| --- | --- | --- |
| `NotSupported` | Capability not in allowlist or not implemented. | `BadRequest { code: "NOT_SUPPORTED" }` |
| `PolicyBlocked` | Admission gate or permissions policy denied access. | `BadRequest { code: "POLICY_BLOCKED" }` |
| `Forbidden` | Authentication/authorization failed (wrong creator, cross-creator access). | `Forbidden` |
| `InvalidInput` | Input validation failed (missing field, wrong type). | `InvalidInput` or `BadRequest { code: "INVALID_INPUT" }` |
| `Internal` | Database error, filesystem error, or unexpected failure. | `Internal` |

**Test requirement**: Each row's primary failure mode must be
verified by at least one test (e.g. `context_assemble_policy_blocked_when_local_only`
verifies `PolicyBlocked` for `nexus.context.assemble`).

### 2.7 `handler test vector`

Descriptor for the minimum test coverage required for each
capability row. Used by test infrastructure (and future
test-generation tools) to verify that every row is tested.

**Concrete Rust type**:
```rust
struct TestVector {
    description: &'static str,        // Human-readable description
    expected_outcome: &'static str,   // "success" or "failure:<reason>"
    test_fn_name: &'static str,       // Test function name (for grep-ability)
}
```

**Test requirement**: Every `TestVector::test_fn_name` must
correspond to an actual `#[test]` or `#[tokio::test]` function
in the repository. A cross-validation test in `capability_registry.rs`
verifies that all 7 fields are populated for every registered row.

### 2.8 `nexus.reference.refresh` (V1.58 P1 — DF-44)

**id**: `nexus.reference.refresh`
**access**: `Read` + side-effect (writes `last_refreshed_at` / `refresh_status` to `reference_sources`)
**admission**: Reference source must exist in `reference_sources` table; `refresh_policy != 'offline'` (else `policy_blocked`); URL must be valid (else `invalid_input`); network timeout returns `transient_error`.
**handler**: `ReferenceRefresh::run()` in `crates/nexus-orchestration/src/capability/builtins/reference_refresh.rs`. Registered in orchestration `CapabilityRegistry` (pool-aware; without pool returns `WorkerUnavailable`). Not registered in `host_tool_registry()` (reference-source-scoped, not ACP-facing).
**ACP wire**: Not ACP-facing — dispatched internally by daemon refresh-scheduler hook and direct capability invocation.
**failure mode**: `PolicyBlocked` when `refresh_policy = 'offline'`; `InvalidInput` when reference source not found or URL is empty; `TransientExternal` on network timeout.
**handler test vector**: ≥1 success (fetch + compare + update) + ≥1 failure (offline source → policy_blocked, not-found → invalid_input, network error → error).

---

## 3. Authority chain

1. Repo root `AGENTS.md` and active iteration compass define scope and local-first boundaries.
2. `acp-capability-set.md` defines the logical capability catalog.
3. This Draft overlay defines the runtime registry contract for active V1.53 work.
4. `agent-nexus-tool-bridge.md` defines mediated external-agent tool invocation and admission invariants.
5. `acp-client-tech-spec.md` and `orchestration-engine.md` define ACP client topology and schedule/tool request participation.
6. Runtime implementation must not create a second dispatch table for the same `nexus.*` id.

---

## 4. Boundaries with existing specs

| Existing spec | Boundary |
| --- | --- |
| `acp-capability-set.md` | Logical catalog only; no runtime dispatch authority. |
| `agent-nexus-tool-bridge.md` | Master spec (promoted V1.57 P-last). Entrypoint/admission history and mediated external-agent tool invocation; registry is the shared runtime SSOT underneath it. |
| `acp-client-tech-spec.md` | ACP client behavior and handshake; registry rows may reference wire details but do not redefine ACP. |
| `orchestration-engine.md` | Schedules and worker tool requests; registry may serve schedule-initiated tool dispatch but does not replace preset grammar. |
| `cli-spec.md` | User-visible commands; capability registry is not a CLI command tree. |

---

## 5. Acceptance (spec-level)

Promote decision checklist for P-last:

- [x] P0 has filled field semantics for all registry fields.
- [x] P0 has recorded explicit cutover triggers and no lingering dual dispatch path.
- [x] P1 has added five read-heavy registry rows and handler test vectors (V1.53).
- [x] P0 (V1.54) has added six write-tool registry rows with admission gate patterns.
- [ ] `acp-capability-set.md` remains catalog-only and points here for runtime SSOT.
- [x] `agent-nexus-tool-bridge.md` §8 documents write-tool dispatch patterns and allocation cache.
- [ ] P-last decides whether this overlay is promoted into a Master or retained as a Draft overlay with a successor plan.

---

## V1.58 P0 Draft overlay: `registry.refresh` capability body extension

**Status**: Draft (V1.58 P0)
**Plans**: `2026-06-22-v1.58-workspace-occ-hardening` (T7–T16, T20)

### Body extension

The `registry.refresh` capability (`crates/nexus-orchestration/src/capability/builtins/registry.rs`)
gained the following quality hardening in V1.58 P0:

- **`force` param wired** (R-V156P1-M003): `RegistryRefreshInput.force` is
  parsed and honored. In synthetic mode it is a no-op (embedded snapshot is
  always fresh). In CDN mode it bypasses cache freshness. Logged via
  `tracing::info!(force, ...)`.
- **Tracing spans** (R-V156P1-M004): `run()` is wrapped in a
  `tracing::info_span!("registry_refresh", force, cdn_configured, generated_at)`
  covering admission → fetch → response phases.
- **Shared reqwest client** (R-V156P1-M005): a `LazyLock<reqwest::Client>`
  (`SHARED_CDN_CLIENT`) with `redirect(Policy::limited(0))` + connection
  pooling is reused across invocations. Per-request timeout applied via
  `.timeout()` on the request builder.
- **Help text** (R-V156P1-L001): `registry_refresh_help_text()` documents
  HTTPS-only + public-internet requirement + `force` semantics.
- **Body-size cap configurable** (R-V156P1-L002): `CdnConfig.max_body_bytes`
  (default 8 MiB via `DEFAULT_MAX_CDN_BODY_SIZE`); `CdnConfig::new`
  constructor.
- **Retry jitter** (R-V156P1-L004): 100–500 ms randomized jitter added to
  the exponential backoff via `retry_jitter_ms()`.
- **Latency benchmark** (R-V156P1-L005):
  `crates/nexus-orchestration/benches/registry_refresh_latency.rs` (cold +
  warm).
- **`generated_at` determinism** (R-V156P1-L006): captured once per
  invocation (`now = Utc::now().to_rfc3339()`) before the retry loop.
- **Structured metrics** (R-V156P1-L007): AtomicU64 counters —
  `refresh_total`, `refresh_success`, `refresh_failure`,
  `refresh_cache_hit` — with pub readers.

### Per-ID test vector extension (R-V157P0-L002)

Failure-path test vectors for `registry.refresh`:
- `registry_refresh_rejects_invalid_input_type` — non-object input →
  `CapabilityError::InputInvalid`.
- `registry_refresh_rejects_non_boolean_force` — string `force` →
  `CapabilityError::InputInvalid`.
- `registry_refresh_rejects_unknown_field_strictly` — documents the
  serde-default contract (unknown fields ignored, not rejected).

---

## V1.59 P0: DF-47 manuscript & misc capability parity batch (9 host tools)

**Status**: Shipped (V1.59 P0)
**Plans**: `2026-06-22-v1.59-df47-manuscript-and-misc-capabilities`
**Host tool count**: 21 → 30

All 9 capabilities transition from `catalog-only` (Registry row ref = orchestration)
to `shipped` with a `host_tool` binding in `host_tool_registry()`. Each entry below
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
