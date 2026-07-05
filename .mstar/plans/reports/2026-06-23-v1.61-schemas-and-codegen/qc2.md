---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-23-v1.61-schemas-and-codegen"
verdict: "Approve"
generated_at: "2026-06-23"
---

# Code Review Report — qc2 (Security & Correctness)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1 (via OpenCode)
- Review Perspective: Security and correctness risk (JSON Schema validity, additive-only proof, wire contract integrity, compute I/O envelope correctness, injection/untrusted-input surface on state_delta/new_key_blocks, battle_report freeform risk)
- Report Timestamp: 2026-06-23

## Scope
- plan_id: 2026-06-23-v1.61-schemas-and-codegen
- Review range / Diff basis: iteration/v1.61..feature/v1.61-schemas-and-codegen
- Working branch (verified): feature/v1.61-schemas-and-codegen
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 20 (5 schema sources + 11 generated + drift test update + plan/status/README hand-edits)
- Commit range: 9b5450fd (as reported in plan Completion Report)
- Tools run: `git diff`, `git branch --show-current`, `git rev-parse --show-toplevel`, `cargo test -p nexus-contracts --test schema_drift_detection`, `cargo check -p nexus-contracts`, `cargo clippy -p nexus-contracts -- -D warnings`, python jsonschema Draft7Validator instance validation (legacy KB, computable KB, ComputeInput, ComputeOutput), manual schema inspection for $ref / required / additionalProperties / enum constraints.

## Required Reading Verified
- `.mstar/iterations/v1.61-programmable-narrative-progression-delivery-compass-v1.md` §0 (Q3, Q4, Q8, Q11) + §1.3 (wire contracts note)
- `.mstar/plans/2026-06-23-v1.61-schemas-and-codegen.md` (plan + dev Completion Report)
- `schemas/AGENTS.md` + `crates/nexus-contracts/AGENTS.md`
- Actual diff (20 files, 617 insertions)

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None (for P0 schemas+codegen scope).

**Observations (non-blocking for this plan):**
- `state_delta.op` is defined as inline `enum: ["add", "sub", "set"]` in `compute-output.schema.json`. Codegen maps this to `String` in generated Rust (`ComputeOutputStateDelta.op: String`). P3 (state delta merge) intends to implement semantics; the schema is additive and will not block a future typed enum promotion. Documented in plan Completion Report.
- `battle_report` uses `additionalProperties: true` with only `kind` as a documented discriminator. This is an explicit V1 envelope decision (compass Q8). Downstream consumers (P3 `apply_state_delta` + narrative engine) must switch on `kind` and defensively handle unknown shapes. No schema-level enforcement by design. Not a correctness bug in P0.

### 🟢 Suggestion
- (S-V161P0-QC2-001) Consider adding a top-level `description` or `$comment` in `compute-output.schema.json` for `battle_report` explicitly stating "freeform per module manifest; consumers MUST discriminate on `kind`". Current description is adequate but could be more defensive for future readers.
- (S-V161P0-QC2-002) The 4 new compute schemas correctly use draft-07 and cross-$ref to domain/common. For future compute schema evolution, consider pinning a `compute-abi-version` alongside `schema_version` if V2 envelope diverges.

