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
