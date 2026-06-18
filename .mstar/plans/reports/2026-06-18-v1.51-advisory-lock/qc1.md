---
report_kind: qc_review
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-18-v1.51-advisory-lock
verdict: Approve
generated_at: 2026-06-18T00:00:00Z
revalidated_at: 2026-06-18T00:00:00Z
---

# Code Review Report

## Reviewer Metadata

- **Reviewer**: @qc-specialist
- **Runtime Agent ID**: `qc-specialist`
- **Runtime Model**: zhipuai-coding-plan/glm-5.2
- **Reviewer Index**: 1
- **Review Perspective**: Architecture coherence and maintainability risk
- **Report Timestamp**: 2026-06-18T00:00:00Z

## Scope

- **plan_id**: `2026-06-18-v1.51-advisory-lock`
- **Review range / Diff basis**: `iteration/v1.51...HEAD` (= `ca494f03...0c36f8c5`)
- **Working branch (verified)**: `feature/v1.51-advisory-lock`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-b-p0` (from `git rev-parse --show-toplevel`)
- **Files reviewed**: 20 (1504 insertions, 55 deletions)
- **Commit range**: `ca494f03..0c36f8c5` (6 commits)
- **Tools run**:
  - `git diff` / `git show` over the full range
  - `cargo test -p nexus-local-db --test file_lock` (3/3 pass)
  - `cargo test -p nexus-local-db --lib file_lock` (12/12 pass)
  - `cargo test -p nexus-daemon-runtime --test cron_lock_integration` (3/3 pass)
  - `cargo test -p nexus42 --test cli_lock_contention` (3/3 pass)
  - `cargo test -p nexus42 --test works_status_lock_holder` (2/2 pass)
  - `cargo test -p nexus-orchestration --test cron_supervisor` (22/22 pass — regression)
  - `cargo test -p nexus-orchestration --test review_cron_e2e` (2/2 pass — regression)
  - `cargo clippy --all -- -D warnings` (clean)
  - `cargo +nightly fmt --all --check` (clean)

## Findings

### 🔴 Critical

None.

### 🟡 Warning

#### W-001 — Incomplete CLI mutation-path coverage vs. plan Goals §4.4 and assignment acceptance focus

**Where**: `crates/nexus42/src/commands/creator/run/` (no diff), `crates/nexus42/src/commands/creator/world/kb.rs` (no diff).

**Observation**: Plan §4.4 (Goals) commits to acquiring the file lock on **three** mutating CLI commands:

> 4. CLI-side: every mutating command (`creator works cron set`, `creator run`, `creator world kb adopt`) acquires file lock; on contention, returns `E_LOCK: work is held by <pid>:<holder>` with stable exit code 75.

The Assignment's acceptance focus repeats the same three sites:

> - CLI integration in `creator works cron set` (handle_set) + `creator run` + `creator world kb adopt` is non-invasive (no API surface changes outside scope).

The implementation only integrated **`creator works cron set` (`handle_set`)**. Verified by `git diff ca494f03..0c36f8c5 -- crates/nexus42/src/commands/creator/{run,world/kb.rs}` returning no changes.

The spec body (`knowledge/specs/concurrency.md` §4.1) also lists all three commands as lock-required.

The completion report (lines 169–181) verifies only AC #4 (`creator works cron set`) and does not surface the deferral of the remaining two commands in its Risks / Follow-ups section, nor register a residual finding for the gap. The follow-up note for T-B P1 covers *per-row OCC* for `kb_extract_jobs` + `novel_pool_entries`, which is a *different* mechanism (CAS) that complements — but does not replace — the cross-process advisory lock required by spec §4.1.

**Architecture impact**: Without the advisory lock on `creator run` and `creator world kb adopt`, the cross-process race window flagged in plan §1 (the read-modify-write on `works.schedule_json`, `kb_extract_jobs`, and `works` row mutations) is only closed for the cron-set path. The other two mutating paths remain vulnerable to the same daemon↔CLI race that motivated the plan.

**Fix**: Either (a) extend file lock acquisition to `creator run` mutating subcommands and `creator world kb adopt` in this plan, or (b) explicitly defer with a residual finding (e.g. `R-V151TB-P0-01`) that names the deferred commands, the rationale (e.g. handles-`work_id` vs. handles-`work_ref` path-resolution complexity), and the target plan. Update the spec §4.1 to mark the two commands as "deferred to <plan>" so the spec/code drift is honest. Update the completion report's Risks / Follow-ups to name the deferral.

---

#### W-002 — I/O errors during lock acquisition are silently mapped to a misleading `Locked { pid: 0, holder_name: "unknown" }`

**Where**: `crates/nexus-local-db/src/file_lock.rs:154-171` (`try_acquire`):

```rust
if let Some(parent) = lock_path.parent() {
    std::fs::create_dir_all(parent).ok();  // <-- error silently discarded
}

