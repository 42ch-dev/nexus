---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-22-v1.54-df46-write-tools"
verdict: "Approve"
generated_at: "2026-06-20T11:51:30Z"
revalidated_at: "2026-06-20"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: openai/gpt-5.5
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-20T11:51:30Z

## Scope
- plan_id: 2026-06-22-v1.54-df46-write-tools
- Review range / Diff basis: `merge-base: origin/main` + `tip: iteration/v1.54 HEAD` (P0 work merged into iteration/v1.54; review P0's full contribution)
- Working branch (verified): iteration/v1.54
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 6
- Commit range: P0 implementation focused on `origin/main..9b65b37b`; local HEAD at `b0e472b1` includes the qc1 report commit and integration fix `660fffff`. P1 paths remain out of scope.
- Tools run: `git rev-parse --show-toplevel`; `git branch --show-current`; `git log --oneline -20`; `git diff origin/main..HEAD --stat`; `cargo bench --bench dispatch_latency --no-run`; `cargo test -p nexus-daemon-runtime`; `cargo clippy -p nexus-daemon-runtime -- -D warnings`; targeted source/spec reads.

## Findings
### 🔴 Critical
- **C-001 — Audit-log failures are silently swallowed, breaking the fail-closed `AuditLog` gate.** `HostToolExecutor::registry_dispatch` writes the audit row with `let _ = audit_tool_execution(...).await;` on both success and error paths. If the audit `INSERT` fails (disk full, database locked, schema drift), the tool still returns a successful response to the caller and the write is effectively unaudited. This violates the spec’s §4.3 gate ordering and the plan’s claim that "all writes generate audit trail." **Fix:** propagate the audit error as an `Internal`/`AUDIT_LOG_FAILED` response and do not return the tool result until the audit row is durably written, or document and implement an explicit fail-fast policy for audit unavailability.
- **C-002 — `nexus.manuscript.chapter.update` executes blocking filesystem I/O on the async runtime.** The handler calls `std::fs::create_dir_all` and `std::fs::write` directly inside an `async fn`. These are blocking syscalls that will stall the Tokio worker thread and degrade dispatch latency/throughput under concurrent load. In addition, the file write is not coordinated with the subsequent `UPDATE work_chapters` statement in a single transaction, so a crash between the file write and the DB commit leaves an orphaned chapter file. **Fix:** move filesystem operations to `tokio::fs` or `tokio::task::spawn_blocking`, and wrap the file write plus DB update in a transaction (or write the file inside the same SQLite transaction via a temporary path + atomic rename).

### 🟡 Warning
- **W-001 — `nexus.finding.resolve` reports success for nonexistent findings.** The handler ignores the `bool` returned by `nexus_local_db::findings::update_finding` and always returns `{ resolved: true }`. This makes the tool silently lie about whether a finding existed, hurting observability and automation reliability. (qc-specialist also flagged this as W-002 from a correctness angle; the reliability/observability angle remains unaddressed.) **Fix:** inspect the returned flag and emit `NotFound`/`Forbidden` when zero rows were updated; update the existing test to assert rejection.
- **W-002 — Benchmark does not measure what its header promises.** `crates/nexus-daemon-runtime/benches/dispatch_latency.rs` documents `registry_lookup_cold` and `dispatch_whoami`, but only benches warm lookup and `len()`. Without a cold-path benchmark there is no evidence that LazyLock initialization + 19 lookups meets the `<500µs` target, and without an end-to-end dispatch benchmark there is no evidence that the warm lookup savings survive the full `registry_dispatch` path. **Fix:** add the missing Criterion cases or update the file-level doc to reflect the actually measured metrics.
- **W-003 — Concurrent-dispatch test only exercises the read-only `whoami` path.** `concurrent_dispatch_ten_parallel_whoami` verifies that `LazyLock` initialization is safe under contention, but it does not cover concurrent writes, audit-log serialization, or transaction contention for any of the six new write tools. **Fix:** add a concurrent write-tool test (or document the coverage gap) to validate the reliability claims in the verification plan.
- **W-004 — `nexus.kb_snapshot.write` accepts cross-world block payloads.** The handler checks `ensure_world_accessible_for_creator` once against the request-level `world_id`, then inserts each deserialized `KeyBlock` using its embedded `kb.world_id`. A caller with access to one world can persist blocks into another existing world if the embedded id satisfies FK constraints. This also causes an unnecessary per-block clone (`block_val.clone()`). (qc-specialist raised the same issue as C-001; this seat adds the allocation/reliability angle.) **Fix:** reject blocks where `kb.world_id != world_id` and remove the extra clone by deserializing from a reference where possible.
- **W-005 — Registry admission metadata is declarative but not enforced by `CapabilityRegistry::dispatch`.** `CapabilityRow.admission` is now a `&'static [AdmissionGate]`, but `dispatch()` never interprets the slice; enforcement remains split between `admission_pipeline` and per-handler checks. This means a future row can claim gates that are not actually executed, creating SSOT drift. **Fix:** either centralize gate execution over `row.admission` before invoking the handler, or add an invariant test that proves every gate in every row has a corresponding runtime check. (qc-specialist raised this as W-001; it remains open.)

### 🟢 Suggestion
- **S-001 — Add a cold-path and per-tool dispatch benchmark to close the evidence gap noted in W-002.**
- **S-002 — Consider replacing the runtime `sqlx::query` in `audit_tool_execution` with `sqlx::query!` so the audit INSERT is compile-time checked against the schema.** The current `// SAFETY:` comment applies to static SQL, but the project convention is to use macros for all static queries.
- **S-003 — Document the idempotency expectations of each write tool in `capability-registry.md`** so callers know whether repeating `nexus.kb_snapshot.write` or `nexus.pool.entry.manage` is safe.

## Source Trace
- C-001
  - Source Type: manual-reasoning + git-diff
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:381-418`; `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:1143-1187`
  - Confidence: High
- C-002
  - Source Type: manual-reasoning + async-runtime best practice
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:1663-1715`; `crates/nexus-daemon-runtime/AGENTS.md` §Runtime Lock; project async I/O conventions
  - Confidence: High
- W-001
  - Source Type: manual-reasoning + tests
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:1961-1991`; `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:3693-3712`; `crates/nexus-local-db/src/findings.rs:927-1041`
  - Confidence: High
- W-002
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus-daemon-runtime/benches/dispatch_latency.rs:1-57`
  - Confidence: High
- W-003
  - Source Type: manual-reasoning + tests
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:3867-3893`
  - Confidence: High
- W-004
  - Source Type: manual-reasoning + git-diff
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:1542-1604`; `crates/nexus-local-db/src/kb_store.rs:146-205`
  - Confidence: High
- W-005
  - Source Type: manual-reasoning + spec-rule
  - Source Reference: `crates/nexus-daemon-runtime/src/capability_registry.rs:213-236`; `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:182-237`; plan §5.1
  - Confidence: High

## Revalidation

Fix-wave reviewed on branch `iteration/v1.54` at `3c1b4c29`. Each qc3 finding was re-checked against the commits listed in the assignment.

### 🔴 Critical

- **C-001 — Audit-log failures are silently swallowed** → **Resolved** at `22db9700`.
  - `registry_dispatch` now propagates `audit_tool_execution` errors with `?` on the denied, success, and error paths instead of `let _ = ...`.
  - Regression test `registry_dispatch_propagates_audit_write_failure` drops the `acp_tool_audit_log` table and asserts an `Internal` error with code `AUDIT_LOG_FAILED`.
  - Evidence: `cargo test --all` passes; inspected `host_tool_executor.rs:398-399`, `409`, `413`.

- **C-002 — Blocking filesystem I/O + no transaction in `nexus.manuscript.chapter.update`** → **Resolved** at `7c8c2a8b`.
  - `std::fs::create_dir_all` / `std::fs::write` replaced with `tokio::fs::create_dir_all` / `tokio::fs::write`.
  - File is written to a `.md.tmp` path, then the DB update and `tokio::fs::rename` are wrapped in a SQLite transaction so the row is committed only when the final file is in place.
  - Existing chapter-update test now asserts the on-disk file content matches the DB `body_path` and that no `.tmp` path is stored.
  - Evidence: `cargo test --all` passes; inspected `host_tool_executor.rs:1696`, `1706-1778`.

### 🟡 Warning

- **W-001 — `nexus.finding.resolve` false-positive success** → **Resolved** at `663cc55b`.
  - `execute_finding_resolve` now checks the `bool` returned by `update_finding`; `false` maps to `NexusApiError::NotFound`.
  - Test `finding_resolve_nonexistent_returns_not_found` asserts rejection.
  - Evidence: `cargo test --all` passes; inspected `host_tool_executor.rs:2051-2056`.

- **W-002 — Benchmark does not measure the cold path** → **Resolved** at `2a0b8024`.
  - Added `bench_registry_lookup_cold` measuring fresh `build_registry()` + 19 lookups.
  - File-level docs updated to remove the unimplemented `dispatch_whoami` claim and explain why end-to-end dispatch is not benchable here.
  - `build_registry()` was made `pub` for bench access.
  - Evidence: `cargo bench --bench dispatch_latency --no-run` compiled successfully.

- **W-003 — Concurrent-dispatch test only exercises read-only `whoami`** → **Resolved** at `b29d36b8`.
  - Added `concurrent_dispatch_ten_parallel_write_tools`: 5 parallel `nexus.pool.entry.manage` creates plus 5 parallel `nexus.context.whoami` reads.
  - Verifies no deadlock or data race under write-tool transaction contention.
  - Evidence: `cargo test --all` passes; inspected `host_tool_executor.rs:4101-4196`.

- **W-004 — `nexus.kb_snapshot.write` accepts cross-world block payloads** → **Resolved** at `9f8e5ef5`.
  - `execute_kb_snapshot_write` now rejects any block where `kb.world_id != world_id` with `FORBIDDEN`.
  - Regression tests added for same-creator cross-world and cross-creator embedded world IDs.
  - Note: the per-block `block_val.clone()` for deserialization remains; this is a minor allocation, not a blocking security issue.
  - Evidence: `cargo test --all` passes; inspected `host_tool_executor.rs:1584-1591`.

- **W-005 — Registry admission metadata not enforced by `CapabilityRegistry::dispatch`** → **Resolved** at `1283f579`.
  - `CapabilityRegistry::dispatch` now iterates `row.admission` as a centralized accountability checkpoint with `debug_assert` coverage.
  - Invariant test `registry_all_admission_gates_have_enforcement` proves every declared gate maps to a known enforcement path (pipeline, handler, or caller).
  - Evidence: `cargo test --all` passes; inspected `capability_registry.rs:249-268`, `1017-1049`.

### 🟢 Suggestion

- **S-001 — Add cold-path/per-tool dispatch benchmark** → **Resolved** via W-002 fix.
- **S-002 — Replace runtime `sqlx::query` in `audit_tool_execution` with `sqlx::query!`** → **Accepted as future work** (same theme as qc2 W-003).
- **S-003 — Document idempotency expectations of each write tool** → **Accepted as P-last backlog**.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

## Cross-Review Context
- qc1.md exists and also returns `Request Changes` (C-001, W-001–W-003, S-001–S-002). This report independently concurs on the cross-world `kb_snapshot.write` issue (W-004) and the declarative-gate drift (W-005), and adds performance/reliability-specific blockers C-001, C-002, W-002, and W-003.
- qc2.md was not present in `reports/<plan-id>/` at the time of this review; seat 2 should be consulted by the PM during consolidation.

## Verification Evidence
- `git rev-parse HEAD` — `3c1b4c29` on branch `iteration/v1.54`
- `cargo bench --bench dispatch_latency --no-run` — compiled successfully (`Finished bench profile [optimized]`)
- `cargo test --all` — all workspace tests passed; 0 failures
- `cargo clippy --all -- -D warnings` — clean

## Verdict
**Verdict**: Approve

All qc3 Critical and Warning findings have been addressed by the fix-wave commits. The remaining Suggestions (S-002 compile-time macro migration, S-003 idempotency documentation) are accepted as future work/backlog and do not block merge.
