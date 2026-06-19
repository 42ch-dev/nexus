# Completion Report v2

**Agent**: fullstack-dev-2
**Task**: V1.51 T-B P0 — `Works/<work_ref>/.lock` advisory lock + daemon cron-side + CLI integration
**Status**: Done (all 11 acceptance criteria pass)
**Plan ID**: 2026-06-18-v1.51-advisory-lock

---

## Summary

Implemented file-based advisory lock (`flock` + heartbeat + zombie detection) at `Works/<work_ref>/.lock`, integrated into daemon cron-side (cron supervisor `try_fire_role`) and CLI-side (`creator works cron set`), with `E_LOCK` exit code 75, `lock_holder` field in `creator works status --json`, and spec body authored for `knowledge/specs/concurrency.md`. Closed R-V149P1-01 advisory-lock portion.

---

## Artifacts

| File | New/Modified | Lines | Description |
|------|-------------|-------|-------------|
| `.mstar/knowledge/specs/concurrency.md` | New | 229 | Master spec §1-§6 (problem, lock pattern, daemon, CLI, heartbeat, zombie) |
| `crates/nexus-local-db/Cargo.toml` | Modified | +3 | Added `nix = "0.28"` under `[target.'cfg(unix)'.dependencies]` |
| `crates/nexus-local-db/src/file_lock.rs` | New | 494 | `FileLockGuard` RAII, `try_acquire`, `Locked`, heartbeat, zombie detection |
| `crates/nexus-local-db/src/lib.rs` | Modified | +1 | `#[cfg(unix)] pub mod file_lock;` |
| `crates/nexus-local-db/tests/file_lock.rs` | New | 77 | 3 integration tests (concurrent tasks, lock_holder_info, zombie) |
| `crates/nexus-orchestration/src/schedule/cron_supervisor.rs` | Modified | ~90 | `workspace_dir` param through chain, `maybe_acquire_cron_file_lock`, file lock acquisition before enqueue |
| `crates/nexus-orchestration/tests/cron_supervisor.rs` | Modified | ~40 | Updated `evaluate_cron_fires` calls with `None` workspace_dir |
| `crates/nexus-orchestration/tests/review_cron_e2e.rs` | Modified | +2 | Same |
| `crates/nexus-daemon-runtime/src/cron_supervisor.rs` | Modified | ~19 | Added workspace_dir param, passed `Some(workspace_dir)` |
| `crates/nexus-daemon-runtime/src/boot.rs` | Modified | +6 | Resolve and pass `workspace_path` to `spawn_cron_supervisor` |
| `crates/nexus-daemon-runtime/tests/cron_lock_integration.rs` | New | 169 | 3 integration tests (skip, handle, block) |
| `crates/nexus42/src/errors.rs` | Modified | +22 | `CliError::Locked` variant with display + exit code 75 doc |
| `crates/nexus42/src/main.rs` | Modified | +8 | Exit code 75 on `CliError::Locked` |
| `crates/nexus42/src/commands/creator/works/cron.rs` | Modified | +30 | `handle_set` acquires file lock before CAS write; workspace_dir resolution |
| `crates/nexus42/src/commands/creator/works/mod.rs` | Modified | +43 | `read_lock_holder_json` for status JSON enrichment |
| `crates/nexus42/tests/cli_lock_contention.rs` | New | 42 | 3 tests (E_LOCK display, stale, matches pattern) |
| `crates/nexus42/tests/works_status_lock_holder.rs` | New | 38 | 2 tests (serialization, null when missing) |
| `.mstar/status.json` | Modified | ~5 | R-V149P1-01 lifecycle: deferred→resolved |

---

## Spec Body Authored

- **`knowledge/specs/concurrency.md`** — Master class, status flipped **Draft** (body authored + impl in same iteration; P-last will promote to Normative after QC/QA).
- §1: Problem statement (multi-writer contention post-V1.50 cron staggering)
- §2: Lock pattern (`flock`, file format `<pid>:<holder>:<expires_at_ms>`, acquisition contract, lock ordering)
- §3: Daemon-side cron integration (`try_fire_role` acquires before enqueue)
- §4: CLI-side integration (mutating commands, contention → E_LOCK exit 75)
- §5: Heartbeat protocol (30 s refresh, 60 s expiry)
- §6: Zombie detection (stale `expires_at_ms` reading, informational recovery)

---

## Residual Closure

**R-V149P1-01** (advisory-lock note portion):

- **lifecycle**: `deferred` → `resolved`
- **closed_at**: 2026-06-18
- **closure_evidence**: `feature/v1.51-advisory-lock` (commit `8f625a7f`)
- **Test names**: `file_lock::tests::test_acquire_and_release_via_drop`, `file_lock::tests::test_second_acquire_fails_with_locked_info`, `file_lock::tests::test_stale_lock_file_overwritten_on_acquire`, `file_lock::tests::test_read_lock_holder_info_stale`, `cron_lock_integration::file_lock_blocks_cron_fire_when_held`, `cli_lock_contention::locked_error_display_shows_holder_info`
- Note: Spec-reconciliation portion already closed V1.49 P-last; only advisory-lock portion was in scope.

---

## Verification

### 1. `cargo test -p nexus-local-db --test file_lock`
```
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured
```

### 2. `cargo test -p nexus-daemon-runtime --test cron_lock_integration`
```
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured
```

