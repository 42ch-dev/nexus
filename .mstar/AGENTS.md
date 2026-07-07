# Nexus OSS — Harness Directory (`{HARNESS_DIR}`)

> For project-level rules, tech stack, and domain-specific conventions, see the root [`AGENTS.md`](../AGENTS.md).

## Source priority

1. Current user instruction
2. Root [`AGENTS.md`](../AGENTS.md)
3. This file
4. Upstream `mstar-*` skills (runtime SSOT for harness behavior)

## Concepts

| Symbol | Meaning | Path (this repo) |
|--------|---------|------------------|
| `{HARNESS_DIR}` | Root of engineering harness (this tree) | `.mstar/` |
| `{PLAN_DIR}` | Plan documents and QC/QA reports | `plans/` |
| `{SDD_DIR}` | SDD runtime scratch (gitignored) | `sdd/<plan-id>/` |
| `{ITERATION_DIR}` | Iteration-level compass specs | `iterations/` |
| `{KNOWLEDGE_DIR}` | Knowledge root (cross-cutting policy + trackers) | `knowledge/` |
| `{SPECS_DIR}` | Frozen functional/normative OSS specs | `knowledge/specs/` (**deviation** — see below) |

## Upstream Harness

This repo follows the **[Morning Star (mstar-harness)](https://github.com/btspoony/mstar-harness)** framework. Default harness behavior lives in upstream `mstar-*` skills; **this file records project-specific deviations only.**

**Load order:** Read `mstar-harness-core` first; load other `mstar-*` topic skills on demand per its role matrix. State machine, phase gates, dispatch, SDD, QC/QA, and iteration Phase 1–5 → upstream skills (not duplicated here).

### Editing this file

Rules and invariants only — not a changelog or audit trail.

- Use generic placeholders (`<plan-id>`, `{ver}`) in examples.
- State the rule, not the incident story (`notes.json` / git history hold narrative).
- Anti-patterns describe the mistake generically.

## Layout & write boundaries

**Plan layout:** each plan is a **single `.md` file** under `plans/` — never `plans/<plan-id>/` as a directory. QC/QA reports: `plans/reports/<plan-id>/` only (not nested under a plan directory). Details → `mstar-plan-artifacts/references/plan-files-and-reports.md`.

**Who writes where:**

| Path | Typical writers | Content |
|------|-----------------|--------|
| **`{SDD_DIR}`** | Implementers (SDD default), PM / `mstar-sdd` | `task-N-brief.md`, `task-N-report.md`, `progress.md`, review diffs |
| **`plans/<plan-id>-<name>.md`** | Implementers (checkboxes), PM, architect, product-manager | Main plan — **not** SDD bodies |
| **`plans/reports/<plan-id>/`** | `qc-specialist*`, PM, QA | `qc1`…`qc3`, consolidated — **plan-level L3 only** |

Default **`Execution mode: sdd`:** implementors write to **`{SDD_DIR}`**, not `plans/reports/`. Plan QC after all L2 task reviews pass. Inline/hotfix: no `{SDD_DIR}`; single `qc.md` in reports when applicable.

**Reachability:** git-tracked harness docs must survive a fresh `git clone` (no sole authority in gitignored or machine-local paths).

**Content boundaries (this repo):**

| Area | Notes |
|------|-------|
| `docs/` (repo root) | Human contributor docs only — not plan I/O or harness artifacts |
| `{ITERATION_DIR}` | Delivery compasses — index: [`iterations/README.md`](iterations/README.md) |
| `{SPECS_DIR}` | **`knowledge/specs/`** — not repo-root `specs/` (upstream default). Wire JSON Schema: repo-root `schemas/` |
| `{KNOWLEDGE_DIR}` | Cross-cutting policy — layout: [`knowledge/AGENTS.md`](knowledge/AGENTS.md) |

## Pre-merge Checklist (this repository)

1. Update `status.json` (plans, residuals, gates, timeline)
2. Run `pnpm run codegen` and commit regenerated output if `schemas/` changed
3. Update `roadmap.md` in `nexus-platform` if a plan is marked `Done`
4. Archive Done plans per Profile B → `mstar-plan-artifacts/references/done-compaction.md`
5. **Size gate:** `wc -c .mstar/status.json` < 20_000; archive resolved residuals. Git discipline: root [`AGENTS.md`](../AGENTS.md) § Git & repository hygiene.

## Project-Specific Deviations

### `status.json` metadata — no narrative prose

`status.json` is structured SSOT only. Narrative → `notes.json`, commits, or compass/plan docs.

**Forbidden in `metadata`:** `*_plan_registration_note`, `*_carry_forward_index`, `tech_debt_summary.*_ship_note`, or any new `*_note` / `*_narrative` paragraph field. If derivable from `plans[]`, `residual_findings`, or `archived/plans/<id>.json`, the field is redundant.

### Iteration branches — field names & naming

Upstream workflow (`mstar-iteration`, `mstar-branch-worktree`, `mstar-plan-conventions`) applies; this repo uses:

| Tier | `status.json` field |
|------|---------------------|
| Iteration base | `metadata.iteration_base_branch` |
| Spec integration | `metadata.spec_integration_branch` / `plans[].metadata.spec_integration_branch` |
| PR target | `metadata.target_branch` |
| Per-plan topic | `plans[].working_branch` |
| Per-plan merge | `plans[].merge_target` (= `spec_integration_branch`) |

**Legacy (read-only):** `integration_branch` → `spec_integration_branch`; `integration_merge_target` → `target_branch`. New writes use upstream names.

**Naming:** integration `iteration/{ver}`; topic `feature/{ver}-{plan-slug}`; hotfix `fix/{short-name}`. Single-plan iterations may collapse topic and integration to one branch.

### Plan compaction — Profile B

Adopted per `mstar-plan-artifacts/references/done-compaction.md` Template B.

| File | Content |
|------|---------|
| `status.json` → `plans[]` | **non-`Done` only** |
| `archived/plans-done.json` → `plans` | **string `plan_id` index only** |
| `archived/plans/<plan-id>.json` | full Done plan snapshot |

**Anti-patterns:** full plan objects in `plans-done.json`; missing per-file archive; mixing strings and dicts in `plans[]`.

### Residual detail prose

Optional: `plans/residuals/<plan-id>/<finding-id>-<label>.md` supplements root `residual_findings` (schema: `mstar-plan-artifacts/references/status-and-residuals.md`). Archive to `archived/residuals/<plan-id>.json` when closed.

### Post-merge hotfix (this repo)

After a merge to `main` exposes CI regression:

1. Register `residual_findings` **before** opening the fix branch.
2. Branch `fix/<short-name>` from current `main` HEAD (not a retired integration branch).
3. Surgical fix + regression test per bug class.
4. Verify: `cargo test -p <crate> --test <file>` + `cargo clippy --all -- -D warnings` + `cargo +nightly-2026-06-26 fmt --all --check`.
5. PR → merge with **`--merge`** (not squash) for audit provenance.
6. Update `status.json` (hotfix plan row + resolve residual).

Prepare-phase hotfix compression → `mstar-phase-gates`.

### "Pre-existing" claim verification

PM override citing pre-existing failure must verify against **current `metadata.target_branch` HEAD**:

| Step | Action |
|------|--------|
| 1 | Identify failing test(s) and mode |
| 2 | Run against `origin/<target_branch>` |
| 3 | Passes on target → claim **FALSE** (attributable to iteration under review) |
| 4 | Fails on target → claim **TRUE**; document base SHA + repro command |
| 5 | Flaky → fixed seed or documented flake rate |
