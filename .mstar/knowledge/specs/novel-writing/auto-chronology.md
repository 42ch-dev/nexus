# Novel Auto-Chronology — Draft Overlay (V1.50)

**Status**: Draft (V1.50)
**Document class**: Draft overlay (Wave-0 of V1.50)
**Created**: 2026-06-18
**Last updated**: 2026-06-18
**Supersession note**: To be folded into [workflow-profile.md](workflow-profile.md) §6.5 (auto-chronology subsection) at V1.50 P-last.
**Scope**: Per-Work opt-in volume auto-advance on finish; daemon task that creates next volume outline + seed when current volume all chapters finalized and intake complete.
**Coordinates with**:

- [cron-staggering.md](cron-staggering.md) — same per-Work config surface, distinct cron vs advance trigger
- [multi-work-lifecycle.md](multi-work-lifecycle.md) — completion lock + Work lifecycle
- [workflow-profile.md §4.3](workflow-profile.md) — chapter frontmatter `volume` field (V1.42 shipped)

**Iteration compass**: [v1.50-novel-author-production-loop-and-world-kb-closure-delivery-compass-v1.md](../../iterations/v1.50-novel-author-production-loop-and-world-kb-closure-delivery-compass-v1.md)

---

## 1. Purpose

DF-62 (multi-volume PK + seed) shipped in V1.42 but **manual volume advancement** is required. Authors must:

1. Manually edit `Works/<work_ref>/Outlines/volume-N+1-outline.md`.
2. Run `creator works seed-volume my-work --volume <n+1> --from-outline ...` or equivalent.

V1.50 introduces **per-Work opt-in auto-chronology**: when the current volume is complete (all chapters finalized + intake complete), the daemon **automatically** creates `Outlines/volume-N+1-outline.md` from a template, seeds the chapter table for volume N+1, and logs the advance in `Logs/chronology/`.

This is the **novels-system reference** behavior (DF-62 row in [deferred-features-cross-version-tracker.md §3.6.1](../../deferred-features-cross-version-tracker.md)).

---

## 2. Per-Work opt-in flag (normative V1.50)

### 2.1 Storage

`works.auto_chronology` (BOOLEAN, default `false`) added by T-A P3 migration.

### 2.2 CLI surface

```text
creator works chronology set my-work --auto true       # enable
creator works chronology set my-work --auto false      # disable (default)
creator works chronology show my-work                 # display state + last advance
creator works chronology advance my-work --volume 2    # manual override (T-A P3 also)
```

`creator works chronology advance` is the **manual** path; it bypasses the finish-detection gate and creates the next volume immediately. Manual advance is **always available** regardless of `auto_chronology` flag.

---

## 3. Finish detection (normative V1.50)

Daemon `auto_chronology_tick` runs every 5 minutes (configurable via env `NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN`; default 5). For each Work with `auto_chronology=true`:

1. Read `state.db` for the Work's `current_volume` (or `max(volume)` over chapters).
2. List chapters of that volume where `status != 'finalized'`. If any → not eligible, skip.
3. Read `intake_status` for the Work. If `!= 'complete'` → not eligible, skip.
4. Read `completion_locked` for the Work. If `true` → not eligible (Work is fully complete), skip.
5. **Eligible**: trigger advance (§4).

Skipped fires log at `DEBUG`.

### 3.1 Edge cases

| Edge | Behavior |
| --- | --- |
| Volume N is the last planned volume (no further outline) | Skip with `INFO` log: "auto_chronology: no further volume planned" |
| Volume N+1 outline already exists | Skip with `INFO` log: "auto_chronology: outline already exists" (idempotent guard) |
| `runtime_lock_holder != NULL` | Skip with `DEBUG` log: "auto_chronology: work is locked" |
| Daemon interrupted mid-advance | Atomic `state.db` tx wraps the entire advance; on crash, rolled back. Next tick retries cleanly. |

---

## 4. Advance execution (normative V1.50)

When eligible:

### 4.1 Outline creation

1. Compute `next_volume = current_volume + 1`.
2. Read template at `embedded-presets/novel-writing/templates/volume-outline.md.tmpl`.
3. Substitute author info from `Works/<work_ref>/README.md` frontmatter (title, total_planned_chapters, etc.).
4. Write `Works/<work_ref>/Outlines/volume-<next_volume>-outline.md` with template + placeholder sections.
5. Atomic write: temp file + `fsync` + rename (existing V1.36 atomicity pattern).

### 4.2 Chapter seed

1. Open `state.db` transaction.
2. Insert one `work_chapters` row per planned chapter with `(work_id, volume=next_volume, chapter=N, status='not_started', word_count=0, world_refs=[])`.
3. Commit.

If the outline specifies fewer planned chapters than the global `total_planned_chapters`, seed up to that count. If outline is empty (placeholder), seed zero chapters (author must fill outline + `creator works seed-volume` to add chapters manually).

### 4.3 Log entry

Append to `Works/<work_ref>/Logs/chronology/<YYYY-MM-DD>-advance-vol<N+1>.md`:

```text
# Auto-Chronology Advance — Volume <N+1>

- At: <UTC timestamp>
- Trigger: daemon auto_chronology_tick (finish detected)
- Previous volume: <N> (all finalized, intake complete)
- New volume: <N+1>
- Outline: Works/<work_ref>/Outlines/volume-<N+1>-outline.md (template-rendered)
- Chapters seeded: <count>
```

---

## 5. Interaction with cron staggering

- Cron **does not gate** auto-chronology. Auto-advance can fire while cron schedules are still queued for the new volume.
- Once volume N+1 is seeded, cron fires for `write` will operate on the new volume's chapters (per `state.db` chapter list).
- Authors can disable cron for a specific volume via `creator works cron set --no-write` if they prefer manual chapter production on the new volume.

---

## 6. Acceptance criteria (T-A P3)

1. `works.auto_chronology` column + DAO + `creator works chronology set/show/advance` CLI.
2. Daemon `auto_chronology_tick` task; hermetic test for finish detection (positive + 4 negative edge cases).
3. Atomic outline creation + chapter seed + log entry; idempotent retry after crash.
4. CLI `creator works chronology advance --volume N+1` manual override; works regardless of `auto_chronology` flag.
5. Documentation pointer: `embedded-presets/novel-writing/templates/volume-outline.md.tmpl` exists and renders correctly.

---

## 7. P-last merge

At V1.50 P-last, fold this overlay into [workflow-profile.md §6.5](workflow-profile.md) (new subsection "Auto-chronology — multi-volume advancement"). Update §6.5 with the normative opt-in flag, finish detection rules, advance execution steps. Archive this overlay with `Superseded by:` stub.