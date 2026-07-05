---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-22-v1.57-v156-carry-forwards-and-compliance"
verdict: "Approve"
generated_at: "2026-06-22"
---

# QC1 Review — V1.57 P2 V1.56 Carry-Forwards & Compliance

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek-v4-flash
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-22T00:30:00+08:00

## Scope
- plan_id: `2026-06-22-v1.57-v156-carry-forwards-and-compliance`
- Review range / Diff basis: `64a8a9f0..236c34a4` (merge-base: Wave 1 closeout; tip: P2 merge)
- Scope: P2's commit only (`28005f11`)
- Working branch (verified): `iteration/v1.57`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 2 (plus plan stub, compass, and specs for cross-reference)
- Commit range: `28005f11` (P2 implementation)
- Tools run: `git log`, `git diff`, `git show`, `cargo test`, `cargo clippy`, `grep`, code review

## Acceptance Criteria Checklist

- [x] **AC1**: Schema rename applied — `capability_count` in `RegistryRefreshOutput` with `#[serde(alias = "agent_count", alias = "agentCount")]`.
- [x] **AC2**: Reproducer test exists — 5 tests in `crates/nexus-contracts/tests/schema_rename_compliance.rs`.
- [x] **AC3**: CLI surface — response schema at `capability_registry.rs:682` uses `capabilityCount` (canonical camelCase name).
- [x] **AC4**: Agent surface — same reference confirms rename reflected in agent tool bridge output.
- [x] **AC5**: In-scope V1.56 residuals documented as "absorbed in P2" — plan stub §6 cross-cuts table.
- [x] **AC6**: Out-of-scope residues enumerated — plan stub §6: `R-V156P0-M001`, `R-V156P0-M002` deferred to V1.58+.
- [x] **AC7**: `cargo test -p nexus-contracts` — 14 tests passed (5 rename compliance + 9 others).
- [x] **AC8**: `cargo test -p nexus42` — all tests passed.
- [x] **AC9**: `cargo test -p nexus-daemon-runtime` — all tests passed.
- [x] **AC10**: `cargo clippy -p nexus-contracts -p nexus42 -p nexus-daemon-runtime -- -D warnings` — clean.

**AC met: 10 / 10**

## Findings

### 🟢 Suggestion — F-001: Commit message could enumerate deferred residuals

**Severity**: Suggestion → `severity: low`

The merge commit message (`236c34a4`) references the absorbed residual `R-V156P1-M001` but does not explicitly enumerate the deferred residuals (`R-V156P0-M001`, `R-V156P0-M002`). The plan stub §6 already documents the deferred list, so this is **archival completeness**, not a correctness issue. Enumerating deferred residuals in merge commit bodies helps future maintainers understand what was deliberately excluded without needing to cross-reference the plan stub.

**Recommendation**: Future merge commits for carry-forward plans should include both absorbed and deferred residual lists in the body. No action required for this plan — the plan stub is the SSOT.

## Detailed Notes

### Focus: Schema rename `agent_count` → `capability_count` semantic accuracy

**Verdict**: The rename is semantically correct. The field `capability_count` in `RegistryRefreshOutput` represents the number of registered **capabilities** (not ACP agents). The residual `R-V156P1-M001` recommendation has been properly applied.

**Regarding the `agent_count` local variable in `registry.rs:388`**: This variable (`let agent_count = registry.agents.len()`) is architecturally distinct — it counts **ACP agents** in the loaded registry for internal cache bookkeeping. It is **not a schema field** and should retain its name. This is a correctly preserved distinction, not a missed rename opportunity.

### Focus: Backward-compat serde aliases

**Verdict**: Both aliases are appropriate:
- `alias = "agentCount"` — the old camelCase JSON serialized form (canonical under `#[serde(rename_all = "camelCase")]`)
- `alias = "agent_count"` — the old Rust struct field name; defends against hand-crafted JSON or debug output that used snake_case

Both aliases should remain **permanent** for the lifecycle of this struct, or at minimum until all consumers have migrated. The cost is zero (serde aliases have no serialization footprint) and the defensive value is real.

The test design correctly covers this: T4 verifies serialization emits only `capabilityCount` (canonical), while T2/T3 verify both old names deserialize correctly.

### Focus: Reproducer test design

**Verdict**: 5 tests in `schema_rename_compliance.rs` are well-named and thorough:

| Test | What it exercises | Would it fail if rename were undone? |
|------|-------------------|---------------------------------------|
| `test_capability_count_rename_does_not_break_consumers_new_name` | New field name `capabilityCount` deserialize | Yes — field name mismatch |
| `test_capability_count_rename_does_not_break_consumers_old_camelcase` | Old `agentCount` alias deserialize | Yes — alias removed |
| `test_capability_count_rename_does_not_break_consumers_old_snakecase` | Old `agent_count` alias deserialize | Yes — alias removed |
| `test_capability_count_rename_serialization_uses_canonical_name` | Serialization emits only new name | Yes — would emit old name |
| `test_capability_count_rename_both_names_rejected_as_ambiguous` | Ambiguity safety via serde | Yes — duplicate-field check |

All 5 tests pass. The test names follow the AC pattern verbatim (`test_capability_count_rename_does_not_break_consumers_*`), making them filterable with `cargo test test_capability_count_rename`.

**Minor note**: Test names are verbose but this is standard Rust convention for clarity. No change needed.

### Focus: Spec alignment

**Verdict**: No spec amend was required or expected for this plan. The `capability_count` field in `RegistryRefreshOutput` is a **runtime wire field** in the contracts crate — not a spec-level catalog concept in `acp-capability-set.md` §4. The specs that reference this field (`entity-scope-model.md`, `preset-conditional-routing.md`) already use the new name `capability_count`, updated in prior plans (P0/P1). The primary spec reference on the plan stub ("Master — post-roster rename compliance") correctly describes the state: the specs were already updated, and this plan applies the mechanical code change to match.

### Focus: No scope creep

**Verdict**: P2 touched only these files:
1. `crates/nexus-contracts/src/local/orchestration/mod.rs` — the serde alias on `capability_count`
2. `crates/nexus-contracts/tests/schema_rename_compliance.rs` — 5 reproducer tests

No changes to:
- Registry (`nexus-acp-host/src/registry.rs`)
- God-file (`host_tool_executor.rs`)
- Host-call CLI subcommand
- Worker IPC

The git diff excluding `crates/nexus-contracts/` and report paths is **empty**. Zero scope creep.

### AC5/AC6: Residual documentation

The plan stub §6 (Cross-cuts) already documents:
- **Absorbed**: `R-V156P1-M001` (schema rename)
- **Deferred**: `R-V156P0-M001` (sha2 dep) and `R-V156P0-M002` (path canonicalize) — both workspace OCC, V1.58+

This meets AC5 and AC6. No additional commit-level documentation is strictly necessary, though merge commit bodies could list deferred residuals for enhanced auditability (see F-001).

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|-----------|-------------|------------------|------------|
| F-001 | manual-reasoning | Commit `236c34a4` message vs. plan stub §6 | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict: Approve**

All 10 acceptance criteria are met. The schema rename is mechanically correct, the serde aliases are properly scoped, the 5 reproducer tests exercise all relevant paths and would fail if the rename were undone, no spec amend was required, and there is zero scope creep. The single suggestion (F-001) is archival in nature and does not block approval.
