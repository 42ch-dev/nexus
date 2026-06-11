---
report_kind: qc-review
reviewer: "@qc-specialist"
reviewer_index: 1
focus: architecture-coherence-maintainability
plan_id: 2026-06-10-v1.41-multi-work-switch
verdict: Approve
generated_at: 2026-06-10T20:30:00+08:00
review_range: "merge-base: 55689706 → tip: 9b6627dd"
working_branch_verified: iteration/v1.41
review_cwd_verified: /Users/bibi/workspace/organizations/42ch/nexus
files_reviewed: 14
tools_run: cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db -- -D warnings, cargo +nightly fmt --all -- --check, cargo test -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db, manual review of fix-wave delta edf0a621..9b6627dd
re_review_of: edf0a621
---

# Code Review Report — V1.41 P0 (qc1)

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-10T18:30:00+08:00

## Scope
- plan_id: 2026-06-10-v1.41-multi-work-switch
- Review range / Diff basis: merge-base: 55689706 → tip: f4b39d42
- Working branch (verified): iteration/v1.41
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 14
- Tools run: cargo clippy --all-targets, cargo +nightly fmt --check, cargo test (all affected crates), manual review

## Findings
### 🔴 Critical

**F-001: Missing daemon API routes — `creator works use` and `creator works completion-lock release` will 404 at runtime**

The CLI sends requests to endpoints that have **no corresponding daemon routes or handlers**:

| CLI command | HTTP request | Daemon route? | Handler? |
|---|---|---|---|
| `creator works use <id>` | `POST /v1/local/works/pool` | **No route** | **No handler** |
| `creator works completion-lock release <id>` | `POST /v1/local/works/{id}/completion-lock/release` | **No route** | **No handler** |

- `crates/nexus-daemon-runtime/src/api/mod.rs` `works_routes()` (lines 245–284): no `/v1/local/works/pool` or `/v1/local/works/{work_id}/completion-lock/release` routes registered.
- `crates/nexus-daemon-runtime/src/api/handlers/`: no `pool` or `completion_lock` handler functions exist.
- The CLI code in `crates/nexus42/src/commands/creator/works/mod.rs` `handle_use()` (line 368) and `handle_completion_lock()` (line 390) sends these requests — they will receive HTTP 404 at runtime.

**Impact**: Two of the four `creator works` subcommands (`use`, `completion-lock release`) are dead code at the daemon boundary. The P0 deliverable "`creator works use` + pool active row" (compass §1.1 row 6) and "completion-lock release" (spec §3.1) are not functional end-to-end.

**Fix**: Add daemon routes and handlers:
1. `POST /v1/local/works/pool` → handler that reads `action: "set_pool_active"` and manages `novel_pool_entries` rows (demote prior active → queued, promote target → active).
2. `POST /v1/local/works/{work_id}/completion-lock/release` → handler that calls `completion_lock::release_completion_lock()` + clears `works.completion_locked_at` in DB.

→ **Fix required before merge.**

---

**F-002: `--from-work` and `--set-default` flags are silently ignored by daemon — no handler processes them**

The CLI `creator run start` adds `lineage_from_work_id` and `set_pool_active` as extra JSON fields in the `POST /v1/local/works` body (run.rs lines 279–295). However:

- `CreateWorkRequest` (works.rs line 141) has **no** `lineage_from_work_id` or `set_pool_active` fields — serde silently ignores unknown fields.
- `create_work` handler (works.rs line 226) hardcodes `lineage_from_work_id: None` in the `WorkRecord`.
- No pool-insertion logic exists in the handler.

**Impact**: `creator run start --from-work <id>` and `creator run start --set-default` have **zero effect** at runtime. The flags parse correctly in CLI but the daemon never acts on them. The P0 deliverable "`creator run start --from-work`" (compass §1.1 row 5) is not functional.

**Fix**: Extend `CreateWorkRequest` with `lineage_from_work_id: Option<String>` and `set_pool_active: Option<bool>`. In the handler, populate `WorkRecord.lineage_from_work_id` from the request, and after successful creation, insert/update `novel_pool_entries` if `set_pool_active` is true.

→ **Fix required before merge.**

---

**F-003: Completion-lock file is never written to disk — only debug-logged**

In `mark_work_completed()` (auto_chain.rs lines 278–294), a `CompletionLock` struct is created but `let _ = lock;` discards it. The comment says "The actual file I/O is done by the supervisor or CLI layer." However:

