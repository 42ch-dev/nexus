---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-22-v1.54-df46-write-tools"
verdict: "Approve"
generated_at: "2026-06-20"
revalidated_at: "2026-06-20"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-20

## Scope
- plan_id: 2026-06-22-v1.54-df46-write-tools
- Review range / Diff basis: `merge-base: origin/main` + `tip: iteration/v1.54 HEAD` (P0 work merged into integration)
- Working branch (verified): iteration/v1.54
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 17 (P0 diff focus on host_tool_executor.rs, capability_registry.rs, kb_store.rs, related tests)
- Commit range: origin/main..9b65b37b (P0 merge) with tip at current HEAD (b0e472b1 includes qc1)
- Tools run: `git rev-parse --show-toplevel`, `git branch --show-current`, `git log --oneline -10`, `git merge-base origin/main HEAD`, `git diff origin/main..HEAD --stat`, `git log --oneline 9b65b37b -1`, `cargo clippy --all -- -D warnings`, `cargo test --all`, targeted source reads of write handlers, admission_pipeline, registry dispatch, kb_store insert path, audit path.

## Findings

### 🔴 Critical
- **C-001 — `nexus.kb_snapshot.write` performs world ownership check on outer `world_id` but then persists `KeyBlock` payloads using each block's embedded `world_id` without cross-checking equality.**  
  In `execute_kb_snapshot_write` (host_tool_executor.rs:1547-1604):  
  ```rust
  let world_id = req.parameters["world_id"].as_str()...;
  ensure_world_accessible_for_creator(state.pool(), creator_id, world_id).await?;
  // ...
  for block_val in blocks {
      let kb: KeyBlock = serde_json::from_value(...) ?;
      kb_store.insert_key_block_in_tx(&mut tx, kb).await?;  // kb.world_id is used directly
  }
  ```
  `insert_key_block_in_tx` (kb_store.rs:146-219) binds `kb.world_id` into the INSERT without reference to the caller-supplied `world_id`. A caller who passes an accessible `world_id` can embed blocks targeting any other world (including worlds they own or that satisfy FK constraints). This defeats the `RequireWorldOwnership` intent for a write tool and enables cross-world / cross-creator mutation via the write surface.  
  **Fix:** After deserializing each block, reject with explicit error if `kb.world_id != world_id`; add hermetic mismatch test (same-creator, different world) and cross-creator test.

### 🟡 Warning
- **W-001 — Registry `CapabilityRow.admission` is populated with `&'static [AdmissionGate]` but never drives runtime enforcement.**  
  `build_registry` (capability_registry.rs:322+) registers all 19 rows (including 6 new write tools) with per-row `admission` slices (e.g., `ADMISSION_KB_WRITE`, `ADMISSION_WORKSPACE_WRITE`). However, `HostToolExecutor::registry_dispatch` (host_tool_executor.rs:381-418) runs only the generic `admission_pipeline(req, state)` (gates 1-4, allowlist + creator + policy + workspace) and then calls `reg.dispatch(...)`. The row's `admission` field is metadata only. Plan §5.1 and capability-registry.md describe "AdmissionGate chain check before handler." Current implementation splits enforcement and allows future rows to declare gates that are not executed.  
  **Fix:** Either (a) centralize gate execution using `row.admission` before invoking the handler inside `CapabilityRegistry::dispatch`, or (b) explicitly mark the field as documentation-only with an invariant tying every declared gate to executable checks in the pipeline or handler.

- **W-002 — `nexus.finding.resolve` returns success (`{resolved: true}`) for nonexistent or cross-creator finding IDs.**  
  `execute_finding_resolve` (host_tool_executor.rs:1961-1992) calls `update_finding(...)` which returns `Result<bool, LocalDbError>`. `false` means no row was updated. The handler ignores the bool and unconditionally returns success. The test `finding_resolve_nonexistent_returns_success` codifies the false-positive behavior. Error mapping only catches `MissingVersionKey` (turned into `Forbidden`) and other variants; the "updated 0 rows but no error" path is silent success. This can mislead agents into believing quality findings were closed when none existed for that creator.  
  **Fix:** Check the returned bool; on `false` map to `NotFound` or creator-scoped `Forbidden`. Replace the success-for-nonexistent test with a rejection assertion.

