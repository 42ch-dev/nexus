# Novel Manuscript Audit — Normative Specification v1

**Status**: Shipped  
**Document class**: Feature line (DF-69 audit supplement)  
**Created**: 2026-06-13  
**Last updated**: 2026-10-13  
**Scope**: On-demand chapter audit for `work_profile: novel` — structured review report and/or World KB extract **without** entering the full FL-E auto-chain driver  
**Coordinates with**:

- [quality-loop.md](quality-loop.md) — findings lifecycle; review routing
- [workflow-profile.md](workflow-profile.md) — chapter paths, `Logs/review/`, 五问 baseline
- [creator-workflow.md](../creator/creator-workflow.md) — FL-E stages (audit is **out-of-band**)
- [cli-spec.md](../cli/cli-spec.md) — CLI surface authority (no audit CLI entry; see §4)
- [entity-scope-model.md](../architecture/entity-scope-model.md) — World-bound extract mode
- [world-kb-runtime-architecture.md](../architecture/world-kb-runtime-architecture.md) — `kb.extract_work` on-demand path

**Tracker**: DF-69

---

## 1. Purpose

The FL-E chain covers inline finalize `llm_judge`, `novel-chapter-review`, `novel-review-master` on **existing findings**, and queue-based `kb-extract`. This spec adds a **single on-demand entry** to audit an already-written chapter body (`Works/<work_ref>/Stories/ch*.md`) outside the auto-chain driver: **DF-69**, a dual-mode embedded preset (or preset pair) plus an audit entry.

The manuscript reading surface is distinct from this on-demand audit line: reading is a read-only consumption surface for authors, while manuscript audit remains an out-of-band quality/extract tool that may create reports, findings, or KnowledgeEntries.

---

## 2. Distinction from shipped presets

| Surface | Role | Enters FL-E driver? |
| --- | --- | --- |
| `novel-writing` finalize `llm_judge` | Inline quality gate on finalize transition | Yes (produce stage) |
| `novel-chapter-review` | Default FL-E `review` stage | Yes |
| `novel-review-master` | Master decisions on **existing** findings | Auxiliary schedule only |
| `kb-extract` preset | Queue claim → job lifecycle | No (queue path) |
| `creator kb queue-extract --chapter N` | CLI sugar → `kb_extract_jobs` row | No (queue path) |
| **`novel-manuscript-audit` (this spec)** | On-demand read chapter → review report and/or sync extract | **No** |

---

## 3. Modes

### 3.1 `mode=review`

**Input** (minimum):

| Field | Required | Notes |
| --- | --- | --- |
| `work_id` | yes | Target Work |
| `chapter` | yes | Integer chapter number |
| `volume` | no | Default `1`; required when Work has multi-volume chapters |
| `body_path` | no | Override; default resolved from `work_chapters` + layout SSOT |

**Behavior**:

1. Read chapter body from resolved `body_path`.
2. Run structured review (五问 baseline per [workflow-profile.md §5.1](workflow-profile.md#51-chapter-finalize-quality-gate); optional extended checks in preset prompts).
3. Write human-readable report under `Works/<work_ref>/Logs/review/` (filename includes chapter + volume label).
4. Optionally upsert `findings` rows when review detects actionable issues (`upsert_findings: true` default for review mode).

**Output artifacts**:

- `Logs/review/audit-ch{nn}-v{vol}-{timestamp}.md` (or equivalent stable naming locked in the implement plan)
- Optional `findings` rows with `target_executor` per [quality-loop.md §2.2](quality-loop.md#22-executor-routing)

### 3.2 `mode=extract`

**Input**: same locators as §3.1.

**Precondition**: Work must be **World-bound** (`world_id` non-null). Worldless Works receive `422 world_required_for_extract`.

**Behavior**:

1. Read chapter body from resolved path.
2. Invoke `kb.extract_work` capability for promoted KnowledgeEntries **without** `kb_extract_jobs` queue ceremony.
3. Do not create a FL-E driver schedule.

**Output**: KnowledgeEntries upserted per World KB rules; optional summary line in CLI stdout.

---

## 4. CLI entry

> **No CLI audit entry.** The CLI daemon group and the generic preset runner no longer exist, so no CLI audit entry is provided. The §3 modes and §5 preset contract are unchanged; audit admission goes through the core execution surface (schedule add via `POST /v1/daemon/orchestration/schedules`, served by `apps/nexus-service`). The pre-retirement CLI IA is recorded as history in [creator-run-preset-entry.md](../../archived/creator/2026-09-21-creator-run-preset-entry.md).

**Invariants**:

- Audit admission must **not** set `fl_e_stage` driver fields or enqueue auto-chain continuation.
- Admission must respect `runtime_lock_holder` when Work is locked (same as other mutating Work paths).

---

## 5. Preset contract (embedded)

Two shipped embedded presets, one per mode:

| Preset ID | Mode | Declared `cli_args` |
| --- | --- | --- |
| `novel-manuscript-audit-review` | `review` | `chapter` (integer, required); `volume` (integer, optional, default 1) |
| `novel-manuscript-audit-extract` | `extract` | `chapter` (integer, required); `volume` (integer, optional, default 1) |

`cli_args` is a field of the preset manifest; its schema is owned by [orchestration-engine.md](../orchestration/orchestration-engine.md) §7.

Input locators (both presets): `work_id`, `chapter`, optional `volume`, optional `body_path`. Review mode additionally accepts `upsert_findings` (boolean, default true).

---

## 6. Acceptance (spec-level)

1. Hermetic test: review mode writes report file under `Logs/review/`.
2. Hermetic test: extract mode on World-bound Work promotes at least one KnowledgeEntry path (mock LLM acceptable).
3. Hermetic test: extract on worldless Work fails closed with documented error.
4. No regression: auto-chain driver invariants unchanged (one active FL-E driver per Work).
5. CLI help cites [docs/README.md](../../../../docs/README.md).