- The supervisor (`schedule/supervisor.rs`) only checks `completion_locked_at` for skip logic — it never calls `write_completion_lock_for_work()`.
- The daemon's `get_work` handler (works.rs line 451) auto-promotes `status = 'completed'` but does **not** call `write_completion_lock_for_work()`.
- The `write_completion_lock_for_work()` function exists in `nexus-orchestration` but has **zero callers** in the daemon layer.

**Impact**: The `.completion-lock.json` file (spec §3) is never written. The file-based lock that the spec says "Daemon must not start new auto-chain ticks on this Work while lock exists" (§3 table) relies on a file that is never created. The DB column `completion_locked_at` is set, so the DB-level guard works — but the file-level guard (which the spec treats as the primary SSOT for auto-chain blocking) is non-functional.

**Fix**: In the daemon's `get_work` handler, after auto-promoting `status = 'completed'`, call `write_completion_lock_for_work()` with the workspace directory. Or add a post-completion hook in the supervisor that writes the lock file.

→ **Fix required before merge.**

### 🟡 Warning

**F-004: `runtime_lock_holder` has no TTL/heartbeat — stale locks survive CLI crash**

The spec §4.1 defines `cli:<pid>:<uuid>` with release "On command return (RAII / defer)." If the CLI process crashes (SIGKILL, power loss, terminal close), the runtime lock is **never released**. There is no TTL, heartbeat, or daemon-side watchdog to detect and clear stale locks.

**Impact**: A crashed `nexus42 creator run continue` leaves the Work permanently locked. The user must manually clear `runtime_lock_holder` via direct DB access (no CLI command exists for this). This is a reliability gap for a pre-1.0 product where crashes are expected.

**Fix (recommended for this round)**: Add a `runtime_lock_acquired_at` timestamp check in the daemon's `patch_work` guard — if the lock was acquired more than N minutes ago (e.g., 30 min for CLI, longer for daemon schedules), treat it as stale and allow the operation with a warning log. This is a minimal safety net without requiring a full heartbeat system.

→ **Fix or accept as deferred residual with explicit tracking.**

---

**F-005: Dual SSOT for completion lock — DB `completion_locked_at` vs file `.completion-lock.json`**

The spec §3 defines the file as the primary guard ("Daemon must not start new auto-chain ticks on this Work while lock exists"), but the implementation uses the DB column `completion_locked_at` for both the supervisor skip (supervisor.rs line 396) and the API mutation guard (works.rs line 800). The file is never written (F-003). If F-003 is fixed, there will be two sources of truth that can drift (e.g., file deleted manually but DB column still set, or vice versa).

**Impact**: Ambiguous lock state if one SSOT is cleared but not the other. The spec needs to clarify which is authoritative, and the implementation must reconcile them.

**Fix**: After fixing F-003, decide: either (a) file is SSOT and DB column is a cache, or (b) DB column is SSOT and file is a convenience artifact. Update spec §3 accordingly. Ensure `release_completion_lock` clears both atomically.

→ **Clarify spec and align implementation before merge.**

---

**F-006: `completion_lock` module placed in `nexus-orchestration` but file I/O needs workspace_dir from daemon layer**

The `completion_lock` module (completion_lock.rs) is in `nexus-orchestration`, which is a pure-logic crate with no access to the workspace filesystem path. The `write_completion_lock_for_work()` wrapper in `auto_chain.rs` bridges this gap, but it requires the caller (daemon layer) to provide `workspace_dir`. This is a clean separation in principle, but the daemon layer never calls it (F-003).

**Impact**: If F-003 is fixed, the architecture is sound — `nexus-orchestration` owns the lock format and atomic I/O, daemon provides the path. No architectural change needed beyond wiring the call.

→ **No fix needed beyond F-003.**

### 🟢 Suggestion

**F-007: `handle_status` in `creator works` duplicates ~200 lines from removed `run.rs` `Status` handler**

The `handle_status` function in `works/mod.rs` (lines 140–354) is a near-verbatim copy of the removed `RunCommand::Status` handler from `run.rs`. The migration is clean (hard-remove, no alias), but the duplication means future changes to status display must be made in one place. This is acceptable for now since the old code was removed, not duplicated.

→ **No action needed; note for future refactoring.**

---

**F-008: `novel_pool_entries` partial unique index is correct but `creator works use` handler is missing (F-001)**

