# Nexus OSS — Harness Directory (`.mstar/`)

> Project identity & tech stack: root [`AGENTS.md`](../AGENTS.md).  
> Runtime lifecycle: upstream Morning Star `mstar-*` skills (not duplicated here).

This file is the **harness-layer SSOT** for path layout, git visibility, and write boundaries in Nexus.

## Conflict resolution

On conflicts (unless the user overrides):

1. Current user instruction  
2. Root [`AGENTS.md`](../AGENTS.md)  
3. **This file**  
4. Upstream `mstar-*` skills  

**Read order (not precedence):** `mstar-harness-core` first; then other `mstar-*` on demand.

## Principle — process vs results

**Process stays out of git. Results are shared with the team.**

| Class | Meaning | Git |
|-------|---------|-----|
| **Results** | Normative or reusable artifacts others must clone | **tracked** |
| **Process** | Local orchestration, scratch, coordination state | **ignored** |

| Path (under `.mstar/`) | Class | Purpose |
|------------------------|-------|---------|
| `AGENTS.md` | Result | This harness contract |
| `specs/` | Result | Frozen normative OSS specs / ADRs |
| `knowledge/` | Result | Compounded cross-iteration knowledge + shared trackers |
| `plans/` | Process | Main plans, checkboxes, gate summaries |
| `iterations/` | Process | Compass, guides, iteration packages |
| `sdd/` | Process | SDD scratch + QC/QA raw review bundles |
| `archived/` | Process | Local / archived process snapshots (plans-done, residuals, legacy knowledge dumps) |
| `status.json` | Process | v2 active-lifecycle register |
| `workflows/` | Process | Per-lifecycle snapshot + `notes.jsonl` ledger |
| `projects/` | Process | Per-project `roadmap.md`, residual register, research |

**Rules:**

- Agents **may** read/write process paths locally for orchestration.
- Do **not** `git add -f` ignored paths unless the user explicitly overrides for a one-off handoff.
- Root [`.gitignore`](../.gitignore) and [`.mstar/.gitignore`](.gitignore) encode the ignore list — keep them in sync with this table.
- Fresh clone: recreate process files from `mstar-plan-conventions` / `mstar-plan-artifacts` templates as needed. Process paths are **not** clone-shared SSOT.
- Prefer **result-only commits** under `knowledge/` / `specs/` (and product trees). Do not mix process paths into those commits.

Wire/schema **code** SSOT remains repo-root `schemas/` (outside `.mstar/`). Language packages are under `packages/` and `crates/`.

## Path symbols

| Symbol | Resolves to (this repo) | Git |
|--------|-------------------------|-----|
| `{HARNESS_DIR}` | `.mstar/` | mixed — see table above |
| `{SPECS_DIR}` | `.mstar/specs/` | tracked |
| `{KNOWLEDGE_DIR}` | `.mstar/knowledge/` | tracked |
| `{PLAN_DIR}` | `.mstar/plans/` | ignored |
| `{ITERATION_DIR}` | `.mstar/iterations/` | ignored |
| `{SDD_DIR}` | `.mstar/sdd/<plan-id>/` | ignored |
| `{WORKFLOW_DIR}` | `.mstar/workflows/` | ignored |
| `{PROJECT_DIR}` | `.mstar/projects/` | ignored |

Plan `metadata.primary_spec` / `spec_refs` should point at paths under `{SPECS_DIR}` when the contract is team-shared. Iteration-scoped drafts under `{ITERATION_DIR}/…/specs/` are **local process** until promoted into `{SPECS_DIR}`.

## Layout & write boundaries

| Path | Typical writers | Notes |
|------|-----------------|-------|
| `{SPECS_DIR}` | product-manager, architect, writing-specialist | Long-lived normative text only — not iteration scratch |
| `{KNOWLEDGE_DIR}` | `mstar-compound` (iteration-close), writing-specialist (hygiene) | Patterns / conventions / shared trackers — **not** a second specs tree |
| `{PLAN_DIR}` | PM, implementers (checkboxes) | One `.md` per plan; never `plans/<plan-id>/` as a directory |
| `{ITERATION_DIR}` | PM, Phase 1 specialists | Compass + guides; local process |
| `{SDD_DIR}` | implementers, QC, QA | Ephemeral; durable QC/QA conclusions summarize into the main plan (locally) |
| `{WORKFLOW_DIR}` | PM | Lifecycle snapshots and notes ledger |
| `{PROJECT_DIR}` | PM | Roadmap, residuals, project research |