## Evidence — Instance Validation (Additive-Only Proof)
Executed concrete validation with `python jsonschema` (Draft7Validator + RefResolver over https://nexus42.invalid/ URIs):

1. **Legacy KeyBlock (no `state`, no `computable`)**: valid against `schemas/domain/key-block.schema.json`.
2. **Computable KeyBlock (with `body.state` + `body.computable: true`)**: valid.
3. **ComputeInput envelope** (world_ref + key_blocks array containing both legacy + computable KBs + narrative_state + invocation): valid.
4. **ComputeOutput 4-part envelope** (state_delta with add/sub/set + path + value, timeline_events, new_key_blocks: [], battle_report with kind + freeform fields): valid.
5. **Top-level `required` on key-block.schema.json**: `["schema_version", "key_block_id", "world_id", "block_type", "canonical_name", "status", "created_at"]` — `body` is NOT required; `state`/`computable` live inside `body` and are not top-level required. Unchanged from pre-P0.

All $ref URIs (common/common, domain/key-block, domain/timeline-event) resolved successfully. No validation errors on shape or presence of new optional fields.

## Evidence — Wire Contract Integrity + Drift
- `cargo test -p nexus-contracts --test schema_drift_detection`: PASS
  - 56 schemas, 55 structs checked in 10ms (well under 500ms limit)
  - All 4 new compute schemas explicitly registered as `Strict`:
    - `schemas/compute/compute-input.schema.json` → `ComputeInput`
    - `schemas/compute/compute-output.schema.json` → `ComputeOutput`
    - `schemas/compute/entity-attributes.schema.json` → `EntityAttributes`
    - `schemas/compute/entity-state.schema.json` → `EntityState`
  - `schemas/domain/key-block.schema.json` → `KeyBlock` (Strict) still passes (additive fields inside `body` do not break bidirectional match because generated `body` remains `Option<serde_json::Value>` / freeform object in TS).
- `cargo check -p nexus-contracts`: PASS
- `cargo clippy -p nexus-contracts -- -D warnings`: PASS (0 warnings on default targets)
- Generated files present and consistent: `crates/nexus-contracts/src/generated/{compute_input,compute_output,entity_attributes,entity_state}.rs` + matching `.ts` in `packages/nexus-contracts/src/generated/`.
- `pnpm run codegen` was run as part of the plan (per dev Completion Report); no hand-edits under `generated/`.

## Evidence — Compute I/O Envelope Correctness
- `compute-input.schema.json`: required `["schema_version", "world_ref", "key_blocks"]`; `key_blocks` items ref the full KeyBlock schema (so they carry `body.state` / `body.computable` when present). `invocation` is intentionally freeform for module-declared params. `additionalProperties: false`.
- `compute-output.schema.json`: required `["schema_version", "state_delta", "timeline_events", "new_key_blocks", "battle_report"]` — exactly the 4-part envelope specified in compass Q8 and plan T2.
  - `state_delta` items: required `["op", "path"]`; `op` enum `["add","sub","set"]`; `value` untyped (any JSON) to support module-declared state shapes.
  - `timeline_events` / `new_key_blocks`: ref existing domain schemas.
  - `battle_report`: `kind` discriminator + `additionalProperties: true` (intentional).
- No drift between schema and generated types for the envelope.

## Security / Untrusted-Input Surface (P0 Scope)
P0 is schemas-only (no host application logic). However, the shapes that will be applied in P3 were reviewed for obvious over-permissiveness:

- `state_delta[].value`: any JSON — correct for V1 (modules declare their own state shapes via entity-state schema). Host (P3) must still perform path validation, type checks on numeric ops, and target existence before mutating KB state. Schema does not over-authorize.
- `new_key_blocks`: full KeyBlock items — modules can propose new blocks; host will upsert. Schema correctly re-uses the domain KeyBlock contract (including the new `state`/`computable` inside body). No new top-level required fields introduced.
- `battle_report` freeform: per design (V1 envelope escape hatch). Correctness risk is deferred to consumers (documented).
- No credential, path-traversal, or injection vectors exist at the schema layer. The security model (per-invocation sandbox, fuel/memory limits) is described in compass Q6 but is out of scope for P0.

`entity-attributes.schema.json` and `entity-state.schema.json` use `additionalProperties: true` for non-character BlockTypes (placeholders). This is called out in the plan's own residual (R-V161P0-INFO-001) and the schema descriptions. Not a security issue; will be tightened with future modules.

## Source Trace
- Finding source: manual review of diff + schema sources + generated output + drift test registration + concrete instance validation + CI-equivalent commands.
- Primary artifacts inspected: the 4 new `schemas/compute/*.schema.json`, `schemas/domain/key-block.schema.json` diff, `crates/nexus-contracts/tests/schema_drift_detection.rs` (build_schema_map entries), generated Rust/TS, plan Completion Report validation claims.
- Confidence: High (all automated checks + instance validation passed with correct data shapes).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## Additional Notes
- The change is purely additive at the wire level: pre-existing KeyBlocks without `state`/`computable` remain valid; `body` remains `Option<serde_json::Value>` in generated types (no DB migration, no breaking change to existing wire consumers).
- All reviewer alignment fields (plan_id, Working branch, Review cwd, Review range / Diff basis) match the Assignment verbatim.
- No source code, schemas, or generated files were modified during this review (read-only). Report is the only artifact written.