The migration `202606100002_v141_multi_work_locks.sql` creates `novel_pool_entries` with `UNIQUE (creator_id, work_id)` and a partial unique index `WHERE status = 'active'` — this correctly enforces the "one active per creator" invariant (spec §1.1). However, since the daemon handler for `works use` is missing (F-001), this schema is never exercised through the CLI path. The schema design itself is sound.

→ **No schema change needed; fix F-001 to wire the handler.**

---

**F-009: `WorkPatch` has 30 fields — consider builder pattern for future maintainability**

`WorkPatch` (works.rs lines 151–201) now has 30 `Option` fields. Every call site that constructs a `WorkPatch` must list all fields (often with `..Default::default()`). This is verbose but correct. A builder pattern or derive macro could reduce boilerplate in future iterations but is not blocking for P0.

→ **Defer to future hygiene plan.**

---

**F-010: Test coverage is strong for orchestration + daemon layers but missing CLI→daemon integration tests**

The 8 hermetic tests (3 in `nexus-orchestration/tests/multi_work_switch.rs`, 3 in `nexus-daemon-runtime/tests/multi_work_switch.rs`, 5 CLI contract tests in `nexus42/tests/command_surface_contract.rs`) all pass. However, the CLI contract tests only verify `--help` output — they don't exercise the HTTP round-trip. The missing daemon routes (F-001, F-002) would have been caught by an integration test that actually calls the CLI against a running daemon.

→ **Add at least one integration test that exercises `creator works use` and `creator works completion-lock release` against a real daemon after fixing F-001.**

## Source Trace
- Finding ID: F-001
- Source Type: manual-reasoning (cross-referencing CLI HTTP calls vs daemon route table)
- Source Reference: `crates/nexus42/src/commands/creator/works/mod.rs:368,390` vs `crates/nexus-daemon-runtime/src/api/mod.rs:245-284`
- Confidence: High

- Finding ID: F-002
- Source Type: manual-reasoning (cross-referencing CLI JSON body fields vs daemon request struct)
- Source Reference: `crates/nexus42/src/commands/creator/run.rs:279-295` vs `crates/nexus-daemon-runtime/src/api/handlers/works.rs:141-152,273-312`
- Confidence: High

- Finding ID: F-003
- Source Type: git-diff + manual-reasoning (caller analysis of `write_completion_lock_for_work`)
- Source Reference: `crates/nexus-orchestration/src/auto_chain.rs:278-294`, grep for callers of `write_completion_lock_for_work` in daemon layer
- Confidence: High

- Finding ID: F-004
- Source Type: manual-reasoning (spec §4.1 vs implementation — no TTL logic found)
- Source Reference: `.mstar/knowledge/specs/novel-multi-work-lifecycle.md:109-114`, `crates/nexus-daemon-runtime/src/api/handlers/works.rs:809-816`
- Confidence: Medium

- Finding ID: F-005
- Source Type: manual-reasoning (spec §3 vs implementation — dual SSOT analysis)
- Source Reference: `.mstar/knowledge/specs/novel-multi-work-lifecycle.md:56-75`, `crates/nexus-orchestration/src/completion_lock.rs`, `crates/nexus-local-db/migrations/202606100002_v141_multi_work_locks.sql:6`
- Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 3 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

### Rationale

Three Critical findings (F-001, F-002, F-003) mean that core P0 features — `creator works use`, `creator works completion-lock release`, `--from-work` lineage, and the `.completion-lock.json` file — are not functional end-to-end. The CLI parses the commands correctly, the DB schema is correct, and the orchestration logic is correct, but the daemon HTTP layer is missing routes and handlers for the new endpoints, and the completion-lock file I/O is never invoked.

The architecture design (separation of `nexus-orchestration` logic from `nexus-daemon-runtime` HTTP layer from `nexus42` CLI surface) is sound and follows the established crate boundaries. The naming is consistent (`CompletionLock`, `completion_lock`, `runtime_lock_holder`, `novel_pool_entries`). The DB migration is well-structured with proper constraints. The test coverage at the unit/hermetic level is good. The issues are purely wiring gaps — missing daemon routes and missing caller invocations — not architectural flaws.

Once F-001, F-002, and F-003 are fixed, the architecture is coherent and maintainable. F-004 (stale lock TTL) and F-005 (dual SSOT) should be addressed or explicitly deferred with tracking.