let fd = std::fs::OpenOptions::new()
    .read(true).write(true).create(true).truncate(false)
    .open(&lock_path)
    .map_err(|_e| Locked {
        holder_pid: 0,
        holder_name: "unknown".to_string(),
        expires_at_ms: 0,
        stale: false,
    })?;
```

**Observation**: When the parent directory cannot be created (permission denied) or the `.lock` file cannot be opened/created (disk full, FS unmounted, parent not a directory, MAC policy denial, etc.), the error is mapped to a synthetic `Locked` variant with `holder_pid: 0`, `holder_name: "unknown"`, `stale: false`. The CLI handler (`crates/nexus42/src/commands/creator/works/cron.rs:668-674`) then converts that to `CliError::Locked`, which `main.rs:90-94` maps to exit code **75 (`EX_TEMPFAIL`)** with the message:

```
E_LOCK: work is held by unknown pid=0; retry after the holder releases
```

This is semantically wrong: nobody holds the lock. The user is told to "wait for the holder to release the lock" when there is no holder — the real cause is an environment/permission problem. The user will waste time waiting/retrying/killing the daemon before discovering the underlying I/O issue.

**Architecture impact**: Exit code 75 specifically denotes *temporary* lock contention in Unix convention (`EX_TEMPFAIL`). Permission-denied / disk-full / missing-parent are not temporary contention; they are persistent environment errors that need operator intervention, not retry. Conflating them corrupts the contract of `E_LOCK` and the meaning of exit 75, which is the stability guarantee the plan advertises (plan §4 Goal 4: "stable exit code 75").

**Fix**: Introduce a distinct error path for acquisition-time I/O failures. Options (in order of preference):
1. Return `io::Error` wrapped in a new variant (e.g. `FileLockError::Io(io::Error)`) so the CLI can surface it as `CliError::Io` (exit 1) with the underlying cause; or
2. At minimum, log the underlying error at `error!` before mapping to `Locked`, and amend the spec §4.2 contention semantics to call out the conflation explicitly.

Drop the `.ok()` on `create_dir_all` so a real permission failure surfaces rather than being silently swallowed before the subsequent `OpenOptions.open` fails with a less-specific error.

---

### 🟢 Suggestion

#### S-001 — Spec/code drift on `holder_name` format

**Where**: `knowledge/specs/concurrency.md` §2.2 vs. `crates/nexus-orchestration/src/schedule/cron_supervisor.rs:263`.

Spec §2.2 commits the daemon holder name to `daemon:schedule:<schedule_id>`, e.g. `daemon:schedule:SCH20260618120000`. The implementation uses `daemon:schedule:cron-<role_name>` (e.g. `daemon:schedule:cron-brainstorm`).

The code's choice is arguably better (the `schedule_id` is only minted at enqueue time, so it isn't available before the lock is acquired; the role name is more immediately useful for debugging), but the spec text should match the implementation. Either:
- Update spec §2.2 to read `daemon:schedule:cron-<role>` (recommended), or
- Refactor code to capture the schedule_id post-enqueue and rewrite the holder name (more invasive, not justified by debugging value).

Confidence: High. Reproduced by reading the format string at `cron_supervisor.rs:263`.

---

#### S-002 — Test-file rustc warnings not caught by the project's clippy gate

**Where**: `crates/nexus-daemon-runtime/tests/cron_lock_integration.rs`.

`cargo test -p nexus-daemon-runtime --test cron_lock_integration --no-run` emits five `rustc` warnings:

1. Line 11 — unused import `tokio::sync::Notify`.
2. Lines 79, 105, 144 — three callsites of deprecated `tempfile::TempDir::into_path()` (should be `keep()`).

The project's CI gate (`cargo clippy --all -- -D warnings`) does **not** include `--tests` / `--all-targets`, so these warnings are invisible to CI but appear during local `cargo test`. The implementer's verification §6 ("no errors, no warnings") only reflects the lib/bins scope of `cargo clippy --all`, not test targets.

**Fix**: `cargo clippy --fix --tests -p nexus-daemon-runtime` then commit. Alternatively, run `cargo clippy --all --all-targets -- -D warnings` as an additional gate (separate decision; would affect CI command surface).

Confidence: High. Reproduced locally.

---

#### S-003 — Spec §5.2 heartbeat write protocol vs. `std::fs::write` implementation

**Where**: `knowledge/specs/concurrency.md` §5.2 vs. `crates/nexus-local-db/src/file_lock.rs:216-218` (heartbeat task body) and `:128-137` (`write_lock_metadata_to_path`).

Spec §5.2 prescribes:

> 1. Seeks to the start of the `.lock` file.
> 2. Writes `<pid>:<holder_name>:<expires_at_ms>` where `expires_at_ms = now_ms + 60_000`.
> 3. Flushes.

The implementation calls `std::fs::write(path, body)`, which is *truncate-then-rewrite* (not seek-in-place). The behavioural difference: a concurrent reader (`read_lock_holder_info`, used by `creator works status --json`) may briefly observe an empty file during the truncate→write window, returning `None` for `lock_holder` even though the lock is held.

The lock_holder info is documented as best-effort and informational (spec §7.3), so this is not a correctness bug, but it is a spec/code drift and a small observability gap (transient `null` lock_holder for ~µs during each 30 s heartbeat refresh).

**Fix**: Either update spec §5.2 to describe the truncate-then-rewrite behaviour explicitly, or change the heartbeat to open the existing `File`, `seek(SeekFrom::Start(0))`, `write_all`, `set_len` to truncate at the new length, and `flush` — keeping the same underlying file descriptor for atomic-in-place refresh.

Confidence: Medium. Reproduced by code inspection; observable impact not measured.

---

#### S-004 — `status.json` R-V149P1-01 `closure_evidence` is a placeholder

**Where**: `.mstar/status.json` (root `residual_findings`), entry `_plan_id: 2026-06-17-v1.49-narrative-indexes`:

```json
"closure_evidence": "feature/v1.51-advisory-lock commit (pending), test names: ..."
```

The string `commit (pending)` is a placeholder. The completion report (line 59) cites `feature/v1.51-advisory-lock` (commit `8f625a7f`), but the SSOT (`status.json`) has not been updated with the real commit hash.

**Fix**: After the implementer's branch is merged into `iteration/v1.51` (or after the QC report is committed and the plan is marked Done by PM/QA), PM/QA should substitute the real commit hash. If the placeholder is intentional pre-merge state, document that convention in `.mstar/AGENTS.md` so reviewers do not flag it again.

Per `mstar-review-qc` Residual Findings 留档门禁, this is not a blocker for QC approval (residual lifecycle is PM/QA-owned, not QC-owned).

Confidence: High.

---

#### S-005 — Spec §7.2 inaccuracy: `creator world show <work_ref>` argument and `lock_holder` claim

**Where**: `knowledge/specs/concurrency.md` §7.2.

Spec §7.2 reads:

> ### 7.2 `creator world show <work_ref>`
> Same `lock_holder` field included in the output.

Two inaccuracies:

1. The actual CLI signature is `creator world show <world_id>` (see `crates/nexus42/src/commands/creator/world/mod.rs:283` — `async fn run_show(config: &CliConfig, world_id: &str)`), not `<work_ref>`.
2. `run_show` prints world-level state (world_id, title, slug, status, canon_revision, timeline_head, time_pointer, created_at). It does not show any Work-bound row, and the file lock lives at `Works/<work_ref>/.lock` — a Work-scoped path, not a World-scoped one. The `lock_holder` field does not naturally apply here.

The implementation correctly omits `lock_holder` from `world show`, but the spec text over-promises. The binding AC #5 in the plan only mentions `creator works status --json`, which the implementer satisfied.

**Fix**: Amend spec §7.2 to either (a) drop the section entirely (lock_holder is works-status-only for V1.51 T-B P0), or (b) rewrite it to describe a future `creator works status <work_ref>` JSON contract that may eventually land in a separate spec.

Confidence: High. Reproduced by reading the world/mod.rs handler.

---

#### S-006 — Duplicated comment block in `try_fire_role`

**Where**: `crates/nexus-orchestration/src/schedule/cron_supervisor.rs:340-348`.

Two near-identical comment blocks are stacked:

```rust
// V1.51 T-B P0: acquire file lock before enqueuing (spec §3.1).
//
// Ok(None) → workspace dir missing (test env), skip lock.
// Err(())  → lock held, skip fire.
// Ok(Some) → acquired, proceed to enqueue.
// V1.51 T-B P0: acquire file lock before enqueuing (spec §3.1).
// Ok(None) → workspace dir missing (test env), skip lock but proceed.
// Err(())  → lock held, skip fire.
// Ok(Some) → acquired — guard held through enqueue, released on scope exit.
```

Likely a leftover from an edit pass. Cosmetic only; no behavioural impact. Confidence: High.

**Fix**: Keep only the second (more complete) comment block.

---

#### S-007 — Silent degradation to no-lock when `workspace_path` is unset on the daemon

**Where**: `crates/nexus-daemon-runtime/src/boot.rs:452` (`cron_ws_path = cron_workspace.unwrap_or_default()`) → `crates/nexus-orchestration/src/schedule/cron_supervisor.rs:254-260` (`maybe_acquire_cron_file_lock` returns `Ok(None)` when the path doesn't exist).

When the daemon starts without a resolved `workspace_path`, the cron supervisor receives an empty `PathBuf`. The `work_dir.exists()` check inside `maybe_acquire_cron_file_lock` returns `false` for the empty path, so the file lock is silently skipped on every cron fire — the daemon degrades to "DB-level lock only" with no log line alerting the operator.

The boot.rs inline comment ("defensive — a daemon without a workspace should not have schedule_json-bearing Works") is plausible, but if the assumption ever breaks (e.g. an env misconfiguration causes `workspace_path` to be unset on a real authoring daemon), the cross-process safety contract is silently lost.

**Fix**: Emit a single `warn!` on the first cron tick (or in boot.rs at spawn time) when `workspace_path` is `None`, naming the degraded mode and its consequence (no file-level cross-process lock; only DB-level guard active). Cheap, high-value for diagnosing misconfigurations.

Confidence: High. Reproduced by code inspection.

---

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|------------|------------------|------------|
| W-001 | git-diff + spec/plan text | `git diff ca494f03..0c36f8c5 -- crates/nexus42/src/commands/creator/{run,world/kb.rs}` (empty); plan §4.4 line 31; spec §4.1 lines 122-130; assignment acceptance focus | High |
| W-002 | manual-reasoning + git-diff | `crates/nexus-local-db/src/file_lock.rs:154-171` (`create_dir_all(parent).ok()` + `OpenOptions.open().map_err(\|_e\| Locked { pid: 0, ... })`); `crates/nexus42/src/commands/creator/works/cron.rs:666-679`; `crates/nexus42/src/main.rs:88-95` | High |
| S-001 | manual-reasoning + spec/code compare | spec `concurrency.md` §2.2 lines 56-62; `crates/nexus-orchestration/src/schedule/cron_supervisor.rs:263` | High |
| S-002 | linter (rustc, not clippy) | `cargo test -p nexus-daemon-runtime --test cron_lock_integration --no-run` output; `crates/nexus-daemon-runtime/tests/cron_lock_integration.rs:11,79,105,144` | High |
| S-003 | manual-reasoning + spec/code compare | spec `concurrency.md` §5.2 lines 154-161; `crates/nexus-local-db/src/file_lock.rs:128-137, 206-224` | Medium |
| S-004 | doc-rule | `.mstar/status.json` diff; completion report line 59 | High |
| S-005 | manual-reasoning + spec/code compare | spec `concurrency.md` §7.2 lines 235-237; `crates/nexus42/src/commands/creator/world/mod.rs:283` | High |
| S-006 | git-diff | `crates/nexus-orchestration/src/schedule/cron_supervisor.rs:340-348` | High |
| S-007 | manual-reasoning + code inspection | `crates/nexus-daemon-runtime/src/boot.rs:445-460`; `crates/nexus-orchestration/src/schedule/cron_supervisor.rs:244-268` | High |

## Summary

| Severity | Count | Resolved |
|----------|-------|----------|
| 🔴 Critical | 0 | — |
| 🟡 Warning | 2 | 2 (W-001, W-002) |
| 🟢 Suggestion | 7 | 0 (non-blocking) |

**Test verification** (run locally in Review cwd):

| Suite | Result |
|-------|--------|
| `cargo test -p nexus-local-db --test file_lock` | 3/3 pass |
| `cargo test -p nexus-local-db --lib file_lock` | 12/12 pass |
| `cargo test -p nexus-daemon-runtime --test cron_lock_integration` | 3/3 pass (5 rustc warnings — see S-002) |
| `cargo test -p nexus42 --test cli_lock_contention` | 3/3 pass |
| `cargo test -p nexus42 --test works_status_lock_holder` | 2/2 pass |
| `cargo test -p nexus-orchestration --test cron_supervisor` | 22/22 pass (regression) |
| `cargo test -p nexus-orchestration --test review_cron_e2e` | 2/2 pass (regression) |
| `cargo clippy --all -- -D warnings` | clean |
| `cargo +nightly fmt --all --check` | clean |

**Architecture coherence (reviewer focus)**: The core design is sound:

- `nexus_local_db::file_lock` module is well-shaped: `FileLockGuard` RAII + `tokio::spawn` heartbeat + `tokio::sync::watch` cancellation, all `Send`-auto-derived; `#[cfg(unix)]` consistently applied; lock file format `<pid>:<holder>:<expires_at_ms>` parseable via first-colon/last-colon split (handles colons in holder names like `cli:cron-set` and `daemon:schedule:cron-brainstorm`).
- Lock ordering is documented in three places (file_lock.rs header doc, spec §2.4, plan §1) and the implementation honours it: CLI `handle_set` acquires the file lock before opening the SQLite transaction (`cron.rs:660-745`); daemon cron-side acquires before `enqueue_cron_schedule` (`cron_supervisor.rs:340-391`).
- The `nix = "0.28"` dependency is correctly placed under `[target.'cfg(unix)'.dependencies]` (not dev-deps), with `features = ["fs"]` only — no dependency creep.
- `E_LOCK` exit code 75 (`EX_TEMPFAIL`) is added cleanly to `main.rs` via `matches!` pattern on `CliError::Locked`, with display in `errors.rs` carrying the stable code prefix.
- The spec body (`knowledge/specs/concurrency.md`) is structured coherently (§1 problem → §2 lock pattern → §3 daemon-side → §4 CLI-side → §5 heartbeat → §6 zombie detection → §7 status visibility) and the document class lifecycle is honest (`Status: Draft (V1.51 T-B P0)`, `Document class: Master`, with P-last promotion to Normative noted).
- R-V149P1-01 advisory-lock portion is correctly closed (lifecycle `resolved`, closure_note distinguishes the spec-reconciliation portion closed V1.49 P-last from the advisory-lock portion closed here). The closure does not conflate the two portions.

