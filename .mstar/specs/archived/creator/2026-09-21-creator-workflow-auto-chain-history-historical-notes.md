# Creator Workflow — Auto-Chain and Runner History Historical Notes

**Provenance**: frozen from [`specs/active/creator/creator-workflow.md`](../../active/creator/creator-workflow.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-09-21 — v1.193 P2-T1 retired the incomplete Creator runner and auto-chain/resume-chain entrances in that iteration; the removed blocks are the file's shipped-history records of the V1.33–V1.45 execution surface. Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Removed Status line and footer (replaced present-tense)

**Status**: Shipped (V1.34 — 2026-06-05; V1.35 P4 partial; V1.39 — DF-53 full auto-chain + daemon continuity; **V1.40 Shipped** — DF-63 W5 `kb-extract` persistence via `novel-review-master sync_world_kb`: World-bound Works enqueue extract with `work.world_id`, `source_kind=work_chapter`, `source_locator={{preset.input.body_path}}`, `work_id`; worldless Works (legacy V1.39-and-earlier) skip World promotion; V1.79 additive SOUL visualization contract over memory fragments)

**Current authority** (removed form): Retained Work stage/field model only for workflow execution; the incomplete Creator runner and auto-chain/resume-chain entrances were removed in v1.193 P2-T1. Automatic progression and restart continuation below are historical shipped behavior, not current guarantees (§0).

Footer (removed): *Normative for retained Work stage/field vocabulary and the separate SOUL read contract; execution and auto-chain sections preserve V1.34–V1.45 shipped history, retired in v1.193 P2-T1 (§0).*

## §0 removed framing

- Heading suffix "(v1.193 P2-T1)".
- "[`apps/nexus42/src/commands/creator/works/mod.rs`] records the v1.193 P2-T1 removal of the incomplete Creator runner and `works intake` / `works resume-chain`." → present-tense.
- "**Reading boundary:** the execution rules, command journeys, automatic full-stage/chapter progression, and daemon restart continuation in §1–§7 are preserved as shipped history, not present-day scheduling instructions. …" → present-tense.

## §1 removed historical paragraph

**Historical shipped behavior (retired v1.193 P2-T1):** the workflow reused the existing scheduler and World/KB SSOT. After V1.35 P4, `creator bootstrap` chained intake → produce by default (`--chain-novel-writing`, default true). **V1.39** shipped full-stage `--auto-chain` (default **true**, opt-out `--no-auto-chain`): while the daemon was online, stages advanced through `research → produce → review → persist` per chapter without a manual preset dispatch at each boundary. Daemon restart resumed from a Work continuation checkpoint (DF-68). These are not guarantees of the retained stage fields (§0).

## §2 removed relationship table

> The table records the shipped relationship. Its onboarding and preset-runner progression entries are historical; current CLI operations are the retained `creator works` families (§0).

| Concept | Work model (baseline) | Staged workflow (this spec) |
| --- | --- | --- |
| Work container | Shipped | Extended with `stage`, `stage_status` |
| Entry | `creator bootstrap` | Composite onboarding; intake still `work_init` |
| Continue inspiration | `creator works inspire --note` | Unchanged |
| Stage progression | N/A | **`creator run <preset_id>`** (preset runner applies stage gates before enqueue) |
| Primary produce preset | `novel-writing` | Default for `produce` stage |
| Generic multi-stage workflow | Deferred | **Shipped V1.34** |

("New invariant (V1.34)" label on invariant 4 also dropped.)

## §3 removed framing

- Blockquote: "> Stage identifiers and stored fields remain part of the Work model. The schedule-driven meanings, force-skip rules, and stage-gate command examples below describe the historical runner; they do not supply a current dispatch or resume entrance (§0)."
- §3.1 review row "(V1.47: renamed from `reflection-loop`)".
- §3.3 CLI block `creator run <preset_id> [<work_id>]` line and "(V1.41; …)" on the status line.

## §4 removed annotation tags

Heading "(historical shipped mapping)"; intake row "Shipped V1.33"; review row "V1.47 P0: renamed from `reflection-loop` (compass §0.1 #6)."; persist row "**V1.40 P3**:".

## §5 removed historical journeys (§5.1, §5.3, §5.4 and retired spellings)

> **Historical execution journeys:** §5.1–§5.5 retain the shipped CLI/auto-chain record, including command spellings removed or migrated since then. They are not runnable current instructions. §5.6 is the separate retained SOUL visualization contract.

### 5.1 Happy path (explicit stages)

```text
creator workspace init && daemon start && acp agent use
creator bootstrap --idea "..."
  → intake schedule → brief complete
creator run research <work_id>      # preset runner validates stage gates, PATCHes Work stage
creator run novel-writing <work_id>
creator run novel-chapter-review <work_id>
creator run kb-extract <work_id>
creator memory review <id>
creator kb queue-extract --world-id <work.world_id>  # World-bound novel Works (V1.40 P3)
```

For **World-bound** novel Works (`work.world_id != NULL`), the auto-chain `persist` stage MUST enqueue extraction targeting `work.world_id`. For **worldless** Works, persist skips World KB promotion.

### 5.2 (original spelling, retired)

```text
creator run continue <work_id> --note "new angle"
```

### 5.3 Power user

(Historical — retired in v1.193 P2.) `daemon schedule` was the power-user surface; the group is deleted. Schedules created for a Work **must** record `work_id` and stage id in schedule seed/metadata (wire key `fl_e_stage` in V1.34 implementation) — this metadata rule survives on the retained execution surface (schedule add via `apps/nexus-service`; CLI declaration via `creator works cron`).

### 5.4 Daemon-attached auto-chain (V1.39 shipped history; retired v1.193 P2-T1)

When `auto_chain_enabled` on a Work (default true for new starts):

1. **Online**: on stage/chapter completion, the engine enqueues the next FL-E driver schedule without a manual `creator run <preset>` dispatch.
2. **Chapter outer loop**: after `persist` for chapter N, auto-enqueue `produce` for chapter N+1 until Work completion (novel profile).
3. **Checkpoint**: persist `stage`, `chapter`, `driver_schedule_id`, and `auto_chain_interrupted` on daemon shutdown or unexpected pause.
4. **Boot resume**: daemon restart auto-resumes only schedules tied to checkpointed auto-chain Works; other schedules remain paused (safe default).

`creator run resume <work_id>` recovers when auto-resume did not run or user disabled auto-chain.

**Retirement boundary:** the V1.39 list above is historical shipped behavior. Both the old `creator run resume` spelling and its later `creator works resume-chain` entrance are removed; no current Work restart-continuation command is claimed. The retained checkpoint fields are observable state only (§0), not evidence of a resumed chain. [daemon-runtime.md](../runtime/2026-09-21-daemon-runtime.md) is itself a retired-host record and does not restore this execution path.

### 5.5 (removed row provenance)

`creator works inspire --note` row was "(V1.45 P2 path; V1.39 form was `creator run continue --note`, retired in v1.193 P2)"; heading was "(V1.39 extension)"; intro said "Insertions during an active auto-chain"; closer was "Invariant: at most one active FL-E stage driver schedule per Work remains enforced."

### 5.6 heading tag "(V1.79 extension)" and opening "V1.79 adds a read-only reflection surface over …" sentence.

## §6 removed rows and framing

- Blockquote: "> Execution-related rows below are historical shipped constraints, including auto-chain, completion-lock tick suppression, multi-Work scheduling, and the former CLI IA. Retained state/atomic operations do not re-enable those execution entrances (§0)."
- `--auto-chain` row: "**V1.39 target (DF-53)**: default true for full FL-E chain + chapter outer loop; `--no-auto-chain` opt-out; manual `creator run <preset>` dispatch still valid for power users".
- Novel completion "V1.41 extends … (DF-60)".
- Completion-lock: "auto-chain **must not** tick that Work; after release, `resume --reopen` may resume same `work_id` (V1.41 P0)".
- Runtime lock "(V1.41 P0)".
- Multi-Work "may auto-chain concurrently".
- CLI IA row: "`creator run` = single-Work actions; `creator works` = list/status/use/pool (V1.41)".

## §7 removed acceptance items (historical spec-level)

Heading tag "(historical spec-level)"; item 2 "The preset runner rejects wrong stage order without `--force-gates` (stage gate validation inside `creator run <preset_id>`)."; item 3 "Demo path in V1.34 compass §4 is achievable on integration branch."

## V1.45 supersession section (removed)

**Historical supersession:** [creator-run-preset-entry.md](../creator/2026-09-21-creator-run-preset-entry.md) shipped as the V1.45 Master and superseded the FL-E CLI table with preset-id grammar and three-plane IA. That runner was retired in v1.193 P2-T1; its historical promotion is not current dispatch authority. Current workflow scope is §0.

## Annotation drops (reworded in place; originals in git history)

- Coordinates-with cli-spec line "retired `creator run` / `creator bootstrap` history is not a current entry contract" → "no `creator run` / `creator bootstrap` CLI entry"; workflow-profile line "(Draft V1.36)".
- "Last updated" narration "— separate retained state from retired runner/auto-chain behavior"; Scope "historical preset dispatch/auto-chain contract; V1.79 read-only SOUL visualization".
