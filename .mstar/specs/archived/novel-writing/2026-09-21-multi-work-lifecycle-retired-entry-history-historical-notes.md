# Multi-Work Lifecycle — Retired Entry and Amendment History Historical Notes

**Provenance**: frozen from [`specs/active/novel-writing/multi-work-lifecycle.md`](../../active/novel-writing/multi-work-lifecycle.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-09-21 — v1.193 P2 removed the `creator run` runner, `creator bootstrap`, and `creator works resume-chain` entries this file narrated; earlier V1.41 (PR #53, post-merge `156e669d` / `12753eb8`, 2026-06-11) and V1.42 P0 provenance is preserved below. Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Removed header lines

**Status**: Shipped (V1.41 — PR #53; post-merge `156e669d` / `12753eb8`)

**V1.42 amend**: §4.2 production acquire gap —  P0

## §1 removed sentence

V1.39–V1.40 automate chapter production per Work.

## §3 lock-table removed parenthetical

(the retired `creator run <preset_id>` dispatch and `creator works resume-chain` entries were removed in v1.193 P2)

## §3.3 removed blocks

5. `--extend-chapters` has no shipped path: the retired `run resume --reopen` flag is gone with the runner.

**Historical (retired runner, removed in v1.193 P2):** the old two-step semantics — `creator works completion-lock release` merely *enabling* a separate `creator run resume --reopen` that set `works.status = active` and cleared `novel_completion_status`, carrying a **required** `--extend-chapters <new_total>` flag that seeded new `work_chapters` rows before resume enqueue — belonged to the retired `creator run` runner. The retained atomic `creator works reopen` performs the release in one core call and never sets `works.status = active`.

Distinct from `creator bootstrap --from-work` (new Work + `lineage_from_work_id`; the `creator bootstrap` entry itself was removed in v1.193 P2 — the lineage contract remains the historical record).

## §4.2 removed amendment framing

- Heading suffix `(V1.42 P0 — Implemented)`.
- ~~**Gap (PR #53 security re-review)**: V1.41 shipped DB columns and spec rules but **production paths do not yet acquire** `runtime_lock_holder`.~~
- `**Resolved (V1.42 P0)**:` prefix on the acquire/release statement (statement kept present-tense).
- Table qualifiers: `creator works` row `(V1.45 P2 migration; the `creator run` runner rows were removed in v1.193 P2)` + `**Required** (P0)`; service row `; historically daemon handlers`; stale-recovery `; historically the daemon`.

## §5.1 removed whole subsection (retired `creator run` entry)

### 5.1 ~~`creator run <preset_id>`~~ — strategy execution (**retired in v1.193 P2**, historical record)

The generic preset dispatch entry `creator run <preset_id> [<work_id>]` (V1.45) was removed in v1.193 P2; there is no current CLI preset-dispatch entrance. Historical rules of that entry:

- Optional `work_id`: omit → pool `active` → bound `work_id`; if none, fail → `creator works use`. (This resolution rule survives on the retained `creator works` atomic commands.)
- **No** `list` / `status` on `run` (hard-removed V1.41); use `creator works list` / `creator works status`.
- Current preset admission: schedule add via the core execution surface (`POST /v1/daemon/orchestration/schedules`, served by `apps/nexus-service`); the retained CLI scheduling surface is `creator works cron` declaration editing.

## §5.2 removed whole subsection (retired `creator bootstrap --from-work` entry)

### 5.2 ~~`creator bootstrap --from-work <completed_work_id>`~~ (**retired in v1.193 P2**, historical record)

The `creator bootstrap` entry was removed in v1.193 P2; no current CLI scaffold/lineage entry exists. Its historical contract:

- Creates **new** Work; sets `works.lineage_from_work_id` on the new row.
- **Validation (shipped `12753eb8`)**: `completed_work_id` must exist, belong to the active creator, and be in `completed` status; otherwise **422** before INSERT.
- Copies optional `creative_brief` defaults from completed Work metadata (not filesystem tree).
- Does **not** mutate or resume the completed Work.

## §6 removed cell qualifier

| Otherwise | V1.39 rules unchanged |

## Additional retired-surface rewords (same pass)

- Coordinate line: dropped `creator bootstrap --from-work` / `resume reopen`; now `creator works`, completion-lock release, reopen.
- §1.1 cell and §1.1/§4.1 heading provenance tags `(grill-me locked)` / `(grill-me A)` dropped (process markers, no content).
- §1.1 pool row `creator run` subcommands → `creator works` subcommands.
- §5.4 `runtime lock like CLI continue` → `like CLI inspire`.
- §6 heading `Daemon behavior` → `Auto-chain gating` (retired host naming).

## Removed footer

*Shipped V1.41. §4.2 production wiring is V1.42 P0.*
