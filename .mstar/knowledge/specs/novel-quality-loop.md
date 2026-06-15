# Novel Quality Loop — Normative Specification v1

**Status**: Draft (V1.47) — overlay on Shipped (V1.44) baseline; promote at P-last  
**Document class**: Feature line (quality-loop supplement)  
**Created**: 2026-06-09  
**Last updated**: 2026-06-15 (V1.47 P-1 harness — §8 reflection-loop output contract; §2.1 `rule_suggestion` metadata)  
**Scope**: Local-first quality loop for `work_profile: novel` — findings, review routing, rules, logs, 96h escalation, on-demand audit cross-refs  
**Coordinates with**:

- [novel-workflow-profile.md](novel-workflow-profile.md) — §5.5 roadmap promoted to implement contract here for V1.39
- [creator-workflow.md](creator-workflow.md) — FL-E `review` stage and auto-chain
- [orchestration-engine.md](orchestration-engine.md) — presets, daemon scheduled tasks
- [cli-spec.md](cli-spec.md) — status/banner surfaces
- [novel-manuscript-audit.md](novel-manuscript-audit.md) — DF-69 on-demand audit (V1.44 P0)
- [novel-author-experience.md](novel-author-experience.md) — quickstart §5 cross-refs (V1.43 shipped)

**Iteration compass**: [v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md](../../iterations/v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md) · [v1.44-novel-quality-and-serial-hardening-delivery-compass-v1.md](../../iterations/v1.44-novel-quality-and-serial-hardening-delivery-compass-v1.md) · [v1.47-novel-quality-loop-closure-delivery-compass-v1.md](../../iterations/v1.47-novel-quality-loop-closure-delivery-compass-v1.md) (active)

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
| `rule_suggestion` | TEXT NULL | **V1.47** — optional prose suggestion for Layer 2 rules; persisted on finding row only (no `AGENTS.md` write in P0) |
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
| `novel-chapter-review` | FL-E `review` stage — novel/work/chapter-aware review producer (findings writer); **V1.47 P0** shipped. Named `novel-chapter-review` (replaces the former generic `reflection-loop` demo) |
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

See [novel-workflow-profile.md §5.5.4](novel-workflow-profile.md#554-two-layer-rules-architecture-v147) — V1.47 normative: Layer 2 = `Works/<work_ref>/AGENTS.md` (runtime reader migration deferred).

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

## 8. Reflection-loop output contract (V1.47 Draft)

**Scope**: Normative behavior for the FL-E `review` preset after P0 implement. Applies to **both** auto-chain review stage and on-demand `creator run <preset_id>`.

### 8.1 Inputs (minimum)

| Input | Source | Required |
| --- | --- | --- |
| `work_id` | Schedule / CLI | Yes |
| `chapter` | `work_chapters` selection for review pass | Yes when multi-chapter |
| `body_path` / `outline_path` | Chapter artifacts | Best-effort |
| Rules context | Layer 1 embedded + Layer 2 (shipped: `Rules/novel-rules.md`; normative target: `AGENTS.md`) | Best-effort |

### 8.2 Finding creation

1. On terminal success of review preset, call existing `create_finding_from_review` (or supervisor-equivalent) **≥1** time per review pass.
2. Each finding MUST set: `work_id`, `chapter` (when known), `kind`, `severity`, `status=open`, `target_executor`, `body`.
3. Optional `rule_suggestion` text MAY be stored on the finding row; accepting a suggestion does **not** mutate `Works/<work_ref>/AGENTS.md` in V1.47.

### 8.3 Idempotency

Re-running review on the same chapter SHOULD avoid duplicate open findings with identical `body` hash within a 24h window (implementer may use content hash or finding kind+chapter dedupe — lock in P0 plan).

### 8.4 Auto-chain invariant

Finding creation MUST NOT fork or cancel the active FL-E auto-chain driver schedule.

---

*Draft overlay for V1.39. Merge into novel-workflow-profile §5.5 at iteration P5 hygiene if section stabilizes.*

---

## V1.45 supersession (P-last promotion)

**Superseded by**: [creator-run-preset-entry.md](creator-run-preset-entry.md) (Shipped Master V1.45). The `novel-review-master` preset id + enqueue-only semantics + audit preset ids are now part of the canonical Master body.

