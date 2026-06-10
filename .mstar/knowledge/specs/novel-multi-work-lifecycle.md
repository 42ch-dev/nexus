# Novel Multi-Work Lifecycle — Normative Specification v1

**Status**: Draft (V1.41 — grill-me locked)  
**Document class**: Feature line (profile overlay extension)  
**Created**: 2026-06-10  
**Scope**: Creator-scoped **multi-novel Work completion**, **runtime/concurrency locks**, and **CLI default Work** — DF-60 Medium ceremony  
**Coordinates with**:

- [novel-workflow-profile.md](novel-workflow-profile.md) — §6 completion criteria
- [novel-work-pool.md](novel-work-pool.md) — default Work pointer (`novel_pool_entries.status = active`)
- [creator-workflow.md](creator-workflow.md) — auto-chain pause during completion-lock; runtime lock on mutating paths
- [cli-spec.md](cli-spec.md) — `creator works`, `creator run start --from-work`, `resume --reopen`
- [work-experience-model.md](work-experience-model.md) — Work is single-Work; pool `active` is CLI default only
- [agent-nexus-tool-bridge.md](agent-nexus-tool-bridge.md) — `nexus.work.patch` obeys same locks

**Iteration compass**: [v1.41-multi-work-author-desk-delivery-compass-v1.md](../../iterations/v1.41-multi-work-author-desk-delivery-compass-v1.md)

---

## 1. Purpose

A prolific author may run **multiple** `work_profile: novel` Works over time — including **concurrent** auto-chain on different Works. V1.39–V1.40 automate chapter production per Work. This spec defines how OSS **completes** a Work, **locks** it from further mutation, and how CLI picks a **default** Work — without Redis, InStreet APIs, novels-system 8-step ceremony, or a global “switch” mutex.

**Pre-1.0 policy**: Local-only; `metadata.platform_integration = paused`.

### 1.1 Concurrency model (grill-me locked)

| Concept | Rule |
| --- | --- |
| Multiple Works in flight | **Allowed** — different `work_id` values may each have active schedules / auto-chain |
| Pool `active` row | **CLI default only** — `creator run` subcommands omitting `work_id` resolve to pool `active`; does **not** block other Works |
| Same Work, two processes | **Forbidden** for mutating operations — enforced by `works.runtime_lock_holder` (§4) |
| Completed Work | **Write-protected while completion-lock present** (§3); may **reopen** same `work_id` after release (§3.4) |

**OUT**: novels-system “switch” semantics (pause all other novels, Redis `novel:active` mutex, 2h cron).

---

## 2. Completion ceremony (2-step)

When [novel-workflow-profile.md](novel-workflow-profile.md) §6 completion criteria are met:

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
| Presence | Daemon **must not** start new auto-chain ticks on this Work while lock exists |
| Mutating CLI/API | `continue`, `stage advance`, `resume` (without prior release), `nexus.work.patch`, schedule enqueue **fail closed** |
| Read-only | `creator works status`, `creator works list` always allowed |

### 3.1 Release

`creator works completion-lock release <work_id>`:

1. Removes file + clears `works.completion_locked_at`.
2. Enables **`creator run resume`** on the **same** `work_id` (grill-me B).
3. Does **not** auto-change pool row status (remains `completed` until operator `works use` / promote).

### 3.2 Reopen (resume on completed Work)

When `works.status == completed` after lock release:

```text
creator run resume <work_id> --reopen --reason "<text>"
```

1. `--reason` **required** (audited).
2. Sets `works.status = active`, clears `novel_completion_status`.
3. Re-enables auto-chain per V1.39 rules (does not delete `work_chapters` rows).
4. If §6 completion criteria still hold (all chapters `finalized` and `current_chapter >= total`), **`--extend-chapters <new_total>`** is **required** where `new_total > total_planned_chapters`; seeds new `work_chapters` rows before resume enqueue.
5. Distinct from `creator run start --from-work` (new Work + `lineage_from_work_id`).

**OUT**: silent reopen without `--reopen`.

---

## 4. Per-work runtime lock (mutating operations)

| Column | Type | Notes |
| --- | --- | --- |
| `runtime_lock_holder` | `TEXT` nullable | See holder formats below |
| `runtime_lock_acquired_at` | `TEXT` nullable | ISO-8601 |

### 4.1 Holder formats (grill-me A)

| Holder | When | Release |
| --- | --- | --- |
| `cli:<pid>:<uuid>` | Synchronous CLI mutating command | On command return (RAII / defer) |
| `daemon:schedule:<schedule_id>` | Active FL-E driver schedule | On schedule **terminal** transition |

Rules:

- Acquire before enqueue/patch that changes Work state, chapters, or driver.
- Auto-chain tick: if foreign `runtime_lock_holder` present, skip enqueue for that Work.
- Second acquirer: fail closed with holder + `creator works status` hint.
- **Independent** of completion-lock (both may apply).

Daemon Local API and `nexus.work.patch` **must** use the same acquire/release paths as CLI.

---

## 5. CLI surfaces (summary)

Full flags in [cli-spec.md](cli-spec.md) §6.2D / §6.2H.

### 5.1 `creator run` — single-Work actions

- Subcommands: `start`, `continue`, `stage`, `resume`, `reconcile-chapters`.
- Optional `work_id`: omit → pool `active` → bound `work_id`; if none, fail → `creator works use`.
- **No** `list` / `status` on `run` (hard-removed V1.41).

### 5.2 `creator run start --from-work <completed_work_id>`

- Creates **new** Work; sets `works.lineage_from_work_id` on the new row.
- Copies optional `creative_brief` defaults from completed Work metadata (not filesystem tree).
- Does **not** mutate or resume the completed Work.

### 5.3 `creator works use <work_id>`

1. Find `novel_pool_entries` by `(creator_id, work_id)`; if missing, **insert** `queued` row (title from Work record).
2. Demote prior `active` row → `queued` (one-active invariant).
3. Set target row → `active`.
4. Does **not** pause other Works’ auto-chain.

### 5.4 Agent tools

`nexus.work.patch`: append inspiration only; blocked under completion-lock; must acquire runtime lock like CLI `continue`.

---

## 6. Daemon behavior

| Condition | Auto-chain |
| --- | --- |
| `.completion-lock.json` present | `Blocked(CompletionLock)` |
| `runtime_lock_holder` foreign | Skip competing enqueue |
| `status == completed` | No chain until `resume --reopen` after lock release |
| Otherwise | V1.39 rules unchanged |

---

## 7. Acceptance (spec-level)

1. Completion §6 + completion-lock + runtime lock testable without publish.
2. `creator works status` shows `completion_lock`, `runtime_lock_holder`, pool binding.
3. Release + `resume --reopen` path documented and tested.
4. No global switch mutex in CLI or specs.

---

*Draft for V1.41 P0 implement. Promote to Shipped (V1.41) at iteration close.*
