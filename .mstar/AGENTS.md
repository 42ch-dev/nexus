# Nexus OSS — Harness Directory (`{HARNESS_DIR}`)

> Project rules: root [`AGENTS.md`](../AGENTS.md). Runtime harness: upstream `mstar-*` skills.

## Conflict resolution priority

On conflicts (user has not overridden): **1** current instruction → **2** root `AGENTS.md` → **3** this file → **4** `mstar-*` skills. Higher number yields to lower.

**Read order (not precedence):** load `mstar-harness-core` first for harness context; then other `mstar-*` on demand per its role matrix.

## Concepts (path symbols)

| Symbol | Path (this repo) |
|--------|------------------|
| `{HARNESS_DIR}` | `.mstar/` |
| `{PLAN_DIR}` | `plans/` |
| `{SDD_DIR}` | `sdd/<plan-id>/` — **ephemeral scratch** (gitignored; not handoff) |
| `{ITERATION_DIR}` | `iterations/` |
| `{KNOWLEDGE_DIR}` | `knowledge/` — cross-cutting patterns, conventions, compound output |
| `{SPECS_DIR}` | **`specs/`** — frozen normative specs / ADR (upstream default). Wire contracts: repo-root `schemas/` |

## Layout & write boundaries

**Plans:** one `.md` file per plan under `plans/` — never `plans/<plan-id>/` as a directory. Layout details → `mstar-plan-artifacts/references/plan-files-and-reports.md`.

| Path | Writers | Purpose |
|------|---------|---------|
| `{SDD_DIR}` | Implementers (SDD default), PM | Session-local briefs, per-task reports, `progress.md`, review diffs — **not** clone handoff |
| `{SDD_DIR}/review/` | `qc-specialist*`, PM, QA | QC/QA **raw process reports** (gitignored review bundle) |
| `plans/<plan-id>-*.md` | Implementers (checkboxes), PM | Main plan — durable gate summaries live here |
| `specs/` | product-manager, architect, writing-specialist | Long-lived normative specs; plan `primary_spec` / `spec_refs` |
| `knowledge/` | `mstar-compound`, writing-specialist | Cross-iteration patterns — **not** normative product contracts |

**SDD default:** implementors and QC/QA write `{SDD_DIR}` (including `review/`). Inline/hotfix: no `{SDD_DIR}` unless Assignment says otherwise.

**Reachability:** git-tracked harness handoff (`plans/`, `specs/`, `status.json`, compasses, archived snapshots, etc.) must survive a fresh `git clone`. **`{SDD_DIR}` is excluded** — ephemeral working space; durable QC decisions are summarized on the main plan (`## Review Gate Summary` / `## QA Gate Summary`) and in `status.json` `residual_findings`.

**Legacy:** `plans/reports/` was the pre-V1.99 tracked QC path. It is **retired** — do not create new files there. Historical raw reports may exist locally under `.mstar/sdd/<plan-id>/review/` after migration.

**Content:** `docs/` = human contributor docs; `{ITERATION_DIR}` = compasses + per-iteration workspace; `{KNOWLEDGE_DIR}` layout → [`knowledge/AGENTS.md`](knowledge/AGENTS.md); `{SPECS_DIR}` layout → [`specs/AGENTS.md`](specs/AGENTS.md).

## Pre-merge checklist

1. `status.json` + `notes.json` (narrative timeline)
2. `pnpm run codegen` if `schemas/` changed
3. `nexus-platform` `roadmap.md` if plan `Done`
4. Profile B closeout → `mstar-plan-artifacts/references/done-compaction.md`
5. `wc -c .mstar/status.json` < 20_000; archive resolved residuals
6. Refresh `metadata.tech_debt_summary` via `mstar-plan-artifacts/scripts/tech-debt-rollup.sh` (counts only — narrative in `notes.json`)

Git hygiene → root [`AGENTS.md`](../AGENTS.md) § Git & repository hygiene.

## Project deviations

### `status.json` — structured metadata only

Narrative (ship stories, QC summaries, refresh rationales) → **`notes.json`**, commits, or compass — not `metadata` prose.

**Rule:** if a `metadata` value is a sentence or paragraph, it is forbidden. Counts, enums, dates, paths, and short IDs are OK.

**`tech_debt_summary`:** optional rollup per `mstar-plan-artifacts/references/status-and-residuals.md` — `total_open`, `by_severity`, `by_target`, `by_plan`, `updated_at` only. Refresh with `tech-debt-rollup.sh`; do **not** add `refreshed_reason`, `*_ship_note`, or similar prose fields.

**Branch metadata:** upstream canonical fields only (`iteration_base_branch`, `spec_integration_branch`, `target_branch`; per-plan `working_branch`, `merge_target`) — see `mstar-iteration` §2.3, `mstar-plan-artifacts/references/status-and-residuals.md`. Branch names and Git workflow → `mstar-branch-worktree`, `mstar-plan-conventions`.

**Legacy keys (`integration_branch`, `integration_merge_target`):** tolerated in archived JSON; **do not write on new iterations**.

### Git & PR merge policy

All landings on the protected branch (`target_branch`, usually `main`) via **GitHub PR with squash merge** — iteration integration, plan topics (via integration), and hotfixes. Branch naming → upstream (`mstar-iteration`, `mstar-branch-worktree`); PM assigns explicit names in Assignment.

### Profile B compaction

Adopted (`mstar-plan-artifacts/references/done-compaction.md` Template B): hot `plans[]` = non-`Done` only; `archived/plans/<plan-id>.json` = snapshot; `archived/plans-done.json` → `{ "plans": ["<id>", ...] }` strings only.

**Nexus legacy:** `plans-done.json` may still carry `iteration_summaries` from older closeouts — **do not extend**; new delivery snapshots → [`archived/shipped-features-tracker.md`](archived/shipped-features-tracker.md) or `notes.json`.

### Residual detail prose (optional)

`plans/residuals/<plan-id>/<finding-id>-<label>.md` supplements root `residual_findings`. Archive closed rows → `archived/residuals/<plan-id>.json`.

**`tracking_link`:** point to durable surfaces — `.mstar/plans/<plan-id>.md` (gate summary) or `.mstar/status.json` `residual_findings[<plan-id>]` — not gitignored `{SDD_DIR}/review/` paths.

### Post-merge hotfix

1. Register `residual_findings` before branching.
2. `fix/*` from current `main` HEAD (PM-named per `mstar-branch-worktree`).
3. Surgical fix + regression test; verify per root `AGENTS.md` Development Policy.
4. Squash-merge PR to `main`; update `status.json`. Prepare compression → `mstar-phase-gates`.

### Pre-existing failure claims (PM override)

Before accepting “pre-existing” to waive a test/QC finding: reproduce against **current** `origin/<target_branch>` HEAD. Passes there → not pre-existing. Fails there → document base SHA + repro. Flaky → fixed seed or measured rate.
