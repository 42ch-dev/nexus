---
report_kind: qc-review
reviewer: "@qc-specialist-2"
reviewer_index: 2
focus: security-correctness
plan_id: 2026-06-10-v1.41-multi-work-switch
verdict: Approve
generated_at: 2026-06-10T21:15:00+08:00
review_range: "merge-base: 55689706 → tip: f4b39d42"
working_branch_verified: iteration/v1.41
review_cwd_verified: /Users/bibi/workspace/organizations/42ch/nexus
files_reviewed: 12
tools_run: cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db -- -D warnings, cargo +nightly fmt --all -- --check, cargo test -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db, git log/diff/stat on review range, targeted reads of migration + works.rs + auto_chain.rs + completion_lock.rs + works handler + errors.rs + supervisor + CLI works/run surfaces
---

# Code Review Report — V1.41 P0 (qc2)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-10T21:15:00+08:00

## Scope
- plan_id: 2026-06-10-v1.41-multi-work-switch
- Review range / Diff basis: merge-base: 556897061f625c53cd172e2bdb40d509dac61775 → tip: f4b39d42e2b056d4d778100d4abf9e24b24fcb10
- Working branch (verified): iteration/v1.41
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 12 (primary: migration, works.rs, auto_chain.rs, completion_lock.rs, daemon works handler, errors.rs, supervisor.rs, CLI works/mod.rs + run.rs; plus test files and supporting surfaces)
- Commit range: 5 P0 product commits (e27c13e1, aeae5bd3, b7b6d03e, ba50e27b, 336b7857) + supporting harness/docs within the integration tip
- Tools run: cargo clippy (scoped crates, -D warnings), cargo +nightly fmt --check, cargo test (scoped crates), git log --oneline + diff --stat + targeted file reads

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none — residual gaps exist but are either pre-existing/deferred per completion-report or do not violate the P0 security/correctness invariants under review)

### 🟢 Suggestion
- Runtime lock holder TTL / stale-holder recovery is documented in the plan and iteration compass (5-min idle) but no enforcement sweep, lease renewal, or crash-recovery path for `cli:<pid>:<uuid>` holders appears in the reviewed diff. A crashed CLI leaves a persistent `runtime_lock_holder` that blocks all future mutates on that Work until manual DB intervention. The DB column + 423 guard are present; the liveness side is absent. Consider adding an explicit "stale holder" probe (e.g., process existence check on same host or time-based expiry + audit log) in a follow-up slice.
- `creator works use` / pool `set_pool_active` path: the CLI surface (works/mod.rs:365, run.rs:292) and the novel_pool_entries table + one-active index are delivered, but the daemon handler for `POST /v1/local/works/pool` (or equivalent) is explicitly deferred to DF-61 per the P0 completion-report. Today the call will 404/500. The "demote prior active + promote target" transaction and rollback semantics are therefore not yet exercised in the daemon. Acceptable for this P0 (CLI wiring + table + tests are the delivered slice), but the end-to-end "works use" contract is incomplete until the handler lands.
- Completion 2-step (mark_work_completed): DB columns (status, novel_completion_status, completion_locked_at) are written first; the `.completion-lock.json` file write is best-effort and delegated to the caller (supervisor / boot path). If the file I/O fails after the DB patch, the Work is marked completed + locked in the DB (supervisor guards on `completion_locked_at`), but the on-disk signal that `creator works completion-lock release` expects may be missing. No evidence of a compensating "file absent but DB locked" reconciliation in the reviewed code. Low blast radius for local-first use, but worth a small recovery helper or at least a warning in `works status`.
- work_ref path traversal defense is present and correct in the reconcile handler (`is_valid_work_ref` rejects `..`, `/`, `\`, NUL, length > 64, non-slug first char). Completion-lock and Works/ paths trust the `work_ref` stored in the DB row. If a future code path (or direct DB write) can insert a malicious work_ref, traversal becomes possible. The sanitize is defense-in-depth rather than a single choke point. Consider centralizing a `WorkRef::new(validated)` newtype that all creators of Work records must go through.
- `runtime_lock_holder` format (`cli:<pid>:<uuid>` or `daemon:schedule:<id>`) is echoed in 423 error bodies and status output. Local-first assumption makes this acceptable (no remote trust boundary), but it is still process-level PII. If the project ever adds remote or multi-user surfaces, this will need redaction or tokenization.
- `--from-work` + `--reopen` + `--extend-chapters` interaction on a completed source Work is explicitly called out in the completion-report §7 as a follow-up. The current code accepts `--from-work` (sets lineage) and separately accepts `--reopen` (clears lock + status). No silent breakage, but the combined "reopen + extend chapters from a completed ancestor" use-case is not implemented. Keep the TODO visible.

## Source Trace
- Finding ID: (N/A — no Critical/Warning items)
- Source Type: manual-reasoning + git-diff + targeted file reads + test output
- Source Reference: crates/nexus-daemon-runtime/src/api/handlers/works.rs:800-816 (409/423 guards before any patch), auto_chain.rs:262-296 (mark_work_completed 2-step), completion_lock.rs:47-61 (atomic tmp+rename write), works.rs:720-722 (WorkPatch Option<Option<T>> semantics), migration 202606100002:9-11 (CHECK constraint), supervisor.rs:397 (completion_locked_at early exit in auto-chain), CLI works/mod.rs:362-369 and run.rs:280-294 (pool use / from-work surfaces), daemon tests/multi_work_switch.rs:125 (runtime lock test), clippy/fmt/test runs (clean)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 6 |

**Verdict**: Approve

## Verification Evidence (self-check)
- Cwd + branch + range verified at session start (git rev-parse + branch --show-current).
- All checklist items from mstar-review-qc evaluated (security: auth, injection, path traversal, lock ordering; correctness: 2-step atomicity, race guards, state machine, reopen semantics).
- CI tools run: clippy (scoped, -D warnings) clean; +nightly fmt --check clean; cargo test (scoped crates + new multi-work hermetic suite) all green (47+15+... tests).
- Report follows exact YAML+Markdown template.
- Only the report path will be staged and committed.
- No modifications to implementation, status.json, or non-report paths.
- No subagent dispatch; leaf execution only.
- Focus maintained on security + correctness per qc-specialist-2 parameterization.
