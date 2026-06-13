# Novel Quality Loop — Normative Specification v1

**Status**: Shipped (V1.44 — 2026-06-13; V1.39 baseline retained; V1.45 overlay 2026-06-14)  
**Document class**: Feature line (quality-loop supplement)  
**Created**: 2026-06-09  
**Last updated**: 2026-06-14 (V1.45 P3 — §3.4/§6 updated to preset-id commands per compass §2 migration appendix)  
**Scope**: Local-first quality loop for `work_profile: novel` — findings, review routing, rules, logs, 96h escalation, on-demand audit cross-refs  
**Coordinates with**:

- [novel-workflow-profile.md](novel-workflow-profile.md) — §5.5 roadmap promoted to implement contract here for V1.39
- [creator-workflow.md](creator-workflow.md) — FL-E `review` stage and auto-chain
- [orchestration-engine.md](orchestration-engine.md) — presets, daemon scheduled tasks
- [cli-spec.md](cli-spec.md) — status/banner surfaces
- [novel-manuscript-audit.md](novel-manuscript-audit.md) — DF-69 on-demand audit (V1.44 P0)
- [novel-author-experience.md](novel-author-experience.md) — quickstart §5 cross-refs (V1.43 shipped)

**Iteration compass**: [v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md](../../iterations/v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md) · [v1.44-novel-quality-and-serial-hardening-delivery-compass-v1.md](../../iterations/v1.44-novel-quality-and-serial-hardening-delivery-compass-v1.md) (P1 overlay)

---

## 1. Purpose

V1.36 shipped inline `llm_judge` 五问 on finalize. V1.39 implements a durable quality-loop backplane: findings lifecycle, auxiliary review presets, three-layer rules, process logs, and 96h master-decision escalation — all local DB + daemon + CLI, no Redis or platform workers.

---

## 2. Findings lifecycle

### 2.1 Schema (normative minimum)

| Column | Type | Notes |
| --- | --- | --- |
| `finding_id` | TEXT PK | ULID |
| `work_id` | TEXT FK | |
| `chapter` | INTEGER NULL | Optional chapter binding |
| `kind` | TEXT | `continuity`, `craft`, `plot_hole`, `world_inconsistency`, … |
| `severity` | TEXT | `info`, `minor`, `major`, `blocker` |
| `status` | TEXT | `open`, `resolved`, `wont_fix` |
| `target_executor` | TEXT | `write`, `brainstorm`, `none`, `master` |
| `body` | TEXT | Human-readable finding |
| `created_at` / `updated_at` | INTEGER | Unix epoch |

Indexes: `(work_id, status)`, `(work_id, chapter, status)`.

### 2.2 Executor routing

| `target_executor` | Preset / action |
| --- | --- |
| `write` | Re-run or continue `novel-writing` (`produce`) |
| `brainstorm` | `novel-brainstorm` |
| `none` | User resolves manually |
| `master` | `novel-review-master` |

Auto-chain must not fork driver when routing spawns auxiliary schedules; at most one active FL-E driver per Work remains invariant.

---

## 3. Presets

| Preset ID | Role |
| --- | --- |
| `reflection-loop` | Default FL-E `review` stage (shipped) |
| `novel-brainstorm` | Ideation from open findings (V1.39 P2) |
| `novel-review-master` | Master decision surface (V1.39 P2) |
| `novel-manuscript-audit` | On-demand chapter audit — review and/or extract (V1.44 P0; see [novel-manuscript-audit.md](novel-manuscript-audit.md)) |

### 3.4 Review-master CLI surface (V1.45 P0–P2 — generic preset dispatch)

V1.44 shipped a dedicated `review-master` subcommand. V1.45 replaces it with the generic `creator run <preset_id>` entry — `creator run novel-review-master` is the preset-id form. Findings listing moves to `creator works status` (P4 enhancement).

**Normative CLI** (Shipped V1.45):

```bash
nexus42 creator run novel-review-master [<work_id>] [--finding-id <id>] [--auto-schedule]
```

| Behavior | Requirement |
| --- | --- |
| `--finding-id` | Runs or enqueues `novel-review-master` preset scoped to one finding |
| `--auto-schedule` | Opt-in: enqueue `novel-review-master` when 96h stale findings exist (mirrors DF-67 Work setting) |
| Driver interaction | Must not fork or cancel active FL-E auto-chain driver |

**Presentation** (minimum):

- Use `creator works status` to list open findings with severity breakdown
- Quickstart §5 updated to cite `creator run novel-review-master` as primary path (V1.45 P3)
- On empty findings: `creator works status [<work_id>]` surfaces a clear "no findings yet" message and suggests `creator run novel-review-master` to enqueue a master-decision review

**Residual**: R-V143P0-002 — resolved V1.44 P1; close in P-last hygiene.

---

## 4. Rules architecture (DF-65)

See [novel-workflow-profile.md §5.5.4](novel-workflow-profile.md#554-three-layer-rules-architecture) — V1.39 implements all three layers.

---

## 5. Logs structure (DF-66)

See [novel-workflow-profile.md §5.5.5](novel-workflow-profile.md#555-logs-structure-and-write-discipline) — V1.39 creates subdirs and write discipline.

---

## 6. Master-decision timeout (DF-67)

1. Daemon task every 24h queries open findings with `created_at < now - 96h`.
2. `creator works status` banner lists stale count + `novel-review-master` hint.
3. Automatic `novel-review-master` schedule: **opt-in only** (Work setting or CLI flag).

---

## 7. Acceptance (spec-level)

1. Findings CRUD isolated per creator.
2. Review stage in auto-chain can create findings without canceling driver.
3. Rules and Logs paths match novel-workflow-profile layout.
4. No Redis, external cron, or platform dependency.

---

*Draft overlay for V1.39. Merge into novel-workflow-profile §5.5 at iteration P5 hygiene if section stabilizes.*

---

## V1.45 Draft overlay — preset-id commands (applied P3 2026-06-14)

1. **§3.4** command is now `creator run novel-review-master [<work_id>] [--finding-id <id>] [--auto-schedule]` — applied to body in P3.
2. **Enqueue only** — findings list moves to `creator works status` enhancement (P4).
3. **§6** remediation hints use `creator run novel-review-master`, not `creator run review-master` — applied P3.
4. On-demand audit: `creator run novel-manuscript-audit-review|extract` per [novel-manuscript-audit.md](novel-manuscript-audit.md) V1.45 overlay.

