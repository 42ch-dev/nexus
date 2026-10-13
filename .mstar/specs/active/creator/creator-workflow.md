# Creator Workflow — Normative Specification

**Status**: Shipped  
**Current authority**: Work stage/field model for workflow execution; the Creator runner and auto-chain/resume-chain entrances have no CLI entries (§0).  
**Document class**: Feature line  
**Created**: 2026-06-04  
**Last updated**: 2026-09-29  
**Scope**: Staged Work model (`intake → research → produce → review → persist`) and retained observable fields; preset chain mapping; read-only SOUL visualization  
**Coordinates with**:

- [work-experience-model.md](work-experience-model.md) — Work entity, intake, run_intents
- [novel-writing/workflow-profile.md](../novel-writing/workflow-profile.md) — novel `produce` artifacts and completion
- [cli-spec.md](../cli/cli-spec.md) — retained `creator works` atomic operations; no `creator run` / `creator bootstrap` CLI entry
- [orchestration-engine.md](../orchestration/orchestration-engine.md) — presets, schedules, capabilities
- [agent-nexus-tool-bridge.md](../agents/agent-nexus-tool-bridge.md) — Agent-initiated context/tools (parallel channel)

---

## 0. Current workflow boundary

The stage vocabulary and stored Work state remain: `current_stage`, `stage_status`, `current_chapter`, `auto_chain_enabled`, `driver_schedule_id`, and `auto_chain_interrupted` are retained in [`crates/nexus-local-db/src/works.rs`](../../../../crates/nexus-local-db/src/works.rs) (`WorkRecord`, `WORKS_COLUMNS`). Retained `creator works` commands expose Work state and atomic operations; **field persistence or visibility does not promise automatic stage dispatch, a live driver, or restart resume**.

[`apps/nexus42/src/commands/creator/works/mod.rs`](../../../../apps/nexus42/src/commands/creator/works/mod.rs) owns the retained `creator works` family; it has no `works intake` / `works resume-chain` subcommands. Its status output still reports `auto_chain_enabled`, `driver_schedule_id`, and interrupted state, but deliberately advertises no resume remediation command. The generic `creator run <preset_id>` entry does not exist, and there is no replacement CLI preset-dispatch entrance; see [creator-run-preset-entry.md](../../archived/creator/2026-09-21-creator-run-preset-entry.md) §0.

**Reading boundary:** the execution rules, command journeys, automatic full-stage/chapter progression, and daemon restart continuation are not part of the current contract; the stage/field vocabulary (§1–§4) and the §5.6 SOUL read contract are.

---

## 1. Purpose

The Work loop generalizes the creative journey to:

```text
intake → research → produce → review → persist
```

The workflow reuses the existing scheduler and World/KB SSOT.

---

## 2. Relationship to Work model

**Invariants** (from work-experience-model):

1. Work ≠ workspace ≠ World ≠ work KB index.
2. `work_continue` presets require `intake_status == complete` unless `--force` (logged).
3. At most one **active** `work_init` intake schedule per Work while intake incomplete.
4. At most one **active stage schedule** per Work at a time (no parallel `research` + `novel-writing` stage drivers).

---

## 3. Stage model

> Stage identifiers, stored fields, and stage-gate rules are part of the Work model; they do not supply a CLI dispatch or resume entrance (§0).

### 3.1 Stage identifiers (closed enum)

| `stage` | Meaning | Typical preset(s) | `run_intents` |
| --- | --- | --- | --- |
| `intake` | Creative Brief Intake | `creative-brief-intake` | `work_init` |
| `research` | Reference / KB gathering | `research` | `knowledge_ingest`, `work_continue` |
| `produce` | Primary drafting / generation | `novel-writing` (default) | `work_continue` |
| `review` | Quality loop / revision | `novel-chapter-review` | `work_continue` |
| `persist` | Memory + KB promotion | `kb-extract`, `creator memory review` CLI | `knowledge_ingest`, `work_continue` |

Stages are **linear** in product order. Skipping a stage requires explicit `--force` (logged) or PM-approved compass exception — not conditional graph edges.

### 3.2 `stage_status` (per Work)

| Value | Meaning |
| --- | --- |
| `pending` | Stage not started |
| `active` | Schedule running or waiting for user |
| `complete` | Stage finished successfully |
| `skipped` | User forced skip (audit) |
| `failed` | Terminal failure; user may retry advance |

**Storage**: columns on `works` table — `current_stage`, `stage_status` (JSON map optional for per-stage status in P1 plan).

### 3.3 Stage gates

Advance from stage `S` to `S+1` requires:

1. `intake_status == complete` before leaving `intake` (unchanged).
2. Current stage `stage_status == complete` for `S`, unless `--force`.
3. No other **active** stage schedule on the Work.

CLI:

```text
creator works status [<work_id>]    # includes current_stage + stage_status (default = pool active)
```

---

## 4. Preset chain