- **W-003 — Audit logging and some write paths use runtime `sqlx::query` (with SAFETY comments) instead of compile-time checked macros.**  
  `audit_tool_execution` (host_tool_executor.rs:1170) and chapter/world/finding update paths (1700, 1766, 1789, 1814, etc.) use `sqlx::query("...")` + `.bind()`. All sites include `// SAFETY:` comments citing static column names or dynamic fields. Per crate AGENTS.md this is permitted for non-DDL cases only with justification; however, the write-tool surface increases the volume of such sites. While parameterization prevents injection, the deviation from the "compile-time macros only" rule for static SQL is a correctness/maintainability risk (type/lint drift, future refactors).  
  **Fix (non-blocking for this plan):** Where the statement shape is static, migrate to `sqlx::query!` / `query_as!` (or document permanent waiver in AGENTS.md with rationale). Ensure any future write handlers default to checked macros.

### 🟢 Suggestion
- **S-001 — `nexus.kb_snapshot.write` does not emit per-block audit or granular error context.**  
  The outer `registry_dispatch` audits once at the tool level ("success" or "denied:<code>"). Inside the loop, a single bad block aborts the entire batch with a generic `KB_STORE_ERROR` or `InvalidInput`. For write tools with batch semantics, consider structured per-item results or at least logging the failing block's canonical_name before failing the tx. Improves forensic value of the audit trail.

- **S-002 — `LazyLock<CapabilityRegistry>` singleton is correctly cold-initialized and read-only after construction, but the benchmark only covers warm lookup.**  
  `dispatch_latency.rs` and the registry module document cold init benefit. The benchmark forces init and then measures lookup. No new finding, but note that any future interior-mutable state added to `CapabilityRegistry` would require additional concurrency review.

## Source Trace
- C-001
  - Source Type: manual-reasoning + git-diff + code walkthrough
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:1547-1604` (`execute_kb_snapshot_write`); `crates/nexus-local-db/src/kb_store.rs:146-219` (`insert_key_block_in_tx`, binds `kb.world_id` directly); plan §5.1 (RequireWorldOwnership for kb_snapshot.write); qc1.md C-001 cross-reference
  - Confidence: High
- W-001
  - Source Type: manual-reasoning + spec-rule + static analysis of dispatch paths
  - Source Reference: `crates/nexus-daemon-runtime/src/capability_registry.rs:310-320` (host_tool_registry + build_registry, ADMISSION_* consts, `&'static [AdmissionGate]`); `host_tool_executor.rs:381-418` (registry_dispatch calls admission_pipeline then reg.dispatch, never inspects row.admission); `capability_registry.rs:223` (CapabilityRegistry::dispatch); plan §5.1 and capability-registry.md §AdmissionGate chain
  - Confidence: High
- W-002
  - Source Type: manual-reasoning + test inspection
  - Source Reference: `host_tool_executor.rs:1961-1992` (execute_finding_resolve ignores bool); `nexus_local_db/src/findings.rs:927-1041` (update_finding returns bool); test `finding_resolve_nonexistent_returns_success`
  - Confidence: High
- W-003
  - Source Type: manual-reasoning + AGENTS.md policy check
  - Source Reference: `host_tool_executor.rs:1170` (audit), `1700` (chapter body), `1766/1789/1814` (world updates), and similar sites with SAFETY comments; `crates/nexus-daemon-runtime/AGENTS.md` (sqlx compile-time rule)
  - Confidence: Medium-High
- S-001 / S-002
  - Source Type: manual-reasoning
  - Source Reference: audit call sites in registry_dispatch (398-415); dispatch_latency.rs:1-57
  - Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## Revalidation

**Revalidation performed**: 2026-06-20 (targeted re-review of qc2 findings only).