The two Warnings are localised and fixable without re-architecting: W-001 is a scope-completion gap (extend coverage or register residual); W-002 is an error-mapping fix (introduce an `Io` path or at minimum log the underlying cause).

## Verdict

**Original: Request Changes** — W-001 (incomplete CLI mutation-path coverage) and W-002 (I/O errors silently mapped to Locked) blocked approval.

**Revalidated: Approve** — both Warnings resolved (see §Revalidation below).

## Revalidation (2026-06-18)

- **Resolved: W-001 (Warning)** — `creator run` + `creator world kb adopt` now acquire `Works/<work_ref>/.lock` before mutating (commits `6dccee36` + `3444d046`). Both paths mirror the `creator works cron set` pattern: resolve `work_ref` from DB, acquire non-blocking `flock(LOCK_EX|LOCK_NB)`, return `E_LOCK` + exit 75 on contention or `E_LOCK_IO` + exit 78 on I/O failure. New hermetic tests: `creator_run_lock` (3) + `kb_adopt_lock` (3).
- **Resolved: W-002 (Warning)** — `FileLockError` enum (`Locked(Locked)` | `Io(io::Error)`) added; `try_acquire` propagates I/O errors with `?` (no more `.ok()` swallow). CLI maps `Locked` → exit 75 (`E_LOCK`, temporary contention) and `Io` → exit 78 (`E_LOCK_IO`, persistent config error). New hermetic test: `cli_lock_io_error` (5) + `test_io_error_surfaces_not_locked` (unit).
- **Evidence:** All 59 tests pass (47 pre-existing + 12 new), clippy clean (`-D warnings`), `cargo +nightly fmt --all` clean. Spec `concurrency.md` §2.3-2.4 updated with `FileLockError` type and dual exit-code contract.
- **Re-verdict:** **Approve** — both blocking Warnings resolved; architecture coherence preserved.
