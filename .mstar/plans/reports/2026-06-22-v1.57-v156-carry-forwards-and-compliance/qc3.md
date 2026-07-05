---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-22-v1.57-v156-carry-forwards-and-compliance
verdict: Approve
generated_at: 2026-06-22
---

# QC3 Review — V1.57 P2 V1.56 Carry-Forwards & Compliance

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: deepseek/deepseek-v4-pro
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-22T00:30:00Z

## Scope
- plan_id: 2026-06-22-v1.57-v156-carry-forwards-and-compliance
- Review range / Diff basis: merge-base: 64a8a9f0, tip: 236c34a4
- Working branch (verified): iteration/v1.57
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 4 (2 source/test, 2 mid-QA reports)
- Commit range: 64a8a9f0..236c34a4
- Tools run: cargo test (3 crates × 3 runs), cargo clippy, cargo +nightly fmt, pnpm run codegen, git diff/log

## Summary
- AC met: 10 / 10
- Findings: 0
- Verdict: **Approve**

## Acceptance Criteria Checklist

| # | AC | Status | Evidence |
|---|----|--------|----------|
| 1 | `R-V156P1-M001` schema rename applied | ✅ | `capability_count` field with `#[serde(alias = "agent_count", alias = "agentCount")]` at `crates/nexus-contracts/src/local/orchestration/mod.rs:160`; doc comment documents rename provenance |
| 2 | Reproducer test exists | ✅ | 5 hermetic tests in `crates/nexus-contracts/tests/schema_rename_compliance.rs`: new name, old camelCase alias, old snake_case alias, canonical serialization, ambiguity rejection |
| 3 | CLI surface returns new field name | ✅ | `RegistryRefreshOutput` serialized with `#[serde(rename_all = "camelCase")]` — canonical wire name is `capabilityCount`; daemon status path consumes through `nexus-orchestration` → `registry_output_to_context` which maps `capability_count` into context |
| 4 | Agent surface reflects rename | ✅ | Same `RegistryRefreshOutput` type is consumed by agent tool bridge through orchestration layer; `#[serde(alias)]` handles old-wire-format consumers |
| 5 | In-scope residuals documented as absorbed | ✅ | Plan stub §6 Cross-cuts table: `R-V156P1-M001` — "T1–T4: Mechanical rename + deprecation shim + reproducer test" |
| 6 | Out-of-scope residuals enumerated as deferred | ✅ | Plan stub §6: `R-V156P0-M001` (sha2 dep) + `R-V156P0-M002` (path canonicalize) — "workspace OCC, V1.58+" |
| 7 | `cargo test -p nexus-contracts` passes | ✅ | 107 tests (93 lib + 2 core_context + 3 schedule_types + 4 schema_drift + 5 schema_rename) all passed, 0 failed |
| 8 | `cargo test -p nexus42` passes | ✅ | 762+ tests passed, 0 failed; 3 host_call_smoke ignored (documented — requires live daemon, R-V157P1-W001) |
| 9 | `cargo test -p nexus-daemon-runtime` passes | ✅ | 267+ tests passed, 0 failed; 2 pre-existing test warnings (unused `axum::Json`, unused import `HostToolCallerKind`) — not introduced by P2 |
| 10 | `cargo clippy -p nexus-contracts -p nexus42 -p nexus-daemon-runtime -- -D warnings` passes | ✅ | Clean — no warnings |

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
None.

## Detailed Notes

### Performance Analysis

**Serde alias overhead**: Two `#[serde(alias)]` annotations added to a single field on `RegistryRefreshOutput`. The alias lookup in serde is an O(1) hash-table comparison during deserialization. This type is deserialized only in the `capability/builtins/registry.rs` handler — either from a CDN fetch or as a synthetic fallback — not in a hot loop. Measurable overhead is below baseline noise. ✅

**Reproducer test runtime (flakiness check)**:

| Run | Tests | Passed | Failed | Wall time |
|-----|-------|--------|--------|-----------|
| 1 | 5 | 5 | 0 | <0.01s |
| 2 | 5 | 5 | 0 | <0.01s |
| 3 | 5 | 5 | 0 | <0.01s |

No flakiness across 3 consecutive runs. ✅

### Codegen Drift

`pnpm run codegen` produced zero diffs against committed generated code. Generated TypeScript and Rust types are in sync with schemas. ✅

### DB Migration Assessment

`RegistryRefreshOutput` is constructed at runtime (lines 291–301, 309–321, 327–337 in `capability/builtins/registry.rs`) via `serde_json::to_value()` — it is NOT deserialized from a persisted database file. The struct is never stored to SQLite or a JSON state file. The serde aliases are defensive: they exist for forward/backward compatibility in the CDN wire format. No migration script is needed. ✅

### Non-Regression Verification

**Pre-existing `agent_count` in non-schema code**: `nexus-acp-host/src/registry.rs:388` uses `let agent_count = registry.agents.len()` — this is a LOCAL variable name (count of ACP agents in the host registry), completely unrelated to the schema field `capability_count`. No action needed. ✅

**Test warnings in `nexus-daemon-runtime`**: Two pre-existing warnings (unused `axum::Json` in `findings_api` test, unused import `HostToolCallerKind` in `agent_tool_api` test) were present before P2 and are not introduced by this change. Noted for tracking; does not block approval. ✅

### Scope Verification

The diff is surgical — exactly 2 source files changed:
1. `crates/nexus-contracts/src/local/orchestration/mod.rs`: +5 lines (doc comment + serde aliases)
2. `crates/nexus-contracts/tests/schema_rename_compliance.rs`: +151 lines (new test file)

Plus 2 mid-QA reports (pre-existing Wave 1 reports, read-only for this P2).

No files outside the expected scope were modified. ✅

## Verdict

**Approve**

All 10 acceptance criteria are met. No critical, warning, or suggestion findings. The implementation is surgical (2 files, 156 lines), performance impact is negligible (serde alias O(1) lookup on a non-hot-path deserialization), and backward compatibility is verified via 5 hermetic reproducer tests. No flakiness across 3 test runs. Codegen, clippy, and formatting gates are all clean. No DB migration is required.