## Revalidation (fix-wave delta: edf0a621..9b6627dd)

**Reviewer**: @qc-specialist (qc-specialist, reviewer_index: 1)
**Re-review timestamp**: 2026-06-10T20:30:00+08:00
**Re-review range**: `merge-base: 55689706` → `tip: 9b6627dd` (focus delta `edf0a621..9b6627dd`)
**Working branch (verified)**: iteration/v1.41
**Review cwd (verified)**: /Users/bibi/workspace/organizations/42ch/nexus
**Tools run**: cargo clippy, cargo +nightly fmt --check, cargo test, manual review of fix-wave diff

### Disposition

| Finding | Original severity | New severity | Disposition | Evidence |
|---------|-------------------|--------------|-------------|----------|
| F-001 (missing daemon routes) | critical | resolved | Wired in Fix 1 (commit `7c738164`) | `POST /v1/local/works/pool` route registered at `api/mod.rs:252`; `POST /v1/local/works/{work_id}/completion-lock/release` route registered at `api/mod.rs:264`; `set_pool_active` handler at `handlers/works.rs:1014` with transactional demote-promote; `release_completion_lock_handler` at `handlers/works.rs:1059` with DB-clear + file-delete; hermetic tests pass (0 failures) |
| F-002 (--from-work / --set-default dropped) | critical | resolved | Wired in Fix 2 (commit `7c738164`) | `CreateWorkRequest` extended with `lineage_from_work_id: Option<String>` (line 92) and `set_pool_active: Option<bool>` (line 158); `create_work` handler populates `WorkRecord.lineage_from_work_id` from request (line 344); `set_pool_active` promotion logic after creation (lines 367–380); all existing tests updated with new fields |
| F-003 (lockfile never written) | critical | resolved | Wired in Fix 1 (commit `7c738164`) | Supervisor `WorkComplete` path calls `write_completion_lock_if_available` at `supervisor.rs:486` → `write_completion_lock_for_work` at line 513; boot recovery `WorkComplete` path calls `write_completion_lock_for_work` at `boot.rs:331`; both paths are best-effort with warn logs; DB is SSOT |
| F-004 (runtime_lock TTL) | warning | defer | Optional Fix 6 skipped; residual R-V141P0-01 covers it | `status.json` residual `R-V141P0-01`: severity `high`, decision `defer`, target `V1.41 P-last or V1.42`; consistent with implementer report |
| F-005 (dual SSOT) | warning | resolved | Spec amendment §3.2 declares DB SSOT (commit `59f41dfd`) | `.mstar/knowledge/specs/novel-multi-work-lifecycle.md` §3.2: "DB column `works.completion_locked_at` is the authoritative lock state. The `.completion-lock.json` file is a derived artifact for cross-tool observation."; `completion_lock.rs` module doc and `release_completion_lock` doc both repeat SSOT declaration; `release_completion_lock_handler` clears DB first, then deletes file (best-effort) |

### New findings

None. The fix-wave delta (5 commits, 11 files, +551/-20 lines) is surgical and addresses exactly the 3 Critical + 2 Warning findings assigned to this reviewer. No new architectural concerns, code duplication, or maintainability risks introduced.

### Tools / verification tails

```
$ cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.20s
(clean — 0 warnings)

$ cargo +nightly fmt --all -- --check
(no output — clean)

$ cargo test -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db
test result: ok. 15 passed; 0 failed (nexus42)
test result: ok. 18 passed; 0 failed (nexus-daemon-runtime)
test result: ok. 24 passed; 0 failed (nexus-orchestration)
test result: ok. 12 passed; 0 failed (nexus-local-db)
(all doc-tests pass; 0 failures across all crates)
```

### Updated verdict

**Approve**

**Rationale**: All 3 Critical findings (F-001, F-002, F-003) are resolved with daemon routes, handlers, transactional pool logic, and lockfile write callers wired in both supervisor and boot recovery paths. F-005 (dual SSOT) is resolved with a clear spec amendment declaring DB as authoritative. F-004 (runtime_lock TTL) is explicitly deferred as residual R-V141P0-01 with target V1.41 P-last/V1.42. No new Critical or Warning findings. All CI tools pass clean. The architecture remains coherent with established crate boundaries (orchestration logic in `nexus-orchestration`, HTTP layer in `nexus-daemon-runtime`, CLI in `nexus42`).
