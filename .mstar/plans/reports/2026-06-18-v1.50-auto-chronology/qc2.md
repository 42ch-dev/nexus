---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-18-v1.50-auto-chronology
working_branch: feature/v1.50-auto-chronology
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-auto-chronology
review_range: merge-base eceb22507259b8d7f1f1ffbeacfc3258c4c8059e..44b03171edb3e399c287827af0d17e8254937c74
verdict: Approve
generated_at: 2026-06-17T15:30:06Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: security + correctness (focus: finish detection races, atomicity, SQL parameterization, path construction, manual override semantics, idempotency)
- Report Timestamp: 2026-06-17T15:30:06Z

## Scope
- plan_id: 2026-06-18-v1.50-auto-chronology
- Review range / Diff basis: merge-base eceb22507259b8d7f1f1ffbeacfc3258c4c8059e..44b03171edb3e399c287827af0d17e8254937c74
- Working branch (verified): feature/v1.50-auto-chronology
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-auto-chronology
- Files reviewed: 15 (core: orchestration auto_chronology.rs, daemon auto_chronology.rs, CLI chronology.rs, local-db works.rs + work_chapters.rs + migration, tests, template)
- Commit range: eceb2250..44b03171 (7 commits; 2319 insertions)
- Tools run: `cargo test -p nexus-orchestration --test auto_chronology_tick`, `cargo +nightly fmt --all --check`, `cargo clippy -p nexus-orchestration -p nexus42 -p nexus-local-db -p nexus-daemon-runtime -- -D warnings`, `git diff` + targeted file reads

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- Consider adding a directory `fsync` (or equivalent) after the successful `rename` in `write_outline_atomic` for stronger durability on filesystems where rename is not guaranteed to persist the parent dir entry without it. Current implementation matches the pre-existing V1.36 atomic-write pattern referenced by the spec and is acceptable for this feature (user-editable outline files); no correctness or security impact.
- The `last_advance_label` helper in the CLI does a best-effort filesystem scan of `Logs/chronology/` without path canonicalization or strict name validation. It is read-only and only used for display; still, a future hardening could restrict the scan to names strictly matching the expected `YYYY-MM-DD-advance-volN.md` pattern.

## Source Trace
- Finding ID: (N/A — no blocking findings)
- Source Type: manual code review + test execution + spec cross-check
- Source Reference: crates/nexus-orchestration/src/auto_chronology.rs (finish detection, perform_advance, write_outline_atomic, advance_log_path), crates/nexus42/src/commands/creator/works/chronology.rs (handle_advance + resolve_work_id), crates/nexus-local-db/src/{works.rs,work_chapters.rs} (list_works_with_auto_chronology, current_volume, is_volume_fully_finalized, seed_volume_chapters_tx), daemon wrapper, hermetic tests
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## Detailed Security + Correctness Review (per assignment focus)

**Finish detection (SQL + parameterization)**: All queries use `sqlx` parameterized binding (`.bind()`). No string concatenation into SQL. `list_works_with_auto_chronology`, `load_row_for_manual`, `current_volume`, `is_volume_fully_finalized`, and chapter seed all go through the DAO layer with bound parameters. No injection surface.

**TOCTOU / race between detection and advance**: `advance_auto` reads the gating row, then separately calls `current_volume` + `is_volume_fully_finalized`. State can change between the scan and the advance (CLI, another daemon instance, manual edits). The design intentionally relies on the final idempotent guard inside `perform_advance` (outline already exists → `AlreadyAdvanced` skip) plus the enclosing DB transaction. The daemon task is single-threaded per process (tokio interval with `Delay` missed-tick behavior); cross-process races are tolerated via idempotency. This matches the documented crash-recovery path and the spec (§3.1). No data corruption or duplicate-volume creation is possible. Acceptable.

