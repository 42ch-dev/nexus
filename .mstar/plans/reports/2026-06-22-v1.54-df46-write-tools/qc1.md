---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-22-v1.54-df46-write-tools"
verdict: "Approve"
generated_at: "2026-06-20T12:42:30Z"
revalidated_at: "2026-06-20T12:42:30Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-20T11:43:43Z

## Scope
- plan_id: 2026-06-22-v1.54-df46-write-tools
- Review range / Diff basis: `merge-base: origin/main` + `tip: iteration/v1.54 HEAD (660fffff)` (P0 work has been merged into iteration/v1.54; review P0's full contribution)
- Working branch (verified): iteration/v1.54
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 17
- Commit range: origin/main..9b65b37b for P0-specific implementation/stat focus; assignment tip verified at 660fffff, with P1 paths treated out of scope except where they affected merged integration evidence.
- Tools run: `git rev-parse --show-toplevel`; `git branch --show-current`; `git log --oneline -10`; `git diff origin/main..HEAD --stat`; `git diff origin/main..9b65b37b --stat`; `cargo clippy --all -- -D warnings`; `cargo test --all`; targeted source/spec reads.

## Findings
### 🔴 Critical
- **C-001 — `nexus.kb_snapshot.write` authorizes the request `world_id` but persists each block's embedded `world_id` without enforcing equality.** `execute_kb_snapshot_write` checks `ensure_world_accessible_for_creator(..., world_id)` once, then deserializes each `KeyBlock` and calls `insert_key_block_in_tx` using `kb.world_id`. A caller can supply an owned/accessible outer `world_id` while a block payload names a different existing world; the insert path then persists the embedded world id. This breaks the `RequireWorldOwnership` intent for a write tool and can become cross-world/cross-creator mutation if the target world id is known and satisfies DB FK constraints. **Fix:** before insert, reject any block whose `kb.world_id != world_id`; add a hermetic mismatch test and a cross-creator/world mismatch test.

### 🟡 Warning
- **W-001 — Registry admission gates are declarative metadata, not the runtime gate chain the plan/spec describe.** `CapabilityRow.admission` is populated with `&'static [AdmissionGate]`, but `CapabilityRegistry::dispatch` and `HostToolExecutor::registry_dispatch` never interpret the row's gate slice. Actual enforcement remains split between a generic `admission_pipeline` and per-handler helper calls. This weakens the registry-as-SSOT architecture and allows future rows to claim gates that are not actually enforced. **Fix:** centralize gate execution over `row.admission` before handler invocation, or explicitly downgrade the field to documentation-only and add invariants that bind every gate to executable checks.
- **W-002 — `nexus.finding.resolve` reports success for nonexistent finding ids.** `nexus_local_db::findings::update_finding` returns `Result<bool, LocalDbError>`, where `false` means no row was updated. The handler ignores the bool and always returns `{ resolved: true }`; the test `finding_resolve_nonexistent_returns_success` codifies this false-positive behavior. This can make agents believe quality findings were closed when no finding existed. **Fix:** check the returned bool and map `false` to `NotFound` or creator-scoped `Forbidden`; replace the current test with a rejection assertion.
- **W-003 — `nexus.manuscript.chapter.update` writes chapter bodies to a path outside the established `work_chapters` layout.** Seeded chapters use relative paths like `Works/{work_ref}/Stories/ch01-ch01.md`, but the new handler writes `workspace_path/Stories/{work_id}/ch_XX_vYY/body.md` and stores that absolute-ish path into `work_chapters.body_path`. This introduces a second manuscript location convention and bypasses existing Work-ref based organization. **Fix:** use the existing `WorkChapterRecord.body_path` (or derive from `work_ref`/chapter slug via the same helper as `seed_chapters`) and add a test that asserts both file content and DB `body_path` follow the canonical `Works/{work_ref}/Stories/...` shape.

### 🟢 Suggestion
- **S-001 — Benchmark artifact does not measure the cold path or end-to-end dispatch despite its header.** `dispatch_latency.rs` documents cold initialization and `dispatch_whoami`, but only benchmarks warm lookup and `len()`. Add the missing benchmark cases or narrow the comments/plan evidence to what is actually measured.
- **S-002 — Some P0 closure notes overstate residual closure semantics.** The status rows for LIMIT-parameter residuals are marked resolved while their closure notes say `Deferred: LIMIT ? sqlx regen...`. PM/QA own lifecycle updates, but future consolidation should ensure resolved lifecycle matches actual fix/defer state.

## Source Trace
- C-001
  - Source Type: manual-reasoning + git-diff
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:1547-1585`; `crates/nexus-local-db/src/kb_store.rs:146-205`
  - Confidence: High
- W-001
  - Source Type: manual-reasoning + spec-rule
  - Source Reference: `crates/nexus-daemon-runtime/src/capability_registry.rs:138-144,223-235,527-634`; `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:391-405`; plan §5.1 says gate-chain check occurs before handler.
  - Confidence: High
- W-002
  - Source Type: manual-reasoning + tests
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:1961-1991,3693-3712`; `crates/nexus-local-db/src/findings.rs:927-1041`
  - Confidence: High
- W-003
  - Source Type: manual-reasoning + existing layout comparison
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:1663-1715`; `crates/nexus-local-db/src/work_chapters.rs:63-68`
  - Confidence: High
- S-001
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus-daemon-runtime/benches/dispatch_latency.rs:1-8,13-57`
  - Confidence: High
- S-002
  - Source Type: status-review
  - Source Reference: `.mstar/status.json:1811-1825,1845-1859`
  - Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 2 |

## Verdict
**Verdict**: Request Changes

`cargo clippy --all -- -D warnings` and `cargo test --all` both passed, but the write-tool review found one blocking authorization/data-integrity issue and three unresolved warning-level architecture/correctness issues. Per QC gate rules, this cannot be approved until the Critical and Warning findings are addressed or explicitly reassigned by PM as residuals with an accepted risk decision.

## Revalidation

Targeted re-review scope: only qc1 findings C-001, W-001, W-002, W-003, with S-001 accepted as deferred outside the P0 fix-wave and S-002 noted for PM residual lifecycle review. Current checkout verified as `/Users/bibi/workspace/organizations/42ch/nexus` on `iteration/v1.54`; fix-wave commits verified in `git log --oneline -15` including `9f8e5ef5`, `1283f579`, `663cc55b`, `d383e6e6`, `2a0b8024`, `b29d36b8`, `22db9700`, and `e188979d`.

- **C-001 — Resolved.** `execute_kb_snapshot_write` now rejects any block whose embedded `kb.world_id` differs from the request-level `world_id` before inserting into the KB store (`host_tool_executor.rs:1576-1592`). Regression coverage passed: `cargo test -p nexus-daemon-runtime kb_snapshot_write_rejects_cross` (`kb_snapshot_write_rejects_cross_world_block_same_creator`, `kb_snapshot_write_rejects_cross_creator_world_block`).
- **W-001 — Resolved.** The fix explicitly makes registry admission rows declarative/accountability metadata, documents the enforcement split in `CapabilityRegistry::dispatch`, iterates `row.admission` as the central checkpoint, and adds `registry_all_admission_gates_have_enforcement` to bind every declared `AdmissionGate` variant to a runtime enforcement path. This satisfies qc1's alternate fix path (downgrade documentation-only semantics + invariant) rather than converting `dispatch` into the executor for every gate. Regression coverage passed: `cargo test -p nexus-daemon-runtime registry_all_admission_gates_have_enforcement`.
- **W-002 — Resolved.** `execute_finding_resolve` now checks the `bool` returned by `nexus_local_db::findings::update_finding` and maps `false` to `NOT_FOUND` instead of returning `{ resolved: true }` (`host_tool_executor.rs:2024-2056`). Regression coverage passed: `cargo test -p nexus-daemon-runtime finding_resolve_nonexistent_returns_not_found`.
- **W-003 — Resolved.** `execute_manuscript_chapter_update` now uses the existing canonical `WorkChapterRecord.body_path`, or falls back to `Works/{work_ref}/Stories/{ch_nn}-{ch_nn}.md`, and stores the relative canonical path in `work_chapters.body_path` (`host_tool_executor.rs:1682-1715`, `1720-1757`). Regression coverage passed: `cargo test -p nexus-daemon-runtime manuscript_chapter_update_writes_content`, which asserts the DB path starts with `Works/`, contains `Stories/`, ends with `.md`, and the file content is written at that canonical relative path.

Non-blocking qc1 notes:
- **S-001 — Accepted as deferred / out of fix-wave scope.** The benchmark now includes a cold-path case and clarified that end-to-end `dispatch_whoami` is not benchmarked here (`dispatch_latency.rs:3-13`, `24-55`). Evidence: `cargo bench -p nexus-daemon-runtime --bench dispatch_latency --no-run` compiled successfully.
- **S-002 — PM residual lifecycle review still advised.** QC1 does not own `.mstar/status.json` lifecycle edits; PM should ensure any deferred LIMIT/sqlx closure notes are represented as defer/accepted residuals rather than overstated resolved closures.

Revalidation gates:
- `cargo test --all` — passed (`760 passed; 0 failed` plus integration/doc tests; command exit 0).
- `cargo clippy --all -- -D warnings` — passed (command exit 0).

**Revalidated Verdict**: Approve