| Stage | Preset ID | Notes |
| --- | --- | --- |
| `intake` | `creative-brief-intake` | Composite onboarding entry preset |
| `research` | `research` | May append references to Work context |
| `produce` | `novel-writing` | Uses `creative_brief` + `inspiration_log`; novel profile writes to `Works/<work_ref>/` per [novel-writing/workflow-profile.md](../novel-writing/workflow-profile.md) |
| `review` | `novel-chapter-review` | Persists ≥1 finding per review pass via the supervisor terminal hook; see [novel-writing/quality-loop.md §8](../novel-writing/quality-loop.md#8-reflection-loop-output-contract). |
| `persist` | `kb-extract` (via queue) + CLI memory review | World-bound novel Works enqueue extract with `work.world_id`; worldless Works skip World promotion. See [novel-writing/workflow-profile.md §3.5.1.5](../novel-writing/workflow-profile.md). |

P2 may add wiring presets or seeds only; **no** new conditional `next.kind`.

---

## 5. User journeys

> The retired `creator run` journeys (explicit-stage dispatch, daemon-attached auto-chain, `creator run resume`, the `daemon schedule` power-user surface) are not current CLI journeys; the retained surfaces are the `creator works` atomic operations (§0) and the lanes below.

### 5.2 Inspiration without stage change

```text
creator works inspire --note "new angle"
```

Does **not** advance `current_stage`; appends a `{at, note}` entry to the Work's `inspiration_log`. It creates no `core_context` version and does not touch a schedule — the appended notes are read when the preset input is built at the **next** state transition. `creator works inspire` is **refused** while the Work has an active auto-chain driver (`auto_chain_enabled` + `driver_schedule_id` → `AUTO_CHAIN_DRIVER_ACTIVE` conflict), so it cannot steer a running driver.

### 5.5 Side-input lane

Side input never forks or cancels an active stage driver schedule — but the inspiration surfaces below are **refused** while one is running, not merged into it:

| Insertion | Behavior |
| --- | --- |
| `creator works inspire --note` | Refused while the Work has an active auto-chain driver (`AUTO_CHAIN_DRIVER_ACTIVE`); otherwise appends `inspiration_log` only — no `core_context` version, no schedule write; visible in `creator works status`; read when preset input is built at the **next** preset state transition |
| `nexus.work.patch` (agent) | Same `append_work_inspiration` surface, so the same active-driver refusal; blocked under completion-lock per agent-nexus-tool-bridge |
| Research stage side effects | KB/reference artifacts written during the chain; consumed by downstream produce/review via existing assembly (P0.5) |

Invariant: at most one active FL-E stage driver schedule per Work is enforced.

### 5.6 Author reflection: SOUL visualization

The read-only reflection surface over the creator's internalized SOUL fragments consumes rows already stored in the local `memory_fragments` (`n`) table and renders:

- **Keyword clusters** from each fragment's `keywords` JSON array.
- **Temporal drift** from each fragment's `created_at` timestamp, with growth count folded into the timeline.

Wire contract: `schemas/daemon-api/memory/memory-fragment-info.schema.json` extends the list-fragments item DTO with optional `keywords: string[]` and optional `created_at: string` (RFC 3339 by description). The extension is additive: `fragment_id` and `summary` remain the only required fields, and internal ownership/session fields (`creator_id`, `session_id`, `ttl`) stay out of the response. The visualization is a consumer of the creator-scoped memory list endpoint; it does not create, patch, or delete memory fragments.

---

## 6. Conflicts and non-goals


| Topic | Rule |
| --- | --- |
| Work vs `creator kb --scope work` | Index entries may tag `work_id`; index does not define Work |
| Agent tools vs presets | Agent may read/patch Work via `nexus.work.*`; production presets still run via orchestration |
| Conditional routing | **Not** used for stage selection (DF-56) |
| Novel project init | Separate preset `novel-project-init` (DF-58); **not** part of the `novel-writing` chain |
| Novel completion | Work `status == completed` stops further `novel-writing`; `mark_work_completed` per [novel-writing/multi-work-lifecycle.md](../novel-writing/multi-work-lifecycle.md) |
| Completion-lock | While `.completion-lock.json` exists, stage drivers **must not** tick that Work; `creator works reopen` releases the lock but leaves `works.status = completed` and does not re-enable auto-chain ([multi-work-lifecycle.md §3.3](../novel-writing/multi-work-lifecycle.md#33-reopen-completion-lock-release)) |
| Runtime lock | `works.runtime_lock_holder` blocks concurrent mutating CLI/API on same Work |
| Multi-Work concurrency | Multiple Works may run stage drivers concurrently; pool `active` is CLI default only (DF-60/61) |
| Selection pool | `creator works pool` + `works use` via [novel-writing/work-pool.md](../novel-writing/work-pool.md); not a Work profile (DF-61) |
| CLI IA | `creator works` carries list/status/use/pool and the Work atomic operations; single-Work `creator run` actions have no CLI entry (§0) |
| Platform cloud assemble | Not part of this workflow; see agent-nexus-tool-bridge `policy_blocked` |

---

## 7. Acceptance

1. Stage enum and preset mapping are stable in cli-spec and this document.
2. No contradiction with [work-experience-model.md](work-experience-model.md) §3–7.

---

*Normative for the Work stage/field vocabulary and the separate SOUL read contract; no CLI execution/auto-chain entrances exist (§0).*