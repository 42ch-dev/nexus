---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-04-v1.33-creative-brief-intake-preset"
verdict: "Request Changes"
generated_at: "2026-06-04"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-04T00:00:00Z

## Scope
- plan_id: 2026-06-04-v1.33-creative-brief-intake-preset
- Review range / Diff basis: merge-base: 569f79b + tip: 12481ec (equivalent to git diff 569f79b..12481ec)
- Working branch (verified): feature/v1.33-work-experience-loop
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 13
- Commit range: 569f79b..12481ec
- Tools run: cargo check, cargo clippy, cargo +nightly fmt --check, cargo test, rg (atomic write audit)

## Findings

### 🔴 Critical

#### C-V133P2-01: Integration test regression — `registry_has_sixteen_builtins` fails
- **Source**: `crates/nexus-orchestration/tests/capability_registry.rs:13-16`
- **Evidence**: `cargo test -p nexus-orchestration --test capability_registry` fails with:
  ```
  assertion `left == right` failed
    left: 18
   right: 17
  ```
- **Impact**: CI-blocking. The `mod.rs` unit test was updated to `registry_has_eighteen_builtins` (line 341) but the integration test in `tests/capability_registry.rs` was missed. The test name already mismatched reality ("sixteen" but asserting 17); now with `creator.write_brief` added it asserts 17 but gets 18.
- **Fix**: Update `tests/capability_registry.rs:15` to `assert_eq!(reg.len(), 18);` and rename the test to `registry_has_eighteen_builtins` for consistency with `src/capability/mod.rs:341`.

### 🟡 Warning

#### W-V133P2-02: No tracing/observability in new capability and CLI paths
- **Source**: `crates/nexus-orchestration/src/capability/builtins/creator.rs` (write_brief impl, lines 633-684), `crates/nexus42/src/commands/creator/run.rs` (lines 71-286)
- **Evidence**: `rg 'tracing::' crates/nexus-orchestration/src/capability/builtins/creator.rs crates/nexus42/src/commands/creator/run.rs` returns zero matches.
- **Impact**: A multi-turn LLM-driven intake flow (3-4 ACP prompts + capability calls) with no `tracing::info!`, `warn!`, or `debug!` events makes production debugging extremely difficult. If `write_brief` fails validation after a 4-turn conversation, there's no structured log to correlate the failure with the work_id, schedule_id, or brief content hash. Per P1/P3 patterns, capabilities should emit at least `tracing::info!` at entry/exit and `tracing::warn!` on validation failures.
- **Fix**: Add `tracing::info!` at capability entry (with work_id), `tracing::warn!` on validation failure (with anonymized brief size/error), and `tracing::info!` in CLI run.rs for schedule creation success/failure.

#### W-V133P2-03: Brief validation lacks size bounds
- **Source**: `crates/nexus-orchestration/src/capability/builtins/creator.rs:533-584` (`validate_creative_brief`)
- **Evidence**: The validator checks schema correctness (required keys, non-empty strings, array minimum length) but imposes no maximum length on string fields or array cardinality. A malicious or malformed LLM could produce:
  - `constraints` with 10,000 entries
  - `non_goals` with megabyte-long strings
  - `open_questions_resolved` with unbounded arrays
- **Impact**: The brief is stored as a JSON string in SQLite `creative_brief` TEXT column. While SQLite handles large TEXT, the validation step deserializes the entire JSON into memory before checking. Unbounded input could cause memory pressure or OOM during validation, especially since the brief comes from untrusted LLM output.
- **Fix**: Add size guards to `validate_creative_brief`: max total brief_text length (e.g., 64 KiB), max array length per field (e.g., 100 entries), max string length per field (e.g., 4 KiB). Return `CapabilityError::InputInvalid` with a clear size-exceeded message.

#### W-V133P2-04: Intake → production chaining gap
- **Source**: `crates/nexus42/src/commands/creator/run.rs:129-182`
- **Evidence**: `creator run start` creates a Work and schedules `creative-brief-intake`, but after intake completes, the user must manually run a second `nexus42 daemon schedule add --preset novel-writing` command (line 178). There is no daemon-side auto-chaining mechanism.
- **Impact**: Reliability gap. If the user forgets the second command, the Work sits indefinitely with `intake_status=complete` but no production schedule. This is a common user journey failure mode. The CLI output prints instructions, but human-instruction is not a reliable chaining mechanism.
- **Fix**: Acceptable for V1.33, but should be tracked as residual. Future iteration should add daemon-side chaining: when a schedule with `run_intents: [work_init]` completes and the Work has no production schedule, auto-enqueue the primary_preset_id. Document this gap in the plan's residual findings.

