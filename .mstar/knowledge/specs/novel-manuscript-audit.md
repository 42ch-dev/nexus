# Novel Manuscript Audit — Normative Specification v1

**Status**: Draft (V1.44 — 2026-06-13)  
**Document class**: Draft overlay  
**Created**: 2026-06-13  
**Scope**: On-demand chapter audit for `work_profile: novel` — structured review report and/or World KB extract **without** entering the full FL-E auto-chain driver  
**Coordinates with**:

- [novel-quality-loop.md](novel-quality-loop.md) — findings lifecycle; review routing
- [novel-workflow-profile.md](novel-workflow-profile.md) — chapter paths, `Logs/review/`, 五问 baseline
- [creator-workflow.md](creator-workflow.md) — FL-E stages (audit is **out-of-band**)
- [cli-spec.md](cli-spec.md) — `creator run audit-chapter` IA (P0 implement)
- [entity-scope-model.md](entity-scope-model.md) — World-bound extract mode
- [world-kb-runtime-architecture.md](../world-kb-runtime-architecture.md) — `kb.extract_work` on-demand path

**Iteration compass**: [v1.44-novel-quality-and-serial-hardening-delivery-compass-v1.md](../../iterations/v1.44-novel-quality-and-serial-hardening-delivery-compass-v1.md)  
**Tracker**: DF-69

---

## 1. Purpose

V1.39–V1.43 shipped inline finalize `llm_judge`, FL-E `reflection-loop`, `novel-review-master` on **existing findings**, and queue-based `kb-extract`. Authors still lack a **single on-demand entry** to audit an already-written chapter body (`Works/<work_ref>/Stories/ch*.md`) outside the auto-chain driver.

V1.44 P0 implements **DF-69**: a dual-mode embedded preset (or preset pair) plus CLI entry.

---

## 2. Distinction from shipped presets

| Surface | Role | Enters FL-E driver? |
| --- | --- | --- |
| `novel-writing` finalize `llm_judge` | Inline quality gate on finalize transition | Yes (produce stage) |
| `reflection-loop` | Default FL-E `review` stage | Yes |
| `novel-review-master` | Master decisions on **existing** findings | Auxiliary schedule only |
| `kb-extract` preset | Queue claim → job lifecycle | No (queue path) |
| **`novel-manuscript-audit` (this spec)** | On-demand read chapter → review report and/or extract | **No** |

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
2. Run structured review (五问 baseline per [novel-workflow-profile.md §5.1](novel-workflow-profile.md); optional extended checks in preset prompts).
3. Write human-readable report under `Works/<work_ref>/Logs/review/` (filename includes chapter + volume label).
4. Optionally upsert `findings` rows when review detects actionable issues (`upsert_findings: true` default for review mode).

**Output artifacts**:

- `Logs/review/audit-ch{nn}-v{vol}-{timestamp}.md` (or equivalent stable naming locked in P0 plan)
- Optional `findings` rows with `target_executor` per [novel-quality-loop.md §2.2](novel-quality-loop.md)

### 3.2 `mode=extract`

**Input**: same locators as §3.1.

**Precondition**: Work must be **World-bound** (`world_id` non-null). Worldless Works receive `422 world_required_for_extract`.

**Behavior**:

1. Read chapter body from resolved path.
2. Invoke `kb.extract_work` capability for promoted KeyBlocks **without** `kb_extract_jobs` queue ceremony.
3. Do not create a FL-E driver schedule.

**Output**: KeyBlocks upserted per World KB rules; optional summary line in CLI stdout.

---

## 4. CLI entry (normative sketch — P0 locks single IA)

**Preferred** (compass grill-me 2026-06-13):

```bash
nexus42 creator run audit-chapter <work_id> --mode review|extract [--chapter N] [--volume V]
```

**Alternatives** (document only; P0 plan picks one):

- `nexus42 daemon schedule add --preset novel-manuscript-audit --input '{"mode":"review",...}'`
- Subcommand under `creator run` with `audit-chapter` as a run intent

**Invariants**:

- Command must **not** set `fl_e_stage` driver fields or enqueue auto-chain continuation.
- Command must respect `runtime_lock_holder` when Work is locked (same as other mutating `creator run` paths).

---

## 5. Preset contract (embedded)

| Preset ID | Mode | Notes |
| --- | --- | --- |
| `novel-manuscript-audit` | `review` + `extract` | Single preset with `mode` input **or** two thin presets; P0 plan decides |

Minimum preset surface:

- `preset.input.mode`: `review` | `extract`
- `preset.input.work_id`, `chapter`, optional `volume`, optional `body_path`
- `preset.input.upsert_findings`: boolean (review mode only; default true)

---

## 6. Acceptance (spec-level)

1. Hermetic test: review mode writes report file under `Logs/review/`.
2. Hermetic test: extract mode on World-bound Work promotes at least one KeyBlock path (mock LLM acceptable).
3. Hermetic test: extract on worldless Work fails closed with documented error.
4. No regression: auto-chain driver invariants unchanged (one active FL-E driver per Work).
5. CLI help cites [docs/novel-writing-quickstart.md](../../../docs/novel-writing-quickstart.md) §5 after P1 merge.

---

## 7. Promotion (iteration close)

At V1.44 P-last hygiene:

- [ ] Promote Status to **Shipped (V1.44)** or merge into `novel-quality-loop.md` §3 if section stabilizes.
- [ ] Update deferred tracker DF-69 → shipped archive.

---

*Draft overlay for V1.44 P0. P0 implement plan is scope authority for preset YAML and exact CLI flags.*
