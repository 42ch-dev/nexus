# Concurrency — Master Specification

**Status**: Draft (V1.51 T-B P0)
**Document class**: Master
**Created**: 2026-06-18
**Scope**: Multi-writer concurrency control for the local-first daemon + CLI model — advisory file lock + heartbeat + zombie detection + CLI integration.
**Coordinates with**:

- [novel-writing/multi-work-lifecycle.md](novel-writing/multi-work-lifecycle.md) — §4-§5 DB-level `runtime_lock_holder`
- [novel-writing/cron-staggering.md](novel-writing/cron-staggering.md) — §4 daemon cron evaluator
- [novel-writing/workflow-profile.md](novel-writing/workflow-profile.md) — completion locking
- [cli-spec.md](cli-spec.md) — `creator works cron set`, `creator run`, `creator world kb adopt`
- [daemon-runtime.md](daemon-runtime.md) — daemon tick / cron supervisor

**Iteration compass**: [v1.51-kb-closure-and-multi-writer-concurrency-delivery-compass-v1.md](../../iterations/v1.51-kb-closure-and-multi-writer-concurrency-delivery-compass-v1.md)
**Plan**: [2026-06-18-v1.51-advisory-lock.md](../../plans/2026-06-18-v1.51-advisory-lock.md)

---

## 1. Problem Statement

V1.42 P0 shipped the DB-level `works.runtime_lock_holder` column + `RuntimeLockGuard` RAII in `nexus_local_db::runtime_lock` — a single-process concurrency guard using SQLite's transactional semantics. V1.49 chose a single-writer daemon model with atomic temp+rename as the safe default (single `nexus42 daemon start` instance).

V1.50 introduced cron staggering (three-role per-Work automated scheduling), which creates **real multi-writer contention**:
- The daemon cron evaluator fires `novel-brainstorm` / `novel-write` / `novel-review-master` on a 1-minute tick.
- The author concurrently runs CLI commands like `creator works cron set` or `creator run outline-chapter`.

The DB-level lock (`runtime_lock_holder`) is process-local — it cannot protect against two independent `nexus42` processes (daemon + CLI) racing on `works.schedule_json`. The V1.50 P0 CAS fix (`set_schedule_json_tx`) added transactional compare-and-swap on `schedule_json`, but the broader class of cross-process races remains:

| Race class | Source | Risk |
|------------|--------|------|
| `schedule_json` read-modify-write | CLI `cron set` vs daemon cron-fire enqueue | Lost update |
| `kb_extract_jobs` status mutation | CLI `kb adopt` vs daemon review-time extract | Duplicate extract |
| `works` row mutation | CLI `creator run` vs daemon auto-chain | Inconsistent state |

A **hard cross-process advisory lock** (`flock` on `Works/<work_ref>/.lock`) is the V1.51 fix.

---

## 2. Lock Pattern

### 2.1 File-based Advisory Lock

A companion file `Works/<work_ref>/.lock` serves as the cross-process mutual exclusion point:

- **Acquire**: `flock(LOCK_EX | LOCK_NB)` — exclusive, non-blocking. If the lock is held by another process, `try_acquire` returns immediately with conflict information.
- **Release**: `flock(LOCK_UN)` on drop (RAII). The OS releases `flock` automatically if the process crashes.
- **Metadata**: The lock file body contains plaintext `<pid>:<holder_name>:<expires_at_ms>` for visibility and zombie detection (§6).

### 2.2 Lock File Format

```
<pid>:<holder_name>:<expires_at_ms>
```

- `<pid>` — OS process ID (decimal)
- `<holder_name>` — human-readable holder identity, one of:
  - `cli:<command_name>` — CLI mutating path (e.g., `cli:cron-set`, `cli:run-outline-chapter`)
  - `daemon:schedule:<schedule_id>` — daemon cron-fire enqueue
- `<expires_at_ms>` — Unix epoch milliseconds when the heartbeat expires (current time + 60 000 ms at acquisition; refreshed every 30 s)

Example: `12345:daemon:schedule:SCH20260618120000:1718700000000`

### 2.3 Acquisition Contract

```rust
/// Try to acquire the advisory file lock for a Work.
///
/// On success, returns a `FileLockGuard` that releases on drop.
/// On conflict, returns `Locked` with holder details (pid, name, staleness).
///
/// Never blocks — returns immediately.
pub fn try_acquire(work_dir: &Path, holder_name: &str) -> Result<FileLockGuard, Locked>;
```

Where `work_dir` is the resolved `Works/<work_ref>/` directory.

### 2.4 Lock Ordering

To prevent deadlocks:

1. **File lock BEFORE DB lock.** Acquire `FileLockGuard` before beginning any SQLite transaction that mutates `works` or related rows.
2. **Never acquire two file locks simultaneously** in the same process (single-Work scope per operation).

### 2.5 Relationship to DB-Level Runtime Lock

The file-based lock is a **companion** to the existing DB-level `runtime_lock_holder` (V1.42 P0). Both must be acquired for mutating paths:

| Layer | Mechanism | Scope | Purpose |
|-------|-----------|-------|---------|
| File lock | `flock` on `.lock` | Cross-process | Prevent daemon ↔ CLI races |
| DB lock | `runtime_lock_holder` column | Single-process | Prevent intra-process concurrency (two CLI commands) |

Acquiring the file lock does **not** automatically acquire the DB lock — callers must acquire both, in order (file lock first, then DB lock inside the transaction).

---

## 3. Daemon-Side Cron Integration

### 3.1 Cron-Fire Enqueue (try_fire_role)

Before enqueuing a cron-triggered schedule (via `auto_chain::enqueue_cron_schedule`), the daemon must acquire the file lock for the target Work:

1. Resolve `Works/<work_ref>/` directory from the Work record.
2. Call `try_acquire(work_dir, holder_name)` where `holder_name` is `daemon:schedule:<new_schedule_id>`.
3. On success: proceed to enqueue; the `FileLockGuard` must remain alive through the enqueue + DB write.
4. On `Locked`: skip this fire with `info!("cron-supervisor: work is file-locked by {holder}, skipping fire")`; increment `skipped_gated` counter.

### 3.2 Cron Evaluator (Read-Only)

The cron evaluator (`cron_fires_at_minute`, `has_active_role_schedule`) is **read-only** on `works.schedule_json` and `creator_schedules`. It does **not** need the file lock. This is documented here for clarity — the V1.50 CAS fix ensures the evaluator never mutates `schedule_json`.

### 3.3 Per-Work Cron Evaluator Reads

All per-Work cron evaluator reads (schedule_json parse, idempotency guard COUNT) are **read-only** and do not acquire the file lock.

---

## 4. CLI-Side Integration

### 4.1 Mutating Commands (Lock Required)

The following CLI paths must acquire the file lock before mutating any `works` or `kb_extract_jobs` rows:

| Command | Crate path | Holder name |
|---------|------------|-------------|
| `creator works cron set` | `nexus42::commands::creator::works::cron::handle_set` | `cli:cron-set` |
| `creator run` (all subcommands that mutate) | `nexus42::commands::creator::run::handle_run` | `cli:run` |
| `creator world kb adopt` | `nexus42::commands::creator::world::handle_adopt` | `cli:kb-adopt` |

### 4.2 Contention Behavior

When a CLI command cannot acquire the file lock (another process holds it):

- Return `CliError::Locked { holder_pid, holder_name, stale }` — a stable error variant.
- Exit with code **75** (EX_TEMPFAIL — stable across Unix). This is the canonical sysexits code for temporary failure due to resource contention.
- Print a user-friendly message: `E_LOCK: work is held by <holder_name> pid=<holder_pid>; retry after the holder releases`

### 4.3 Read-Only Commands (Lock NOT Required)

Commands that only read state do **not** acquire the file lock:
- `creator works cron show`
- `creator works cron list`
- `creator works status` (informational: displays lock holder if present, but does not acquire)

---

## 5. Heartbeat Protocol

### 5.1 Refresh Interval

The holder must refresh the `expires_at_ms` field in the lock file every **30 seconds** while the lock is held. This is managed by a background tokio task spawned inside `FileLockGuard::new`.

### 5.2 Write Protocol

The heartbeat task:
1. Seeks to the start of the `.lock` file.
2. Writes `<pid>:<holder_name>:<expires_at_ms>` where `expires_at_ms = now_ms + 60_000`.
3. Flushes.

If the write fails (e.g., disk full), the heartbeat task logs at `error!` and continues retrying. The lock validity window is 60 s — a single missed heartbeat is tolerated.

### 5.3 Shutdown

When `FileLockGuard` is dropped:
1. The heartbeat task is cancelled (abort handle).
2. `flock(LOCK_UN)` is called on the file descriptor.
3. The lock file is **not** deleted — the content serves as a tombstone for the next acquirer to detect zombie state (§6).

---

## 6. Zombie Detection

### 6.1 Definition

A lock is **stale** (zombie) when `expires_at_ms` in the lock file content is more than 60 seconds in the past. This indicates:

- The holder process crashed (OS released `flock` automatically).
- The holder's heartbeat thread died.
- The system clock jumped.

### 6.2 Detection on Acquire

When `try_acquire` succeeds (i.e., `flock` returns without contention), the acquirer reads the existing lock file content:

1. If the file is empty or newly created → fresh lock; write metadata and proceed.
2. If `expires_at_ms` < `now_ms - 60_000` → the previous holder was a zombie. Log at `warn!` with the stale holder details. Overwrite with fresh metadata and proceed (the lock was already released by the OS).
3. If `expires_at_ms` ≥ `now_ms - 60_000` → the previous holder released cleanly but didn't delete the file. Normal — overwrite and proceed.

### 6.3 Detection on Conflict

When `try_acquire` fails (i.e., `flock` returns `EAGAIN` or `EWOULDBLOCK`), the caller reads the lock file content to report holder information:

- Parse `<pid>:<holder_name>:<expires_at_ms>` from the file.
- If `expires_at_ms` < `now_ms - 60_000` → set `Locked::stale = true`.
- If `expires_at_ms` ≥ `now_ms - 60_000` → set `Locked::stale = false`.
- Set `Locked::holder_pid` and `Locked::holder_name` from the file.

The `stale` flag is **informational** — the caller cannot break another process's `flock`. The flag signals to the user/operator that the holder has not refreshed its heartbeat and may be deadlocked.

### 6.4 Stale Lock Recovery

The only safe recovery for a zombie lock is to kill the holder process (SIGTERM) so the OS releases `flock`. The `stale` flag in `Locked` enables automated recovery in future iterations (e.g., daemon supervisor kills stale holders).

---

## 7. Status Visibility

### 7.1 `creator works status --json`

The JSON output includes an optional `lock_holder` field:

```json
{
  "work_id": "...",
  "lock_holder": null,
  ...
}
```

When a lock is held:

```json
{
  "work_id": "...",
  "lock_holder": {
    "pid": 12345,
    "holder_name": "daemon:schedule:SCH20260618120000",
    "expires_at_ms": 1718700000000
  },
  ...
}
```

### 7.2 `creator world show <work_ref>`

Same `lock_holder` field included in the output.

### 7.3 Implementation

The lock holder is read from the `.lock` file content only — it does not require acquiring the lock. The read is best-effort: if the file doesn't exist or is unreadable, `lock_holder` is `null`.