#### W-V133P2-05: `write_brief` integration tests miss error paths
- **Source**: `crates/nexus-orchestration/src/capability/builtins/creator.rs:1158-1225`
- **Evidence**: The integration test `write_brief_with_store_roundtrip` only tests the happy path. Missing coverage:
  - Invalid brief rejected by validator with store injected
  - Concurrent brief writes to the same Work (race condition)
  - Brief with extremely long strings (performance/regression)
  - Brief with special characters that might affect SQLite storage
- **Impact**: Low for V1.33, but error paths are the most likely production failures. The standalone tests cover validation failures, but not the DB interaction on rejection.
- **Fix**: Add at least one integration test for `write_brief_with_store_rejects_invalid_brief`.

### 🟢 Suggestion

#### S-V133P2-06: Document intake latency trade-off
- **Source**: `crates/nexus-orchestration/embedded-presets/creative-brief-intake/preset.yaml`
- **Evidence**: The preset defines 3 clarify prompts + 1 synthesize prompt = 4 ACP worker IPC calls minimum. Each ACP prompt involves LLM round-trip latency (typically 2-10s depending on model and token count).
- **Impact**: Total intake latency is ~4-5× a baseline single-turn `novel-writing` run. For V1.33 this is acceptable given the value of structured briefs, but the trade-off should be documented so users and PMs understand why intake takes longer than production.
- **Fix**: Add a latency note to the preset README or CLI help text.

#### S-V133P2-07: Binary size impact negligible
- **Source**: `crates/nexus-orchestration/embedded-presets/creative-brief-intake/`
- **Evidence**: 1 preset YAML (~2.3 KB) + 4 prompt files (~5.2 KB total) embedded via `include_dir!`. Total ~7.5 KB of source text compiled into binary.
- **Impact**: Negligible. No action needed.

#### S-V133P2-08: Atomic write pattern confirmed — not applicable to SQLite path
- **Source**: `crates/nexus-creator-memory/src/memory_io.rs:134-161` (P4 fix), `crates/nexus-orchestration/src/capability/builtins/creator.rs:151-177` (write_memory)
- **Evidence**: The `memory-augmented` preset fix writes `state.generate.output` to LTM via `creator.write_memory` → `nexus_local_db::create_fragment` (SQLite INSERT). SQLite transactions provide atomicity. The P4 `.tmp + rename` pattern in `memory_io.rs` is for filesystem SOUL.md writes, not DB writes. The `soul_io.rs:87` direct `fs::write` is a separate concern (unrelated to this change).
- **Impact**: None. The memory-augmented persist fix does not introduce atomic write risks.

#### S-V133P2-09: Startup cost of dual schedule enqueue acceptable
- **Source**: `crates/nexus42/src/commands/creator/run.rs:119-156`
- **Evidence**: `creator run start` performs 2 HTTP POSTs: `/v1/local/works` (Work creation) + `/v1/local/orchestration/schedules` (intake scheduling). Each is a DB write + daemon registration. Total cost is ~2× single schedule. Intake schedule failure is handled gracefully (non-fatal, line 151-156).
- **Impact**: Acceptable for V1.33. The graceful degradation on schedule failure is good reliability practice.

#### S-V133P2-10: RunIntent enum unchanged — no validation overhead
- **Source**: `crates/nexus-contracts/src/local/orchestration/preset.rs`
- **Evidence**: The `creative-brief-intake` preset uses existing `work_init` intent. No new enum values added.
- **Impact**: No additional validation overhead per preset load.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| C-V133P2-01 | test-failure | `cargo test -p nexus-orchestration --test capability_registry` | High |
| W-V133P2-02 | static-analysis | `rg 'tracing::' creator.rs run.rs` (0 matches) | High |
| W-V133P2-03 | manual-reasoning | `validate_creative_brief` lines 533-584 | High |
| W-V133P2-04 | manual-reasoning | `run.rs` lines 129-182 | High |
| W-V133P2-05 | manual-reasoning | `creator.rs` test section lines 1158-1225 | Medium |
| S-V133P2-06 | manual-reasoning | `preset.yaml` inner_graphs definition | Medium |
| S-V133P2-07 | manual-reasoning | `embedded-presets/creative-brief-intake/` directory | High |
| S-V133P2-08 | static-analysis | `memory_io.rs:134-161`, `creator.rs:151-177` | High |
| S-V133P2-09 | manual-reasoning | `run.rs` lines 119-156 | High |
| S-V133P2-10 | static-analysis | `preset.rs` — no diff in manifest.rs | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 6 |

**Verdict**: Request Changes

**Rationale**: The failing integration test `registry_has_sixteen_builtins` (C-V133P2-01) is a CI-blocking regression that must be fixed before merge. The test was already misnamed (asserting 17 while called "sixteen") and was not updated to 18 when `creator.write_brief` was added. Additionally, the lack of tracing in multi-turn LLM paths (W-V133P2-02) and unbounded brief validation (W-V133P2-03) represent reliability risks that should be addressed or explicitly deferred with residual tracking.