### 3. `cargo test -p nexus42 --test cli_lock_contention`
```
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured
```

### 4. `cargo test -p nexus42 --test works_status_lock_holder`
```
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured
```

### 5. `cargo test -p nexus-orchestration -- cron_supervisor` (regression)
```
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured (cron_supervisor)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured (review_cron_e2e)
```

### 6. `cargo clippy --all -- -D warnings`
```
Finished `dev` profile — no errors, no warnings
```

### 7. `cargo +nightly fmt --all --check`
```
fmt check clean
```

### Additional: unit tests
```
cargo test -p nexus-local-db --lib file_lock: 12 passed
```

---

## Concurrency Safety Analysis

### Heartbeat Design

- Background tokio task spawned when `try_acquire` succeeds
- Refreshes `expires_at_ms` field in `.lock` file every 30 s via `std::fs::write`
- `expires_at_ms = now_ms + 60_000` (60 s window)
- Heartbeat cancelled (`abort`) on `FileLockGuard::drop`
- `flock` is the primary mutual exclusion; heartbeat is for visibility + zombie detection

### Zombie Detection

- `try_acquire` reads `.lock` file content when `flock` acquisition fails
- If `expires_at_ms` is > 60 s in the past → `Locked::stale = true`
- A new acquirer encountering a stale lock file (dead process, `flock` released by OS) overwrites stale metadata — the OS handles the actual lock release on process death
- Heartbeat thread crash without process death: `flock` still held → `try_acquire` fails → reports `stale: true` but cannot break the lock (only `SIGTERM` can)

### Edge Cases

| Scenario | Behavior |
|----------|----------|
| Process crashes | OS releases `flock`, `.lock` file has stale metadata → next acquirer overwrites |
| Heartbeat thread crashes | `flock` still held → `stale: true` reported, lock cannot be broken |
| Two processes race on `flock` | Kernel serialises via advisory lock — one wins, one gets `Locked` |
| Disk full during heartbeat | `std::fs::write` fails, logged at `error!`, lock validity window is 60 s |
| Clock jump backwards | `expires_at_ms` unchanged but `now_ms` smaller → stale not triggered prematurely |
| Clock jump forwards | `expires_at_ms` suddenly in the past → `stale: true` reported prematurely (acceptable: false positive on staleness is safer than false negative) |
| NFS / network filesystem | `flock` may not work reliably on NFS — documented as POSIX requirement in spec |

---

## Risks / Follow-ups

- **T-B P1**: Per-row version columns + CAS pattern (generalise `set_schedule_json_tx` pattern to `kb_extract_jobs` + `novel_pool_entries`; `E_VERSION` stable code) — complementary to this plan's advisory lock.
- **Cross-Work advisory lock** (V1.51 §1.2 O14): currently per-Work only; cross-Work serialisation deferred.
- **Lock priority / FIFO cron ordering**: not in scope.
- **Windows support**: `file_lock` is `#[cfg(unix)]`; Windows needs a different flock alternative.
- **NFS**: POSIX `flock` is not reliable on NFS; documented limitation.

---

## Git Context

```
Worktree path: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-b-p0
Working branch: feature/v1.51-advisory-lock
Merge base: ca494f03 (iteration/v1.51 HEAD)
HEAD: 8f625a7f

Commits:
8f625a7f feat: integration tests + residual closure (V1.51 T-B P0 T7+T8)
e49ffb2c feat(nexus42): lock_holder field in creator works status --json (V1.51 T-B P0 T6)
216e08e3 feat(nexus42): CLI file lock integration + E_LOCK exit code 75 (V1.51 T-B P0 T4+T5)
66d66b88 feat(nexus-orchestration): daemon cron-side file lock acquisition (V1.51 T-B P0 T3)
bc9d033a feat(nexus-local-db): file_lock module + Works/<work_ref>/.lock advisory lock + concurrency.md spec
```

---

## Acceptance Criteria Checklist

1. ✅ `Works/<work_ref>/.lock` file-based lock with `flock` + heartbeat; hermetic test acquires + releases
2. ✅ `nexus_local_db::file_lock::FileLockGuard` RAII releases on drop; hermetic test verifies drop-on-scope-exit
3. ✅ Daemon cron-side: cron-fire enqueue acquires file lock; hermetic test simulates daemon holding lock + CLI attempting to acquire
4. ✅ CLI-side: file lock acquisition in `handle_set` → `E_LOCK` exit 75 on contention
5. ✅ `creator works status --json` includes `lock_holder` field (nullable)
6. ✅ Lock holder stale (>60 s without heartbeat) is auto-detected; hermetic test
7. ✅ R-V149P1-01 advisory-lock note closed with evidence (commit hash + test names) in status.json
8. ✅ Spec body authored for `knowledge/specs/concurrency.md` Master with §1-§6 sections
9. ✅ Wire contracts unchanged (no `schemas/` change)
10. ✅ No `#[allow(clippy::*)]` without justification comment (only `#[allow(deprecated)]` for nix `flock` with rationale, `#[allow(clippy::cast_possible_truncation)]` for `as_millis() as u64` with rationale)
11. ✅ No race condition regressions — all 22 cron_supervisor + 2 review_cron_e2e regression tests pass; file_lock tests use `--test-threads=1` for determinism
