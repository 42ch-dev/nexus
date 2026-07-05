---
report_kind: qc_review
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-18-v1.51-advisory-lock
verdict: Approve
generated_at: 2026-06-18T13:42:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk (QC #2)
- Report Timestamp: 2026-06-18T13:42:00Z

## Scope
- plan_id: 2026-06-18-v1.51-advisory-lock
- Review range / Diff basis: iteration/v1.51...HEAD (= ca494f03...0c36f8c5)
- Working branch (verified): feature/v1.51-advisory-lock
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-b-p0
- Files reviewed: 20
- Commit range: 0c36f8c5 (docs: Completion Report) .. bc9d033a (feat: file_lock module)
- Tools run: cargo test -p nexus-local-db --test file_lock, cargo test -p nexus-daemon-runtime --test cron_lock_integration, cargo test -p nexus42 --test cli_lock_contention, cargo clippy --all -- -D warnings, cargo +nightly fmt --all --check, git diff, source tracing via read/grep on file_lock.rs + cron.rs + cron_supervisor.rs + errors.rs + status.json

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **S-1 (defence-in-depth)**: `try_acquire` and the call sites in `handle_set` / `maybe_acquire_cron_file_lock` trust that the `work_ref` string passed to `Works/<work_ref>/.lock` construction is already safe. Creation paths enforce `validate_work_ref` (via `novel_scaffold_sanitize`), and CLI/daemon obtain the value via DB resolution (`resolve_work_id_by_ref_or_id`) or from DB rows. Consider adding an explicit `debug_assert` or lightweight re-check at the lock acquisition boundary (or a `FileLockGuard::try_acquire_validated`) so a future code path cannot accidentally pass a raw unvalidated slug. Low risk today because the DB gate precedes path construction in all reviewed sites.
- **S-2 (observability)**: The "best-effort" skip when `work_dir.exists()` is false (CLI cron) or workspace_dir/work_ref is None (daemon) is correct for test environments and partial bootstraps, but the decision is silent in production logs at info level. Consider a single `debug!` line when skipping due to missing dir so operators can distinguish "no dir yet" from "lock contended".
- **S-3 (future hardening)**: Lock file writes (`std::fs::write`) and heartbeat updates are not `fsync`'d. For a 60 s advisory window this is acceptable (stale detection recovers), but if a future iteration tightens durability or adds crash-consistency requirements for the tombstone, a documented note + optional `File::sync_all` on the final write before spawn would be the extension point. No current correctness impact.

## Source Trace

**Finding ID**: Overall security/correctness trace (no per-finding IDs required; all items passed)
**Source Type**: git-diff + manual source review + test execution + spec cross-check
**Source Reference**:
- `crates/nexus-local-db/src/file_lock.rs` (try_acquire, parse_lock_body first/last colon split, format_lock_body, Drop unlock + heartbeat abort, read_lock_holder_info, zombie stale computation)
- `crates/nexus42/src/commands/creator/works/cron.rs:660` (file lock acquire before CAS using resolved work_ref + workspace_dir; `if work_dir.exists()` best-effort)
- `crates/nexus42/src/errors.rs:136` (CliError::Locked + Display with E_LOCK)
- `crates/nexus42/src/main.rs:88` (match on Locked → exit 75)
- `crates/nexus-orchestration/src/schedule/cron_supervisor.rs:249` (maybe_acquire_cron_file_lock) and `try_fire_role:349` (acquire before enqueue_cron_schedule)
- `.mstar/knowledge/specs/concurrency.md` §2.4 (lock ordering: "File lock BEFORE DB lock"), §3 (daemon), §4 (CLI), §5 (heartbeat 30 s / 60 s expiry), §6 (zombie detection on acquire success vs conflict)
- `crates/nexus42/tests/cli_lock_contention.rs`, `crates/nexus-local-db/tests/file_lock.rs`, `crates/nexus-daemon-runtime/tests/cron_lock_integration.rs`
- `.mstar/status.json` residual `R-V149P1-01` under `2026-06-17-v1.49-narrative-indexes` (closure_note explicitly distinguishes "advisory-lock portion" from "spec-reconciliation portion already closed V1.49 P-last")
**Confidence**: High

**Key verified properties**:
- `work_ref` for `Works/<work_ref>/.lock` is never taken raw from an untrusted CLI flag; it is either (a) resolved via `resolve_work_id_by_ref_or_id` (DB match on existing row) before path join, or (b) taken from a DB row (`row.work_ref`) populated at creation time under `is_valid_work_ref`.
- Lock body format `<pid>:<holder>:<expires_at_ms>`; parser splits first `:` (pid) + last `:` (expires) so holder names containing colons are preserved. No JSON, no format-string interpretation.
- `flock(LOCK_EX | LOCK_NB)` + `Unlock` on drop; kernel auto-release on process death. No manual `unlink` while a holder may exist.
- Acquire order documented and followed: file lock before any DB mutation (CAS or enqueue).
- Heartbeat: spawned after successful flock + initial write; cancelled + aborted on Drop. Stale (>60 s) is detected on conflict (sets `stale=true`) and on acquire-over-stale (just overwrites).
- TOCTOU for zombie double-acquire is prevented by `flock` kernel serialization: only one process wins the lock; the loser sees the (new) metadata.
- PID reuse mitigated by holder_name (e.g. `cli:cron-set`, `daemon:schedule:...`) + short 60 s expiry + fresh process creating the guard.
- E_LOCK: stable variant, exit 75 (EX_TEMPFAIL), message contains only holder_name + pid + optional (STALE); no absolute paths, no secrets.
- CLI contention example matches spec: `E_LOCK: work is held by ... pid=...`.
- R-V149P1-01 closure note is specific to the advisory-lock portion.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## Verdict Reasoning

All acceptance-focus items for security/correctness were reviewed and pass:

- **Lock file path traversal**: `work_ref` reaches the `Works/<work_ref>/.lock` path construction only after DB resolution (`resolve_work_id_by_ref_or_id`) or from a DB row that was created under `validate_work_ref`. No direct user-controlled raw slug bypasses the gate. The directory join is therefore safe.
- **Lock file content**: Plain `<pid>:<holder_name>:<expires_at_ms>` with first/last-colon parsing that correctly tolerates colons inside holder names. Written via `std::fs::write`; no injection surface.
- **Deadlock potential**: Explicitly documented in `concurrency.md` §2.4 and `file_lock.rs` module header ("File lock BEFORE DB lock. Never the reverse."). Code follows the order (acquire guard, then CAS/enqueue inside the scope).
- **`flock` semantics**: Correct use of `LOCK_EX | LOCK_NB` + `Unlock`; kernel releases on death; lock file intentionally left as tombstone (no corrupting recovery path).
- **Heartbeat race**: If the heartbeat task dies or the process crashes after flock but before/ during a write, the file may contain stale or empty metadata. The next successful acquirer overwrites; zombie detection on conflict surfaces `(STALE)`. Acceptable for advisory 60 s window.
- **Zombie detection TOCTOU**: `flock` itself is the serializing primitive. Two would-be acquirers cannot both hold the lock simultaneously; the loser always observes the winner's (or previous) metadata. No double-acquire window.
- **PID reuse**: Holder name + short expiry + the fact that a new process creates a fresh guard provide mitigation. Display-only pid does not confer authority.
- **E_LOCK exit code path**: `CliError::Locked` → `main.rs` match → `std::process::exit(75)`. Stable, documented as EX_TEMPFAIL.
- **CLI contention message**: "E_LOCK: work is held by <name> pid=<pid>[ (STALE)]" — no sensitive data leakage.
- **R-V149P1-01 closure**: The residual entry under the V1.49 plan explicitly states that the spec-reconciliation portion was closed in V1.49 P-last and that the advisory-lock portion is closed here with evidence (file_lock module, daemon + CLI integration, tests, spec body). The note is specific as required.

Tests required by the plan (file_lock, cron_lock_integration, cli_lock_contention) all pass. `cargo clippy --all -- -D warnings` and `cargo +nightly fmt --all --check` are clean. No Critical or Warning findings. Suggestions are low-impact hardening/observability items that do not block approval.

Verdict: **Approve**.
