# Novel Multi-Work Lifecycle — Normative Specification v1

**Status**: Shipped — multi-Work completion, runtime/concurrency locks, and CLI default Work  
**Document class**: Feature line (profile overlay extension)  
**Created**: 2026-06-10  
**Scope**: Creator-scoped **multi-novel Work completion**, **runtime/concurrency locks**, and **CLI default Work** — DF-60 Medium ceremony  
**Coordinates with**:

- [workflow-profile.md](workflow-profile.md) — §6 completion criteria
- [work-pool.md](work-pool.md) — default Work pointer (`novel_pool_entries.status = active`)
- [creator-workflow.md](../creator/creator-workflow.md) — auto-chain pause during completion-lock; runtime lock on mutating paths
- [cli-spec.md](../cli/cli-spec.md) — `creator works`, completion-lock release, reopen
- [work-experience-model.md](../creator/work-experience-model.md) — Work is single-Work; pool `active` is CLI default only
- [agent-nexus-tool-bridge.md](../agents/agent-nexus-tool-bridge.md) — `nexus.work.patch` obeys same locks

---

## 1. Purpose

A prolific author may run **multiple** `work_profile: novel` Works over time — including **concurrent** auto-chain on different Works. This spec defines how OSS **completes** a Work, **locks** it from further mutation, and how CLI picks a **default** Work — without Redis, InStreet APIs, novels-system 8-step ceremony, or a global “switch” mutex.

**Pre-1.0 policy**: Local-only; `metadata.platform_integration = paused`.

### 1.1 Concurrency model

| Concept | Rule |
| --- | --- |
| Multiple Works in flight | **Allowed** — different `work_id` values may each have active schedules / auto-chain |
| Pool `active` row | **CLI default only** — `creator works` subcommands omitting `work_id` resolve to pool `active`; does **not** block other Works |
| Same Work, two processes | **Forbidden** for mutating operations — enforced by `works.runtime_lock_holder` (§4) |
| Completed Work | **Write-protected while completion-lock present** (§3); `creator works reopen` releases the lock atomically (§3.3) |

**OUT**: novels-system “switch” semantics (pause all other novels, Redis `novel:active` mutex, 2h cron).

---

## 2. Completion ceremony (2-step)

When [workflow-profile.md](workflow-profile.md) §6 completion criteria are met:

1. Set `works.status = completed` (and `novel_completion_status = completed` when column present).
2. Ensure all `work_chapters` rows for the Work are `finalized` (DB SSOT).

**Does not** run platform publish (DF-59 OUT).

On success (extend `auto_chain::mark_work_completed` — **reuse** `is_work_completed`):

1. Write **completion-lock** (§3) unless `--no-completion-lock` (power-user escape; audited).
2. If a `novel_pool_entries` row binds this `work_id`, set its `status` → `completed`.
3. If that row was pool `active`, **clear** the creator’s `active` slot (no automatic promotion).

---

## 3. Completion-lock file

Path: `Works/<work_ref>/.completion-lock.json`

Minimum schema:

```json
{
  "work_id": "wrk_...",
  "locked_at": "2026-06-10T12:00:00Z",
  "reason": "completion"
}
```

| Rule | Behavior |
| --- | --- |
| Presence | The scheduler (`nexus-orchestration` supervisor, hosted via `nexus-core`) **must not** start new auto-chain ticks on this Work while lock exists |
| Mutating CLI/API | `nexus.work.patch` and schedule enqueue **fail closed** while the lock is present; `creator works reopen` is the audited command that releases it (§3.3) |
| Read-only | `creator works status`, `creator works list` always allowed |

### 3.1 Release

`creator works completion-lock release <work_id>`:

1. Removes file + clears `works.completion_locked_at`.
2. Runs the same core release as `creator works reopen` (§3.3): clears `works.completion_locked_at`, sets `novel_completion_status = reopened`, and removes the derived file. It does **not** change `works.status`.
3. Does **not** auto-change pool row status (remains `completed` until operator `works use` / promote).

### 3.2 Source-of-truth declaration

DB column `works.completion_locked_at` is the authoritative lock state. The `.completion-lock.json` file is a derived artifact for cross-tool observation. The supervisor gates ticks on the DB column. If the file exists but the DB column is NULL, the supervisor treats the work as unlocked. If the file is missing but the DB column is set, the supervisor treats the work as locked.

### 3.3 Reopen (completion-lock release)

```text
creator works reopen [<work_id>] --reason "<text>"
```

Current command path (`handle_reopen`, `apps/nexus42/src/commands/creator/works/mod.rs:1005-1043`): it runs the core's completion-lock release (`CoreService::release_work_completion_lock`, `crates/nexus-core/src/works.rs:1581-1613`). `creator works completion-lock release` is the sibling alias for the same core call (`apps/nexus42/src/commands/creator/works/mod.rs:1274-1295`).