**Do not** put plan progress, residual prose, or QC narratives in root `AGENTS.md`.

**Do not** treat ignored process files as team handoff — share **results** (`specs/`, `knowledge/`) and product trees (`schemas/`, apps, crates) via git.

**Content:** `docs/` = human contributor docs; `{KNOWLEDGE_DIR}` layout → [`knowledge/AGENTS.md`](knowledge/AGENTS.md); `{SPECS_DIR}` layout → [`specs/AGENTS.md`](specs/AGENTS.md).

## Local process conventions (still apply on disk)

These govern **local** harness files. They are **not** clone SSOT.

### `status.json` — structured metadata only

Narrative (ship stories, QC summaries) → **`workflows/<id>/notes.jsonl`**, commits, or compass — not `metadata` prose.

**Rule:** if a `metadata` value is a sentence or paragraph, it is forbidden. Counts, enums, dates, paths, and short IDs are OK.

**`tech_debt_summary`:** optional rollup per `mstar-plan-artifacts/references/status-and-residuals.md` — counts only. Refresh per that reference (engine `techDebtRollup` when available; else recompute counts from the open list).

**Branch metadata:** upstream canonical fields only (`iteration_base_branch`, `spec_integration_branch`, `target_branch`; per-plan `working_branch`, `merge_target`).

### Profile B compaction (local)

Hot `plans[]` = non-`Done` only; snapshots under `archived/plans/` locally. Delivery snapshots go to the project trackers (`.mstar/projects/<id>/`, gitignored) or compound into `{KNOWLEDGE_DIR}` patterns.

### Residual detail (local)

Open QC residual rows live in the project register (`projects/<id>/residuals.json`). Prefer `tracking_link` to durable **tracked** surfaces (`{SPECS_DIR}`, `{KNOWLEDGE_DIR}`) when sharing with the team.

### Pre-merge checklist (PM)

1. Local `status.json` + `workflows/<id>/notes.jsonl` coherent for the session  
2. `pnpm run codegen` if `schemas/` changed  
3. Share **results** only: `{SPECS_DIR}`, `{KNOWLEDGE_DIR}`, product code — not process paths  
4. Profile B closeout locally as needed  
5. Optional: `wc -c .mstar/status.json` hygiene for local file size  

Git hygiene → root [`AGENTS.md`](../AGENTS.md).

### Git & PR merge policy

All landings on the protected branch (`target_branch`, usually `main`) via **GitHub PR** (never local merge onto the protected branch). **Merge method by PR commit count** (commits on the PR head vs base): **≤30 → merge commit**; **>30 → squash**. Detail → root [`AGENTS.md`](../AGENTS.md) Commit / Merge discipline. Branch naming → upstream (`mstar-iteration`, `mstar-branch-worktree`).

### Post-merge hotfix

1. Register residuals in `projects/<id>/residuals.json` before branching.  
2. `fix/*` from current `main` HEAD.  
3. Surgical fix + regression test.  
4. Open PR to `main` and merge with the commit-count rule above (≤30 merge commit / >30 squash); update local `status.json`.  

### Pre-existing failure claims (PM override)

Before accepting “pre-existing” to waive a test/QC finding: reproduce against **current** `origin/<target_branch>` HEAD.

## Anti-patterns

- Committing `status.json`, `workflows/`, `projects/`, `plans/`, `iterations/`, `archived/`, or `sdd/`
- Using `{KNOWLEDGE_DIR}` as a dumping ground for unfinished specs
- Duplicating wire contracts under `{SPECS_DIR}` that belong in root `schemas/`
- Force-adding ignored harness paths “for convenience”
- Mixing process paths into commits whose intent is `knowledge/` / `specs/` sharing