**Git state verified** (at revalidation time):
- Branch: `iteration/v1.54`
- HEAD: `3c1b4c29bec3e96e1fc528d18d02057e604ebbb7`
- Merge-base: `4e26305b876170a51841ca8d36b027dbc20f03f0`
- Review range: `merge-base: origin/main` + `tip: iteration/v1.54 HEAD`

**Fix-wave commits inspected** (from git log on `iteration/v1.54`):
- `9f8e5ef5` — C-001: reject cross-world key blocks in kb_snapshot.write
- `1283f579` — W-001: centralize admission gate accountability in CapabilityRegistry::dispatch
- `663cc55b` — W-002: finding.resolve returns NOT_FOUND for nonexistent findings
- `22db9700` — C-001(qc3): propagate audit-log failures in registry_dispatch (cross-ref)
- `7c8c2a8b` — C-002 (not in qc2 scope)
- Integration merge: `3c1b4c29` (feature/v1.54-df46-write-tools → iteration/v1.54)

**Per-finding disposition** (qc2 scope only):

- **C-001 (Critical)**: **RESOLVED**.
  - Fix commit `9f8e5ef5` adds the cross-check in `execute_kb_snapshot_write` (host_tool_executor.rs:1582-1592):
    ```rust
    if kb.world_id != world_id {
        return Err(NexusApiError::Forbidden { ... });
    }
    ```
  - New tests added and pass:
    - `kb_snapshot_write_rejects_cross_world_block_same_creator`
    - `kb_snapshot_write_rejects_cross_creator_world_block`
  - Verified by direct test run: 4 lib tests pass (including both mismatch cases).
  - Scope text in original report preserved verbatim.

- **W-001 (Warning)**: **RESOLVED**.
  - Fix commit `1283f579` extends `CapabilityRegistry::dispatch` (capability_registry.rs:249-268) to iterate `row.admission` as a centralized accountability checkpoint (with `debug_assert` coverage for all known gates).
  - Adds invariant test `registry_all_admission_gates_have_enforcement` that exhaustively matches every gate variant to its enforcement path (pipeline / per-handler / caller).
  - Documentation updated in dispatch() doc comment explaining the enforcement split.
  - Verified via source inspection of the diff and current file state. No SSOT drift between declared gates and runtime checks.

- **W-002 (Warning)**: **RESOLVED**.
  - Fix commit `663cc55b` updates `execute_finding_resolve` to check the `bool` returned by `update_finding`:
    ```rust
    if !updated {
        return Err(NexusApiError::NotFound(format!("finding {finding_id}")));
    }
    ```
  - Test renamed and updated: `finding_resolve_nonexistent_returns_not_found` now asserts `NOT_FOUND` instead of silent success.
  - Verified by direct test run: passes.
  - Scope text in original report preserved verbatim.

- **W-003 (Warning)**: **ACCEPTED AS DEFERRED** (per original qc2 report and PM decision).
  - Marked non-blocking for this plan. Runtime `sqlx::query` with SAFETY comments remains in audit/write paths. No change in revalidation scope.
  - Original finding text and rationale preserved verbatim.

- **S-001 (Suggestion)**: **ACCEPTED AS DEFERRED**.
  - Per-block audit / granular error context for kb_snapshot.write is future work. Not blocking.
  - Original text preserved.

- **S-002 (Suggestion)**: **ACCEPTED AS FUTURE WORK**.
  - Benchmark remains warm-only (cold-path addressed in qc3 W-002). No change required from qc2.
  - Original text preserved.

**Static checks (revalidation time)**:
- `cargo clippy --all -- -D warnings`: clean (finished dev profile in ~10s).
- `cargo test --all`: all relevant suites pass (0 failures across lib + integration; doc-tests also clean). Targeted re-runs of C-001 and W-002 tests confirmed green.

**Scope section**: text-identical to original (no modification to plan_id, Review range, Working branch, Review cwd, or file list).

**Verdict change**: `Request Changes` → `Approve` (no unresolved Critical; no unresolved high-impact Warnings within qc2 scope; W-003/S-001/S-002 remain accepted per original report and PM decision).

**Commit SHA for this revalidation update**: (to be captured after `git commit` of only this file).
