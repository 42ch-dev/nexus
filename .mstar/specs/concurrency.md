# Concurrency — Master Specification

**Status**: Normative — V1.51 Shipped (T-B P0/P1 advisory lock + heartbeat + OCC), V1.56 P0 amendment (§9 workspace session OCC)
**Document class**: Master
**Created**: 2026-06-18
**Scope**: Multi-writer concurrency control for the local-first daemon + CLI model — advisory file lock + heartbeat + zombie detection + CLI integration.
**Coordinates with**:

- [novel-writing/multi-work-lifecycle.md](novel-writing/multi-work-lifecycle.md) — §4-§5 DB-level `runtime_lock_holder`
- [novel-writing/cron-staggering.md](../../archived/knowledge/novel-writing/cron-staggering.md) — §4 daemon cron evaluator (archived; folded into workflow-profile.md §11)
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
/// On conflict, returns `FileLockError::Locked` with holder details (pid, name, staleness).
/// On I/O failure, returns `FileLockError::Io(io::Error)` (permission denied, disk full, etc.).
///
/// Never blocks — returns immediately.
pub fn try_acquire(work_dir: &Path, holder_name: &str) -> Result<FileLockGuard, FileLockError>;
```

Where `work_dir` is the resolved `Works/<work_ref>/` directory.

### 2.4 Lock Ordering and Exit-Code Contract

To prevent deadlocks:

1. **File lock BEFORE DB lock.** Acquire `FileLockGuard` before beginning any SQLite transaction that mutates `works` or related rows.
2. **Never acquire two file locks simultaneously** in the same process (single-Work scope per operation).

The CLI maps `try_acquire` failures to distinct exit codes so operators can distinguish temporary contention from persistent I/O failures:

| Error | Stable code | Exit code | Meaning |
|-------|-------------|-----------|---------|
| `FileLockError::Locked` | `E_LOCK` | 75 (`EX_TEMPFAIL`) | Temporary contention — another process holds the lock; retry later. |
| `FileLockError::Io` | `E_LOCK_IO` | 78 (`EX_CONFIG`) | Persistent I/O failure — permission denied, disk full, or missing parent directory; operator intervention required. |

These error types are surfaced through the `CliError` enum:

- `CliError::Locked { holder_pid, holder_name, stale }` → `E_LOCK` + exit 75
- `CliError::LockIo(io::Error)` → `E_LOCK_IO` + exit 78

Callers must **never** map an I/O failure to `E_LOCK` or exit 75 — that would mislead operators into retrying against a permanent environment problem.

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
4. On `FileLockError::Locked`: skip this fire with `info!("cron-supervisor: work is file-locked by {holder}, skipping fire")`; increment `skipped_gated` counter.

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

### 4.2 Contention and I/O Error Behavior

When a CLI command cannot acquire the file lock because another process holds it:

- Return `CliError::Locked { holder_pid, holder_name, stale }` — a stable error variant.
- Exit with code **75** (`EX_TEMPFAIL`). This is the canonical sysexits code for temporary failure due to resource contention.
- Print a user-friendly message: `E_LOCK: work is held by <holder_name> pid=<holder_pid>; retry after the holder releases`.

When a CLI command cannot acquire the file lock because of an I/O failure (permission denied, disk full, missing parent directory):

- Return `CliError::LockIo(io::Error)` — a stable error variant.
- Exit with code **78** (`EX_CONFIG`). This signals a persistent environment/configuration error that requires operator intervention, **not** a retry.
- Print a user-friendly message: `E_LOCK_IO: could not acquire file lock (<error>); check filesystem permissions and disk space`.

Callers must **never** map an I/O failure to `E_LOCK` or exit 75 — temporary contention and persistent I/O failure are distinct failure modes with distinct exit codes.

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

## 7. Per-Row Optimistic Concurrency Control (OCC) — V1.51 T-B P1

### 7.1 Rationale

The advisory file lock (§2) serialises **intra-Work** mutating paths across processes, but does not guard against **logical state divergence** within the locked scope: if the daemon reads a row's state, then a concurrent internal path (e.g. an inline extractor) modifies the same row before the daemon's write lands, a stale preimage can overwrite a fresher write — even under the file lock.

Per-row OCC adds a **version column** (`INTEGER NOT NULL DEFAULT 0`) to contention-prone tables. Every mutating UPDATE that carries semantic intent (state transition, payload refresh) must:

1. Read the current version from the row (the **preimage**).
2. Issue `UPDATE ... SET ..., version = version + 1 WHERE id = ? AND version = ?`.
3. If `rows_affected == 0`, the version changed between read and write → retry or surface `E_VERSION` (exit 76).

### 7.2 Versioned Tables

| Table | Version column | Added | Rationale |
|---|---|---|---|
| `kb_extract_jobs` | `version` | V1.51 T-B P1 migration `202606190001` | Promotion status transitions (`mark_confirmed`, `upsert_pending_candidate`), LLM payload refresh (`insert_pending_with_llm`), and cron-side extract job mutation are multi-actor paths where a stale preimage can produce duplicate extracts or lost confirmations. |
| `novel_pool_entries` | `version` | V1.51 T-B P1 migration `202606190001` | Pool promote/demote is an `INSERT ... ON CONFLICT DO UPDATE` that can race with concurrent `archive`/`completed` transitions from the FL-E completion hook. |

`works.schedule_json` remains under its own column-value CAS (V1.50 P0 `set_schedule_json_tx`) because the compare-and-swap target is the JSON content itself, not a monotonic version counter.

### 7.3 CAS Helper (`nexus-local-db::cas`)

The `cas` module provides two primitives (source: `crates/nexus-local-db/src/cas.rs`):

- **`cas_check(pool, rows_affected, table, id_col, id_val, expected_version) -> Result<(), LocalDbError>`** — call after `UPDATE ... WHERE version = ?`. On `rows_affected == 0`, re-reads the current version and returns `VersionMismatch`.
- **`with_cas_retry(max_attempts, backoff_ms, name, f) -> Result<T, LocalDbError>`** — retries the closure `f` up to `max_attempts` times (default 3, 100 ms) when a `VersionMismatch` is caught. Logs `warn!` on each retry. Any other error is returned immediately without retrying.

### 7.4 KB-Side CAS Integration (adopt / rescan)

The `creator world kb adopt` path is the primary CAS consumer:

```
1. Read promotion row → version = V
2. Validate canonical_name + block_type → KeyBlock
3. Call mark_confirmed_in_tx_with_cas(tx, job_id, V)
   - UPDATE ... SET promotion_status='confirmed', version=version+1
     WHERE job_id = ? AND promotion_status='pending' AND version = V
