# Runtime Concurrency — Version History Historical Notes

**Provenance**: frozen from [`specs/active/runtime/concurrency.md`](../../active/runtime/concurrency.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-09-30 — v1.201 P2 (`c61c88dfd`, 2026-09-30) is the last iteration named in the removed text; the frozen lines span the V1.42 P0 lock → V1.50 cron staggering → V1.51 advisory lock/OCC → V1.56 workspace sessions → V1.188/V1.190 core-service extraction version ledger. Every normative rule the lines annotated is retained present-tense in the live spec.
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every line removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order (25 lines), anchored to the line number it occupied in the pre-freeze document. Still-true statements were reworded in place in the live spec; their superseded wording appears here.

### at pre-freeze line ~3

```markdown
**Status**: Normative — V1.51 advisory lock/heartbeat/OCC; V1.56 workspace sessions; V1.188 recoverable target-content commit (§9); V1.201 P2 §6.2 successful-acquire detection restated as an implementable decision table (semantics unchanged)
```

### at pre-freeze line ~10

```markdown
- `cron-staggering.md` — §4 daemon cron evaluator (archived; folded into workflow-profile.md §11)
- [novel-writing/workflow-profile.md](../../active/novel-writing/workflow-profile.md) — completion locking
```

### at pre-freeze line ~19

```markdown
V1.42 P0 shipped the DB-level `works.runtime_lock_holder` column + `RuntimeLockGuard` RAII in `nexus_local_db::runtime_lock` — a single-process concurrency guard using SQLite's transactional semantics. V1.49 chose a single-writer daemon model with atomic temp+rename as the safe default (single `nexus42 daemon start` instance — the daemon host was retired in v1.193 P2; the single-writer service model carries over to `apps/nexus-service` over `nexus-core`).
```

### at pre-freeze line ~21

```markdown
V1.50 introduced cron staggering (three-role per-Work automated scheduling), which creates **real multi-writer contention**:
```

### at pre-freeze line ~23

```markdown
- The cron evaluator (`nexus-orchestration` cron supervisor; V1.50-era host was the daemon, retired in v1.193 P2) fires `novel-brainstorm` / `novel-write` / `novel-review-master` on a 1-minute tick.
```

### at pre-freeze line ~26

```markdown
The DB-level lock (`runtime_lock_holder`) is process-local — it cannot protect against two independent processes (service/scheduler + CLI) racing on `works.schedule_json`. The V1.50 P0 CAS fix (`set_schedule_json_tx`) added transactional compare-and-swap on `schedule_json`, but the broader class of cross-process races remains:
```

### at pre-freeze line ~34

```markdown
A **hard cross-process advisory lock** (`flock` on `Works/<work_ref>/.lock`) is the V1.51 fix.
```

### at pre-freeze line ~36

```markdown
V1.190 P1 moves the reading/annotations, findings, work-scope KB index and
reference-source read authorities into the guarded `nexus-core` service (one
engine-owner `CoreService` per daemon; CLI composition follows in P6). The
concurrency rules in this document are unchanged by that extraction: the core
```

### at pre-freeze line ~107

```markdown
The file-based lock is a **companion** to the existing DB-level `runtime_lock_holder` (V1.42 P0). Both must be acquired for mutating paths:
```

### at pre-freeze line ~131

```markdown
The cron evaluator (`cron_fires_at_minute`, `has_active_role_schedule`) is **read-only** on `works.schedule_json` and `creator_schedules`. It does **not** need the file lock. This is documented here for clarity — the V1.50 CAS fix ensures the evaluator never mutates `schedule_json`.
```

### at pre-freeze line ~218

```markdown
Binding clarifications for implementers and reviewers (V1.201 P2, register `core-and-spoke` / R-DOCS-001):
```

### at pre-freeze line ~242

```markdown
## 7. Per-Row Optimistic Concurrency Control (OCC) — V1.51 T-B P1
```

### at pre-freeze line ~256

```markdown
| Table | Version column | Added | Rationale |
```

### at pre-freeze line ~258

```markdown
| `kb_extract_jobs` | `version` | V1.51 T-B P1 migration `202606190001` | Promotion status transitions (`mark_confirmed`, `upsert_pending_candidate`), LLM payload refresh (`insert_pending_with_llm`), and cron-side extract job mutation are multi-actor paths where a stale preimage can produce duplicate extracts or lost confirmations. |
| `novel_pool_entries` | `version` | V1.51 T-B P1 migration `202606190001` | Pool promote/demote is an `INSERT ... ON CONFLICT DO UPDATE` that can race with concurrent `archive`/`completed` transitions from the FL-E completion hook. |
```

### at pre-freeze line ~261

```markdown
`works.schedule_json` remains under its own column-value CAS (V1.50 P0 `set_schedule_json_tx`) because the compare-and-swap target is the JSON content itself, not a monotonic version counter.
```

### at pre-freeze line ~268

```markdown
- **`with_cas_retry(max_attempts, backoff_ms, name, f) -> Result<T, LocalDbError>`** — retries the closure `f` up to `max_attempts` times (default 3, 100 ms) when a `VersionMismatch` is caught. Logs `warn!` on each retry. Any other error is returned immediately without retrying.
```

### at pre-freeze line ~286

```markdown
`upsert_pending_candidate` (V1.50 T-B P2) refreshes `proposed_payload` for an existing `pending` row. T-A P1 (cross-chapter rescan) and T-A P2 (missing-KB detection) **must** pass the version from their preimage read through this path to close the TOCTOU window.
```

### at pre-freeze line ~290

```markdown
The daemon cron-fire enqueue path (`cron_supervisor::try_fire_role`) wraps `enqueue_cron_schedule` in a retry loop (3 attempts, 100 ms backoff). When a `VersionMismatch` propagates from a versioned-table mutation inside the fire scope, the loop re-reads the preimage and retries.
```

### at pre-freeze line ~308

```markdown
- **DO NOT** add a `version` column to tables outside the V1.51 scope (`kb_extract_jobs` + `novel_pool_entries`).
```

### at pre-freeze line ~351

```markdown
## 9. Workspace Session OCC (V1.56; V1.188 recoverable content commit)
```