1. `--reason` **required** (audited).
2. The Work **must** be completion-locked; the core refuses a Work whose `completion_locked_at` is NULL with `not_locked` instead of accepting a no-op patch.
3. The release clears `works.completion_locked_at` (SSOT), sets `novel_completion_status = 'reopened'`, and deletes the derived `.completion-lock.json`. It does **not** change `works.status` — the row stays `completed`; the Work is writable again because the completion-lock gate no longer blocks it, not because a column flipped to `active`.
4. Does **not** re-enable auto-chain: the release leaves `works.status = completed`, and auto-chain returns `ChainAction::NoAction` whenever `work.status == 'completed'` (`crates/nexus-orchestration/src/auto_chain.rs:691-694`). It does not delete `work_chapters` rows.
5. `--extend-chapters` has no shipped path.

**OUT**: silent reopen without `--reason`.

---

## 4. Per-work runtime lock (mutating operations)

| Column | Type | Notes |
| --- | --- | --- |
| `runtime_lock_holder` | `TEXT` nullable | See holder formats below |
| `runtime_lock_acquired_at` | `TEXT` nullable | ISO-8601 |

### 4.1 Holder formats

| Holder | When | Release |
| --- | --- | --- |
| `cli:<pid>:<uuid>` | Synchronous CLI mutating command | On command return (RAII / defer) |
| `daemon:schedule:<schedule_id>` | Active FL-E driver schedule | On schedule **terminal** transition |

Rules:

- Acquire before enqueue/patch that changes Work state, chapters, or driver.
- Auto-chain tick: if foreign `runtime_lock_holder` present, skip enqueue for that Work.
- Second acquirer: fail closed with holder + `creator works status` hint.
- **Independent** of completion-lock (both may apply).

Daemon API and `nexus.work.patch` **must** use the same acquire/release paths as CLI.

### 4.2 Production acquire contract

All mutating paths acquire/release `runtime_lock_holder` via `RuntimeLockGuard` RAII + `nexus_local_db::runtime_lock` module.

| Path | Acquire before mutate | Release |
| --- | --- | --- |
| `creator works` mutating subcommands | **Required** | On command return (RAII) |
| Service `patch_work` / schedule enqueue (`apps/nexus-service` over `nexus-core`) | **Required** | On schedule terminal |
| Auto-chain tick | Skip if foreign holder | N/A |

**Stale recovery (R-V141P0-01)**: If `runtime_lock_acquired_at` older than **2h** (default; env/config override allowed), the scheduler owner (`nexus-orchestration` supervisor under `nexus-core`) **may** clear holder before new acquire. Hermetic tests required.

---

## 5. CLI surfaces (summary)

Full flags in [cli-spec.md](../cli/cli-spec.md) §6.2H.

Commands that accept an omitted `<work_id>` resolve it through the selection-pool `active` entry's bound `work_id`; an explicit `<work_id>` always wins, and with no pool `active` entry the command refuses with remediation to `creator works use <work_id>` ([cli-spec.md](../cli/cli-spec.md) §6.2H). There is no CLI preset-dispatch entrance; schedule add goes through the core execution surface (`POST /v1/daemon/orchestration/schedules`, served by `apps/nexus-service`), with `creator works cron` declaration editing as the retained CLI scheduling surface.

### 5.3 `creator works use <work_id>`

1. Find `novel_pool_entries` by `(creator_id, work_id)`; if missing, **insert** `queued` row (title from Work record).
2. Demote prior `active` row → `queued` (one-active invariant).
3. Set target row → `active`.
4. Does **not** pause other Works’ auto-chain.

### 5.4 Agent tools

`nexus.work.patch`: append inspiration only; blocked under completion-lock; must acquire runtime lock like CLI `inspire`.

---

## 6. Auto-chain gating

| Condition | Auto-chain |
| --- | --- |
| `.completion-lock.json` present | `Blocked(CompletionLock)` |
| `runtime_lock_holder` foreign | Skip competing enqueue |
| `status == completed` | No auto-chain (`ChainAction::NoAction`, `crates/nexus-orchestration/src/auto_chain.rs:691-694`); `creator works reopen` clears the completion-lock but leaves `status = completed` (§3.3) |
| Otherwise | Auto-chain rules per [workflow-profile.md](workflow-profile.md) |

---

## 7. Acceptance (spec-level)

1. Completion §6 + completion-lock + runtime lock testable without publish.
2. `creator works status` shows `completion_lock`, `runtime_lock_holder`, pool binding.
3. Release + `reopen` path documented and tested.
4. No global switch mutex in CLI or specs.
