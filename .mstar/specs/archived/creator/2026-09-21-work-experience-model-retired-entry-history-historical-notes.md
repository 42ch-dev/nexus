# Work Experience Model — Retired Entry and Version-Ledger History Historical Notes

**Provenance**: frozen from [`specs/active/creator/work-experience-model.md`](../../active/creator/work-experience-model.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-09-21 — v1.193 P2 (T1) retired the `creator run` runner, `creator bootstrap`, and the `daemon schedule` group whose V1.33-era contracts this file preserved inline. Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Removed header lines

**Status**: Shipped (V1.33 — Work loop + Creative Brief Intake + `creator run` + `run_intents`)

**Current authority (v1.193 P2)**: the Work/entity/journey model below is retained; the `creator run` runner, `creator bootstrap`, and the `daemon schedule` group were removed in v1.193 P2. Where the body names those entries, read them as the V1.33-era contract: the surviving CLI equivalents are the `creator works` atomic operations (`inspire` ← `run continue`, `reopen` ← `run resume --reopen`, `reconcile-chapters`, `status`/`list`); preset admission has no CLI entry and goes through the core execution surface (`POST /v1/daemon/orchestration/schedules`, served by `apps/nexus-service`), with `creator works cron` as the retained CLI scheduling declaration surface.

**Shipped**: 2026-06-04 (compass  — 5 plans P1–P5 all Done)

Coordinate qualifiers (removed): cli-spec `— creator works / preset (current); creator run, system preset (retired in v1.193 P2)`; workflow-profile `(Draft V1.36)`; essay-profile `(Draft V1.52; shipped V1.63)`.

## §1 removed qualifiers

Without Work, users must understand `daemon schedule`, preset IDs, seed strings, and `core_context` — which is correct for power users but not for a complete product cycle. (V1.33 problem statement; the `daemon schedule` group itself was retired in v1.193 P2.)

## §2 removed cell provenance

Shipped V1.33; current persistence: `nexus-local-db` (`works` table/DAO); operations: `nexus-core` + CLI `creator works` (§3.3)

## §3 removed field provenance

- `story_ref` cell suffix ` (V1.36)`.
- `work_profile` cell `novel` (V1.36+), `essay` (V1.63), `game_bible` (V1.54), `script` (V1.55).
- §3.3 heading suffix `(V1.33 target)`; `**Current implementation ownership** (the V1.33 shipped product history remains unchanged)`.
- §3.3 tail sentence: Historical `creator run` spellings in this spec are not current implementation owners or dispatch entrances; see creator-run-preset-entry.md §0 for the v1.193 P2-T1 retirement.

## §5 removed markers

- `Allowed values (closed enum for V1.33):` qualifier.
- `Embedded presets updated in V1.33 plans.`
- §5.2 heading suffix `(V1.33 target)`.

## §6 removed blocks

### 6.1 removed parenthetical

Same as [cli-spec.md] §7: workspace init, active creator, ACP agent use (the V1.33 list named `daemon start`; the daemon group was retired in v1.193 P2 and no service-launch step exists).

### 6.2 Start Work (`creator bootstrap`) — **entry retired in v1.193 P2** (historical contract) — removed whole subsection

```text
creator bootstrap --idea "<text>" [--preset novel-writing] [--world-id <id>]
```

Behavior:

1. Create Work (`draft` → `active`, `intake_status=pending`).
2. If intake required: enqueue `creative-brief-intake` schedule (`work_init`).
3. On intake complete: set `creative_brief`, `intake_status=complete`.
4. Enqueue primary preset (default `novel-writing`) with seed derived from brief + `initial_idea`.
5. Print `work_id`, schedule ids, next-step hints.

### 6.3 removed qualifiers and historical steps

- Heading suffix `; V1.33 entry was `creator run continue`, retired in v1.193 P2`.
- 2. (Historical V1.33 step, retired:) the note was merged into the schedule `core_context` via `daemon schedule edit --append` semantics; that group was removed in v1.193 P2 and the current `inspire` command is a pure side-input lane that does not touch schedules.
- 3. (Historical V1.33 step, retired:) optionally start/resume preset with `work_continue` intent — preset admission now goes through the core execution surface, not a CLI flag (`--preset` was dropped in the V1.45 P2 migration).

### 6.4 removed parenthetical

(V1.41 command paths; the V1.33 forms were `creator run list` / `creator run status`, retired in v1.193 P2.)

### 6.5 removed historical narration

(Historical — V1.33.) `daemon schedule ...` was the power-user escape hatch; the group was deleted in v1.193 P2. Today the retained scheduling surfaces are `creator works cron` declaration editing (CLI) and the `/v1/daemon/orchestration/schedules` add/signal/list/inspect routes served by `apps/nexus-service`.

## §8 removed qualifiers

- `V1.33 requires runtime alignment with` (kept present-tense).
- (the V1.33 recovery verbs were `daemon schedule advance` / `creator run continue`, both retired in v1.193 P2; today the author appends context via `creator works inspire --note` and the next admission comes from the execution surface)

## §10 removed blocks

- Heading suffix `— V1.34`; `V1.34 extends Work with:` opener.
- Stage progression provenance: the V1.33 dispatch entry was `creator run <preset_id>` (the preset runner validated stage gates and PATCHed Work stage fields before enqueue — not default auto-chain; see DF-53). The runner was retired in v1.193 P2;
- FL-E reused V1.33 `creator bootstrap` (composite onboarding) and `creator run <preset_id>` (strategy execution) and did not replace `daemon schedule` (V1.33–v1.192 arrangement; all three entries were retired in v1.193 P2).

## §11 Resolved / deferred questions — removed whole section (version-shipping ledger)

| Topic | Status |
| --- | --- |
| `works` DDL | Shipped V1.33 P1; stage columns V1.34 P1 |
| `creative-brief-intake` standalone preset | Shipped V1.33 P2 |
| Brief JSON from orchestration | Shipped V1.33 P2 |
| Auto-start `novel-writing` after intake | V1.33 `--chain-novel-writing`; V1.34+ uses the preset runner for `produce` |

## Removed footer and V1.45 supersession section

*Normative product model for Work + FL-E. Shipped V1.33 / V1.34.*

## Additional retired-surface rewords and bare markers (same pass)

- §4 fail-closed rule: `novel-writing` cannot be admitted to run (was "cannot start via `creator run`").
- §3.3 SSOT bullet `(exact DDL in P1 plan)` pointer dropped (bare plan pointer, no content).
- Header `**Shipped**: 2026-06-04 (compass — 5 plans P1–P5 all Done)` removed except the plain `**Created**: 2026-06-04` date.

## V1.45 supersession (P-last promotion)

**Superseded by**: [creator-run-preset-entry.md](2026-09-21-creator-run-preset-entry.md) (Shipped Master V1.45). The side-input lane (`creator works inspire`), `run_intents` enforcement at schedule creation, and the `creator bootstrap`-only new-Work rule are now part of the canonical Master body.
