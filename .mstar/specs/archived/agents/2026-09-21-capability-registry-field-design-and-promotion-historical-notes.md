# Capability Registry — Field Design & Promotion Historical Notes

**Provenance**: frozen from [`specs/active/agents/capability-registry.md`](../../active/agents/capability-registry.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-09-21 — the live spec's own "Historical boundary" note ties the supersession of these records to the v1.193 P2 daemon deletion (2026-09-21, commit `e3df40f1`); the promotion checklist itself records the earlier V1.53–V1.57 promotion and is preserved verbatim here. Nothing here is current authority; the current authority is §0.1 of the live spec (`nexus-core` `execution/capabilities.rs`).
**Freeze date**: 2026-10-13

---

## Former §0.1 trailing paragraph (removed)

"**Historical boundary:** the integrated daemon and `HostToolExecutor` were deleted in v1.193 P2. §2 preserves versioned field-design/migration records; §5 preserves the promotion checklist; the dated amendments retain their delivery context. Any `WorkspaceState`, daemon wrapper, worker transport, or Draft-overlay wording in those records is historical, not an alternative current authority."

Also removed from §0.1: the sentence "The former 18-tool count was a V1.57 snapshot." (past-state narration; the current count is 30).

## Former §1.1 scope item (removed)

"Preserve the versioned migration and promotion record without treating it as a second current authority." (That record now lives in this archive file.)

---

## Former §2 (Registry field skeleton — historical design and migration record)

The original field definitions below retain their V1.53–V1.175 context. Current `CapabilityRow` metadata, `RegistryHandlerFn` (using `&ToolContext`, not `&WorkspaceState`), catalog emission and execution are defined in §0.1's core source. Audit ownership is `execute_tool`; the old `HostToolExecutor::execute()` attribution below is historical.

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

For V1.53, all entries used human-readable inline shape descriptions
(e.g. `r#"{"work_id":"string"}"#`). Full ACP-facing JSON Schema drafts
remain **durable roadmap** DR-24.

**V1.175 (DF-89):** `AcpWire` is **removed**. Catalog emission now reads
`CatalogDescriptor { description, input_schema: Option<&'static str>,
output_schema: Option<&'static str> }` on each `CapabilityRow`. Builtin
`input_schema` values are real draft-2020-12 object schemas (or the
named remainder placeholder
`{"type":"object","$comment":"nexus42:schema-pending"}`). The catalog
route (`GET /v1/daemon/tools`) carries those strings verbatim. Schemas
are descriptive — the builtin dispatch path gains no schema-validation
gate this iteration. See
the V1.175 catalog-and-cli lock AR-78 (local record).

**Cross-reference**: `acp-capability-set.md` is the logical catalog
(one-liner per ID). The daemon catalog is the runtime emission of
`CatalogDescriptor`.

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

**Historical V1.58 P1 binding:** the following row predates the V1.58 P3 host-tool addition. Today `build_registry` also binds `nexus.reference.refresh` as a core host tool (`Access::Write`, `ADMISSION_WRITE_WORKSPACE`, `registry_reference_refresh`); the separate orchestration handler remains distinct.

**id**: `nexus.reference.refresh`
**access**: `Read` + side-effect (writes `last_refreshed_at` / `refresh_status` to `reference_sources`)
**admission**: Reference source must exist in `reference_sources` table; `refresh_policy != 'offline'` (else `policy_blocked`); URL must be valid (else `invalid_input`); network timeout returns `transient_error`.
**handler**: `ReferenceRefresh::run()` in `crates/nexus-orchestration/src/capability/builtins/reference_refresh.rs`. Registered in orchestration `CapabilityRegistry` (pool-aware; without pool returns `WorkerUnavailable`). Not registered in `host_tool_registry()` (reference-source-scoped, not ACP-facing).
**ACP wire**: Not ACP-facing — dispatched internally by daemon refresh-scheduler hook and direct capability invocation.
**failure mode**: `PolicyBlocked` when `refresh_policy = 'offline'`; `InvalidInput` when reference source not found or URL is empty; `TransientExternal` on network timeout.
**handler test vector**: ≥1 success (fetch + compare + update) + ≥1 failure (offline source → policy_blocked, not-found → invalid_input, network error → error).


---

## Former §5 (Acceptance — historical promotion checklist)

V1.53–V1.57 promotion checklist, preserved as recorded. Master promotion completed in V1.57 P-last; the unchecked decision row below is historical, not an open authority choice.

- [x] P0 has filled field semantics for all registry fields.
- [x] P0 has recorded explicit cutover triggers and no lingering dual dispatch path.
- [x] P1 has added five read-heavy registry rows and handler test vectors (V1.53).
- [x] P0 (V1.54) has added six write-tool registry rows with admission gate patterns.
- [ ] `acp-capability-set.md` remains catalog-only and points here for runtime SSOT.
- [x] `agent-nexus-tool-bridge.md` §8 documents write-tool dispatch patterns and allocation cache.
- [ ] P-last decides whether this overlay is promoted into a Master or retained as a Draft overlay with a successor plan.

---

## Removed amendment framing (content retained in live spec, present tense)

- §"V1.58 P0" heading and the line "**Historical status**: Draft (V1.58 P0 record; not the status of this shipped Master)."; the intro phrase "gained the following quality hardening in V1.58 P0"; the per-bullet review-finding IDs `(R-V156P1-M003)` … `(R-V156P1-L007)`, `(R-V157P0-L002)`; "(DR-01 landed v1.179)"; and the sentence "Supersedes the former fixed 100–500 ms additive band that let same-generation retries cluster."
- §"V1.59 P0" heading; the lines "**Status**: Shipped (V1.59 P0)" and "**Host tool count**: 21 → 30"; the past-transition framing "transition from `catalog-only` (Registry row ref = orchestration) to `shipped`".
- §"V1.60 P0" heading; the lines "**Status**: Shipped (V1.60 P0)" and "**Orchestration registry count**: 26 → 31"; the past-transition framing "transition from `catalog-only` to `shipped`"; the "(Draft, V1.60 P0)" annotation on the world-delta-propose-apply link.