4. rows_affected == 0 → check cause:
   - Row is confirmed/rejected → Ok(false) (already handled)
   - Version mismatch → Err(VersionMismatch) → E_VERSION exit 76
5. On success → commit KeyBlock + flip atomically
```

`upsert_pending_candidate` (V1.50 T-B P2) refreshes `proposed_payload` for an existing `pending` row. T-A P1 (cross-chapter rescan) and T-A P2 (missing-KB detection) **must** pass the version from their preimage read through this path to close the TOCTOU window.

### 7.5 Cron-Side CAS Retry

The daemon cron-fire enqueue path (`cron_supervisor::try_fire_role`) wraps `enqueue_cron_schedule` in a retry loop (3 attempts, 100 ms backoff). When a `VersionMismatch` propagates from a versioned-table mutation inside the fire scope, the loop re-reads the preimage and retries.

**Acquire-order discipline:** file lock → DB lock → CAS (never reverse). The CAS always executes **inside** the file-lock scope (§2.4). Two CAS-protected writes to different tables are sequenced by the file lock; no two-phase commit or distributed consensus is needed (local-only).

### 7.6 Exit Code Contract

| Error | Code | Exit | Meaning |
|---|---|---|---|
| `LocalDbError::VersionMismatch` | `E_VERSION` | 76 | Row was modified by another writer between read and write; retry the operation. |
| `FileLockError::Locked` | `E_LOCK` | 75 | Temporary file-lock contention; retry later. |
| `FileLockError::Io` | `E_LOCK_IO` | 78 | Persistent I/O failure; operator intervention required. |

All three codes are mapped in `apps/nexus42/src/main.rs` §Exit mapping.

### 7.7 Anti-Patterns

- **DO NOT** CAS-guard a write that is already fully serialised by the file lock and performs only INSERT (no read-modify-write). The `version` column adds a monotonic counter; INSERT is not a TOCTOU surface.
- **DO NOT** increment `version` on purely informational/read-path queries.
- **DO NOT** add a `version` column to tables outside the V1.51 scope (`kb_extract_jobs` + `novel_pool_entries`).
- **DO NOT** use `unsafe` for any CAS code.

---

## 8. Status Visibility

### 8.1 `creator works status --json`

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

### 8.2 `creator world show <work_ref>`

Same `lock_holder` field included in the output.

### 8.3 Implementation

The lock holder is read from the `.lock` file content only — it does not require acquiring the lock. The read is best-effort: if the file doesn't exist or is unreadable, `lock_holder` is `null`.

---

## 9. Workspace Session OCC (V1.56 P0)

### 9.1 Rationale

`workspace.open` and `workspace.commit` implement file-level optimistic concurrency control using content hashes. This prevents lost-update races when multiple actors attempt to write to the same workspace scope through different sessions.

### 9.2 Algorithm

1. **`workspace.open`**: Scans the target directory, computes SHA-256 content hashes for all regular files, stores `{relative_path: sha256_hex}` as JSON in the session snapshot (`workspace_sessions.file_hashes_json`).
2. **`workspace.commit`**: Accepts a `changes[]` manifest of `(path, content_hash, op)` tuples. For each `Modify` entry, verifies the current file hash matches the stored hash. For `Create`, verifies the file is NOT in the snapshot. For `Delete`, verifies the file IS in the snapshot. On any mismatch, rejects with `SessionError::HashConflict` (HTTP 409).
3. **Atomic consumption**: The session is atomically marked `consumed = 1` via `UPDATE...WHERE consumed = 0 AND expires_at > ...`. SQLite's row-level locking guarantees single-consumer semantics.

### 9.3 Hash Algorithm

SHA-256 over file content. Documented choice: sufficient collision resistance for local workspace use; widely available; no known practical collision attacks against SHA-256.

### 9.4 Retry Model

Clients receiving a `HASH_CONFLICT` error must re-open the session (getting fresh hashes) and retry the commit with updated change entries. No automatic retry is performed by the daemon.

### 9.5 Anti-Patterns

- **DO NOT** use the workspace session OCC layer for intra-process concurrency — that is the file lock's responsibility (§2).
- **DO NOT** mix workspace session hashes with `kb_extract_jobs` version-based OCC (§7) — they are independent concurrency domains.
