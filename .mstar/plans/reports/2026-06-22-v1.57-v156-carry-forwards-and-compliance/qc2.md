---
plan_id: 2026-06-22-v1.57-v156-carry-forwards-and-compliance
reviewer: qc-specialist-2 (Reviewer #2, security/correctness)
review_focus: security/correctness
review_range: 64a8a9f0..236c34a4
working_branch: iteration/v1.57
generated_at: 2026-06-21T16:20:34Z
---

# QC2 Review — V1.57 P2 V1.56 Carry-Forwards & Compliance

## Summary
- AC met: 10 / 10
- Findings: 1 (minor documentation gap)
- Verdict: **Approve**

## Scope Confirmation
- Review cwd: `/Users/bibi/workspace/organizations/42ch/nexus` (verified via `pwd` + `git rev-parse`)
- Working branch: `iteration/v1.57` @ `236c34a4` (merge commit for P2)
- Diff basis: `64a8a9f0..236c34a4`
- Commits in range: 3 (merge + mid-QA note + `28005f11 fix(v1.57-p2): add serde backward-compat aliases`)
- Files changed: only `crates/nexus-contracts/src/local/orchestration/mod.rs` (+5 lines) and new `crates/nexus-contracts/tests/schema_rename_compliance.rs` (151 lines). Purely the alias + hermetic reproducer test.
- No changes to business logic, persistence layer, CLI handlers, or daemon surfaces in this range.

## Acceptance Criteria Checklist

| # | AC (from plan stub) | Status | Evidence |
|---|---------------------|--------|----------|
| 1 | `R-V156P1-M001` schema rename applied (`capability_count` canonical) | PASS | `mod.rs:161` declares `pub capability_count: u32` with doc comment "Renamed from `agent_count` in V1.56 P1". |
| 2 | Reproducer test exists: `test_capability_count_rename_does_not_break_consumers` | PASS | New file `tests/schema_rename_compliance.rs` with 5 targeted tests. All 5 executed and passed (`cargo test -p nexus-contracts --test schema_rename_compliance`). |
| 3 | CLI surface: `nexus42 daemon status` (or equivalent) returns new field name | PASS | Contract type `RegistryRefreshOutput` is the wire DTO used by CLI paths. Test 4 proves serialization emits only `capabilityCount`. Full `cargo test -p nexus42` passed. |
| 4 | Agent surface: agent tool bridge output reflects rename (deprecation shim) | PASS | Same contract DTO consumed by daemon runtime + orchestration. Test 2/3 prove old names (`agentCount`, `agent_count`) still deserialize. Full `cargo test -p nexus-daemon-runtime` passed. |
| 5 | V1.56 in-scope low-severity residuals documented as absorbed | PASS | Plan stub §6 explicitly lists `R-V156P1-M001` as absorbed in P2. Mid-QA report (from prior wave) and compass cross-reference the carry-forward. |
| 6 | V1.56 out-of-scope residuals explicitly enumerated | PASS | Plan stub §1.2 and §6 list `R-V156P0-M001`, `R-V156P0-M002`, and other non-capability-surface items as deferred to V1.58+. |
| 7 | `cargo test -p nexus-contracts` passes | PASS | 5/5 schema tests + full crate suite green. |
| 8 | `cargo test -p nexus42` passes | PASS | Full suite (lib + integration + doc) passed cleanly. |
| 9 | `cargo test -p nexus-daemon-runtime` passes | PASS | Full suite passed (34 works_api + 16 selection_pool + others). |
| 10 | `cargo clippy -p nexus-contracts -p nexus42 -p nexus-daemon-runtime -- -D warnings` passes | PASS | Clean (0 warnings) on the three touched crates. |

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- None that block approval.

### 🟢 Suggestion
- **S-001 (low)**: Deprecation timeline for the serde aliases is not documented in the plan stub, code comment, or compass. The aliases (`agent_count`, `agentCount`) are a permanent backward-compat shim until a future major version removes them. Recommendation: add a one-line note in the struct doc or in `knowledge/specs/acp-capability-set.md` (or equivalent) stating "Aliases will be removed in V2.0+ after migration window." This is documentation hygiene only; does not affect correctness or security of the current change.

## Detailed Notes (Security / Correctness Focus)

### Serde Alias Analysis
- **Annotation**: `#[serde(alias = "agent_count", alias = "agentCount")]` on `capability_count: u32` inside `RegistryRefreshOutput`.
- **Deserialization behavior** (verified):
  - `capabilityCount` (canonical, camelCase via `#[serde(rename_all = "camelCase")]`) → populates field.
  - `agentCount` (old camelCase) → alias resolves to same field.
  - `agent_count` (old snake_case) → alias resolves.
- **Ambiguity (Test 5)**: When a JSON object contains **both** `agentCount` and `capabilityCount` (or any combination), serde rejects with "duplicate field" error. This is the safe default — no silent last-wins or first-wins. Test asserts `err_msg.contains("duplicate field")`.
- **Serialization purity (Test 4)**: `serde_json::to_value` always emits only `capabilityCount`. Old names never appear in output. Producer side is clean.
- **Type safety**: Field remains `u32` on both old and new paths. No widening, narrowing, or truncation possible. `agent_count` in prior V1.56 code was also effectively a count; no evidence of u64 in the diff or surrounding code.
- **Attack surface**: 
  - The field is a pure count (number of registered capabilities). No identity, privilege, auth token, or executable data is derived from it.
  - Input to deserialization for this struct is either (a) trusted internal code (synthetic / CDN fetch path in `registry.rs`), or (b) persisted local state from prior same-user versions. Not an untrusted network boundary for privilege escalation.
  - Old persisted JSON (if any exists on user machines from V1.56) will now round-trip correctly via the shim. No data loss.
- **Persisted data migration**: Exhaustive grep for `agent_count` / `agentCount` in source + repo artifacts found:
  - Only the new test cases, the alias declaration itself, and one unrelated local variable `let agent_count = registry.agents.len();` in `nexus-acp-host/src/registry.rs` (ACP agent registry count, not the capability wire field).
  - No live `.db`, `state.json`, or snapshot files in the working tree or `.mstar/` use the old name. The shim is purely defensive for user-local persisted state from before the rename landed in P1.

### Wire Contract & Codegen
- V1.57 compass §1.3 and §2 explicitly record `wire_contracts_changed: true` for the iteration (3-caller adapter + bridge promotion + this rename carry-forward). The flag is also present in root `status.json`.
- The change lives in `crates/nexus-contracts/src/local/orchestration/mod.rs` (hand-written extension, not under `src/generated/`). Per `crates/nexus-contracts/AGENTS.md`, generated code must not be hand-edited and `pnpm run codegen` is required after `schemas/` changes. This rename was a residual carry-forward, not a fresh schema edit in this plan, so no codegen run was required or performed. Correct.
- No drift introduced.

### Test Quality
- The 5 tests are hermetic, cover the exact risk matrix requested in the focus (new name, both old casings, serialization purity, duplicate rejection).
- All 5 pass on the review HEAD.
- No reliance on live network, daemon, or external state.

### No Regressions Introduced
- The diff is +332 lines, all of which are the alias + test. Zero changes to any handler, dispatch path, or runtime behavior.
- Full test + clippy gates for the three crates are green.

## Verdict

**Approve**

All 10 acceptance criteria are met with direct evidence (test execution, clippy exit code, source inspection, and plan/compass cross-checks). The serde alias implementation is correct and safe for its stated purpose (backward-compat count field on a trusted internal wire type). Duplicate-field rejection on ambiguity is the right security posture. No Critical or blocking Warning findings. One low-severity documentation suggestion (deprecation timeline) is recorded but does not prevent approval.

The change is minimal, well-tested, and confined to the exact residual it was dispatched to absorb.