**Atomic write path (temp + fsync + rename)**: `write_outline_atomic` creates `<target>.md.tmp`, `write_all` + `sync_all`, drops the handle, then `rename`. On any error the temp is best-effort removed. If rename fails the target is untouched. If the subsequent DB tx fails, the outline file may exist on disk while the DB state is unchanged; the next tick (or manual re-invocation) sees the outline and returns `AlreadyAdvanced` — exactly the intended recovery semantics documented in the module header and `SkipReason`. Directory creation uses `create_dir_all` before the write. No path traversal: `outline_path` and `advance_log_path` are built from DB-resolved `work_ref` + integer volume + a chrono-derived date string. No user-controlled path components reach the filesystem write paths for the advance.

**DB transaction for chapter seed**: `perform_advance` opens a tx, optionally calls `seed_volume_chapters_tx` (which loops with `INSERT OR IGNORE`), does a single `UPDATE works SET updated_at`, then commits. Any failure before `tx.commit()` drops the transaction (rollback). Seed loop failures mid-batch leave the tx uncommitted. The `OR IGNORE` + outline-existence guard makes retries safe. Correct.

**Manual override (`creator works chronology advance --volume N+1`)**: `advance_manual` deliberately bypasses the finish-detection gates (`load_row_for_manual` ignores `auto_chronology`). It still enforces the idempotent outline guard. Per spec §2.2 this must always be allowed; the implementation and the hermetic test `chronology_advance_round_trip` (explicitly run against a Work with the flag `false`) confirm it. Correct.

**Log file path construction**: `advance_log_path` uses:
- `work_ref` — resolved via `resolve_work_id` (DAO, creator/workspace scoped) before any path is built;
- `date_utc` — always `now_utc()` → `chrono::Utc::now().to_rfc3339()` (trusted);
- `next_volume` — computed as `prev + 1` (auto) or taken from validated `clap` `i32` (manual).
No `..`, `/`, or other traversal sequences can be injected. The log write is append-only after directory creation; failures are logged at WARN and non-fatal. Safe.

**Stable error codes + idempotency on retried ticks**: `AdvanceOutcome` is a clean enum (`Advanced` vs `Skipped { reason: SkipReason }`). `SkipReason` is exhaustive and stable. `run_one_tick` treats per-Work errors as non-fatal (log + continue). The outline-existence check at the very start of `perform_advance` guarantees that a retried tick after partial progress (outline written, tx not yet committed, or crash) is a clean no-op. Hermetic tests exercise the positive case, the four negative edges (`IntakeIncomplete`, `RuntimeLocked`, `CompletionLocked`, `VolumeNotFinalized`), `AlreadyAdvanced`, and the atomic-write + idempotency recovery path. All 9 tests pass.

**Additional observations**:
- Error handling is explicit; no `unwrap`/`expect` on fallible paths in production code.
- No new `#[allow(...)]` lints were added.
- `cargo +nightly fmt --all --check` and `cargo clippy ... -D warnings` are clean on the touched crates.
- Workspace-root discovery for CLI advance fails with a clear message if the user is not inside a workspace; no silent fallback.
- All new public functions have doc comments referencing the spec sections.

## Checklist (shared baseline + security/correctness focus)

- [x] Naming clear and consistent.
- [x] Responsibilities not overly mixed (orchestration owns the advance logic; daemon is a thin periodic wrapper; CLI is a thin dispatcher + resolver).
- [x] Error handling explicit and actionable.
- [x] Input validated / parameterized (SQL) or constructed from trusted/DB-resolved values (paths).
- [x] No injection / path traversal / permission issues identified.
- [x] State transitions (skip reasons, idempotency) coherent and documented.
- [x] Atomicity and rollback semantics match the spec and the documented crash-recovery contract.
- [x] Tests cover the security/correctness-critical paths (finish detection edges + idempotent advance).
- [x] Manual override honors the spec requirement to work regardless of the `auto_chronology` flag.
- [x] CI gates (fmt + clippy -D warnings) pass on changed crates.

**Verdict**: Approve
