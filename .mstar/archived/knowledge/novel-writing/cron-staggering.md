# Novel Cron Staggering — Superseded Overlay (V1.50)

**Status**: Superseded by [workflow-profile.md §11](../../specs/novel-writing/workflow-profile.md#11-cron-staggering-and-auto-chronology-v150) (V1.50 P-last)
**Document class**: Draft overlay (Wave-0 of V1.50; archived)
**Created**: 2026-06-18
**Last updated**: 2026-06-18
**Supersession note**: Folded into `workflow-profile.md` §11 (V1.50 P-last on 2026-06-18). Preserved here for historical reference.
**Scope**: Per-Work cron configuration for `novel-brainstorm` / `novel-write` / `novel-review-master` schedules; three-role staggering defaults; CLI surface for set/show/list.
**Coordinates with**:

- [workflow-profile.md](workflow-profile.md) §6 (merge target at P-last)
- [auto-chronology.md](auto-chronology.md) — same per-Work opt-in surface, distinct cron vs advance trigger
- [creator-schedule-and-core-context.md](../creator-schedule-and-core-context.md) — Schedule wire key contract (cron schedule fields)
- [quality-loop.md](quality-loop.md) §3 (review cron interplay)

**Iteration compass**: [v1.50-novel-author-production-loop-and-world-kb-closure-delivery-compass-v1.md](../../iterations/v1.50-novel-author-production-loop-and-world-kb-closure-delivery-compass-v1.md)

---

## 1. Purpose

V1.39 shipped the auto-chain engine (`intake → research → produce → review → persist`) but the **cadence** is single-role sequential. Authors cannot pre-set when each role fires. The novels-system reference ([deferred-features-cross-version-tracker.md §3.6.1](../../deferred-features-cross-version-tracker.md)) describes three-role staggering that lets authors set the work on autopilot.

V1.50 introduces **per-Work cron configuration** so authors can let the daemon fire each role on their preferred schedule while keeping the auto-chain intact.

---

## 2. Per-Work cron config (normative V1.50)

### 2.1 Storage

Per-Work cron config is stored in **`works.schedule_json` (TEXT)** in `state.db` (added by T-A P0 migration). The JSON shape:

```json
{
  "tz": "Asia/Shanghai",
  "roles": {
    "brainstorm": { "cron": "0 3,9,15,21 * * *", "enabled": true },
    "write":      { "cron": "0 4,10,16,22 * * *", "enabled": true },
    "review":     { "cron": "0,30 * * * *",      "enabled": true }
  }
}
```

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `tz` | IANA timezone string | `UTC` | Author TZ; daemon converts to UTC for cron firing |
| `roles.<role>.cron` | 5-field cron expression | role default (see §2.2) | Standard cron syntax |
| `roles.<role>.enabled` | bool | `true` | Per-role opt-out without removing the schedule |

**Roles** (canonical names, lowercase):

| Role | Trigger preset | Default cron (author local TZ) |
| --- | --- | --- |
| `brainstorm` | `novel-brainstorm` | `0 3,9,15,21 * * *` (4× daily at 03/09/15/21) |
| `write` | `novel-write` | `0 4,10,16,22 * * *` (4× daily at 04/10/16/22; offset +1h from brainstorm) |
| `review` | `novel-review-master` | `0,30 * * * *` (every 30 min on the hour and half-hour) |

These defaults match the novels-system reference table.

### 2.2 Default table source

Defaults live in **`crates/nexus42/src/commands/creator/works/cron.rs::DEFAULT_SCHEDULE`** (or equivalent), keyed by role. When `works.schedule_json` is empty or absent, the daemon uses defaults from this table.

### 2.3 Migration behavior

On T-A P0 migration, existing Works get an empty `schedule_json` (= use defaults). No retroactive override.

---

## 3. CLI surface (normative V1.50)

### 3.1 `creator works cron set <work_ref>`

Sets per-Work cron. Subcommands:

```text
creator works cron set my-work                                       # reset to defaults
creator works cron set my-work --brainstorm "0 3,9,15,21 * * *"
creator works cron set my-work --write "0 4,10,16,22 * * *"
creator works cron set my-work --review "0,30 * * * *"
creator works cron set my-work --tz Asia/Shanghai
creator works cron set my-work --no-brainstorm                       # disable role
creator works cron set my-work --no-review
```

Flags:

| Flag | Purpose |
| --- | --- |
| `--brainstorm <cron>` | Set brainstorm cron expression |
| `--write <cron>` | Set write cron expression |
| `--review <cron>` | Set review cron expression |
| `--tz <iana-tz>` | Set author TZ (default: read from env `NEXUS_TZ`, fallback `UTC`) |
| `--no-brainstorm` / `--no-write` / `--no-review` | Disable role (sets `enabled: false`) |

Validation:

- Each `--<role>` value parses via `cron` crate (already a dep; verify V1.50 T-A P0 T2).
- Timezone must be a valid IANA string (use `chrono-tz::Tz::from_str`).
- At least one role must remain `enabled: true` unless `--all-off` is passed (CLI rejects empty schedules).

### 3.2 `creator works cron show <work_ref>`

Displays the resolved schedule with both author local time and UTC firing times:

```text
Work: my-work
TZ:   Asia/Shanghai (UTC+08:00)

Role        Cron                Local time           Next fire (UTC)
brainstorm  0 3,9,15,21 * * *   03:00 / 09:00 / ...   2026-06-19 19:00 UTC
write       0 4,10,16,22 * * *   04:00 / 10:00 / ...   2026-06-19 20:00 UTC
review      0,30 * * * *         :00 / :30 every hour  2026-06-19 14:00 UTC
```

If any role is `enabled: false`, show `disabled` in place of the cron.

### 3.3 `creator works cron list`

Lists cron config across all Works in the active workspace:

```text
WORK_REF       TZ                BRAINSTORM       WRITE            REVIEW
my-work        Asia/Shanghai     0 3,9,15,21      0 4,10,16,22     0,30 * * * *
other-work     UTC               (defaults)       (defaults)       (defaults)
```

Defaults are shown as the canonical cron expression without rewriting to the actual scheduled time.

---

## 4. Daemon firing semantics

### 4.1 Tick evaluator

On each daemon tick (already 1-min interval per V1.39), the schedule supervisor:

1. Reads all Works with `schedule_json` non-empty OR `auto_chronology=true` (T-A P3 coupling).
2. For each role enabled, evaluates whether the cron fires in the next tick (using `cron` crate's `iter_after(now)` and matching against current minute).
3. If fire: enqueue a `Schedule` with `preset_id = <role-preset>` and `work_ref` from the source Work.

### 4.2 Idempotency / re-entrance

If a cron fire enqueues a `Schedule` while a previous schedule of the same role for the same Work is still active, the new fire **skips** (logged as `INFO`). Authors must resolve the prior schedule (or wait for it to complete) before the next fire lands.

### 4.3 Per-Work gating

Cron fires for a Work are **gated** by:

- `works.intake_status == "complete"` (skip if intake not done)
- `works.runtime_lock_holder == NULL` (skip if locked)
- `works.completion_locked == false` (skip if completed)

Skipped fires log at `DEBUG` (visible with `RUST_LOG=nexus_orchestration=debug`).

### 4.4 Auto-promotion interplay (T-B P1)

When `novel-review-master` cron fires, the **review-time KB extraction** (T-B P1) runs as part of the same schedule execution. Extraction populates `kb_extract_jobs` with `status='pending'` rows for new candidate KB entities. The cron schedule itself does not gate auto-promotion; author confirm is via `creator world kb adopt`.

---

## 5. Authoring rules

- Cron expressions are **per-Work**, not global. Global default table is the source for unset Works.
- TZ is **per-Work**, not per-role. Authors who want different TZs per role must edit `Works/<work_ref>/.schedule.json` directly (out of CLI scope).
- Cron firing respects **daemon uptime**. If the daemon is offline during a fire window, the fire is **skipped**, not queued.
- Cron firing does **not** bypass `creator run`; authors can still invoke `novel-write` manually between cron fires.

---

## 6. Acceptance criteria (T-A P0–P2)

1. T-A P0: `works.schedule_json` column + DAO; `creator works cron set/show/list` CLI; default table in code.
2. T-A P1: `novel-brainstorm` + `novel-write` cron wiring into auto-chain; per-Work gating; hermetic tests for fire/skipped/locked.
3. T-A P2: `novel-review-master` cron firing; review-time gating respects `intake==complete` + `runtime_lock==NULL`; quality loop interplay (review result feeds `open_findings_block`).

---

## 7. P-last merge

At V1.50 P-last, fold this overlay into [workflow-profile.md](workflow-profile.md) §6 (new section: "Cron staggering — three-role scheduling"). Update §6 with the normative defaults, CLI surface, daemon firing semantics. Archive this overlay with `Superseded by:` stub.